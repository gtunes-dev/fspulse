// Scan States
// 1. Initial scan.
//      - New items: create Item with metadata,last_scan; create change (Add)
//      - For each found item:
//          - If tombstone: Update item type, metadata, is_ts, last_scan; null hash, valid; create change (Add)
//          - If folder <-> file change: update Item metadata, last_scan; null hash, valid; create change (Type Changed)
//          - If metadata change: update Item metadata, last_scan; create change (Modify)
//  (Set State to 2)
// 2. Tombstone
//      - For each previously seen, non-tombstone item:
//          - Set is_ts; create change (Delete)
//  (If --hash or --validate, set state to 3 else set state to [TBD])
// 3. Hash and/or Validate
//      - For each non-tombstone, file item with last_scan < current scan:
//          - Hash and/or Validate per scan configuration
//          - If Hash and/or Valid are non-null and have changed, create change record with old value(s) of the changed value(s)
// 4. Completed
// 5. Stopped

use crate::alerts::Alerts;
use crate::changes::ChangeType;
use crate::hash::Hash;
use crate::items::{Access, AnalysisItem, Item, ItemType};
use crate::roots::Root;
use crate::scans::ScanState;
use crate::schedules::{AnalysisTracker, ScanTaskState};
use crate::task::TaskProgress;
use crate::validate::validator::{from_path, ValidationState};
use crate::{database::Database, error::FsPulseError, scans::Scan};

use crossbeam_channel::bounded;
use log::{error, info, trace, warn, Level};
use logging_timer::timer;
use rusqlite::Connection;
use threadpool::ThreadPool;

use std::fs::Metadata;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::{cmp, fs};

pub struct Scanner {}

/// Batch size for database write operations during scanning
const SCAN_BATCH_SIZE: usize = 100;

/// Context passed through recursive directory scanning to avoid large parameter lists
struct ScanContext<'a> {
    conn: &'a Connection,
    scan: &'a Scan,
    task_progress: &'a Arc<TaskProgress>,
    interrupt_token: &'a Arc<AtomicBool>,
    batch_count: usize,
    files_scanned: u64,
    directories_scanned: u64,
}

impl<'a> ScanContext<'a> {
    fn new(
        conn: &'a Connection,
        scan: &'a Scan,
        task_progress: &'a Arc<TaskProgress>,
        interrupt_token: &'a Arc<AtomicBool>,
    ) -> Self {
        Self {
            conn,
            scan,
            task_progress,
            interrupt_token,
            batch_count: 0,
            files_scanned: 0,
            directories_scanned: 0,
        }
    }

    fn increment_files_scanned(&mut self) {
        self.files_scanned += 1;
        self.update_scanning_progress();
    }

    fn increment_directories_scanned(&mut self) {
        self.directories_scanned += 1;
        self.update_scanning_progress();
    }

    fn update_scanning_progress(&self) {
        self.task_progress.set_indeterminate_progress(&format!(
            "{} files in {} directories",
            self.files_scanned, self.directories_scanned
        ));
    }

    fn execute_batch_write<F, T>(&mut self, f: F) -> Result<T, FsPulseError>
    where
        F: FnOnce(&Connection) -> Result<T, FsPulseError>,
    {
        // Start transaction on first write
        if self.batch_count == 0 {
            self.conn
                .execute("BEGIN IMMEDIATE", [])
                .map_err(FsPulseError::DatabaseError)?;
        }

        let result = f(self.conn)?;
        self.batch_count += 1;

        // Auto-flush at batch size
        if self.batch_count >= SCAN_BATCH_SIZE {
            self.flush()?;
        }

        Ok(result)
    }

    fn flush(&mut self) -> Result<(), FsPulseError> {
        let _tmr = timer!(Level::Trace; "ScanContext.flush", "{}", self.batch_count);
        if self.batch_count > 0 {
            self.conn
                .execute("COMMIT", [])
                .map_err(FsPulseError::DatabaseError)?;
            self.batch_count = 0;
        }
        Ok(())
    }
}

impl<'a> Drop for ScanContext<'a> {
    fn drop(&mut self) {
        // If we still have unflushed writes, we're in an error scenario
        // (normal path explicitly calls flush()). Rollback to maintain data integrity.
        if self.batch_count > 0 {
            error!(
                "ScanContext dropped with {} unflushed writes - rolling back transaction",
                self.batch_count
            );
            let _ = self.conn.execute("ROLLBACK", []);
        }
    }
}

impl Scanner {
    pub fn do_scan_machine(
        scan: &mut Scan,
        root: &Root,
        task_id: i64,
        mut initial_task_state: Option<String>,
        task_progress: Arc<TaskProgress>,
        interrupt_token: Arc<AtomicBool>,
    ) -> Result<(), FsPulseError> {
        // NOTE: We check for interrupt at appropriate points in the scanner code with:
        //
        //   if interrupt_token.load(Ordering::Acquire) {
        //       return Err(FsPulseError::TaskInterrupted);
        //   }
        //
        // Interrupt may be signaled because the user explicitly stopped this scan,
        // paused all scanning, or because the process is shutting down. This module
        // does not differentiate between the reasons for interrupting a scan -
        // it exits cleanly with the scan, and database, in a resumable state
        // independent of the reason the interrupt was triggered

        // Loop through all states, even if resuming, to allow progress updates
        let mut loop_state = ScanState::Scanning;

        loop {
            // When the state is completed, the scan is done. We check this before checking
            // for interrupt because a complete scan should not be treated as a successfully
            // interrupted scan
            if loop_state == ScanState::Completed {
                break;
            }

            // Check for interrupt at the top of the loop
            Scanner::check_interrupted(&interrupt_token)?;

            match loop_state {
                ScanState::Scanning => {
                    task_progress.set_phase("Phase 1 of 3: Scanning");
                    if scan.state() == ScanState::Scanning {
                        Scanner::do_state_scanning(root, scan, task_progress.clone(), &interrupt_token)?;
                    }
                    loop_state = ScanState::Sweeping;
                }
                ScanState::Sweeping => {
                    task_progress.set_phase("Phase 2 of 3: Sweeping");
                    if scan.state() == ScanState::Sweeping {
                        Scanner::do_state_sweeping(scan, task_progress.clone(), &interrupt_token)?;
                    }
                    loop_state = ScanState::Analyzing;
                }
                ScanState::Analyzing => {
                    task_progress.set_phase("Phase 3 of 3: Analyzing");
                    let analysis_result = if scan.state() == ScanState::Analyzing {
                        Scanner::do_state_analyzing(scan, task_id, initial_task_state.take(), task_progress.clone(), &interrupt_token)
                    } else {
                        Ok(())
                    };

                    analysis_result?;
                    loop_state = ScanState::Completed;
                }
                unexpected => {
                    return Err(FsPulseError::Error(format!(
                        "Unexpected incomplete scan state: {unexpected}"
                    )));
                }
            };
        }

        Ok(())
    }

    fn scan_directory_recursive(ctx: &mut ScanContext, path: &Path) -> Result<i64, FsPulseError> {
        let _tmr = timer!(Level::Trace; "scan_directory_recursive", "{}", path.display());

        Scanner::check_interrupted(ctx.interrupt_token)?;

        ctx.increment_directories_scanned();

        // Try to read directory contents, handling access errors
        let items = match fs::read_dir(path) {
            Ok(items) => items,
            Err(e) => {
                match e.kind() {
                    ErrorKind::PermissionDenied => {
                        // Can't list directory contents - signal to caller
                        return Err(FsPulseError::DirectoryUnreadable(path.to_path_buf()));
                    }
                    ErrorKind::NotFound => {
                        // Directory disappeared during scan
                        trace!("Directory disappeared during scan: '{}'", path.display());
                        return Ok(0);
                    }
                    _ => {
                        error!(
                            "Unexpected error reading directory '{}': {} (kind: {:?})",
                            path.display(),
                            e,
                            e.kind()
                        );
                        return Err(FsPulseError::from(e));
                    }
                }
            }
        };

        let mut total_size: i64 = 0;

        for item in items {
            // Handle errors during directory iteration
            let item = match item {
                Ok(entry) => entry,
                Err(e) => {
                    match e.kind() {
                        ErrorKind::PermissionDenied => {
                            error!(
                                "Permission denied reading directory entry in '{}': {}",
                                path.display(),
                                e
                            );
                            continue;
                        }
                        _ => {
                            // Other I/O errors during iteration - log and continue
                            error!(
                                "Error reading directory entry in '{}': {}",
                                path.display(),
                                e
                            );
                            continue;
                        }
                    }
                }
            };
            let item_path = item.path();

            // Try to get metadata, handling access errors gracefully
            let item_metadata = match fs::symlink_metadata(&item_path) {
                Ok(metadata) => Some(metadata),
                Err(e) => {
                    match e.kind() {
                        ErrorKind::NotFound => {
                            // File disappeared during scan (race condition) - skip it
                            trace!("File disappeared during scan: '{}'", item_path.display());
                            continue;
                        }
                        ErrorKind::PermissionDenied => {
                            // Can't access metadata - treat as file with MetaError access state
                            error!(
                                "Cannot access metadata for '{}': {}. Treating as file with MetaError.",
                                item_path.display(),
                                e
                            );
                            None
                        }
                        _ => {
                            // Other errors - log and propagate
                            error!(
                                "Unexpected error getting metadata for '{}': {} (kind: {:?})",
                                item_path.display(),
                                e,
                                e.kind()
                            );
                            return Err(FsPulseError::from(e));
                        }
                    }
                }
            };

            ctx.increment_files_scanned();

            match item_metadata {
                Some(ref metadata) if metadata.is_dir() => {
                    // Recursively scan the subdirectory and get its size
                    // If we can't read the directory contents, we still handle it as an item
                    let (subdir_size, dir_read_error) =
                        match Scanner::scan_directory_recursive(ctx, &item_path) {
                            Ok(size) => (size, false),
                            Err(FsPulseError::DirectoryUnreadable(ref p)) => {
                                error!(
                                    "Cannot read directory contents for '{}': Permission denied",
                                    p.display()
                                );
                                (0, true)
                            }
                            Err(e) => return Err(e),
                        };

                    // Handle the subdirectory with its computed size
                    let returned_size = Scanner::handle_scan_item(
                        ctx,
                        ItemType::Directory,
                        &item_path,
                        Some(metadata),
                        Some(subdir_size),
                        dir_read_error,
                    )?;

                    total_size += returned_size;
                }
                Some(ref metadata) => {
                    Scanner::check_interrupted(ctx.interrupt_token)?;

                    // Handle files, symlinks, and other items
                    let item_type = if metadata.is_file() {
                        ItemType::File
                    } else if metadata.is_symlink() {
                        ItemType::Symlink
                    } else {
                        ItemType::Unknown
                    };

                    // Files have meaningful sizes, symlinks and other don't
                    let item_size = if metadata.is_file() {
                        Some(metadata.len() as i64)
                    } else {
                        None
                    };

                    let returned_size = Scanner::handle_scan_item(
                        ctx,
                        item_type,
                        &item_path,
                        Some(metadata),
                        item_size,
                        false, // No directory read error for non-directories
                    )?;

                    total_size += returned_size;
                }
                None => {
                    // Metadata unavailable (permission denied) - treat as Unknown with MetaError
                    Scanner::check_interrupted(ctx.interrupt_token)?;

                    Scanner::handle_scan_item(
                        ctx,
                        ItemType::Unknown,
                        &item_path,
                        None, // No metadata available
                        None, // No size available
                        false,
                    )?;
                    // Don't add to total_size since we don't know the size
                }
            }
        }

        Ok(total_size)
    }

    fn do_state_scanning(
        root: &Root,
        scan: &mut Scan,
        task_progress: Arc<TaskProgress>,
        interrupt_token: &Arc<AtomicBool>,
    ) -> Result<(), FsPulseError> {
        let conn = Database::get_connection()?;
        let root_path_buf = PathBuf::from(root.root_path());

        // Create scanning context
        let mut ctx = ScanContext::new(&conn, scan, &task_progress, interrupt_token);

        // Recursively scan the root directory and get the total size
        // Note: We don't store the root directory itself as an item in the database
        let total_size = match Scanner::scan_directory_recursive(&mut ctx, &root_path_buf) {
            Ok(size) => size,
            Err(FsPulseError::DirectoryUnreadable(ref p)) => {
                // Root directory is unreadable - scan cannot proceed
                error!(
                    "Cannot read root directory '{}': Permission denied. Scan cannot proceed.",
                    p.display()
                );
                return Err(FsPulseError::Error(format!(
                    "Root directory '{}' is unreadable",
                    p.display()
                )));
            }
            Err(e) => return Err(e),
        };

        // Flush any remaining batched writes
        ctx.flush()?;

        // Add breadcrumb for completed scanning phase
        ctx.task_progress.add_breadcrumb(&format!(
            "Scanned {} files in {} directories",
            ctx.files_scanned, ctx.directories_scanned
        ));

        // Drop ctx to release the immutable borrow of scan before we mutably borrow it
        drop(ctx);

        // The total_size column is set on the scan row before advancing to the next phase
        // This means it doesn't have to be computed or set later in the scan, but it does need
        // to be set to NULL if the scan ends in stoppage or error
        scan.set_total_size(&conn, total_size)?;

        scan.set_state_sweeping(&conn)
    }

    fn do_state_sweeping(
        scan: &mut Scan,
        task_progress: Arc<TaskProgress>,
        interrupt_token: &Arc<AtomicBool>,
    ) -> Result<(), FsPulseError> {
        Scanner::check_interrupted(interrupt_token)?;

        let conn = Database::get_connection()?;

        Database::immediate_transaction(&conn, |c| {
            // Insert deletion records into changes
            c.execute(
                "INSERT INTO changes (scan_id, item_id, change_type)
                    SELECT ?, item_id, ?
                    FROM items
                    WHERE root_id = ? AND is_ts = 0 AND last_scan < ?",
                (
                    scan.scan_id(),
                    ChangeType::Delete.as_i64(),
                    scan.root_id(),
                    scan.scan_id(),
                ),
            )?;

            // Mark unseen items as tombstones
            c.execute(
                "UPDATE items SET
                    is_ts = 1,
                    last_scan = ?
                WHERE root_id = ? AND last_scan < ? AND is_ts = 0",
                (scan.scan_id(), scan.root_id(), scan.scan_id()),
            )?;

            Ok(())
        })?;

        task_progress.add_breadcrumb("Tombstoned deleted items");

        scan.set_state_analyzing(&conn)
    }

    fn do_state_analyzing(
        scan: &mut Scan,
        task_id: i64,
        initial_task_state: Option<String>,
        task_progress: Arc<TaskProgress>,
        interrupt_token: &Arc<AtomicBool>,
    ) -> Result<(), FsPulseError> {
        // Get a single connection for this entire phase
        let conn = Database::get_connection()?;

        let is_hash = scan.analysis_spec().is_hash();
        let is_val = scan.analysis_spec().is_val();

        // If the scan doesn't hash or validate, then the scan
        // can be marked complete and we just return
        if !is_hash && !is_val {
            Scanner::check_interrupted(interrupt_token)?;
            scan.set_state_completed(&conn)?;
            return Ok(());
        }

        // Parse initial task state for restart resilience (HWM loaded from TaskRow)
        let initial_state = ScanTaskState::from_task_state(initial_task_state.as_deref())?;
        let initial_hwm = initial_state.high_water_mark;

        let (analyze_total, analyze_done) =
            Item::get_analysis_counts(&conn, scan.scan_id(), scan.analysis_spec(), initial_hwm)?;

        // Set up counter-based progress tracking
        task_progress.set_progress_total(analyze_total, analyze_done, Some("files"));

        // Create the analysis tracker for HWM management (shared with worker threads)
        let tracker = Arc::new(AnalysisTracker::new(task_id, initial_state));

        // Create a bounded channel to limit the number of queued tasks (e.g., max 100 tasks)
        let (sender, receiver) = bounded::<AnalysisItem>(100);

        // Initialize the thread pool
        let items_remaining = analyze_total.saturating_sub(analyze_done); // avoids underflow
        let items_remaining_usize = items_remaining.try_into().unwrap_or(usize::MAX);

        let thread_count = crate::config::Config::get_analysis_threads();

        let num_threads = cmp::min(items_remaining_usize, thread_count);
        let pool = ThreadPool::new(num_threads.max(1)); // ensure at least one thread

        // Set up thread states
        task_progress.set_thread_count(num_threads);

        for thread_index in 0..num_threads {
            // Clone shared resources for each worker thread.
            let receiver = receiver.clone();
            let scan_copy = scan.clone();
            let task_progress_clone = Arc::clone(&task_progress);
            let interrupt_token_clone = Arc::clone(interrupt_token);
            let tracker_clone = Arc::clone(&tracker);

            // Worker thread: continuously receive and process tasks.
            // Each thread gets its own connection from the pool!
            pool.execute(move || {
                while let Ok(analysis_item) = receiver.recv() {
                    Scanner::process_item_async(
                        &scan_copy,
                        analysis_item,
                        thread_index,
                        &task_progress_clone,
                        &interrupt_token_clone,
                        &tracker_clone,
                    );
                }
                task_progress_clone.set_thread_idle(thread_index);
            });
        }

        let mut last_item_id = initial_hwm;

        loop {
            if interrupt_token.load(Ordering::Acquire) {
                break;
            }

            let analysis_items = Item::fetch_next_analysis_batch(
                &conn,
                scan.scan_id(),
                scan.analysis_spec(),
                last_item_id,
                100,
            )?;

            if analysis_items.is_empty() {
                break;
            }

            // Add batch item IDs to tracker before distributing work
            tracker.add_batch(analysis_items.iter().map(|item| item.item_id()));

            for analysis_item in analysis_items {
                // Items will be ordered by id. We keep track of the last seen id and provide
                // it in calls to fetch_next_analysis_batch to avoid picking up unprocessed
                // items that we've already picked up. This avoids a race condition
                // in which we'd pick up unprocessed items that are currently being processed
                last_item_id = analysis_item.item_id();

                // This send will block if the channel already has 100 items.
                sender
                    .send(analysis_item)
                    .expect("Failed to send task into the bounded channel");
            }

            // HWM is now updated by workers via tracker.complete_item()
        }

        // Drop the sender to signal the workers that no more items will come.
        drop(sender);

        // Wait for all tasks to complete.
        pool.join();

        // It is critical that we check for completion and return the interrupt error
        // without marking the scan completed. Once the scan is marked completed, attempting to
        // "stop" the scan will be a no-op and the scan will remain in a completed state.
        // Because we may have detected the interrupt and correctly interrupted or never started
        // some hashing or validation operations, we have to be careful to not allow it to
        // appear complete
        Scanner::check_interrupted(interrupt_token)?;

        // If we got here without interruption, all items should have been processed.
        // Warn if the tracker still has items (indicates a bug in the tracking logic).
        tracker.warn_if_not_empty();

        // Clear thread states and add breadcrumb
        task_progress.clear_thread_states();
        task_progress.add_breadcrumb(&format!("Analyzed {} files", analyze_total));

        scan.set_state_completed(&conn)
    }

    fn process_item_async(
        scan: &Scan,
        mut analysis_item: AnalysisItem,
        thread_index: usize,
        task_progress: &Arc<TaskProgress>,
        interrupt_token: &Arc<AtomicBool>,
        tracker: &Arc<AnalysisTracker>,
    ) {
        // This function is the entry point for each worker thread to process an item.
        // It performs hashing and/or validation as needed and updates the database.
        // It does not return errors, but it does need to check for interruption.
        // If an interrupt is detected, it should exit promptly without updating
        // the database. The hashing and validation processes exit when detecting
        // interrupt and may return an interrupt error, but we ignore that here.
        // The calling code will always check for interrupt and do the right thing
        // depending on why the interrupt was generated

        let item_id = analysis_item.item_id();
        let path = PathBuf::from(analysis_item.item_path());

        info!("Beginning analysis of: {path:?}");

        let display_path = path
            .file_name()
            .unwrap_or(path.as_os_str())
            .to_string_lossy();

        let mut new_hash = None;
        let mut read_attempted = false;
        let mut read_succeeded = false;
        let mut read_permission_denied = false;
        let mut file_not_found = false;

        if analysis_item.needs_hash() && !Scanner::is_interrupted(interrupt_token) {
            task_progress.set_thread_state(thread_index, "Hashing", "info", Some(&display_path));
            read_attempted = true;

            // The hash function checks for interrupt at its start and periodically
            match Hash::compute_sha2_256_hash(&path, interrupt_token) {
                Ok(hash_s) => {
                    new_hash = Some(hash_s);
                    read_succeeded = true;
                }
                Err(FsPulseError::IoError(ref io_err))
                    if io_err.kind() == ErrorKind::PermissionDenied =>
                {
                    error!(
                        "Cannot read file for hashing '{}': Permission denied",
                        &display_path
                    );
                    read_permission_denied = true;
                }
                Err(FsPulseError::IoError(ref io_err))
                    if io_err.kind() == ErrorKind::NotFound =>
                {
                    // File was deleted between the scan/sweep phase and analysis.
                    // This is a normal race condition, not an error the user needs
                    // to act on. We skip hash and validation analysis for this item
                    // so that update_item_analysis won't examine or compare hash/val
                    // state, and set access to MetaError to reflect that the file
                    // could not be found.
                    warn!(
                        "File not found during hashing '{}': skipping analysis",
                        &display_path
                    );
                    file_not_found = true;
                }
                Err(error) => {
                    error!("Error hashing '{}': {}", &display_path, error);
                    // Other errors (not permission denied) - don't affect access state
                }
            };
        }

        let mut new_val = ValidationState::Unknown;
        let mut new_val_error = None;

        // Skip validation if we already know we can't read the file
        if analysis_item.needs_val()
            && !read_permission_denied
            && !file_not_found
            && !Scanner::is_interrupted(interrupt_token)
        {
            let validator = from_path(&path);
            match validator {
                Some(v) => {
                    task_progress.set_thread_state(thread_index, "Validating", "info-alternate", Some(&display_path));
                    read_attempted = true;

                    match v.validate(&path, interrupt_token) {
                        Ok((res_validity_state, res_validation_error)) => {
                            new_val = res_validity_state;
                            new_val_error = res_validation_error;
                            read_succeeded = true;
                        }
                        Err(FsPulseError::IoError(ref io_err))
                            if io_err.kind() == ErrorKind::PermissionDenied =>
                        {
                            error!(
                                "Cannot read file for validation '{}': Permission denied",
                                &display_path
                            );
                            read_permission_denied = true;
                        }
                        Err(FsPulseError::IoError(ref io_err))
                            if io_err.kind() == ErrorKind::NotFound =>
                        {
                            // File deleted between hashing and validation (or only
                            // validation was requested). Same handling as hashing.
                            warn!(
                                "File not found during validation '{}': skipping analysis",
                                &display_path
                            );
                            file_not_found = true;
                        }
                        Err(e) => {
                            let e_str = e.to_string();
                            error!("Error validating '{}': {}", &display_path, e_str);
                            new_val = ValidationState::Invalid;
                            new_val_error = Some(e_str);
                        }
                    }
                }
                None => new_val = ValidationState::NoValidator,
            }
        }

        // If the file was not found during hashing or validation, disable both
        // hash and validation analysis. The item was deleted between scan/sweep
        // and analysis — a normal race condition. By clearing needs_hash and
        // needs_val, update_item_analysis will skip all hash/val comparisons and
        // only update the access state to MetaError. The item's last_hash_scan
        // and last_val_scan markers are intentionally NOT advanced, so the item
        // will be picked up for analysis again on the next scan when (if) the
        // file reappears.
        if file_not_found {
            analysis_item.set_needs_hash(false);
            analysis_item.set_needs_val(false);
        }

        // Determine new access state based on read results
        let new_access = if file_not_found {
            Some(Access::MetaError)
        } else if read_permission_denied {
            Some(Access::ReadError)
        } else if read_attempted && read_succeeded {
            Some(Access::Ok)
        } else {
            None // No change - preserve current access state
        };

        if !Scanner::is_interrupted(interrupt_token) {
            if let Err(error) = Scanner::update_item_analysis(
                scan,
                &analysis_item,
                new_hash,
                new_val,
                new_val_error,
                new_access,
                interrupt_token,
            ) {
                let e_str = error.to_string();
                error!(
                    "Error updating item analysis '{}': {}",
                    &display_path, e_str
                );
            }
        }

        task_progress.increment_progress();

        // Set thread back to idle after completing work
        task_progress.set_thread_idle(thread_index);

        // Mark item complete in tracker (updates HWM if appropriate)
        if let Err(e) = tracker.complete_item(item_id) {
            error!("Error updating analysis HWM for item {}: {}", item_id, e);
        }

        info!("Done analyzing: {path:?}");
    }

    /// Calculate the new access state based on item type, current access, and scan results.
    ///
    /// Priority (highest to lowest):
    /// 1. meta_error=true → MetaError (can't stat at all)
    /// 2. dir_read_error=true (directories only) → ReadError (can stat but can't list)
    /// 3. Otherwise, clear MetaError (stat worked), preserve ReadError for non-directories
    ///
    /// For directories:
    /// - If meta_error: MetaError
    /// - If read_dir failed: ReadError (can stat but can't list contents)
    /// - If read_dir succeeded: Ok (stat and read both work, clear any previous error)
    ///
    /// For files/symlinks/other:
    /// - If meta_error: MetaError
    /// - MetaError → Ok (stat works now)
    /// - ReadError → ReadError (preserved until analysis phase clears it)
    /// - Ok → Ok
    fn calculate_new_access(
        item_type: ItemType,
        old_access: Access,
        dir_read_error: bool,
        meta_error: bool,
    ) -> Access {
        // MetaError takes priority - if we can't stat, that's the access state
        if meta_error {
            return Access::MetaError;
        }

        if item_type == ItemType::Directory {
            if dir_read_error {
                Access::ReadError
            } else {
                // Directory stat and read_dir both succeeded - clear any errors
                Access::Ok
            }
        } else {
            // For non-directories, clear MetaError (stat worked), preserve ReadError
            match old_access {
                Access::MetaError => Access::Ok,
                Access::ReadError => Access::ReadError,
                Access::Ok => Access::Ok,
            }
        }
    }

    /// Helper to check update result and return appropriate error
    fn check_update_result(rows_updated: usize, item_id: i64) -> Result<(), FsPulseError> {
        if rows_updated == 0 {
            Err(FsPulseError::Error(format!(
                "Item Id {} not found for update",
                item_id
            )))
        } else {
            Ok(())
        }
    }

    /// Helper to compute access change values for change records
    fn access_change_values(old_access: Access, new_access: Access) -> (Option<i64>, Option<i64>) {
        if old_access != new_access {
            (Some(old_access.as_i64()), Some(new_access.as_i64()))
        } else {
            (None, None)
        }
    }

    /// Returns true if an AccessDenied alert should be created.
    /// Alert is created when access transitions from Ok to an error state.
    fn should_alert_access_denied(old_access: Access, new_access: Access) -> bool {
        old_access == Access::Ok && new_access != Access::Ok
    }

    fn handle_scan_item(
        ctx: &mut ScanContext,
        item_type: ItemType,
        path: &Path,
        metadata: Option<&Metadata>,
        computed_size: Option<i64>,
        dir_read_error: bool,
    ) -> Result<i64, FsPulseError> {
        let _tmr = timer!(Level::Trace; "handle_scan_item", "{}", path.display());

        let path_str = path.to_string_lossy();
        let existing_item =
            Item::get_by_root_path_type(ctx.conn, ctx.scan.root_id(), &path_str, item_type)?;

        // If metadata is None, we have a MetaError (couldn't stat)
        let meta_error = metadata.is_none();

        let mod_date = metadata.and_then(|m| {
            m.modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs() as i64)
        });

        let size = computed_size;

        match existing_item {
            Some(ref item) if item.last_scan() == ctx.scan.scan_id() => {
                // Already processed this scan - return cached size for folder aggregation
                Ok(item.size().unwrap_or(0))
            }
            Some(existing_item) => {
                Scanner::handle_existing_item(
                    ctx,
                    &existing_item,
                    item_type,
                    mod_date,
                    size,
                    dir_read_error,
                    meta_error,
                )?;
                Ok(computed_size.unwrap_or(0))
            }
            None => {
                Scanner::handle_new_item(
                    ctx,
                    item_type,
                    &path_str,
                    mod_date,
                    size,
                    dir_read_error,
                    meta_error,
                )?;
                Ok(computed_size.unwrap_or(0))
            }
        }
    }

    /// Handle an existing item (non-tombstone or tombstone)
    fn handle_existing_item(
        ctx: &mut ScanContext,
        existing_item: &Item,
        item_type: ItemType,
        mod_date: Option<i64>,
        size: Option<i64>,
        dir_read_error: bool,
        meta_error: bool,
    ) -> Result<(), FsPulseError> {
        let old_access = existing_item.access();
        let new_access =
            Scanner::calculate_new_access(item_type, old_access, dir_read_error, meta_error);
        let access_changed = old_access != new_access;
        let meta_change = existing_item.mod_date() != mod_date || existing_item.size() != size;

        if existing_item.is_ts() {
            Scanner::handle_tombstone_rehydration(
                ctx,
                existing_item,
                mod_date,
                size,
                old_access,
                new_access,
            )
        } else if meta_change || access_changed {
            Scanner::handle_item_modification(
                ctx,
                existing_item,
                mod_date,
                size,
                old_access,
                new_access,
                meta_change,
            )
        } else {
            // No change at all - just update last_scan
            Scanner::handle_item_no_change(ctx, existing_item)
        }
    }

    /// Handle tombstone rehydration (item coming back from deletion)
    fn handle_tombstone_rehydration(
        ctx: &mut ScanContext,
        existing_item: &Item,
        mod_date: Option<i64>,
        size: Option<i64>,
        old_access: Access,
        new_access: Access,
    ) -> Result<(), FsPulseError> {
        ctx.execute_batch_write(|c| {
            let rows_updated = c.execute(
                "UPDATE items SET
                    is_ts = 0,
                    access = ?,
                    mod_date = ?,
                    size = ?,
                    file_hash = NULL,
                    val = ?,
                    val_error = NULL,
                    last_scan = ?,
                    last_hash_scan = NULL,
                    last_val_scan = NULL
                WHERE item_id = ?",
                (
                    new_access.as_i64(),
                    mod_date,
                    size,
                    ValidationState::Unknown.as_i64(),
                    ctx.scan.scan_id(),
                    existing_item.item_id(),
                ),
            )?;
            Scanner::check_update_result(rows_updated, existing_item.item_id())?;

            let (access_old_val, access_new_val) =
                Scanner::access_change_values(old_access, new_access);

            c.execute(
                "INSERT INTO changes (
                    scan_id, item_id, change_type,
                    access_old, access_new,
                    is_undelete, mod_date_old, mod_date_new, size_old, size_new,
                    hash_old, val_old, val_error_old
                ) VALUES (?, ?, ?, ?, ?, 1, ?, ?, ?, ?, ?, ?, ?)",
                (
                    ctx.scan.scan_id(),
                    existing_item.item_id(),
                    ChangeType::Add.as_i64(),
                    access_old_val,
                    access_new_val,
                    existing_item.mod_date(),
                    mod_date,
                    existing_item.size(),
                    size,
                    existing_item.file_hash(),
                    existing_item.validity_state_as_str(),
                    existing_item.val_error(),
                ),
            )?;

            // Alert if rehydrated item is inaccessible
            // For rehydration, we alert whenever new_access is not Ok, regardless of what
            // the old access state was (since the item was a tombstone, any new access
            // error is a problem worth alerting on, similar to a brand new item)
            if new_access != Access::Ok {
                Alerts::add_access_denied_alert(c, ctx.scan.scan_id(), existing_item.item_id())?;
            }

            Ok(())
        })
    }

    /// Handle item modification (metadata and/or access change)
    fn handle_item_modification(
        ctx: &mut ScanContext,
        existing_item: &Item,
        mod_date: Option<i64>,
        size: Option<i64>,
        old_access: Access,
        new_access: Access,
        meta_change: bool,
    ) -> Result<(), FsPulseError> {
        ctx.execute_batch_write(|c| {
            // Update item with new values
            let rows_updated = c.execute(
                "UPDATE items SET
                    access = ?,
                    mod_date = ?,
                    size = ?,
                    last_scan = ?
                WHERE item_id = ?",
                (
                    new_access.as_i64(),
                    mod_date,
                    size,
                    ctx.scan.scan_id(),
                    existing_item.item_id(),
                ),
            )?;
            Scanner::check_update_result(rows_updated, existing_item.item_id())?;

            // Build change record
            let (access_old_val, access_new_val) =
                Scanner::access_change_values(old_access, new_access);

            let (mod_date_old, mod_date_new, size_old, size_new) = if meta_change {
                (
                    existing_item.mod_date(),
                    mod_date,
                    existing_item.size(),
                    size,
                )
            } else {
                (None, None, None, None)
            };

            c.execute(
                "INSERT INTO changes (
                    scan_id, item_id, change_type,
                    access_old, access_new,
                    meta_change, mod_date_old, mod_date_new, size_old, size_new
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
                (
                    ctx.scan.scan_id(),
                    existing_item.item_id(),
                    ChangeType::Modify.as_i64(),
                    access_old_val,
                    access_new_val,
                    if meta_change { Some(true) } else { None },
                    mod_date_old,
                    mod_date_new,
                    size_old,
                    size_new,
                ),
            )?;

            // Alert if item became inaccessible (Ok → error)
            if Scanner::should_alert_access_denied(old_access, new_access) {
                Alerts::add_access_denied_alert(c, ctx.scan.scan_id(), existing_item.item_id())?;
            }

            Ok(())
        })
    }

    /// Handle item with no changes - just update last_scan
    fn handle_item_no_change(
        ctx: &mut ScanContext,
        existing_item: &Item,
    ) -> Result<(), FsPulseError> {
        ctx.execute_batch_write(|c| {
            let rows_updated = c.execute(
                "UPDATE items SET last_scan = ? WHERE item_id = ?",
                (ctx.scan.scan_id(), existing_item.item_id()),
            )?;
            Scanner::check_update_result(rows_updated, existing_item.item_id())
        })
    }

    /// Handle a new item (never seen before)
    fn handle_new_item(
        ctx: &mut ScanContext,
        item_type: ItemType,
        path_str: &str,
        mod_date: Option<i64>,
        size: Option<i64>,
        dir_read_error: bool,
        meta_error: bool,
    ) -> Result<(), FsPulseError> {
        // For new items, calculate access based on error conditions
        // Priority: meta_error (can't stat) > dir_read_error (can stat, can't read_dir) > Ok
        let new_access = if meta_error {
            Access::MetaError
        } else if dir_read_error {
            Access::ReadError
        } else {
            Access::Ok
        };

        ctx.execute_batch_write(|c| {
            c.execute(
                "INSERT INTO items (root_id, item_path, item_type, access, mod_date, size, val, last_scan)
                VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
                (
                    ctx.scan.root_id(),
                    path_str,
                    item_type.as_i64(),
                    new_access.as_i64(),
                    mod_date,
                    size,
                    ValidationState::Unknown.as_i64(),
                    ctx.scan.scan_id(),
                ),
            )?;

            let item_id: i64 = c.query_row("SELECT last_insert_rowid()", [], |row| row.get(0))?;

            // For new items with access errors (MetaError or ReadError), record the access state
            let (access_old_val, access_new_val): (Option<i64>, Option<i64>) =
                if new_access != Access::Ok {
                    (None, Some(new_access.as_i64()))
                } else {
                    (None, None)
                };

            c.execute(
                "INSERT INTO changes (
                    scan_id, item_id, change_type,
                    access_old, access_new,
                    is_undelete, mod_date_new, size_new, hash_change, val_change
                ) VALUES (?, ?, ?, ?, ?, 0, ?, ?, 0, 0)",
                (
                    ctx.scan.scan_id(),
                    item_id,
                    ChangeType::Add.as_i64(),
                    access_old_val,
                    access_new_val,
                    mod_date,
                    size,
                ),
            )?;

            Ok(())
        })
    }

    pub fn update_item_analysis(
        scan: &Scan,
        analysis_item: &AnalysisItem,
        new_hash: Option<String>,
        new_val: ValidationState,
        new_val_error: Option<String>,
        new_access: Option<Access>,
        interrupt_token: &Arc<AtomicBool>,
    ) -> Result<(), FsPulseError> {
        let mut update_changes = false;

        // values to use when updating the changes table
        let mut c_hash_change = Some(false);
        let mut c_last_hash_scan_old = None;
        let mut c_hash_old = None;
        let mut c_hash_new = None;
        let mut c_val_change = Some(false);
        let mut c_last_val_scan_old = None;
        let mut c_val_old = None;
        let mut c_val_new = None;
        let mut c_val_error_old = None;
        let mut c_val_error_new = None;
        let mut c_access_old: Option<i64> = None;
        let mut c_access_new: Option<i64> = None;

        // values to use in the item update
        let mut i_hash = analysis_item.file_hash();
        let mut i_val = analysis_item.val();
        let mut i_val_error = analysis_item.val_error();
        let mut i_last_hash_scan = analysis_item.last_hash_scan();
        let mut i_last_val_scan = analysis_item.last_val_scan();
        let mut i_access = analysis_item.access();

        let mut alert_possible_hash = false;
        let mut alert_invalid_item = false;
        let mut alert_access_denied = false;

        // Check if access state changed
        if let Some(access) = new_access {
            if access != analysis_item.access() {
                update_changes = true;
                c_access_old = Some(analysis_item.access().as_i64());
                c_access_new = Some(access.as_i64());
                i_access = access;

                // Alert if item became inaccessible (Ok → error)
                alert_access_denied =
                    Scanner::should_alert_access_denied(analysis_item.access(), access);
            }
        }

        if analysis_item.needs_hash() {
            if analysis_item.file_hash() != new_hash.as_deref() {
                // if either the hash or validation state changes, we update changes
                update_changes = true;
                c_hash_change = Some(true);
                c_last_hash_scan_old = analysis_item.last_hash_scan();
                c_hash_old = analysis_item.file_hash();
                c_hash_new = new_hash.as_deref();

                i_hash = new_hash.as_deref();

                // The hash changed. Assess whether it's suspicious or not
                // It's only suspicious if metadata (file size and mod date)
                // didn't change in this update or any update since that last
                // hash scan
                alert_possible_hash = match analysis_item.meta_change() {
                    Some(true) => false,
                    Some(false) | None => true,
                }
            }

            // Update the last scan id whether or not anything changed
            i_last_hash_scan = Some(scan.scan_id());
        }

        if analysis_item.needs_val() {
            if (analysis_item.val() != new_val)
                || (analysis_item.val_error() != new_val_error.as_deref())
            {
                if new_val == ValidationState::Invalid {
                    // if we're here, then either the previous validation
                    // state wasn't Invalid or it was invalid but the
                    // error message changed. In both fo these cases,
                    // we should alert
                    alert_invalid_item = true;
                }

                // if either the hash or validation state changes, we update changes
                update_changes = true;
                c_val_change = Some(true);
                c_last_val_scan_old = analysis_item.last_val_scan();
                c_val_old = Some(analysis_item.val().as_i64());
                c_val_new = Some(new_val.as_i64());
                c_val_error_old = analysis_item.val_error();
                c_val_error_new = new_val_error.as_deref();

                // always update the item when the validation state changes
                i_val = new_val;
                i_val_error = new_val_error.as_deref();
            }

            // update the last validation scan id whether or not anything changed
            i_last_val_scan = Some(scan.scan_id());
        }

        Scanner::check_interrupted(interrupt_token)?;

        let conn = Database::get_connection()?;

        // Use IMMEDIATE transaction for read-then-write pattern
        Database::immediate_transaction(&conn, |c| {
            if alert_possible_hash {
                if let Some(last_hash_scan) = analysis_item.last_hash_scan() {
                    if !Alerts::meta_changed_between(
                        c,
                        analysis_item.item_id(),
                        last_hash_scan,
                        scan.scan_id(),
                    )? {
                        Alerts::add_suspicious_hash_alert(
                            c,
                            scan.scan_id(),
                            analysis_item.item_id(),
                            analysis_item.last_hash_scan(),
                            analysis_item.file_hash(),
                            c_hash_new.unwrap(),
                        )?;
                    }
                }
            }

            if alert_invalid_item {
                Alerts::add_invalid_item_alert(
                    c,
                    scan.scan_id(),
                    analysis_item.item_id(),
                    c_val_error_new.unwrap(),
                )?;
            }

            if alert_access_denied {
                Alerts::add_access_denied_alert(c, scan.scan_id(), analysis_item.item_id())?;
            }

            // Step 1: UPSERT into `changes` table if the change is something other than moving from the default state
            if update_changes {
                c.execute(
                    "INSERT INTO changes (
                            scan_id,
                            item_id,
                            change_type,
                            access_old,
                            access_new,
                            meta_change,
                            hash_change,
                            last_hash_scan_old,
                            hash_old,
                            hash_new,
                            val_change,
                            last_val_scan_old,
                            val_old,
                            val_new,
                            val_error_old,
                            val_error_new
                        )
                        VALUES (?, ?, 2, ?, ?, 0, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                    ON CONFLICT(scan_id, item_id)
                        DO UPDATE SET
                            access_old = COALESCE(excluded.access_old, changes.access_old),
                            access_new = COALESCE(excluded.access_new, changes.access_new),
                            hash_change = excluded.hash_change,
                            last_hash_scan_old = excluded.last_hash_scan_old,
                            hash_old = excluded.hash_old,
                            hash_new = excluded.hash_new,
                            val_change = excluded.val_change,
                            last_val_scan_old = excluded.last_val_scan_old,
                            val_old = excluded.val_old,
                            val_new = excluded.val_new,
                            val_error_old = excluded.val_error_old,
                            val_error_new = excluded.val_error_new",
                    rusqlite::params![
                        scan.scan_id(),
                        analysis_item.item_id(),
                        c_access_old,
                        c_access_new,
                        c_hash_change,
                        c_last_hash_scan_old,
                        c_hash_old,
                        c_hash_new,
                        c_val_change,
                        c_last_val_scan_old,
                        c_val_old,
                        c_val_new,
                        c_val_error_old,
                        c_val_error_new,
                    ],
                )?;
            }

            // Step 2: Update `items` table
            c.execute(
                "UPDATE items
                SET
                    access = ?,
                    file_hash = ?,
                    val = ?,
                    val_error = ?,
                    last_hash_scan = ?,
                    last_val_scan = ?
                WHERE item_id = ?",
                rusqlite::params![
                    i_access.as_i64(),
                    i_hash,
                    i_val.as_i64(),
                    i_val_error,
                    i_last_hash_scan,
                    i_last_val_scan,
                    analysis_item.item_id()
                ],
            )?;

            Ok(())
        })?;

        Ok(())
    }

    /// Check if scan has been interrupted, returning error if so
    fn check_interrupted(interrupt_token: &Arc<AtomicBool>) -> Result<(), FsPulseError> {
        if interrupt_token.load(Ordering::Acquire) {
            Err(FsPulseError::TaskInterrupted)
        } else {
            Ok(())
        }
    }

    /// Check if scan has been interrupted
    fn is_interrupted(interrupt_token: &Arc<AtomicBool>) -> bool {
        interrupt_token.load(Ordering::Acquire)
    }
}
