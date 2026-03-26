// ============================================================================
// Schema Upgrade: Version 29 → 30 — Fix folder deletion inconsistencies
//
// The v27→v28 migration's cleanup_overlapping_versions resolved temporally
// overlapping folder versions by pure chronological chaining (sorting by
// first_scan_id and trimming last_scan_id). This was content-blind: it did
// not consider is_deleted or child item state.
//
// As a result, some folders ended up with is_deleted=1 at scans where their
// descendant files were still alive. When phase3_recompute_folder_counts then
// walked the directory tree, it skipped these "deleted" folders, producing
// undercounted file_count and folder_count on those scans.
//
// This migration:
//   1. Finds all folder versions marked is_deleted=1 that have alive
//      descendant items at any scan in their range.
//   2. Flips those folder versions to is_deleted=0.
//   3. Recomputes population and integrity counts for all affected scans
//      using flat queries (no tree walk needed).
//
// No schema DDL changes — this is a pure data-repair migration.
//
// This is a Transacted migration. All work runs inside a single IMMEDIATE
// transaction and the schema version is bumped atomically.
// ============================================================================

use std::collections::HashSet;

use rusqlite::{params, Connection};

use crate::db::migration_info;
use crate::error::FsPulseError;

pub const UPGRADE_29_TO_30_PRE_SQL: &str =
    "INSERT OR REPLACE INTO meta (key, value) VALUES ('schema_version', '30');";

/// Repair folder deletion flags and recompute scan counts for affected scans.
pub fn migrate_v29_to_v30(conn: &Connection) -> Result<(), FsPulseError> {
    // Phase 1: Find and fix inconsistent folder deletion flags.
    //
    // A folder version is inconsistent if is_deleted=1 but any descendant file
    // has an alive version overlapping the same scan range. We check each scan
    // in the folder version's [first_scan_id, last_scan_id] range — if ANY
    // alive descendant exists at any of those scans, the folder should be alive.
    //
    // Strategy: for each deleted folder version, check whether any item under
    // that folder's path has an alive version overlapping the folder version's
    // scan range. This avoids per-scan checking — a single overlap query covers
    // the entire range.
    migration_info("  Fixing inconsistent folder deletion flags...");

    // Find all deleted folder versions
    let mut stmt = conn.prepare(
        "SELECT iv.item_id, iv.item_version, iv.first_scan_id, iv.last_scan_id, i.item_path, i.root_id
         FROM item_versions iv
         JOIN items i ON i.item_id = iv.item_id
         WHERE i.item_type = 1 AND iv.is_deleted = 1
         ORDER BY i.root_id, iv.first_scan_id",
    )?;

    let deleted_folders: Vec<(i64, i64, i64, i64, String, i64)> = stmt
        .query_map([], |row| {
            Ok((
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
                row.get(4)?,
                row.get(5)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;

    drop(stmt);

    if deleted_folders.is_empty() {
        migration_info("    No deleted folder versions found.");
        return Ok(());
    }

    migration_info(&format!(
        "    Checking {} deleted folder versions for alive descendants...",
        deleted_folders.len()
    ));

    // For each deleted folder version, check if any descendant file is alive
    // during the folder's scan range. We use a path-prefix match to find
    // descendants, and check for overlapping alive versions.
    let mut check_stmt = conn.prepare(
        "SELECT EXISTS(
            SELECT 1
            FROM items ci
            JOIN item_versions civ ON civ.item_id = ci.item_id
            WHERE ci.root_id = ?1
              AND ci.item_type = 0
              AND ci.item_path > ?2
              AND ci.item_path < ?3
              AND civ.is_deleted = 0
              AND civ.first_scan_id <= ?5
              AND civ.last_scan_id >= ?4
            LIMIT 1
        )",
    )?;

    let mut fix_stmt = conn.prepare(
        "UPDATE item_versions SET is_deleted = 0 WHERE item_id = ? AND item_version = ?",
    )?;

    let mut fixed_count = 0u64;
    let mut affected_scans: HashSet<i64> = HashSet::new();

    for (item_id, item_version, first_scan, last_scan, folder_path, root_id) in &deleted_folders {
        // Build path prefix range for direct and indirect descendants
        let path_prefix = if folder_path.ends_with('/') {
            folder_path.clone()
        } else {
            format!("{}/", folder_path)
        };
        // Upper bound: same prefix but with next char after '/'
        let path_upper = format!(
            "{}{}",
            &folder_path,
            char::from(b'/' + 1) // '0' — first char after '/'
        );

        let has_alive_descendants: bool = check_stmt.query_row(
            params![root_id, path_prefix, path_upper, first_scan, last_scan],
            |row| row.get(0),
        )?;

        if has_alive_descendants {
            fix_stmt.execute(params![item_id, item_version])?;
            fixed_count += 1;

            // Collect all completed scans in this range that need recount
            let mut scan_stmt = conn.prepare_cached(
                "SELECT scan_id FROM scans
                 WHERE root_id = ? AND state = 4
                   AND scan_id >= ? AND scan_id <= ?",
            )?;
            let scans: Vec<i64> = scan_stmt
                .query_map(params![root_id, first_scan, last_scan], |row| row.get(0))?
                .collect::<Result<Vec<_>, _>>()?;
            affected_scans.extend(scans);
        }
    }

    drop(check_stmt);
    drop(fix_stmt);

    migration_info(&format!(
        "    Fixed {} folder versions, {} scans need recount",
        fixed_count,
        affected_scans.len()
    ));

    if affected_scans.is_empty() {
        return Ok(());
    }

    // Phase 2: Recompute population and integrity counts for affected scans.
    //
    // We use flat queries matching the runtime scanner's approach (scans.rs
    // set_state_completed). For historical scans we must use the range pattern
    // (first_scan_id <= scan AND last_scan_id >= scan) since last_scan_id may
    // extend beyond the scan being recomputed.
    migration_info("  Recomputing scan counts for affected scans...");

    let mut sorted_scans: Vec<i64> = affected_scans.into_iter().collect();
    sorted_scans.sort();

    // Prepare recount statements
    let mut pop_stmt = conn.prepare(
        "SELECT
            COALESCE(SUM(CASE WHEN i.item_type = 0 THEN 1 ELSE 0 END), 0),
            COALESCE(SUM(CASE WHEN i.item_type = 1 THEN 1 ELSE 0 END), 0)
         FROM items i
         JOIN item_versions iv ON iv.item_id = i.item_id
         WHERE i.root_id = ?1
           AND iv.is_deleted = 0
           AND iv.first_scan_id = (
               SELECT MAX(first_scan_id) FROM item_versions
               WHERE item_id = i.item_id AND first_scan_id <= ?2
           )
           AND iv.last_scan_id >= ?2",
    )?;

    let mut integrity_stmt = conn.prepare(
        "SELECT
            COALESCE(SUM(CASE WHEN i.has_validator = 1 AND iv.val_state IS NULL THEN 1 ELSE 0 END), 0),
            COALESCE(SUM(CASE WHEN iv.val_state = 1 THEN 1 ELSE 0 END), 0),
            COALESCE(SUM(CASE WHEN iv.val_state = 2 THEN 1 ELSE 0 END), 0),
            COALESCE(SUM(CASE WHEN i.has_validator = 0 THEN 1 ELSE 0 END), 0),
            COALESCE(SUM(CASE WHEN hv.hash_state IS NULL THEN 1 ELSE 0 END), 0),
            COALESCE(SUM(CASE WHEN hv.hash_state = 1 THEN 1 ELSE 0 END), 0),
            COALESCE(SUM(CASE WHEN hv.hash_state = 2 THEN 1 ELSE 0 END), 0)
         FROM items i
         JOIN item_versions iv ON iv.item_id = i.item_id
         LEFT JOIN hash_versions hv ON hv.item_id = iv.item_id
             AND hv.item_version = iv.item_version
             AND hv.first_scan_id = (
                 SELECT MAX(first_scan_id) FROM hash_versions
                 WHERE item_id = iv.item_id AND item_version = iv.item_version
                   AND first_scan_id <= ?2
             )
         WHERE i.root_id = ?1
           AND i.item_type = 0
           AND iv.is_deleted = 0
           AND iv.first_scan_id = (
               SELECT MAX(first_scan_id) FROM item_versions
               WHERE item_id = i.item_id AND first_scan_id <= ?2
           )
           AND iv.last_scan_id >= ?2",
    )?;

    let mut change_stmt = conn.prepare(
        "SELECT
            COALESCE(COUNT(*) FILTER (WHERE iv.is_deleted = 0
                AND (pv.item_id IS NULL OR pv.is_deleted = 1)), 0),
            COALESCE(COUNT(*) FILTER (WHERE iv.is_deleted = 0
                AND pv.item_id IS NOT NULL AND pv.is_deleted = 0), 0),
            COALESCE(COUNT(*) FILTER (WHERE iv.is_deleted = 1
                AND pv.item_id IS NOT NULL AND pv.is_deleted = 0), 0)
         FROM item_versions iv
         LEFT JOIN item_versions pv ON pv.item_id = iv.item_id
             AND pv.first_scan_id = (
                 SELECT MAX(first_scan_id) FROM item_versions
                 WHERE item_id = iv.item_id AND first_scan_id < iv.first_scan_id
             )
         WHERE iv.root_id = ?1 AND iv.first_scan_id = ?2",
    )?;

    let mut update_stmt = conn.prepare(
        "UPDATE scans SET
            file_count = ?, folder_count = ?,
            add_count = ?, modify_count = ?, delete_count = ?,
            val_unknown_count = ?, val_valid_count = ?,
            val_invalid_count = ?, val_no_validator_count = ?,
            hash_unknown_count = ?, hash_baseline_count = ?, hash_suspect_count = ?
         WHERE scan_id = ?",
    )?;

    let total = sorted_scans.len();
    for (idx, scan_id) in sorted_scans.iter().enumerate() {
        // Look up root_id for this scan
        let root_id: i64 = conn.query_row(
            "SELECT root_id FROM scans WHERE scan_id = ?",
            [scan_id],
            |row| row.get(0),
        )?;

        // Population counts
        let (file_count, folder_count): (i64, i64) =
            pop_stmt.query_row(params![root_id, scan_id], |row| {
                Ok((row.get(0)?, row.get(1)?))
            })?;

        // Integrity counts
        let (vu, vv, vi, vn, hu, hb, hs): (i64, i64, i64, i64, i64, i64, i64) =
            integrity_stmt.query_row(params![root_id, scan_id], |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                    row.get(6)?,
                ))
            })?;

        // Change counts
        let (adds, mods, dels): (i64, i64, i64) =
            change_stmt.query_row(params![root_id, scan_id], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?))
            })?;

        update_stmt.execute(params![
            file_count,
            folder_count,
            adds,
            mods,
            dels,
            vu,
            vv,
            vi,
            vn,
            hu,
            hb,
            hs,
            scan_id,
        ])?;

        if (idx + 1) % 25 == 0 || idx + 1 == total {
            migration_info(&format!("    Recounted {}/{} scans", idx + 1, total));
        }
    }

    drop(pop_stmt);
    drop(integrity_stmt);
    drop(change_stmt);
    drop(update_stmt);

    migration_info(&format!(
        "  Migration v29→v30 complete: {} folder versions fixed, {} scans recounted",
        fixed_count, total
    ));
    Ok(())
}
