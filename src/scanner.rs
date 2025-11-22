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
use crate::items::{AnalysisItem, Item, ItemType};
use crate::progress::ProgressReporter;
use crate::roots::Root;
use crate::scans::ScanState;
use crate::validate::validator::{from_path, ValidationState};
use crate::{database::Database, error::FsPulseError, scans::Scan};

use crossbeam_channel::bounded;
use log::{error, info, Level};
use logging_timer::timer;
use rusqlite::Connection;
use threadpool::ThreadPool;

use std::fs::Metadata;
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
    reporter: &'a Arc<ProgressReporter>,
    interrupt_token: &'a Arc<AtomicBool>,
    batch_count: usize,
}

impl<'a> ScanContext<'a> {
    fn new(
        conn: &'a Connection,
        scan: &'a Scan,
        reporter: &'a Arc<ProgressReporter>,
        interrupt_token: &'a Arc<AtomicBool>,
    ) -> Self {
        Self {
            conn,
            scan,
            reporter,
            interrupt_token,
            batch_count: 0,
        }
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
        reporter: Arc<ProgressReporter>,
        interrupt_token: Arc<AtomicBool>,
    ) -> Result<(), FsPulseError> {
        // NOTE: We check for interrupt at appropriate points in the scanner code with:
        //
        //   if interrupt_token.load(Ordering::Acquire) {
        //       return Err(FsPulseError::ScanInterrupted);
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
                    reporter.start_scanning_phase();
                    if scan.state() == ScanState::Scanning {
                        Scanner::do_state_scanning(
                            root,
                            scan,
                            reporter.clone(),
                            &interrupt_token,
                        )?;
                    }
                    loop_state = ScanState::Sweeping;
                }
                ScanState::Sweeping => {
                    reporter.start_sweeping_phase();
                    if scan.state() == ScanState::Sweeping {
                        Scanner::do_state_sweeping(scan, &interrupt_token)?;
                    }
                    loop_state = ScanState::Analyzing;
                }
                ScanState::Analyzing => {
                    let analysis_result = if scan.state() == ScanState::Analyzing {
                        Scanner::do_state_analyzing(
                            scan,
                            reporter.clone(),
                            &interrupt_token,
                        )
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

        ctx.reporter.increment_directories_scanned();

        let items = {
            let _tmr = timer!(Level::Trace; "fs::read_dir", "{}", path.display());
            fs::read_dir(path)?
        };
        let mut total_size: i64 = 0;

        for item in items {
            let item = item?;
            let item_path = item.path();
            let item_metadata = {
                let _tmr = timer!(Level::Trace; "fs::symlink_metadata", "{}", item_path.display());
                fs::symlink_metadata(&item_path)?
            };

            ctx.reporter.increment_files_scanned();

            if item_metadata.is_dir() {
                // Recursively scan the subdirectory and get its size
                let subdir_size = Scanner::scan_directory_recursive(ctx, &item_path)?;

                // Handle the subdirectory with its computed size
                let returned_size = Scanner::handle_scan_item(
                    ctx,
                    ItemType::Directory,
                    &item_path,
                    &item_metadata,
                    Some(subdir_size),
                )?;

                total_size += returned_size;
            } else {
                Scanner::check_interrupted(ctx.interrupt_token)?;

                // Handle files, symlinks, and other items
                let item_type = if item_metadata.is_file() {
                    ItemType::File
                } else if item_metadata.is_symlink() {
                    ItemType::Symlink
                } else {
                    ItemType::Other
                };

                // Files have meaningful sizes, symlinks and other don't
                let item_size = if item_metadata.is_file() {
                    Some(item_metadata.len() as i64)
                } else {
                    None
                };

                let returned_size = Scanner::handle_scan_item(
                    ctx,
                    item_type,
                    &item_path,
                    &item_metadata,
                    item_size,
                )?;

                total_size += returned_size;
            }
        }

        Ok(total_size)
    }

    fn do_state_scanning(
        root: &Root,
        scan: &mut Scan,
        reporter: Arc<ProgressReporter>,
        interrupt_token: &Arc<AtomicBool>,
    ) -> Result<(), FsPulseError> {
        let conn = Database::get_connection()?;
        let root_path_buf = PathBuf::from(root.root_path());

        // Create scanning context
        let mut ctx = ScanContext::new(&conn, scan, &reporter, interrupt_token);

        // Recursively scan the root directory and get the total size
        // Note: We don't store the root directory itself as an item in the database
        let total_size = Scanner::scan_directory_recursive(&mut ctx, &root_path_buf)?;

        // Flush any remaining batched writes
        ctx.flush()?;

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

        scan.set_state_analyzing(&conn)
    }

    fn do_state_analyzing(
        scan: &mut Scan,
        reporter: Arc<ProgressReporter>,
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

        let (analyze_total, analyze_done) =
            Item::get_analysis_counts(&conn, scan.scan_id(), scan.analysis_spec())?;

        reporter.start_analyzing_phase(analyze_total, analyze_done);

        // Create a bounded channel to limit the number of queued tasks (e.g., max 100 tasks)
        let (sender, receiver) = bounded::<AnalysisItem>(100);

        // Initialize the thread pool
        let items_remaining = analyze_total.saturating_sub(analyze_done); // avoids underflow
        let items_remaining_usize = items_remaining.try_into().unwrap_or(usize::MAX);

        let thread_count = crate::config::Config::get_analysis_threads();

        let num_threads = cmp::min(items_remaining_usize, thread_count);
        let pool = ThreadPool::new(num_threads.max(1)); // ensure at least one thread

        for thread_index in 0..num_threads {
            // Clone shared resources for each worker thread.
            let receiver = receiver.clone();
            let scan_copy = scan.clone();
            let reporter_clone = Arc::clone(&reporter);
            let interrupt_token_clone = Arc::clone(interrupt_token);

            // Set initial idle state
            reporter.set_thread_idle(thread_index);

            // Worker thread: continuously receive and process tasks.
            // Each thread gets its own connection from the pool!
            pool.execute(move || {
                while let Ok(analysis_item) = receiver.recv() {
                    Scanner::process_item_async(
                        &scan_copy,
                        analysis_item,
                        thread_index,
                        &reporter_clone,
                        &interrupt_token_clone,
                    );
                }
                reporter_clone.set_thread_idle(thread_index);
            });
        }

        let mut last_item_id = 0;

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
        scan.set_state_completed(&conn)
    }

    fn process_item_async(
        scan: &Scan,
        analysis_item: AnalysisItem,
        thread_index: usize,
        reporter: &Arc<ProgressReporter>,
        interrupt_token: &Arc<AtomicBool>,
    ) {
        // TODO: Improve the error handling for all analysis. Need to differentiate
        // between file system errors and actual content errors

        // This function is the entry point for each worker thread to process an item.
        // It performs hashing and/or validation as needed and updates the database.
        // It does not return errors, but it does need to check for interruption.
        // If an interrupt is detected, it should exit promptly without updating
        // the database. The hashing and validation processes exit when detecting
        // interrupt and may return an interrupt error, but we ignore that here.
        // The calling code will always check for interrupt and do the right thing
        // depending on why the interrupt was generated

        let path = Path::new(analysis_item.item_path());

        info!("Beginning analysis of: {path:?}");

        let display_path = path
            .file_name()
            .unwrap_or(path.as_os_str())
            .to_string_lossy();

        let mut new_hash = None;

        if analysis_item.needs_hash() && !Scanner::is_interrupted(interrupt_token) {
            reporter.set_thread_hashing(thread_index, display_path.to_string());

            // The hash function checks for interrupt at its start and periodically
            new_hash = match Hash::compute_sha2_256_hash(path, interrupt_token) {
                Ok(hash_s) => Some(hash_s),
                Err(error) => {
                    error!("Error hashing '{}': {}", &display_path, error);
                    // If hashing fails, we set the hash to the error string
                    // This isn't great, but it allows us to have a string value when stopping a scan
                    // and leaves an error artifact behind for investigation
                    Some(error.to_string())
                }
            };
        }

        let mut new_val = ValidationState::Unknown;
        let mut new_val_error = None;

        if analysis_item.needs_val() && !Scanner::is_interrupted(interrupt_token) {
            let validator = from_path(path);
            match validator {
                Some(v) => {
                    reporter.set_thread_validating(thread_index, display_path.to_string());

                    match v.validate(path, interrupt_token) {
                        Ok((res_validity_state, res_validation_error)) => {
                            new_val = res_validity_state;
                            new_val_error = res_validation_error;
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

        if !Scanner::is_interrupted(interrupt_token) {
            if let Err(error) = Scanner::update_item_analysis(
                scan,
                &analysis_item,
                new_hash,
                new_val,
                new_val_error,
                interrupt_token,
            ) {
                let e_str = error.to_string();
                error!(
                    "Error updating item analysis '{}': {}",
                    &display_path, e_str
                );
            }
        }

        reporter.increment_analysis_completed();

        // Set thread back to idle after completing work
        reporter.set_thread_idle(thread_index);

        info!("Done analyzing: {path:?}");
    }

    fn handle_scan_item(
        ctx: &mut ScanContext,
        item_type: ItemType,
        path: &Path,
        metadata: &Metadata,
        computed_size: Option<i64>,
    ) -> Result<i64, FsPulseError> {
        let _tmr = timer!(Level::Trace; "handle_scan_item", "{}", path.display());

        // load the item
        let path_str = path.to_string_lossy();
        let existing_item = Item::get_by_root_path_type(ctx.conn, ctx.scan.root_id(), &path_str, item_type)?;

        let mod_date = metadata
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs() as i64);

        let size = computed_size;

        if let Some(existing_item) = existing_item {
            // If the item was already processed for this scan, return its size from the database.
            // This allows scan resumption to work correctly - we don't re-process the item, but
            // we do need its size for folder size aggregation. We intentionally do not handle
            // the case where the item was seen within this scan but has since been modified or
            // changed type. There are edge cases where this might cause strangeness in reports
            // such as when an item was seen as a file, the scan was resumed and the item has
            // changed into a directory. In this case, we'll still traverse the children within
            // the resumed scan and a tree report will look odd.
            if existing_item.last_scan() == ctx.scan.scan_id() {
                return Ok(existing_item.size().unwrap_or(0));
            }

            let meta_change = existing_item.mod_date() != mod_date || existing_item.size() != size;

            if existing_item.is_ts() {
                // Rehydrate a tombstone
                ctx.execute_batch_write(|c| {
                    let rows_updated = c.execute(
                        "UPDATE items SET
                                is_ts = 0,
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
                            mod_date,
                            size,
                            ValidationState::Unknown.as_i64(),
                            ctx.scan.scan_id(),
                            existing_item.item_id(),
                        ),
                    )?;
                    if rows_updated == 0 {
                        return Err(FsPulseError::Error(format!(
                            "Item Id {} not found for update",
                            existing_item.item_id()
                        )));
                    }

                    c.execute(
                        "INSERT INTO changes
                            (
                                scan_id,
                                item_id,
                                change_type,
                                is_undelete,
                                mod_date_old,
                                mod_date_new,
                                size_old,
                                size_new,
                                hash_old,
                                val_old,
                                val_error_old
                            )
                        VALUES
                            (?, ?, ?, 1, ?, ?, ?, ?, ?, ?, ?)",
                        (
                            ctx.scan.scan_id(),
                            existing_item.item_id(),
                            ChangeType::Add.as_i64(),
                            existing_item.mod_date(),
                            mod_date,
                            existing_item.size(),
                            size,
                            existing_item.file_hash(),
                            existing_item.validity_state_as_str(),
                            existing_item.val_error(),
                        ),
                    )?;

                    Ok(())
                })?;
            } else if meta_change {
                let _tmr = timer!(Level::Trace; "db::immediate_transaction meta_change", "{}", path_str);
                ctx.execute_batch_write(|c| {
                    let rows_updated = c.execute(
                        "UPDATE items SET
                            mod_date = ?,
                            size = ?,
                            last_scan = ?
                        WHERE item_id = ?",
                        (mod_date, size, ctx.scan.scan_id(), existing_item.item_id()),
                    )?;
                    if rows_updated == 0 {
                        return Err(FsPulseError::Error(format!(
                            "Item Id {} not found for update",
                            existing_item.item_id()
                        )));
                    }
                    c.execute(
                        "INSERT INTO changes
                                (
                                    scan_id,
                                    item_id,
                                    change_type,
                                    meta_change,
                                    mod_date_old,
                                    mod_date_new,
                                    size_old,
                                    size_new)
                                VALUES (?, ?, ?, 1, ?, ?, ?, ?)",
                        (
                            ctx.scan.scan_id(),
                            existing_item.item_id(),
                            ChangeType::Modify.as_i64(),
                            meta_change.then_some(existing_item.mod_date()),
                            meta_change.then_some(mod_date),
                            meta_change.then_some(existing_item.size()),
                            meta_change.then_some(size),
                        ),
                    )?;

                    Ok(())
                })?;
            } else {
                // No change - just update last_scan
                ctx.execute_batch_write(|c| {
                    let rows_updated = c.execute(
                        "UPDATE items SET last_scan = ? WHERE item_id = ?",
                        (ctx.scan.scan_id(), existing_item.item_id()),
                    )?;

                    if rows_updated == 0 {
                        return Err(FsPulseError::Error(format!(
                            "Item Id {} not found for update",
                            existing_item.item_id()
                        )));
                    }
                    Ok(())
                })?;
            }
        } else {
            // Item is new, insert into items and changes tables
            let _tmr = timer!(Level::Trace; "db::immediate_transaction new_item", "{}", path_str);
            ctx.execute_batch_write(|c| {
                c.execute("INSERT INTO items (root_id, item_path, item_type, mod_date, size, val, last_scan) VALUES (?, ?, ?, ?, ?, ?, ?)",
                    (ctx.scan.root_id(), &path_str, item_type.as_i64(), mod_date, size, ValidationState::Unknown.as_i64(), ctx.scan.scan_id()))?;

                let item_id: i64 = c.query_row("SELECT last_insert_rowid()", [], |row| row.get(0))?;

                c.execute("INSERT INTO changes (scan_id, item_id, change_type, is_undelete, mod_date_new, size_new, hash_change, val_change) VALUES (?, ?, ?, 0, ?, ?, 0, 0)",
                    (ctx.scan.scan_id(), item_id, ChangeType::Add.as_i64(), mod_date, size))?;

                Ok(())
            })?;
        }

        // Return the size for folder aggregation (0 if None)
        Ok(computed_size.unwrap_or(0))
    }

    pub fn update_item_analysis(
        scan: &Scan,
        analysis_item: &AnalysisItem,
        new_hash: Option<String>,
        new_val: ValidationState,
        new_val_error: Option<String>,
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

        // values to use in the item update
        let mut i_hash = analysis_item.file_hash();
        let mut i_val = analysis_item.val();
        let mut i_val_error = analysis_item.val_error();
        let mut i_last_hash_scan = analysis_item.last_hash_scan();
        let mut i_last_val_scan = analysis_item.last_val_scan();

        let mut alert_possible_hash = false;
        let mut alert_invalid_item = false;

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

            // Step 1: UPSERT into `changes` table if the change is something other than moving from the default state
            if update_changes {
                c.execute(
                    "INSERT INTO changes (
                            scan_id,
                            item_id,
                            change_type,
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
                        VALUES (?, ?, 2, 0, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                    ON CONFLICT(scan_id, item_id)
                        DO UPDATE SET
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
                    file_hash = ?,
                    val = ?,
                    val_error = ?,
                    last_hash_scan = ?,
                    last_val_scan = ?
                WHERE item_id = ?",
                rusqlite::params![
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
            Err(FsPulseError::ScanInterrupted)
        } else {
            Ok(())
        }
    }

    /// Check if scan has been interrupted
    fn is_interrupted(interrupt_token: &Arc<AtomicBool>) -> bool {
        interrupt_token.load(Ordering::Acquire)
    }
}
