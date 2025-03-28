use rusqlite::{self, params, OptionalExtension};

use crate::{database::Database, error::FsPulseError, validators::validator::ValidationState};

#[derive(Copy,Clone, Debug, PartialEq)]
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
    path: String,
    item_type: String,
    is_tombstone: bool,
    last_modified: Option<i64>,
    file_size: Option<i64>,
    file_hash: Option<String>,
    validation_state: String,
    validation_state_desc: Option<String>,
    last_scan_id: i64,
    last_hash_scan_id: Option<i64>,
    last_validation_scan_id: Option<i64>
}

impl Item {
    const ITEM_COLUMNS: &str = "id, root_id, path, item_type, is_tombstone, last_modified, file_size, file_hash, validation_state, validation_state_desc, last_scan_id, last_hash_scan_id, last_validation_scan_id";
    
    fn from_row(row: &rusqlite::Row) -> rusqlite::Result<Self> {
        Ok(Item {
            id: row.get(0)?,
            root_id: row.get(1)?,
            path: row.get(2)?,
            item_type: row.get(3)?,
            is_tombstone: row.get(4)?,
            last_modified: row.get(5)?,
            file_size: row.get(6)?,
            file_hash: row.get(7)?,
            validation_state: row.get(8)?,
            validation_state_desc: row.get(9)?,
            last_scan_id: row.get(10)?,
            last_hash_scan_id: row.get(11)?,
            last_validation_scan_id: row.get(12)?,
        })
    }

    pub fn get_by_id(db: &Database, id: i64) -> Result<Option<Self>, FsPulseError> {
        
        let query = format!("SELECT {} FROM ITEMS WHERE id = ?", Item::ITEM_COLUMNS);
        db.conn().query_row(
            &query,
            params![id],
            |row| Item::from_row(row),
        )
        .optional()
        .map_err(FsPulseError::Database)
    }

    pub fn get_by_root_and_path(
        db: &Database, 
        root_id: i64, 
        path: &str
    ) -> Result<Option<Self>, FsPulseError> 
    {
        let query = format!("SELECT {} FROM ITEMS WHERE root_id = ? AND path = ?", Item::ITEM_COLUMNS);

        db.conn().query_row(
            &query,
            params![root_id, path],
            |row| Item::from_row(row),
        )
        .optional()
        .map_err(FsPulseError::Database)
    }

    pub fn id(&self) -> i64 { self.id }
    pub fn root_id(&self) -> i64 { self.root_id }
    pub fn path(&self) -> &str { &self.path }
    pub fn item_type(&self) -> &str { &self.item_type }
    pub fn is_tombstone(&self) -> bool { self.is_tombstone }
    pub fn last_modified(&self) -> Option<i64> { self.last_modified }
    pub fn file_size(&self) -> Option<i64> { self.file_size }
    pub fn file_hash(&self) -> Option<&str> { self.file_hash.as_deref() }
    pub fn validation_state(&self) -> ValidationState { ValidationState::from_string(&self.validation_state) }
    pub fn validation_state_desc(&self) -> Option<&str> {self.validation_state_desc.as_deref()}
    pub fn last_scan_id(&self) -> i64 { self.last_scan_id }
    pub fn last_hash_scan_id(&self) -> Option<i64> { self.last_hash_scan_id }
    pub fn last_validation_scan_id(&self) -> Option<i64> { self.last_validation_scan_id }

    pub fn count_analyzed_items(db: &Database, scan_id: i64) -> Result<i64, FsPulseError> {
        let mut stmt = db.conn().prepare(
            "SELECT COUNT(*) FROM items
             WHERE
                last_scan_id = ? AND
                item_type = 'F' AND
                (last_hash_scan_id = ? OR last_validation_scan_id = ?)"
        )?;
    
        let count: i64 = stmt.query_row([scan_id, scan_id, scan_id], |row| row.get(0))?;
        Ok(count)
    }

    pub fn fetch_next_analysis_batch(
        db: &Database,
        scan_id: i64,
        hashing: bool,
        validating: bool,
        last_item_id: i64,
        limit: usize,  // Parameterized limit
    ) -> Result<Vec<Item>, FsPulseError> {

        let query = format!(
            "SELECT {}
            FROM items
             WHERE
                    last_scan_id = ?
                AND
                    item_type = 'F'
                AND 
                    ((? = 1 AND (last_hash_scan_id IS NULL OR last_hash_scan_id < ?))
                    OR 
                    (? = 1 AND (last_validation_scan_id IS NULL OR last_validation_scan_id < ?)))
                AND
                    id > ?
             ORDER BY id ASC
             LIMIT {}",
            Item::ITEM_COLUMNS, limit
        );
    
        let mut stmt = db.conn().prepare(&query)?;
    
        let rows = stmt.query_map([scan_id, hashing as i64, scan_id, validating as i64, scan_id, last_item_id], |row| {
            Item::from_row(row)
        })?;
    
        let items: Vec<Item> = rows.collect::<Result<Vec<_>, _>>()?;
    
        Ok(items)
    }
    
    pub fn for_each_invalid_item_in_root<F>(db: &Database, root_id: i64, mut func: F) -> Result<(), FsPulseError>
    where
        F: FnMut(&Item) -> Result<(), FsPulseError>,
    {
        let sql = format!("SELECT {}
             FROM items
             WHERE root_id = ? AND is_tombstone = 0 AND validation_state = 'I'
             ORDER BY path ASC", Item::ITEM_COLUMNS);

        let mut stmt = db.conn().prepare(&sql)?;

        let rows = stmt.query_map([root_id], |row| {
            Item::from_row(row)
        })?;
        
        for row in rows {
            let item = row?;
            func(&item)?;
        }

        Ok(())
    }

    pub fn for_each_item_in_latest_scan<F>(db: &Database, scan_id: i64, mut func: F) -> Result<(), FsPulseError>
    where
        F: FnMut(&Item) -> Result<(), FsPulseError>,
    {
        let mut item_count = 0;

        let sql = format!("SELECT {}
             FROM items
             WHERE last_scan_id = ?
             ORDER BY path ASC", Item::ITEM_COLUMNS);

        let mut stmt = db.conn().prepare(&sql)?;

        let rows = stmt.query_map([scan_id], |row| {
            Item::from_row(row)
        })?;
        
        for row in rows {
            let item = row?;
            func(&item)?;
            item_count = item_count + 1;
        }
        Ok(())
    }

    pub fn for_each_item_with_path<F>(db: &Database, path: &str, mut func: F) -> Result<(), FsPulseError>
    where
        F: FnMut(&Item) -> Result<(), FsPulseError>,
    {
        let mut item_count = 0;

        let sql = format!("SELECT {}
             FROM items
             WHERE path = ?
             ORDER BY id ASC", Item::ITEM_COLUMNS);

        let mut stmt = db.conn().prepare(&sql)?;

        let rows = stmt.query_map([path], |row| {
            Item::from_row(row)
        })?;
        
        for row in rows {
            let item = row?;
            func(&item)?;
            item_count = item_count + 1;
        }
        Ok(())
    }
}