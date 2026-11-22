use rusqlite::{self, params, OptionalExtension};
use serde::{Deserialize, Serialize, Serializer};
use std::path::MAIN_SEPARATOR_STR;

use crate::{
    db::Database, error::FsPulseError,
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
pub struct ChildrenCounts {
    pub file_count: i64,
    pub directory_count: i64,
}

// ---- Version history types and functions ----

fn serialize_optional_hash<S: Serializer>(
    hash: &Option<Vec<u8>>,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    match hash {
        Some(bytes) => serializer.serialize_some(&hex::encode(bytes)),
        None => serializer.serialize_none(),
    }
}

fn serialize_hash<S: Serializer>(
    hash: &Vec<u8>,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    serializer.serialize_str(&hex::encode(hash))
}

/// A single version entry for the ItemDetail version history
#[derive(Clone, Debug, Serialize)]
pub struct VersionHistoryEntry {
    pub item_version: i64,
    pub first_seen_at: i64,
    pub last_changed_at: i64,
    pub change_count: i64,
    pub source: i64,
    pub is_added: bool,
    pub is_deleted: bool,
    /// Written because of the ignore patterns: with `is_deleted`,
    /// tracking stopped; with `is_added`, tracking resumed.
    pub is_ignored: bool,
    pub access: i64,
    pub mod_date: Option<i64>,
    pub size: Option<i64>,
    // Integrity fields (NULL for non-files or when no record exists)
    pub hash_state: Option<i64>,
    #[serde(serialize_with = "serialize_optional_hash")]
    pub file_hash: Option<Vec<u8>>,
    pub val_state: Option<i64>,
    pub val_error: Option<String>,
    /// When this version was validated.
    pub val_at: Option<i64>,
    /// When this version's latest hash was last confirmed by an integrity check.
    pub hash_last_seen_at: Option<i64>,
    // Review timestamps
    pub val_reviewed_at: Option<i64>,
    pub hash_reviewed_at: Option<i64>,
}

impl VersionHistoryEntry {
    fn from_row(row: &rusqlite::Row) -> rusqlite::Result<Self> {
        Ok(VersionHistoryEntry {
            item_version: row.get(0)?,
            first_seen_at: row.get(1)?,
            last_changed_at: row.get(2)?,
            change_count: row.get(3)?,
            source: row.get(4)?,
            is_added: row.get(5)?,
            is_deleted: row.get(6)?,
            access: row.get(7)?,
            mod_date: row.get(8)?,
            size: row.get(9)?,
            hash_state: row.get(10)?,
            file_hash: row.get(11)?,
            val_state: row.get(12)?,
            val_error: row.get(13)?,
            val_reviewed_at: row.get(14)?,
            hash_reviewed_at: row.get(15)?,
            is_ignored: row.get(16)?,
            val_at: row.get(17)?,
            hash_last_seen_at: row.get(18)?,
        })
    }
}

const VERSION_HISTORY_COLUMNS: &str =
    "v.item_version, v.first_seen_at, v.last_changed_at, v.change_count, v.source, \
     v.is_added, v.is_deleted, v.access, \
     v.mod_date, v.size, \
     hv.hash_state, hv.file_hash, v.val_state, v.val_error, \
     v.val_reviewed_at, v.hash_reviewed_at, v.is_ignored, \
     v.val_at, hv.last_seen_at";

const VERSION_HISTORY_JOINS: &str =
    "LEFT JOIN hash_versions hv ON hv.item_id = v.item_id \
       AND hv.item_version = v.item_version \
       AND hv.first_seen_at = ( \
           SELECT MAX(hv2.first_seen_at) FROM hash_versions hv2 \
           WHERE hv2.item_id = v.item_id AND hv2.item_version = v.item_version \
       )";

/// How many versions an item has, and a stamp that changes whenever
/// anything recorded about the item does: a new version, further
/// changes folded into the latest one, a validation, a hash computed
/// or confirmed. A viewer compares stamps to learn that what it shows
/// is out of date.
pub fn count_versions(item_id: i64) -> Result<(i64, String), FsPulseError> {
    let conn = Database::get_connection()?;
    let found = conn.query_row(
        "SELECT COUNT(*), COALESCE(MAX(last_changed_at), 0), COALESCE(MAX(val_at), 0),
                (SELECT COALESCE(MAX(last_seen_at), 0) FROM hash_versions WHERE item_id = ?1)
           FROM item_versions WHERE item_id = ?1",
        params![item_id],
        |row| {
            let total: i64 = row.get(0)?;
            let (changed, validated, hashed): (i64, i64, i64) = (row.get(1)?, row.get(2)?, row.get(3)?);
            Ok((total, format!("{total}.{changed}.{validated}.{hashed}")))
        },
    )?;
    Ok(found)
}

/// Get a page of version history for an item.
/// `order` is "asc" or "desc" (by item_version).
pub fn get_versions(
    item_id: i64,
    offset: i64,
    limit: i64,
    order: &str,
) -> Result<Vec<VersionHistoryEntry>, FsPulseError> {
    let conn = Database::get_connection()?;
    let order_clause = if order == "asc" { "ASC" } else { "DESC" };

    let sql = format!(
        "SELECT {VERSION_HISTORY_COLUMNS} \
         FROM item_versions v \
         {VERSION_HISTORY_JOINS} \
         WHERE v.item_id = ? \
         ORDER BY v.item_version {order_clause} \
         LIMIT ? OFFSET ?"
    );

    let mut stmt = conn.prepare(&sql)?;
    let versions = stmt
        .query_map(params![item_id, limit, offset], VersionHistoryEntry::from_row)?
        .collect::<Result<Vec<_>, _>>()?;

    Ok(versions)
}

/// The version of an item in effect at a time.
#[derive(Clone, Debug, Serialize)]
pub struct VersionAt {
    pub item_version: i64,
    /// Not present then: deleted or, with `is_ignored`, not tracked.
    pub is_deleted: bool,
    pub is_ignored: bool,
}

/// Find the version in effect at a point in time: the latest version
/// that began at or before it.
///
/// The time is Unix seconds, inclusive.
pub fn get_version_at(item_id: i64, at: i64) -> Result<Option<VersionAt>, FsPulseError> {
    let conn = Database::get_connection()?;
    let version = conn
        .query_row(
            "SELECT item_version, is_deleted, is_ignored FROM item_versions \
             WHERE item_id = ? AND first_seen_at <= ? \
             ORDER BY item_version DESC \
             LIMIT 1",
            params![item_id, at],
            |row| {
                Ok(VersionAt {
                    item_version: row.get(0)?,
                    is_deleted: row.get(1)?,
                    is_ignored: row.get(2)?,
                })
            },
        )
        .optional()?;
    Ok(version)
}

/// Count the files and folders beneath a folder, at any depth, that
/// were present at a time (Unix seconds, inclusive).
pub fn get_children_counts(
    item_id: i64,
    at: i64,
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
          AND iv.first_seen_at = (
              SELECT MAX(first_seen_at) FROM item_versions
              WHERE item_id = i.item_id AND first_seen_at <= ?2
          )
          AND iv.is_deleted = 0
          AND i.item_path >= ?3
          AND i.item_path < ?4
          AND i.item_path != ?5
          AND (i.item_type = 0 OR i.item_type = 1)
        GROUP BY i.item_type"#;

    let mut stmt = conn.prepare_cached(sql)?;
    let rows = stmt.query_map(params![root_id, at, path_prefix, path_upper, parent_path], |row| {
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

// ---- Hash history types and functions ----

/// A single hash version entry for the hash history of an item version
#[derive(Clone, Debug, Serialize)]
pub struct HashHistoryEntry {
    pub first_seen_at: i64,
    pub last_seen_at: i64,
    #[serde(serialize_with = "serialize_hash")]
    pub file_hash: Vec<u8>,
    pub hash_state: i64,
}

impl HashHistoryEntry {
    fn from_row(row: &rusqlite::Row) -> rusqlite::Result<Self> {
        Ok(HashHistoryEntry {
            first_seen_at: row.get(0)?,
            last_seen_at: row.get(1)?,
            file_hash: row.get(2)?,
            hash_state: row.get(3)?,
        })
    }
}

/// Get all hash versions for a given item_id and item_version, ordered chronologically.
pub fn get_hash_history(
    item_id: i64,
    item_version: i64,
) -> Result<Vec<HashHistoryEntry>, FsPulseError> {
    let conn = Database::get_connection()?;

    let sql = r#"
        SELECT hv.first_seen_at, hv.last_seen_at, hv.file_hash, hv.hash_state
        FROM hash_versions hv
        WHERE hv.item_id = ? AND hv.item_version = ?
        ORDER BY hv.first_seen_at ASC"#;

    let mut stmt = conn.prepare_cached(sql)?;
    let rows = stmt.query_map(
        params![item_id, item_version],
        HashHistoryEntry::from_row,
    )?;

    let mut history = Vec::new();
    for row in rows {
        history.push(row?);
    }

    Ok(history)
}

/// Response for integrity state of an item at a specific scan point
#[derive(Clone, Debug, Serialize)]
pub struct IntegrityState {
    pub has_validator: bool,
    pub do_not_validate: bool,
    pub hash_state: Option<i64>,
    pub file_hash: Option<Vec<u8>>,
    pub val_state: Option<i64>,
    pub val_error: Option<String>,
}

/// Get the integrity state (hash + validation) for an item at a specific scan point.
/// Queries hash_versions (keyed on item_id, item_version) and val columns on item_versions,
/// plus has_validator from the items table.
pub fn get_integrity_state(
    item_id: i64,
    at: i64,
) -> Result<IntegrityState, FsPulseError> {
    let conn = Database::get_connection()?;

    let (has_validator, do_not_validate) = conn
        .query_row(
            "SELECT has_validator, do_not_validate FROM items WHERE item_id = ?",
            params![item_id],
            |row| Ok((
                row.get::<_, i64>(0).map(|v| v != 0).unwrap_or(false),
                row.get::<_, i64>(1).map(|v| v != 0).unwrap_or(false),
            )),
        )
        .optional()?
        .unwrap_or((false, false));

    // Get the version active at this scan point
    let version_row: Option<(i64, Option<i64>, Option<String>)> = conn
        .query_row(
            "SELECT item_version, val_state, val_error FROM item_versions \
             WHERE item_id = ?1 \
               AND first_seen_at = ( \
                   SELECT MAX(first_seen_at) FROM item_versions \
                   WHERE item_id = ?1 AND first_seen_at <= ?2 \
               )",
            params![item_id, at],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()?;

    let (item_version, val_state, val_error) = match version_row {
        Some((iv, vs, ve)) => (iv, vs, ve),
        None => return Ok(IntegrityState {
            has_validator,
            do_not_validate,
            hash_state: None,
            file_hash: None,
            val_state: None,
            val_error: None,
        }),
    };

    let hash_row: Option<(i64, Option<Vec<u8>>)> = conn
        .query_row(
            "SELECT hash_state, file_hash FROM hash_versions \
             WHERE item_id = ?1 AND item_version = ?2 \
               AND first_seen_at = ( \
                   SELECT MAX(first_seen_at) FROM hash_versions \
                   WHERE item_id = ?1 AND item_version = ?2 \
               )",
            params![item_id, item_version],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;

    Ok(IntegrityState {
        has_validator,
        do_not_validate,
        hash_state: hash_row.as_ref().map(|(s, _)| *s),
        file_hash: hash_row.and_then(|(_, h)| h),
        val_state,
        val_error,
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
