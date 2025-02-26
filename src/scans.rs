use crate::changes::{ ChangeCounts, ChangeType };
use crate::error::DirCheckError;
use crate::database::{ Database, ItemType };
use crate::hash::Hash;
use crate::root_paths::RootPath;
use crate::utils::Utils;

use rusqlite::{ OptionalExtension, Result, params };

use std::collections::VecDeque;
use std::env;
use std::fs;
use std::fs::Metadata;
use std::path::Path;
use std::path::PathBuf;

#[derive(Clone, Debug, Default)]
pub struct Scan {
    // Schema fields
    id: i64,
    root_path_id: i64,
    is_deep: bool,
    time_of_scan: i64,
    file_count: Option<i64>,
    folder_count: Option<i64>,
    is_complete: bool,   

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
    fn new_for_scan(id: i64, root_path_id: i64, is_deep: bool, time_of_scan: i64) -> Self {
        Scan {
            id,
            root_path_id,
            is_deep,
            time_of_scan,
            ..Default::default()
        }
    }

    pub fn new_from_id(db: &Database, id: Option<i64>) -> Result<Self, DirCheckError> {
        let conn = &db.conn;

        // If the scan id wasn't explicitly specified, load the most recent otherwise,
        // load the specified scan
        let scan_row: Option<(i64, i64, bool, i64, Option<i64>, Option<i64>, bool)> = conn.query_row(
            "SELECT id, root_path_id, is_deep, time_of_scan, file_count, folder_count, is_complete
                FROM scans
                WHERE id = COALESCE(?1, (SELECT MAX(id) FROM scans))",
            params![id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?, row.get(5)?, row.get(6)?)),
        ).optional()?;

        let (id, root_path_id, is_deep, time_of_scan, file_count, folder_count, is_complete) = scan_row.ok_or_else(|| {
            DirCheckError::Error("No scan found".to_string())
        })?;

        //let root_path: RootPath = RootPath::get(db, root_path_id)?;

        let change_counts = ChangeCounts::from_scan_id(db, id)?;

        Ok(Scan {
            id,
            root_path_id,
            is_deep,
            time_of_scan,
            file_count,
            folder_count,
            is_complete,
            change_counts,
            ..Default::default()
        })
    }

    pub fn id(&self) -> i64 {
        self.id
    }

    pub fn root_path_id(&self) -> i64 {
        self.root_path_id
    }

    pub fn is_deep(&self) -> bool {
        self.is_deep
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

    pub fn is_complete(&self) -> bool {
        self.is_complete
    }

    pub fn change_counts(&self) -> &ChangeCounts {
        &self.change_counts
    }

    // TODO: Is this dead code?
    /*
    pub fn with_id_or_latest<F>(db: &Database, id: Option<i64>, func: F) -> Result<(), DirCheckError>
    where
        F: FnOnce(&Database, &Scan) -> Result<(), DirCheckError>,
    {
        let scan = Scan::new_from_id(db, id)?;

        func(db, &scan)
    }
    */

    pub fn do_scan(db: &mut Database, path_arg: String, deep: bool) -> Result<Scan, DirCheckError> {
        let (scan, _root_path) = Scan::scan_directory(db, path_arg, deep)?;
        scan.print_scan_results();

        Ok(scan)
    }

    fn print_scan_results(&self) {
        println!("Scan Complete");
        println!("Id:             {}", self.id);
        println!("Root Path ID:   {}", self.root_path_id);
        println!("Deep Scan:      {}", self.is_deep);
        println!("File Count:     {}", Utils::opt_i64_or_none_as_str(self.file_count));
        println!("Folder Count:   {}", Utils::opt_i64_or_none_as_str(self.folder_count));

        println!("\nChanges");
        let change_counts = self.change_counts();
        println!("Add             {}", change_counts.get(ChangeType::Add));
        println!("Modify          {}", change_counts.get(ChangeType::Modify));
        println!("Delete          {}", change_counts.get(ChangeType::Delete));
        println!("Type Change     {}", change_counts.get(ChangeType::TypeChange));
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

    fn scan_directory(db: &mut Database, path_arg: String, deep: bool) -> Result<(Self, RootPath), DirCheckError> {
        let (mut scan, root_path) = Self::begin_scan(db, path_arg, deep)?;
        let root_path_buf = PathBuf::from(root_path.path());
        let metadata = fs::symlink_metadata(&root_path_buf)?;

        let mut q = VecDeque::new();
    
        q.push_back(QueueEntry {
            path: root_path_buf.clone(),
            metadata,
        });
    
        while let Some(q_entry) = q.pop_front() {
            // Update the database
            if q_entry.path != root_path_buf {
                let dir_change_type = scan.handle_item(db, ItemType::Directory, q_entry.path.as_path(), &q_entry.metadata, None)?;
                scan.change_counts.increment(dir_change_type);
            }
    
            let items = fs::read_dir(&q_entry.path)?;
    
            for item in items {
                let item = item?;
                let metadata = fs::symlink_metadata(item.path())?; // Use symlink_metadata to check for symlinks
    
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
                    if deep && item_type == ItemType::File {
                        hash = match Hash::compute_md5_hash(&item.path()) {
                            Ok(hash_s) => Some(hash_s),
                            Err(error) => {
                                eprintln!("Error computing hash: {}", error);
                                None
                            }
                        }
                    };

                    let file_change_type = scan.handle_item(db, item_type, &item.path(), &metadata, hash.as_deref())?;
                    scan.change_counts.increment(file_change_type);
                }
            }
        }
        scan.end_scan(db)?;
    
        Ok((scan, root_path))
    }

    fn begin_scan(db: &mut Database, path_arg: String, deep: bool) -> Result<(Self, RootPath), DirCheckError> {
        let path_canonical = Self::path_arg_to_canonical_path_buf(&path_arg)?;
        let root_path_str = path_canonical.to_string_lossy().to_string();

        let root_path = RootPath::get_or_insert(db, &root_path_str)?;
        let root_path_id = root_path.id();

        let (scan_id, time_of_scan): (i64, i64) = db.conn.query_row(
            "INSERT INTO scans (root_path_id, is_deep, time_of_scan) 
             VALUES (?, ?, strftime('%s', 'now', 'utc')) 
             RETURNING id, time_of_scan",
            [root_path.id(), deep as i64],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;

        let scan = Scan::new_for_scan(scan_id, root_path_id, deep, time_of_scan);
        Ok((scan, root_path))
    }

    fn handle_item(
        &mut self, 
        db: &mut Database,
        item_type: ItemType, 
        path: &Path, 
        metadata: &Metadata, 
        file_hash: Option<&str>
    ) -> Result<ChangeType, DirCheckError> {
        let path_str = path.to_string_lossy();
        let scan_id = self.id;
        let root_path_id = self.root_path_id;

        let conn = &mut db.conn;
    
        // Determine timestamps and file size
        let last_modified = metadata.modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs() as i64);
        let file_size = if metadata.is_file() { Some(metadata.len() as i64) } else { None };
    
        // Check if the item already exists (fetching `id`, `is_tombstone` as well)
        let existing_item: Option<(i64, String, Option<i64>, Option<i64>, Option<String>, bool)> = conn.query_row(
            "SELECT id, item_type, last_modified, file_size, file_hash, is_tombstone FROM items WHERE root_path_id = ? AND path = ?",
            (root_path_id, &path_str),
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2).ok(), row.get(3).ok(), row.get(4)?, row.get(5)?)),
        ).optional()?;
    
        let change_type = match existing_item {
            Some((item_id, existing_type, existing_modified, existing_size, existing_hash, is_tombstone)) => {
                let item_type_str = item_type.as_db_str();
                let metadata_changed = existing_modified != last_modified || existing_size != file_size;

                // println!("{}", Utils::string_value_or_none(&existing_hash));
                // println!("{}", Utils::str_value_or_none(&file_hash));
                // println!();

                let hash_changed = file_hash.map_or(false, |h| Some(h) != existing_hash.as_deref());
            
                if is_tombstone {
                    let tx = conn.transaction()?;
                    tx.execute("UPDATE items SET item_type = ?, last_modified = ?, file_size = ?, file_hash = ?, last_seen_scan_id = ?, is_tombstone = 0 WHERE id = ?", 
                        (item_type_str, last_modified, file_size, file_hash, scan_id, item_id))?;
                    tx.execute("INSERT INTO changes (scan_id, item_id, change_type) VALUES (?, ?, ?)", 
                        (scan_id, item_id, ChangeType::Add.as_db_str()))?;
                    tx.commit()?;
                    ChangeType::Add
                } else if existing_type != item_type_str {
                    // Item type changed (e.g., file -> directory)
                    let tx = conn.transaction()?;
                    tx.execute("UPDATE items SET item_type = ?, last_modified = ?, file_size = ?, file_hash = ?, last_seen_scan_id = ? WHERE id = ?", 
                        (item_type_str, last_modified, file_size, file_hash, scan_id, item_id))?;
                    tx.execute("INSERT INTO changes (scan_id, item_id, change_type) VALUES (?, ?, ?)", 
                        (self.id, item_id, ChangeType::TypeChange.as_db_str()))?;
                    tx.commit()?;
                    ChangeType::TypeChange
                } else if metadata_changed || hash_changed {
                    // Item content changed
                    let tx = conn.transaction()?;
                    tx.execute("UPDATE items 
                        SET last_modified = ?, 
                        file_size = ?,
                        file_hash = ?,
                        last_seen_scan_id = ? 
                        WHERE id = ?", 
                        (last_modified, file_size, file_hash.or(existing_hash.as_deref()), scan_id, item_id))?;
                    tx.execute("INSERT INTO changes 
                        (scan_id, item_id, change_type, metadata_changed, hash_changed, prev_last_modified, prev_file_size, prev_hash) 
                        VALUES (?, ?, ?, ?, ?, ?, ?, ?)", 
                        (
                            scan_id, 
                            item_id, 
                            ChangeType::Modify.as_db_str(),
                            metadata_changed,
                            hash_changed,
                            metadata_changed.then_some(existing_modified),
                            metadata_changed.then_some(existing_size),
                            hash_changed.then_some(existing_hash),
                        ))?;
                    tx.commit()?;
                    ChangeType::Modify
                } else {
                    // No change, just update last_seen_scan_id
                    conn.execute("UPDATE items SET last_seen_scan_id = ? WHERE root_path_id = ? AND id = ?", 
                        (scan_id, root_path_id, item_id))?;
                    ChangeType::NoChange
                }
            }
            None => {
                // Item is new, insert into items and changes tables
                let tx = conn.transaction()?;
                tx.execute("INSERT INTO items (root_path_id, path, item_type, last_modified, file_size, file_hash, last_seen_scan_id) VALUES (?, ?, ?, ?, ?, ?, ?)",
                    (root_path_id, &path_str, item_type.as_db_str(), last_modified, file_size, file_hash, scan_id))?;
                let item_id: i64 = tx.query_row("SELECT last_insert_rowid()", [], |row| row.get(0))?;
                tx.execute("INSERT INTO changes (scan_id, item_id, change_type) VALUES (?, ?, ?)",
                    (scan_id, item_id, ChangeType::Add.as_db_str()))?;
                tx.commit()?;
                ChangeType::Add
            }
        };
        
        Ok(change_type)
    }

    fn end_scan(&mut self, db: &mut Database) -> Result<(), DirCheckError> {
        let root_path_id = self.root_path_id;
        let scan_id = self.id;

        let conn = &mut db.conn;
    
        let tx = conn.transaction()?;
    
        // Insert deletion records into changes
        tx.execute(
            "INSERT INTO changes (scan_id, item_id, change_type)
             SELECT ?, id, ?
             FROM items
             WHERE root_path_id = ? AND is_tombstone = 0 AND last_seen_scan_id < ?",
            (scan_id, ChangeType::Delete.as_db_str(), root_path_id, scan_id),
        )?;
        
        // Mark unseen items as tombstones
        tx.execute(
            "UPDATE items SET is_tombstone = 1 WHERE root_path_id = ? AND last_seen_scan_id < ? AND is_tombstone = 0",
            (root_path_id, scan_id),
        )?;

        // Step 3: Count total files and directories seen in this scan
        let (file_count, folder_count): (i64, i64) = tx.query_row(
        "SELECT 
            SUM(CASE WHEN item_type = 'F' THEN 1 ELSE 0 END) AS file_count, 
            SUM(CASE WHEN item_type = 'D' THEN 1 ELSE 0 END) AS folder_count 
            FROM items WHERE last_seen_scan_id = ?",
            [scan_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        ).unwrap_or((0, 0)); // If no data, default to 0

        // Update the scan entity to indicate that it completed
        tx.execute(
            "UPDATE scans SET file_count = ?, folder_count = ?, is_complete = 1 WHERE id = ?",
            (file_count, folder_count, scan_id)
        )?;

        tx.commit()?;

        self.file_count = Some(file_count);
        self.folder_count = Some(folder_count);
        self.is_complete = true;

        // scan.change_counts acts as an accumulator during a scan but now we get the truth from the
        // database. We need this to include deletes since they aren't known until tombstoning is complete
        self.change_counts = ChangeCounts::from_scan_id(db, self.id)?;

        Ok(())
    }

    pub fn for_each_scan<F>(db: &Database, num_scans: Option<i64>, mut func: F) -> Result<i32, DirCheckError> 
    where
        F: FnMut(&Database, &Scan) -> Result<(), DirCheckError>,
    {
        if num_scans == Some(0) {
            return Ok(0);
        }

        let num_scans = num_scans.unwrap_or(10);
        
        let mut stmt = db.conn.prepare(
            "SELECT 
                s.id,
                s.is_deep,
                s.time_of_scan,
                s.file_count,
                s.folder_count, 
                s.is_complete,
                s.root_path_id,
                COALESCE(SUM(CASE WHEN c.change_type = 'A' THEN 1 ELSE 0 END), 0) AS add_count,
                COALESCE(SUM(CASE WHEN c.change_type = 'M' THEN 1 ELSE 0 END), 0) AS modify_count,
                COALESCE(SUM(CASE WHEN c.change_type = 'D' THEN 1 ELSE 0 END), 0) AS delete_count,
                COALESCE(SUM(CASE WHEN c.change_type = 'T' THEN 1 ELSE 0 END), 0) AS type_change_count
            FROM scans s
            LEFT JOIN changes c ON s.id = c.scan_id
            GROUP BY s.id, s.is_deep, s.time_of_scan, s.file_count, s.folder_count, s.is_complete, s.root_path_id
            ORDER BY s.id DESC
            LIMIT ?"
        )?;

        let rows = stmt.query_map([num_scans], |row| {
            Ok(Scan {
                id: row.get::<_, i64>(0)?,                              // scan id
                is_deep: row.get::<_, bool>(1)?,                        // is deep
                time_of_scan: row.get::<_, i64>(2)?,                    // time of scan
                file_count: row.get::<_, Option<i64>>(3)?,              // file count
                folder_count: row.get::<_, Option<i64>>(4)?,            // folder count
                is_complete: row.get::<_, bool>(5)?,                    // is complete
                root_path_id: row.get::<_, i64>(6)?,                    // root path id
                change_counts: ChangeCounts::new(  
                    row.get::<_, i64>(7)?,             // adds
                    row.get::<_, i64>(8)?,          // modifies
                    row.get::<_, i64>(9)?,          // deletes
                    row.get::<_, i64>(10)?,    // type changes
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