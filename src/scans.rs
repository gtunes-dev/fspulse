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
        let (scan_id, time_of_scan): (i64, i64) = db.conn.query_row(
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
        let conn = &db.conn;

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
        let tx = db.conn.transaction()?;

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
        let conn = &mut db.conn;

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

/*     pub fn do_scan(
        db: &mut Database, 
        root_id: Option<u32>, 
        root_path: Option<String>, 
        last: bool, 
        hash: bool,
        validate: bool
    ) -> Result<Scan, FsPulseError> {
        let path = match (last, root_id, root_path) {
            (_, Some(root_id), _) => {
                Root::get_by_id(db, root_id.into())?
                    .ok_or_else(|| FsPulseError::Error(format!("Root {} not found", root_id)))?
                    .path()
                    .to_string()
            }
            (true, None, _) => {
                let scan = Self::get_latest(db)?
                    .ok_or_else(|| FsPulseError::Error("No latest scan found".into()))?;
                Root::get_by_id(db, scan.root_id())?
                    .ok_or_else(|| FsPulseError::Error("Root not found".into()))?
                    .path()
                    .to_string()
            }
            (false, None, Some(p)) => p,
            (false, None, None) => return Err(FsPulseError::Error("No path specified".into())),
        };
        
        let (scan, _root) = Scan::scan_directory(db, &path, hash, validate)?;
        Reports::print_scan(db, &Some(scan), ReportFormat::Table)?;

        Ok(scan)
    } */
/* 
    fn path_arg_to_canonical_path_buf(path_arg: &str) -> Result<PathBuf, FsPulseError> {
        if path_arg.is_empty() {
            return Err(FsPulseError::Error("Provided path is empty".into()));
        }

        let path = Path::new(path_arg);

        let absolute_path = if path.is_absolute() {
            path.to_owned() 
        } else {
            env::current_dir()?.join(path)
        };
        
        if !absolute_path.exists() {
            return Err(FsPulseError::Error(format!("Path '{}' does not exist", absolute_path.display())));
        }
    
        let metadata = fs::symlink_metadata(&absolute_path)?;

        if metadata.file_type().is_symlink() {
            return Err(FsPulseError::Error(format!("Path '{}' is a symlink and not allowed", absolute_path.display())));
        }
        
        if !metadata.is_dir() {
            return Err(FsPulseError::Error(format!("Path '{}' is not a directory", absolute_path.display())));
        }

        let canonical_path = absolute_path.canonicalize()?;
    
        Ok(canonical_path)
    }
 */
    /* 
    fn scan_directory(db: &mut Database, path: &str, hash: bool, validate: bool) -> Result<(Self, Root), FsPulseError> {
        let (mut scan, root) = Self::begin_scan(db, path, hash, validate)?;
        let root_path_buf = PathBuf::from(root.path());
        let metadata = fs::symlink_metadata(&root_path_buf)?;

        let mut q = VecDeque::new();

        
        let multi = MultiProgress::new();
        multi.println(format!("Scanning: {}", &path))?;
        let dir_bar = multi.add(ProgressBar::new_spinner());
        dir_bar.enable_steady_tick(Duration::from_millis(100));
        let item_bar = multi.add(ProgressBar::new_spinner());
        item_bar.enable_steady_tick(Duration::from_millis(100));

        let mut progress_bar = if hash {
            let bar = ProgressBar::new(0); // Initialize with 0 length
        
            // TODO: this error will panic
            bar.set_style(ProgressStyle::default_bar()
                .template("{msg}\n[{bar:40}] {bytes}/{total_bytes} ({eta})")
                .unwrap()
                .progress_chars("#>-"));
            Some(bar)
        } else {
            None
        };
    
        q.push_back(QueueEntry {
            path: root_path_buf.clone(),
            metadata,
        });
    
        while let Some(q_entry) = q.pop_front() {
            dir_bar.set_message(format!("Directory: '{}'", q_entry.path.to_string_lossy()));
            // Update the database
            if q_entry.path != root_path_buf {
                let dir_change_type = scan.handle_item(db, ItemType::Directory, q_entry.path.as_path(), &q_entry.metadata, None)?;
                scan.change_counts.increment_count_of(dir_change_type);
            }
    
            let items = fs::read_dir(&q_entry.path)?;
    
            for item in items {
                let item = item?;
                let metadata = fs::symlink_metadata(item.path())?; // Use symlink_metadata to check for symlinks
                item_bar.set_message(format!("Item: '{}'", item.file_name().to_string_lossy()));

                if metadata.is_dir() {
                    q.push_back(QueueEntry {
                        path: item.path(),
                        metadata,
                    });
                } else {
                    let item_type = if metadata.is_file() {
                        ItemType::File
                    } else if metadata.is_symlink() {
                        ItemType::Symlink
                    } else {
                        ItemType::Other
                    };
    
                    // println!("{:?}: {}", item_type, item.path().display());
                    let mut hash = None;
                    match (item_type, progress_bar.as_mut()) {
                        (ItemType::File, Some(ref progress_bar)) => {
                            hash = match Hash::compute_md5_hash(&item.path(), progress_bar) {
                                Ok(hash_s) => Some(hash_s),
                                Err(error) => {
                                    eprintln!("Error computing hash: {}", error);
                                    None
                                }
                            }
                        },
                        _ => { // nothing to do
                        },
                    }

                    let file_change_type = scan.handle_item(db, item_type, &item.path(), &metadata, hash.as_deref())?;
                    scan.change_counts.increment_count_of(file_change_type);
                }
            }
        }
        scan.end_scan(db)?;
    
        Ok((scan, root))
    }
    */

 /*    fn begin_scan(db: &mut Database, path_arg: &str, hashing: bool, validating: bool) -> Result<(Self, Root), FsPulseError> {
        let path_canonical = Self::path_arg_to_canonical_path_buf(&path_arg)?;
        let root_path_str = path_canonical.to_string_lossy().to_string();

        let root = Root::get_or_insert(db, &root_path_str)?;
        let root_id = root.id();

        let (scan_id, time_of_scan): (i64, i64) = db.conn.query_row(
            "INSERT INTO scans (root_id, state, hashing, validating, time_of_scan) 
             VALUES (?, ?, ?, ?, strftime('%s', 'now', 'utc')) 
             RETURNING id, time_of_scan",
            [root_id, ScanState::Scanning.as_i64(), hashing as i64, validating as i64],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;

        let scan = Scan::new_for_scan(scan_id, root_id, ScanState::Scanning, false, false, time_of_scan);
        Ok((scan, root))
    }

    fn handle_item(
        &mut self, 
        db: &mut Database,
        item_type: ItemType, 
        path: &Path, 
        metadata: &Metadata, 
        file_hash: Option<&str>
    ) -> Result<ChangeType, FsPulseError> {
        let path_str = path.to_string_lossy();
        let scan_id = self.id;
        let root_id = self.root_id;

        let conn = &mut db.conn;
    
        // Determine timestamps and file size
        let last_modified = metadata.modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs() as i64);
        let file_size = if metadata.is_file() { Some(metadata.len() as i64) } else { None };
    
        // Check if the item already exists (fetching `id`, `is_tombstone` as well)
        let existing_item: Option<(i64, String, Option<i64>, Option<i64>, Option<String>, bool)> = conn.query_row(
            "SELECT id, item_type, last_modified, file_size, file_hash, is_tombstone FROM items WHERE root_id = ? AND path = ?",
            (root_id, &path_str),
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2).ok(), row.get(3).ok(), row.get(4)?, row.get(5)?)),
        ).optional()?;
    
        let change_type = match existing_item {
            Some((item_id, existing_type, existing_modified, existing_size, existing_hash, is_tombstone)) => {
                let item_type_str = item_type.as_str();
                let metadata_changed = existing_modified != last_modified || existing_size != file_size;

                // println!("{}", Utils::string_value_or_none(&existing_hash));
                // println!("{}", Utils::str_value_or_none(&file_hash));
                // println!();

                let hash_changed = file_hash.map_or(false, |h| Some(h) != existing_hash.as_deref());
            
                if is_tombstone {
                    let tx = conn.transaction()?;
                    tx.execute("UPDATE items SET item_type = ?, last_modified = ?, file_size = ?, file_hash = ?, last_scan_id = ?, is_tombstone = 0 WHERE id = ?", 
                        (item_type_str, last_modified, file_size, file_hash, scan_id, item_id))?;
                    tx.execute("INSERT INTO changes (scan_id, item_id, change_type) VALUES (?, ?, ?)", 
                        (scan_id, item_id, ChangeType::Add.as_str()))?;
                    tx.commit()?;
                    ChangeType::Add
                } else if existing_type != item_type_str {
                    // Item type changed (e.g., file -> directory)
                    let tx = conn.transaction()?;
                    tx.execute("UPDATE items SET item_type = ?, last_modified = ?, file_size = ?, file_hash = ?, last_scan_id = ? WHERE id = ?", 
                        (item_type_str, last_modified, file_size, file_hash, scan_id, item_id))?;
                    tx.execute("INSERT INTO changes (scanhttps://docs.rs/crate/console/0.15.11/features_id, item_id, change_type) VALUES (?, ?, ?)", 
                        (self.id, item_id, ChangeType::TypeChange.as_str()))?;
                    tx.commit()?;
                    ChangeType::TypeChange
                } else if metadata_changed || hash_changed {
                    // Item content changed
                    let tx = conn.transaction()?;

                     // TODO: this is not doing the right thing with last_hash_scan_id and last_is_valid_scan_id
                    tx.execute("UPDATE items 
                        SET last_modified = ?, 
                        file_size = ?,             
                        file_hash = ?,
                        last_scan_id = ? 
                        WHERE id = ?", 
                        (last_modified, file_size, file_hash.or(existing_hash.as_deref()), scan_id, item_id))?;
                    tx.execute("INSERT INTO changes 
                        (scan_id, item_id, change_type, prev_last_modified, prev_file_size, prev_hash) 
                        VALUES (?, ?, ?, ?, ?, ?)", 
                        (
                            scan_id, 
                            item_id, 
                            ChangeType::Modify.as_str(),
                            metadata_changed.then_some(existing_modified),
                            metadata_changed.then_some(existing_size),
                            hash_changed.then_some(existing_hash),
                        ))?;
                    tx.commit()?;
                    ChangeType::Modify
                } else {
                    // No change, just update last_scan_id
                    conn.execute("UPDATE items SET last_scan_id = ? WHERE root_id = ? AND id = ?", 
                        (scan_id, root_id, item_id))?;
                    ChangeType::NoChange
                }
            }
            None => {
                // Item is new, insert into items and changes tables
                let tx = conn.transaction()?;
                tx.execute("INSERT INTO items (root_id, path, item_type, last_modified, file_size, file_hash, last_scan_id) VALUES (?, ?, ?, ?, ?, ?, ?)",
                    (root_id, &path_str, item_type.as_str(), last_modified, file_size, file_hash, scan_id))?;
                let item_id: i64 = tx.query_row("SELECT last_insert_rowid()", [], |row| row.get(0))?;
                tx.execute("INSERT INTO changes (scan_id, item_id, change_type) VALUES (?, ?, ?)",
                    (scan_id, item_id, ChangeType::Add.as_str()))?;
                tx.commit()?;
                ChangeType::Add
            }
        };
        
        Ok(change_type)
    }

    fn end_scan(&mut self, db: &mut Database) -> Result<(), FsPulseError> {
        let root_id = self.root_id;
        let scan_id = self.id;

        let conn = &mut db.conn;
    
        let tx = conn.transaction()?;
    
        // Insert deletion records into changes
        tx.execute(
            "INSERT INTO changes (scan_id, item_id, change_type)
             SELECT ?, id, ?
             FROM items
             WHERE root_id = ? AND is_tombstone = 0 AND last_scan_id < ?",
            (scan_id, ChangeType::Delete.as_str(), root_id, scan_id),
        )?;
        
        // Mark unseen items as tombstones
        tx.execute(
            "UPDATE items SET is_tombstone = 1 WHERE root_id = ? AND last_scan_id < ? AND is_tombstone = 0",
            (root_id, scan_id),
        )?;

        // Step 3: Count total files and directories seen in this scan
        let (file_count, folder_count): (i64, i64) = tx.query_row(
        "SELECT 
            SUM(CASE WHEN item_type = 'F' THEN 1 ELSE 0 END) AS file_count, 
            SUM(CASE WHEN item_type = 'D' THEN 1 ELSE 0 END) AS folder_count 
            FROM items WHERE last_scan_id = ?",
            [scan_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        ).unwrap_or((0, 0)); // If no data, default to 0

        // Update the scan entity to indicate that it completed
        tx.execute(
            "UPDATE scans SET file_count = ?, folder_count = ?, state = ? WHERE id = ?",
            (file_count, folder_count, ScanState::Completed.as_i64(), scan_id)
        )?;

        tx.commit()?;

        self.file_count = Some(file_count);
        self.folder_count = Some(folder_count);
        self.state = ScanState::Completed;

        // scan.change_counts acts as an accumulator during a scan but now we get the truth from the
        // database. We need this to include deletes since they aren't known until tombstoning is complete
        self.change_counts = ChangeCounts::get_by_scan_id(db, self.id)?;

        Ok(())
    }
 */
    pub fn for_each_scan<F>(db: &Database, last: u32, mut func: F) -> Result<i32, FsPulseError> 
    where
        F: FnMut(&Database, &Scan) -> Result<(), FsPulseError>,
    {
        if last == 0 {
            return Ok(0);
        }
        
        let mut stmt = db.conn.prepare(
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