use crate::database::Database;
use crate::error::FsPulseError;
use crate::roots::Root;
use crate::undo_log::UndoLog;

use chrono::{Local, NaiveDate, TimeZone, Utc};
use rusqlite::{params, Connection, OptionalExtension, Result};
use serde::Serialize;

use std::fmt;

const SQL_SCAN_ID_OR_LATEST: &str =
    "SELECT scan_id, root_id, schedule_id, started_at, ended_at, was_restarted, state, is_hash, hash_all, is_val, val_all, file_count, folder_count, total_size, alert_count, add_count, modify_count, delete_count, error
        FROM scans
        WHERE scan_id = IFNULL(?1, (SELECT MAX(scan_id) FROM scans))";

const SQL_LATEST_FOR_ROOT: &str =
    "SELECT scan_id, root_id, schedule_id, started_at, ended_at, was_restarted, state, is_hash, hash_all, is_val, val_all, file_count, folder_count, total_size, alert_count, add_count, modify_count, delete_count, error
        FROM scans
        WHERE root_id = ?
        ORDER BY scan_id DESC LIMIT 1";

#[derive(Copy, Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[repr(i32)]
pub enum HashMode {
    None = 0,
    New = 1,
    All = 2,
}

impl HashMode {
    pub fn from_i32(value: i32) -> Option<Self> {
        match value {
            0 => Some(Self::None),
            1 => Some(Self::New),
            2 => Some(Self::All),
            _ => None,
        }
    }

    pub fn as_i32(self) -> i32 {
        self as i32
    }
}

#[derive(Copy, Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[repr(i32)]
pub enum ValidateMode {
    None = 0,
    New = 1,
    All = 2,
}

impl ValidateMode {
    pub fn from_i32(value: i32) -> Option<Self> {
        match value {
            0 => Some(Self::None),
            1 => Some(Self::New),
            2 => Some(Self::All),
            _ => None,
        }
    }

    pub fn as_i32(self) -> i32 {
        self as i32
    }
}

#[derive(Copy, Clone, Debug)]
pub struct AnalysisSpec {
    hash_mode: HashMode,
    val_mode: ValidateMode,
}

impl AnalysisSpec {
    pub fn from_modes(hash_mode: HashMode, val_mode: ValidateMode) -> Self {
        AnalysisSpec {
            hash_mode,
            val_mode,
        }
    }

    pub fn is_hash(&self) -> bool {
        self.hash_mode != HashMode::None
    }

    pub fn hash_all(&self) -> bool {
        self.hash_mode == HashMode::All
    }

    pub fn is_val(&self) -> bool {
        self.val_mode != ValidateMode::None
    }

    pub fn val_all(&self) -> bool {
        self.val_mode == ValidateMode::All
    }
}

#[derive(Clone, Debug)]
pub struct Scan {
    // Schema fields
    scan_id: i64,
    root_id: i64,
    #[allow(dead_code)]
    schedule_id: Option<i64>,
    #[allow(dead_code)]
    started_at: i64,
    #[allow(dead_code)]
    ended_at: Option<i64>,
    #[allow(dead_code)]
    was_restarted: bool,
    state: ScanState,
    analysis_spec: AnalysisSpec,
    file_count: Option<i64>,
    folder_count: Option<i64>,
    total_size: Option<i64>,
    alert_count: Option<i64>,
    add_count: Option<i64>,
    modify_count: Option<i64>,
    delete_count: Option<i64>,
    error: Option<String>,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
#[repr(i64)] // Ensures explicit numeric representation
pub enum ScanState {
    Scanning = 1,
    Sweeping = 2,
    Analyzing = 3,
    Completed = 4,
    Stopped = 5,
    Error = 6,
}

impl ScanState {
    pub fn from_i64(value: i64) -> Self {
        match value {
            1 => ScanState::Scanning,
            2 => ScanState::Sweeping,
            3 => ScanState::Analyzing,
            4 => ScanState::Completed,
            5 => ScanState::Stopped,
            6 => ScanState::Error,
            _ => panic!("Invalid ScanState value: {}", value),
        }
    }

    pub fn as_i64(&self) -> i64 {
        *self as i64
    }

    pub fn short_name(&self) -> &'static str {
        match self {
            ScanState::Scanning => "S",
            ScanState::Sweeping => "W",
            ScanState::Analyzing => "A",
            ScanState::Completed => "C",
            ScanState::Stopped => "P",
            ScanState::Error => "E",
        }
    }

    pub fn full_name(&self) -> &'static str {
        match self {
            ScanState::Scanning => "Scanning",
            ScanState::Sweeping => "Sweeping",
            ScanState::Analyzing => "Analyzing",
            ScanState::Completed => "Completed",
            ScanState::Stopped => "Stopped",
            ScanState::Error => "Error",
        }
    }

    pub fn from_string(s: &str) -> Option<Self> {
        // Try to match against full name or short name (case-insensitive)
        match s.to_ascii_uppercase().as_str() {
            // Full names
            "SCANNING" => Some(ScanState::Scanning),
            "SWEEPING" => Some(ScanState::Sweeping),
            "ANALYZING" => Some(ScanState::Analyzing),
            "COMPLETED" => Some(ScanState::Completed),
            "STOPPED" => Some(ScanState::Stopped),
            "ERROR" => Some(ScanState::Error),
            // Short names
            "S" => Some(ScanState::Scanning),
            "W" => Some(ScanState::Sweeping),
            "A" => Some(ScanState::Analyzing),
            "C" => Some(ScanState::Completed),
            "P" => Some(ScanState::Stopped),
            "E" => Some(ScanState::Error),
            _ => None,
        }
    }
}

impl fmt::Display for ScanState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.full_name())
    }
}

impl crate::query::QueryEnum for ScanState {
    fn from_token(s: &str) -> Option<i64> {
        Self::from_string(s).map(|state| state.as_i64())
    }
}

impl Scan {
    // Create a Scan that will be used during a directory scan
    // In this case, the scan_id is not yet known
    fn new_for_scan(
        scan_id: i64,
        root_id: i64,
        schedule_id: Option<i64>,
        started_at: i64,
        state: i64,
        analysis_spec: AnalysisSpec,
    ) -> Self {
        Scan {
            scan_id,
            root_id,
            schedule_id,
            started_at,
            ended_at: None,
            was_restarted: false,
            state: ScanState::from_i64(state),
            analysis_spec,
            file_count: None,
            folder_count: None,
            total_size: None,
            alert_count: None,
            add_count: None,
            modify_count: None,
            delete_count: None,
            error: None,
        }
    }

    pub fn create(
        conn: &rusqlite::Connection,
        root: &Root,
        schedule_id: Option<i64>,
        analysis_spec: &AnalysisSpec,
    ) -> Result<Self, FsPulseError> {
        let (scan_id, started_at): (i64, i64) = conn.query_row(
            "INSERT INTO scans (root_id, schedule_id, state, is_hash, hash_all, is_val, val_all, started_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, strftime('%s', 'now', 'utc'))
             RETURNING scan_id, started_at",
            params![
                root.root_id(),
                schedule_id,
                ScanState::Scanning.as_i64(),
                analysis_spec.is_hash() as i64,
                analysis_spec.hash_all() as i64,
                analysis_spec.is_val() as i64,
                analysis_spec.val_all() as i64,
            ],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;

        let scan = Scan::new_for_scan(
            scan_id,
            root.root_id(),
            schedule_id,
            started_at,
            ScanState::Scanning.as_i64(),
            *analysis_spec,
        );
        Ok(scan)
    }

    pub fn get_latest_for_root(root_id: i64) -> Result<Option<Self>, FsPulseError> {
        let conn = Database::get_connection()?;
        Self::get_by_id_or_latest(&conn, None, Some(root_id))
    }

    pub fn get_by_id_or_latest(
        conn: &rusqlite::Connection,
        scan_id: Option<i64>,
        root_id: Option<i64>,
    ) -> Result<Option<Self>, FsPulseError> {
        let (query, query_param) = match (scan_id, root_id) {
            (Some(_), _) => (SQL_SCAN_ID_OR_LATEST, scan_id),
            (_, Some(_)) => (SQL_LATEST_FOR_ROOT, root_id),
            _ => (SQL_SCAN_ID_OR_LATEST, None),
        };

        // If the scan id wasn't explicitly specified, load the most recent otherwise,
        // load the specified scan
        let scan_row: Option<Scan> = conn
            .query_row(query, params![query_param], |row| {
                let is_hash = row.get(7)?;
                let hash_all = row.get(8)?;
                let hash_mode = match (is_hash, hash_all) {
                    (false, _) => HashMode::None,
                    (_, true) => HashMode::All,
                    _ => HashMode::New,
                };

                let is_val = row.get(9)?;
                let val_all = row.get(10)?;

                let val_mode = match (is_val, val_all) {
                    (false, _) => ValidateMode::None,
                    (_, true) => ValidateMode::All,
                    _ => ValidateMode::New,
                };

                Ok(Scan {
                    scan_id: row.get(0)?,
                    root_id: row.get(1)?,
                    schedule_id: row.get(2)?,
                    started_at: row.get(3)?,
                    ended_at: row.get(4)?,
                    was_restarted: row.get(5)?,
                    state: ScanState::from_i64(row.get(6)?),
                    analysis_spec: AnalysisSpec {
                        hash_mode,
                        val_mode,
                    },
                    file_count: row.get(11)?,
                    folder_count: row.get(12)?,
                    total_size: row.get(13)?,
                    alert_count: row.get(14)?,
                    add_count: row.get(15)?,
                    modify_count: row.get(16)?,
                    delete_count: row.get(17)?,
                    error: row.get(18)?,
                })
            })
            .optional()?;

        Ok(scan_row)
    }

    pub fn scan_id(&self) -> i64 {
        self.scan_id
    }

    pub fn root_id(&self) -> i64 {
        self.root_id
    }

    pub fn state(&self) -> ScanState {
        self.state
    }

    pub fn analysis_spec(&self) -> &AnalysisSpec {
        &self.analysis_spec
    }

    pub fn started_at(&self) -> i64 {
        self.started_at
    }

    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    pub fn file_count(&self) -> Option<i64> {
        self.file_count
    }

    pub fn folder_count(&self) -> Option<i64> {
        self.folder_count
    }

    pub fn total_size(&self) -> Option<i64> {
        self.total_size
    }

    pub fn alert_count(&self) -> Option<i64> {
        self.alert_count
    }

    pub fn add_count(&self) -> Option<i64> {
        self.add_count
    }

    pub fn modify_count(&self) -> Option<i64> {
        self.modify_count
    }

    pub fn delete_count(&self) -> Option<i64> {
        self.delete_count
    }

    /// Resolve a date to the most recent completed scan for a root at or before that date.
    /// If `date_str` is None, returns the most recent completed scan for the root.
    /// Returns (scan_id, started_at) or None if no matching scan exists.
    pub fn resolve_scan_for_date(
        root_id: i64,
        date_str: Option<&str>,
    ) -> Result<Option<(i64, i64)>, FsPulseError> {
        let conn = Database::get_connection()?;

        match date_str {
            Some(date) => {
                // Get end-of-day timestamp for the given date
                let (_start_ts, end_ts) = crate::utils::Utils::single_date_bounds(date)?;

                let result: Option<(i64, i64)> = conn
                    .query_row(
                        "SELECT scan_id, started_at FROM scans
                         WHERE root_id = ? AND state = 4 AND started_at <= ?
                         ORDER BY started_at DESC LIMIT 1",
                        params![root_id, end_ts],
                        |row| Ok((row.get(0)?, row.get(1)?)),
                    )
                    .optional()?;

                Ok(result)
            }
            None => {
                let result: Option<(i64, i64)> = conn
                    .query_row(
                        "SELECT scan_id, started_at FROM scans
                         WHERE root_id = ? AND state = 4
                         ORDER BY scan_id DESC LIMIT 1",
                        params![root_id],
                        |row| Ok((row.get(0)?, row.get(1)?)),
                    )
                    .optional()?;

                Ok(result)
            }
        }
    }

    pub fn set_total_size(
        &mut self,
        conn: &Connection,
        total_size: i64,
    ) -> Result<(), FsPulseError> {
        let rows_updated = conn.execute(
            "UPDATE scans SET total_size = ? WHERE scan_id = ?",
            [total_size, self.scan_id],
        )?;

        if rows_updated == 0 {
            return Err(FsPulseError::Error(format!(
                "Could not update the total_size of Scan Id {} to {}",
                self.scan_id, total_size
            )));
        }

        self.total_size = Some(total_size);

        Ok(())
    }

    pub fn set_state_sweeping(&mut self, conn: &Connection) -> Result<(), FsPulseError> {
        match self.state() {
            ScanState::Scanning => self.set_state(conn, ScanState::Sweeping),
            _ => Err(FsPulseError::Error(format!(
                "Can't set Scan Id {} to state sweeping from state {}",
                self.scan_id(),
                self.state().as_i64()
            ))),
        }
    }

    pub fn set_state_analyzing(&mut self, conn: &Connection) -> Result<(), FsPulseError> {
        match self.state() {
            ScanState::Sweeping => self.set_state(conn, ScanState::Analyzing),
            _ => Err(FsPulseError::Error(format!(
                "Can't set Scan Id {} to state analyzing from state {}",
                self.scan_id(),
                self.state().as_i64()
            ))),
        }
    }

    pub fn set_state_completed(&mut self, conn: &Connection) -> Result<(), FsPulseError> {
        match self.state() {
            ScanState::Analyzing => {
                // Use IMMEDIATE transaction for read-then-write pattern
                let (file_count, folder_count, alert_count, add_count, modify_count, delete_count) =
                    Database::immediate_transaction(conn, |c| {
                        // Compute file_count and folder_count from current versions (exclude deleted)
                        let (file_count, folder_count): (i64, i64) = c
                            .query_row(
                                "SELECT
                                    COALESCE(SUM(CASE WHEN i.item_type = 0 THEN 1 ELSE 0 END), 0),
                                    COALESCE(SUM(CASE WHEN i.item_type = 1 THEN 1 ELSE 0 END), 0)
                                 FROM items i
                                 JOIN item_versions iv ON iv.item_id = i.item_id
                                 WHERE i.root_id = ? AND iv.last_scan_id = ? AND iv.is_deleted = 0",
                                params![self.root_id, self.scan_id],
                                |row| Ok((row.get(0)?, row.get(1)?)),
                            )
                            .unwrap_or((0, 0));

                        // Compute alert_count
                        let alert_count: i64 = c
                            .query_row(
                                "SELECT COUNT(*) FROM alerts WHERE scan_id = ?",
                                [self.scan_id],
                                |row| row.get(0),
                            )
                            .unwrap_or(0);

                        // Compute add_count, modify_count, delete_count from versions created this scan
                        let (add_count, modify_count, delete_count): (i64, i64, i64) = c
                            .query_row(
                                "SELECT
                                    COALESCE(COUNT(*) FILTER (WHERE iv.is_deleted = 0
                                        AND (pv.version_id IS NULL OR pv.is_deleted = 1)), 0),
                                    COALESCE(COUNT(*) FILTER (WHERE iv.is_deleted = 0
                                        AND pv.version_id IS NOT NULL AND pv.is_deleted = 0), 0),
                                    COALESCE(COUNT(*) FILTER (WHERE iv.is_deleted = 1
                                        AND pv.version_id IS NOT NULL AND pv.is_deleted = 0), 0)
                                 FROM item_versions iv
                                 LEFT JOIN item_versions pv ON pv.item_id = iv.item_id
                                     AND pv.first_scan_id = (
                                         SELECT MAX(first_scan_id) FROM item_versions
                                         WHERE item_id = iv.item_id AND first_scan_id < iv.first_scan_id
                                     )
                                 WHERE iv.first_scan_id = ?",
                                [self.scan_id],
                                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                            )
                            .unwrap_or((0, 0, 0));

                        // Update the scan with all counts and set state to Completed in one operation
                        c.execute(
                            "UPDATE scans SET
                                file_count = ?,
                                folder_count = ?,
                                alert_count = ?,
                                add_count = ?,
                                modify_count = ?,
                                delete_count = ?,
                                state = ?,
                                ended_at = strftime('%s', 'now', 'utc')
                            WHERE scan_id = ?",
                            (
                                file_count,
                                folder_count,
                                alert_count,
                                add_count,
                                modify_count,
                                delete_count,
                                ScanState::Completed.as_i64(),
                                self.scan_id,
                            ),
                        )?;

                        // Clear the undo log atomically with the state transition.
                        // Must be in this transaction so a crash can't leave a completed
                        // scan with a stale undo log.
                        UndoLog::clear(c)?;

                        Ok((
                            file_count,
                            folder_count,
                            alert_count,
                            add_count,
                            modify_count,
                            delete_count,
                        ))
                    })?;

                // Update in-memory struct
                self.state = ScanState::Completed;
                self.file_count = Some(file_count);
                self.folder_count = Some(folder_count);
                self.alert_count = Some(alert_count);
                self.add_count = Some(add_count);
                self.modify_count = Some(modify_count);
                self.delete_count = Some(delete_count);

                Ok(())
            }
            _ => Err(FsPulseError::Error(format!(
                "Can't set Scan Id {} to state completed from state {}",
                self.scan_id(),
                self.state().as_i64()
            ))),
        }
    }

    pub fn set_state_stopped(&mut self, conn: &Connection) -> Result<(), FsPulseError> {
        match self.state() {
            ScanState::Scanning | ScanState::Sweeping | ScanState::Analyzing => {
                Scan::stop_scan(conn, self, None)?;
                self.state = ScanState::Stopped;
                Ok(())
            }
            _ => Err(FsPulseError::Error(format!(
                "Can't stop scan - invalid state {}",
                self.state().as_i64()
            ))),
        }
    }

    fn set_state(&mut self, conn: &Connection, new_state: ScanState) -> Result<(), FsPulseError> {
        let rows_updated = conn.execute(
            "UPDATE scans SET state = ? WHERE scan_id = ?",
            [new_state.as_i64(), self.scan_id],
        )?;

        if rows_updated == 0 {
            return Err(FsPulseError::Error(format!(
                "Could not update the state of Scan Id {} to {}",
                self.scan_id,
                new_state.as_i64()
            )));
        }

        self.state = new_state;

        Ok(())
    }

    pub fn stop_scan(
        conn: &Connection,
        scan: &Scan,
        error_message: Option<&str>,
    ) -> Result<(), FsPulseError> {
        Database::immediate_transaction(conn, |c| {
            // Roll back item_versions, orphaned identities, and undo log
            UndoLog::rollback(c, scan.scan_id())?;

            // Delete all alerts created during the scan
            c.execute(
                "DELETE FROM alerts WHERE scan_id = ?",
                [scan.scan_id()],
            )?;

            // Mark the scan as stopped (state=5) or error (state=6)
            // Null the total_size that may have been set at the end of the scanning phase
            let final_state = if error_message.is_some() { 6 } else { 5 };

            c.execute(
                "UPDATE scans SET state = ?, total_size = NULL, error = ? WHERE scan_id = ?",
                params![final_state, error_message, scan.scan_id()],
            )?;

            Ok(())
        })?;

        Ok(())
    }
}

/// Statistics for a completed or in-progress scan
#[derive(Debug, Clone)]
pub struct ScanStats {
    pub scan_id: i64,
    pub root_id: i64,
    pub root_path: String,
    pub state: ScanState,
    pub started_at: i64,

    // Total counts from scans table
    pub total_files: i64,
    pub total_folders: i64,
    pub total_size: i64,

    // Total change counts from scans table
    pub total_adds: i64,
    pub total_modifies: i64,
    pub total_deletes: i64,

    // Change breakdown by type (files)
    pub files_added: i64,
    pub files_modified: i64,
    pub files_deleted: i64,

    // Change breakdown by type (folders)
    pub folders_added: i64,
    pub folders_modified: i64,
    pub folders_deleted: i64,

    // Analysis statistics
    pub items_hashed: i64,
    pub items_validated: i64,
    pub alerts_generated: i64,

    // Scan configuration
    pub hash_enabled: bool,
    pub validation_enabled: bool,

    // Error information
    pub error: Option<String>,
}

impl ScanStats {
    /// Get statistics for a specific scan ID
    pub fn get_for_scan(conn: &Connection, scan_id: i64) -> Result<Option<Self>, FsPulseError> {
        // Use existing function to get scan
        let scan = match Scan::get_by_id_or_latest(conn, Some(scan_id), None)? {
            Some(s) => s,
            None => return Ok(None),
        };

        // Use existing function to get root path
        let root = crate::roots::Root::get_by_id(conn, scan.root_id())?
            .ok_or_else(|| FsPulseError::Error(format!("Root {} not found", scan.root_id())))?;

        // Get change statistics broken down by file vs folder from temporal model.
        // Versions with first_scan_id = scan_id are new versions created in this scan.
        // By comparing with the previous version we classify as add/modify/delete.
        let changes: (i64, i64, i64, i64, i64, i64) = conn.query_row(
            "SELECT
                COALESCE(COUNT(*) FILTER (WHERE i.item_type = 0 AND iv.is_deleted = 0
                    AND (pv.version_id IS NULL OR pv.is_deleted = 1)), 0),
                COALESCE(COUNT(*) FILTER (WHERE i.item_type = 0 AND iv.is_deleted = 0
                    AND pv.version_id IS NOT NULL AND pv.is_deleted = 0), 0),
                COALESCE(COUNT(*) FILTER (WHERE i.item_type = 0 AND iv.is_deleted = 1
                    AND pv.version_id IS NOT NULL AND pv.is_deleted = 0), 0),
                COALESCE(COUNT(*) FILTER (WHERE i.item_type = 1 AND iv.is_deleted = 0
                    AND (pv.version_id IS NULL OR pv.is_deleted = 1)), 0),
                COALESCE(COUNT(*) FILTER (WHERE i.item_type = 1 AND iv.is_deleted = 0
                    AND pv.version_id IS NOT NULL AND pv.is_deleted = 0), 0),
                COALESCE(COUNT(*) FILTER (WHERE i.item_type = 1 AND iv.is_deleted = 1
                    AND pv.version_id IS NOT NULL AND pv.is_deleted = 0), 0)
             FROM item_versions iv
             JOIN items i ON i.item_id = iv.item_id
             LEFT JOIN item_versions pv ON pv.item_id = iv.item_id
                 AND pv.first_scan_id = (
                     SELECT MAX(first_scan_id) FROM item_versions
                     WHERE item_id = iv.item_id AND first_scan_id < iv.first_scan_id
                 )
             WHERE iv.first_scan_id = ?",
            params![scan_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?, row.get(5)?))
        ).unwrap_or((0, 0, 0, 0, 0, 0));

        // Get hashing statistics from temporal model
        let items_hashed: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM item_versions
                 WHERE last_hash_scan = ? AND file_hash IS NOT NULL",
                params![scan_id],
                |row| row.get(0),
            )
            .unwrap_or(0);

        // Get validation statistics from temporal model
        let items_validated: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM item_versions
                 WHERE last_val_scan = ?",
                params![scan_id],
                |row| row.get(0),
            )
            .unwrap_or(0);

        Ok(Some(ScanStats {
            scan_id: scan.scan_id(),
            root_id: scan.root_id(),
            root_path: root.root_path().to_string(),
            state: scan.state(),
            started_at: scan.started_at(),
            total_files: scan.file_count().unwrap_or(0),
            total_folders: scan.folder_count().unwrap_or(0),
            total_size: scan.total_size().unwrap_or(0),
            total_adds: scan.add_count().unwrap_or(0),
            total_modifies: scan.modify_count().unwrap_or(0),
            total_deletes: scan.delete_count().unwrap_or(0),
            files_added: changes.0,
            files_modified: changes.1,
            files_deleted: changes.2,
            folders_added: changes.3,
            folders_modified: changes.4,
            folders_deleted: changes.5,
            items_hashed,
            items_validated,
            alerts_generated: scan.alert_count().unwrap_or(0),
            hash_enabled: scan.analysis_spec().is_hash(),
            validation_enabled: scan.analysis_spec().is_val(),
            error: scan.error().map(|s| s.to_string()),
        }))
    }

    /// Get statistics for the most recent scan across all roots
    pub fn get_latest(conn: &Connection) -> Result<Option<Self>, FsPulseError> {
        // Use existing function with None to get latest scan
        let scan = match Scan::get_by_id_or_latest(conn, None, None)? {
            Some(s) => s,
            None => return Ok(None),
        };

        Self::get_for_scan(conn, scan.scan_id())
    }
}

/// Compact scan summary for the ScanPicker component
#[derive(Debug, Clone, Serialize)]
pub struct ScanSummary {
    pub scan_id: i64,
    pub started_at: i64,
    pub add_count: Option<i64>,
    pub modify_count: Option<i64>,
    pub delete_count: Option<i64>,
}

/// Get the distinct local-date strings (YYYY-MM-DD) that have completed scans
/// for a given root within a calendar month.
pub fn get_scan_dates_for_month(
    conn: &Connection,
    root_id: i64,
    year: i32,
    month: u32,
) -> Result<Vec<String>, FsPulseError> {
    use chrono::Datelike;

    let first_day = NaiveDate::from_ymd_opt(year, month, 1).ok_or_else(|| {
        FsPulseError::Error(format!("Invalid year/month: {year}-{month}"))
    })?;
    let last_day = if month == 12 {
        NaiveDate::from_ymd_opt(year + 1, 1, 1)
    } else {
        NaiveDate::from_ymd_opt(year, month + 1, 1)
    }
    .unwrap()
    .pred_opt()
    .unwrap();

    let first_str = first_day.format("%Y-%m-%d").to_string();
    let last_str = last_day.format("%Y-%m-%d").to_string();
    let (start_ts, end_ts) = crate::utils::Utils::range_date_bounds(&first_str, &last_str)?;

    let mut stmt = conn.prepare(
        "SELECT started_at FROM scans
         WHERE root_id = ? AND state = 4 AND started_at >= ? AND started_at <= ?
         ORDER BY started_at",
    )?;

    let timestamps: Vec<i64> = stmt
        .query_map(params![root_id, start_ts, end_ts], |row| row.get(0))?
        .filter_map(|r| r.ok())
        .collect();

    // Convert UTC timestamps to local dates and deduplicate
    let mut dates: Vec<String> = Vec::new();
    for ts in timestamps {
        let dt = Utc
            .timestamp_opt(ts, 0)
            .single()
            .ok_or_else(|| FsPulseError::Error(format!("Invalid timestamp: {ts}")))?;
        let local = dt.with_timezone(&Local);
        let date_str = format!(
            "{:04}-{:02}-{:02}",
            local.year(),
            local.month(),
            local.day()
        );
        if dates.last() != Some(&date_str) {
            dates.push(date_str);
        }
    }

    Ok(dates)
}

/// Get all completed scans for a root on a specific local date, most recent first.
pub fn get_scans_for_date(
    conn: &Connection,
    root_id: i64,
    date_str: &str,
) -> Result<Vec<ScanSummary>, FsPulseError> {
    let (start_ts, end_ts) = crate::utils::Utils::single_date_bounds(date_str)?;

    let mut stmt = conn.prepare(
        "SELECT scan_id, started_at, add_count, modify_count, delete_count
         FROM scans
         WHERE root_id = ? AND state = 4 AND started_at >= ? AND started_at <= ?
         ORDER BY started_at DESC",
    )?;

    let rows = stmt.query_map(params![root_id, start_ts, end_ts], |row| {
        Ok(ScanSummary {
            scan_id: row.get(0)?,
            started_at: row.get(1)?,
            add_count: row.get(2)?,
            modify_count: row.get(3)?,
            delete_count: row.get(4)?,
        })
    })?;

    let mut result = Vec::new();
    for row in rows {
        result.push(row?);
    }
    Ok(result)
}

/// Scan history row for the scan history table
#[derive(Debug, Clone, Serialize)]
pub struct ScanHistoryRow {
    pub scan_id: i64,
    pub root_id: i64,
    pub started_at: i64,
    pub ended_at: Option<i64>,
    pub was_restarted: bool,
    pub schedule_id: Option<i64>,
    pub schedule_name: Option<String>,
    pub add_count: Option<i64>,
    pub modify_count: Option<i64>,
    pub delete_count: Option<i64>,
    pub state: i64,
}

/// Get count of scan history entries (completed, stopped, or error states)
/// Optionally filtered by root_id
pub fn get_scan_history_count(
    conn: &Connection,
    root_id: Option<i64>,
) -> Result<i64, FsPulseError> {
    let count: i64 = if let Some(root_id) = root_id {
        conn.query_row(
            "SELECT COUNT(*) FROM scans
             WHERE state IN (4, 5, 6) AND root_id = ?",
            [root_id],
            |row| row.get(0),
        )?
    } else {
        conn.query_row(
            "SELECT COUNT(*) FROM scans
             WHERE state IN (4, 5, 6)",
            [],
            |row| row.get(0),
        )?
    };

    Ok(count)
}

/// Get paginated scan history with schedule information
/// Only includes completed, stopped, or error states
/// Optionally filtered by root_id
/// Ordered by scan_id DESC
pub fn get_scan_history(
    conn: &Connection,
    root_id: Option<i64>,
    limit: i64,
    offset: i64,
) -> Result<Vec<ScanHistoryRow>, FsPulseError> {
    let mut result = Vec::new();

    if let Some(root_id) = root_id {
        let mut stmt = conn.prepare(
            "SELECT
                s.scan_id,
                s.root_id,
                s.started_at,
                s.ended_at,
                s.was_restarted,
                s.schedule_id,
                sch.schedule_name,
                s.add_count,
                s.modify_count,
                s.delete_count,
                s.state
            FROM scans s
            LEFT JOIN scan_schedules sch ON s.schedule_id = sch.schedule_id
            WHERE s.state IN (4, 5, 6) AND s.root_id = ?
            ORDER BY s.scan_id DESC
            LIMIT ? OFFSET ?",
        )?;

        let rows = stmt.query_map(params![root_id, limit, offset], |row| {
            Ok(ScanHistoryRow {
                scan_id: row.get(0)?,
                root_id: row.get(1)?,
                started_at: row.get(2)?,
                ended_at: row.get(3)?,
                was_restarted: row.get(4)?,
                schedule_id: row.get(5)?,
                schedule_name: row.get(6)?,
                add_count: row.get(7)?,
                modify_count: row.get(8)?,
                delete_count: row.get(9)?,
                state: row.get(10)?,
            })
        })?;

        for row in rows {
            result.push(row?);
        }
    } else {
        let mut stmt = conn.prepare(
            "SELECT
                s.scan_id,
                s.root_id,
                s.started_at,
                s.ended_at,
                s.was_restarted,
                s.schedule_id,
                sch.schedule_name,
                s.add_count,
                s.modify_count,
                s.delete_count,
                s.state
            FROM scans s
            LEFT JOIN scan_schedules sch ON s.schedule_id = sch.schedule_id
            WHERE s.state IN (4, 5, 6)
            ORDER BY s.scan_id DESC
            LIMIT ? OFFSET ?",
        )?;

        let rows = stmt.query_map(params![limit, offset], |row| {
            Ok(ScanHistoryRow {
                scan_id: row.get(0)?,
                root_id: row.get(1)?,
                started_at: row.get(2)?,
                ended_at: row.get(3)?,
                was_restarted: row.get(4)?,
                schedule_id: row.get(5)?,
                schedule_name: row.get(6)?,
                add_count: row.get(7)?,
                modify_count: row.get(8)?,
                delete_count: row.get(9)?,
                state: row.get(10)?,
            })
        })?;

        for row in rows {
            result.push(row?);
        }
    }

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scan_state_as_i64() {
        assert_eq!(ScanState::Scanning.as_i64(), 1);
        assert_eq!(ScanState::Sweeping.as_i64(), 2);
        assert_eq!(ScanState::Analyzing.as_i64(), 3);
        assert_eq!(ScanState::Completed.as_i64(), 4);
        assert_eq!(ScanState::Stopped.as_i64(), 5);
        assert_eq!(ScanState::Error.as_i64(), 6);
    }

    #[test]
    fn test_scan_state_from_i64() {
        assert_eq!(ScanState::from_i64(1), ScanState::Scanning);
        assert_eq!(ScanState::from_i64(2), ScanState::Sweeping);
        assert_eq!(ScanState::from_i64(3), ScanState::Analyzing);
        assert_eq!(ScanState::from_i64(4), ScanState::Completed);
        assert_eq!(ScanState::from_i64(5), ScanState::Stopped);
        assert_eq!(ScanState::from_i64(6), ScanState::Error);
    }

    #[test]
    fn test_scan_state_round_trip() {
        let states = [
            ScanState::Scanning,
            ScanState::Sweeping,
            ScanState::Analyzing,
            ScanState::Completed,
            ScanState::Stopped,
            ScanState::Error,
        ];

        for state in states {
            let i64_val = state.as_i64();
            let converted_back = ScanState::from_i64(i64_val);
            assert_eq!(state, converted_back, "Round trip failed for {state:?}");
        }
    }

    #[test]
    fn test_scan_state_display() {
        assert_eq!(ScanState::Scanning.to_string(), "Scanning");
        assert_eq!(ScanState::Sweeping.to_string(), "Sweeping");
        assert_eq!(ScanState::Analyzing.to_string(), "Analyzing");
        assert_eq!(ScanState::Completed.to_string(), "Completed");
        assert_eq!(ScanState::Stopped.to_string(), "Stopped");
        assert_eq!(ScanState::Error.to_string(), "Error");
    }

    #[test]
    fn test_scan_state_ordering() {
        // Test that enum ordering works as expected
        assert!(ScanState::Scanning < ScanState::Sweeping);
        assert!(ScanState::Sweeping < ScanState::Analyzing);
        assert!(ScanState::Analyzing < ScanState::Completed);
        assert!(ScanState::Completed < ScanState::Stopped);
        assert!(ScanState::Stopped < ScanState::Error);
    }

    #[test]
    fn test_hash_mode_enum() {
        // Test that HashMode enum has expected variants
        let _none = HashMode::None;
        let _new = HashMode::New;
        let _all = HashMode::All;

        // Test PartialEq
        assert_eq!(HashMode::None, HashMode::None);
        assert_ne!(HashMode::None, HashMode::New);
        assert_ne!(HashMode::New, HashMode::All);
    }

    #[test]
    fn test_validate_mode_enum() {
        // Test that ValidateMode enum has expected variants
        let _none = ValidateMode::None;
        let _new = ValidateMode::New;
        let _all = ValidateMode::All;

        // Test PartialEq
        assert_eq!(ValidateMode::None, ValidateMode::None);
        assert_ne!(ValidateMode::None, ValidateMode::New);
        assert_ne!(ValidateMode::New, ValidateMode::All);
    }
}
