use crate::error::DirCheckError;
use crate::database::{ Database, ItemType, ChangeType };

use rusqlite::{ OptionalExtension, Result };

use std::collections::VecDeque;
use std::env;
use std::fs;
use std::fs::Metadata;
use std::path::Path;
use std::path::PathBuf;

#[derive(Default)]
struct ChangeCounts {
    add_count: u32,
    modify_count: u32,
    delete_count: u32,
    type_change_count: u32,
    unchanged_count: u32,
}

pub struct DirScan<'a> {
    change_counts: ChangeCounts,
    db: &'a mut Database,
    user_path: &'a Path,
    absolute_path: PathBuf,
    root_path_id: Option<i64>,
    current_scan_id: Option<i64>,
}

#[derive(Clone, Debug)]
struct QueueEntry {
    path: PathBuf,
    metadata: fs::Metadata,
}

impl<'a> DirScan<'a> {
    fn new(db: &'a mut Database, user_path: &'a Path, absolute_path: PathBuf) -> Self {
        Self {
            change_counts: ChangeCounts::default(),
            db,
            user_path,
            absolute_path,
            root_path_id: None,
            current_scan_id: None,
        }
    }

    pub fn scan_directory(db: &mut Database, user_path: &Path) -> Result<(), DirCheckError> {
        let absolute_path = DirScan::validate_and_resolve_path(user_path)?;

        let mut dir_scan = DirScan::new(db, user_path, absolute_path);
        dir_scan.do_scan_directory()?;
        dir_scan.print_scan_results();

        Ok(())
    }

    fn do_scan_directory(&mut self) -> Result<(), DirCheckError> {
        let absolute_path = self.absolute_path.clone();

        let metadata = fs::symlink_metadata(&absolute_path)?;
    
        self.begin_scan(&absolute_path.to_string_lossy())?;
    
        let mut q = VecDeque::new();
    
        q.push_back(QueueEntry {
            path: self.absolute_path.clone(),
            metadata,
        });
    
        while let Some(q_entry) = q.pop_front() {
            // println!("Directory: {}", q_entry.path.display());
    
            // Update the database
            let change_type = self.handle_item(ItemType::Directory, q_entry.path.as_path(), &q_entry.metadata)?;
            self.update_change_counts(change_type);
    
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
                    self.handle_item(item_type, &entry.path(), &metadata)?;
                }
            }
        }
        self.end_scan()?;
    
        Ok(())
    }
    
    fn validate_and_resolve_path(user_path: &Path) -> Result<PathBuf, DirCheckError> {
        if user_path.as_os_str().is_empty() {
            return Err(DirCheckError::Error("Provided path is empty".to_string()));
        }
    
        let absolute_path = if user_path.is_absolute() {
            user_path.to_owned()
        }  else {
            env::current_dir()?.join(user_path)
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
    
        Ok(absolute_path)
    }

    fn begin_scan(&mut self, root_path: &str) -> Result<(), DirCheckError> {
        let conn = &mut self.db.conn;

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

        // Store it in the struct
        self.root_path_id = Some(root_path_id);

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

        // Store it in the struct
        self.current_scan_id = Some(scan_id);

        Ok(())
    }

    fn end_scan(&mut self) -> Result<(), DirCheckError> {
        let root_path_id = self.root_path_id.ok_or_else(|| DirCheckError::Error("No root path ID set".to_string()))?;
        let scan_id = self.current_scan_id.ok_or_else(|| DirCheckError::Error("No active scan".to_string()))?;

        let conn = &mut self.db.conn;
    
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
    
        // Reset current scan ID since scan is complete
        self.current_scan_id = None;
    
        Ok(())
    }

    fn handle_item(&mut self, item_type: ItemType, path: &Path, metadata: &Metadata) -> Result<ChangeType, DirCheckError> {
        let root_path_id = self.root_path_id.ok_or_else(|| DirCheckError::Error("No root path ID set".to_string()))?;
        let current_scan_id = self.current_scan_id.ok_or_else(|| DirCheckError::Error("No active scan".to_string()))?;
        let path_str = path.to_string_lossy();

        let conn = &mut self.db.conn;

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
                        (item_type_str, last_modified, file_size, current_scan_id, entry_id))?;
                    tx.execute("INSERT INTO changes (scan_id, entry_id, change_type) VALUES (?, ?, ?)", 
                        (current_scan_id, entry_id, ChangeType::Add.as_db_str()))?;
                    change_type = Some(ChangeType::Add);
                    tx.commit()?;
                } else if existing_type != item_type_str {
                    // Item type changed (e.g., file -> directory)
                    let tx = conn.transaction()?;
                    tx.execute("UPDATE entries SET item_type = ?, last_modified = ?, file_size = ?, last_seen_scan_id = ? WHERE id = ?", 
                        (item_type_str, last_modified, file_size, current_scan_id, entry_id))?;
                    tx.execute("INSERT INTO changes (scan_id, entry_id, change_type) VALUES (?, ?, ?)", 
                        (current_scan_id, entry_id, ChangeType::TypeChange.as_db_str()))?;
                    change_type = Some(ChangeType::TypeChange);
                    tx.commit()?;
                } else if existing_modified != last_modified || existing_size != file_size {
                    // Item content changed
                    let tx = conn.transaction()?;
                    tx.execute("UPDATE entries SET last_modified = ?, file_size = ?, last_seen_scan_id = ? WHERE id = ?", 
                        (last_modified, file_size, current_scan_id, entry_id))?;
                    tx.execute("INSERT INTO changes (scan_id, entry_id, change_type) VALUES (?, ?, ?)", 
                        (current_scan_id, entry_id, ChangeType::Modify.as_db_str()))?;
                    change_type = Some(ChangeType::Modify);
                    tx.commit()?;
                } else {
                    // No change, just update last_seen_scan_id
                    conn.execute("UPDATE entries SET last_seen_scan_id = ? WHERE root_path_id = ? AND id = ?", 
                        (current_scan_id, root_path_id, entry_id))?;
                    change_type = Some(ChangeType::NoChange);
                }
            }
            None => {
                // Item is new, insert into entries and changes tables
                let tx = conn.transaction()?;
                tx.execute("INSERT INTO entries (root_path_id, path, item_type, last_modified, file_size, last_seen_scan_id) VALUES (?, ?, ?, ?, ?, ?)",
                    (root_path_id, &path_str, item_type.as_db_str(), last_modified, file_size, current_scan_id))?;
                let entry_id: i64 = tx.query_row("SELECT last_insert_rowid()", [], |row| row.get(0))?;
                tx.execute("INSERT INTO changes (scan_id, entry_id, change_type) VALUES (?, ?, ?)",
                    (current_scan_id, entry_id, ChangeType::Add.as_db_str()))?;
                change_type = Some(ChangeType::Add);
                tx.commit()?;
            }
        }
        
        change_type.ok_or(DirCheckError::Error("Expected a change type, but found None".to_string()))
    }

    fn update_change_counts(&mut self, change_type: ChangeType) {
        match change_type {
            ChangeType::Add => self.change_counts.add_count += 1,
            ChangeType::Modify => self.change_counts.modify_count += 1,
            ChangeType::Delete => self.change_counts.delete_count += 1,
            ChangeType::TypeChange => self.change_counts.type_change_count += 1,
            ChangeType::NoChange => self.change_counts.unchanged_count += 1,
        }
    }

    fn print_scan_results(&self) {
        println!("Scan Results: {}", self.absolute_path.display());
        println!("-------------");
        println!("{:<12} {}", "Added:", self.change_counts.add_count);
        println!("{:<12} {}", "Modified:", self.change_counts.modify_count);
        println!("{:<12} {}", "Deleted:", self.change_counts.delete_count);
        println!("{:<12} {}", "Type Change:", self.change_counts.type_change_count);
        println!("{:<12} {}", "No Change:", self.change_counts.unchanged_count);
        println!();
    }
}