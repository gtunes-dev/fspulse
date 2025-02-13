use crate::error::DirCheckError;
use crate::database::{ Database, ItemType, ChangeType };

use rusqlite::ffi::SQLITE_CHANGESET_CONFLICT;
use rusqlite::{ OptionalExtension, Result, params };

use std::collections::VecDeque;
use std::{env, path};
use std::fs;
use std::fs::Metadata;
use std::path::Path;
use std::path::PathBuf;

#[derive(Debug, Default)]
struct ChangeCounts {
    add_count: u32,
    modify_count: u32,
    delete_count: u32,
    type_change_count: u32,
    unchanged_count: u32,
}

#[derive(Debug, Default)]
pub struct Scan {
    // Schema fields
    scan_id: i64,
    root_path_id: i64,
    root_path: String,

    // Optional values
    path_arg: Option<String>,

    // Scan state
    change_counts: Option<ChangeCounts>,
}

#[derive(Clone, Debug)]
struct QueueEntry {
    path: PathBuf,
    metadata: fs::Metadata,
}

impl Scan {
    // Create a Scan that will be used during a directory scan
    // In this case, the scan_id is not yet known
    fn new_from_path_arg(scan_id: i64, root_path_id: i64, root_path: String, path_arg: String) -> Self {
        Scan {
            scan_id,
            root_path_id,
            root_path,
            path_arg: Some(path_arg),
            ..Default::default()
        }
    }

    // Private function used once all fields have been fetched
    fn new(scan_id: i64, root_path_id: i64, root_path: String) -> Self {
        Scan {
            scan_id,
            root_path_id,
            root_path,
            ..Default::default()
        }
    }

    pub fn new_latest(db: &Database) -> Result<Self, DirCheckError> {
        let conn = &db.conn;
      
       // If the scan id wasn't explicitly specified, load the
       let scan_row: Option<(i64, i64)> = conn.query_row(
        "SELECT id, root_path_id FROM scans
            WHERE id = SELECT MAX(id) FROM scans",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        ).optional()?;

        let (scan_id, root_path_id) = scan_row.ok_or_else(|| {
            DirCheckError::Error("No scan found".to_string())
        })?;

        let scan = Scan::new_from_id_and_path_id(db, scan_id, root_path_id)?;

        Ok(scan)
    }

    pub fn new_from_scan_id(db: &Database, scan_id: i64) -> Result<Self, DirCheckError> {
        let conn = &db.conn;

        // If the scan id wasn't explicitly specified, load the
        let scan_row: Option<(i64, i64)> = conn.query_row(
            "SELECT id, root_path_id FROM scans
            WHERE id",
            params![scan_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        ).optional()?;

        let (scan_id, root_path_id) = scan_row.ok_or_else(|| {
            DirCheckError::Error("No scan found".to_string())
        })?;

        let scan = Scan::new_from_id_and_path_id(db, scan_id, root_path_id)?;

        Ok(scan) 
    }

    fn new_from_id_and_path_id(db: &Database, scan_id: i64, root_path_id: i64) -> Result<Self, DirCheckError> {
        let conn = &db.conn;

        // TODO: This is temporary and should move to a root_path class
        let root_path: Option<String> = conn.query_row(
            "SELECT path FROM root_paths WHERE id = ?",
            params![root_path_id],
            |row| row.get(0),
        ).optional()?;

        let root_path = root_path.ok_or_else(|| {
            DirCheckError::Error(format!("No root path found for scan id: {}", scan_id))
        })?;

        let scan = Scan::new(scan_id, root_path_id, root_path);

        Ok(scan)
    }

    pub fn scan_id(&self) -> i64 {
        self.scan_id
    }

    pub fn root_path_id(&self) -> i64 {
        self.root_path_id
    }

    pub fn root_path(&self) -> &str {
        &self.root_path
    }

    pub fn path_arg(&self) -> &str {
        self.path_arg.as_deref().unwrap_or("")
    }

    pub fn change_counts(&self) -> Option<&ChangeCounts> {
        self.change_counts.as_ref()
    }

    pub fn with_id_or_latest<F>(db: &Database, scan_id: Option<i64>, func: F) -> Result<(), DirCheckError>
    where
        F: FnOnce(&Database, &Scan) -> Result<(), DirCheckError>,
    {
        let scan = scan_id
            .map_or_else(|| Scan::new_latest(db), |id| Scan::new_from_scan_id(db, id))?;

        func(db, &scan)
    }

    fn increment_change_count(&mut self, change_type: ChangeType) {
        let change_counts = self.change_counts.get_or_insert_default();

        match change_type {
            ChangeType::Add => change_counts.add_count += 1,
            ChangeType::Delete => change_counts.delete_count += 1,
            ChangeType::Modify => change_counts.modify_count += 1,
            ChangeType::TypeChange => change_counts.type_change_count += 1,
            ChangeType::NoChange => change_counts.unchanged_count += 1,
        }
    }

    pub fn do_scan(db: &mut Database, path_arg: String) -> Result<Scan, DirCheckError> {
        //let path_canonical: PathBuf = Self::path_arg_to_canonical_path_buf(&path_arg)?;

        //let mut scan = Self::new_for_scan(path_arg, path_canonical);
        let mut scan = Scan::scan_directory(db, path_arg)?;
        scan.print_scan_results();

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
        //let path_canonical: PathBuf = Self::path_arg_to_canonical_path_buf(&path_arg)?;

    
        let mut scan = Self::begin_scan(db, path_arg)?;
        let root_path_buf = PathBuf::from(scan.root_path());
        let metadata = fs::symlink_metadata(&root_path_buf)?;

        let mut q = VecDeque::new();
    
        q.push_back(QueueEntry {
            path: root_path_buf,
            metadata,
        });
    
        while let Some(q_entry) = q.pop_front() {    
            // Update the database
            let dir_change_type = scan.handle_item(db, ItemType::Directory, q_entry.path.as_path(), &q_entry.metadata)?;
            scan.increment_change_count(dir_change_type);
    
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
                    scan.increment_change_count(file_change_type);
                }
            }
        }
        scan.end_scan(db)?;
    
        Ok(scan)
    }

    fn begin_scan(db: &mut Database, path_arg: String) -> Result<Self, DirCheckError> {
        let conn = &mut db.conn;

        let path_canonical = Self::path_arg_to_canonical_path_buf(&path_arg)?;
        let root_path = path_canonical.to_string_lossy().to_string();

        // TODO: wrap this in a transaction
        conn.execute(
            "INSERT OR IGNORE INTO root_paths (path) VALUES (?)", 
            [&root_path]
        )?;

        // Get the root_path_id
        let root_path_id: i64 = conn.query_row(
            "SELECT id FROM root_paths WHERE path = ?",
            [&root_path],
            |row| row.get(0),
        )?;

        // Insert into scans table with UTC timestamp
        conn.execute(
            "INSERT INTO scans (root_path_id, scan_time) VALUES (?, strftime('%s', 'now', 'utc'))",
            [root_path_id],
        )?;

        // Get the new scan_id
        let scan_id: i64 = conn.query_row(
            "SELECT last_insert_rowid()",
            [],
            |row| row.get(0),
        )?;

        Ok(Scan::new_from_path_arg(scan_id, root_path_id, root_path, path_arg))
    }

    fn handle_item(&mut self, db: &mut Database, item_type: ItemType, path: &Path, metadata: &Metadata) -> Result<ChangeType, DirCheckError> {
        let path_str = path.to_string_lossy();
        let scan_id = self.scan_id;
        let root_path_id = self.root_path_id;

        let conn = &mut db.conn;

        let mut change_type: Option<ChangeType> = None;
    
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
                    change_type = Some(ChangeType::Add);
                    tx.commit()?;
                } else if existing_type != item_type_str {
                    // Item type changed (e.g., file -> directory)
                    let tx = conn.transaction()?;
                    tx.execute("UPDATE entries SET item_type = ?, last_modified = ?, file_size = ?, last_seen_scan_id = ? WHERE id = ?", 
                        (item_type_str, last_modified, file_size, scan_id, entry_id))?;
                    tx.execute("INSERT INTO changes (scan_id, entry_id, change_type) VALUES (?, ?, ?)", 
                        (self.scan_id, entry_id, ChangeType::TypeChange.as_db_str()))?;
                    change_type = Some(ChangeType::TypeChange);
                    tx.commit()?;
                } else if existing_modified != last_modified || existing_size != file_size {
                    // Item content changed
                    let tx = conn.transaction()?;
                    tx.execute("UPDATE entries SET last_modified = ?, file_size = ?, last_seen_scan_id = ? WHERE id = ?", 
                        (last_modified, file_size, scan_id, entry_id))?;
                    tx.execute("INSERT INTO changes (scan_id, entry_id, change_type) VALUES (?, ?, ?)", 
                        (scan_id, entry_id, ChangeType::Modify.as_db_str()))?;
                    change_type = Some(ChangeType::Modify);
                    tx.commit()?;
                } else {
                    // No change, just update last_seen_scan_id
                    conn.execute("UPDATE entries SET last_seen_scan_id = ? WHERE root_path_id = ? AND id = ?", 
                        (scan_id, root_path_id, entry_id))?;
                    change_type = Some(ChangeType::NoChange);
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
                change_type = Some(ChangeType::Add);
                tx.commit()?;
            }
        }
        
        change_type.ok_or(DirCheckError::Error("Expected a change type, but found None".to_string()))
    }

    fn end_scan(&mut self, db: &mut Database) -> Result<(), DirCheckError> {
        let root_path_id = self.root_path_id;
        let scan_id = self.scan_id;

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
    
        tx.commit()?;
    
        Ok(())
    }

    fn str_value_or_none(s: &Option<String>) -> &str {
        s.as_deref().unwrap_or("None")
    }

    fn print_scan_results(&mut self) {

        let change_counts = self.change_counts();

        println!("Scan Id: {}", self.scan_id());
        println!("Path Argument: {}", Self::str_value_or_none(&self.path_arg));
        println!("Root Path Id: {}", self.root_path_id);
        println!("Root Path: {}", self.root_path());

        println!("-- Change Counts --");
        if let Some(counts) = change_counts {
            println!("{:<12} {}", "Added:", counts.add_count);
            println!("{:<12} {}", "Modified:", counts.modify_count);
            println!("{:<12} {}", "Deleted:", counts.delete_count);
            println!("{:<12} {}", "Type Change:", counts.type_change_count);
            println!("{:<12} {}", "No Change:", counts.unchanged_count);
            println!();
        } else {
            println!("None");
        }


    }
}