
/*
CREATE TABLE IF NOT EXISTS items (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    root_path_id INTEGER NOT NULL,    -- Links each item to a root path
    path TEXT NOT NULL,               -- Relative path from the root path
    is_tombstone BOOLEAN NOT NULL DEFAULT 0,  -- Indicates if the item was deleted
    item_type CHAR(1) NOT NULL,       -- ('F' for file, 'D' for directory, 'S' for symlink, 'O' for other)
    last_modified INTEGER,            -- Last modified timestamp
    file_size INTEGER,                -- File size in bytes (NULL for directories)
    file_hash TEXT,                    -- Hash of file contents (NULL for directories and if not computed)
    last_seen_scan_id INTEGER NOT NULL, -- Last scan where the item was present
    FOREIGN KEY (root_path_id) REFERENCES root_paths(id),
    FOREIGN KEY (last_seen_scan_id) REFERENCES scans(id),
    UNIQUE (root_path_id, path)        -- Ensures uniqueness within each root path
);
*/

use rusqlite::{self, params, OptionalExtension};

use crate::{database::Database, error::DirCheckError};

#[derive(Clone, Debug, Default)]
pub struct Item {           // TODO: Change sql schema to have this column order
    id: i64,
    root_path_id: i64,
    last_seen_scan_id: i64,
    is_tombstone: bool,
    item_type: String,
    path: String,
    last_modified: Option<i64>,
    file_size: Option<i64>,
    file_hash: Option<String>,
    
}

impl Item {
    pub fn new(db: &Database, id: i64) -> Result<Option<Self>, DirCheckError> {
        let conn = &db.conn;

        

        match conn.query_row(
            "SELECT id, root_path_id, path, is_tombstone, item_type, last_modified, file_size, file_hash, last_seen_scan_id
             FROM items
             WHERE id = ?",
            params![id],
            |row| Ok(Item {
                id: row.get(0)?,
                root_path_id: row.get(1)?,
                path: row.get(2)?,
                is_tombstone: row.get(3)?,
                item_type: row.get(4)?,
                last_modified: row.get(5)?,
                file_size: row.get(6)?,
                file_hash: row.get(7)?,
                last_seen_scan_id: row.get(8)?,
            }),
        ).optional()? {
            Some(item) => Ok(Some(item)),
            None => Ok(None),
        }
    }

    pub fn id(&self) -> i64 { self.id }
    pub fn root_path_id(&self) -> i64 { self.root_path_id }
    pub fn last_seen_scan_id(&self) -> i64 { self.last_seen_scan_id }
    pub fn is_tombstone(&self) -> bool { self.is_tombstone }
    pub fn item_type(&self) -> &str { &self.item_type }
    pub fn path(&self) -> &str { &self.path }
    pub fn last_modified(&self) -> Option<i64> { self.last_modified }
    pub fn file_size(&self) -> Option<i64> { self.file_size }
    pub fn file_hash(&self) -> Option<&str> { self.file_hash.as_deref() }
    



}