use crossbeam_channel::bounded;
use log::{error, info};
use rusqlite::{params, Connection, OptionalExtension};
use threadpool::ThreadPool;

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::{cmp, io::ErrorKind};

use crate::db::Database;
use crate::error::FsPulseError;
use crate::hash::Hash;
use crate::item_identity::Access;
use crate::scans::{AnalysisSpec, Scan};
use crate::task::{AnalysisTracker, ScanTaskState, TaskProgress};
use crate::validate::validator::ValidationState;

use super::hash_analysis;
use super::val_analysis;

/// Run the file analysis phase (Phase 3 of 4).
///
/// Fetches batches of items needing hash/val analysis and dispatches them
/// to worker threads. Each worker calls `analyze_item` for its item.
pub fn run_analysis_phase(
    scan: &mut Scan,
    task_id: i64,
    initial_task_state: Option<String>,
    task_progress: Arc<TaskProgress>,
    interrupt_token: &Arc<AtomicBool>,
) -> Result<(), FsPulseError> {
    let conn = Database::get_connection()?;

    let is_hash = scan.analysis_spec().is_hash();
    let is_val = scan.analysis_spec().is_val();

    // If the scan doesn't hash or validate, skip to scan analyzing
    if !is_hash && !is_val {
        check_interrupted(interrupt_token)?;
        scan.set_state_analyzing_scan(&conn)?;
        return Ok(());
    }

    // Compute prev_scan_id once for Case B access versioning (same query as Phase 4)
    let prev_scan_id = query_prev_completed_scan(&conn, scan.root_id(), scan.scan_id())?;

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
    let items_remaining = analyze_total.saturating_sub(analyze_done);
    let items_remaining_usize = items_remaining.try_into().unwrap_or(usize::MAX);

    let thread_count = crate::config::Config::get_analysis_threads();

    let num_threads = cmp::min(items_remaining_usize, thread_count);
    let pool = ThreadPool::new(num_threads.max(1));

    // Set up thread states
    task_progress.set_thread_count(num_threads);

    for thread_index in 0..num_threads {
        let receiver = receiver.clone();
        let scan_copy = scan.clone();
        let task_progress_clone = Arc::clone(&task_progress);
        let interrupt_token_clone = Arc::clone(interrupt_token);
        let tracker_clone = Arc::clone(&tracker);

        pool.execute(move || {
            while let Ok(analysis_item) = receiver.recv() {
                analyze_item(
                    &scan_copy,
                    analysis_item,
                    prev_scan_id,
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
            last_item_id = analysis_item.item_id();

            sender
                .send(analysis_item)
                .expect("Failed to send task into the bounded channel");
        }
    }

    // Drop the sender to signal the workers that no more items will come.
    drop(sender);

    // Wait for all tasks to complete.
    pool.join();

    check_interrupted(interrupt_token)?;

    // If we got here without interruption, all items should have been processed.
    tracker.warn_if_not_empty();

    // Clear thread states and add breadcrumb
    task_progress.clear_thread_states();
    task_progress.add_breadcrumb("Analysis phase complete");

    // Advance to next state
    scan.set_state_analyzing_scan(&conn)?;

    Ok(())
}

/// Process a single item: compute hash/validate, determine state, persist.
///
/// Called by worker threads. Does not return errors — logs them instead.
fn analyze_item(
    scan: &Scan,
    mut analysis_item: AnalysisItem,
    prev_scan_id: Option<i64>,
    thread_index: usize,
    task_progress: &Arc<TaskProgress>,
    interrupt_token: &Arc<AtomicBool>,
    tracker: &Arc<AnalysisTracker>,
) {
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

    // --- Hash computation ---
    if analysis_item.needs_hash() && !is_interrupted(interrupt_token) {
        task_progress.set_thread_state(thread_index, "Hashing", "info", Some(&display_path));
        read_attempted = true;

        match hash_analysis::compute_hash(&path, interrupt_token) {
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
                log::warn!(
                    "File not found during hashing '{}': skipping analysis",
                    &display_path
                );
                file_not_found = true;
            }
            Err(error) => {
                error!("Error hashing '{}': {}", &display_path, error);
            }
        };
    }

    // --- Validation ---
    let mut new_val = ValidationState::Unknown;
    let mut new_val_error = None;

    if analysis_item.needs_val()
        && !read_permission_denied
        && !file_not_found
        && !is_interrupted(interrupt_token)
    {
        match val_analysis::run_validation(&path, interrupt_token) {
            Ok((state, err)) => {
                read_attempted = true;
                new_val = state;
                new_val_error = err;
                read_succeeded = true;
            }
            Err(ValAnalysisError::PermissionDenied) => {
                read_attempted = true;
                error!(
                    "Cannot read file for validation '{}': Permission denied",
                    &display_path
                );
                read_permission_denied = true;
            }
            Err(ValAnalysisError::NotFound) => {
                log::warn!(
                    "File not found during validation '{}': skipping analysis",
                    &display_path
                );
                file_not_found = true;
            }
            Err(ValAnalysisError::NoValidator) => {
                // Should not happen — needs_val is gated on has_validator.
                // If it does, just skip validation for this item.
                log::warn!("NoValidator error for item that passed needs_val check: {:?}", &display_path);
            }
            Err(ValAnalysisError::ValidationError(e_str)) => {
                read_attempted = true;
                error!("Error validating '{}': {}", &display_path, e_str);
                new_val = ValidationState::Invalid;
                new_val_error = Some(e_str);
            }
        }
    }

    // If the file was not found, disable hash/val analysis — the item was deleted
    // between scan/sweep and analysis. The item will be picked up next scan.
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
        None
    };

    if !is_interrupted(interrupt_token) {
        if let Err(error) = persist_analysis(
            scan,
            &analysis_item,
            prev_scan_id,
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
    task_progress.set_thread_idle(thread_index);

    // Mark item as completed in the tracker (updates HWM)
    if let Err(e) = tracker.complete_item(item_id) {
        error!("Failed to complete item {}: {}", item_id, e);
    }
}

/// Persist hash and validation results to the database.
///
/// Writes to `hash_versions` and `val_versions` tables. Also handles
/// access state changes on `item_versions` and alert creation.
fn persist_analysis(
    scan: &Scan,
    analysis_item: &AnalysisItem,
    prev_scan_id: Option<i64>,
    new_hash: Option<String>,
    new_val: ValidationState,
    new_val_error: Option<String>,
    new_access: Option<Access>,
    interrupt_token: &Arc<AtomicBool>,
) -> Result<(), FsPulseError> {
    // Determine what needs to be written
    let hash_changed = analysis_item.needs_hash()
        && analysis_item.file_hash() != new_hash.as_deref();

    let val_state_changed = val_analysis::is_val_changed(analysis_item, new_val, new_val_error.as_deref());

    let mut access_changed = false;
    let mut new_access_value = analysis_item.access();

    if let Some(access) = new_access {
        if access != analysis_item.access() {
            access_changed = true;
            new_access_value = access;
        }
    }

    check_interrupted(interrupt_token)?;

    // Pre-write file guard
    if analysis_item.needs_hash() || analysis_item.needs_val() {
        let path = std::path::Path::new(analysis_item.item_path());
        if !super::file_guard::check_file_unchanged(
            path,
            analysis_item.mod_date(),
            analysis_item.size(),
        ) {
            info!(
                "File guard: skipping analysis write for {:?} (file changed since walk)",
                analysis_item.item_path()
            );
            return Ok(());
        }
    }

    let conn = Database::get_connection()?;

    Database::immediate_transaction(&conn, |c| {
        // Hash persistence
        if analysis_item.needs_hash() {
            hash_analysis::persist_hash(
                c, scan, analysis_item, new_hash.as_deref(), hash_changed,
            )?;
        }

        // Val persistence
        if analysis_item.needs_val() {
            val_analysis::persist_val(
                c, scan, analysis_item, new_val, new_val_error.as_deref(), val_state_changed,
            )?;
        }

        // Access state change on item_versions — uses Case A/B pattern
        // like Phase 4 folder counts. Access changes create proper version
        // boundaries so they are correctly scoped in time and rolled back.
        if access_changed {
            if analysis_item.version_first_scan_id() == scan.scan_id() {
                // Case A: Version was created this scan — UPDATE in place.
                // No undo needed; the entire version is deleted on rollback.
                c.execute(
                    "UPDATE item_versions SET access = ? WHERE version_id = ?",
                    rusqlite::params![new_access_value.as_i64(), analysis_item.version_id()],
                )?;
            } else {
                // Case B: Pre-existing version. Close it by restoring last_scan_id
                // to prev_scan_id, then INSERT a new version with the changed access.
                // On rollback, the new version is deleted (first_scan_id = scan_id)
                // and the old version's last_scan_id is restored by the undo log
                // entry from Phase 1's touch_last_scan.
                if let Some(prev) = prev_scan_id {
                    c.execute(
                        "UPDATE item_versions SET last_scan_id = ? WHERE version_id = ?",
                        rusqlite::params![prev, analysis_item.version_id()],
                    )?;
                }

                crate::item_version::ItemVersion::insert_full(
                    c,
                    analysis_item.item_id(),
                    scan.scan_id(),
                    false,          // is_added
                    false,          // is_deleted (analysis only runs on non-deleted items)
                    new_access_value,
                    analysis_item.mod_date(),
                    analysis_item.size(),
                    None,           // counts (files only, no folder counts)
                )?;
            }
        }

        // Alerts
        if analysis_item.needs_val() && val_state_changed && new_val == ValidationState::Invalid {
            crate::alerts::Alerts::add_invalid_item_alert(
                c,
                scan.scan_id(),
                analysis_item.item_id(),
                new_val_error.as_deref().unwrap_or("Unknown error"),
            )?;
        }

        if access_changed {
            let should_alert = should_alert_access_denied(analysis_item.access(), new_access_value);
            if should_alert {
                crate::alerts::Alerts::add_access_denied_alert(c, scan.scan_id(), analysis_item.item_id())?;
            }
        }

        Ok(())
    })?;

    Ok(())
}

/// An item ready for the analysis phase, with its current state and flags
/// indicating which analysis operations are needed.
///
/// Hash/val state is sourced from `hash_versions` and `val_versions` tables
/// via LEFT JOINs, not from `item_versions`.
#[derive(Clone, Debug)]
pub struct AnalysisItem {
    item_id: i64,
    item_path: String,
    version_id: i64,
    version_first_scan_id: i64,
    access: i64,
    mod_date: Option<i64>,
    size: Option<i64>,
    has_validator: bool,
    // From hash_versions (NULL if never hashed)
    hash_first_scan_id: Option<i64>,
    hash_last_scan_id: Option<i64>,
    file_hash: Option<String>,
    // From val_versions (NULL if never validated)
    val_first_scan_id: Option<i64>,
    val_state: Option<i64>,
    val_error: Option<String>,
    // Computed flags
    needs_hash: bool,
    needs_val: bool,
}

impl AnalysisItem {
    pub fn item_id(&self) -> i64 {
        self.item_id
    }

    pub fn item_path(&self) -> &str {
        &self.item_path
    }

    pub fn version_id(&self) -> i64 {
        self.version_id
    }

    pub fn version_first_scan_id(&self) -> i64 {
        self.version_first_scan_id
    }

    pub fn access(&self) -> Access {
        Access::from_i64(self.access)
    }

    pub fn mod_date(&self) -> Option<i64> {
        self.mod_date
    }

    pub fn size(&self) -> Option<i64> {
        self.size
    }

    #[allow(dead_code)]
    pub fn has_validator(&self) -> bool {
        self.has_validator
    }

    pub fn hash_first_scan_id(&self) -> Option<i64> {
        self.hash_first_scan_id
    }

    pub fn hash_last_scan_id(&self) -> Option<i64> {
        self.hash_last_scan_id
    }

    pub fn file_hash(&self) -> Option<&str> {
        self.file_hash.as_deref()
    }

    pub fn val_first_scan_id(&self) -> Option<i64> {
        self.val_first_scan_id
    }

    pub fn val_state(&self) -> Option<ValidationState> {
        self.val_state.map(ValidationState::from_i64)
    }

    pub fn val_error(&self) -> Option<&str> {
        self.val_error.as_deref()
    }

    pub fn needs_hash(&self) -> bool {
        self.needs_hash
    }

    pub fn set_needs_hash(&mut self, value: bool) {
        self.needs_hash = value;
    }

    pub fn needs_val(&self) -> bool {
        self.needs_val
    }

    pub fn set_needs_val(&mut self, value: bool) {
        self.needs_val = value;
    }

    fn from_row(row: &rusqlite::Row) -> rusqlite::Result<Self> {
        Ok(AnalysisItem {
            item_id: row.get(0)?,
            item_path: row.get(1)?,
            version_id: row.get(2)?,
            version_first_scan_id: row.get(3)?,
            access: row.get(4)?,
            mod_date: row.get(5)?,
            size: row.get(6)?,
            has_validator: row.get::<_, i64>(7)? != 0,
            hash_first_scan_id: row.get(8)?,
            hash_last_scan_id: row.get(9)?,
            file_hash: Hash::opt_blob_to_hex(row.get(10)?),
            val_first_scan_id: row.get(11)?,
            val_state: row.get(12)?,
            val_error: row.get(13)?,
            needs_hash: row.get(14)?,
            needs_val: row.get(15)?,
        })
    }

    pub fn get_analysis_counts(
        conn: &Connection,
        scan_id: i64,
        analysis_spec: &AnalysisSpec,
        last_item_id: i64,
    ) -> Result<(u64, u64), FsPulseError> {
        // Sources hash/val state from hash_versions/val_versions via LEFT JOIN.
        // "Never hashed" = no row in hash_versions (hv columns are NULL).
        // "Never validated" = no row in val_versions (vv columns are NULL).
        let sql = r#"
            WITH candidates AS (
                SELECT
                    hv.last_scan_id AS hash_last_scan,
                    vv.last_scan_id AS val_last_scan,
                    CASE
                        WHEN $1 = 0 THEN 0
                        WHEN $2 = 1 AND (hv.file_hash IS NULL OR hv.last_scan_id < $3) THEN 1
                        WHEN hv.file_hash IS NULL THEN 1
                        WHEN cv.first_scan_id = $3 AND pv.version_id IS NULL THEN 1
                        WHEN cv.first_scan_id = $3 AND pv.is_deleted = 1 THEN 1
                        WHEN cv.first_scan_id = $3 AND (cv.mod_date IS NOT pv.mod_date OR cv.size IS NOT pv.size) THEN 1
                        ELSE 0
                    END AS needs_hash,
                    CASE
                        WHEN $4 = 0 THEN 0
                        WHEN i.has_validator = 0 THEN 0
                        WHEN $5 = 1 AND (vv.val_state IS NULL OR vv.last_scan_id < $3) THEN 1
                        WHEN vv.val_state IS NULL THEN 1
                        WHEN cv.first_scan_id = $3 AND pv.version_id IS NULL THEN 1
                        WHEN cv.first_scan_id = $3 AND pv.is_deleted = 1 THEN 1
                        WHEN cv.first_scan_id = $3 AND (cv.mod_date IS NOT pv.mod_date OR cv.size IS NOT pv.size) THEN 1
                        ELSE 0
                    END AS needs_val
                FROM items i
                JOIN item_versions cv
                    ON cv.item_id = i.item_id
                    AND cv.last_scan_id = $3
                LEFT JOIN item_versions pv
                    ON pv.item_id = i.item_id
                    AND pv.first_scan_id = (
                        SELECT MAX(first_scan_id)
                        FROM item_versions
                        WHERE item_id = i.item_id
                          AND first_scan_id < cv.first_scan_id
                    )
                LEFT JOIN hash_versions hv
                    ON hv.item_id = i.item_id
                    AND hv.first_scan_id = (
                        SELECT MAX(first_scan_id) FROM hash_versions WHERE item_id = i.item_id
                    )
                LEFT JOIN val_versions vv
                    ON vv.item_id = i.item_id
                    AND vv.first_scan_id = (
                        SELECT MAX(first_scan_id) FROM val_versions WHERE item_id = i.item_id
                    )
                WHERE
                    i.item_type = 0
                    AND cv.is_deleted = 0
                    AND cv.access <> 1
                    AND i.item_id > $6
            )
            SELECT
                COALESCE(SUM(CASE WHEN needs_hash = 1 OR needs_val = 1 THEN 1 ELSE 0 END), 0) AS total_needed,
                COALESCE(SUM(CASE
                    WHEN (needs_hash = 1 AND hash_last_scan = $3)
                    OR (needs_val = 1 AND val_last_scan = $3)
                    THEN 1 ELSE 0 END), 0) AS total_done
            FROM candidates"#;

        let mut stmt = conn.prepare_cached(sql)?;
        let mut rows = stmt.query(params![
            analysis_spec.is_hash() as i64,
            analysis_spec.hash_all() as i64,
            scan_id,
            analysis_spec.is_val() as i64,
            analysis_spec.val_all() as i64,
            last_item_id
        ])?;

        if let Some(row) = rows.next()? {
            let total_needed = row.get::<_, i64>(0)? as u64;
            let total_done = row.get::<_, i64>(1)? as u64;
            Ok((total_needed, total_done))
        } else {
            Ok((0, 0))
        }
    }

    pub fn fetch_next_batch(
        conn: &Connection,
        scan_id: i64,
        analysis_spec: &AnalysisSpec,
        last_item_id: i64,
        limit: usize,
    ) -> Result<Vec<AnalysisItem>, FsPulseError> {
        // Sources hash/val state from hash_versions/val_versions via LEFT JOIN.
        // needs_val is gated on i.has_validator = 1 — files without a validator are skipped.
        let query = format!(
            "SELECT
                i.item_id,
                i.item_path,
                cv.version_id,
                cv.first_scan_id,
                cv.access,
                cv.mod_date,
                cv.size,
                i.has_validator,
                hv.first_scan_id,
                hv.last_scan_id,
                hv.file_hash,
                vv.first_scan_id,
                vv.val_state,
                vv.val_error,
                CASE
                    WHEN $1 = 0 THEN 0
                    WHEN $2 = 1 AND (hv.file_hash IS NULL OR hv.last_scan_id < $3) THEN 1
                    WHEN hv.file_hash IS NULL THEN 1
                    WHEN cv.first_scan_id = $3 AND pv.version_id IS NULL THEN 1
                    WHEN cv.first_scan_id = $3 AND pv.is_deleted = 1 THEN 1
                    WHEN cv.first_scan_id = $3 AND (cv.mod_date IS NOT pv.mod_date OR cv.size IS NOT pv.size) THEN 1
                    ELSE 0
                END AS needs_hash,
                CASE
                    WHEN $4 = 0 THEN 0
                    WHEN i.has_validator = 0 THEN 0
                    WHEN $5 = 1 AND (vv.val_state IS NULL OR vv.last_scan_id < $3) THEN 1
                    WHEN vv.val_state IS NULL THEN 1
                    WHEN cv.first_scan_id = $3 AND pv.version_id IS NULL THEN 1
                    WHEN cv.first_scan_id = $3 AND pv.is_deleted = 1 THEN 1
                    WHEN cv.first_scan_id = $3 AND (cv.mod_date IS NOT pv.mod_date OR cv.size IS NOT pv.size) THEN 1
                    ELSE 0
                END AS needs_val
            FROM items i
            JOIN item_versions cv
                ON cv.item_id = i.item_id
                AND cv.last_scan_id = $3
            LEFT JOIN item_versions pv
                ON pv.item_id = i.item_id
                AND pv.first_scan_id = (
                    SELECT MAX(first_scan_id)
                    FROM item_versions
                    WHERE item_id = i.item_id
                      AND first_scan_id < cv.first_scan_id
                )
            LEFT JOIN hash_versions hv
                ON hv.item_id = i.item_id
                AND hv.first_scan_id = (
                    SELECT MAX(first_scan_id) FROM hash_versions WHERE item_id = i.item_id
                )
            LEFT JOIN val_versions vv
                ON vv.item_id = i.item_id
                AND vv.first_scan_id = (
                    SELECT MAX(first_scan_id) FROM val_versions WHERE item_id = i.item_id
                )
            WHERE
                i.item_type = 0
                AND cv.is_deleted = 0
                AND cv.access <> 1
                AND i.item_id > $6
                AND (
                    ($1 = 1 AND (
                        ($2 = 1 AND (hv.file_hash IS NULL OR hv.last_scan_id < $3)) OR
                        hv.file_hash IS NULL OR
                        (cv.first_scan_id = $3 AND pv.version_id IS NULL AND (hv.last_scan_id IS NULL OR hv.last_scan_id < $3)) OR
                        (cv.first_scan_id = $3 AND pv.is_deleted = 1 AND (hv.last_scan_id IS NULL OR hv.last_scan_id < $3)) OR
                        (cv.first_scan_id = $3 AND (cv.mod_date IS NOT pv.mod_date OR cv.size IS NOT pv.size) AND (hv.last_scan_id IS NULL OR hv.last_scan_id < $3))
                    )) OR
                    ($4 = 1 AND i.has_validator = 1 AND (
                        ($5 = 1 AND (vv.val_state IS NULL OR vv.last_scan_id < $3)) OR
                        vv.val_state IS NULL OR
                        (cv.first_scan_id = $3 AND pv.version_id IS NULL AND (vv.last_scan_id IS NULL OR vv.last_scan_id < $3)) OR
                        (cv.first_scan_id = $3 AND pv.is_deleted = 1 AND (vv.last_scan_id IS NULL OR vv.last_scan_id < $3)) OR
                        (cv.first_scan_id = $3 AND (cv.mod_date IS NOT pv.mod_date OR cv.size IS NOT pv.size) AND (vv.last_scan_id IS NULL OR vv.last_scan_id < $3))
                    ))
                )
            ORDER BY i.item_id ASC
            LIMIT {limit}"
        );

        let mut stmt = conn.prepare(&query)?;

        let rows = stmt.query_map(
            [
                analysis_spec.is_hash() as i64,
                analysis_spec.hash_all() as i64,
                scan_id,
                analysis_spec.is_val() as i64,
                analysis_spec.val_all() as i64,
                last_item_id,
            ],
            AnalysisItem::from_row,
        )?;

        let analysis_items: Vec<AnalysisItem> = rows.collect::<Result<Vec<_>, _>>()?;

        Ok(analysis_items)
    }
}

/// Error types from validation analysis
pub enum ValAnalysisError {
    PermissionDenied,
    NotFound,
    NoValidator,
    ValidationError(String),
}

/// Find the most recent completed scan before the current one for this root.
///
/// Used by Case B access versioning to restore `last_scan_id` on the pre-existing
/// version. Same query as `Scanner::query_prev_completed_scan`.
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

fn check_interrupted(interrupt_token: &Arc<AtomicBool>) -> Result<(), FsPulseError> {
    if interrupt_token.load(Ordering::Acquire) {
        Err(FsPulseError::TaskInterrupted)
    } else {
        Ok(())
    }
}

fn is_interrupted(interrupt_token: &Arc<AtomicBool>) -> bool {
    interrupt_token.load(Ordering::Acquire)
}

fn should_alert_access_denied(old_access: Access, new_access: Access) -> bool {
    new_access != Access::Ok && old_access == Access::Ok
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_analysis_item_getters() {
        let analysis_item = AnalysisItem {
            item_id: 123,
            item_path: "/test/path".to_string(),
            version_id: 999,
            version_first_scan_id: 50,
            access: Access::Ok.as_i64(),
            mod_date: Some(1000),
            size: Some(2000),
            has_validator: true,
            hash_first_scan_id: Some(456),
            hash_last_scan_id: Some(460),
            file_hash: Some("abc123".to_string()),
            val_first_scan_id: Some(789),
            val_state: Some(ValidationState::Valid.as_i64()),
            val_error: Some("test error".to_string()),
            needs_hash: true,
            needs_val: false,
        };

        assert_eq!(analysis_item.item_id(), 123);
        assert_eq!(analysis_item.item_path(), "/test/path");
        assert_eq!(analysis_item.access(), Access::Ok);
        assert!(analysis_item.has_validator());
        assert_eq!(analysis_item.hash_first_scan_id(), Some(456));
        assert_eq!(analysis_item.file_hash(), Some("abc123"));
        assert_eq!(analysis_item.val_first_scan_id(), Some(789));
        assert_eq!(analysis_item.val_error(), Some("test error"));
        assert!(analysis_item.needs_hash());
        assert!(!analysis_item.needs_val());
    }
}
