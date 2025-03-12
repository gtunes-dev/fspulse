use rusqlite::{self, params, OptionalExtension};

use crate::{database::Database, error::FsPulseError};

#[derive(Debug, PartialEq)]
pub enum ItemType {
    File,
    Directory,
    Symlink,
    Other,
}

impl ItemType {
    pub fn as_str(&self) -> &'static str {
        match self {
            ItemType::File => "F",
            ItemType::Directory => "D",
            ItemType::Symlink => "S",
            ItemType::Other => "O",
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct Item {           // TODO: Change sql schema to have this column order
    id: i64,
    root_id: i64,
    last_seen_scan_id: i64,
    is_tombstone: bool,
    item_type: String,
    path: String,
    last_modified: Option<i64>,
    file_size: Option<i64>,
    file_hash: Option<String>,
    
}

impl Item {
    pub fn get_by_id(db: &Database, id: i64) -> Result<Option<Self>, FsPulseError> {
        let conn = &db.conn;

        conn.query_row(
            "SELECT id, root_id, path, is_tombstone, item_type, last_modified, file_size, file_hash, last_seen_scan_id
             FROM items
             WHERE id = ?",
            params![id],
            |row| Ok(Item {
                id: row.get(0)?,
                root_id: row.get(1)?,
                path: row.get(2)?,
                is_tombstone: row.get(3)?,
                item_type: row.get(4)?,
                last_modified: row.get(5)?,
                file_size: row.get(6)?,
                file_hash: row.get(7)?,
                last_seen_scan_id: row.get(8)?,
            }),
        )
        .optional()
        .map_err(FsPulseError::Database)
    }

    pub fn id(&self) -> i64 { self.id }
    pub fn root_id(&self) -> i64 { self.root_id }
    pub fn last_seen_scan_id(&self) -> i64 { self.last_seen_scan_id }
    pub fn is_tombstone(&self) -> bool { self.is_tombstone }
    pub fn item_type(&self) -> &str { &self.item_type }
    pub fn path(&self) -> &str { &self.path }
    pub fn last_modified(&self) -> Option<i64> { self.last_modified }
    pub fn file_size(&self) -> Option<i64> { self.file_size }
    pub fn file_hash(&self) -> Option<&str> { self.file_hash.as_deref() }

    pub fn for_each_item_in_latest_scan<F>(db: &Database, scan_id: i64, mut func: F) -> Result<(), FsPulseError>
    where
        F: FnMut(&Item) -> Result<(), FsPulseError>,
    {
        let mut item_count = 0;

        let mut stmt = db.conn.prepare(
            "SELECT id, root_id, last_seen_scan_id, is_tombstone, item_type, path, last_modified, file_size, file_hash
             FROM items
             WHERE last_seen_scan_id = ?
             ORDER BY path ASC"
        )?;

        let rows = stmt.query_map([scan_id], |row| {
            Ok(Item {
                id: row.get::<_, i64>(0)?,
                root_id: row.get::<_, i64>(1)?,
                last_seen_scan_id: row.get::<_, i64>(2)?,
                is_tombstone: row.get::<_, bool>(3)?,
                item_type: row.get::<_, String>(4)?,
                path: row.get::<_, String>(5)?,
                last_modified: row.get::<_, Option<i64>>(6)?,
                file_size: row.get::<_, Option<i64>>(7)?,
                file_hash: row.get::<_, Option<String>>(8)?,
            })
        })?;
        
        for row in rows {
            let item = row?;
            func(&item)?;
            item_count = item_count + 1;
        }
        Ok(())
    }
}