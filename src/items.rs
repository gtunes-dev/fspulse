use rusqlite::{self, params, OptionalExtension};
use serde::{Deserialize, Serialize, Serializer};
use std::path::MAIN_SEPARATOR_STR;

use crate::{
    db::Database, error::FsPulseError, utils::Utils,
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
    pub has_validator: bool,
    pub first_scan_id: i64,
    pub is_added: bool,
    pub is_deleted: bool,
    pub mod_date: Option<i64>,
    pub size: Option<i64>,
    pub add_count: Option<i64>,
    pub modify_count: Option<i64>,
    pub delete_count: Option<i64>,
    pub unchanged_count: Option<i64>,
    // Integrity state (NULL if never hashed/validated)
    pub val_state: Option<i64>,
    pub hash_state: Option<i64>,
}

/// Get immediate children at a point in time using items + item_versions.
/// Returns the effective version of each immediate child of `parent_path`
/// as of `scan_id`.
/// Common WHERE clause fragment for temporal immediate children queries.
fn temporal_children_where(parent_path: &str) -> (String, String, String) {
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

    let where_fragment = format!(
        "WHERE i.root_id = ?1
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
           AND SUBSTR(i.item_path, LENGTH(?3) + 1) NOT LIKE '%{}%'",
        MAIN_SEPARATOR_STR
    );

    (path_prefix, path_upper, where_fragment)
}

/// Count immediate children of a directory at a point in time.
pub fn count_temporal_immediate_children(
    root_id: i64,
    parent_path: &str,
    scan_id: i64,
) -> Result<i64, FsPulseError> {
    let conn = Database::get_connection()?;
    let (path_prefix, path_upper, where_fragment) = temporal_children_where(parent_path);

    let sql = format!(
        "SELECT COUNT(*)
         FROM items i
         JOIN item_versions iv ON iv.item_id = i.item_id
         LEFT JOIN hash_versions hv ON hv.item_id = i.item_id
             AND hv.item_version = iv.item_version
             AND hv.first_scan_id = (
                 SELECT MAX(first_scan_id) FROM hash_versions
                 WHERE item_id = i.item_id AND item_version = iv.item_version
             )
         {}",
        where_fragment
    );

    let count: i64 = conn
        .prepare(&sql)?
        .query_row(
            params![root_id, scan_id, &path_prefix, &path_upper, parent_path],
            |row| row.get(0),
        )?;

    Ok(count)
}

pub fn get_temporal_immediate_children(
    root_id: i64,
    parent_path: &str,
    scan_id: i64,
    limit: Option<i64>,
    offset: Option<i64>,
) -> Result<Vec<TemporalTreeItem>, FsPulseError> {
    let conn = Database::get_connection()?;
    let (path_prefix, path_upper, where_fragment) = temporal_children_where(parent_path);

    let limit_clause = match limit {
        Some(l) => format!("\n         LIMIT {}", l),
        None => String::new(),
    };
    let offset_clause = match offset {
        Some(o) if o > 0 => format!("\n         OFFSET {}", o),
        _ => String::new(),
    };

    // Query now includes hierarchy_id from items for descendant count computation
    let sql = format!(
        "SELECT i.item_id, i.item_path, i.item_name, i.item_type, i.has_validator,
                iv.first_scan_id, iv.is_added, iv.is_deleted, iv.mod_date, iv.size,
                iv.add_count, iv.modify_count, iv.delete_count, iv.unchanged_count,
                iv.val_state, hv.hash_state,
                i.hierarchy_id
         FROM items i
         JOIN item_versions iv ON iv.item_id = i.item_id
         LEFT JOIN hash_versions hv ON hv.item_id = i.item_id
             AND hv.item_version = iv.item_version
             AND hv.first_scan_id = (
                 SELECT MAX(first_scan_id) FROM hash_versions
                 WHERE item_id = i.item_id AND item_version = iv.item_version
             )
         {}
         ORDER BY i.item_path COLLATE natural_path ASC{}{}",
        where_fragment, limit_clause, offset_clause
    );

    let mut stmt = conn.prepare(&sql)?;

    let rows = stmt.query_map(
        params![root_id, scan_id, &path_prefix, &path_upper, parent_path],
        |row| {
            Ok((
                TemporalTreeItem {
                    item_id: row.get(0)?,
                    item_path: row.get(1)?,
                    item_name: row.get(2)?,
                    item_type: ItemType::from_i64(row.get(3)?),
                    has_validator: row.get::<_, i64>(4)? != 0,
                    first_scan_id: row.get(5)?,
                    is_added: row.get(6)?,
                    is_deleted: row.get(7)?,
                    mod_date: row.get(8)?,
                    size: row.get(9)?,
                    add_count: row.get(10)?,
                    modify_count: row.get(11)?,
                    delete_count: row.get(12)?,
                    unchanged_count: row.get(13)?,
                    val_state: row.get(14)?,
                    hash_state: row.get(15)?,
                },
                row.get::<_, Option<Vec<u8>>>(16)?, // hierarchy_id
            ))
        },
    )?;

    let items_with_hid: Vec<(TemporalTreeItem, Option<Vec<u8>>)> =
        rows.collect::<Result<Vec<_>, _>>()?;
    drop(stmt);

    // Compute descendant change counts for directory children using hierarchy_id ranges.
    // For each folder, the subtree range is (folder_hid, next_sibling_hid).
    // Items are already sorted by path (= hierarchy_id order), so the next
    // item's hierarchy_id is the upper bound.
    compute_descendant_counts(&conn, root_id, scan_id, items_with_hid)
}

/// Compute descendant change counts for directory children using hierarchy_id
/// range queries on item_versions. Replaces the precomputed folder counts.
fn compute_descendant_counts(
    conn: &rusqlite::Connection,
    root_id: i64,
    scan_id: i64,
    items_with_hid: Vec<(TemporalTreeItem, Option<Vec<u8>>)>,
) -> Result<Vec<TemporalTreeItem>, FsPulseError> {
    if items_with_hid.is_empty() {
        return Ok(Vec::new());
    }

    // Build hierarchy_id list in order, so we can compute (hid, next_hid) ranges.
    // Clone so we don't borrow items_with_hid (which we consume below).
    let hids: Vec<Option<Vec<u8>>> = items_with_hid
        .iter()
        .map(|(_, hid)| hid.clone())
        .collect();

    // Prepared statement for descendant counts within a hierarchy range.
    // Uses idx_iv_root_firstscan_hid: (root_id, first_scan_id, hierarchy_id)
    let mut count_stmt_bounded = conn.prepare_cached(
        "SELECT
            COALESCE(SUM(CASE WHEN is_added = 1 THEN 1 ELSE 0 END), 0),
            COALESCE(SUM(CASE WHEN is_added = 0 AND is_deleted = 0 THEN 1 ELSE 0 END), 0),
            COALESCE(SUM(CASE WHEN is_deleted = 1 THEN 1 ELSE 0 END), 0)
         FROM item_versions
         WHERE root_id = ?1
           AND first_scan_id = ?2
           AND hierarchy_id > ?3
           AND hierarchy_id < ?4",
    )?;

    // For the last child, there's no upper bound from a next sibling.
    let mut count_stmt_unbounded = conn.prepare_cached(
        "SELECT
            COALESCE(SUM(CASE WHEN is_added = 1 THEN 1 ELSE 0 END), 0),
            COALESCE(SUM(CASE WHEN is_added = 0 AND is_deleted = 0 THEN 1 ELSE 0 END), 0),
            COALESCE(SUM(CASE WHEN is_deleted = 1 THEN 1 ELSE 0 END), 0)
         FROM item_versions
         WHERE root_id = ?1
           AND first_scan_id = ?2
           AND hierarchy_id > ?3",
    )?;

    let mut results = Vec::with_capacity(items_with_hid.len());

    for (idx, (mut item, hid)) in items_with_hid.into_iter().enumerate() {
        if item.item_type == ItemType::Directory && !item.is_deleted {
            if let Some(ref my_hid) = hid {
                // Find next sibling's hierarchy_id (next item in the sorted list)
                let next_hid = hids.get(idx + 1).and_then(|h| h.as_deref());

                let (adds, mods, dels): (i64, i64, i64) = if let Some(next) = next_hid {
                    count_stmt_bounded.query_row(
                        params![root_id, scan_id, my_hid, next],
                        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                    )?
                } else {
                    count_stmt_unbounded.query_row(
                        params![root_id, scan_id, my_hid],
                        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                    )?
                };

                item.add_count = Some(adds);
                item.modify_count = Some(mods);
                item.delete_count = Some(dels);
                // unchanged_count: use 1 as a sentinel to indicate the folder
                // has content, so the tree renders it as expandable. The exact
                // count isn't needed for the tree view — only that it's non-zero
                // when the folder is alive.
                item.unchanged_count = Some(1);
            }
        }

        results.push(item);
    }

    Ok(results)
}

/// Common WHERE fragment for temporal search queries.
const TEMPORAL_SEARCH_WHERE: &str =
    "WHERE i.root_id = ?1
       AND iv.first_scan_id = (
           SELECT MAX(first_scan_id)
           FROM item_versions
           WHERE item_id = i.item_id
             AND first_scan_id <= ?2
       )
       AND (iv.is_deleted = 0 OR iv.first_scan_id = ?2)
       AND i.item_name LIKE '%' || ?3 || '%'";

const TEMPORAL_SEARCH_FROM: &str =
    "FROM items i
     JOIN item_versions iv ON iv.item_id = i.item_id
     LEFT JOIN hash_versions hv ON hv.item_id = i.item_id
         AND hv.item_version = iv.item_version
         AND hv.first_scan_id = (
             SELECT MAX(first_scan_id) FROM hash_versions
             WHERE item_id = i.item_id AND item_version = iv.item_version
         )";

/// Count items matching a name search at a point in time.
pub fn count_temporal_search_items(
    root_id: i64,
    scan_id: i64,
    query: &str,
) -> Result<i64, FsPulseError> {
    let conn = Database::get_connection()?;

    let sql = format!(
        "SELECT COUNT(*) {} {}",
        TEMPORAL_SEARCH_FROM, TEMPORAL_SEARCH_WHERE
    );

    let count: i64 = conn
        .prepare(&sql)?
        .query_row(params![root_id, scan_id, query], |row| row.get(0))?;

    Ok(count)
}

/// Search for items by name at a point in time using items + item_versions.
/// Matches against the item name (last path segment) rather than the full path.
/// Returns items whose name contains the search query, ordered by path.
pub fn get_temporal_search_items(
    root_id: i64,
    scan_id: i64,
    query: &str,
    limit: Option<i64>,
    offset: Option<i64>,
) -> Result<Vec<TemporalTreeItem>, FsPulseError> {
    let conn = Database::get_connection()?;

    let limit_clause = match limit {
        Some(l) => format!("\n         LIMIT {}", l),
        None => String::new(),
    };
    let offset_clause = match offset {
        Some(o) if o > 0 => format!("\n         OFFSET {}", o),
        _ => String::new(),
    };

    let sql = format!(
        "SELECT i.item_id, i.item_path, i.item_name, i.item_type, i.has_validator,
                iv.first_scan_id, iv.is_added, iv.is_deleted, iv.mod_date, iv.size,
                iv.add_count, iv.modify_count, iv.delete_count, iv.unchanged_count,
                iv.val_state, hv.hash_state
         {} {} ORDER BY i.item_path COLLATE natural_path ASC{}{}",
        TEMPORAL_SEARCH_FROM, TEMPORAL_SEARCH_WHERE, limit_clause, offset_clause
    );

    let mut stmt = conn.prepare(&sql)?;

    let rows = stmt.query_map(
        params![root_id, scan_id, query],
        |row| {
            Ok(TemporalTreeItem {
                item_id: row.get(0)?,
                item_path: row.get(1)?,
                item_name: row.get(2)?,
                item_type: ItemType::from_i64(row.get(3)?),
                has_validator: row.get::<_, i64>(4)? != 0,
                first_scan_id: row.get(5)?,
                is_added: row.get(6)?,
                is_deleted: row.get(7)?,
                mod_date: row.get(8)?,
                size: row.get(9)?,
                add_count: row.get(10)?,
                modify_count: row.get(11)?,
                delete_count: row.get(12)?,
                unchanged_count: row.get(13)?,
                val_state: row.get(14)?,
                hash_state: row.get(15)?,
            })
        },
    )?;

    let items: Vec<TemporalTreeItem> = rows.collect::<Result<Vec<_>, _>>()?;

    Ok(items)
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
    pub first_scan_id: i64,
    pub last_scan_id: i64,
    pub first_scan_date: i64,
    pub last_scan_date: i64,
    pub is_added: bool,
    pub is_deleted: bool,
    pub access: i64,
    pub mod_date: Option<i64>,
    pub size: Option<i64>,
    // Folder counts (NULL for files)
    pub add_count: Option<i64>,
    pub modify_count: Option<i64>,
    pub delete_count: Option<i64>,
    pub unchanged_count: Option<i64>,
    // Integrity fields (NULL for non-files or when no record exists)
    pub hash_state: Option<i64>,
    #[serde(serialize_with = "serialize_optional_hash")]
    pub file_hash: Option<Vec<u8>>,
    pub val_state: Option<i64>,
    pub val_error: Option<String>,
    // Review timestamps
    pub val_reviewed_at: Option<i64>,
    pub hash_reviewed_at: Option<i64>,
}

impl VersionHistoryEntry {
    fn from_row(row: &rusqlite::Row) -> rusqlite::Result<Self> {
        Ok(VersionHistoryEntry {
            item_version: row.get(0)?,
            first_scan_id: row.get(1)?,
            last_scan_id: row.get(2)?,
            first_scan_date: row.get(3)?,
            last_scan_date: row.get(4)?,
            is_added: row.get(5)?,
            is_deleted: row.get(6)?,
            access: row.get(7)?,
            mod_date: row.get(8)?,
            size: row.get(9)?,
            add_count: row.get(10)?,
            modify_count: row.get(11)?,
            delete_count: row.get(12)?,
            unchanged_count: row.get(13)?,
            hash_state: row.get(14)?,
            file_hash: row.get(15)?,
            val_state: row.get(16)?,
            val_error: row.get(17)?,
            val_reviewed_at: row.get(18)?,
            hash_reviewed_at: row.get(19)?,
        })
    }
}

const VERSION_HISTORY_COLUMNS: &str =
    "v.item_version, v.first_scan_id, v.last_scan_id, \
     s1.started_at, s2.started_at, \
     v.is_added, v.is_deleted, v.access, \
     v.mod_date, v.size, \
     v.add_count, v.modify_count, v.delete_count, v.unchanged_count, \
     hv.hash_state, hv.file_hash, v.val_state, v.val_error, \
     v.val_reviewed_at, v.hash_reviewed_at";

const VERSION_HISTORY_JOINS: &str =
    "JOIN scans s1 ON s1.scan_id = v.first_scan_id \
     JOIN scans s2 ON s2.scan_id = v.last_scan_id \
     LEFT JOIN hash_versions hv ON hv.item_id = v.item_id \
       AND hv.item_version = v.item_version \
       AND hv.first_scan_id = ( \
           SELECT MAX(hv2.first_scan_id) FROM hash_versions hv2 \
           WHERE hv2.item_id = v.item_id AND hv2.item_version = v.item_version \
       )";

/// Count total versions for an item.
pub fn count_versions(item_id: i64) -> Result<i64, FsPulseError> {
    let conn = Database::get_connection()?;
    let total: i64 = conn.query_row(
        "SELECT COUNT(*) FROM item_versions WHERE item_id = ?",
        params![item_id],
        |row| row.get(0),
    )?;
    Ok(total)
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

/// Find the item_version that was active at a given scan_id.
/// Returns the version where first_scan_id <= scan_id <= last_scan_id.
pub fn get_version_at_scan(item_id: i64, scan_id: i64) -> Result<Option<i64>, FsPulseError> {
    let conn = Database::get_connection()?;
    let version: Option<i64> = conn
        .query_row(
            "SELECT item_version FROM item_versions \
             WHERE item_id = ? AND first_scan_id <= ? AND last_scan_id >= ? \
             LIMIT 1",
            params![item_id, scan_id, scan_id],
            |row| row.get(0),
        )
        .optional()?;
    Ok(version)
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

// ---- Hash history types and functions ----

/// A single hash version entry for the hash history of an item version
#[derive(Clone, Debug, Serialize)]
pub struct HashHistoryEntry {
    pub first_scan_id: i64,
    pub last_scan_id: i64,
    pub scan_started_at: i64,
    #[serde(serialize_with = "serialize_hash")]
    pub file_hash: Vec<u8>,
    pub hash_state: i64,
}

impl HashHistoryEntry {
    fn from_row(row: &rusqlite::Row) -> rusqlite::Result<Self> {
        Ok(HashHistoryEntry {
            first_scan_id: row.get(0)?,
            last_scan_id: row.get(1)?,
            scan_started_at: row.get(2)?,
            file_hash: row.get(3)?,
            hash_state: row.get(4)?,
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
        SELECT hv.first_scan_id, hv.last_scan_id, s.started_at, hv.file_hash, hv.hash_state
        FROM hash_versions hv
        JOIN scans s ON s.scan_id = hv.first_scan_id
        WHERE hv.item_id = ? AND hv.item_version = ?
        ORDER BY hv.first_scan_id ASC"#;

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
    scan_id: i64,
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
               AND first_scan_id = ( \
                   SELECT MAX(first_scan_id) FROM item_versions \
                   WHERE item_id = ?1 AND first_scan_id <= ?2 \
               )",
            params![item_id, scan_id],
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
               AND first_scan_id = ( \
                   SELECT MAX(first_scan_id) FROM hash_versions \
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
