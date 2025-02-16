use crate::change::{ ChangeCounts, ChangeType };
use crate::error::DirCheckError;
use crate::database::{ Database, ItemType };
use crate::reports::Reports;
use crate::root_paths::RootPath;

use rusqlite::{ OptionalExtension, Result, params };

use std::collections::VecDeque;
use std::env;
use std::fs;
use std::fs::Metadata;
use std::path::Path;
use std::path::PathBuf;

#[derive(Debug, Default)]
pub struct Scan {
    // Schema fields
    id: i64,
    //root_path_id: i64,
    //root_path: String, // TODO: Move to Path struct
    time_of_scan: i64,
    file_count: Option<i64>,
    folder_count: Option<i64>,
    is_complete: bool,

    // Related entities
    root_path: RootPath,

    // Scan state
    change_counts: ChangeCounts,
}

#[derive(Clone, Debug)]
struct QueueEntry {
    path: PathBuf,
    metadata: fs::Metadata,
}

impl Scan {
    // Create a Scan that will be used during a directory scan
    // In this case, the scan_id is not yet known
    fn new_for_scan(id: i64, time_of_scan: i64, root_path: RootPath) -> Self {
        Scan {
            id,
            time_of_scan,
            root_path,
            ..Default::default()
        }
    }
    
    /*

    // Private function used once all fields have been fetched
    fn new(scan_id: i64, root_path_id: i64, root_path: String) -> Self {
        Scan {
            scan_id,
            root_path_id,
            root_path,
            ..Default::default()
        }
    }

    pub fn new_from_latest(db: &Database) -> Result<Self, DirCheckError> {
        let scan = Scan::new_from_scan_id(db, None)?;
        Ok(scan)
    }
    */

    pub fn new_from_id(db: &Database, id: Option<i64>) -> Result<Self, DirCheckError> {
        let conn = &db.conn;

        // If the scan id wasn't explicitly specified, load the most recent otherwise,
        // load the specified scan
        let scan_row: Option<(i64, i64, i64, Option<i64>, Option<i64>, bool)> = conn.query_row(
            "SELECT id, root_path_id, time_of_scan, file_count, folder_count, is_complete
                FROM scans
                WHERE id = COALESCE(?1, (SELECT MAX(id) FROM scans))",
            params![id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?, row.get(5)?)),
        ).optional()?;

        let (id, root_path_id, time_of_scan, file_count, folder_count, is_complete) = scan_row.ok_or_else(|| {
            DirCheckError::Error("No scan found".to_string())
        })?;

        let root_path = RootPath::get(db, root_path_id)?;

        Ok(Scan {
            id,
            time_of_scan,
            root_path,
            file_count,
            folder_count,
            is_complete,
            ..Default::default()
        })
    }

    pub fn id(&self) -> i64 {
        self.id
    }

    pub fn root_path_id(&self) -> i64 {
        self.root_path.id()
    }

    pub fn root_path(&self) -> &str {
        self.root_path.path()
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

    pub fn with_id_or_latest<F>(db: &Database, id: Option<i64>, func: F) -> Result<(), DirCheckError>
    where
        F: FnOnce(&Database, &Scan) -> Result<(), DirCheckError>,
    {
        let scan = Scan::new_from_id(db, id)?;

        func(db, &scan)
    }

    pub fn do_scan(db: &mut Database, path_arg: String) -> Result<Scan, DirCheckError> {
        let scan = Scan::scan_directory(db, path_arg)?;
        Reports::print_scan_block(db, &scan)?;

        Ok(scan)
    }

    fn path_arg_to_canonical_path_buf(path_arg: &str) -> Result<PathBuf, DirCheckError> {
        if path_arg.is_empty() {
            return Err(DirCheckError::Error("Provided path is empty".to_string()));
        }

        let path = Path::new(path_arg);

        let absolute_path = if path.is_absolute() {
            path.to_owned() 
        } else {
            env::current_dir()?.join(path)
        };
        
        if !absolute_path.exists() {
            return Err(DirCheckError::Error(format!("Path '{}' does not exist", absolute_path.display())));
        }
    
        let metadata = fs::symlink_metadata(&absolute_path)?;

        if metadata.file_type().is_symlink() {
            return Err(DirCheckError::Error(format!("Path '{}' is a symlink and not allowed", absolute_path.display())));
        }
        
        if !metadata.is_dir() {
            return Err(DirCheckError::Error(format!("Path '{}' is not a directory", absolute_path.display())));
        }

        let canonical_path = absolute_path.canonicalize()?;
    
        Ok(canonical_path)
    }

    fn scan_directory(db: &mut Database, path_arg: String) -> Result<Self, DirCheckError> {
        let mut scan = Self::begin_scan(db, path_arg)?;
        let root_path_buf = PathBuf::from(scan.root_path());
        let metadata = fs::symlink_metadata(&root_path_buf)?;

        let mut q = VecDeque::new();
    
        q.push_back(QueueEntry {
            path: root_path_buf.clone(),
            metadata,
        });
    
        while let Some(q_entry) = q.pop_front() {    
            // Update the database
            if q_entry.path != root_path_buf {
                let dir_change_type = scan.handle_item(db, ItemType::Directory, q_entry.path.as_path(), &q_entry.metadata)?;
                scan.change_counts.increment(dir_change_type);
            }
    
            let entries = fs::read_dir(&q_entry.path)?;
    
            for entry in entries {
                let entry = entry?;
                let metadata = fs::symlink_metadata(entry.path())?; // Use symlink_metadata to check for symlinks
    
                if metadata.is_dir() {
                    q.push_back(QueueEntry {
                        path: entry.path(),
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
    
                    // println!("{:?}: {}", item_type, entry.path().display());
                    
                    // Update the database
                    let file_change_type = scan.handle_item(db, item_type, &entry.path(), &metadata)?;
                    scan.change_counts.increment(file_change_type);
                }
            }
        }
        scan.end_scan(db)?;
    
        Ok(scan)
    }

    fn begin_scan(db: &mut Database, path_arg: String) -> Result<Self, DirCheckError> {
        let path_canonical = Self::path_arg_to_canonical_path_buf(&path_arg)?;
        let root_path_str = path_canonical.to_string_lossy().to_string();

        let root_path = RootPath::get_or_insert(db, &root_path_str)?;

        let (scan_id, time_of_scan): (i64, i64) = db.conn.query_row(
            "INSERT INTO scans (root_path_id, time_of_scan) 
             VALUES (?, strftime('%s', 'now', 'utc')) 
             RETURNING id, time_of_scan",
            [root_path.id()],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;

        let scan = Scan::new_for_scan(scan_id, time_of_scan, root_path);
        Ok(scan)
    }

    fn handle_item(&mut self, db: &mut Database, item_type: ItemType, path: &Path, metadata: &Metadata) -> Result<ChangeType, DirCheckError> {
        let path_str = path.to_string_lossy();
        let scan_id = self.id;
        let root_path_id = self.root_path.id();

        let conn = &mut db.conn;

        let mut change_type: ChangeType = ChangeType::NoChange;
    
        // Determine timestamps and file size
        let last_modified = metadata.modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs() as i64);
        let file_size = if metadata.is_file() { Some(metadata.len() as i64) } else { None };
    
        // Check if the entry already exists (fetching `id`, `is_tombstone` as well)
        let existing_entry: Option<(i64, String, Option<i64>, Option<i64>, bool)> = conn.query_row(
            "SELECT id, item_type, last_modified, file_size, is_tombstone FROM entries WHERE root_path_id = ? AND path = ?",
            (root_path_id, &path_str),
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2).ok(), row.get(3).ok(), row.get(4)?)),
        ).optional()?;
    
        match existing_entry {
            Some((entry_id, existing_type, existing_modified, existing_size, is_tombstone)) => {
                let item_type_str = item_type.as_db_str();
                
                if is_tombstone {
                    // Item previously deleted - resurrect it
                    let tx = conn.transaction()?;
                    tx.execute("UPDATE entries SET item_type = ?, last_modified = ?, file_size = ?, last_seen_scan_id = ?, is_tombstone = 0 WHERE id = ?", 
                        (item_type_str, last_modified, file_size, scan_id, entry_id))?;
                    tx.execute("INSERT INTO changes (scan_id, entry_id, change_type) VALUES (?, ?, ?)", 
                        (scan_id, entry_id, ChangeType::Add.as_db_str()))?;
                    change_type = ChangeType::Add;
                    tx.commit()?;
                } else if existing_type != item_type_str {
                    // Item type changed (e.g., file -> directory)
                    let tx = conn.transaction()?;
                    tx.execute("UPDATE entries SET item_type = ?, last_modified = ?, file_size = ?, last_seen_scan_id = ? WHERE id = ?", 
                        (item_type_str, last_modified, file_size, scan_id, entry_id))?;
                    tx.execute("INSERT INTO changes (scan_id, entry_id, change_type) VALUES (?, ?, ?)", 
                        (self.id, entry_id, ChangeType::TypeChange.as_db_str()))?;
                    change_type = ChangeType::TypeChange;
                    tx.commit()?;
                } else if existing_modified != last_modified || existing_size != file_size {
                    // Item content changed
                    let tx = conn.transaction()?;
                    tx.execute("UPDATE entries SET last_modified = ?, file_size = ?, last_seen_scan_id = ? WHERE id = ?", 
                        (last_modified, file_size, scan_id, entry_id))?;
                    tx.execute("INSERT INTO changes (scan_id, entry_id, change_type) VALUES (?, ?, ?)", 
                        (scan_id, entry_id, ChangeType::Modify.as_db_str()))?;
                    change_type = ChangeType::Modify;
                    tx.commit()?;
                } else {
                    // No change, just update last_seen_scan_id
                    conn.execute("UPDATE entries SET last_seen_scan_id = ? WHERE root_path_id = ? AND id = ?", 
                        (scan_id, root_path_id, entry_id))?;
                    change_type = ChangeType::NoChange;
                }
            }
            None => {
                // Item is new, insert into entries and changes tables
                let tx = conn.transaction()?;
                tx.execute("INSERT INTO entries (root_path_id, path, item_type, last_modified, file_size, last_seen_scan_id) VALUES (?, ?, ?, ?, ?, ?)",
                    (root_path_id, &path_str, item_type.as_db_str(), last_modified, file_size, scan_id))?;
                let entry_id: i64 = tx.query_row("SELECT last_insert_rowid()", [], |row| row.get(0))?;
                tx.execute("INSERT INTO changes (scan_id, entry_id, change_type) VALUES (?, ?, ?)",
                    (scan_id, entry_id, ChangeType::Add.as_db_str()))?;
                change_type = ChangeType::Add;
                tx.commit()?;
            }
        }
        
        Ok(change_type)
    }

    fn end_scan(&mut self, db: &mut Database) -> Result<(), DirCheckError> {
        let root_path_id = self.root_path.id();
        let scan_id = self.id;

        let conn = &mut db.conn;
    
        let tx = conn.transaction()?;
    
        // Insert deletion records into changes
        tx.execute(
            "INSERT INTO changes (scan_id, entry_id, change_type)
             SELECT ?, id, ?
             FROM entries
             WHERE root_path_id = ? AND is_tombstone = 0 AND last_seen_scan_id < ?",
            (scan_id, ChangeType::Delete.as_db_str(), root_path_id, scan_id),
        )?;
        
        // Mark unseen entries as tombstones
        tx.execute(
            "UPDATE entries SET is_tombstone = 1 WHERE root_path_id = ? AND last_seen_scan_id < ? AND is_tombstone = 0",
            (root_path_id, scan_id),
        )?;

        // Step 3: Count total files and directories seen in this scan
        let (file_count, folder_count): (i64, i64) = tx.query_row(
        "SELECT 
            SUM(CASE WHEN item_type = 'F' THEN 1 ELSE 0 END) AS file_count, 
            SUM(CASE WHEN item_type = 'D' THEN 1 ELSE 0 END) AS folder_count 
            FROM entries WHERE last_seen_scan_id = ?",
            [scan_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        ).unwrap_or((0, 0)); // If no data, default to 0

        // Update the scan entry to indicate that it completed
        tx.execute(
            "UPDATE scans SET file_count = ?, folder_count = ?, is_complete = 1 WHERE id = ?",
            (file_count, folder_count, scan_id)
        )?;

        tx.commit()?;

        self.file_count = Some(file_count);
        self.folder_count = Some(folder_count);
        self.is_complete = true;

        Ok(())
    }
}