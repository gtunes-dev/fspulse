use rusqlite::{self, params, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::path::MAIN_SEPARATOR_STR;

use crate::{
    database::Database, error::FsPulseError, utils::Utils,
};

// Re-export types that were moved to item_identity.rs.
// Keeps existing consumers (query module, API routes) working without import changes.
pub use crate::item_identity::{Access, ItemType};

// QueryEnum impls for types re-exported from item_identity
impl crate::query::QueryEnum for ItemType {
    fn from_token(s: &str) -> Option<i64> {
        Self::from_string(s).map(|item_type| item_type.as_i64())
    }
}

impl crate::query::QueryEnum for Access {
    fn from_token(s: &str) -> Option<i64> {
        Self::from_string(s).map(|access| access.as_i64())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Item {
    #[serde(rename = "id")]
    item_id: i64,
    root_id: i64,
    #[serde(rename = "path")]
    item_path: String,
    #[serde(rename = "type")]
    item_type: ItemType,

    // Access state property group
    access: Access,

    last_scan: i64,
    is_ts: bool,

    // Metadata property group
    mod_date: Option<i64>,
    size: Option<i64>,

    // Hash property group
    last_hash_scan: Option<i64>,
    file_hash: Option<String>,

    // Validation property group
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
        access,
        last_scan,
        is_ts,
        mod_date,
        size,
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
            access: Access::from_i64(row.get(4)?),
            last_scan: row.get(5)?,
            is_ts: row.get(6)?,
            mod_date: row.get(7)?,
            size: row.get(8)?,
            last_hash_scan: row.get(9)?,
            file_hash: row.get(10)?,
            last_val_scan: row.get(11)?,
            val: row.get(12)?,
            val_error: row.get(13)?,
        })
    }

    pub fn item_id(&self) -> i64 {
        self.item_id
    }

    pub fn item_path(&self) -> &str {
        &self.item_path
    }

    pub fn item_type(&self) -> ItemType {
        self.item_type
    }

    pub fn is_ts(&self) -> bool {
        self.is_ts
    }

    /// Get size history for an item over a date range
    /// Returns a list of (scan_id, started_at, size) tuples from the changes table
    /// filtered by scan date range. Only includes changes where size_new is not NULL.
    /// Date strings should be in format "yyyy-MM-dd" (e.g., "2025-11-07")
    pub fn get_size_history(
        item_id: i64,
        from_date_str: &str, // Date string in format "yyyy-MM-dd"
        to_date_str: &str,   // Date string in format "yyyy-MM-dd"
    ) -> Result<Vec<SizeHistoryPoint>, FsPulseError> {
        // Use the same date bounds logic as FsPulse queries
        // This ensures full-day inclusivity (start at 00:00:00, end at 23:59:59)
        let (from_timestamp, to_timestamp) = Utils::range_date_bounds(from_date_str, to_date_str)?;

        let sql = r#"
            SELECT c.scan_id, s.started_at, c.size_new
            FROM changes c
            JOIN scans s ON c.scan_id = s.scan_id
            WHERE c.item_id = ?
              AND c.size_new IS NOT NULL
              AND s.started_at BETWEEN ? AND ?
            ORDER BY s.started_at ASC"#;

        let conn = Database::get_connection()?;
        let mut stmt = conn.prepare(sql)?;
        let rows = stmt.query_map(
            params![item_id, from_timestamp, to_timestamp],
            SizeHistoryPoint::from_row,
        )?;

        let mut history = Vec::new();
        for row in rows {
            history.push(row?);
        }

        Ok(history)
    }

    /// Get immediate children (one level deep) of a directory (old model)
    /// Returns only the direct children, not nested descendants
    /// Always includes tombstones - filtering should be done client-side
    pub fn old_get_immediate_children(
        root_id: i64,
        parent_path: &str,
    ) -> Result<Vec<Item>, FsPulseError> {
        let conn = Database::get_connection()?;

        // Build the path prefix for matching
        // Handle root path specially - if parent is "/" then children are like "/folder", not "//folder"
        let path_prefix = if parent_path == MAIN_SEPARATOR_STR {
            MAIN_SEPARATOR_STR.to_string()
        } else {
            format!("{}{}", parent_path, MAIN_SEPARATOR_STR)
        };

        // SQL to get immediate children:
        // 1. Match items whose path starts with parent_path/
        // 2. Exclude items that have additional slashes after the parent prefix
        //    (by checking that the remainder of the path contains no slashes)
        // Note: Always includes tombstones - client-side filtering provides better UX
        let sql = format!(
            "SELECT {}
             FROM items_old
             WHERE root_id = ?
               AND item_path LIKE ? || '%'
               AND item_path != ?
               AND SUBSTR(item_path, LENGTH(?) + 1) NOT LIKE '%{}%'
             ORDER BY item_path COLLATE natural_path ASC",
            Item::ITEM_COLUMNS,
            MAIN_SEPARATOR_STR
        );

        let mut stmt = conn.prepare(&sql)?;

        let rows = stmt.query_map(
            params![root_id, &path_prefix, parent_path, &path_prefix],
            Item::from_row,
        )?;

        let items: Vec<Item> = rows.collect::<Result<Vec<_>, _>>()?;

        Ok(items)
    }

    /// Get counts of children (files and directories) for a directory item (old model)
    /// Returns counts of non-tombstone files and directories that are direct or nested children
    pub fn old_get_children_counts(item_id: i64) -> Result<ChildrenCounts, FsPulseError> {
        // First get the path and root_id of the parent directory
        let parent_sql = "SELECT item_path, root_id FROM items_old WHERE item_id = ?";
        let conn = Database::get_connection()?;
        let (parent_path, root_id): (String, i64) = conn
            .query_row(parent_sql, params![item_id], |row| {
                Ok((row.get(0)?, row.get(1)?))
            })
            .optional()?
            .ok_or_else(|| FsPulseError::Error(format!("Item not found: item_id={}", item_id)))?;

        // Count children by type
        // Children are items whose path starts with parent_path/
        // We need to ensure the path comparison is correct
        let sql = r#"
            SELECT
                item_type,
                COUNT(*) as count
            FROM items_old
            WHERE root_id = ?
              AND is_ts = 0
              AND item_path LIKE ? || '%'
              AND item_path != ?
              AND (item_type = 0 OR item_type = 1)
            GROUP BY item_type"#;

        let mut stmt = conn.prepare(sql)?;
        let path_prefix = if parent_path == MAIN_SEPARATOR_STR {
            MAIN_SEPARATOR_STR.to_string()
        } else {
            format!("{}{}", parent_path, MAIN_SEPARATOR_STR)
        };

        let rows = stmt.query_map(params![root_id, path_prefix, parent_path], |row| {
            let item_type: i64 = row.get(0)?;
            let count: i64 = row.get(1)?;
            Ok((item_type, count))
        })?;

        let mut file_count = 0;
        let mut directory_count = 0;

        for row in rows {
            let (item_type, count) = row?;
            match ItemType::from_i64(item_type) {
                ItemType::File => file_count = count,
                ItemType::Directory => directory_count = count,
                _ => {}
            }
        }

        Ok(ChildrenCounts {
            file_count,
            directory_count,
        })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SizeHistoryPoint {
    pub scan_id: i64,
    pub started_at: i64,
    pub size: i64,
}

impl SizeHistoryPoint {
    fn from_row(row: &rusqlite::Row) -> rusqlite::Result<Self> {
        Ok(SizeHistoryPoint {
            scan_id: row.get(0)?,
            started_at: row.get(1)?,
            size: row.get(2)?,
        })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ChildrenCounts {
    pub file_count: i64,
    pub directory_count: i64,
}

/// Lightweight struct for temporal tree browsing results
#[derive(Clone, Debug, Serialize)]
pub struct TemporalTreeItem {
    pub item_id: i64,
    pub item_path: String,
    pub item_name: String,
    pub item_type: ItemType,
    pub is_deleted: bool,
}

/// Get immediate children at a point in time using items + item_versions.
/// Returns the effective version of each immediate child of `parent_path`
/// as of `scan_id`.
pub fn get_temporal_immediate_children(
    root_id: i64,
    parent_path: &str,
    scan_id: i64,
) -> Result<Vec<TemporalTreeItem>, FsPulseError> {
    let conn = Database::get_connection()?;

    let path_prefix = if parent_path == MAIN_SEPARATOR_STR {
        MAIN_SEPARATOR_STR.to_string()
    } else {
        format!("{}{}", parent_path, MAIN_SEPARATOR_STR)
    };

    let sql = format!(
        "SELECT i.item_id, i.item_path, i.item_name, i.item_type, iv.is_deleted
         FROM items i
         JOIN item_versions iv ON iv.item_id = i.item_id
         WHERE i.root_id = ?1
           AND iv.first_scan_id = (
               SELECT MAX(first_scan_id)
               FROM item_versions
               WHERE item_id = i.item_id
                 AND first_scan_id <= ?2
           )
           AND i.item_path LIKE ?3 || '%'
           AND i.item_path != ?4
           AND SUBSTR(i.item_path, LENGTH(?3) + 1) NOT LIKE '%{}%'
         ORDER BY i.item_path COLLATE natural_path ASC",
        MAIN_SEPARATOR_STR
    );

    let mut stmt = conn.prepare(&sql)?;

    let rows = stmt.query_map(
        params![root_id, scan_id, &path_prefix, parent_path],
        |row| {
            Ok(TemporalTreeItem {
                item_id: row.get(0)?,
                item_path: row.get(1)?,
                item_name: row.get(2)?,
                item_type: ItemType::from_i64(row.get(3)?),
                is_deleted: row.get(4)?,
            })
        },
    )?;

    let items: Vec<TemporalTreeItem> = rows.collect::<Result<Vec<_>, _>>()?;

    Ok(items)
}

/// Search for items by name at a point in time using items + item_versions.
/// Matches against the item name (last path segment) rather than the full path.
/// Returns items whose name contains the search query, ordered by path.
pub fn search_temporal_items(
    root_id: i64,
    scan_id: i64,
    query: &str,
) -> Result<Vec<TemporalTreeItem>, FsPulseError> {
    let conn = Database::get_connection()?;

    let sql =
        "SELECT i.item_id, i.item_path, i.item_name, i.item_type, iv.is_deleted
         FROM items i
         JOIN item_versions iv ON iv.item_id = i.item_id
         WHERE i.root_id = ?1
           AND iv.first_scan_id = (
               SELECT MAX(first_scan_id)
               FROM item_versions
               WHERE item_id = i.item_id
                 AND first_scan_id <= ?2
           )
           AND i.item_name LIKE '%' || ?3 || '%'
         ORDER BY i.item_path COLLATE natural_path ASC
         LIMIT 200";

    let mut stmt = conn.prepare(sql)?;

    let rows = stmt.query_map(
        params![root_id, scan_id, query],
        |row| {
            Ok(TemporalTreeItem {
                item_id: row.get(0)?,
                item_path: row.get(1)?,
                item_name: row.get(2)?,
                item_type: ItemType::from_i64(row.get(3)?),
                is_deleted: row.get(4)?,
            })
        },
    )?;

    let items: Vec<TemporalTreeItem> = rows.collect::<Result<Vec<_>, _>>()?;

    Ok(items)
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
        assert_eq!(ItemType::Unknown.as_i64(), 3);
    }

    #[test]
    fn test_item_type_from_i64() {
        // Verify round-trip conversion
        assert_eq!(ItemType::from_i64(0), ItemType::File);
        assert_eq!(ItemType::from_i64(1), ItemType::Directory);
        assert_eq!(ItemType::from_i64(2), ItemType::Symlink);
        assert_eq!(ItemType::from_i64(3), ItemType::Unknown);

        // Invalid values should default to Unknown
        assert_eq!(ItemType::from_i64(999), ItemType::Unknown);
        assert_eq!(ItemType::from_i64(-1), ItemType::Unknown);
    }

    #[test]
    fn test_item_type_short_name() {
        assert_eq!(ItemType::File.short_name(), "F");
        assert_eq!(ItemType::Directory.short_name(), "D");
        assert_eq!(ItemType::Symlink.short_name(), "S");
        assert_eq!(ItemType::Unknown.short_name(), "U");
    }

    #[test]
    fn test_item_type_full_name() {
        assert_eq!(ItemType::File.full_name(), "File");
        assert_eq!(ItemType::Directory.full_name(), "Directory");
        assert_eq!(ItemType::Symlink.full_name(), "Symlink");
        assert_eq!(ItemType::Unknown.full_name(), "Unknown");
    }

    #[test]
    fn test_item_type_enum_all_variants() {
        // Test that all enum variants work correctly
        let types = [
            ItemType::File,
            ItemType::Directory,
            ItemType::Symlink,
            ItemType::Unknown,
        ];
        let expected = ["F", "D", "S", "U"];

        for (i, item_type) in types.iter().enumerate() {
            assert_eq!(item_type.short_name(), expected[i]);
        }
    }

    #[test]
    fn test_access_integer_values() {
        assert_eq!(Access::Ok.as_i64(), 0);
        assert_eq!(Access::MetaError.as_i64(), 1);
        assert_eq!(Access::ReadError.as_i64(), 2);
    }

    #[test]
    fn test_access_from_i64() {
        assert_eq!(Access::from_i64(0), Access::Ok);
        assert_eq!(Access::from_i64(1), Access::MetaError);
        assert_eq!(Access::from_i64(2), Access::ReadError);

        // Invalid values should default to Ok
        assert_eq!(Access::from_i64(999), Access::Ok);
        assert_eq!(Access::from_i64(-1), Access::Ok);
    }

    #[test]
    fn test_access_short_name() {
        assert_eq!(Access::Ok.short_name(), "N");
        assert_eq!(Access::MetaError.short_name(), "M");
        assert_eq!(Access::ReadError.short_name(), "R");
    }

    #[test]
    fn test_access_full_name() {
        assert_eq!(Access::Ok.full_name(), "No Error");
        assert_eq!(Access::MetaError.full_name(), "Meta Error");
        assert_eq!(Access::ReadError.full_name(), "Read Error");
    }

    #[test]
    fn test_access_from_string() {
        // Full names
        assert_eq!(Access::from_string("No Error"), Some(Access::Ok));
        assert_eq!(Access::from_string("NoError"), Some(Access::Ok));
        assert_eq!(Access::from_string("Meta Error"), Some(Access::MetaError));
        assert_eq!(Access::from_string("MetaError"), Some(Access::MetaError));
        assert_eq!(Access::from_string("Read Error"), Some(Access::ReadError));
        assert_eq!(Access::from_string("ReadError"), Some(Access::ReadError));

        // Short names
        assert_eq!(Access::from_string("N"), Some(Access::Ok));
        assert_eq!(Access::from_string("M"), Some(Access::MetaError));
        assert_eq!(Access::from_string("R"), Some(Access::ReadError));

        // Case insensitive
        assert_eq!(Access::from_string("no error"), Some(Access::Ok));
        assert_eq!(Access::from_string("META ERROR"), Some(Access::MetaError));

        // Invalid
        assert_eq!(Access::from_string("X"), None);
        assert_eq!(Access::from_string(""), None);
    }

    #[test]
    fn test_access_display() {
        assert_eq!(Access::Ok.to_string(), "No Error");
        assert_eq!(Access::MetaError.to_string(), "Meta Error");
        assert_eq!(Access::ReadError.to_string(), "Read Error");
    }

    #[test]
    fn test_access_round_trip() {
        let states = [Access::Ok, Access::MetaError, Access::ReadError];

        for access in states {
            let str_val = access.short_name();
            let parsed_back = Access::from_string(str_val).unwrap();
            assert_eq!(access, parsed_back, "Round trip failed for {access:?}");
        }
    }
}
