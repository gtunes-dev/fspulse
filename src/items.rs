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

/// Get size history for an item using item_versions.
/// Returns size data points from versions filtered by scan date range.
/// Uses `from_date` for the lower bound and `to_scan_id` to cap at a specific scan.
pub fn get_size_history(
    item_id: i64,
    from_date_str: &str,
    to_scan_id: i64,
) -> Result<Vec<SizeHistoryPoint>, FsPulseError> {
    let conn = Database::get_connection()?;

    // Get the upper bound timestamp from the anchor scan
    let to_timestamp: i64 = conn
        .query_row(
            "SELECT started_at FROM scans WHERE scan_id = ?",
            params![to_scan_id],
            |row| row.get(0),
        )?;

    // Get the lower bound timestamp from the date string
    let (from_timestamp, _) = Utils::range_date_bounds(from_date_str, from_date_str)?;

    let sql = r#"
        SELECT iv.first_scan_id, s.started_at, iv.size
        FROM item_versions iv
        JOIN scans s ON iv.first_scan_id = s.scan_id
        WHERE iv.item_id = ?
          AND iv.size IS NOT NULL
          AND s.started_at BETWEEN ? AND ?
        ORDER BY s.started_at ASC"#;

    let mut stmt = conn.prepare_cached(sql)?;
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
    pub first_scan_id: i64,
    pub is_added: bool,
    pub is_deleted: bool,
    pub mod_date: Option<i64>,
    pub size: Option<i64>,
    pub add_count: Option<i64>,
    pub modify_count: Option<i64>,
    pub delete_count: Option<i64>,
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

    let path_prefix = if parent_path.ends_with(MAIN_SEPARATOR_STR) {
        parent_path.to_string()
    } else {
        format!("{}{}", parent_path, MAIN_SEPARATOR_STR)
    };

    // Upper bound for range scan: replace trailing separator with next ASCII char.
    // Unix: '/' (0x2F) + 1 = '0' (0x30). Windows: '\' (0x5C) + 1 = ']' (0x5D).
    let path_upper = format!(
        "{}{}",
        &path_prefix[..path_prefix.len() - MAIN_SEPARATOR_STR.len()],
        char::from(std::path::MAIN_SEPARATOR as u8 + 1)
    );

    let sql = format!(
        "SELECT i.item_id, i.item_path, i.item_name, i.item_type,
                iv.first_scan_id, iv.is_added, iv.is_deleted, iv.mod_date, iv.size,
                iv.add_count, iv.modify_count, iv.delete_count
         FROM items i
         JOIN item_versions iv ON iv.item_id = i.item_id
         WHERE i.root_id = ?1
           AND iv.first_scan_id = (
               SELECT MAX(first_scan_id)
               FROM item_versions
               WHERE item_id = i.item_id
                 AND first_scan_id <= ?2
           )
           AND (iv.is_deleted = 0 OR iv.first_scan_id = ?2)
           AND i.item_path >= ?3
           AND i.item_path < ?4
           AND i.item_path != ?5
           AND SUBSTR(i.item_path, LENGTH(?3) + 1) NOT LIKE '%{}%'
         ORDER BY i.item_path COLLATE natural_path ASC",
        MAIN_SEPARATOR_STR
    );

    let mut stmt = conn.prepare(&sql)?;

    let rows = stmt.query_map(
        params![root_id, scan_id, &path_prefix, &path_upper, parent_path],
        |row| {
            Ok(TemporalTreeItem {
                item_id: row.get(0)?,
                item_path: row.get(1)?,
                item_name: row.get(2)?,
                item_type: ItemType::from_i64(row.get(3)?),
                first_scan_id: row.get(4)?,
                is_added: row.get(5)?,
                is_deleted: row.get(6)?,
                mod_date: row.get(7)?,
                size: row.get(8)?,
                add_count: row.get(9)?,
                modify_count: row.get(10)?,
                delete_count: row.get(11)?,
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
        "SELECT i.item_id, i.item_path, i.item_name, i.item_type,
                iv.first_scan_id, iv.is_added, iv.is_deleted, iv.mod_date, iv.size,
                iv.add_count, iv.modify_count, iv.delete_count
         FROM items i
         JOIN item_versions iv ON iv.item_id = i.item_id
         WHERE i.root_id = ?1
           AND iv.first_scan_id = (
               SELECT MAX(first_scan_id)
               FROM item_versions
               WHERE item_id = i.item_id
                 AND first_scan_id <= ?2
           )
           AND (iv.is_deleted = 0 OR iv.first_scan_id = ?2)
           AND i.item_name LIKE '%' || ?3 || '%'
         ORDER BY i.item_path COLLATE natural_path ASC
         LIMIT 200";

    let mut stmt = conn.prepare_cached(sql)?;

    let rows = stmt.query_map(
        params![root_id, scan_id, query],
        |row| {
            Ok(TemporalTreeItem {
                item_id: row.get(0)?,
                item_path: row.get(1)?,
                item_name: row.get(2)?,
                item_type: ItemType::from_i64(row.get(3)?),
                first_scan_id: row.get(4)?,
                is_added: row.get(5)?,
                is_deleted: row.get(6)?,
                mod_date: row.get(7)?,
                size: row.get(8)?,
                add_count: row.get(9)?,
                modify_count: row.get(10)?,
                delete_count: row.get(11)?,
            })
        },
    )?;

    let items: Vec<TemporalTreeItem> = rows.collect::<Result<Vec<_>, _>>()?;

    Ok(items)
}

// ---- Version history types and functions ----

/// A single version entry for the ItemDetailSheet version history
#[derive(Clone, Debug, Serialize)]
pub struct VersionHistoryEntry {
    pub version_id: i64,
    pub first_scan_id: i64,
    pub last_scan_id: i64,
    pub first_scan_date: i64,
    pub last_scan_date: i64,
    pub is_deleted: bool,
    pub access: i64,
    pub mod_date: Option<i64>,
    pub size: Option<i64>,
    pub file_hash: Option<String>,
    pub val: Option<i64>,
    pub val_error: Option<String>,
    pub last_hash_scan: Option<i64>,
    pub last_val_scan: Option<i64>,
}

impl VersionHistoryEntry {
    fn from_row(row: &rusqlite::Row) -> rusqlite::Result<Self> {
        Ok(VersionHistoryEntry {
            version_id: row.get(0)?,
            first_scan_id: row.get(1)?,
            last_scan_id: row.get(2)?,
            first_scan_date: row.get(3)?,
            last_scan_date: row.get(4)?,
            is_deleted: row.get(5)?,
            access: row.get(6)?,
            mod_date: row.get(7)?,
            size: row.get(8)?,
            file_hash: row.get(9)?,
            val: row.get(10)?,
            val_error: row.get(11)?,
            last_hash_scan: row.get(12)?,
            last_val_scan: row.get(13)?,
        })
    }
}

/// Response for initial version history load
#[derive(Debug, Serialize)]
pub struct VersionHistoryResponse {
    pub versions: Vec<VersionHistoryEntry>,
    pub anchor_index: Option<usize>,
    pub has_more: bool,
    pub total_count: i64,
    pub first_seen_scan_id: i64,
    pub first_seen_scan_date: i64,
    pub anchor_scan_date: i64,
}

/// Response for version history pagination
#[derive(Debug, Serialize)]
pub struct VersionHistoryPageResponse {
    pub versions: Vec<VersionHistoryEntry>,
    pub has_more: bool,
}

const VERSION_HISTORY_COLUMNS: &str =
    "v.version_id, v.first_scan_id, v.last_scan_id, \
     s1.started_at, s2.started_at, \
     v.is_deleted, v.access, \
     v.mod_date, v.size, v.file_hash, v.val, v.val_error, v.last_hash_scan, v.last_val_scan";

/// Get version history for an item, starting from a specific scan going backwards.
/// Returns up to `limit` versions ordered by first_scan_id DESC.
pub fn get_version_history_init(
    item_id: i64,
    scan_id: i64,
    limit: i64,
) -> Result<VersionHistoryResponse, FsPulseError> {
    let conn = Database::get_connection()?;

    // Get total count
    let total_count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM item_versions WHERE item_id = ?",
        params![item_id],
        |row| row.get(0),
    )?;

    if total_count == 0 {
        return Ok(VersionHistoryResponse {
            versions: Vec::new(),
            anchor_index: None,
            has_more: false,
            total_count: 0,
            first_seen_scan_id: 0,
            first_seen_scan_date: 0,
            anchor_scan_date: 0,
        });
    }

    // Get first-seen scan id and date
    let (first_seen_scan_id, first_seen_scan_date): (i64, i64) = conn
        .query_row(
            "SELECT iv.first_scan_id, s.started_at \
             FROM item_versions iv \
             JOIN scans s ON s.scan_id = iv.first_scan_id \
             WHERE iv.item_id = ? \
             ORDER BY iv.first_scan_id ASC \
             LIMIT 1",
            params![item_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;

    // Get the anchor scan date
    let anchor_scan_date: i64 = conn.query_row(
        "SELECT started_at FROM scans WHERE scan_id = ?",
        params![scan_id],
        |row| row.get(0),
    )?;

    // Load versions from the anchor scan going backwards, joining scans for dates
    let sql = format!(
        "SELECT {} \
         FROM item_versions v \
         JOIN scans s1 ON s1.scan_id = v.first_scan_id \
         JOIN scans s2 ON s2.scan_id = v.last_scan_id \
         WHERE v.item_id = ? AND v.first_scan_id <= ? \
         ORDER BY v.first_scan_id DESC \
         LIMIT ?",
        VERSION_HISTORY_COLUMNS
    );

    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(
        params![item_id, scan_id, limit + 1],
        VersionHistoryEntry::from_row,
    )?;

    let mut versions: Vec<VersionHistoryEntry> = rows.collect::<Result<Vec<_>, _>>()?;
    let has_more = versions.len() as i64 > limit;
    if has_more {
        versions.truncate(limit as usize);
    }

    Ok(VersionHistoryResponse {
        versions,
        anchor_index: Some(0),
        has_more,
        total_count,
        first_seen_scan_id,
        first_seen_scan_date,
        anchor_scan_date,
    })
}

/// Get more version history (older versions) using cursor-based pagination.
/// Returns versions with first_scan_id strictly less than `before_scan_id`.
pub fn get_version_history_page(
    item_id: i64,
    before_scan_id: i64,
    limit: i64,
) -> Result<VersionHistoryPageResponse, FsPulseError> {
    let conn = Database::get_connection()?;

    let sql = format!(
        "SELECT {} \
         FROM item_versions v \
         JOIN scans s1 ON s1.scan_id = v.first_scan_id \
         JOIN scans s2 ON s2.scan_id = v.last_scan_id \
         WHERE v.item_id = ? AND v.first_scan_id < ? \
         ORDER BY v.first_scan_id DESC \
         LIMIT ?",
        VERSION_HISTORY_COLUMNS
    );

    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(
        params![item_id, before_scan_id, limit + 1],
        VersionHistoryEntry::from_row,
    )?;

    let mut versions: Vec<VersionHistoryEntry> = rows.collect::<Result<Vec<_>, _>>()?;
    let has_more = versions.len() as i64 > limit;
    if has_more {
        versions.truncate(limit as usize);
    }

    Ok(VersionHistoryPageResponse { versions, has_more })
}

/// Get counts of children (files and directories) for a directory item using temporal model.
/// Counts non-deleted items at the given scan_id.
pub fn get_children_counts(
    item_id: i64,
    scan_id: i64,
) -> Result<ChildrenCounts, FsPulseError> {
    let conn = Database::get_connection()?;

    // Get the parent item's path and root_id
    let (parent_path, root_id): (String, i64) = conn
        .query_row(
            "SELECT item_path, root_id FROM items WHERE item_id = ?",
            params![item_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?
        .ok_or_else(|| FsPulseError::Error(format!("Item not found: item_id={}", item_id)))?;

    let path_prefix = if parent_path.ends_with(MAIN_SEPARATOR_STR) {
        parent_path.to_string()
    } else {
        format!("{}{}", parent_path, MAIN_SEPARATOR_STR)
    };

    // Upper bound for range scan: replace trailing separator with next ASCII char.
    // Unix: '/' (0x2F) + 1 = '0' (0x30). Windows: '\' (0x5C) + 1 = ']' (0x5D).
    let path_upper = format!(
        "{}{}",
        &path_prefix[..path_prefix.len() - MAIN_SEPARATOR_STR.len()],
        char::from(std::path::MAIN_SEPARATOR as u8 + 1)
    );

    let sql = r#"
        SELECT
            i.item_type,
            COUNT(*) as count
        FROM items i
        JOIN item_versions iv ON iv.item_id = i.item_id
        WHERE i.root_id = ?1
          AND iv.first_scan_id = (
              SELECT MAX(first_scan_id) FROM item_versions
              WHERE item_id = i.item_id AND first_scan_id <= ?2
          )
          AND iv.is_deleted = 0
          AND i.item_path >= ?3
          AND i.item_path < ?4
          AND i.item_path != ?5
          AND (i.item_type = 0 OR i.item_type = 1)
        GROUP BY i.item_type"#;

    let mut stmt = conn.prepare_cached(sql)?;
    let rows = stmt.query_map(params![root_id, scan_id, path_prefix, path_upper, parent_path], |row| {
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
