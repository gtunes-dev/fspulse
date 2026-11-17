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
use crate::utils::Utils;
use crate::validate::validator::ValidationState;

use super::hash_analysis;
use super::val_analysis;

/// Batch size for fetching items to analyze.
const FETCH_BATCH_SIZE: usize = 100;

/// Run the file analysis phase (Phase 1 of 2).
///
/// Fetches batches of items needing hash/val analysis and dispatches
/// them to worker threads. Each worker calls `analyze_item`.
///
/// Concurrent-watcher resilience:
/// - Batch fetching continues as long as the previous batch was
///   full-sized, regardless of the initial total count.
/// - When processed exceeds the cached total, the total is re-queried.
/// - Write-time guards in `persist_analysis` skip items whose version
///   has been bumped or tombstoned by the watcher since the batch was
///   fetched.
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

    // Compute disabled validation extensions once from config
    let disabled_exts = crate::validate::validator::disabled_extensions();

    // If the scan doesn't hash or validate, nothing to do
    if !is_hash && !is_val {
        return Ok(());
    }

    // Parse initial task state for restart resilience (HWM)
    let initial_state = ScanTaskState::from_task_state(initial_task_state.as_deref())?;
    let initial_hwm = initial_state.high_water_mark;

    let (mut analyze_total, analyze_done) = AnalysisItem::get_analysis_counts(
        &conn,
        scan.root_id(),
        scan.analysis_spec(),
        initial_hwm,
        &disabled_exts,
    )?;

    // Set up counter-based progress tracking
    task_progress.set_progress_total(analyze_total, analyze_done, Some("files"));

    // Create the analysis tracker for HWM management
    let tracker = Arc::new(AnalysisTracker::new(task_id, initial_state));

    // Bounded channel limits queued work
    let (sender, receiver) = bounded::<AnalysisItem>(FETCH_BATCH_SIZE);

    // Thread pool
    let items_remaining = analyze_total.saturating_sub(analyze_done);
    let items_remaining_usize = items_remaining.try_into().unwrap_or(usize::MAX);
    let thread_count = crate::config::Config::get_analysis_threads();
    let num_threads = cmp::min(items_remaining_usize, thread_count).max(1);
    let pool = ThreadPool::new(num_threads);

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
    let mut total_dispatched: u64 = analyze_done;

    loop {
        if interrupt_token.load(Ordering::Acquire) {
            break;
        }

        let analysis_items = AnalysisItem::fetch_next_batch(
            &conn,
            scan.root_id(),
            scan.analysis_spec(),
            last_item_id,
            FETCH_BATCH_SIZE,
            &disabled_exts,
        )?;

        let batch_len = analysis_items.len();

        if batch_len == 0 {
            break;
        }

        tracker.add_batch(analysis_items.iter().map(|item| item.item_id()));

        for analysis_item in analysis_items {
            last_item_id = analysis_item.item_id();
            sender
                .send(analysis_item)
                .expect("Failed to send task into the bounded channel");
        }

        total_dispatched += batch_len as u64;

        // If we've exceeded the cached total (items added by the
        // watcher since we started), re-query to refresh the progress
        // bar denominator.
        if total_dispatched > analyze_total {
            let (new_total, _) = AnalysisItem::get_analysis_counts(
                &conn,
                scan.root_id(),
                scan.analysis_spec(),
                initial_hwm,
                &disabled_exts,
            )?;
            analyze_total = new_total;
            task_progress.set_progress_total(analyze_total, total_dispatched, Some("files"));
        }

        // Continue fetching as long as we got a full batch. A partial
        // batch means we've reached the end of eligible items.
        if batch_len < FETCH_BATCH_SIZE {
            break;
        }
    }

    drop(sender);
    pool.join();

    check_interrupted(interrupt_token)?;

    tracker.warn_if_not_empty();

    // Force progress to 100% with matching counts
    task_progress.set_progress_total(total_dispatched, total_dispatched, Some("files"));

    task_progress.clear_thread_states();
    task_progress.add_breadcrumb("Analysis phase complete");

    Ok(())
}

/// Process a single item: compute hash/validate, determine state, persist.
///
/// Called by worker threads. Does not return errors — logs them instead.
fn analyze_item(
    scan: &Scan,
    mut analysis_item: AnalysisItem,
    thread_index: usize,
    task_progress: &Arc<TaskProgress>,
    interrupt_token: &Arc<AtomicBool>,
    tracker: &Arc<AnalysisTracker>,
) {
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
                    display_path
                );
                read_permission_denied = true;
            }
            Err(FsPulseError::IoError(ref io_err))
                if io_err.kind() == ErrorKind::NotFound =>
            {
                log::warn!(
                    "File not found during hashing '{}': skipping analysis",
                    display_path
                );
                file_not_found = true;
            }
            Err(error) => {
                error!("Error hashing '{}': {}", display_path, error);
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
        task_progress.set_thread_state(thread_index, "Validating", "info-alternate", Some(&display_path));
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
                    display_path
                );
                read_permission_denied = true;
            }
            Err(ValAnalysisError::NotFound) => {
                log::warn!(
                    "File not found during validation '{}': skipping analysis",
                    display_path
                );
                file_not_found = true;
            }
            Err(ValAnalysisError::NoValidator) => {
                log::warn!("NoValidator error for item that passed needs_val check: {:?}", display_path);
            }
            Err(ValAnalysisError::ValidationError(e_str)) => {
                read_attempted = true;
                error!("Error validating '{}': {}", display_path, e_str);
                new_val = ValidationState::Invalid;
                new_val_error = Some(e_str);
            }
        }
    }

    // If the file was not found, disable hash/val analysis — the item
    // was deleted between fetch and analysis. It will be picked up next scan.
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

    let persisted = if !is_interrupted(interrupt_token) {
        match persist_analysis(
            scan,
            &analysis_item,
            new_hash,
            new_val,
            new_val_error,
            new_access,
            interrupt_token,
        ) {
            Ok(outcome) => {
                if outcome != PersistOutcome::Applied {
                    info!(
                        "Analysis write skipped for '{}': {:?}",
                        display_path, outcome
                    );
                }
                outcome == PersistOutcome::Applied
            }
            Err(error) => {
                error!(
                    "Error updating item analysis '{}': {}",
                    display_path, error
                );
                false
            }
        }
    } else {
        false
    };

    task_progress.increment_progress();
    task_progress.set_thread_idle(thread_index);

    // Only advance the HWM if the item was successfully persisted.
    if persisted {
        let item_id = analysis_item.item_id();
        if let Err(e) = tracker.complete_item(item_id) {
            error!("Failed to complete item {}: {}", item_id, e);
        }
    }

    info!("Done analyzing: {path:?}");
}

/// Outcome of attempting to persist analysis results for one item.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PersistOutcome {
    /// Results were written to the database.
    Applied,
    /// Item's version was bumped by the watcher between fetch and persist.
    SkippedVersionBumped,
    /// Item was tombstoned by the watcher between fetch and persist.
    SkippedDeleted,
    /// File on disk changed between walk observation and analysis.
    SkippedFileChanged,
}

/// Persist hash and validation results to the database.
///
/// Includes two guards against concurrent watcher changes:
/// 1. **File guard** (pre-lock): checks that the file's mod_date and
///    size on disk still match what was observed at fetch time.
/// 2. **Version guard** (under lock): re-queries the item's current
///    version and confirms it hasn't been bumped or tombstoned.
#[allow(clippy::too_many_arguments)]
fn persist_analysis(
    _scan: &Scan,
    analysis_item: &AnalysisItem,
    new_hash: Option<String>,
    new_val: ValidationState,
    new_val_error: Option<String>,
    new_access: Option<Access>,
    interrupt_token: &Arc<AtomicBool>,
) -> Result<PersistOutcome, FsPulseError> {
    let hash_changed = analysis_item.needs_hash()
        && analysis_item.file_hash() != new_hash.as_deref();

    let mut access_changed = false;
    let mut new_access_value = analysis_item.access();

    if let Some(access) = new_access {
        if access != analysis_item.access() {
            access_changed = true;
            new_access_value = access;
        }
    }

    check_interrupted(interrupt_token)?;

    // Pre-lock file guard: check the file on disk hasn't changed
    if analysis_item.needs_hash() || analysis_item.needs_val() {
        let path = std::path::Path::new(analysis_item.item_path());
        if !super::file_guard::check_file_unchanged(
            path,
            analysis_item.mod_date(),
            analysis_item.size(),
        ) {
            return Ok(PersistOutcome::SkippedFileChanged);
        }
    }

    let conn = Database::get_connection()?;

    Database::immediate_transaction(&conn, |c| {
        // Write-time version guard: re-query the item's current state
        // under the lock. If the watcher has bumped the version or
        // tombstoned the item since we fetched this batch, skip.
        let current: Option<(i64, bool)> = c
            .query_row(
                "SELECT item_version, is_deleted
                 FROM item_versions
                 WHERE item_id = ?1
                 ORDER BY item_version DESC
                 LIMIT 1",
                params![analysis_item.item_id()],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;

        match current {
            Some((v, _)) if v != analysis_item.item_version() => {
                return Ok(PersistOutcome::SkippedVersionBumped);
            }
            Some((_, true)) => {
                return Ok(PersistOutcome::SkippedDeleted);
            }
            None => {
                return Ok(PersistOutcome::SkippedDeleted);
            }
            Some(_) => {} // Version matches, not deleted — proceed
        }

        // Hash persistence
        if analysis_item.needs_hash() {
            hash_analysis::persist_hash(
                c, analysis_item, new_hash.as_deref(), hash_changed,
            )?;
        }

        // Val persistence
        if analysis_item.needs_val() {
            val_analysis::persist_val(
                c, analysis_item, new_val, new_val_error.as_deref(),
            )?;
        }

        // Access change → insert a new item_version row (consistent
        // with how checkpoint and watcher handle access transitions).
        if access_changed {
            let now = Utils::now_secs();
            c.execute(
                "INSERT INTO item_versions (
                    item_id, item_version, root_id, parent_item_id, hierarchy_id,
                    first_seen_at, last_seen_at,
                    is_added, is_deleted, access, mod_date, size
                 )
                 SELECT
                    i.item_id,
                    ?1,
                    i.root_id,
                    i.parent_item_id,
                    i.hierarchy_id,
                    ?2, ?2,
                    0, 0, ?3,
                    iv.mod_date, iv.size
                 FROM items i
                 JOIN item_versions iv ON iv.item_id = i.item_id
                    AND iv.item_version = ?4
                 WHERE i.item_id = ?5",
                params![
                    analysis_item.item_version() + 1,
                    now,
                    new_access_value.as_i64(),
                    analysis_item.item_version(),
                    analysis_item.item_id(),
                ],
            )?;
        }

        Ok(PersistOutcome::Applied)
    })
}

/// An item ready for the analysis phase, with its current state and
/// flags indicating which analysis operations are needed.
#[derive(Clone, Debug)]
pub struct AnalysisItem {
    item_id: i64,
    item_path: String,
    item_version: i64,
    access: i64,
    mod_date: Option<i64>,
    size: Option<i64>,
    has_validator: bool,
    // From hash_versions (NULL if never hashed for this version)
    hash_first_seen_at: Option<i64>,
    file_hash: Option<String>,
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

    pub fn item_version(&self) -> i64 {
        self.item_version
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

    pub fn hash_first_seen_at(&self) -> Option<i64> {
        self.hash_first_seen_at
    }

    pub fn file_hash(&self) -> Option<&str> {
        self.file_hash.as_deref()
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
            item_version: row.get(2)?,
            access: row.get(3)?,
            mod_date: row.get(4)?,
            size: row.get(5)?,
            has_validator: row.get::<_, i64>(6)? != 0,
            hash_first_seen_at: row.get(7)?,
            file_hash: Hash::opt_blob_to_hex(row.get(8)?),
            needs_hash: row.get(9)?,
            needs_val: row.get(10)?,
        })
    }

    /// Build a SQL WHEN clause to exclude disabled extensions from needs_val.
    fn build_ext_exclusion_clause(disabled_exts: &[&str]) -> String {
        if disabled_exts.is_empty() {
            String::new()
        } else {
            let quoted: Vec<String> = disabled_exts.iter().map(|e| format!("'{}'", e)).collect();
            format!(
                "WHEN i.file_extension IN ({}) THEN 0\n                        ",
                quoted.join(", ")
            )
        }
    }

    /// Build a SQL AND clause to exclude disabled extensions from WHERE.
    fn build_ext_not_in_clause(disabled_exts: &[&str]) -> String {
        if disabled_exts.is_empty() {
            String::new()
        } else {
            let quoted: Vec<String> = disabled_exts.iter().map(|e| format!("'{}'", e)).collect();
            format!(
                "\n                        AND i.file_extension NOT IN ({})",
                quoted.join(", ")
            )
        }
    }

    /// Count items eligible for analysis (total needing work, and how
    /// many are already done). Used for progress bar initialization
    /// and overflow re-query.
    pub fn get_analysis_counts(
        conn: &Connection,
        root_id: i64,
        analysis_spec: &AnalysisSpec,
        last_item_id: i64,
        disabled_exts: &[&str],
    ) -> Result<(u64, u64), FsPulseError> {
        let ext_exclusion = Self::build_ext_exclusion_clause(disabled_exts);

        // Find the latest alive version per item for this root.
        // Then compute needs_hash / needs_val flags and count.
        let sql = format!(
            "WITH current_versions AS (
                SELECT iv.item_id, iv.item_version, iv.access, iv.val_state
                FROM item_versions iv
                WHERE iv.root_id = ?1
                  AND iv.item_id > ?2
                  AND iv.is_deleted = 0
                  AND iv.item_version = (
                      SELECT MAX(iv2.item_version) FROM item_versions iv2
                      WHERE iv2.item_id = iv.item_id
                  )
            ),
            candidates AS (
                SELECT
                    cv.item_id,
                    CASE
                        WHEN ?3 = 0 THEN 0
                        WHEN ?4 = 1 THEN 1
                        WHEN NOT EXISTS (
                            SELECT 1 FROM hash_versions hv
                            WHERE hv.item_id = cv.item_id AND hv.item_version = cv.item_version
                        ) THEN 1
                        ELSE 0
                    END AS needs_hash,
                    CASE
                        WHEN ?5 = 0 THEN 0
                        WHEN i.has_validator = 0 THEN 0
                        WHEN i.do_not_validate = 1 THEN 0
                        {ext_exclusion}WHEN cv.val_state IS NULL THEN 1
                        ELSE 0
                    END AS needs_val,
                    cv.val_state
                FROM current_versions cv
                JOIN items i ON i.item_id = cv.item_id
                WHERE i.item_type = 0
                  AND cv.access <> 1
            )
            SELECT
                COALESCE(SUM(CASE WHEN needs_hash = 1 OR needs_val = 1 THEN 1 ELSE 0 END), 0),
                COALESCE(SUM(CASE
                    WHEN needs_hash = 0 AND needs_val = 0 THEN 0
                    WHEN (needs_hash = 1 AND EXISTS (
                        SELECT 1 FROM hash_versions hv2
                        WHERE hv2.item_id = candidates.item_id
                    ))
                    OR (needs_val = 1 AND val_state IS NOT NULL)
                    THEN 1 ELSE 0 END), 0)
            FROM candidates"
        );

        let mut stmt = conn.prepare_cached(&sql)?;
        let mut rows = stmt.query(params![
            root_id,
            last_item_id,
            analysis_spec.is_hash() as i64,
            analysis_spec.hash_all() as i64,
            analysis_spec.is_val() as i64,
        ])?;

        if let Some(row) = rows.next()? {
            let total_needed = row.get::<_, i64>(0)? as u64;
            let total_done = row.get::<_, i64>(1)? as u64;
            Ok((total_needed, total_done))
        } else {
            Ok((0, 0))
        }
    }

    /// Fetch the next batch of items eligible for analysis, ordered
    /// by item_id for cursor-based pagination.
    pub fn fetch_next_batch(
        conn: &Connection,
        root_id: i64,
        analysis_spec: &AnalysisSpec,
        last_item_id: i64,
        limit: usize,
        disabled_exts: &[&str],
    ) -> Result<Vec<AnalysisItem>, FsPulseError> {
        let ext_exclusion = Self::build_ext_exclusion_clause(disabled_exts);
        let ext_not_in = Self::build_ext_not_in_clause(disabled_exts);

        let query = format!(
            "SELECT
                iv.item_id,
                i.item_path,
                iv.item_version,
                iv.access,
                iv.mod_date,
                iv.size,
                i.has_validator,
                hv.first_seen_at,
                hv.file_hash,
                CASE
                    WHEN ?1 = 0 THEN 0
                    WHEN ?2 = 1 THEN 1
                    WHEN hv.file_hash IS NULL THEN 1
                    ELSE 0
                END AS needs_hash,
                CASE
                    WHEN ?3 = 0 THEN 0
                    WHEN i.has_validator = 0 THEN 0
                    WHEN i.do_not_validate = 1 THEN 0
                    {ext_exclusion}WHEN iv.val_state IS NULL THEN 1
                    ELSE 0
                END AS needs_val
            FROM item_versions iv
            JOIN items i ON i.item_id = iv.item_id
            LEFT JOIN hash_versions hv ON hv.item_id = iv.item_id
                AND hv.item_version = iv.item_version
                AND hv.first_seen_at = (
                    SELECT MAX(hv2.first_seen_at) FROM hash_versions hv2
                    WHERE hv2.item_id = iv.item_id AND hv2.item_version = iv.item_version
                )
            WHERE iv.root_id = ?4
              AND iv.item_id > ?5
              AND i.item_type = 0
              AND iv.is_deleted = 0
              AND iv.access <> 1
              AND iv.item_version = (
                  SELECT MAX(iv2.item_version) FROM item_versions iv2
                  WHERE iv2.item_id = iv.item_id
              )
              AND (
                  (?1 = 1 AND (
                      hv.file_hash IS NULL
                      OR ?2 = 1
                  ))
                  OR
                  (?3 = 1 AND i.has_validator = 1 AND i.do_not_validate = 0 AND iv.val_state IS NULL{ext_not_in})
              )
            ORDER BY iv.item_id ASC
            LIMIT {limit}"
        );

        let mut stmt = conn.prepare(&query)?;

        let rows = stmt.query_map(
            params![
                analysis_spec.is_hash() as i64,
                analysis_spec.hash_all() as i64,
                analysis_spec.is_val() as i64,
                root_id,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_analysis_item_getters() {
        let analysis_item = AnalysisItem {
            item_id: 123,
            item_path: "/test/path".to_string(),
            item_version: 3,
            access: Access::Ok.as_i64(),
            mod_date: Some(1000),
            size: Some(2000),
            has_validator: true,
            hash_first_seen_at: Some(1700000000),
            file_hash: Some("abc123".to_string()),
            needs_hash: true,
            needs_val: false,
        };

        assert_eq!(analysis_item.item_id(), 123);
        assert_eq!(analysis_item.item_path(), "/test/path");
        assert_eq!(analysis_item.access(), Access::Ok);
        assert!(analysis_item.has_validator());
        assert_eq!(analysis_item.hash_first_seen_at(), Some(1700000000));
        assert_eq!(analysis_item.file_hash(), Some("abc123"));
        assert!(analysis_item.needs_hash());
        assert!(!analysis_item.needs_val());
    }
}
