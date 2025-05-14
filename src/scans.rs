use crate::database::Database;
use crate::error::FsPulseError;
use crate::roots::Root;

use rusqlite::{params, OptionalExtension, Result, Transaction};

use std::fmt;

const SQL_SCAN_ID_OR_LATEST: &str =
    "SELECT scan_id, root_id, state, is_hash, hash_all, is_val, val_all, scan_time, file_count, folder_count
        FROM scans
        WHERE scan_id = IFNULL(?1, (SELECT MAX(scan_id) FROM scans))";

const SQL_LATEST_FOR_ROOT: &str =
    "SELECT scan_id, root_id, state, is_hash, hash_all, is_val, val_all, scan_time, file_count, folder_count
        FROM scans
        WHERE root_id = ?
        ORDER BY scan_id DESC LIMIT 1";

#[derive(Copy, Clone, Debug)]
pub struct AnalysisSpec {
    is_hash: bool,
    hash_all: bool,
    is_val: bool,
    val_all: bool,
}

impl AnalysisSpec {
    pub fn new(is_hash: bool, hash_all: bool, is_val: bool, val_all: bool) -> Self {
        AnalysisSpec {
            is_hash,
            hash_all,
            is_val,
            val_all,
        }
    }
    pub fn is_hash(&self) -> bool {
        self.is_hash
    }

    pub fn hash_all(&self) -> bool {
        self.hash_all
    }

    pub fn is_val(&self) -> bool {
        self.is_val
    }

    pub fn val_all(&self) -> bool {
        self.val_all
    }
}

#[derive(Copy, Clone, Debug)]
pub struct Scan {
    // Schema fields
    scan_id: i64,
    root_id: i64,
    state: i64,
    analysis_spec: AnalysisSpec,
    #[allow(dead_code)]
    scan_time: i64,
    file_count: Option<i64>,
    folder_count: Option<i64>,
}

#[derive(Debug, Default, Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
#[repr(i64)] // Ensures explicit numeric representation
pub enum ScanState {
    #[default]
    Pending = 0,
    Scanning = 1,
    Sweeping = 2,
    Analyzing = 3,
    Alerting = 4,
    Stopped = 50,
    Completed = 100,
    Unknown = -1,
}

impl ScanState {
    pub fn from_i64(value: i64) -> Self {
        match value {
            1 => ScanState::Scanning,
            2 => ScanState::Sweeping,
            3 => ScanState::Analyzing,
            4 => ScanState::Alerting,
            50 => ScanState::Stopped,
            100 => ScanState::Completed,
            _ => ScanState::Unknown, // Handle unknown states
        }
    }

    pub fn as_i64(&self) -> i64 {
        *self as i64
    }
}

impl fmt::Display for ScanState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            ScanState::Pending => "Pending",
            ScanState::Scanning => "Scanning",
            ScanState::Sweeping => "Sweeping",
            ScanState::Analyzing => "Analyzing",
            ScanState::Alerting => "Alerting",
            ScanState::Completed => "Completed",
            ScanState::Stopped => "Stopped",
            ScanState::Unknown => "Unknown",
        };
        write!(f, "{}", name)
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
            state,
            analysis_spec,
            scan_time,
            file_count: None,
            folder_count: None,
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
                analysis_spec.is_hash as i64,
                analysis_spec.hash_all as i64,
                analysis_spec.is_val as i64,
                analysis_spec.val_all as i64,
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
                Ok(Scan {
                    scan_id: row.get(0)?,
                    root_id: row.get(1)?,
                    state: row.get(2)?,
                    analysis_spec: AnalysisSpec {
                        is_hash: row.get(3)?,
                        hash_all: row.get(4)?,
                        is_val: row.get(5)?,
                        val_all: row.get(6)?,
                    },
                    scan_time: row.get(7)?,
                    file_count: row.get(8)?,
                    folder_count: row.get(9)?,
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
        ScanState::from_i64(self.state)
    }

    pub fn analysis_spec(&self) -> &AnalysisSpec {
        &self.analysis_spec
    }

    /*
    pub fn scan_time(&self) -> i64 {
        self.scan_time
    }
    */

    pub fn file_count(&self) -> Option<i64> {
        self.file_count
    }

    pub fn folder_count(&self) -> Option<i64> {
        self.folder_count
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
        let tx = db.conn_mut().transaction()?;

        let (file_count, folder_count): (i64, i64) = tx
            .query_row(
                "SELECT 
                SUM(CASE WHEN item_type = 'F' THEN 1 ELSE 0 END) AS file_count, 
                SUM(CASE WHEN item_type = 'D' THEN 1 ELSE 0 END) AS folder_count 
                FROM items WHERE last_scan = ?",
                [self.scan_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap_or((0, 0)); // If no data, default to 0

        // Update the scan entity to indicate that it completed
        tx.execute(
            "UPDATE scans SET file_count = ?, folder_count = ?, state = ? WHERE scan_id = ?",
            (
                file_count,
                folder_count,
                ScanState::Analyzing.as_i64(),
                self.scan_id,
            ),
        )?;

        tx.commit()?;

        self.state = ScanState::Analyzing.as_i64();

        self.file_count = Some(file_count);
        self.folder_count = Some(folder_count);

        Ok(())
    }

   pub fn set_state_alerting(&mut self, tx: Transaction) -> Result<(), FsPulseError> {
        match self.state() {
            ScanState::Analyzing => self.set_state_and_commit_with_tx(tx, ScanState::Alerting),
            _ => Err(FsPulseError::Error(format!(
                "Can't set Scan Id {} to state alerting from state {}",
                self.scan_id(),
                self.state().as_i64()
            ))),
        }
    }


    pub fn set_state_completed(&mut self, db: &mut Database) -> Result<(), FsPulseError> {
        match self.state() {
            ScanState::Alerting => self.set_state(db, ScanState::Completed),
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
                Scan::stop_scan(db, self)?;
                self.state = ScanState::Stopped.as_i64();
                Ok(())
            }
            _ => Err(FsPulseError::Error(format!(
                "Can't stop scan - invalid state {}",
                self.state().as_i64()
            ))),
        }
    }

    fn set_state_and_commit_with_tx(&mut self, tx: Transaction, new_state: ScanState) -> Result<(), FsPulseError> {
        let rows_updated = tx.execute(
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

        tx.commit()?;

        self.state = new_state.as_i64();

        Ok(())
    }

    fn set_state(&mut self, db: &mut Database, new_state: ScanState) -> Result<(), FsPulseError> {
        let tx = db.conn_mut().transaction()?;

        self.set_state_and_commit_with_tx(tx, new_state)
    }

    pub fn stop_scan(db: &mut Database, scan: &Scan) -> Result<(), FsPulseError> {
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
                    CASE WHEN c.change_type = 'A' THEN 1 ELSE items.is_ts END,
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
                    AND (c.change_type = 'A' AND c.is_undelete = 1)
                LIMIT 1
            )
            WHERE item_id IN (
                SELECT item_id 
                FROM changes 
                WHERE scan_id = ?2
                    AND (change_type = 'A' AND is_undelete = 1)
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
                AND c.change_type = 'M'
            LIMIT 1
            )
            WHERE last_scan = ?2
            AND EXISTS (
                SELECT 1 FROM changes c 
                WHERE c.item_id = items.item_id 
                    AND c.scan_id = ?2
                    AND c.change_type = 'M'
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
                  AND change_type = 'D'
            )",
            [prev_scan_id, scan.scan_id()],
        )?;

        // Mark the scan as stopped
        tx.execute(
            "UPDATE scans
                SET state = 5
                WHERE scan_id = ?1",
            [scan.scan_id()],
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
                s.folder_count
            FROM scans s
            LEFT JOIN changes c ON s.scan_id = c.scan_id
            GROUP BY s.scan_id, s.root_id, s.state, s.is_hash, s.hash_all, s.is_val, s.val_all, s.scan_time, s.file_count, s.folder_count
            ORDER BY s.scan_id DESC
            LIMIT ?"
        )?;

        let rows = stmt.query_map([last], |row| {
            Ok(Scan {
                scan_id: row.get::<_, i64>(0)?, // scan id
                root_id: row.get::<_, i64>(1)?, // root id
                state: row.get::<_, i64>(2)?,   // root id
                analysis_spec: AnalysisSpec {
                    is_hash: row.get::<_, bool>(3)?,  // hash new or changed
                    hash_all: row.get::<_, bool>(4)?, // Hash or re-hash everything
                    is_val: row.get::<_, bool>(5)?,   // val new or changed
                    val_all: row.get::<_, bool>(6)?,  // Val or  re-val everything
                },
                scan_time: row.get::<_, i64>(7)?, // time of scan
                file_count: row.get::<_, Option<i64>>(8)?, // file count
                folder_count: row.get::<_, Option<i64>>(9)?, // folder count
            })
        })?;

        for row in rows {
            let scan = row?;
            func(db, &scan)?;
        }

        Ok(())
    }
}
