use rusqlite::{self, params, OptionalExtension};

use crate::{database::Database, error::FsPulseError, scans::AnalysisSpec, validators::validator::ValidationState};


#[derive(Clone, Debug, Default)]
pub struct AnalysisItem {
    item_id: i64,
    item_path: String,
    last_hash_scan: Option<i64>,
    file_hash: Option<String>,
    last_val_scan: Option<i64>,
    val: String,
    val_error: Option<String>,
    needs_hash: bool,
    needs_val: bool,
}

impl AnalysisItem {
    pub fn item_id(&self) -> i64 {
        self.item_id
    }

    pub fn item_path(&self) -> &str {
        &self.item_path
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

    pub fn val(&self) -> ValidationState {
        ValidationState::from_string(&self.val)
    }

    pub fn val_error(&self) -> Option<&str> {
        self.val_error.as_deref()
    }

    pub fn needs_hash(&self) -> bool {
        self.needs_hash
    }

    pub fn needs_val(&self) -> bool {
        self.needs_val
    }

    fn from_row(row: &rusqlite::Row) -> rusqlite::Result<Self> {
        Ok(AnalysisItem {
            item_id: row.get(0)?,
            item_path: row.get(1)?,
            last_hash_scan: row.get(2)?,
            file_hash: row.get(3)?,
            last_val_scan: row.get(4)?,
            val: row.get(5)?,
            val_error: row.get(6)?,
            needs_hash: row.get(7)?,
            needs_val: row.get(8)?
        })
    }
}

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

    pub fn short_str_to_full(s: &str) -> Result<&str, FsPulseError> {
        match s {
            "F" => Ok("File"),
            "D" => Ok("Directory"),
            "S" => Ok("Symlink"),
            "O" => Ok("Other"),
            _ => Err(FsPulseError::Error(format!("Invalid item type: '{}'", s))),
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

    pub fn get_analysis_counts(
        db: &Database,
        scan_id: i64,
        analysis_spec: &AnalysisSpec,
    ) -> Result<(u64, u64), FsPulseError> {
        let sql = r#"
            WITH candidates AS (
    SELECT 
        i.item_id,
        i.last_hash_scan,
        i.last_val_scan,
        CASE
            WHEN $1 = 0 THEN 0
            WHEN $2 = 1 AND i.last_hash_scan < $3 THEN 1
            WHEN c.change_type IS NULL AND i.file_hash IS NULL THEN 1
            WHEN c.change_type = 'A' THEN 1
            WHEN c.change_type = 'M' AND c.meta_change = 1 THEN 1
            ELSE 0
        END AS needs_hash,
        CASE
            WHEN $4 = 0 THEN 0
            WHEN $5 = 1 AND i.last_val_scan < $3 THEN 1
            WHEN c.change_type IS NULL AND i.val IS NULL THEN 1
            WHEN c.change_type = 'A' THEN 1
            WHEN c.change_type = 'M' AND c.meta_change = 1 THEN 1
            ELSE 0
        END AS needs_val
    FROM items i
    LEFT JOIN changes c
        ON c.item_id = i.item_id AND c.scan_id = $3
    WHERE
        i.last_scan = $3 AND
        i.item_type = 'F'
)
SELECT
    COALESCE(SUM(CASE WHEN needs_hash = 1 OR needs_val = 1 THEN 1 ELSE 0 END), 0) AS total_needed,
    COALESCE(SUM(CASE 
        WHEN (needs_hash = 1 AND last_hash_scan = $3) 
           OR (needs_val = 1 AND last_val_scan = $3)
        THEN 1 ELSE 0 END), 0) AS total_done
FROM candidates"#;
    
        let conn = db.conn();
        let mut stmt = conn.prepare(sql)?;
        let mut rows = stmt.query(params![
            analysis_spec.is_hash() as i64,
            analysis_spec.hash_all() as i64,
            scan_id,
            analysis_spec.is_val() as i64,
            analysis_spec.val_all() as i64
        ])?;
    
        if let Some(row) = rows.next()? {
            let total_needed = row.get::<_, i64>(0)? as u64;
            let total_done = row.get::<_, i64>(1)? as u64;
            Ok((total_needed, total_done))
        } else {
            Ok((0, 0))
        }
    }

    /*
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
    */

    pub fn fetch_next_analysis_batch(
        db: &Database,
        scan_id: i64,
        analysis_spec: &AnalysisSpec,
        last_item_id: i64,
        limit: usize, // Parameterized limit
    ) -> Result<Vec<AnalysisItem>, FsPulseError> {

        let query = 
            format!("SELECT
                i.item_id,
                i.item_path,
                i.last_hash_scan,
                i.file_hash,
                i.last_val_scan,
                i.val,
                i.val_error,
            CASE
                WHEN $1 = 0 THEN 0  -- hash disabled
                WHEN $2 = 1 AND i.last_hash_scan < $3 THEN 1  -- hash_all
                WHEN c.change_type IS NULL AND i.file_hash IS NULL THEN 1
                WHEN c.change_type = 'A' THEN 1
                WHEN c.change_type = 'M' AND c.meta_change = 1 THEN 1
                ELSE 0
            END AS needs_hash,
            CASE
                WHEN $4 = 0 THEN 0  -- val disabled
                WHEN $5 = 1 AND i.last_val_scan < $3 THEN 1  -- val_all
                WHEN c.change_type IS NULL AND i.val IS NULL THEN 1
                WHEN c.change_type = 'A' THEN 1
                WHEN c.change_type = 'M' AND c.meta_change = 1 THEN 1
                ELSE 0
            END AS needs_val
        FROM items i
        LEFT JOIN changes c
            ON c.item_id = i.item_id AND c.scan_id = $3
        WHERE
            i.last_scan = $3
            AND i.item_type = 'F'
            AND (
                ($2 = 1 AND i.last_hash_scan < $3) OR
                ($5 = 1 AND i.last_val_scan < $3) OR
                (
                    (c.change_type IS NULL AND ($1 = 1 AND i.file_hash IS NULL OR $4 = 1 AND i.val IS NULL)) OR
                    (c.change_type = 'A') OR
                    (c.change_type = 'M' AND c.meta_change = 1)
                )
            )
            AND i.item_id > $6
        ORDER BY i.item_id ASC
        LIMIT {}", 
        limit);

        let mut stmt = db.conn().prepare(&query)?;

        let rows = stmt.query_map(
            [
                analysis_spec.is_hash() as i64,
                analysis_spec.hash_all() as i64,
                scan_id,
                analysis_spec.is_val() as i64,
                analysis_spec.val_all() as i64,
                last_item_id,
            ],
            AnalysisItem::from_row,
        )?;

        let analysis_items: Vec<AnalysisItem> = rows.collect::<Result<Vec<_>, _>>()?;

        Ok(analysis_items)
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
