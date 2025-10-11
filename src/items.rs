use rusqlite::{self, params, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::{
    database::{Database, ListQuery, ListResult},
    error::FsPulseError,
    scans::AnalysisSpec,
    validate::validator::ValidationState,
};

#[derive(Clone, Debug, Default)]
pub struct AnalysisItem {
    item_id: i64,
    item_path: String,
    last_hash_scan: Option<i64>,
    file_hash: Option<String>,
    last_val_scan: Option<i64>,
    val: String,
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
        ValidationState::from_string(&self.val)
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
            _ => Err(FsPulseError::Error(format!("Invalid item type: '{s}'"))),
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Item {
    #[serde(rename = "id")]
    item_id: i64,
    #[allow(dead_code)]
    root_id: i64,
    #[serde(rename = "path")]
    item_path: String,
    #[serde(rename = "type")]
    item_type: String,

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
                params![root_id, path, item_type.as_str()],
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
        &self.val
    }

    #[allow(dead_code)]
    pub fn val(&self) -> ValidationState {
        ValidationState::from_string(&self.val)
    }

    pub fn val_error(&self) -> Option<&str> {
        self.val_error.as_deref()
    }

    pub fn list_paginated(
        db: &Database,
        query: &ListQuery,
    ) -> Result<ListResult<Item>, FsPulseError> {
        // First get the total count for pagination
        let count_sql = if let Some(filter) = &query.filter {
            format!(
                "SELECT COUNT(*) FROM items WHERE item_path LIKE '%{}%' AND is_ts = 0",
                filter.replace("'", "''") // Basic SQL injection protection
            )
        } else {
            "SELECT COUNT(*) FROM items WHERE is_ts = 0".to_string()
        };

        let total: u32 = db.conn()
            .query_row(&count_sql, [], |row| {
                let count: i64 = row.get(0)?;
                Ok(count as u32)
            })?;

        // Build the main query with sorting and pagination
        let order_clause = match query.sort.as_deref() {
            Some("path") => "ORDER BY item_path ASC, item_id ASC",
            Some("path_desc") => "ORDER BY item_path DESC, item_id DESC",
            Some("type") => "ORDER BY item_type ASC, item_path ASC",
            Some("type_desc") => "ORDER BY item_type DESC, item_path DESC",
            Some("size") => "ORDER BY file_size ASC NULLS FIRST, item_path ASC",
            Some("size_desc") => "ORDER BY file_size DESC NULLS LAST, item_path DESC",
            Some("mod_date") => "ORDER BY mod_date ASC NULLS FIRST, item_path ASC",
            Some("mod_date_desc") => "ORDER BY mod_date DESC NULLS LAST, item_path DESC",
            Some("id") => "ORDER BY item_id ASC",
            Some("id_desc") => "ORDER BY item_id DESC",
            _ => "ORDER BY item_path ASC, item_id ASC", // Default: by path
        };

        let where_clause = if let Some(filter) = &query.filter {
            format!(
                "WHERE item_path LIKE '%{}%' AND is_ts = 0",
                filter.replace("'", "''")
            )
        } else {
            "WHERE is_ts = 0".to_string()
        };

        let offset = (query.page - 1) * query.limit;
        let main_sql = format!(
            "SELECT {} FROM items {} {} LIMIT {} OFFSET {}",
            Item::ITEM_COLUMNS, where_clause, order_clause, query.limit, offset
        );

        let mut stmt = db.conn().prepare(&main_sql)?;
        let item_iter = stmt.query_map([], Item::from_row)?;

        let mut items = Vec::new();
        for item in item_iter {
            items.push(item?);
        }

        let has_next = offset + query.limit < total;
        let has_prev = query.page > 1;

        Ok(ListResult {
            items,
            total,
            page: query.page,
            limit: query.limit,
            has_next,
            has_prev,
        })
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
                    WHEN c.change_type = 'A' THEN 1
                    WHEN c.change_type = 'M' AND c.meta_change = 1 THEN 1
                    ELSE 0
                END AS needs_hash,
                CASE
                    WHEN $4 = 0 THEN 0
                    WHEN $5 = 1 AND (i.val = 'U' OR i.last_val_scan IS NULL OR i.last_val_scan < $3) THEN 1
                    WHEN i.val = 'U' THEN 1
                    WHEN c.change_type = 'A' THEN 1
                    WHEN c.change_type = 'M' AND c.meta_change = 1 THEN 1
                    ELSE 0
                END AS needs_val
            FROM items i
            LEFT JOIN changes c
                ON c.item_id = i.item_id AND c.scan_id = $3
            WHERE
                i.last_scan = $3 AND
                i.item_type = 'F' AND
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
                WHEN c.change_type = 'A' THEN 1
                WHEN c.change_type = 'M' AND c.meta_change = 1 THEN 1
                ELSE 0
            END AS needs_hash,
            CASE
                WHEN $4 = 0 THEN 0  -- val disabled
                WHEN $5 = 1 AND (i.val = 'U' OR i.last_val_scan < $3) THEN 1  -- val_all
                WHEN i.val = 'U' THEN 1
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
            AND i.is_ts = 0
            AND i.item_id > $6
            AND (
                ($1 = 1 AND (  -- hash enabled
                    ($2 = 1 AND (i.file_hash IS NULL OR i.last_hash_scan < $3)) OR
                    i.file_hash IS NULL OR
                    c.change_type = 'A' OR
                    (c.change_type = 'M' AND c.meta_change = 1)
                )) OR
                ($4 = 1 AND (  -- val enabled
                    ($5 = 1 AND (i.val = 'U' OR i.last_val_scan < $3)) OR
                    i.val = 'U' OR
                    c.change_type = 'A' OR
                    (c.change_type = 'M' AND c.meta_change = 1)
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
    fn test_item_type_as_str() {
        assert_eq!(ItemType::File.as_str(), "F");
        assert_eq!(ItemType::Directory.as_str(), "D");
        assert_eq!(ItemType::Symlink.as_str(), "S");
        assert_eq!(ItemType::Other.as_str(), "O");
    }

    #[test]
    fn test_item_type_short_str_to_full() {
        assert_eq!(ItemType::short_str_to_full("F").unwrap(), "File");
        assert_eq!(ItemType::short_str_to_full("D").unwrap(), "Directory");
        assert_eq!(ItemType::short_str_to_full("S").unwrap(), "Symlink");
        assert_eq!(ItemType::short_str_to_full("O").unwrap(), "Other");
        
        // Test invalid type
        let result = ItemType::short_str_to_full("X");
        assert!(result.is_err());
        if let Err(FsPulseError::Error(msg)) = result {
            assert!(msg.contains("Invalid item type: 'X'"));
        } else {
            panic!("Expected FsPulseError::Error");
        }
    }

    #[test]
    fn test_analysis_item_getters() {
        let analysis_item = AnalysisItem {
            item_id: 123,
            item_path: "/test/path".to_string(),
            last_hash_scan: Some(456),
            file_hash: Some("abc123".to_string()),
            last_val_scan: Some(789),
            val: "Valid".to_string(),
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
    fn test_analysis_item_default() {
        let analysis_item = AnalysisItem::default();
        
        assert_eq!(analysis_item.item_id(), 0);
        assert_eq!(analysis_item.item_path(), "");
        assert_eq!(analysis_item.last_hash_scan(), None);
        assert_eq!(analysis_item.file_hash(), None);
        assert_eq!(analysis_item.last_val_scan(), None);
        assert_eq!(analysis_item.val_error(), None);
        assert_eq!(analysis_item.meta_change(), None);
        assert!(!analysis_item.needs_hash());
        assert!(!analysis_item.needs_val());
    }

    #[test]
    fn test_item_getters() {
        let item = Item {
            item_id: 456,
            root_id: 1,
            item_path: "/another/path".to_string(),
            item_type: "F".to_string(),
            last_scan: 123456789,
            is_ts: true,
            mod_date: Some(987654321),
            file_size: Some(1024),
            last_hash_scan: Some(111),
            file_hash: Some("def456".to_string()),
            last_val_scan: Some(222),
            val: "Invalid".to_string(),
            val_error: Some("validation failed".to_string()),
        };

        assert_eq!(item.item_id(), 456);
        assert_eq!(item.root_id(), 1);
        assert_eq!(item.item_path(), "/another/path");
        assert_eq!(item.item_type(), "F");
        assert_eq!(item.last_scan(), 123456789);
        assert!(item.is_ts());
        assert_eq!(item.mod_date(), Some(987654321));
        assert_eq!(item.file_size(), Some(1024));
        assert_eq!(item.last_hash_scan(), Some(111));
        assert_eq!(item.file_hash(), Some("def456"));
        assert_eq!(item.last_val_scan(), Some(222));
        assert_eq!(item.validity_state_as_str(), "Invalid");
        assert_eq!(item.val_error(), Some("validation failed"));
    }

    #[test]
    fn test_item_default() {
        let item = Item::default();
        
        assert_eq!(item.item_id(), 0);
        assert_eq!(item.root_id(), 0);
        assert_eq!(item.item_path(), "");
        assert_eq!(item.item_type(), "");
        assert_eq!(item.last_scan(), 0);
        assert!(!item.is_ts());
        assert_eq!(item.mod_date(), None);
        assert_eq!(item.file_size(), None);
        assert_eq!(item.last_hash_scan(), None);
        assert_eq!(item.file_hash(), None);
        assert_eq!(item.last_val_scan(), None);
        assert_eq!(item.validity_state_as_str(), "");
        assert_eq!(item.val_error(), None);
    }

    #[test]
    fn test_item_type_enum_all_variants() {
        // Test that all enum variants work correctly
        let types = [ItemType::File, ItemType::Directory, ItemType::Symlink, ItemType::Other];
        let expected = ["F", "D", "S", "O"];
        
        for (i, item_type) in types.iter().enumerate() {
            assert_eq!(item_type.as_str(), expected[i]);
        }
    }

    #[test]
    fn test_analysis_item_optional_fields() {
        let mut analysis_item = AnalysisItem::default();
        
        // Test None values
        assert_eq!(analysis_item.file_hash(), None);
        assert_eq!(analysis_item.last_hash_scan(), None);
        assert_eq!(analysis_item.val_error(), None);
        
        // Test Some values
        analysis_item.file_hash = Some("test_hash".to_string());
        analysis_item.last_hash_scan = Some(123);
        analysis_item.val_error = Some("error_msg".to_string());
        
        assert_eq!(analysis_item.file_hash(), Some("test_hash"));
        assert_eq!(analysis_item.last_hash_scan(), Some(123));
        assert_eq!(analysis_item.val_error(), Some("error_msg"));
    }

    #[test]
    fn test_item_optional_fields() {
        let mut item = Item::default();
        
        // Test None values
        assert_eq!(item.file_hash(), None);
        assert_eq!(item.mod_date(), None);
        assert_eq!(item.file_size(), None);
        assert_eq!(item.val_error(), None);
        
        // Test Some values
        item.file_hash = Some("test_hash".to_string());
        item.mod_date = Some(1234567890);
        item.file_size = Some(2048);
        item.val_error = Some("validation_error".to_string());
        
        assert_eq!(item.file_hash(), Some("test_hash"));
        assert_eq!(item.mod_date(), Some(1234567890));
        assert_eq!(item.file_size(), Some(2048));
        assert_eq!(item.val_error(), Some("validation_error"));
    }
}
