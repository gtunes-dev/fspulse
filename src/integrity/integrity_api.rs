use rusqlite::{params, types::Value};

use crate::{db::Database, error::FsPulseError};

/// Parameters for the integrity list query.
pub struct IntegrityQuery {
    pub root_id: i64,
    /// "val", "hash", or None / "all"
    pub issue_type: Option<String>,
    /// Lowercase extensions to filter by (empty = no filter)
    pub extensions: Vec<String>,
    /// "unacknowledged" (default), "acknowledged", or "all"
    pub status: String,
    /// Substring match on item_path
    pub path_search: Option<String>,
    pub offset: i64,
    pub limit: i64,
}

/// A single row returned by the integrity list query.
pub struct IntegrityItem {
    pub item_id: i64,
    pub item_path: String,
    pub item_name: String,
    pub file_extension: Option<String>,
    pub do_not_validate: bool,
    pub item_version: i64,
    pub val_state: Option<i64>,
    pub val_reviewed_at: Option<i64>,
    pub hash_state: Option<i64>,
    pub hash_reviewed_at: Option<i64>,
    pub first_scan_id: i64,
    pub first_detected_at: i64,
}

pub struct IntegrityQueryResult {
    pub items: Vec<IntegrityItem>,
    pub total: i64,
    pub offset: i64,
    pub limit: i64,
}

/// The FROM + LEFT JOIN block shared by count and fetch queries.
/// Driven by idx_versions_root_lastscan (Pattern 1 from schema comments).
const BASE_FROM: &str =
    "FROM item_versions cv
     JOIN items i ON i.item_id = cv.item_id
     LEFT JOIN hash_versions hv
         ON hv.item_id = cv.item_id
         AND hv.item_version = cv.item_version
         AND hv.first_scan_id = (
             SELECT MAX(first_scan_id) FROM hash_versions
             WHERE item_id = cv.item_id AND item_version = cv.item_version
         )
     JOIN scans s ON s.scan_id = cv.first_scan_id";

/// Build the WHERE clause and positional bind values for an IntegrityQuery.
/// Returns (where_clause_string, values).
fn build_where(q: &IntegrityQuery, scan_id: i64) -> (String, Vec<Value>) {
    let mut conds: Vec<String> = vec![
        "cv.root_id = ?".to_string(),
        "cv.last_scan_id = ?".to_string(),
        "cv.is_deleted = 0".to_string(),
        "i.item_type = 0".to_string(),
    ];
    let mut vals: Vec<Value> = vec![Value::Integer(q.root_id), Value::Integer(scan_id)];

    // Issue type
    match q.issue_type.as_deref() {
        Some("val") => conds.push("cv.val_state = 2".to_string()),
        Some("hash") => conds.push("hv.hash_state = 2".to_string()),
        _ => conds.push("(cv.val_state = 2 OR hv.hash_state = 2)".to_string()),
    }

    // Acknowledgment status
    match q.status.as_str() {
        "acknowledged" => conds.push(
            "((cv.val_state IS NULL OR cv.val_state != 2 OR cv.val_reviewed_at IS NOT NULL) \
              AND (hv.hash_state IS NULL OR hv.hash_state != 2 OR cv.hash_reviewed_at IS NOT NULL))"
                .to_string(),
        ),
        "all" => {}
        // "unacknowledged" is the default
        _ => conds.push(
            "((cv.val_state = 2 AND cv.val_reviewed_at IS NULL) \
              OR (hv.hash_state = 2 AND cv.hash_reviewed_at IS NULL))"
                .to_string(),
        ),
    }

    // Path search
    if let Some(search) = &q.path_search {
        conds.push("i.item_path LIKE ?".to_string());
        vals.push(Value::Text(format!("%{}%", search)));
    }

    // Extension filter
    if !q.extensions.is_empty() {
        let placeholders = q.extensions.iter().map(|_| "?").collect::<Vec<_>>().join(", ");
        conds.push(format!("i.file_extension IN ({})", placeholders));
        for ext in &q.extensions {
            vals.push(Value::Text(ext.clone()));
        }
    }

    (conds.join(" AND "), vals)
}

/// Fetch the integrity list for a root, with filtering and pagination.
pub fn query_integrity(q: &IntegrityQuery) -> Result<IntegrityQueryResult, FsPulseError> {
    let conn = Database::get_connection()?;

    // Find the most recent completed scan for this root.
    // Using state = 4 (Completed) matches the analysis phase convention.
    let latest_scan_id: Option<i64> = conn.query_row(
        "SELECT MAX(scan_id) FROM scans WHERE root_id = ? AND state = 4",
        params![q.root_id],
        |row| row.get(0),
    )?;

    let Some(scan_id) = latest_scan_id else {
        return Ok(IntegrityQueryResult {
            items: vec![],
            total: 0,
            offset: q.offset,
            limit: q.limit,
        });
    };

    let (where_clause, base_vals) = build_where(q, scan_id);

    // --- Count ---
    let count_sql = format!("SELECT COUNT(*) {BASE_FROM} WHERE {where_clause}");
    let count_refs: Vec<&dyn rusqlite::ToSql> =
        base_vals.iter().map(|v| v as &dyn rusqlite::ToSql).collect();
    let total: i64 =
        conn.query_row(&count_sql, count_refs.as_slice(), |row| row.get(0))?;

    // --- Fetch ---
    let mut fetch_vals = base_vals;
    fetch_vals.push(Value::Integer(q.limit));
    fetch_vals.push(Value::Integer(q.offset));

    let fetch_sql = format!(
        "SELECT
             i.item_id,
             i.item_path,
             i.item_name,
             i.file_extension,
             i.do_not_validate,
             cv.item_version,
             cv.val_state,
             cv.val_reviewed_at,
             cv.hash_reviewed_at,
             hv.hash_state,
             cv.first_scan_id,
             s.started_at
         {BASE_FROM}
         WHERE {where_clause}
         ORDER BY cv.item_id ASC
         LIMIT ? OFFSET ?"
    );

    let fetch_refs: Vec<&dyn rusqlite::ToSql> =
        fetch_vals.iter().map(|v| v as &dyn rusqlite::ToSql).collect();
    let mut stmt = conn.prepare(&fetch_sql)?;
    let items = stmt
        .query_map(fetch_refs.as_slice(), |row| {
            Ok(IntegrityItem {
                item_id: row.get(0)?,
                item_path: row.get(1)?,
                item_name: row.get(2)?,
                file_extension: row.get(3)?,
                do_not_validate: row.get::<_, i64>(4)? != 0,
                item_version: row.get(5)?,
                val_state: row.get(6)?,
                val_reviewed_at: row.get(7)?,
                hash_reviewed_at: row.get(8)?,
                hash_state: row.get(9)?,
                first_scan_id: row.get(10)?,
                first_detected_at: row.get(11)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    Ok(IntegrityQueryResult {
        items,
        total,
        offset: q.offset,
        limit: q.limit,
    })
}

/// Set val_reviewed_at and/or hash_reviewed_at on an item_version.
pub fn review_integrity(
    item_id: i64,
    item_version: i64,
    review_val: bool,
    review_hash: bool,
) -> Result<(), FsPulseError> {
    let conn = Database::get_connection()?;
    let now = chrono::Utc::now().timestamp();

    if review_val {
        conn.execute(
            "UPDATE item_versions SET val_reviewed_at = ?
             WHERE item_id = ? AND item_version = ?",
            params![now, item_id, item_version],
        )?;
    }

    if review_hash {
        conn.execute(
            "UPDATE item_versions SET hash_reviewed_at = ?
             WHERE item_id = ? AND item_version = ?",
            params![now, item_id, item_version],
        )?;
    }

    Ok(())
}

/// Toggle do_not_validate on an item.
pub fn set_do_not_validate(item_id: i64, do_not_validate: bool) -> Result<(), FsPulseError> {
    let conn = Database::get_connection()?;
    conn.execute(
        "UPDATE items SET do_not_validate = ? WHERE item_id = ?",
        params![do_not_validate as i64, item_id],
    )?;
    Ok(())
}
