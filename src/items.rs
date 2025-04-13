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
    item_id: i64,
    root_id: i64,
    item_path: String,
    item_type: String,

    last_scan: i64,
    is_ts: bool,

    // Metadata property group
    mod_date: Option<i64>,
    file_size: Option<i64>,

    // Hash property group
    last_hash_scan: Option<i64>,
    file_hash: Option<String>,

    // Validation property group
    last_val_scan: Option<i64>,
    val: String,
    val_error: Option<String>,
}

impl Item {
    const ITEM_COLUMNS: &str = "
        item_id, 
        root_id, 
        item_path,
        item_type,
        last_scan, 
        is_ts,
        mod_date,
        file_size,
        last_hash_scan,
        file_hash,
        last_val_scan,
        val,
        val_error";

    fn from_row(row: &rusqlite::Row) -> rusqlite::Result<Self> {
        Ok(Item {
            item_id: row.get(0)?,
            root_id: row.get(1)?,
            item_path: row.get(2)?,
            item_type: row.get(3)?,
            last_scan: row.get(4)?,
            is_ts: row.get(5)?,
            mod_date: row.get(6)?,
            file_size: row.get(7)?,
            last_hash_scan: row.get(8)?,
            file_hash: row.get(9)?,
            last_val_scan: row.get(10)?,
            val: row.get(11)?,
            val_error: row.get(12)?,
        })
    }

    pub fn get_by_id(db: &Database, id: i64) -> Result<Option<Self>, FsPulseError> {
        let query = format!("SELECT {} FROM ITEMS WHERE item_id = ?", Item::ITEM_COLUMNS);
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
            "SELECT {} FROM ITEMS WHERE root_id = ? AND item_path = ? AND item_type = ?",
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

    pub fn item_id(&self) -> i64 {
        self.item_id
    }
    pub fn root_id(&self) -> i64 {
        self.root_id
    }
    pub fn item_path(&self) -> &str {
        &self.item_path
    }
    pub fn item_type(&self) -> &str {
        &self.item_type
    }
    pub fn last_scan(&self) -> i64 {
        self.last_scan
    }
    pub fn is_ts(&self) -> bool {
        self.is_ts
    }
    pub fn mod_date(&self) -> Option<i64> {
        self.mod_date
    }
    pub fn file_size(&self) -> Option<i64> {
        self.file_size
    }
    pub fn last_hash_scan(&self) -> Option<i64> {
        self.last_hash_scan
    }
    pub fn file_hash(&self) -> Option<&str> {
        self.file_hash.as_deref()
    }
    pub fn last_val_scan(&self) -> Option<i64> {
        self.last_val_scan
    }
    pub fn validity_state_as_str(&self) -> &str {
        &self.val
    }
    pub fn val(&self) -> ValidationState {
        ValidationState::from_string(&self.val)
    }
    pub fn val_error(&self) -> Option<&str> {
        self.val_error.as_deref()
    }

    pub fn count_analyzed_items(db: &Database, scan_id: i64) -> Result<i64, FsPulseError> {
        let mut stmt = db.conn().prepare(
            "SELECT COUNT(*) FROM items
             WHERE
                last_scan = ? AND
                item_type = 'F' AND
                (last_hash_scan = ? OR last_val_scan = ?)",
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
                    last_scan = ?
                AND
                    item_type = 'F'
                AND 
                    ((? = 1 AND (last_hash_scan IS NULL OR last_hash_scan < ?))
                    OR 
                    (? = 1 AND (last_val_scan IS NULL OR last_val_scan < ?)))
                AND
                    item_id > ?
             ORDER BY item_id ASC
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
             WHERE root_id = ? AND is_ts = 0 AND val = 'I'
             ORDER BY item_path ASC",
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
             WHERE last_scan = ?
             ORDER BY item_path ASC",
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
             WHERE item_path = ?
             ORDER BY item_id ASC",
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
