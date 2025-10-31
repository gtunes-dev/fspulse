use log::warn;
use rusqlite::{self, params, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::{
    database::Database,
    error::FsPulseError,
    scans::AnalysisSpec,
    validate::validator::ValidationState,
};

#[derive(Clone, Debug)]
pub struct AnalysisItem {
    item_id: i64,
    item_path: String,
    last_hash_scan: Option<i64>,
    file_hash: Option<String>,
    last_val_scan: Option<i64>,
    val: i64,
    val_error: Option<String>,
    meta_change: Option<bool>,
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
        ValidationState::from_i64(self.val)
    }

    pub fn val_error(&self) -> Option<&str> {
        self.val_error.as_deref()
    }

    pub fn meta_change(&self) -> Option<bool> {
        self.meta_change
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
            meta_change: row.get(7)?,
            needs_hash: row.get(8)?,
            needs_val: row.get(9)?,
        })
    }
}

#[repr(i64)]
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum ItemType {
    File = 0,
    Directory = 1,
    Symlink = 2,
    Other = 3,
}

impl ItemType {
    pub fn as_i64(&self) -> i64 {
        *self as i64
    }

    pub fn from_i64(value: i64) -> Self {
        match value {
            0 => ItemType::File,
            1 => ItemType::Directory,
            2 => ItemType::Symlink,
            3 => ItemType::Other,
            _ => {
                warn!("Invalid ItemType value in database: {}, defaulting to Other", value);
                ItemType::Other
            }
        }
    }

    pub fn short_name(&self) -> &'static str {
        match self {
            ItemType::File => "F",
            ItemType::Directory => "D",
            ItemType::Symlink => "S",
            ItemType::Other => "O",
        }
    }

    pub fn full_name(&self) -> &'static str {
        match self {
            ItemType::File => "File",
            ItemType::Directory => "Directory",
            ItemType::Symlink => "Symlink",
            ItemType::Other => "Other",
        }
    }

    pub fn from_string(s: &str) -> Option<Self> {
        match s.to_ascii_uppercase().as_str() {
            // Full names
            "FILE" => Some(ItemType::File),
            "DIRECTORY" | "DIR" => Some(ItemType::Directory),
            "SYMLINK" => Some(ItemType::Symlink),
            "OTHER" => Some(ItemType::Other),
            // Short names
            "F" => Some(ItemType::File),
            "D" => Some(ItemType::Directory),
            "S" => Some(ItemType::Symlink),
            "O" => Some(ItemType::Other),
            _ => None,
        }
    }

}

impl std::fmt::Display for ItemType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.full_name())
    }
}

impl crate::query::QueryEnum for ItemType {
    fn from_token(s: &str) -> Option<i64> {
        Self::from_string(s).map(|item_type| item_type.as_i64())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Item {
    #[serde(rename = "id")]
    item_id: i64,
    #[allow(dead_code)]
    root_id: i64,
    #[serde(rename = "path")]
    item_path: String,
    #[serde(rename = "type")]
    item_type: ItemType,

    last_scan: i64,
    is_ts: bool,

    // Metadata property group
    mod_date: Option<i64>,
    file_size: Option<i64>,

    // Hash property group
    #[allow(dead_code)]
    last_hash_scan: Option<i64>,
    file_hash: Option<String>,

    // Validation property group
    #[allow(dead_code)]
    last_val_scan: Option<i64>,
    val: i64,
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
            item_type: ItemType::from_i64(row.get(3)?),
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

    #[allow(dead_code)]
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
                params![root_id, path, item_type.as_i64()],
                Item::from_row,
            )
            .optional()
            .map_err(FsPulseError::DatabaseError)
    }

    pub fn item_id(&self) -> i64 {
        self.item_id
    }

    #[allow(dead_code)]
    pub fn root_id(&self) -> i64 {
        self.root_id
    }
    pub fn item_path(&self) -> &str {
        &self.item_path
    }
    pub fn item_type(&self) -> ItemType {
        self.item_type
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
    #[allow(dead_code)]
    pub fn last_hash_scan(&self) -> Option<i64> {
        self.last_hash_scan
    }
    pub fn file_hash(&self) -> Option<&str> {
        self.file_hash.as_deref()
    }

    #[allow(dead_code)]
    pub fn last_val_scan(&self) -> Option<i64> {
        self.last_val_scan
    }
    pub fn validity_state_as_str(&self) -> &str {
        self.val().short_name()
    }

    #[allow(dead_code)]
    pub fn val(&self) -> ValidationState {
        ValidationState::from_i64(self.val)
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
                i.file_hash,
                i.last_val_scan,
                i.val,
                CASE
                    WHEN $1 = 0 THEN 0
                    WHEN $2 = 1 AND (i.file_hash IS NULL OR i.last_hash_scan IS NULL OR i.last_hash_scan < $3) THEN 1
                    WHEN i.file_hash IS NULL THEN 1
                    WHEN c.change_type = 1 THEN 1
                    WHEN c.change_type = 2 AND c.meta_change = 1 THEN 1
                    ELSE 0
                END AS needs_hash,
                CASE
                    WHEN $4 = 0 THEN 0
                    WHEN $5 = 1 AND (i.val = 0 OR i.last_val_scan IS NULL OR i.last_val_scan < $3) THEN 1
                    WHEN i.val = 0 THEN 1
                    WHEN c.change_type = 1 THEN 1
                    WHEN c.change_type = 2 AND c.meta_change = 1 THEN 1
                    ELSE 0
                END AS needs_val
            FROM items i
            LEFT JOIN changes c
                ON c.item_id = i.item_id AND c.scan_id = $3
            WHERE
                i.last_scan = $3 AND
                i.item_type = 0 AND
                i.is_ts = 0
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
                item_type = 0 AND
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
        let query = format!(
            "SELECT
                i.item_id,
                i.item_path,
                i.last_hash_scan,
                i.file_hash,
                i.last_val_scan,
                i.val,
                i.val_error,
                c.meta_change,
            CASE
                WHEN $1 = 0 THEN 0  -- hash disabled
                WHEN $2 = 1 AND (i.file_hash IS NULL OR i.last_hash_scan < $3) THEN 1  -- hash_all
                WHEN i.file_hash IS NULL THEN 1
                WHEN c.change_type = 1 THEN 1
                WHEN c.change_type = 2 AND c.meta_change = 1 THEN 1
                ELSE 0
            END AS needs_hash,
            CASE
                WHEN $4 = 0 THEN 0  -- val disabled
                WHEN $5 = 1 AND (i.val = 0 OR i.last_val_scan < $3) THEN 1  -- val_all
                WHEN i.val = 0 THEN 1
                WHEN c.change_type = 1 THEN 1
                WHEN c.change_type = 2 AND c.meta_change = 1 THEN 1
                ELSE 0
            END AS needs_val
        FROM items i
        LEFT JOIN changes c
            ON c.item_id = i.item_id AND c.scan_id = $3
        WHERE
            i.last_scan = $3
            AND i.item_type = 0
            AND i.is_ts = 0
            AND i.item_id > $6
            AND (
                ($1 = 1 AND (  -- hash enabled
                    ($2 = 1 AND (i.file_hash IS NULL OR i.last_hash_scan < $3)) OR
                    i.file_hash IS NULL OR
                    c.change_type = 1 OR
                    (c.change_type = 2 AND c.meta_change = 1)
                )) OR
                ($4 = 1 AND (  -- val enabled
                    ($5 = 1 AND (i.val = 0 OR i.last_val_scan < $3)) OR
                    i.val = 0 OR
                    c.change_type = 1 OR
                    (c.change_type = 2 AND c.meta_change = 1)
                ))
            )
        ORDER BY i.item_id ASC
        LIMIT {limit}"
        );

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

    /*
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
             WHERE root_id = ? AND is_ts = 0 AND val = 2
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
    */

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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_item_type_integer_values() {
        // Verify the integer values match the expected order
        assert_eq!(ItemType::File.as_i64(), 0);
        assert_eq!(ItemType::Directory.as_i64(), 1);
        assert_eq!(ItemType::Symlink.as_i64(), 2);
        assert_eq!(ItemType::Other.as_i64(), 3);
    }

    #[test]
    fn test_item_type_from_i64() {
        // Verify round-trip conversion
        assert_eq!(ItemType::from_i64(0), ItemType::File);
        assert_eq!(ItemType::from_i64(1), ItemType::Directory);
        assert_eq!(ItemType::from_i64(2), ItemType::Symlink);
        assert_eq!(ItemType::from_i64(3), ItemType::Other);

        // Invalid values should default to Other
        assert_eq!(ItemType::from_i64(999), ItemType::Other);
        assert_eq!(ItemType::from_i64(-1), ItemType::Other);
    }

    #[test]
    fn test_item_type_short_name() {
        assert_eq!(ItemType::File.short_name(), "F");
        assert_eq!(ItemType::Directory.short_name(), "D");
        assert_eq!(ItemType::Symlink.short_name(), "S");
        assert_eq!(ItemType::Other.short_name(), "O");
    }

    #[test]
    fn test_item_type_full_name() {
        assert_eq!(ItemType::File.full_name(), "File");
        assert_eq!(ItemType::Directory.full_name(), "Directory");
        assert_eq!(ItemType::Symlink.full_name(), "Symlink");
        assert_eq!(ItemType::Other.full_name(), "Other");
    }

    #[test]
    fn test_analysis_item_getters() {
        let analysis_item = AnalysisItem {
            item_id: 123,
            item_path: "/test/path".to_string(),
            last_hash_scan: Some(456),
            file_hash: Some("abc123".to_string()),
            last_val_scan: Some(789),
            val: ValidationState::Valid.as_i64(),
            val_error: Some("test error".to_string()),
            meta_change: Some(true),
            needs_hash: true,
            needs_val: false,
        };

        assert_eq!(analysis_item.item_id(), 123);
        assert_eq!(analysis_item.item_path(), "/test/path");
        assert_eq!(analysis_item.last_hash_scan(), Some(456));
        assert_eq!(analysis_item.file_hash(), Some("abc123"));
        assert_eq!(analysis_item.last_val_scan(), Some(789));
        assert_eq!(analysis_item.val_error(), Some("test error"));
        assert_eq!(analysis_item.meta_change(), Some(true));
        assert!(analysis_item.needs_hash());
        assert!(!analysis_item.needs_val());
    }

    #[test]
    fn test_item_getters() {
        let item = Item {
            item_id: 456,
            root_id: 1,
            item_path: "/another/path".to_string(),
            item_type: ItemType::File,
            last_scan: 123456789,
            is_ts: true,
            mod_date: Some(987654321),
            file_size: Some(1024),
            last_hash_scan: Some(111),
            file_hash: Some("def456".to_string()),
            last_val_scan: Some(222),
            val: ValidationState::Invalid.as_i64(),
            val_error: Some("validation failed".to_string()),
        };

        assert_eq!(item.item_id(), 456);
        assert_eq!(item.root_id(), 1);
        assert_eq!(item.item_path(), "/another/path");
        assert_eq!(item.item_type(), ItemType::File);
        assert_eq!(item.last_scan(), 123456789);
        assert!(item.is_ts());
        assert_eq!(item.mod_date(), Some(987654321));
        assert_eq!(item.file_size(), Some(1024));
        assert_eq!(item.last_hash_scan(), Some(111));
        assert_eq!(item.file_hash(), Some("def456"));
        assert_eq!(item.last_val_scan(), Some(222));
        assert_eq!(item.validity_state_as_str(), "I");
        assert_eq!(item.val_error(), Some("validation failed"));
    }

    #[test]
    fn test_item_type_enum_all_variants() {
        // Test that all enum variants work correctly
        let types = [ItemType::File, ItemType::Directory, ItemType::Symlink, ItemType::Other];
        let expected = ["F", "D", "S", "O"];

        for (i, item_type) in types.iter().enumerate() {
            assert_eq!(item_type.short_name(), expected[i]);
        }
    }

}
