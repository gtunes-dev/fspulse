use crate::error::FsPulseError;
use crate::database::Database;
use crate::roots::Root;

use rusqlite::{ OptionalExtension, Result, params };

use std::fmt;

const SQL_SCAN_ID_OR_LATEST: &str = 
    "SELECT id, root_id, state, hashing, validating, time_of_scan, file_count, folder_count
        FROM scans
        WHERE id = IFNULL(?1, (SELECT MAX(id) FROM scans))";

const SQL_LATEST_FOR_ROOT: &str = 
    "SELECT id, root_id, state, hashing, validating, time_of_scan, file_count, folder_count
        FROM scans
        WHERE root_id = ?
        ORDER BY id DESC LIMIT 1";

#[derive(Copy, Clone, Debug, Default)]
pub struct Scan {
    // Schema fields
    id: i64,
    root_id: i64,
    state: i64,
    hashing: bool,
    validating: bool,
    time_of_scan: i64,
    file_count: Option<i64>,
    folder_count: Option<i64>,
}

#[derive(Debug, Default, Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
#[repr(i64)]  // Ensures explicit numeric representation
pub enum ScanState {
    #[default]
    Pending = 0,
    Scanning = 1,
    Sweeping = 2,
    Analyzing = 3,
    Completed = 4,
    Stopped = 5,
    Unknown = -1,
}

impl ScanState {
    pub fn from_i64(value: i64) -> Self {
        match value {
            1 => ScanState::Scanning,
            2 => ScanState::Sweeping,
            3 => ScanState::Analyzing,
            4 => ScanState::Completed,
            5 => ScanState::Stopped,
            _ => ScanState::Unknown,  // Handle unknown states
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
    fn new_for_scan(id: i64, root_id: i64, state: i64, hashing: bool, validating: bool, time_of_scan: i64) -> Self {
        Scan {
            id,
            root_id,
            state,
            hashing,
            validating,
            time_of_scan,
            ..Default::default()
        }
    }

    pub fn create(db: &Database, root: &Root, hashing: bool, validating: bool) -> Result<Self, FsPulseError> {
        let (scan_id, time_of_scan): (i64, i64) = db.conn().query_row(
            "INSERT INTO scans (root_id, state, hashing, validating, time_of_scan) 
             VALUES (?, ?, ?, ?, strftime('%s', 'now', 'utc')) 
             RETURNING id, time_of_scan",
            [root.id(), ScanState::Scanning.as_i64(), hashing as i64, validating as i64],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
    
        let scan = Scan::new_for_scan(scan_id, root.id(), ScanState::Scanning.as_i64(), hashing, validating, time_of_scan);
        Ok(scan)
    }

    pub fn get_latest(db: &Database) -> Result<Option<Self>, FsPulseError> {
        Self::get_by_id_or_latest(db, None, None)
    }

    pub fn get_by_id(db: &Database, scan_id: i64) -> Result<Option<Self>, FsPulseError> {
        Self::get_by_id_or_latest(db, Some(scan_id), None)
    }

    pub fn get_latest_for_root(db: &Database, root_id: i64) -> Result<Option<Self>, FsPulseError> {
        Self::get_by_id_or_latest(db, None, Some(root_id))
    }

    fn get_by_id_or_latest(db: &Database, scan_id: Option<i64>, root_id: Option<i64>) -> Result<Option<Self>, FsPulseError> {
        let conn = db.conn();

        let (query, query_param) = match (scan_id, root_id) {
            (Some(_), _) => (SQL_SCAN_ID_OR_LATEST, scan_id),
            (_, Some(_)) => (SQL_LATEST_FOR_ROOT, root_id),
            _ => (SQL_SCAN_ID_OR_LATEST, None),
        };

        // If the scan id wasn't explicitly specified, load the most recent otherwise,
        // load the specified scan
        let scan_row: Option<Scan> = conn.query_row(
            query,
            params![query_param],
            |row| {
                Ok(Scan {
                    id: row.get(0)?,
                    root_id: row.get(1)?,
                    state: row.get(2)?,
                    hashing: row.get(3)?,
                    validating: row.get(4)?,
                    time_of_scan: row.get(5)?,
                    file_count: row.get(6)?,
                    folder_count: row.get(7)?,
                })
            },
        )
        .optional()?;

        Ok(scan_row)
    }

    pub fn id(&self) -> i64 {
        self.id
    }

    pub fn root_id(&self) -> i64 {
        self.root_id
    }

    pub fn state(&self) -> ScanState {
        ScanState::from_i64(self.state)
    }

    pub fn hashing(&self) -> bool {
        self.hashing
    }

    pub fn validating(&self) -> bool {
        self.validating
    }

    pub fn time_of_scan(&self) -> i64 {
        self.time_of_scan
    }

    pub fn file_count(&self) -> Option<i64> {
        self.file_count
    }

    pub fn folder_count(&self) -> Option<i64> {
        self.folder_count
    }

    pub fn set_state_sweeping(&mut self, db: &mut Database) -> Result<(), FsPulseError> {
        match self.state() {
            ScanState::Scanning => self.set_state(db, ScanState::Sweeping),
            _ => Err(FsPulseError::Error(format!("Can't set Scan Id {} to state sweeping from state {}", self.id(), self.state().as_i64())))
        }
    }

    pub fn set_state_analyzing(&mut self, db: &mut Database) -> Result<(), FsPulseError> {
        let tx = db.conn_mut().transaction()?;

        let (file_count, folder_count): (i64, i64) = tx.query_row(
            "SELECT 
                SUM(CASE WHEN item_type = 'F' THEN 1 ELSE 0 END) AS file_count, 
                SUM(CASE WHEN item_type = 'D' THEN 1 ELSE 0 END) AS folder_count 
                FROM items WHERE last_scan_id = ?",
                [self.id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            ).unwrap_or((0, 0)); // If no data, default to 0

        // Update the scan entity to indicate that it completed
        tx.execute(
            "UPDATE scans SET file_count = ?, folder_count = ?, state = ? WHERE id = ?",
            (file_count, folder_count, ScanState::Analyzing.as_i64(), self.id)
        )?;

        tx.commit()?;
        
        self.state = ScanState::Analyzing.as_i64();

        self.file_count = Some(file_count);
        self.folder_count = Some(folder_count);

        Ok(())
    }

    pub fn set_state_completed(&mut self, db: &mut Database) -> Result<(), FsPulseError> {
        match self.state() {
            ScanState::Analyzing => self.set_state(db, ScanState::Completed),
            _ => Err(FsPulseError::Error(format!("Can't set Scan Id {} to state completed from state {}", self.id(), self.state().as_i64())))
        }
    }

    pub fn set_state_stopped(&mut self, db: &mut Database) -> Result<(), FsPulseError> {
        match self.state() {
            ScanState::Scanning | ScanState::Sweeping | ScanState::Analyzing => {
                //self.set_state(db, ScanState::Stopped)
                Scan::stop_scan(db, self)?;
                self.state = ScanState::Stopped.as_i64();
                Ok(())
            }
            _ => Err(FsPulseError::Error(format!("Can't stop scan - invalid state {}", self.state().as_i64())))
        }
    }

    fn set_state(&mut self, db: &mut Database, new_state: ScanState) -> Result<(), FsPulseError> {
        let conn = &mut db.conn_mut();

        let rows_updated = conn.execute(
            "UPDATE scans SET state = ? WHERE id = ?", 
            [new_state.as_i64(), self.id]
        )?;

        if rows_updated == 0 {
            return Err(FsPulseError::Error(
                format!("Could not update the state of Scan Id {} to {}", self.id, new_state.as_i64())));
        }

        self.state = new_state.as_i64();

        Ok(())
    }

    pub fn for_each_scan<F>(db: &Database, last: u32, mut func: F) -> Result<(), FsPulseError> 
    where
        F: FnMut(&Database, &Scan) -> Result<(), FsPulseError>,
    {
        if last == 0 {
            return Ok(());
        }
        
        let mut stmt = db.conn().prepare(
            "SELECT 
                s.id,
                s.root_id,
                s.state,
                s.hashing,
                s.validating,
                s.time_of_scan,
                s.file_count,
                s.folder_count
            FROM scans s
            LEFT JOIN changes c ON s.id = c.scan_id
            GROUP BY s.id, s.root_id, s.state, s.hashing, s.validating, s.time_of_scan, s.file_count, s.folder_count
            ORDER BY s.id DESC
            LIMIT ?"
        )?;

        let rows = stmt.query_map([last], |row| {
            Ok(Scan {
                id: row.get::<_, i64>(0)?,                              // scan id
                root_id: row.get::<_, i64>(1)?,                         // root id
                state: row.get::<_, i64>(2)?,                         // root id
                hashing: row.get::<_, bool>(3)?,                        // hashing
                validating: row.get::<_, bool>(4)?,                        // validating
                time_of_scan: row.get::<_, i64>(5)?,                    // time of scan
                file_count: row.get::<_, Option<i64>>(6)?,              // file count
                folder_count: row.get::<_, Option<i64>>(7)?,            // folder count
            })
        })?;

        for row in rows {
            let scan = row?;
            func(db, &scan)?;
        }

        Ok(())
    }

    pub fn stop_scan(db: &mut Database, scan: &Scan) -> Result<(), FsPulseError> {
        let tx = db.conn_mut().transaction()?;

        // Find the id of the last scan on this root to not be stopped
        // We'll restore this scan_id to all partially updated by the
        // scan being stopped
        let prev_scan_id: i64 = tx.query_row(
            "SELECT COALESCE(
                (SELECT MAX(id)
                 FROM scans
                 WHERE root_id = ?
                   AND id < ?
                   AND state = 4
                ),
                0
            ) AS prev_scan_id",
            [scan.root_id(), scan.id()],
            |row| row.get::<_, i64>(0)
        )?;

        // Undo Add (when they are reyhdrates) and Type Change
        // When an item was previously tombstoned and then was found again during a scan
        // the item is rehydrated. This means that is_tombstone is set to false and all properties
        // on the item are cleared and set to new values. In this case the cleared properties are
        // stored in the change record, and we can recover them from there. A type change is
        // handled similarly. When an item that was known to be a file or folder is next seen as the other
        // type, we clear the properties (and store them on the change record). So, in both cases, we can
        // now recover the modfiied properties from the change and this batch handles the minor differences
        // between the two operations
        tx.execute(
            "UPDATE items
            SET (
                item_type,
                is_tombstone,
                last_modified,
                file_size,
                last_hash_scan_id,
                file_hash,
                last_validation_scan_id,
                validation_state,
                validation_state_desc,
                last_scan_id
            ) =
            (
                SELECT 
                    CASE WHEN c.change_type = 'T' THEN c.prev_item_type ELSE items.item_type END,
                    CASE WHEN c.change_type = 'A' THEN 1 ELSE items.is_tombstone END,
                    c.prev_last_modified,
                    c.prev_file_size,
                    c.prev_last_hash_scan_id,
                    c.prev_hash,
                    c.prev_last_validation_scan_id,
                    c.prev_validation_state,
                    c.prev_validation_state_desc,
                    ?1
                FROM changes c
                WHERE c.item_id = items.id
                    AND c.scan_id = ?2
                    AND ((c.change_type = 'A' AND c.prev_is_tombstone = 1) OR c.change_type = 'T')
                LIMIT 1
            )
            WHERE id IN (
                SELECT item_id 
                FROM changes 
                WHERE scan_id = ?2
                    AND ((change_type = 'A' AND prev_is_tombstone = 1) OR change_type = 'T')
            )", 
            [prev_scan_id, scan.id()]
        )?;
        
        // Undoing a modify requires selectively copying back (from the change)
        // the property groups that were part of the modify
        tx.execute(
            "UPDATE items
            SET (
                last_modified, 
                file_size, 
                last_hash_scan_id, 
                file_hash,
                last_validation_scan_id, 
                validation_state, 
                validation_state_desc, 
                last_scan_id
            ) =
            (
            SELECT 
                CASE WHEN c.metadata_changed = 1 THEN COALESCE(c.prev_last_modified, items.last_modified) ELSE items.last_modified END,
                CASE WHEN c.metadata_changed = 1 THEN COALESCE(c.prev_file_size, items.file_size) ELSE items.file_size END,
                CASE WHEN c.hash_changed = 1 THEN c.prev_last_hash_scan_id ELSE items.last_hash_scan_id END,
                CASE WHEN c.hash_changed = 1 THEN c.prev_hash ELSE items.file_hash END,
                CASE WHEN c.validation_changed = 1 THEN c.prev_last_validation_scan_id ELSE items.last_validation_scan_id END,
                CASE WHEN c.validation_changed = 1 THEN c.prev_validation_state ELSE items.validation_state END,
                CASE WHEN c.validation_changed = 1 THEN c.prev_validation_state_desc ELSE items.validation_state_desc END,
                ?1
            FROM changes c
            WHERE c.item_id = items.id 
                AND c.scan_id = ?2
                AND c.change_type = 'M'
            LIMIT 1
            )
            WHERE last_scan_id = ?2
            AND EXISTS (
                SELECT 1 FROM changes c 
                WHERE c.item_id = items.id 
                    AND c.scan_id = ?2
                    AND c.change_type = 'M'
            )", 
            [prev_scan_id, scan.id()]
        )?;

        // Undo deletes. This is simple because deletes just set the tombstone flag
        tx.execute(
            "UPDATE items
            SET is_tombstone = 0,
                last_scan_id = ?1
            WHERE id IN (
                SELECT item_id
                FROM changes
                WHERE scan_id = ?2
                  AND change_type = 'D'
            )",
            [prev_scan_id, scan.id()]
        )?;

        // Mark the scan as stopped
        tx.execute(
            "UPDATE scans
                SET state = 5
                WHERE id = ?1",
            [scan.id()]
        )?;

        // Delete the change records from the stopped scan
        tx.execute(
            "DELETE FROM changes WHERE scan_id = ?1", 
            [scan.id()])?;

        // Final step is to delete the remaining items that were created during
        // the scan. We have to do this after the change records are deleted otherwise
        // attemping to delete these rows will generate a referential integrity violation
        // since we'll be abandoning change records. This operation assumes that the
        // only remaining items with a last_scan_id of the current scan are the simple
        // adds. This should be true :)
       tx.execute(
        "DELETE FROM items
        WHERE last_scan_id = ?", 
        [scan.id()]
        )?;

        tx.commit()?;

        Ok(())

    }
}


/*

Undo a scan
Find previous scan:



**** Undo Add or Type Change


**** Undo Modify



  **** Undo Delete

 

-- Delete all change records for the aborted scan
DELETE FROM changes
WHERE scan_id = :scan_id;

*/