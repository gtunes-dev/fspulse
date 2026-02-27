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
use crate::hash::Hash;
use crate::item_identity::{Access, ExistingItem, ItemIdentity, ItemType};
use crate::item_version::{AnalysisItem, ItemVersion};
use crate::roots::Root;
use crate::scans::ScanState;
use crate::task::{AnalysisTracker, ScanTaskState, TaskProgress};
use crate::undo_log::UndoLog;
use crate::validate::validator::{from_path, ValidationState};
use crate::utils::Utils;
use crate::{db::Database, error::FsPulseError, scans::Scan};

use crossbeam_channel::bounded;
use log::{error, info, trace, warn, Level};
use logging_timer::timer;
use rusqlite::{params, Connection, OptionalExtension};
use threadpool::ThreadPool;

use std::fs::Metadata;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::path::MAIN_SEPARATOR_STR;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use std::{cmp, fs};

pub struct Scanner {}

/// Batch size for database write operations during scanning
const SCAN_BATCH_SIZE: usize = 100;

/// A pending folder count write, collected during the recursive walk and
/// applied in batched transactions afterward.
struct FolderCountWrite {
    folder_item_id: i64,
    adds: i64,
    mods: i64,
    dels: i64,
    unchanged: i64,
}

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

        // Guard: the undo log should be empty at scan start. If not, a previous scan
        // crashed without cleaning up. Warn and clear to prevent stale entries from
        // corrupting this scan's rollback.
        {
            let conn = Database::get_connection()?;
            UndoLog::warn_and_clear_if_not_empty(&conn)?;
        }

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
                    task_progress.set_phase("Phase 1 of 4: Scanning");
                    if scan.state() == ScanState::Scanning {
                        Scanner::do_state_scanning(root, scan, task_progress.clone(), &interrupt_token)?;
                    }
                    loop_state = ScanState::Sweeping;
                }
                ScanState::Sweeping => {
                    task_progress.set_phase("Phase 2 of 4: Sweeping");
                    if scan.state() == ScanState::Sweeping {
                        Scanner::do_state_sweeping(scan, task_progress.clone(), &interrupt_token)?;
                    }
                    loop_state = ScanState::AnalyzingFiles;
                }
                ScanState::AnalyzingFiles => {
                    task_progress.set_phase("Phase 3 of 4: Analyzing Files");
                    if scan.state() == ScanState::AnalyzingFiles {
                        Scanner::do_state_analyzing_files(scan, task_id, initial_task_state.take(), task_progress.clone(), &interrupt_token)?;
                    }
                    loop_state = ScanState::AnalyzingScan;
                }
                ScanState::AnalyzingScan => {
                    task_progress.set_phase("Phase 4 of 4: Analyzing Scan");
                    if scan.state() == ScanState::AnalyzingScan {
                        Scanner::do_state_analyzing_scan(scan, task_progress.clone(), &interrupt_token)?;
                    }
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
            // NEW MODEL: bulk-insert deletion versions for alive items not seen in this scan.
            // Carries forward all state from the current version. No undo log needed — the
            // previous version is not modified (closing is conceptual), and the new deletion
            // version has first_scan_id = current_scan so it's simply deleted on rollback.
            c.execute(
                "INSERT INTO item_versions (
                    item_id, first_scan_id, last_scan_id,
                    is_added, is_deleted, access, mod_date, size,
                    file_hash, val, val_error,
                    last_hash_scan, last_val_scan,
                    add_count, modify_count, delete_count, unchanged_count
                 )
                 SELECT
                    iv.item_id, ?, ?,
                    0, 1, iv.access, iv.mod_date, iv.size,
                    iv.file_hash, iv.val, iv.val_error,
                    iv.last_hash_scan, iv.last_val_scan,
                    CASE WHEN i.item_type = 1 THEN 0 ELSE NULL END,
                    CASE WHEN i.item_type = 1 THEN 0 ELSE NULL END,
                    CASE WHEN i.item_type = 1 THEN 0 ELSE NULL END,
                    CASE WHEN i.item_type = 1 THEN 0 ELSE NULL END
                 FROM items i
                 JOIN item_versions iv ON iv.item_id = i.item_id
                 WHERE i.root_id = ?
                   AND iv.first_scan_id = (
                       SELECT MAX(iv2.first_scan_id)
                       FROM item_versions iv2
                       WHERE iv2.item_id = i.item_id
                   )
                   AND iv.is_deleted = 0
                   AND iv.last_scan_id < ?",
                (scan.scan_id(), scan.scan_id(), scan.root_id(), scan.scan_id()),
            )?;

            scan.set_state_analyzing_files(c)?;

            Ok(())
        })?;

        task_progress.add_breadcrumb("Tombstoned deleted items");

        Ok(())
    }

    fn do_state_analyzing_files(
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

        // If the scan doesn't hash or validate, skip to scan analyzing
        if !is_hash && !is_val {
            Scanner::check_interrupted(interrupt_token)?;
            scan.set_state_analyzing_scan(&conn)?;
            return Ok(());
        }

        // Parse initial task state for restart resilience (HWM loaded from TaskRow)
        let initial_state = ScanTaskState::from_task_state(initial_task_state.as_deref())?;
        let initial_hwm = initial_state.high_water_mark;

        let (analyze_total, analyze_done) =
            AnalysisItem::get_analysis_counts(&conn, scan.scan_id(), scan.analysis_spec(), initial_hwm)?;

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

            let analysis_items = AnalysisItem::fetch_next_batch(
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

        // It is critical that we check for interruption before advancing to the next state
        // (AnalyzingScan). If the interrupt was detected during this phase, some hashing or
        // validation operations may have been skipped, so we must not allow the scan to
        // progress further.
        Scanner::check_interrupted(interrupt_token)?;

        // If we got here without interruption, all items should have been processed.
        // Warn if the tracker still has items (indicates a bug in the tracking logic).
        tracker.warn_if_not_empty();

        // Clear thread states and add breadcrumb
        task_progress.clear_thread_states();
        task_progress.add_breadcrumb(&format!("Analyzed {} files", analyze_total));

        scan.set_state_analyzing_scan(&conn)?;
        Ok(())
    }

    /// Phase 4: Compute folder descendant change counts for the current scan.
    ///
    /// Walks the folder tree in the database depth-first, computing add/modify/delete
    /// counts for each folder's immediate children, then writing those counts to
    /// the folder's version row for this scan. Runs on a worker thread with
    /// elapsed-time progress reporting.
    fn do_state_analyzing_scan(
        scan: &mut Scan,
        task_progress: Arc<TaskProgress>,
        interrupt_token: &Arc<AtomicBool>,
    ) -> Result<(), FsPulseError> {
        Scanner::check_interrupted(interrupt_token)?;

        task_progress.set_indeterminate_progress("Computing folder counts...");

        let token = Arc::clone(interrupt_token);
        let scan_id = scan.scan_id();
        let root_id = scan.root_id();

        // Look up the root path
        let conn = Database::get_connection()?;
        let root = Root::get_by_id(&conn, root_id)?
            .ok_or_else(|| FsPulseError::Error(format!("Root {} not found", root_id)))?;
        let root_path = root.root_path().to_string();
        drop(conn);

        let handle = std::thread::spawn(move || {
            Scanner::scan_analysis_worker(root_id, scan_id, &root_path, &token)
        });

        let start = Instant::now();
        loop {
            if handle.is_finished() {
                handle.join().unwrap()?;
                break;
            }
            let elapsed = Utils::format_elapsed(start.elapsed());
            task_progress.set_indeterminate_progress(&format!("Computing folder counts... {}", elapsed));
            std::thread::sleep(Duration::from_millis(250));
        }

        task_progress.add_breadcrumb("Computed folder change counts");

        let conn = Database::get_connection()?;
        scan.set_state_completed(&conn)?;
        Ok(())
    }

    /// Worker function for scan analysis — runs on a spawned thread.
    ///
    /// Performs the recursive walk to compute folder counts, then writes them
    /// in batched transactions.
    fn scan_analysis_worker(
        root_id: i64,
        scan_id: i64,
        root_path: &str,
        interrupt_token: &Arc<AtomicBool>,
    ) -> Result<(), FsPulseError> {
        let conn = Database::get_connection()?;
        let prev_scan_id = Scanner::query_prev_completed_scan(&conn, root_id, scan_id)?;

        let mut writes = Vec::new();
        Scanner::walk_folder_counts(&conn, root_id, scan_id, root_path, interrupt_token, &mut writes)?;

        info!("Scan analysis: {} folders have descendant changes", writes.len());

        Scanner::apply_folder_count_writes(&conn, scan_id, &writes, prev_scan_id, interrupt_token)?;
        Ok(())
    }

    /// Find the most recent completed scan before the current one for this root.
    ///
    /// Used by Case B writes to restore `last_scan_id` without depending on the undo log.
    fn query_prev_completed_scan(
        conn: &Connection,
        root_id: i64,
        scan_id: i64,
    ) -> Result<Option<i64>, FsPulseError> {
        let prev: Option<i64> = conn
            .query_row(
                "SELECT MAX(scan_id) FROM scans
                 WHERE root_id = ? AND scan_id < ? AND state = 4",
                params![root_id, scan_id],
                |row| row.get(0),
            )
            .optional()?
            .flatten();
        Ok(prev)
    }

    /// Recursive depth-first walk of the folder tree, computing descendant change counts.
    ///
    /// Returns the cumulative `(adds, mods, dels)` for all descendants under `parent_path`.
    /// Appends a `FolderCountWrite` entry for each folder that has non-zero change counts
    /// (adds > 0 || mods > 0 || dels > 0).
    ///
    /// The `unchanged` count is derived per-folder at write time from the previous version:
    ///   `unchanged = prev_alive - mods - dels`
    /// where `prev_alive = prev_adds + prev_mods + prev_unchanged` from the folder's
    /// temporal version before this scan. Everyone alive previously was either modified,
    /// deleted, or unchanged in this scan — so unchanged is the remainder.
    fn walk_folder_counts(
        conn: &Connection,
        root_id: i64,
        scan_id: i64,
        parent_path: &str,
        interrupt_token: &Arc<AtomicBool>,
        writes: &mut Vec<FolderCountWrite>,
    ) -> Result<(i64, i64, i64), FsPulseError> {
        Scanner::check_interrupted(interrupt_token)?;

        let mut adds = 0i64;
        let mut mods = 0i64;
        let mut dels = 0i64;

        // 1. Get immediate directory children alive at this scan
        //    (including dirs deleted AT this scan — needed to recurse into deleted subtrees)
        let dir_children = Scanner::query_immediate_dir_children(conn, root_id, parent_path, scan_id)?;

        // 2. Recurse into each directory child
        for (_child_id, child_path) in &dir_children {
            let (sa, sm, sd) = Scanner::walk_folder_counts(
                conn, root_id, scan_id, child_path, interrupt_token, writes,
            )?;
            adds += sa;
            mods += sm;
            dels += sd;
        }

        // 3. Count direct children that changed in this scan
        let (da, dm, dd) = Scanner::query_direct_change_counts(conn, root_id, parent_path, scan_id)?;
        adds += da;
        mods += dm;
        dels += dd;

        // 4. Record write if any descendant changes
        if adds > 0 || mods > 0 || dels > 0 {
            if let Some(folder_item_id) = Scanner::lookup_folder_item_id(conn, root_id, parent_path)? {
                // Derive unchanged from previous version's alive count:
                // everyone alive before was either modified, deleted, or unchanged.
                let prev_alive = Scanner::query_prev_alive(conn, folder_item_id, scan_id)?;
                let unchanged = prev_alive - mods - dels;

                writes.push(FolderCountWrite {
                    folder_item_id,
                    adds,
                    mods,
                    dels,
                    unchanged,
                });
            }
        }

        Ok((adds, mods, dels))
    }

    /// Query immediate directory children of `parent_path` that are alive at `scan_id`
    /// (or deleted AT `scan_id`, so we can recurse into deleted subtrees).
    fn query_immediate_dir_children(
        conn: &Connection,
        root_id: i64,
        parent_path: &str,
        scan_id: i64,
    ) -> Result<Vec<(i64, String)>, FsPulseError> {
        let path_prefix = if parent_path.ends_with(MAIN_SEPARATOR_STR) {
            parent_path.to_string()
        } else {
            format!("{}{}", parent_path, MAIN_SEPARATOR_STR)
        };

        // Upper bound for range scan: replace trailing separator with next ASCII char.
        // Unix: '/' (0x2F) + 1 = '0' (0x30). Windows: '\' (0x5C) + 1 = ']' (0x5D).
        let path_upper = format!(
            "{}{}",
            &path_prefix[..path_prefix.len() - MAIN_SEPARATOR_STR.len()],
            char::from(std::path::MAIN_SEPARATOR as u8 + 1)
        );

        let sql = format!(
            "SELECT i.item_id, i.item_path
             FROM items i
             JOIN item_versions iv ON iv.item_id = i.item_id
             WHERE i.root_id = ?1
               AND i.item_type = 1
               AND iv.first_scan_id = (
                   SELECT MAX(first_scan_id) FROM item_versions
                   WHERE item_id = i.item_id AND first_scan_id <= ?2
               )
               AND (iv.is_deleted = 0 OR iv.first_scan_id = ?2)
               AND i.item_path >= ?3
               AND i.item_path < ?4
               AND i.item_path != ?5
               AND SUBSTR(i.item_path, LENGTH(?3) + 1) NOT LIKE '%{}%'",
            MAIN_SEPARATOR_STR
        );

        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map(
            params![root_id, scan_id, &path_prefix, &path_upper, parent_path],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;

        rows.collect::<Result<Vec<_>, _>>()
            .map_err(FsPulseError::DatabaseError)
    }

    /// Count direct children of `parent_path` that changed in this scan, classified
    /// as add/modify/delete by comparing the current version with the previous version.
    fn query_direct_change_counts(
        conn: &Connection,
        root_id: i64,
        parent_path: &str,
        scan_id: i64,
    ) -> Result<(i64, i64, i64), FsPulseError> {
        let path_prefix = if parent_path.ends_with(MAIN_SEPARATOR_STR) {
            parent_path.to_string()
        } else {
            format!("{}{}", parent_path, MAIN_SEPARATOR_STR)
        };

        // Upper bound for range scan: replace trailing separator with next ASCII char.
        // Unix: '/' (0x2F) + 1 = '0' (0x30). Windows: '\' (0x5C) + 1 = ']' (0x5D).
        let path_upper = format!(
            "{}{}",
            &path_prefix[..path_prefix.len() - MAIN_SEPARATOR_STR.len()],
            char::from(std::path::MAIN_SEPARATOR as u8 + 1)
        );

        let sql = format!(
            "SELECT
                COALESCE(SUM(CASE WHEN cv.is_deleted = 0
                    AND (pv.version_id IS NULL OR pv.is_deleted = 1) THEN 1 ELSE 0 END), 0),
                COALESCE(SUM(CASE WHEN cv.is_deleted = 0
                    AND pv.version_id IS NOT NULL AND pv.is_deleted = 0 THEN 1 ELSE 0 END), 0),
                COALESCE(SUM(CASE WHEN cv.is_deleted = 1
                    AND pv.version_id IS NOT NULL AND pv.is_deleted = 0 THEN 1 ELSE 0 END), 0)
             FROM items i
             JOIN item_versions cv ON cv.item_id = i.item_id AND cv.first_scan_id = ?1
             LEFT JOIN item_versions pv ON pv.item_id = i.item_id
                 AND pv.first_scan_id = (
                     SELECT MAX(first_scan_id) FROM item_versions
                     WHERE item_id = i.item_id AND first_scan_id < cv.first_scan_id
                 )
             WHERE i.root_id = ?2
               AND i.item_path >= ?3
               AND i.item_path < ?4
               AND i.item_path != ?5
               AND SUBSTR(i.item_path, LENGTH(?3) + 1) NOT LIKE '%{}%'",
            MAIN_SEPARATOR_STR
        );

        let mut stmt = conn.prepare(&sql)?;
        let result = stmt.query_row(
            params![scan_id, root_id, &path_prefix, &path_upper, parent_path],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )?;

        Ok(result)
    }

    /// Query the total alive descendant count from a folder's previous version.
    ///
    /// Returns `add_count + modify_count + unchanged_count` from the version just before
    /// `scan_id`. Returns 0 if no previous version exists (first scan or new folder).
    ///
    /// Used to derive: `unchanged = prev_alive - mods - dels` — everyone alive in the
    /// previous scan was either modified, deleted, or unchanged in this scan.
    fn query_prev_alive(
        conn: &Connection,
        folder_item_id: i64,
        scan_id: i64,
    ) -> Result<i64, FsPulseError> {
        let alive: Option<i64> = conn
            .query_row(
                "SELECT COALESCE(iv.add_count, 0) + COALESCE(iv.modify_count, 0) + COALESCE(iv.unchanged_count, 0)
                 FROM item_versions iv
                 WHERE iv.item_id = ?1
                   AND iv.first_scan_id = (
                       SELECT MAX(first_scan_id) FROM item_versions
                       WHERE item_id = ?1 AND first_scan_id < ?2
                   )",
                params![folder_item_id, scan_id],
                |row| row.get(0),
            )
            .optional()?;

        Ok(alive.unwrap_or(0))
    }

    /// Look up the item_id for a folder by its path.
    fn lookup_folder_item_id(
        conn: &Connection,
        root_id: i64,
        path: &str,
    ) -> Result<Option<i64>, FsPulseError> {
        let item_id: Option<i64> = conn
            .query_row(
                "SELECT item_id FROM items
                 WHERE root_id = ? AND item_path = ? AND item_type = 1",
                params![root_id, path],
                |row| row.get(0),
            )
            .optional()?;
        Ok(item_id)
    }

    /// Apply folder count writes in batched transactions.
    ///
    /// Each batch is committed independently; interrupts are checked between batches.
    fn apply_folder_count_writes(
        conn: &Connection,
        scan_id: i64,
        writes: &[FolderCountWrite],
        prev_scan_id: Option<i64>,
        interrupt_token: &Arc<AtomicBool>,
    ) -> Result<(), FsPulseError> {
        const BATCH_SIZE: usize = 500;

        for batch in writes.chunks(BATCH_SIZE) {
            Scanner::check_interrupted(interrupt_token)?;
            Database::immediate_transaction(conn, |c| {
                for w in batch {
                    Scanner::write_single_folder_count(c, scan_id, prev_scan_id, w)?;
                }
                Ok(())
            })?;
        }
        Ok(())
    }

    /// Write counts for a single folder.
    ///
    /// - **Case A**: Folder already has a version with `first_scan_id = scan_id` → UPDATE counts.
    /// - **Case B**: No version for this scan → close the pre-existing version by restoring
    ///   `last_scan_id` to `prev_scan_id`, then INSERT a new version carrying forward all
    ///   metadata with the computed counts.
    fn write_single_folder_count(
        conn: &Connection,
        scan_id: i64,
        prev_scan_id: Option<i64>,
        w: &FolderCountWrite,
    ) -> Result<(), FsPulseError> {
        // Check if folder already has a version for this scan
        let existing_version_id: Option<i64> = conn
            .query_row(
                "SELECT version_id FROM item_versions
                 WHERE item_id = ? AND first_scan_id = ?",
                params![w.folder_item_id, scan_id],
                |row| row.get(0),
            )
            .optional()?;

        if let Some(version_id) = existing_version_id {
            // Case A: UPDATE the existing version's counts
            conn.execute(
                "UPDATE item_versions SET add_count = ?, modify_count = ?, delete_count = ?, unchanged_count = ?
                 WHERE version_id = ?",
                params![w.adds, w.mods, w.dels, w.unchanged, version_id],
            )?;
        } else {
            // Case B: Folder metadata unchanged but descendants changed.
            // Get the current (latest) version for this folder.
            let current = ItemVersion::get_current(conn, w.folder_item_id)?;

            if let Some(version) = current {
                // Restore last_scan_id to previous completed scan (avoids undo log dependency)
                if let Some(prev) = prev_scan_id {
                    conn.execute(
                        "UPDATE item_versions SET last_scan_id = ? WHERE version_id = ?",
                        params![prev, version.version_id()],
                    )?;
                }

                // Insert new version carrying forward all metadata with computed counts
                ItemVersion::insert_full(
                    conn,
                    w.folder_item_id,
                    scan_id,
                    false,
                    version.is_deleted(),
                    version.access(),
                    version.mod_date(),
                    version.size(),
                    version.file_hash(),
                    version.val(),
                    version.val_error(),
                    version.last_hash_scan(),
                    version.last_val_scan(),
                    Some((w.adds, w.mods, w.dels, w.unchanged)),
                )?;
            }
        }

        Ok(())
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
            ExistingItem::get_by_root_path_type(ctx.conn, ctx.scan.root_id(), &path_str, item_type)?;

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
            Some(ref existing) if existing.version.last_scan_id() == ctx.scan.scan_id() => {
                // Already processed this scan - return cached size for folder aggregation
                Ok(existing.version.size().unwrap_or(0))
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
        existing_item: &ExistingItem,
        item_type: ItemType,
        mod_date: Option<i64>,
        size: Option<i64>,
        dir_read_error: bool,
        meta_error: bool,
    ) -> Result<(), FsPulseError> {
        let old_access = existing_item.version.access();
        let new_access =
            Scanner::calculate_new_access(item_type, old_access, dir_read_error, meta_error);
        let access_changed = old_access != new_access;
        let meta_change = existing_item.version.mod_date() != mod_date || existing_item.version.size() != size;

        if existing_item.version.is_deleted() {
            Scanner::handle_tombstone_rehydration(
                ctx,
                existing_item,
                item_type,
                mod_date,
                size,
                new_access,
            )
        } else if meta_change || access_changed {
            Scanner::handle_item_modification(
                ctx,
                existing_item,
                item_type,
                mod_date,
                size,
                old_access,
                new_access,
            )
        } else {
            // No change at all - just update last_scan
            Scanner::handle_item_no_change(ctx, existing_item)
        }
    }

    /// Handle tombstone rehydration (item coming back from deletion)
    fn handle_tombstone_rehydration(
        ctx: &mut ScanContext,
        existing_item: &ExistingItem,
        item_type: ItemType,
        mod_date: Option<i64>,
        size: Option<i64>,
        new_access: Access,
    ) -> Result<(), FsPulseError> {
        ctx.execute_batch_write(|c| {
            // Insert new alive version (old deleted version is not modified —
            // its last_scan_id already reflects when the deletion was last confirmed,
            // and temporal queries resolve via MAX(first_scan_id), not last_scan_id)
            let counts = if item_type == ItemType::Directory { Some((0, 0, 0, 0)) } else { None };
            ItemVersion::insert_initial(
                c, existing_item.item_id, ctx.scan.scan_id(), new_access, mod_date, size, counts,
            )?;

            // For rehydration, we alert whenever new_access is not Ok, regardless of what
            // the old access state was (since the item was a tombstone, any new access
            // error is a problem worth alerting on, similar to a brand new item)
            if new_access != Access::Ok {
                Alerts::add_access_denied_alert(c, ctx.scan.scan_id(), existing_item.item_id)?;
            }

            Ok(())
        })
    }

    /// Handle item modification (metadata and/or access change)
    fn handle_item_modification(
        ctx: &mut ScanContext,
        existing_item: &ExistingItem,
        item_type: ItemType,
        mod_date: Option<i64>,
        size: Option<i64>,
        old_access: Access,
        new_access: Access,
    ) -> Result<(), FsPulseError> {
        ctx.execute_batch_write(|c| {
            // Insert new version carrying forward hash/val from previous version
            // (old version is not modified — its last_scan_id already reflects when it was
            // last confirmed, and temporal queries resolve via MAX(first_scan_id))
            ItemVersion::insert_with_carry_forward(
                c, existing_item.item_id, ctx.scan.scan_id(),
                false, new_access, mod_date, size, &existing_item.version,
                item_type == ItemType::Directory,
            )?;

            if Scanner::should_alert_access_denied(old_access, new_access) {
                Alerts::add_access_denied_alert(c, ctx.scan.scan_id(), existing_item.item_id)?;
            }

            Ok(())
        })
    }

    /// Handle item with no changes - just update last_scan
    fn handle_item_no_change(
        ctx: &mut ScanContext,
        existing_item: &ExistingItem,
    ) -> Result<(), FsPulseError> {
        ctx.execute_batch_write(|c| {
            UndoLog::log_update(c, &existing_item.version)?;
            ItemVersion::touch_last_scan(c, existing_item.version.version_id(), ctx.scan.scan_id())?;

            Ok(())
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
            let item_id = ItemIdentity::insert(c, ctx.scan.root_id(), path_str, item_type)?;
            let counts = if item_type == ItemType::Directory { Some((0, 0, 0, 0)) } else { None };
            ItemVersion::insert_initial(c, item_id, ctx.scan.scan_id(), new_access, mod_date, size, counts)?;

            if new_access != Access::Ok {
                Alerts::add_access_denied_alert(c, ctx.scan.scan_id(), item_id)?;
            }

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

        // values to use in the version update
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
                i_access = access;

                // Alert if item became inaccessible (Ok → error)
                alert_access_denied =
                    Scanner::should_alert_access_denied(analysis_item.access(), access);
            }
        }

        if analysis_item.needs_hash() {
            if analysis_item.file_hash() != new_hash.as_deref() {
                update_changes = true;
                i_hash = new_hash.as_deref();

                // The hash changed. It's suspicious if metadata never changed
                // between the last hash scan and now. Only check if there was
                // a previous hash (otherwise it's the first hash, not suspicious).
                if analysis_item.last_hash_scan().is_some() {
                    alert_possible_hash = true;
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
                    alert_invalid_item = true;
                }

                update_changes = true;
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
            let current_version = ItemVersion::get_current(c, analysis_item.item_id())?
                .ok_or_else(|| FsPulseError::Error(format!(
                    "No current version for item {} during analysis",
                    analysis_item.item_id()
                )))?;

            if current_version.first_scan_id() == scan.scan_id() {
                // Same-scan version: UPDATE in place (no undo log — row is deleted on rollback)
                ItemVersion::update_analysis_in_place(
                    c, current_version.version_id(),
                    i_access, i_hash, i_val, i_val_error,
                    i_last_hash_scan, i_last_val_scan,
                )?;
            } else if update_changes {
                // Pre-existing version with state change: undo the walk-phase touch,
                // then INSERT a new version as the sole current version for this scan.
                ItemVersion::restore_last_scan(c, current_version.version_id())?;

                ItemVersion::insert_full(
                    c, analysis_item.item_id(), scan.scan_id(),
                    false, false, i_access,
                    current_version.mod_date(), current_version.size(),
                    i_hash, Some(i_val), i_val_error,
                    i_last_hash_scan, i_last_val_scan,
                    None, // counts: analysis only processes files
                )?;
            } else if i_last_hash_scan != current_version.last_hash_scan()
                || i_last_val_scan != current_version.last_val_scan()
            {
                // Pre-existing version, no state change: UPDATE bookkeeping only.
                // No additional undo log needed — handle_item_no_change already logged
                // the pre-scan values of last_hash_scan and last_val_scan.
                ItemVersion::update_bookkeeping(
                    c, current_version.version_id(),
                    i_last_hash_scan, i_last_val_scan,
                )?;
            }

            // Alerts
            // alert_possible_hash is only set when last_hash_scan is Some
            if alert_possible_hash {
                let last_hash_scan = analysis_item.last_hash_scan().unwrap();
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
                        new_hash.as_deref().unwrap(),
                    )?;
                }
            }

            if alert_invalid_item {
                Alerts::add_invalid_item_alert(
                    c,
                    scan.scan_id(),
                    analysis_item.item_id(),
                    new_val_error.as_deref().unwrap_or("Unknown error"),
                )?;
            }

            if alert_access_denied {
                Alerts::add_access_denied_alert(c, scan.scan_id(), analysis_item.item_id())?;
            }

            Ok(())
        })?;

        Ok(())
    }


    /// Check if scan has been interrupted, returning error if so.
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
