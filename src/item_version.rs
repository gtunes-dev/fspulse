use log::error;
use rusqlite::{params, Connection, OptionalExtension};

use crate::{error::FsPulseError, items::Access, validate::validator::ValidationState};

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
    is_deleted: bool,
    access: Access,
    mod_date: Option<i64>,
    size: Option<i64>,
    file_hash: Option<String>,
    val: ValidationState,
    val_error: Option<String>,
    last_hash_scan: Option<i64>,
    last_val_scan: Option<i64>,
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

    pub fn val(&self) -> ValidationState {
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

    /// Get the current (latest) version of an item.
    pub fn get_current(
        conn: &Connection,
        item_id: i64,
    ) -> Result<Option<Self>, FsPulseError> {
        conn.query_row(
            "SELECT version_id, first_scan_id, last_scan_id, is_deleted, access,
                    mod_date, size, file_hash, val, val_error,
                    last_hash_scan, last_val_scan
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
    pub fn insert_initial(
        conn: &Connection,
        item_id: i64,
        scan_id: i64,
        access: Access,
        mod_date: Option<i64>,
        size: Option<i64>,
    ) -> Result<(), FsPulseError> {
        conn.execute(
            "INSERT INTO item_versions (
                item_id, first_scan_id, last_scan_id,
                is_deleted, access, mod_date, size
             ) VALUES (?, ?, ?, 0, ?, ?, ?)",
            params![item_id, scan_id, scan_id, access.as_i64(), mod_date, size],
        )?;
        Ok(())
    }

    /// Insert a new version with all fields specified explicitly.
    ///
    /// Common INSERT used by `insert_with_carry_forward` (scan phase) and
    /// analysis-phase state changes.
    pub fn insert_full(
        conn: &Connection,
        item_id: i64,
        scan_id: i64,
        is_deleted: bool,
        access: Access,
        mod_date: Option<i64>,
        size: Option<i64>,
        file_hash: Option<&str>,
        val: ValidationState,
        val_error: Option<&str>,
        last_hash_scan: Option<i64>,
        last_val_scan: Option<i64>,
    ) -> Result<(), FsPulseError> {
        conn.execute(
            "INSERT INTO item_versions (
                item_id, first_scan_id, last_scan_id,
                is_deleted, access, mod_date, size,
                file_hash, val, val_error,
                last_hash_scan, last_val_scan
             ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            params![
                item_id, scan_id, scan_id, is_deleted, access.as_i64(),
                mod_date, size, file_hash, val.as_i64(), val_error,
                last_hash_scan, last_val_scan,
            ],
        )?;
        Ok(())
    }

    /// Insert a new version when state changes, carrying forward fields from the previous version.
    ///
    /// Used by tombstone rehydration and item modification. The caller provides the new
    /// observable state; unchanged fields are carried forward from `prev`.
    pub fn insert_with_carry_forward(
        conn: &Connection,
        item_id: i64,
        scan_id: i64,
        is_deleted: bool,
        access: Access,
        mod_date: Option<i64>,
        size: Option<i64>,
        prev: &ItemVersion,
    ) -> Result<(), FsPulseError> {
        Self::insert_full(
            conn, item_id, scan_id, is_deleted, access, mod_date, size,
            prev.file_hash(), prev.val(), prev.val_error(),
            prev.last_hash_scan(), prev.last_val_scan(),
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

    /// Validate that the old model (items_old) and new model (items + item_versions)
    /// are in sync. Logs mismatches and returns an error if any are found.
    ///
    /// Used by both the v15→v16 migration (to verify initial data) and by the scanner
    /// (to verify dual-write correctness after each scan). Temporary — removed at cutover.
    pub fn validate_against_old_model(
        conn: &Connection,
        context: &str,
    ) -> Result<(), FsPulseError> {
        let mut errors: Vec<String> = Vec::new();

        // Validation 1: Row counts — every item in items_old should have an identity in items
        let items_old_count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM items_old", [], |row| row.get(0)
        ).map_err(FsPulseError::DatabaseError)?;

        let items_count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM items", [], |row| row.get(0)
        ).map_err(FsPulseError::DatabaseError)?;

        if items_old_count != items_count {
            errors.push(format!(
                "Identity count mismatch: items_old has {} rows, items has {} rows",
                items_old_count, items_count
            ));
        }

        // Validation 2: Every item should have at least one version
        let items_without_versions: i64 = conn.query_row(
            "SELECT COUNT(*) FROM items i
             WHERE NOT EXISTS (SELECT 1 FROM item_versions v WHERE v.item_id = i.item_id)",
            [], |row| row.get(0)
        ).map_err(FsPulseError::DatabaseError)?;

        if items_without_versions > 0 {
            errors.push(format!(
                "{} items have no version rows in item_versions",
                items_without_versions
            ));
        }

        // Validation 3: Compare current state in items_old against latest version
        let mut stmt = conn.prepare(
            "SELECT
                io.item_id, io.item_path,
                io.is_ts, io.access, io.mod_date, io.size, io.file_hash,
                io.val, io.val_error, io.last_hash_scan, io.last_val_scan, io.last_scan,
                v.is_deleted, v.access, v.mod_date, v.size, v.file_hash,
                v.val, v.val_error, v.last_hash_scan, v.last_val_scan, v.last_scan_id
             FROM items_old io
             JOIN (
                 SELECT item_id, is_deleted, access, mod_date, size, file_hash,
                        val, val_error, last_hash_scan, last_val_scan, last_scan_id
                 FROM item_versions v1
                 WHERE v1.first_scan_id = (
                     SELECT MAX(v2.first_scan_id) FROM item_versions v2
                     WHERE v2.item_id = v1.item_id
                 )
             ) v ON v.item_id = io.item_id"
        ).map_err(FsPulseError::DatabaseError)?;

        let rows = stmt.query_map([], |row| {
            Ok(OldNewComparisonRow {
                item_id: row.get(0)?,
                item_path: row.get(1)?,
                old_is_ts: row.get(2)?,
                old_access: row.get(3)?,
                old_mod_date: row.get(4)?,
                old_size: row.get(5)?,
                old_file_hash: row.get(6)?,
                old_val: row.get(7)?,
                old_val_error: row.get(8)?,
                old_last_hash_scan: row.get(9)?,
                old_last_val_scan: row.get(10)?,
                old_last_scan: row.get(11)?,
                ver_is_deleted: row.get(12)?,
                ver_access: row.get(13)?,
                ver_mod_date: row.get(14)?,
                ver_size: row.get(15)?,
                ver_file_hash: row.get(16)?,
                ver_val: row.get(17)?,
                ver_val_error: row.get(18)?,
                ver_last_hash_scan: row.get(19)?,
                ver_last_val_scan: row.get(20)?,
                ver_last_scan_id: row.get(21)?,
            })
        }).map_err(FsPulseError::DatabaseError)?;

        let mut mismatch_count = 0;
        let max_logged = 50;

        for row_result in rows {
            let r = row_result.map_err(FsPulseError::DatabaseError)?;
            let mut item_errors: Vec<String> = Vec::new();

            if r.old_is_ts != r.ver_is_deleted {
                item_errors.push(format!("is_ts/is_deleted: {} vs {}", r.old_is_ts, r.ver_is_deleted));
            }
            if r.old_access != r.ver_access {
                item_errors.push(format!("access: {} vs {}", r.old_access, r.ver_access));
            }
            if r.old_mod_date != r.ver_mod_date {
                item_errors.push(format!("mod_date: {:?} vs {:?}", r.old_mod_date, r.ver_mod_date));
            }
            if r.old_size != r.ver_size {
                item_errors.push(format!("size: {:?} vs {:?}", r.old_size, r.ver_size));
            }
            if r.old_file_hash != r.ver_file_hash {
                item_errors.push(format!("file_hash: {:?} vs {:?}", r.old_file_hash, r.ver_file_hash));
            }
            if r.old_val != r.ver_val {
                item_errors.push(format!("val: {} vs {}", r.old_val, r.ver_val));
            }
            if r.old_val_error != r.ver_val_error {
                item_errors.push(format!("val_error: {:?} vs {:?}", r.old_val_error, r.ver_val_error));
            }
            if r.old_last_hash_scan != r.ver_last_hash_scan {
                item_errors.push(format!("last_hash_scan: {:?} vs {:?}", r.old_last_hash_scan, r.ver_last_hash_scan));
            }
            if r.old_last_val_scan != r.ver_last_val_scan {
                item_errors.push(format!("last_val_scan: {:?} vs {:?}", r.old_last_val_scan, r.ver_last_val_scan));
            }
            if r.old_last_scan != r.ver_last_scan_id {
                item_errors.push(format!("last_scan/last_scan_id: {} vs {}", r.old_last_scan, r.ver_last_scan_id));
            }

            if !item_errors.is_empty() {
                mismatch_count += 1;
                if mismatch_count <= max_logged {
                    error!(
                        "{} validation: item_id={} path='{}' mismatches: [{}]",
                        context, r.item_id, r.item_path, item_errors.join(", ")
                    );
                }
            }
        }

        if mismatch_count > max_logged {
            error!(
                "{} validation: ... and {} more items with mismatches (only first {} shown)",
                context, mismatch_count - max_logged, max_logged
            );
        }

        if mismatch_count > 0 {
            errors.push(format!(
                "{} items have state mismatches between items_old and latest item_version",
                mismatch_count
            ));
        }

        // Validation 4: Version chain ordering
        let bad_ranges: i64 = conn.query_row(
            "SELECT COUNT(*) FROM item_versions WHERE first_scan_id > last_scan_id",
            [], |row| row.get(0)
        ).map_err(FsPulseError::DatabaseError)?;

        if bad_ranges > 0 {
            errors.push(format!(
                "{} version rows have first_scan_id > last_scan_id",
                bad_ranges
            ));
        }

        if !errors.is_empty() {
            for err in &errors {
                error!("{} validation error: {}", context, err);
            }
            return Err(FsPulseError::Error(format!(
                "{} validation failed with {} error(s). See log for details.",
                context, errors.len()
            )));
        }

        Ok(())
    }

    fn from_row(row: &rusqlite::Row) -> rusqlite::Result<Self> {
        Ok(ItemVersion {
            version_id: row.get(0)?,
            first_scan_id: row.get(1)?,
            last_scan_id: row.get(2)?,
            is_deleted: row.get(3)?,
            access: Access::from_i64(row.get(4)?),
            mod_date: row.get(5)?,
            size: row.get(6)?,
            file_hash: row.get(7)?,
            val: ValidationState::from_i64(row.get(8)?),
            val_error: row.get(9)?,
            last_hash_scan: row.get(10)?,
            last_val_scan: row.get(11)?,
        })
    }
}

/// Row type for comparing old model (items_old) against new model (latest item_version).
/// Temporary — removed at cutover along with validate_against_old_model.
struct OldNewComparisonRow {
    item_id: i64,
    item_path: String,
    old_is_ts: bool,
    old_access: i64,
    old_mod_date: Option<i64>,
    old_size: Option<i64>,
    old_file_hash: Option<String>,
    old_val: i64,
    old_val_error: Option<String>,
    old_last_hash_scan: Option<i64>,
    old_last_val_scan: Option<i64>,
    old_last_scan: i64,
    ver_is_deleted: bool,
    ver_access: i64,
    ver_mod_date: Option<i64>,
    ver_size: Option<i64>,
    ver_file_hash: Option<String>,
    ver_val: i64,
    ver_val_error: Option<String>,
    ver_last_hash_scan: Option<i64>,
    ver_last_val_scan: Option<i64>,
    ver_last_scan_id: i64,
}
