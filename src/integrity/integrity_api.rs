use rusqlite::types::Value;

use crate::{db::Database, error::FsPulseError};

// ---------------------------------------------------------------------------
// Shared query parameters and SQL builders
// ---------------------------------------------------------------------------

/// Filter parameters shared by count, items, and versions queries.
pub struct IntegrityFilter {
    pub root_id: i64,
    /// "val", "hash", or None for all
    pub issue_type: Option<String>,
    /// Lowercase extensions to filter by (empty = no filter)
    pub extensions: Vec<String>,
    /// "unreviewed" (default), "reviewed", or "all"
    pub status: String,
    /// Substring match on item_path
    pub path_search: Option<String>,
    /// If false (default), exclude versions where is_deleted = 1
    pub show_deleted: bool,
}

/// This version has a validation issue.
const HAS_VAL: &str = "iv.val_state = 2";

/// This version has at least one suspect hash.
const HAS_HASH: &str = "EXISTS (
    SELECT 1 FROM hash_versions hv
    WHERE hv.item_id = iv.item_id
      AND hv.item_version = iv.item_version
      AND hv.hash_state = 2
)";

/// This version has an unreviewed validation issue.
const VAL_UNREVIEWED: &str = "(iv.val_state = 2 AND iv.val_reviewed_at IS NULL)";

/// This version has an unreviewed hash issue.
const HASH_UNREVIEWED: &str = "(iv.hash_reviewed_at IS NULL AND EXISTS (
    SELECT 1 FROM hash_versions hv
    WHERE hv.item_id = iv.item_id
      AND hv.item_version = iv.item_version
      AND hv.hash_state = 2
))";

/// Build the version-level inclusion predicate from issue_type + status.
fn build_inclusion(f: &IntegrityFilter) -> String {
    let issue_type = f.issue_type.as_deref().unwrap_or("all");

    match (issue_type, f.status.as_str()) {
        ("val", "unreviewed") => VAL_UNREVIEWED.to_string(),
        ("val", "reviewed") => format!("{HAS_VAL} AND iv.val_reviewed_at IS NOT NULL"),
        ("val", _) => HAS_VAL.to_string(),

        ("hash", "unreviewed") => HASH_UNREVIEWED.to_string(),
        ("hash", "reviewed") => format!("{HAS_HASH} AND iv.hash_reviewed_at IS NOT NULL"),
        ("hash", _) => HAS_HASH.to_string(),

        (_, "unreviewed") => format!("({VAL_UNREVIEWED} OR {HASH_UNREVIEWED})"),
        (_, "reviewed") => format!(
            "({HAS_VAL} OR {HAS_HASH}) AND NOT {VAL_UNREVIEWED} AND NOT {HASH_UNREVIEWED}"
        ),
        _ => format!("({HAS_VAL} OR {HAS_HASH})"),
    }
}

/// Build extra WHERE clauses for path search and extension filtering.
fn build_extra_where(f: &IntegrityFilter) -> (Vec<String>, Vec<Value>) {
    let mut clauses: Vec<String> = vec![];
    let mut vals: Vec<Value> = vec![];

    if let Some(search) = &f.path_search {
        clauses.push("i.item_path LIKE ?".to_string());
        vals.push(Value::Text(format!("%{}%", search)));
    }

    if !f.extensions.is_empty() {
        let placeholders = f.extensions.iter().map(|_| "?").collect::<Vec<_>>().join(", ");
        clauses.push(format!("i.file_extension IN ({placeholders})"));
        for ext in &f.extensions {
            vals.push(Value::Text(ext.clone()));
        }
    }

    (clauses, vals)
}

/// Build the full WHERE clause for version-level queries (items iv JOIN items i).
fn build_version_where(f: &IntegrityFilter) -> (String, Vec<Value>) {
    let inclusion = build_inclusion(f);
    let (extra_clauses, extra_vals) = build_extra_where(f);

    let mut parts = vec![
        "i.root_id = ?".to_string(),
        "i.item_type = 0".to_string(),
        format!("({inclusion})"),
    ];
    if !f.show_deleted {
        parts.push("iv.is_deleted = 0".to_string());
    }
    parts.extend(extra_clauses);

    let mut vals: Vec<Value> = vec![Value::Integer(f.root_id)];
    vals.extend(extra_vals);

    (parts.join(" AND "), vals)
}

// ---------------------------------------------------------------------------
// Count: distinct items matching filters
// ---------------------------------------------------------------------------

pub fn count_items(f: &IntegrityFilter) -> Result<i64, FsPulseError> {
    let conn = Database::get_connection()?;
    let (where_clause, vals) = build_version_where(f);

    let sql = format!(
        "SELECT COUNT(DISTINCT i.item_id)
         FROM item_versions iv
         JOIN items i ON i.item_id = iv.item_id
         WHERE {where_clause}"
    );
    let refs: Vec<&dyn rusqlite::ToSql> = vals.iter().map(|v| v as &dyn rusqlite::ToSql).collect();
    let total: i64 = conn.query_row(&sql, refs.as_slice(), |row| row.get(0))?;
    Ok(total)
}

// ---------------------------------------------------------------------------
// Items: page of items with server-computed summary counts
// ---------------------------------------------------------------------------

pub struct IntegrityItemSummary {
    pub item_id: i64,
    pub item_path: String,
    pub item_name: String,
    pub file_extension: Option<String>,
    pub do_not_validate: bool,
    pub latest_scan_id: i64,
    pub hash_unreviewed: i64,
    pub hash_reviewed: i64,
    pub val_unreviewed: i64,
    pub val_reviewed: i64,
}

pub fn query_items(
    f: &IntegrityFilter,
    offset: i64,
    limit: i64,
) -> Result<Vec<IntegrityItemSummary>, FsPulseError> {
    let conn = Database::get_connection()?;
    let (where_clause, mut vals) = build_version_where(f);

    vals.push(Value::Integer(limit));
    vals.push(Value::Integer(offset));

    let issue_type = f.issue_type.as_deref().unwrap_or("all");

    // Count expressions respect the issue_type filter:
    // When issue_type is "hash", val counts are 0. When "val", hash counts are 0.
    // The WHERE clause already filters to matching versions (issue_type + status),
    // so these counts reflect exactly what passes the filters.
    let (hash_unrev_expr, hash_rev_expr) = if issue_type == "val" {
        ("0".to_string(), "0".to_string())
    } else {
        (
            format!("SUM(CASE WHEN {HAS_HASH} AND iv.hash_reviewed_at IS NULL THEN 1 ELSE 0 END)"),
            format!("SUM(CASE WHEN {HAS_HASH} AND iv.hash_reviewed_at IS NOT NULL THEN 1 ELSE 0 END)"),
        )
    };

    let (val_unrev_expr, val_rev_expr) = if issue_type == "hash" {
        ("0".to_string(), "0".to_string())
    } else {
        (
            format!("SUM(CASE WHEN {HAS_VAL} AND iv.val_reviewed_at IS NULL THEN 1 ELSE 0 END)"),
            format!("SUM(CASE WHEN {HAS_VAL} AND iv.val_reviewed_at IS NOT NULL THEN 1 ELSE 0 END)"),
        )
    };

    let sql = format!(
        "SELECT
             i.item_id,
             i.item_path,
             i.item_name,
             i.file_extension,
             i.do_not_validate,
             {hash_unrev_expr} AS hash_unreviewed,
             {hash_rev_expr} AS hash_reviewed,
             {val_unrev_expr} AS val_unreviewed,
             {val_rev_expr} AS val_reviewed,
             MAX(iv.last_scan_id) AS latest_scan_id
         FROM item_versions iv
         JOIN items i ON i.item_id = iv.item_id
         WHERE {where_clause}
         GROUP BY i.item_id
         ORDER BY i.item_path COLLATE natural_path ASC, i.item_id ASC
         LIMIT ? OFFSET ?"
    );

    let refs: Vec<&dyn rusqlite::ToSql> = vals.iter().map(|v| v as &dyn rusqlite::ToSql).collect();
    let mut stmt = conn.prepare(&sql)?;
    let items = stmt
        .query_map(refs.as_slice(), |row| {
            Ok(IntegrityItemSummary {
                item_id: row.get(0)?,
                item_path: row.get(1)?,
                item_name: row.get(2)?,
                file_extension: row.get(3)?,
                do_not_validate: row.get::<_, i64>(4)? != 0,
                hash_unreviewed: row.get(5)?,
                hash_reviewed: row.get(6)?,
                val_unreviewed: row.get(7)?,
                val_reviewed: row.get(8)?,
                latest_scan_id: row.get(9)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    Ok(items)
}

// ---------------------------------------------------------------------------
// Versions: filtered versions for a single item
// ---------------------------------------------------------------------------

pub struct IntegrityVersion {
    pub item_version: i64,
    pub scan_id: i64,
    pub scan_started_at: i64,
    pub hash_version_count: i64,
    pub hash_suspicious_count: i64,
    pub val_state: Option<i64>,
    pub val_error: Option<String>,
    pub val_reviewed_at: Option<i64>,
    pub hash_reviewed_at: Option<i64>,
}

pub struct IntegrityVersionResult {
    pub versions: Vec<IntegrityVersion>,
    pub total: i64,
}

pub fn query_versions(
    f: &IntegrityFilter,
    item_id: i64,
    limit: i64,
) -> Result<IntegrityVersionResult, FsPulseError> {
    let conn = Database::get_connection()?;
    let inclusion = build_inclusion(f);

    // WHERE: specific item + inclusion filter (no path_search or extensions needed)
    let where_clause = format!("iv.item_id = ? AND ({inclusion})");
    let base_vals: Vec<Value> = vec![Value::Integer(item_id)];

    // Count
    let count_sql = format!(
        "SELECT COUNT(*)
         FROM item_versions iv
         WHERE {where_clause}"
    );
    let count_refs: Vec<&dyn rusqlite::ToSql> =
        base_vals.iter().map(|v| v as &dyn rusqlite::ToSql).collect();
    let total: i64 = conn.query_row(&count_sql, count_refs.as_slice(), |row| row.get(0))?;

    // Fetch
    let mut fetch_vals = base_vals;
    fetch_vals.push(Value::Integer(limit));

    let fetch_sql = format!(
        "SELECT
             iv.item_version,
             iv.first_scan_id,
             s.started_at,
             (SELECT COUNT(*) FROM hash_versions hv
              WHERE hv.item_id = iv.item_id
                AND hv.item_version = iv.item_version
             ) AS hash_version_count,
             (SELECT COUNT(*) FROM hash_versions hv
              WHERE hv.item_id = iv.item_id
                AND hv.item_version = iv.item_version
                AND hv.hash_state = 2
             ) AS hash_suspicious_count,
             iv.val_state,
             iv.val_error,
             iv.val_reviewed_at,
             iv.hash_reviewed_at
         FROM item_versions iv
         JOIN scans s ON s.scan_id = iv.first_scan_id
         WHERE {where_clause}
         ORDER BY iv.item_version DESC
         LIMIT ?"
    );

    let fetch_refs: Vec<&dyn rusqlite::ToSql> =
        fetch_vals.iter().map(|v| v as &dyn rusqlite::ToSql).collect();
    let mut stmt = conn.prepare(&fetch_sql)?;
    let versions = stmt
        .query_map(fetch_refs.as_slice(), |row| {
            Ok(IntegrityVersion {
                item_version: row.get(0)?,
                scan_id: row.get(1)?,
                scan_started_at: row.get(2)?,
                hash_version_count: row.get(3)?,
                hash_suspicious_count: row.get(4)?,
                val_state: row.get(5)?,
                val_error: row.get(6)?,
                val_reviewed_at: row.get(7)?,
                hash_reviewed_at: row.get(8)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    Ok(IntegrityVersionResult { versions, total })
}

// ---------------------------------------------------------------------------
// Review: mark reviewed on item or specific version
// ---------------------------------------------------------------------------

/// Set or clear val_reviewed_at and/or hash_reviewed_at.
///
/// If `item_version` is Some, targets that specific version.
/// If `item_version` is None, targets all versions of the item that have
/// the relevant issue (val_state=2 for val, suspect hashes for hash).
///
/// A single timestamp is used for all updates in the call.
pub fn set_reviewed(
    item_id: i64,
    item_version: Option<i64>,
    set_val: Option<bool>,
    set_hash: Option<bool>,
) -> Result<(), FsPulseError> {
    let conn = Database::get_connection()?;
    let now = chrono::Utc::now().timestamp();

    if let Some(val) = set_val {
        let ts: Option<i64> = if val { Some(now) } else { None };
        match item_version {
            Some(v) => {
                conn.execute(
                    "UPDATE item_versions SET val_reviewed_at = ?
                     WHERE item_id = ? AND item_version = ? AND val_state = 2",
                    rusqlite::params![ts, item_id, v],
                )?;
            }
            None => {
                conn.execute(
                    "UPDATE item_versions SET val_reviewed_at = ?
                     WHERE item_id = ? AND val_state = 2",
                    rusqlite::params![ts, item_id],
                )?;
            }
        }
    }

    if let Some(val) = set_hash {
        let ts: Option<i64> = if val { Some(now) } else { None };
        let version_filter = match item_version {
            Some(v) => format!("AND item_versions.item_version = {v}"),
            None => String::new(),
        };
        conn.execute(
            &format!(
                "UPDATE item_versions SET hash_reviewed_at = ?
                 WHERE item_id = ?
                   {version_filter}
                   AND EXISTS (
                       SELECT 1 FROM hash_versions hv
                       WHERE hv.item_id = item_versions.item_id
                         AND hv.item_version = item_versions.item_version
                         AND hv.hash_state = 2
                   )"
            ),
            rusqlite::params![ts, item_id],
        )?;
    }

    Ok(())
}

/// Toggle do_not_validate on an item.
pub fn set_do_not_validate(item_id: i64, do_not_validate: bool) -> Result<(), FsPulseError> {
    let conn = Database::get_connection()?;
    conn.execute(
        "UPDATE items SET do_not_validate = ? WHERE item_id = ?",
        rusqlite::params![do_not_validate as i64, item_id],
    )?;
    Ok(())
}
