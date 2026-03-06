use log::info;
use rusqlite::{params, Connection};

use crate::db::{migration_info, Database};
use crate::error::FsPulseError;

// ============================================================================
// Schema Upgrade: Version 24 → 25 (Standalone)
//
// Deduplicates consecutive identical folder versions.
//
// Background: A bug in the folder count rollup logic created a new version
// for every folder on every scan even when no descendant state actually
// changed. This migration merges those redundant consecutive versions by
// extending the kept version's last_scan_id and deleting the duplicates.
//
// After deduplication, recomputes scan-level add/modify/delete counts
// in the scans table, which were inflated by the redundant folder
// versions being counted as modifications.
//
// No DDL changes — this is a data-only cleanup.
//
// Algorithm:
//   1. For each folder item, walk versions ordered by first_scan_id.
//      If all observable fields match the "keeper" version, extend the
//      keeper's last_scan_id and delete the redundant version.
//   2. Recompute add_count/modify_count/delete_count for all completed
//      scans from the corrected item_versions table.
//
// Crash recovery: Uses a high-water mark (last processed item_id) so
// the dedup phase can resume after an interruption. The recompute
// phase is idempotent.
// ============================================================================

const HWM_META_KEY: &str = "v25_dedup_hwm";
const BATCH_SIZE: usize = 500;

/// Minimal version row for comparison.
#[allow(dead_code)]
struct FolderVersion {
    version_id: i64,
    first_scan_id: i64,
    last_scan_id: i64,
    is_added: bool,
    is_deleted: bool,
    access: i64,
    mod_date: Option<i64>,
    size: Option<i64>,
    last_val_scan: Option<i64>,
    val_state: Option<i64>,
    val_error: Option<String>,
    last_hash_scan: Option<i64>,
    file_hash: Option<String>,
    hash_state: Option<i64>,
    add_count: Option<i64>,
    modify_count: Option<i64>,
    delete_count: Option<i64>,
    unchanged_count: Option<i64>,
    val_unknown_count: Option<i64>,
    val_valid_count: Option<i64>,
    val_invalid_count: Option<i64>,
    val_no_validator_count: Option<i64>,
    hash_unknown_count: Option<i64>,
    hash_valid_count: Option<i64>,
    hash_suspect_count: Option<i64>,
}

impl FolderVersion {
    /// Check whether the item's own metadata matches another version.
    /// Compares everything except version_id, scan ids, and descendant counts.
    fn metadata_matches(&self, other: &FolderVersion) -> bool {
        self.is_added == other.is_added
            && self.is_deleted == other.is_deleted
            && self.access == other.access
            && self.mod_date == other.mod_date
            && self.size == other.size
            && self.last_val_scan == other.last_val_scan
            && self.val_state == other.val_state
            && self.val_error == other.val_error
            && self.last_hash_scan == other.last_hash_scan
            && self.file_hash == other.file_hash
            && self.hash_state == other.hash_state
    }

    /// Determine whether `other` (the next consecutive version) should be
    /// merged into this keeper version.
    ///
    /// A version is redundant if:
    ///   1. ALL observable fields match (exact duplicate — the common bug case), OR
    ///   2. The version has add_count=0, modify_count=0, delete_count=0 and its
    ///      item metadata matches the keeper.  Such a version was created by the
    ///      bug's `!state_counts.is_zero()` guard even though no descendant was
    ///      actually added, modified, or deleted.
    fn should_merge_next(&self, other: &FolderVersion) -> bool {
        // Case 1: every observable field is identical (counts included)
        if self.metadata_matches(other)
            && self.add_count == other.add_count
            && self.modify_count == other.modify_count
            && self.delete_count == other.delete_count
            && self.unchanged_count == other.unchanged_count
            && self.val_unknown_count == other.val_unknown_count
            && self.val_valid_count == other.val_valid_count
            && self.val_invalid_count == other.val_invalid_count
            && self.val_no_validator_count == other.val_no_validator_count
            && self.hash_unknown_count == other.hash_unknown_count
            && self.hash_valid_count == other.hash_valid_count
            && self.hash_suspect_count == other.hash_suspect_count
        {
            return true;
        }

        // Case 2: no descendant changes and item metadata unchanged — the
        // version should never have been created.
        !other.is_added
            && !other.is_deleted
            && other.add_count == Some(0)
            && other.modify_count == Some(0)
            && other.delete_count == Some(0)
            && self.access == other.access
            && self.mod_date == other.mod_date
            && self.size == other.size
            && self.last_val_scan == other.last_val_scan
            && self.val_state == other.val_state
            && self.val_error == other.val_error
            && self.last_hash_scan == other.last_hash_scan
            && self.file_hash == other.file_hash
            && self.hash_state == other.hash_state
    }
}

/// Query all versions for a folder item, ordered by first_scan_id.
fn query_folder_versions(
    conn: &Connection,
    item_id: i64,
) -> Result<Vec<FolderVersion>, FsPulseError> {
    let mut stmt = conn.prepare_cached(
        "SELECT version_id, first_scan_id, last_scan_id,
                is_added, is_deleted, access, mod_date, size,
                last_val_scan, val_state, val_error,
                last_hash_scan, file_hash, hash_state,
                add_count, modify_count, delete_count, unchanged_count,
                val_unknown_count, val_valid_count, val_invalid_count, val_no_validator_count,
                hash_unknown_count, hash_valid_count, hash_suspect_count
         FROM item_versions
         WHERE item_id = ?
         ORDER BY first_scan_id ASC",
    )?;

    let rows = stmt.query_map(params![item_id], |row| {
        Ok(FolderVersion {
            version_id: row.get(0)?,
            first_scan_id: row.get(1)?,
            last_scan_id: row.get(2)?,
            is_added: row.get(3)?,
            is_deleted: row.get(4)?,
            access: row.get(5)?,
            mod_date: row.get(6)?,
            size: row.get(7)?,
            last_val_scan: row.get(8)?,
            val_state: row.get(9)?,
            val_error: row.get(10)?,
            last_hash_scan: row.get(11)?,
            file_hash: row.get(12)?,
            hash_state: row.get(13)?,
            add_count: row.get(14)?,
            modify_count: row.get(15)?,
            delete_count: row.get(16)?,
            unchanged_count: row.get(17)?,
            val_unknown_count: row.get(18)?,
            val_valid_count: row.get(19)?,
            val_invalid_count: row.get(20)?,
            val_no_validator_count: row.get(21)?,
            hash_unknown_count: row.get(22)?,
            hash_valid_count: row.get(23)?,
            hash_suspect_count: row.get(24)?,
        })
    })?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(FsPulseError::DatabaseError)
}

/// Deduplicate versions for a single folder item.
/// Returns (versions_deleted, versions_kept) for logging.
fn dedup_folder_versions(
    conn: &Connection,
    item_id: i64,
) -> Result<usize, FsPulseError> {
    let versions = query_folder_versions(conn, item_id)?;
    if versions.len() < 2 {
        return Ok(0);
    }

    let mut deleted = 0usize;
    let mut keeper_idx = 0;

    for i in 1..versions.len() {
        if versions[keeper_idx].should_merge_next(&versions[i]) {
            // Redundant — extend keeper's last_scan_id and delete this version
            conn.execute(
                "UPDATE item_versions SET last_scan_id = ? WHERE version_id = ?",
                params![versions[i].last_scan_id, versions[keeper_idx].version_id],
            )?;
            conn.execute(
                "DELETE FROM item_versions WHERE version_id = ?",
                params![versions[i].version_id],
            )?;
            deleted += 1;
        } else {
            keeper_idx = i;
        }
    }

    Ok(deleted)
}

/// Recompute add_count, modify_count, delete_count for all completed scans.
///
/// These counts were inflated by the bug because redundant folder versions
/// were counted as modifications. After dedup removes those versions,
/// recomputing from item_versions gives the correct counts.
fn recompute_scan_change_counts(conn: &Connection) -> Result<(), FsPulseError> {
    migration_info("    Recomputing scan-level change counts...");

    // Build correct counts from the (now-cleaned) item_versions table.
    // This uses the same logic as Scan::set_state_completed.
    let updated = conn.execute(
        "UPDATE scans SET
            add_count = (
                SELECT COALESCE(COUNT(*) FILTER (WHERE iv.is_deleted = 0
                    AND (pv.version_id IS NULL OR pv.is_deleted = 1)), 0)
                FROM item_versions iv
                LEFT JOIN item_versions pv ON pv.item_id = iv.item_id
                    AND pv.first_scan_id = (
                        SELECT MAX(first_scan_id) FROM item_versions
                        WHERE item_id = iv.item_id AND first_scan_id < iv.first_scan_id
                    )
                WHERE iv.first_scan_id = scans.scan_id
            ),
            modify_count = (
                SELECT COALESCE(COUNT(*) FILTER (WHERE iv.is_deleted = 0
                    AND pv.version_id IS NOT NULL AND pv.is_deleted = 0), 0)
                FROM item_versions iv
                LEFT JOIN item_versions pv ON pv.item_id = iv.item_id
                    AND pv.first_scan_id = (
                        SELECT MAX(first_scan_id) FROM item_versions
                        WHERE item_id = iv.item_id AND first_scan_id < iv.first_scan_id
                    )
                WHERE iv.first_scan_id = scans.scan_id
            ),
            delete_count = (
                SELECT COALESCE(COUNT(*) FILTER (WHERE iv.is_deleted = 1
                    AND pv.version_id IS NOT NULL AND pv.is_deleted = 0), 0)
                FROM item_versions iv
                LEFT JOIN item_versions pv ON pv.item_id = iv.item_id
                    AND pv.first_scan_id = (
                        SELECT MAX(first_scan_id) FROM item_versions
                        WHERE item_id = iv.item_id AND first_scan_id < iv.first_scan_id
                    )
                WHERE iv.first_scan_id = scans.scan_id
            )
        WHERE state = 4",
        [],
    )?;

    migration_info(&format!(
        "    Recomputed change counts for {} completed scans.",
        updated
    ));

    Ok(())
}

pub fn run_migration_v24_to_v25(conn: &Connection) -> Result<(), FsPulseError> {
    // Resume from high-water mark if interrupted
    let hwm: i64 = match Database::get_meta_value_locked(conn, HWM_META_KEY)? {
        Some(val) => val.parse().unwrap_or(0),
        None => {
            Database::immediate_transaction(conn, |c| {
                Database::set_meta_value_locked(c, HWM_META_KEY, "0")
            })?;
            0
        }
    };

    // Get all folder item_ids above the high-water mark
    let folder_ids: Vec<i64> = {
        let mut stmt = conn.prepare(
            "SELECT item_id FROM items
             WHERE item_type = 1 AND item_id > ?
             ORDER BY item_id ASC",
        )?;

        let rows = stmt.query_map(params![hwm], |row| row.get(0))?;
        rows.collect::<Result<Vec<_>, _>>()?
    };

    let total = folder_ids.len();
    if total == 0 {
        info!("Migration 24→25: No folder items to deduplicate.");
    } else {
        migration_info(&format!(
            "    Deduplicating versions for {} folder items...",
            total
        ));

        let mut total_deleted = 0usize;

        for batch in folder_ids.chunks(BATCH_SIZE) {
            Database::immediate_transaction(conn, |c| {
                for &item_id in batch {
                    total_deleted += dedup_folder_versions(c, item_id)?;
                }
                // Persist HWM as the last item_id in this batch
                let last_id = batch[batch.len() - 1];
                Database::set_meta_value_locked(c, HWM_META_KEY, &last_id.to_string())?;
                Ok(())
            })?;

            let processed = folder_ids
                .iter()
                .position(|&id| id == batch[batch.len() - 1])
                .unwrap_or(0)
                + 1;
            if processed % 5000 == 0 || processed == total {
                migration_info(&format!(
                    "    Progress: {}/{} folders processed, {} redundant versions removed",
                    processed, total, total_deleted
                ));
            }
        }

        migration_info(&format!(
            "    Deduplication complete: {} redundant versions removed.",
            total_deleted
        ));
    }

    // Recompute scan-level add/modify/delete counts.
    // The bug inflated modify_count because redundant folder versions
    // (with first_scan_id = scan_id) were counted as modifications.
    // Now that those versions are deleted, recompute from the corrected data.
    recompute_scan_change_counts(conn)?;

    // Clean up and bump version
    Database::immediate_transaction(conn, |c| {
        Database::delete_meta_locked(c, HWM_META_KEY)?;
        Database::set_meta_value_locked(c, "schema_version", "25")
    })?;

    Ok(())
}
