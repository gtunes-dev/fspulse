use crate::database::Database;
use crate::error::FsPulseError;
use crate::roots::Root;

use rusqlite::{params, OptionalExtension, Result};

use std::fmt;

const SQL_SCAN_ID_OR_LATEST: &str =
    "SELECT scan_id, root_id, state, is_hash, hash_all, is_val, val_all, scan_time, file_count, folder_count, total_file_size, alert_count, add_count, modify_count, delete_count, error
        FROM scans
        WHERE scan_id = IFNULL(?1, (SELECT MAX(scan_id) FROM scans))";

const SQL_LATEST_FOR_ROOT: &str =
    "SELECT scan_id, root_id, state, is_hash, hash_all, is_val, val_all, scan_time, file_count, folder_count, total_file_size, alert_count, add_count, modify_count, delete_count, error
        FROM scans
        WHERE root_id = ?
        ORDER BY scan_id DESC LIMIT 1";


#[derive(Copy, Clone, Debug, PartialEq)]
pub enum HashMode {
    None,
    New,
    All
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub enum ValidateMode {
    None,
    New,
    All
}

#[derive(Copy, Clone, Debug)]
pub struct AnalysisSpec {
    hash_mode: HashMode,
    val_mode: ValidateMode,
}

impl AnalysisSpec {
    pub fn new(no_hash: bool, hash_new: bool, no_val: bool, val_all: bool) -> Self {
        AnalysisSpec {
            hash_mode: match (no_hash, hash_new) {
                (true, false) => HashMode::None,
                (false, true) => HashMode::New,
                _ => HashMode::All,
            },
            val_mode: match (no_val, val_all) {
                (true, false) => ValidateMode::None,
                (false, true) => ValidateMode::All,
                _ => ValidateMode::New,
            },
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

    pub fn hash_mode(&self) -> HashMode {
        self.hash_mode
    }

    pub fn val_mode(&self) -> ValidateMode {
        self.val_mode
    }
}

#[derive(Clone, Debug)]
pub struct Scan {
    // Schema fields
    scan_id: i64,
    root_id: i64,
    state: ScanState,
    analysis_spec: AnalysisSpec,
    #[allow(dead_code)]
    scan_time: i64,
    file_count: Option<i64>,
    folder_count: Option<i64>,
    total_file_size: Option<i64>,
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
        state: i64,
        analysis_spec: AnalysisSpec,
        scan_time: i64,
    ) -> Self {
        Scan {
            scan_id,
            root_id,
            state: ScanState::from_i64(state),
            analysis_spec,
            scan_time,
            file_count: None,
            folder_count: None,
            total_file_size: None,
            alert_count: None,
            add_count: None,
            modify_count: None,
            delete_count: None,
            error: None,
        }
    }

    pub fn create(
        db: &Database,
        root: &Root,
        analysis_spec: &AnalysisSpec,
    ) -> Result<Self, FsPulseError> {
        let (scan_id, scan_time): (i64, i64) = db.conn().query_row(
            "INSERT INTO scans (root_id, state, is_hash, hash_all, is_val, val_all, scan_time) 
             VALUES (?, ?, ?, ?, ?, ?, strftime('%s', 'now', 'utc')) 
             RETURNING scan_id, scan_time",
            [
                root.root_id(),
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
            ScanState::Scanning.as_i64(),
            *analysis_spec,
            scan_time,
        );
        Ok(scan)
    }

    pub fn get_latest(db: &Database) -> Result<Option<Self>, FsPulseError> {
        Self::get_by_id_or_latest(db, None, None)
    }

    /*
    pub fn get_by_id(db: &Database, scan_id: i64) -> Result<Option<Self>, FsPulseError> {
        Self::get_by_id_or_latest(db, Some(scan_id), None)
    }
    */

    pub fn get_latest_for_root(db: &Database, root_id: i64) -> Result<Option<Self>, FsPulseError> {
        Self::get_by_id_or_latest(db, None, Some(root_id))
    }

    fn get_by_id_or_latest(
        db: &Database,
        scan_id: Option<i64>,
        root_id: Option<i64>,
    ) -> Result<Option<Self>, FsPulseError> {
        let conn = db.conn();

        let (query, query_param) = match (scan_id, root_id) {
            (Some(_), _) => (SQL_SCAN_ID_OR_LATEST, scan_id),
            (_, Some(_)) => (SQL_LATEST_FOR_ROOT, root_id),
            _ => (SQL_SCAN_ID_OR_LATEST, None),
        };

        // If the scan id wasn't explicitly specified, load the most recent otherwise,
        // load the specified scan
        let scan_row: Option<Scan> = conn
            .query_row(query, params![query_param], |row| {
                let is_hash = row.get(3)?;
                let hash_all = row.get(4)?;
                let hash_mode = match (is_hash, hash_all) {
                    (false, _) => HashMode::None,
                    (_, true) => HashMode::All,
                    _ => HashMode::New,
                };

                let is_val = row.get(5)?;
                let val_all = row.get(6)?;

                let val_mode = match (is_val, val_all) {
                    (false, _) => ValidateMode::None,
                    (_, true) => ValidateMode::All,
                    _ => ValidateMode::New,
                };

                Ok(Scan {
                    scan_id: row.get(0)?,
                    root_id: row.get(1)?,
                    state: ScanState::from_i64(row.get(2)?),
                    analysis_spec: AnalysisSpec {
                        hash_mode,
                        val_mode,
                    },
                    scan_time: row.get(7)?,
                    file_count: row.get(8)?,
                    folder_count: row.get(9)?,
                    total_file_size: row.get(10)?,
                    alert_count: row.get(11)?,
                    add_count: row.get(12)?,
                    modify_count: row.get(13)?,
                    delete_count: row.get(14)?,
                    error: row.get(15)?,
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

    pub fn scan_time(&self) -> i64 {
        self.scan_time
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

    pub fn total_file_size(&self) -> Option<i64> {
        self.total_file_size
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

    pub fn set_state_sweeping(&mut self, db: &mut Database) -> Result<(), FsPulseError> {
        match self.state() {
            ScanState::Scanning => self.set_state(db, ScanState::Sweeping),
            _ => Err(FsPulseError::Error(format!(
                "Can't set Scan Id {} to state sweeping from state {}",
                self.scan_id(),
                self.state().as_i64()
            ))),
        }
    }

    pub fn set_state_analyzing(&mut self, db: &mut Database) -> Result<(), FsPulseError> {
        match self.state() {
            ScanState::Sweeping => self.set_state(db, ScanState::Analyzing),
            _ => Err(FsPulseError::Error(format!(
                "Can't set Scan Id {} to state analyzing from state {}",
                self.scan_id(),
                self.state().as_i64()
            ))),
        }
    }

    pub fn set_state_completed(&mut self, db: &mut Database) -> Result<(), FsPulseError> {
        match self.state() {
            ScanState::Analyzing => {
                let tx = db.conn_mut().transaction()?;

                // Compute file_count and folder_count (exclude tombstones)
                let (file_count, folder_count): (i64, i64) = tx
                    .query_row(
                        "SELECT
                        SUM(CASE WHEN item_type = 0 THEN 1 ELSE 0 END) AS file_count,
                        SUM(CASE WHEN item_type = 1 THEN 1 ELSE 0 END) AS folder_count
                        FROM items WHERE last_scan = ? AND is_ts = 0",
                        [self.scan_id],
                        |row| Ok((row.get(0)?, row.get(1)?)),
                    )
                    .unwrap_or((0, 0));

                // Compute total_file_size
                let total_file_size: i64 = tx
                    .query_row(
                        "SELECT COALESCE(SUM(file_size), 0) FROM items
                         WHERE last_scan = ? AND item_type = 0 AND is_ts = 0",
                        [self.scan_id],
                        |row| row.get(0),
                    )
                    .unwrap_or(0);

                // Compute alert_count
                let alert_count: i64 = tx
                    .query_row(
                        "SELECT COUNT(*) FROM alerts WHERE scan_id = ?",
                        [self.scan_id],
                        |row| row.get(0),
                    )
                    .unwrap_or(0);

                // Compute add_count, modify_count, delete_count
                let (add_count, modify_count, delete_count): (i64, i64, i64) = tx
                    .query_row(
                        "SELECT
                        COUNT(*) FILTER (WHERE change_type = 0),
                        COUNT(*) FILTER (WHERE change_type = 2),
                        COUNT(*) FILTER (WHERE change_type = 1)
                        FROM changes WHERE scan_id = ?",
                        [self.scan_id],
                        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                    )
                    .unwrap_or((0, 0, 0));

                // Update the scan with all counts and set state to Completed in one operation
                tx.execute(
                    "UPDATE scans SET
                        file_count = ?,
                        folder_count = ?,
                        total_file_size = ?,
                        alert_count = ?,
                        add_count = ?,
                        modify_count = ?,
                        delete_count = ?,
                        state = ?
                    WHERE scan_id = ?",
                    (
                        file_count,
                        folder_count,
                        total_file_size,
                        alert_count,
                        add_count,
                        modify_count,
                        delete_count,
                        ScanState::Completed.as_i64(),
                        self.scan_id,
                    ),
                )?;

                tx.commit()?;

                // Update in-memory struct
                self.state = ScanState::Completed;
                self.file_count = Some(file_count);
                self.folder_count = Some(folder_count);
                self.total_file_size = Some(total_file_size);
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

    pub fn set_state_stopped(&mut self, db: &mut Database) -> Result<(), FsPulseError> {
        match self.state() {
            ScanState::Scanning | ScanState::Sweeping | ScanState::Analyzing => {
                Scan::stop_scan(db, self, None)?;
                self.state = ScanState::Stopped;
                Ok(())
            }
            _ => Err(FsPulseError::Error(format!(
                "Can't stop scan - invalid state {}",
                self.state().as_i64()
            ))),
        }
    }

    fn set_state(&mut self, db: &mut Database, new_state: ScanState) -> Result<(), FsPulseError> {
        let conn = &mut db.conn_mut();

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

    pub fn stop_scan(db: &mut Database, scan: &Scan, error_message: Option<&str>) -> Result<(), FsPulseError> {
        let tx = db.conn_mut().transaction()?;

        // Find the id of the last scan on this root to not be stopped
        // We'll restore this scan_id to all partially updated by the
        // scan being stopped
        let prev_scan_id: i64 = tx.query_row(
            "SELECT COALESCE(
                (SELECT MAX(scan_id)
                 FROM scans
                 WHERE root_id = ?
                   AND scan_id < ?
                   AND state = 4
                ),
                0
            ) AS prev_scan_id",
            [scan.root_id(), scan.scan_id()],
            |row| row.get::<_, i64>(0),
        )?;

        // Undo Add (when they are reyhdrates) and Type Change
        // When an item was previously tombstoned and then was found again during a scan
        // the item is rehydrated. This means that is_ts is set to false and all properties
        // on the item are cleared and set to new values. In this case the cleared properties are
        // stored in the change record, and we can recover them from there. A type change is
        // handled similarly. When an item that was known to be a file or folder is next seen as the other
        // type, we clear the properties (and store them on the change record). So, in both cases, we can
        // now recover the modfiied properties from the change and this batch handles the minor differences
        // between the two operations
        tx.execute(
            "UPDATE items
            SET (
                is_ts,
                mod_date,
                file_size,
                last_hash_scan,
                file_hash,
                last_val_scan,
                val,
                val_error,
                last_scan
            ) =
            (
                SELECT 
                    CASE WHEN c.change_type = 0 THEN 1 ELSE items.is_ts END,
                    c.mod_date_old,
                    c.file_size_old,
                    c.last_hash_scan_old,
                    c.hash_old,
                    c.last_val_scan_old,
                    c.val_old,
                    c.val_error_old,
                    ?1
                FROM changes c
                WHERE c.item_id = items.item_id
                    AND c.scan_id = ?2
                    AND (c.change_type = 0 AND c.is_undelete = 1)
                LIMIT 1
            )
            WHERE item_id IN (
                SELECT item_id 
                FROM changes 
                WHERE scan_id = ?2
                    AND (change_type = 0 AND is_undelete = 1)
            )",
            [prev_scan_id, scan.scan_id()],
        )?;

        // Undoing a modify requires selectively copying back (from the change)
        // the property groups that were part of the modify
        tx.execute(
            "UPDATE items
            SET (
                mod_date, 
                file_size, 
                last_hash_scan, 
                file_hash,
                last_val_scan, 
                val, 
                val_error, 
                last_scan
            ) =
            (
            SELECT 
                CASE WHEN c.meta_change = 1 THEN COALESCE(c.mod_date_old, items.mod_date) ELSE items.mod_date END,
                CASE WHEN c.meta_change = 1 THEN COALESCE(c.file_size_old, items.file_size) ELSE items.file_size END,
                CASE WHEN c.hash_change = 1 THEN c.last_hash_scan_old ELSE items.last_hash_scan END,
                CASE WHEN c.hash_change = 1 THEN c.hash_old ELSE items.file_hash END,
                CASE WHEN c.val_change = 1 THEN c.last_val_scan_old ELSE items.last_val_scan END,
                CASE WHEN c.val_change = 1 THEN c.val_old ELSE items.val END,
                CASE WHEN c.val_change = 1 THEN c.val_error_old ELSE items.val_error END,
                ?1
            FROM changes c
            WHERE c.item_id = items.item_id 
                AND c.scan_id = ?2
                AND c.change_type = 2
            LIMIT 1
            )
            WHERE last_scan = ?2
            AND EXISTS (
                SELECT 1 FROM changes c 
                WHERE c.item_id = items.item_id 
                    AND c.scan_id = ?2
                    AND c.change_type = 2
            )", 
            [prev_scan_id, scan.scan_id()]
        )?;

        // Undo deletes. This is simple because deletes just set the tombstone flag
        tx.execute(
            "UPDATE items
            SET is_ts = 0,
                last_scan = ?1
            WHERE item_id IN (
                SELECT item_id
                FROM changes
                WHERE scan_id = ?2
                  AND change_type = 1
            )",
            [prev_scan_id, scan.scan_id()],
        )?;

        // Undo alerts. Delete all of the alerts created during the scan
        tx.execute(
            "DELETE FROM alerts
            WHERE scan_id = ?1",
            [scan.scan_id()],
        )?;

        // Mark the scan as stopped (state=5) or error (state=6)
        let final_state = if error_message.is_some() { 6 } else { 5 };

        tx.execute(
            "UPDATE scans SET state = ?, error = ? WHERE scan_id = ?",
            params![final_state, error_message, scan.scan_id()],
        )?;

        // Find the items that had their last_scan updated but where no change
        // record was created, and reset their last_scan
        tx.execute(
            "UPDATE items
             SET last_scan = ?1
             WHERE last_scan = ?2
               AND NOT EXISTS (
                 SELECT 1 FROM changes c
                 WHERE c.item_id = items.item_id
                   AND c.scan_id = ?2
               )",
            [prev_scan_id, scan.scan_id()],
        )?;

        // Delete the change records from the stopped scan
        tx.execute("DELETE FROM changes WHERE scan_id = ?1", [scan.scan_id()])?;

        // Final step is to delete the remaining items that were created during
        // the scan. We have to do this after the change records are deleted otherwise
        // attemping to delete these rows will generate a referential integrity violation
        // since we'll be abandoning change records. This operation assumes that the
        // only remaining items with a last_scan of the current scan are the simple
        // adds. This should be true :)
        tx.execute(
            "DELETE FROM items
        WHERE last_scan = ?",
            [scan.scan_id()],
        )?;

        tx.commit()?;

        Ok(())
    }

    // TODO: This was used for reports and isn't currently used but don't want to
    // commit to throwing it away yet
    #[allow(dead_code)]
    pub fn for_each_scan<F>(db: &Database, last: u32, mut func: F) -> Result<(), FsPulseError>
    where
        F: FnMut(&Database, &Scan) -> Result<(), FsPulseError>,
    {
        if last == 0 {
            return Ok(());
        }

        let mut stmt = db.conn().prepare(
            "SELECT
                s.scan_id,
                s.root_id,
                s.state,
                s.is_hash,
                s.hash_all,
                s.is_val,
                s.val_all,
                s.scan_time,
                s.file_count,
                s.folder_count,
                s.total_file_size,
                s.alert_count,
                s.add_count,
                s.modify_count,
                s.delete_count,
                s.error
            FROM scans s
            LEFT JOIN changes c ON s.scan_id = c.scan_id
            GROUP BY s.scan_id, s.root_id, s.state, s.is_hash, s.hash_all, s.is_val, s.val_all, s.scan_time, s.file_count, s.folder_count, s.total_file_size, s.alert_count, s.add_count, s.modify_count, s.delete_count, s.error
            ORDER BY s.scan_id DESC
            LIMIT ?"
        )?;

        let rows = stmt.query_map([last], |row| {
            let is_hash = row.get::<_, bool>(3)?;
            let hash_all = row.get::<_, bool>(4)?; // Hash or re-hash everything

            let hash_mode = match (is_hash, hash_all) {
                (_, true) => HashMode::All,
                (false, false) => HashMode::None,
                _ => HashMode::New
            };

            let is_val = row.get::<_, bool>(5)?;   // val new or changed;
            let val_all = row.get::<_, bool>(6)?;  // Val or  re-val everything
            let val_mode = match (is_val, val_all) {
                (_, true) => ValidateMode::All,
                (false, false) => ValidateMode::None,
                _ => ValidateMode::New,
            };

            Ok(Scan {
                scan_id: row.get::<_, i64>(0)?,
                root_id: row.get::<_, i64>(1)?,
                state: ScanState::from_i64(row.get::<_, i64>(2)?),
                analysis_spec: AnalysisSpec {
                    hash_mode,
                    val_mode,
                },
                scan_time: row.get::<_, i64>(7)?,
                file_count: row.get::<_, Option<i64>>(8)?,
                folder_count: row.get::<_, Option<i64>>(9)?,
                total_file_size: row.get::<_, Option<i64>>(10)?,
                alert_count: row.get::<_, Option<i64>>(11)?,
                add_count: row.get::<_, Option<i64>>(12)?,
                modify_count: row.get::<_, Option<i64>>(13)?,
                delete_count: row.get::<_, Option<i64>>(14)?,
                error: row.get::<_, Option<String>>(15)?,
            })
        })?;

        for row in rows {
            let scan = row?;
            func(db, &scan)?;
        }

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
    pub scan_time: i64,

    // Total counts from scans table
    pub total_files: i64,
    pub total_folders: i64,
    pub total_file_size: i64,

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
    pub fn get_for_scan(db: &Database, scan_id: i64) -> Result<Option<Self>, FsPulseError> {
        // Use existing function to get scan
        let scan = match Scan::get_by_id_or_latest(db, Some(scan_id), None)? {
            Some(s) => s,
            None => return Ok(None),
        };

        // Use existing function to get root path
        let root = crate::roots::Root::get_by_id(db, scan.root_id())?
            .ok_or_else(|| FsPulseError::Error(format!("Root {} not found", scan.root_id())))?;

        let conn = db.conn();

        // Get change statistics broken down by file vs folder
        let changes: (i64, i64, i64, i64, i64, i64) = conn.query_row(
            "SELECT
                SUM(CASE WHEN c.change_type = 0 AND i.item_type = 0 THEN 1 ELSE 0 END) as files_added,
                SUM(CASE WHEN c.change_type = 2 AND i.item_type = 0 THEN 1 ELSE 0 END) as files_modified,
                SUM(CASE WHEN c.change_type = 1 AND i.item_type = 0 THEN 1 ELSE 0 END) as files_deleted,
                SUM(CASE WHEN c.change_type = 0 AND i.item_type = 1 THEN 1 ELSE 0 END) as folders_added,
                SUM(CASE WHEN c.change_type = 2 AND i.item_type = 1 THEN 1 ELSE 0 END) as folders_modified,
                SUM(CASE WHEN c.change_type = 1 AND i.item_type = 1 THEN 1 ELSE 0 END) as folders_deleted
             FROM changes c
             JOIN items i ON c.item_id = i.item_id
             WHERE c.scan_id = ?",
            params![scan_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?, row.get(5)?))
        ).unwrap_or((0, 0, 0, 0, 0, 0));

        // Get hashing statistics
        let items_hashed: i64 = conn.query_row(
            "SELECT COUNT(*) FROM items
             WHERE last_scan = ? AND hash IS NOT NULL",
            params![scan_id],
            |row| row.get(0)
        ).unwrap_or(0);

        // Get validation statistics
        let items_validated: i64 = conn.query_row(
            "SELECT COUNT(*) FROM items
             WHERE last_scan = ? AND (val IS NOT NULL OR val_error IS NOT NULL)",
            params![scan_id],
            |row| row.get(0)
        ).unwrap_or(0);

        Ok(Some(ScanStats {
            scan_id: scan.scan_id(),
            root_id: scan.root_id(),
            root_path: root.root_path().to_string(),
            state: scan.state(),
            scan_time: scan.scan_time(),
            total_files: scan.file_count().unwrap_or(0),
            total_folders: scan.folder_count().unwrap_or(0),
            total_file_size: scan.total_file_size().unwrap_or(0),
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
    pub fn get_latest(db: &Database) -> Result<Option<Self>, FsPulseError> {
        // Use existing function with None to get latest scan
        let scan = match Scan::get_by_id_or_latest(db, None, None)? {
            Some(s) => s,
            None => return Ok(None),
        };

        Self::get_for_scan(db, scan.scan_id())
    }
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
    fn test_analysis_spec_new_no_hash_no_val() {
        let spec = AnalysisSpec::new(true, false, true, false);
        assert!(!spec.is_hash());
        assert!(!spec.hash_all());
        assert!(!spec.is_val());
        assert!(!spec.val_all());
    }

    #[test]
    fn test_analysis_spec_new_hash_new() {
        let spec = AnalysisSpec::new(false, true, false, false);
        assert!(spec.is_hash());
        assert!(!spec.hash_all());
        assert!(spec.is_val()); // defaults to new when not disabled
        assert!(!spec.val_all());
    }

    #[test]
    fn test_analysis_spec_new_hash_all() {
        let spec = AnalysisSpec::new(false, false, false, false);
        assert!(spec.is_hash()); // defaults to all when not disabled or new
        assert!(spec.hash_all());
        assert!(spec.is_val()); // defaults to new when not disabled
        assert!(!spec.val_all());
    }

    #[test]
    fn test_analysis_spec_new_val_all() {
        let spec = AnalysisSpec::new(false, false, false, true);
        assert!(spec.is_hash()); // defaults to all
        assert!(spec.hash_all());
        assert!(spec.is_val());
        assert!(spec.val_all());
    }

    #[test]
    fn test_analysis_spec_all_combinations() {
        // Test all 16 possible combinations of the 4 boolean flags
        let test_cases = [
            // (no_hash, hash_new, no_val, val_all) -> (expected is_hash, hash_all, is_val, val_all)
            ((true, false, true, false), (false, false, false, false)),   // 0000
            ((true, false, true, true), (false, false, true, false)),     // 0001 - no_val=true, val_all=true -> ValidateMode::New  
            ((true, false, false, false), (false, false, true, false)),   // 0010
            ((true, false, false, true), (false, false, true, true)),     // 0011
            ((true, true, true, false), (true, true, false, false)),      // 0100 - (true,true) -> HashMode::All
            ((true, true, true, true), (true, true, true, false)),        // 0101 - (true,true) -> HashMode::All, no_val=true, val_all=true -> ValidateMode::New
            ((true, true, false, false), (true, true, true, false)),      // 0110 - (true,true) -> HashMode::All
            ((true, true, false, true), (true, true, true, true)),        // 0111 - (true,true) -> HashMode::All
            ((false, false, true, false), (true, true, false, false)),    // 1000
            ((false, false, true, true), (true, true, true, false)),      // 1001 - no_val=true, val_all=true -> ValidateMode::New
            ((false, false, false, false), (true, true, true, false)),    // 1010
            ((false, false, false, true), (true, true, true, true)),      // 1011
            ((false, true, true, false), (true, false, false, false)),    // 1100
            ((false, true, true, true), (true, false, true, false)),      // 1101 - no_val=true, val_all=true -> ValidateMode::New
            ((false, true, false, false), (true, false, true, false)),    // 1110
            ((false, true, false, true), (true, false, true, true)),      // 1111
        ];

        for ((no_hash, hash_new, no_val, val_all), (exp_is_hash, exp_hash_all, exp_is_val, exp_val_all)) in test_cases {
            let spec = AnalysisSpec::new(no_hash, hash_new, no_val, val_all);
            assert_eq!(spec.is_hash(), exp_is_hash, 
                "is_hash failed for ({no_hash}, {hash_new}, {no_val}, {val_all})");
            assert_eq!(spec.hash_all(), exp_hash_all,
                "hash_all failed for ({no_hash}, {hash_new}, {no_val}, {val_all})");
            assert_eq!(spec.is_val(), exp_is_val,
                "is_val failed for ({no_hash}, {hash_new}, {no_val}, {val_all})");
            assert_eq!(spec.val_all(), exp_val_all,
                "val_all failed for ({no_hash}, {hash_new}, {no_val}, {val_all})");
        }
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

    #[test]
    fn test_analysis_spec_copy_clone() {
        let spec = AnalysisSpec::new(false, true, false, false);
        let spec_copy = spec;
        let spec_clone = spec;
        
        // All should have the same behavior
        assert_eq!(spec.is_hash(), spec_copy.is_hash());
        assert_eq!(spec.is_hash(), spec_clone.is_hash());
        assert_eq!(spec.hash_all(), spec_copy.hash_all());
        assert_eq!(spec.hash_all(), spec_clone.hash_all());
    }
}
