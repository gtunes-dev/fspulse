use rusqlite::{self, params, OptionalExtension};

use crate::{database::Database, error::FsPulseError, validators::validator::ValidationState};

#[derive(Copy, Clone, Debug, PartialEq)]
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
pub struct Item {
    id: i64,
    root_id: i64,
    path: String,
    item_type: String,

    last_scan_id: i64,
    is_tombstone: bool,

    // Metadata property group
    mod_date: Option<i64>,
    file_size: Option<i64>,

    // Hash property group
    last_hash_scan_id: Option<i64>,
    file_hash: Option<String>,

    // Validation property group
    last_validation_scan_id: Option<i64>,
    validity_state: String,
    validation_error: Option<String>,
}

impl Item {
    const ITEM_COLUMNS: &str = "id, 
        root_id, 
        path,
        item_type,
        last_scan_id, 
        is_tombstone,
        mod_date,
        file_size,
        last_hash_scan_id,
        file_hash,
        last_validation_scan_id,
        validity_state,
        validation_error";

    fn from_row(row: &rusqlite::Row) -> rusqlite::Result<Self> {
        Ok(Item {
            id: row.get(0)?,
            root_id: row.get(1)?,
            path: row.get(2)?,
            item_type: row.get(3)?,
            last_scan_id: row.get(4)?,
            is_tombstone: row.get(5)?,
            mod_date: row.get(6)?,
            file_size: row.get(7)?,
            last_hash_scan_id: row.get(8)?,
            file_hash: row.get(9)?,
            last_validation_scan_id: row.get(10)?,
            validity_state: row.get(11)?,
            validation_error: row.get(12)?,
        })
    }

    pub fn get_by_id(db: &Database, id: i64) -> Result<Option<Self>, FsPulseError> {
        let query = format!("SELECT {} FROM ITEMS WHERE id = ?", Item::ITEM_COLUMNS);
        db.conn()
            .query_row(&query, params![id], Item::from_row)
            .optional()
            .map_err(FsPulseError::DatabaseError)
    }

    pub fn get_by_root_path_type(
        db: &Database,
        root_id: i64,
        path: &str,
        item_type: ItemType,
    ) -> Result<Option<Self>, FsPulseError> {
        let query = format!(
            "SELECT {} FROM ITEMS WHERE root_id = ? AND path = ? AND item_type = ?",
            Item::ITEM_COLUMNS
        );

        db.conn()
            .query_row(
                &query,
                params![root_id, path, item_type.as_str()],
                Item::from_row,
            )
            .optional()
            .map_err(FsPulseError::DatabaseError)
    }

    pub fn id(&self) -> i64 {
        self.id
    }
    pub fn root_id(&self) -> i64 {
        self.root_id
    }
    pub fn path(&self) -> &str {
        &self.path
    }
    pub fn item_type(&self) -> &str {
        &self.item_type
    }
    pub fn last_scan_id(&self) -> i64 {
        self.last_scan_id
    }
    pub fn is_tombstone(&self) -> bool {
        self.is_tombstone
    }
    pub fn mod_date(&self) -> Option<i64> {
        self.mod_date
    }
    pub fn file_size(&self) -> Option<i64> {
        self.file_size
    }
    pub fn last_hash_scan_id(&self) -> Option<i64> {
        self.last_hash_scan_id
    }
    pub fn file_hash(&self) -> Option<&str> {
        self.file_hash.as_deref()
    }
    pub fn last_validation_scan_id(&self) -> Option<i64> {
        self.last_validation_scan_id
    }
    pub fn validity_state_as_str(&self) -> &str {
        &self.validity_state
    }
    pub fn validity_state(&self) -> ValidationState {
        ValidationState::from_string(&self.validity_state)
    }
    pub fn validation_error(&self) -> Option<&str> {
        self.validation_error.as_deref()
    }

    pub fn count_analyzed_items(db: &Database, scan_id: i64) -> Result<i64, FsPulseError> {
        let mut stmt = db.conn().prepare(
            "SELECT COUNT(*) FROM items
             WHERE
                last_scan_id = ? AND
                item_type = 'F' AND
                (last_hash_scan_id = ? OR last_validation_scan_id = ?)",
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
        limit: usize, // Parameterized limit
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
            Item::ITEM_COLUMNS,
            limit
        );

        let mut stmt = db.conn().prepare(&query)?;

        let rows = stmt.query_map(
            [
                scan_id,
                hashing as i64,
                scan_id,
                validating as i64,
                scan_id,
                last_item_id,
            ],
            Item::from_row,
        )?;

        let items: Vec<Item> = rows.collect::<Result<Vec<_>, _>>()?;

        Ok(items)
    }

    pub fn for_each_invalid_item_in_root<F>(
        db: &Database,
        root_id: i64,
        mut func: F,
    ) -> Result<(), FsPulseError>
    where
        F: FnMut(&Item) -> Result<(), FsPulseError>,
    {
        let sql = format!(
            "SELECT {}
             FROM items
             WHERE root_id = ? AND is_tombstone = 0 AND validity_state = 'I'
             ORDER BY path ASC",
            Item::ITEM_COLUMNS
        );

        let mut stmt = db.conn().prepare(&sql)?;

        let rows = stmt.query_map([root_id], Item::from_row)?;

        for row in rows {
            let item = row?;
            func(&item)?;
        }

        Ok(())
    }

    pub fn for_each_item_in_latest_scan<F>(
        db: &Database,
        scan_id: i64,
        mut func: F,
    ) -> Result<(), FsPulseError>
    where
        F: FnMut(&Item) -> Result<(), FsPulseError>,
    {
        let sql = format!(
            "SELECT {}
             FROM items
             WHERE last_scan_id = ?
             ORDER BY path ASC",
            Item::ITEM_COLUMNS
        );

        let mut stmt = db.conn().prepare(&sql)?;

        let rows = stmt.query_map([scan_id], Item::from_row)?;

        for row in rows {
            let item = row?;
            func(&item)?;
        }
        Ok(())
    }

    pub fn for_each_item_with_path<F>(
        db: &Database,
        path: &str,
        mut func: F,
    ) -> Result<(), FsPulseError>
    where
        F: FnMut(&Item) -> Result<(), FsPulseError>,
    {
        let sql = format!(
            "SELECT {}
             FROM items
             WHERE path = ?
             ORDER BY id ASC",
            Item::ITEM_COLUMNS
        );

        let mut stmt = db.conn().prepare(&sql)?;

        let rows = stmt.query_map([path], Item::from_row)?;

        for row in rows {
            let item = row?;
            func(&item)?;
        }
        Ok(())
    }
}
