use rusqlite::{self, params, Connection, OptionalExtension};

use crate::{error::FsPulseError, item_identity::Access, scans::AnalysisSpec, undo_log::UndoLog, validate::validator::ValidationState};

/// A single temporal version of an item.
///
/// Maps to the `item_versions` table. Each row represents one distinct state of an item.
/// A new row is created only when observable state changes. Identity (path, type, root)
/// comes from JOINing to the `items` table.
#[allow(dead_code)]
pub struct ItemVersion {
    version_id: i64,
    first_scan_id: i64,
    last_scan_id: i64,
    is_added: bool,
    is_deleted: bool,
    access: Access,
    mod_date: Option<i64>,
    size: Option<i64>,
    file_hash: Option<String>,
    val: Option<ValidationState>,
    val_error: Option<String>,
    last_hash_scan: Option<i64>,
    last_val_scan: Option<i64>,
    add_count: Option<i64>,
    modify_count: Option<i64>,
    delete_count: Option<i64>,
    unchanged_count: Option<i64>,
}

#[allow(dead_code, clippy::too_many_arguments)]
impl ItemVersion {
    pub fn version_id(&self) -> i64 {
        self.version_id
    }

    pub fn first_scan_id(&self) -> i64 {
        self.first_scan_id
    }

    pub fn last_scan_id(&self) -> i64 {
        self.last_scan_id
    }

    pub fn is_added(&self) -> bool {
        self.is_added
    }

    pub fn is_deleted(&self) -> bool {
        self.is_deleted
    }

    pub fn access(&self) -> Access {
        self.access
    }

    pub fn mod_date(&self) -> Option<i64> {
        self.mod_date
    }

    pub fn size(&self) -> Option<i64> {
        self.size
    }

    pub fn file_hash(&self) -> Option<&str> {
        self.file_hash.as_deref()
    }

    pub fn val(&self) -> Option<ValidationState> {
        self.val
    }

    pub fn val_error(&self) -> Option<&str> {
        self.val_error.as_deref()
    }

    pub fn last_hash_scan(&self) -> Option<i64> {
        self.last_hash_scan
    }

    pub fn last_val_scan(&self) -> Option<i64> {
        self.last_val_scan
    }

    pub fn add_count(&self) -> Option<i64> {
        self.add_count
    }

    pub fn modify_count(&self) -> Option<i64> {
        self.modify_count
    }

    pub fn delete_count(&self) -> Option<i64> {
        self.delete_count
    }

    pub fn unchanged_count(&self) -> Option<i64> {
        self.unchanged_count
    }

    /// Get the current (latest) version of an item.
    pub fn get_current(
        conn: &Connection,
        item_id: i64,
    ) -> Result<Option<Self>, FsPulseError> {
        conn.query_row(
            "SELECT version_id, first_scan_id, last_scan_id, is_added, is_deleted, access,
                    mod_date, size, file_hash, val, val_error,
                    last_hash_scan, last_val_scan,
                    add_count, modify_count, delete_count, unchanged_count
             FROM item_versions
             WHERE item_id = ?
             ORDER BY first_scan_id DESC
             LIMIT 1",
            params![item_id],
            Self::from_row,
        )
        .optional()
        .map_err(FsPulseError::DatabaseError)
    }

    /// Insert the first version for a newly discovered item.
    ///
    /// `counts` should be `Some((0, 0, 0, 0))` for folders (add, modify, delete, unchanged),
    /// `None` for files.
    pub fn insert_initial(
        conn: &Connection,
        item_id: i64,
        scan_id: i64,
        access: Access,
        mod_date: Option<i64>,
        size: Option<i64>,
        counts: Option<(i64, i64, i64, i64)>,
    ) -> Result<(), FsPulseError> {
        let (add_count, modify_count, delete_count, unchanged_count) = match counts {
            Some((a, m, d, u)) => (Some(a), Some(m), Some(d), Some(u)),
            None => (None, None, None, None),
        };
        // Folders (counts.is_some()) get NULL val; files get Unknown
        let val_value = if counts.is_some() {
            None
        } else {
            Some(ValidationState::Unknown.as_i64())
        };
        conn.execute(
            "INSERT INTO item_versions (
                item_id, first_scan_id, last_scan_id,
                is_added, is_deleted, access, mod_date, size, val,
                add_count, modify_count, delete_count, unchanged_count
             ) VALUES (?, ?, ?, 1, 0, ?, ?, ?, ?, ?, ?, ?, ?)",
            params![item_id, scan_id, scan_id, access.as_i64(), mod_date, size,
                    val_value,
                    add_count, modify_count, delete_count, unchanged_count],
        )?;
        Ok(())
    }

    /// Insert a new version with all fields specified explicitly.
    ///
    /// Common INSERT used by `insert_with_carry_forward` (scan phase) and
    /// analysis-phase state changes.
    ///
    /// `counts` should be `Some((a, m, d, u))` for folders (0,0,0,0 for walk/sweep,
    /// actual values for scan analysis), `None` for files.
    pub fn insert_full(
        conn: &Connection,
        item_id: i64,
        scan_id: i64,
        is_added: bool,
        is_deleted: bool,
        access: Access,
        mod_date: Option<i64>,
        size: Option<i64>,
        file_hash: Option<&str>,
        val: Option<ValidationState>,
        val_error: Option<&str>,
        last_hash_scan: Option<i64>,
        last_val_scan: Option<i64>,
        counts: Option<(i64, i64, i64, i64)>,
    ) -> Result<(), FsPulseError> {
        let (add_count, modify_count, delete_count, unchanged_count) = match counts {
            Some((a, m, d, u)) => (Some(a), Some(m), Some(d), Some(u)),
            None => (None, None, None, None),
        };
        conn.execute(
            "INSERT INTO item_versions (
                item_id, first_scan_id, last_scan_id,
                is_added, is_deleted, access, mod_date, size,
                file_hash, val, val_error,
                last_hash_scan, last_val_scan,
                add_count, modify_count, delete_count, unchanged_count
             ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            params![
                item_id, scan_id, scan_id, is_added, is_deleted, access.as_i64(),
                mod_date, size, file_hash, val.map(|v| v.as_i64()), val_error,
                last_hash_scan, last_val_scan,
                add_count, modify_count, delete_count, unchanged_count,
            ],
        )?;
        Ok(())
    }

    /// Insert a new version when state changes, carrying forward fields from the previous version.
    ///
    /// Used by item modification. The caller provides the new observable state;
    /// unchanged fields are carried forward from `prev`.
    /// Counts are per-scan and never carried forward — folders get `(0,0,0,0)`, files get `None`.
    pub fn insert_with_carry_forward(
        conn: &Connection,
        item_id: i64,
        scan_id: i64,
        is_deleted: bool,
        access: Access,
        mod_date: Option<i64>,
        size: Option<i64>,
        prev: &ItemVersion,
        is_folder: bool,
    ) -> Result<(), FsPulseError> {
        let counts = if is_folder { Some((0, 0, 0, 0)) } else { None };
        Self::insert_full(
            conn, item_id, scan_id, false, is_deleted, access, mod_date, size,
            prev.file_hash(), prev.val(), prev.val_error(),
            prev.last_hash_scan(), prev.last_val_scan(),
            counts,
        )
    }

    /// Update `last_scan_id` in place for an unchanged item confirmed alive.
    pub fn touch_last_scan(
        conn: &Connection,
        version_id: i64,
        scan_id: i64,
    ) -> Result<(), FsPulseError> {
        conn.execute(
            "UPDATE item_versions SET last_scan_id = ? WHERE version_id = ?",
            params![scan_id, version_id],
        )?;
        Ok(())
    }

    /// Restore a pre-existing version's `last_scan_id` to its pre-scan value.
    ///
    /// During the walk phase, `touch_last_scan` advances `last_scan_id` to the current
    /// scan for every unchanged item. When the analysis phase later determines that
    /// the item's state actually changed (hash or validation), it must INSERT a new
    /// version. Before doing so, this method restores the old version's `last_scan_id`
    /// so that only the new version has `last_scan_id = current_scan`, preserving the
    /// invariant that at most one version per item is "current" in any given scan.
    ///
    /// The original value is read from the undo log (written by the walk phase).
    /// On rollback this restore is idempotent — the undo log replay sets the same value.
    pub fn restore_last_scan(
        conn: &Connection,
        version_id: i64,
    ) -> Result<(), FsPulseError> {
        let old_last_scan_id = UndoLog::get_old_last_scan_id(conn, version_id)?;
        Self::touch_last_scan(conn, version_id, old_last_scan_id)
    }

    /// Update a same-scan version in place with analysis results.
    ///
    /// Used when the analysis phase computes hash/val for an item whose version was
    /// created in the current scan (`first_scan_id = current_scan`). No undo log entry
    /// needed — the entire row is deleted on rollback.
    pub fn update_analysis_in_place(
        conn: &Connection,
        version_id: i64,
        access: Access,
        file_hash: Option<&str>,
        val: ValidationState,
        val_error: Option<&str>,
        last_hash_scan: Option<i64>,
        last_val_scan: Option<i64>,
    ) -> Result<(), FsPulseError> {
        conn.execute(
            "UPDATE item_versions SET
                access = ?, file_hash = ?, val = ?, val_error = ?,
                last_hash_scan = ?, last_val_scan = ?
             WHERE version_id = ?",
            params![
                access.as_i64(), file_hash, val.as_i64(), val_error,
                last_hash_scan, last_val_scan, version_id,
            ],
        )?;
        Ok(())
    }

    /// Update bookkeeping fields in place on a pre-existing version.
    ///
    /// Used when analysis computes hash/val that matches existing state — no new version
    /// needed, just advance `last_hash_scan` / `last_val_scan`. The caller is responsible
    /// for ensuring the undo log already has the pre-scan values (logged during scan phase
    /// by `handle_item_no_change`).
    pub fn update_bookkeeping(
        conn: &Connection,
        version_id: i64,
        last_hash_scan: Option<i64>,
        last_val_scan: Option<i64>,
    ) -> Result<(), FsPulseError> {
        conn.execute(
            "UPDATE item_versions SET
                last_hash_scan = ?, last_val_scan = ?
             WHERE version_id = ?",
            params![last_hash_scan, last_val_scan, version_id],
        )?;
        Ok(())
    }

    pub(crate) fn from_row(row: &rusqlite::Row) -> rusqlite::Result<Self> {
        Ok(ItemVersion {
            version_id: row.get(0)?,
            first_scan_id: row.get(1)?,
            last_scan_id: row.get(2)?,
            is_added: row.get(3)?,
            is_deleted: row.get(4)?,
            access: Access::from_i64(row.get(5)?),
            mod_date: row.get(6)?,
            size: row.get(7)?,
            file_hash: row.get(8)?,
            val: row.get::<_, Option<i64>>(9)?.map(ValidationState::from_i64),
            val_error: row.get(10)?,
            last_hash_scan: row.get(11)?,
            last_val_scan: row.get(12)?,
            add_count: row.get(13)?,
            modify_count: row.get(14)?,
            delete_count: row.get(15)?,
            unchanged_count: row.get(16)?,
        })
    }
}

/// An item ready for the analysis phase, with its current state and flags
/// indicating which analysis operations are needed.
#[derive(Clone, Debug)]
pub struct AnalysisItem {
    item_id: i64,
    item_path: String,
    access: i64,
    last_hash_scan: Option<i64>,
    file_hash: Option<String>,
    last_val_scan: Option<i64>,
    val: i64,
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

    pub fn access(&self) -> Access {
        Access::from_i64(self.access)
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

    pub fn needs_hash(&self) -> bool {
        self.needs_hash
    }

    pub fn set_needs_hash(&mut self, value: bool) {
        self.needs_hash = value;
    }

    pub fn needs_val(&self) -> bool {
        self.needs_val
    }

    pub fn set_needs_val(&mut self, value: bool) {
        self.needs_val = value;
    }

    fn from_row(row: &rusqlite::Row) -> rusqlite::Result<Self> {
        Ok(AnalysisItem {
            item_id: row.get(0)?,
            item_path: row.get(1)?,
            access: row.get(2)?,
            last_hash_scan: row.get(3)?,
            file_hash: row.get(4)?,
            last_val_scan: row.get(5)?,
            val: row.get(6)?,
            val_error: row.get(7)?,
            needs_hash: row.get(8)?,
            needs_val: row.get(9)?,
        })
    }

    pub fn get_analysis_counts(
        conn: &Connection,
        scan_id: i64,
        analysis_spec: &AnalysisSpec,
        last_item_id: i64,
    ) -> Result<(u64, u64), crate::error::FsPulseError> {
        let sql = r#"
            WITH candidates AS (
                SELECT
                    cv.last_hash_scan,
                    cv.last_val_scan,
                    CASE
                        WHEN $1 = 0 THEN 0
                        WHEN $2 = 1 AND (cv.file_hash IS NULL OR cv.last_hash_scan IS NULL OR cv.last_hash_scan < $3) THEN 1
                        WHEN cv.file_hash IS NULL THEN 1
                        WHEN cv.first_scan_id = $3 AND pv.version_id IS NULL THEN 1
                        WHEN cv.first_scan_id = $3 AND pv.is_deleted = 1 THEN 1
                        WHEN cv.first_scan_id = $3 AND (cv.mod_date IS NOT pv.mod_date OR cv.size IS NOT pv.size) THEN 1
                        ELSE 0
                    END AS needs_hash,
                    CASE
                        WHEN $4 = 0 THEN 0
                        WHEN $5 = 1 AND (cv.val = 0 OR cv.last_val_scan IS NULL OR cv.last_val_scan < $3) THEN 1
                        WHEN cv.val = 0 THEN 1
                        WHEN cv.first_scan_id = $3 AND pv.version_id IS NULL THEN 1
                        WHEN cv.first_scan_id = $3 AND pv.is_deleted = 1 THEN 1
                        WHEN cv.first_scan_id = $3 AND (cv.mod_date IS NOT pv.mod_date OR cv.size IS NOT pv.size) THEN 1
                        ELSE 0
                    END AS needs_val
                FROM items i
                JOIN item_versions cv
                    ON cv.item_id = i.item_id
                    AND cv.last_scan_id = $3
                LEFT JOIN item_versions pv
                    ON pv.item_id = i.item_id
                    AND pv.first_scan_id = (
                        SELECT MAX(first_scan_id)
                        FROM item_versions
                        WHERE item_id = i.item_id
                          AND first_scan_id < cv.first_scan_id
                    )
                WHERE
                    i.item_type = 0
                    AND cv.is_deleted = 0
                    AND cv.access <> 1
                    AND i.item_id > $6
            )
            SELECT
                COALESCE(SUM(CASE WHEN needs_hash = 1 OR needs_val = 1 THEN 1 ELSE 0 END), 0) AS total_needed,
                COALESCE(SUM(CASE
                    WHEN (needs_hash = 1 AND last_hash_scan = $3)
                    OR (needs_val = 1 AND last_val_scan = $3)
                    THEN 1 ELSE 0 END), 0) AS total_done
            FROM candidates"#;

        let mut stmt = conn.prepare_cached(sql)?;
        let mut rows = stmt.query(params![
            analysis_spec.is_hash() as i64,
            analysis_spec.hash_all() as i64,
            scan_id,
            analysis_spec.is_val() as i64,
            analysis_spec.val_all() as i64,
            last_item_id
        ])?;

        if let Some(row) = rows.next()? {
            let total_needed = row.get::<_, i64>(0)? as u64;
            let total_done = row.get::<_, i64>(1)? as u64;
            Ok((total_needed, total_done))
        } else {
            Ok((0, 0))
        }
    }

    pub fn fetch_next_batch(
        conn: &Connection,
        scan_id: i64,
        analysis_spec: &AnalysisSpec,
        last_item_id: i64,
        limit: usize,
    ) -> Result<Vec<AnalysisItem>, crate::error::FsPulseError> {
        let query = format!(
            "SELECT
                i.item_id,
                i.item_path,
                cv.access,
                cv.last_hash_scan,
                cv.file_hash,
                cv.last_val_scan,
                cv.val,
                cv.val_error,
                CASE
                    WHEN $1 = 0 THEN 0
                    WHEN $2 = 1 AND (cv.file_hash IS NULL OR cv.last_hash_scan IS NULL OR cv.last_hash_scan < $3) THEN 1
                    WHEN cv.file_hash IS NULL THEN 1
                    WHEN cv.first_scan_id = $3 AND pv.version_id IS NULL THEN 1
                    WHEN cv.first_scan_id = $3 AND pv.is_deleted = 1 THEN 1
                    WHEN cv.first_scan_id = $3 AND (cv.mod_date IS NOT pv.mod_date OR cv.size IS NOT pv.size) THEN 1
                    ELSE 0
                END AS needs_hash,
                CASE
                    WHEN $4 = 0 THEN 0
                    WHEN $5 = 1 AND (cv.val = 0 OR cv.last_val_scan IS NULL OR cv.last_val_scan < $3) THEN 1
                    WHEN cv.val = 0 THEN 1
                    WHEN cv.first_scan_id = $3 AND pv.version_id IS NULL THEN 1
                    WHEN cv.first_scan_id = $3 AND pv.is_deleted = 1 THEN 1
                    WHEN cv.first_scan_id = $3 AND (cv.mod_date IS NOT pv.mod_date OR cv.size IS NOT pv.size) THEN 1
                    ELSE 0
                END AS needs_val
            FROM items i
            JOIN item_versions cv
                ON cv.item_id = i.item_id
                AND cv.last_scan_id = $3
            LEFT JOIN item_versions pv
                ON pv.item_id = i.item_id
                AND pv.first_scan_id = (
                    SELECT MAX(first_scan_id)
                    FROM item_versions
                    WHERE item_id = i.item_id
                      AND first_scan_id < cv.first_scan_id
                )
            WHERE
                i.item_type = 0
                AND cv.is_deleted = 0
                AND cv.access <> 1
                AND i.item_id > $6
                AND (
                    ($1 = 1 AND (
                        ($2 = 1 AND (cv.file_hash IS NULL OR cv.last_hash_scan IS NULL OR cv.last_hash_scan < $3)) OR
                        cv.file_hash IS NULL OR
                        (cv.first_scan_id = $3 AND pv.version_id IS NULL AND (cv.last_hash_scan IS NULL OR cv.last_hash_scan < $3)) OR
                        (cv.first_scan_id = $3 AND pv.is_deleted = 1 AND (cv.last_hash_scan IS NULL OR cv.last_hash_scan < $3)) OR
                        (cv.first_scan_id = $3 AND (cv.mod_date IS NOT pv.mod_date OR cv.size IS NOT pv.size) AND (cv.last_hash_scan IS NULL OR cv.last_hash_scan < $3))
                    )) OR
                    ($4 = 1 AND (
                        ($5 = 1 AND (cv.val = 0 OR cv.last_val_scan IS NULL OR cv.last_val_scan < $3)) OR
                        cv.val = 0 OR
                        (cv.first_scan_id = $3 AND pv.version_id IS NULL AND (cv.last_val_scan IS NULL OR cv.last_val_scan < $3)) OR
                        (cv.first_scan_id = $3 AND pv.is_deleted = 1 AND (cv.last_val_scan IS NULL OR cv.last_val_scan < $3)) OR
                        (cv.first_scan_id = $3 AND (cv.mod_date IS NOT pv.mod_date OR cv.size IS NOT pv.size) AND (cv.last_val_scan IS NULL OR cv.last_val_scan < $3))
                    ))
                )
            ORDER BY i.item_id ASC
            LIMIT {limit}"
        );

        let mut stmt = conn.prepare(&query)?;

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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_analysis_item_getters() {
        let analysis_item = AnalysisItem {
            item_id: 123,
            item_path: "/test/path".to_string(),
            access: Access::Ok.as_i64(),
            last_hash_scan: Some(456),
            file_hash: Some("abc123".to_string()),
            last_val_scan: Some(789),
            val: ValidationState::Valid.as_i64(),
            val_error: Some("test error".to_string()),
            needs_hash: true,
            needs_val: false,
        };

        assert_eq!(analysis_item.item_id(), 123);
        assert_eq!(analysis_item.item_path(), "/test/path");
        assert_eq!(analysis_item.access(), Access::Ok);
        assert_eq!(analysis_item.last_hash_scan(), Some(456));
        assert_eq!(analysis_item.file_hash(), Some("abc123"));
        assert_eq!(analysis_item.last_val_scan(), Some(789));
        assert_eq!(analysis_item.val_error(), Some("test error"));
        assert!(analysis_item.needs_hash());
        assert!(!analysis_item.needs_val());
    }
}
