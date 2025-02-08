use rusqlite::{Connection, OptionalExtension, Result};
use std::fs::Metadata;
use std::{io, path::Path};
use crate::error::DirCheckError;
use crate::schema::CREATE_SCHEMA_SQL;

const DB_FILENAME: &str = "dircheck.db";
const SCHEMA_VERSION: &str = "1";

#[derive(Debug, PartialEq)]
pub enum ItemType {
    File,
    Directory,
    Symlink,
    Other,
}

impl ItemType {
    pub fn as_db_str(&self) -> &'static str {
        match self {
            ItemType::File => "F",
            ItemType::Directory => "D",
            ItemType::Symlink => "S",
            ItemType::Other => "O",
        }
    }
}

#[derive(Debug, PartialEq)]
pub enum ChangeType {
    Add,
    Delete,
    Modify,
    TypeChange,
    NoChange,
}

impl ChangeType {
    pub fn as_db_str(&self) -> &'static str {
        match self {
            ChangeType::Add => "A",
            ChangeType::Delete => "D",
            ChangeType::Modify => "M",
            ChangeType::TypeChange => "T",
            ChangeType::NoChange => "N"
        }
    }
}

pub struct Database {
    conn: Connection,
    root_path_id: Option<i64>,
    current_scan_id: Option<i64>,
}

impl Database {
    pub fn connect(db_folder: &str) -> Result<Self, DirCheckError> {
        let folder_path = Path::new(db_folder);
        
        // Ensure the folder exists and is a directory
        if !folder_path.is_dir() {
            return Err(io::Error::new(
                io::ErrorKind::NotADirectory, 
                format!("Database folder '{}' does not exist or is not a directory", db_folder)
            ).into());
        }

        let db_path = folder_path.join(DB_FILENAME);

        // Attempt to open the database
        let conn = Connection::open(&db_path).map_err(DirCheckError::Database)?;
        println!("Database opened at: {}", db_path.display());
        let db = Self { conn, root_path_id: None, current_scan_id: None };
        
        // Ensure schema is current
        db.ensure_schema()?;

        Ok(db)
    }

    fn ensure_schema(&self) -> Result<(), DirCheckError> {
        let table_exists: bool = self.conn
            .query_row(
                "SELECT count(*) FROM sqlite_master WHERE type='table' AND name='meta'",
                [],
                |row| row.get::<_, i32>(0),
            )
            .map(|count| count > 0)
            .unwrap_or(false);

        if !table_exists {
            return self.create_schema();
        }

        // Get the stored schema version
        let stored_version: Option<String> = self.conn
            .query_row(
                "SELECT value FROM meta WHERE key = 'schema_version'",
                [],
                |row| row.get(0),
            )
            .optional()?;

            match stored_version.as_deref() {
                Some(SCHEMA_VERSION) => Ok(()), // Schema is up to date
                Some(_) => Err(DirCheckError::Error("Schema version mismatch".to_string())),
                None => Err(DirCheckError::Error("Schema version missing".to_string())),
            }
    }
    
    fn create_schema(&self) -> Result<(), DirCheckError> {
        self.conn.execute_batch(CREATE_SCHEMA_SQL)?;
        Ok(())
    }

    pub fn begin_scan(&mut self, root_path: &str) -> Result<(), DirCheckError> {
        //let path_str = root_path.to_string_lossy();

        self.conn.execute(
            "INSERT OR IGNORE INTO root_paths (path) VALUES (?)", 
            [&root_path]
        )?;

        // Get the root_path_id
        let root_path_id: i64 = self.conn.query_row(
            "SELECT id FROM root_paths WHERE path = ?",
            [&root_path],
            |row| row.get(0),
        )?;

        // Store it in the struct
        self.root_path_id = Some(root_path_id);

        // Insert into scans table with UTC timestamp
        self.conn.execute(
            "INSERT INTO scans (root_path_id, scan_time) VALUES (?, strftime('%s', 'now', 'utc'))",
            [root_path_id],
        )?;

        // Get the new scan_id
        let scan_id: i64 = self.conn.query_row(
            "SELECT last_insert_rowid()",
            [],
            |row| row.get(0),
        )?;

        // Store it in the struct
        self.current_scan_id = Some(scan_id);

        Ok(())
    }

    pub fn handle_item(&mut self, item_type: ItemType, path: &Path, metadata: &Metadata) -> Result<ChangeType, DirCheckError> {
        let root_path_id = self.root_path_id.ok_or_else(|| DirCheckError::Error("No root path ID set".to_string()))?;
        let current_scan_id = self.current_scan_id.ok_or_else(|| DirCheckError::Error("No active scan".to_string()))?;
        let path_str = path.to_string_lossy();

        let mut change_type: Option<ChangeType> = None;
    
        // Determine timestamps and file size
        let last_modified = metadata.modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs() as i64);
        let file_size = if metadata.is_file() { Some(metadata.len() as i64) } else { None };
    
        // Check if the entry already exists (fetching `id`, `is_tombstone` as well)
        let existing_entry: Option<(i64, String, Option<i64>, Option<i64>, bool)> = self.conn.query_row(
            "SELECT id, item_type, last_modified, file_size, is_tombstone FROM entries WHERE root_path_id = ? AND path = ?",
            (root_path_id, &path_str),
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2).ok(), row.get(3).ok(), row.get(4)?)),
        ).optional()?;
    
        match existing_entry {
            Some((entry_id, existing_type, existing_modified, existing_size, is_tombstone)) => {
                let item_type_str = item_type.as_db_str();
                
                if is_tombstone {
                    // Item previously deleted - resurrect it
                    let tx = self.conn.transaction()?;
                    tx.execute("UPDATE entries SET item_type = ?, last_modified = ?, file_size = ?, last_seen_scan_id = ?, is_tombstone = 0 WHERE id = ?", 
                        (item_type_str, last_modified, file_size, current_scan_id, entry_id))?;
                    tx.execute("INSERT INTO changes (scan_id, entry_id, change_type) VALUES (?, ?, ?)", 
                        (current_scan_id, entry_id, ChangeType::Add.as_db_str()))?;
                    change_type = Some(ChangeType::Add);
                    tx.commit()?;
                } else if existing_type != item_type_str {
                    // Item type changed (e.g., file -> directory)
                    let tx = self.conn.transaction()?;
                    tx.execute("UPDATE entries SET item_type = ?, last_modified = ?, file_size = ?, last_seen_scan_id = ? WHERE id = ?", 
                        (item_type_str, last_modified, file_size, current_scan_id, entry_id))?;
                    tx.execute("INSERT INTO changes (scan_id, entry_id, change_type) VALUES (?, ?, ?)", 
                        (current_scan_id, entry_id, ChangeType::TypeChange.as_db_str()))?;
                    change_type = Some(ChangeType::TypeChange);
                    tx.commit()?;
                } else if existing_modified != last_modified || existing_size != file_size {
                    // Item content changed
                    let tx = self.conn.transaction()?;
                    tx.execute("UPDATE entries SET last_modified = ?, file_size = ?, last_seen_scan_id = ? WHERE id = ?", 
                        (last_modified, file_size, current_scan_id, entry_id))?;
                    tx.execute("INSERT INTO changes (scan_id, entry_id, change_type) VALUES (?, ?, ?)", 
                        (current_scan_id, entry_id, ChangeType::Modify.as_db_str()))?;
                    change_type = Some(ChangeType::Modify);
                    tx.commit()?;
                } else {
                    // No change, just update last_seen_scan_id
                    self.conn.execute("UPDATE entries SET last_seen_scan_id = ? WHERE root_path_id = ? AND id = ?", 
                        (current_scan_id, root_path_id, entry_id))?;
                    change_type = Some(ChangeType::NoChange);
                }
            }
            None => {
                // Item is new, insert into entries and changes tables
                let tx = self.conn.transaction()?;
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

    pub fn end_scan(&mut self) -> Result<(), DirCheckError> {
        let root_path_id = self.root_path_id.ok_or_else(|| DirCheckError::Error("No root path ID set".to_string()))?;
        let scan_id = self.current_scan_id.ok_or_else(|| DirCheckError::Error("No active scan".to_string()))?;
    
        let tx = self.conn.transaction()?;
    
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
}
    