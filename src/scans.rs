use crate::changes::ChangeCounts;
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
    state: ScanState,
    hashing: bool,
    validating: bool,
    time_of_scan: i64,
    file_count: Option<i64>,
    folder_count: Option<i64>,
    
    // Scan state
    change_counts: ChangeCounts,
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
    fn new_for_scan(id: i64, root_id: i64, state: ScanState, hashing: bool, validating: bool, time_of_scan: i64) -> Self {
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
    
        let scan = Scan::new_for_scan(scan_id, root.id(), ScanState::Scanning, hashing, validating, time_of_scan);
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
        let scan_row: Option<(i64, i64, i64, bool, bool, i64, Option<i64>, Option<i64>)> = conn.query_row(
            query,
            params![query_param],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?, row.get(5)?, row.get(6)?, row.get(7)?)),
        )
        .optional()?;

        scan_row.map(|(id, root_id, state, hashing, validating, time_of_scan, file_count, folder_count)| {
            let change_counts = ChangeCounts::get_by_scan_id(db, id)?;
            Ok(Scan {
                id,
                root_id,
                state: ScanState::from_i64(state),
                hashing,
                validating,
                time_of_scan,
                file_count,
                folder_count,
                change_counts,
            })
        })
        .transpose()
    }

    pub fn id(&self) -> i64 {
        self.id
    }

    pub fn root_id(&self) -> i64 {
        self.root_id
    }

    pub fn state(&self) -> ScanState {
        self.state
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

    pub fn change_counts(&self) -> &ChangeCounts {
        &self.change_counts
    }

    pub fn set_state_sweeping(&mut self, db: &mut Database) -> Result<(), FsPulseError> {
        match self.state {
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
        
        self.state = ScanState::Analyzing;

        self.file_count = Some(file_count);
        self.folder_count = Some(folder_count);

        // scan.change_counts acts as an accumulator during a scan but now we get the truth from the
        // database. We need this to include deletes since they aren't known until tombstoning is complete
        self.change_counts = ChangeCounts::get_by_scan_id(db, self.id)?;

        Ok(())
    }

    pub fn set_state_completed(&mut self, db: &mut Database) -> Result<(), FsPulseError> {
        match self.state {
            ScanState::Analyzing => self.set_state(db, ScanState::Completed),
            _ => Err(FsPulseError::Error(format!("Can't set Scan Id {} to state completed from state {}", self.id(), self.state().as_i64())))
        }
    }

    pub fn set_state_stopped(&mut self, db: &mut Database) -> Result<(), FsPulseError> {
        match self.state {
            ScanState::Scanning | ScanState::Sweeping | ScanState::Analyzing => {
                self.set_state(db, ScanState::Stopped)
            }
            _ => Err(FsPulseError::Error(format!("Can't stop scan - invalid state {}", self.state.as_i64())))
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

        self.state = new_state;

        Ok(())
    }

    pub fn for_each_scan<F>(db: &Database, last: u32, mut func: F) -> Result<i32, FsPulseError> 
    where
        F: FnMut(&Database, &Scan) -> Result<(), FsPulseError>,
    {
        if last == 0 {
            return Ok(0);
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
                s.folder_count, 
                COALESCE(SUM(CASE WHEN c.change_type = 'A' THEN 1 ELSE 0 END), 0) AS add_count,
                COALESCE(SUM(CASE WHEN c.change_type = 'M' THEN 1 ELSE 0 END), 0) AS modify_count,
                COALESCE(SUM(CASE WHEN c.change_type = 'D' THEN 1 ELSE 0 END), 0) AS delete_count,
                COALESCE(SUM(CASE WHEN c.change_type = 'T' THEN 1 ELSE 0 END), 0) AS type_change_count
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
                state: ScanState::from_i64(row.get::<_, i64>(2)?),                         // root id
                hashing: row.get::<_, bool>(3)?,                        // hashing
                validating: row.get::<_, bool>(4)?,                        // validating
                time_of_scan: row.get::<_, i64>(5)?,                    // time of scan
                file_count: row.get::<_, Option<i64>>(6)?,              // file count
                folder_count: row.get::<_, Option<i64>>(7)?,            // folder count
                change_counts: ChangeCounts::new(  
                    row.get::<_, i64>(8)?,             // adds
                    row.get::<_, i64>(9)?,          // modifies
                    row.get::<_, i64>(10)?,          // deletes
                    row.get::<_, i64>(11)?,    // type changes
                    0,
                ),
            })
        })?;

        let mut scan_count = 0;

        for row in rows {
            let scan = row?;
            func(db, &scan)?;
            scan_count = scan_count + 1;
        }

        Ok(scan_count)
    }
}