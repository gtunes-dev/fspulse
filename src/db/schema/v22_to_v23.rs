use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use log::info;
use rusqlite::{params, Connection, OptionalExtension};
use std::path::MAIN_SEPARATOR_STR;

use crate::db::{migration_info, Database};
use crate::error::FsPulseError;

// ============================================================================
// Schema Upgrade: Version 22 → 23 (Standalone)
//
// Backfills `unchanged_count` for all historical completed scans.
// This is a Standalone migration — it manages its own transactions because
// the recursive walk + batched writes require multiple independent transactions.
//
// IMPORTANT: This migration is self-contained. All scanner analysis logic is
// duplicated here as archived code. This prevents breakage when the production
// scanner code evolves. The functions below operate against the v22 schema,
// which has add_count, modify_count, delete_count, AND unchanged_count on
// item_versions.
//
// The underlying logic is idempotent: it either UPDATEs an existing version
// from the same scan (Case A) or closes a previous version and INSERTs a new
// one (Case B, with carry-forward). Running it again on an already-processed
// scan is harmless.
//
// Crash recovery: If the process dies mid-backfill, the schema version is
// still 22. On restart, the migration loop re-runs this function. The HWM
// in the meta table lets it skip already-processed scans efficiently.
// ============================================================================

/// Meta table key for tracking backfill progress (high-water mark).
const BACKFILL_META_KEY: &str = "v23_backfill_hwm";

const BATCH_SIZE: usize = 500;

// ---- Archived data types ----

struct V22FolderCountWrite {
    folder_item_id: i64,
    adds: i64,
    mods: i64,
    dels: i64,
    unchanged: i64,
}

// ---- Archived scanner analysis logic (v22 schema: 4-count, with unchanged_count) ----

fn v22_check_interrupted(interrupt_token: &Arc<AtomicBool>) -> Result<(), FsPulseError> {
    if interrupt_token.load(Ordering::Acquire) {
        Err(FsPulseError::TaskInterrupted)
    } else {
        Ok(())
    }
}

/// Find the most recent completed scan before the current one for this root.
fn v22_query_prev_completed_scan(
    conn: &Connection,
    root_id: i64,
    scan_id: i64,
) -> Result<Option<i64>, FsPulseError> {
    let prev: Option<i64> = conn
        .query_row(
            "SELECT MAX(scan_id) FROM scans
             WHERE root_id = ? AND scan_id < ? AND state = 4",
            params![root_id, scan_id],
            |row| row.get(0),
        )
        .optional()?
        .flatten();
    Ok(prev)
}

/// Query immediate directory children of `parent_path` that are alive at `scan_id`
/// (or deleted AT `scan_id`, so we can recurse into deleted subtrees).
fn v22_query_immediate_dir_children(
    conn: &Connection,
    root_id: i64,
    parent_path: &str,
    scan_id: i64,
) -> Result<Vec<(i64, String)>, FsPulseError> {
    let path_prefix = if parent_path.ends_with(MAIN_SEPARATOR_STR) {
        parent_path.to_string()
    } else {
        format!("{}{}", parent_path, MAIN_SEPARATOR_STR)
    };

    let path_upper = format!(
        "{}{}",
        &path_prefix[..path_prefix.len() - MAIN_SEPARATOR_STR.len()],
        char::from(std::path::MAIN_SEPARATOR as u8 + 1)
    );

    let sql = format!(
        "SELECT i.item_id, i.item_path
         FROM items i
         JOIN item_versions iv ON iv.item_id = i.item_id
         WHERE i.root_id = ?1
           AND i.item_type = 1
           AND iv.first_scan_id = (
               SELECT MAX(first_scan_id) FROM item_versions
               WHERE item_id = i.item_id AND first_scan_id <= ?2
           )
           AND (iv.is_deleted = 0 OR iv.first_scan_id = ?2)
           AND i.item_path >= ?3
           AND i.item_path < ?4
           AND i.item_path != ?5
           AND SUBSTR(i.item_path, LENGTH(?3) + 1) NOT LIKE '%{}%'",
        MAIN_SEPARATOR_STR
    );

    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(
        params![root_id, scan_id, &path_prefix, &path_upper, parent_path],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(FsPulseError::DatabaseError)
}

/// Count direct children of `parent_path` that changed in this scan, classified
/// as add/modify/delete by comparing the current version with the previous version.
fn v22_query_direct_change_counts(
    conn: &Connection,
    root_id: i64,
    parent_path: &str,
    scan_id: i64,
) -> Result<(i64, i64, i64), FsPulseError> {
    let path_prefix = if parent_path.ends_with(MAIN_SEPARATOR_STR) {
        parent_path.to_string()
    } else {
        format!("{}{}", parent_path, MAIN_SEPARATOR_STR)
    };

    let path_upper = format!(
        "{}{}",
        &path_prefix[..path_prefix.len() - MAIN_SEPARATOR_STR.len()],
        char::from(std::path::MAIN_SEPARATOR as u8 + 1)
    );

    let sql = format!(
        "SELECT
            COALESCE(SUM(CASE WHEN cv.is_deleted = 0
                AND (pv.version_id IS NULL OR pv.is_deleted = 1) THEN 1 ELSE 0 END), 0),
            COALESCE(SUM(CASE WHEN cv.is_deleted = 0
                AND pv.version_id IS NOT NULL AND pv.is_deleted = 0 THEN 1 ELSE 0 END), 0),
            COALESCE(SUM(CASE WHEN cv.is_deleted = 1
                AND pv.version_id IS NOT NULL AND pv.is_deleted = 0 THEN 1 ELSE 0 END), 0)
         FROM items i
         JOIN item_versions cv ON cv.item_id = i.item_id AND cv.first_scan_id = ?1
         LEFT JOIN item_versions pv ON pv.item_id = i.item_id
             AND pv.first_scan_id = (
                 SELECT MAX(first_scan_id) FROM item_versions
                 WHERE item_id = i.item_id AND first_scan_id < cv.first_scan_id
             )
         WHERE i.root_id = ?2
           AND i.item_path >= ?3
           AND i.item_path < ?4
           AND i.item_path != ?5
           AND SUBSTR(i.item_path, LENGTH(?3) + 1) NOT LIKE '%{}%'",
        MAIN_SEPARATOR_STR
    );

    let mut stmt = conn.prepare(&sql)?;
    let result = stmt.query_row(
        params![scan_id, root_id, &path_prefix, &path_upper, parent_path],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    )?;

    Ok(result)
}

/// Look up the item_id for a folder by its path.
fn v22_lookup_folder_item_id(
    conn: &Connection,
    root_id: i64,
    path: &str,
) -> Result<Option<i64>, FsPulseError> {
    let item_id: Option<i64> = conn
        .query_row(
            "SELECT item_id FROM items
             WHERE root_id = ? AND item_path = ? AND item_type = 1",
            params![root_id, path],
            |row| row.get(0),
        )
        .optional()?;
    Ok(item_id)
}

/// Query the total alive descendant count from a folder's previous version.
///
/// Returns `add_count + modify_count + unchanged_count` from the version just before
/// `scan_id`. Returns 0 if no previous version exists (first scan or new folder).
///
/// Used to derive: `unchanged = prev_alive - mods - dels` — everyone alive in the
/// previous scan was either modified, deleted, or unchanged in this scan.
fn v22_query_prev_alive(
    conn: &Connection,
    folder_item_id: i64,
    scan_id: i64,
) -> Result<i64, FsPulseError> {
    let alive: Option<i64> = conn
        .query_row(
            "SELECT COALESCE(iv.add_count, 0) + COALESCE(iv.modify_count, 0) + COALESCE(iv.unchanged_count, 0)
             FROM item_versions iv
             WHERE iv.item_id = ?1
               AND iv.first_scan_id = (
                   SELECT MAX(first_scan_id) FROM item_versions
                   WHERE item_id = ?1 AND first_scan_id < ?2
               )",
            params![folder_item_id, scan_id],
            |row| row.get(0),
        )
        .optional()?;

    Ok(alive.unwrap_or(0))
}

/// Recursive depth-first walk of the folder tree, computing descendant change counts.
///
/// Returns the cumulative `(adds, mods, dels)` for all descendants under `parent_path`.
/// Appends a `V22FolderCountWrite` entry for each folder that has non-zero change counts.
///
/// The `unchanged` count is derived per-folder at write time from the previous version:
///   `unchanged = prev_alive - mods - dels`
/// where `prev_alive = prev_adds + prev_mods + prev_unchanged` from the folder's
/// temporal version before this scan.
fn v22_walk_folder_counts(
    conn: &Connection,
    root_id: i64,
    scan_id: i64,
    parent_path: &str,
    interrupt_token: &Arc<AtomicBool>,
    writes: &mut Vec<V22FolderCountWrite>,
) -> Result<(i64, i64, i64), FsPulseError> {
    v22_check_interrupted(interrupt_token)?;

    let mut adds = 0i64;
    let mut mods = 0i64;
    let mut dels = 0i64;

    // 1. Get immediate directory children alive at this scan
    let dir_children = v22_query_immediate_dir_children(conn, root_id, parent_path, scan_id)?;

    // 2. Recurse into each directory child
    for (_child_id, child_path) in &dir_children {
        let (sa, sm, sd) = v22_walk_folder_counts(
            conn, root_id, scan_id, child_path, interrupt_token, writes,
        )?;
        adds += sa;
        mods += sm;
        dels += sd;
    }

    // 3. Count direct children that changed in this scan
    let (da, dm, dd) = v22_query_direct_change_counts(conn, root_id, parent_path, scan_id)?;
    adds += da;
    mods += dm;
    dels += dd;

    // 4. Record write if any descendant changes
    if adds > 0 || mods > 0 || dels > 0 {
        if let Some(folder_item_id) = v22_lookup_folder_item_id(conn, root_id, parent_path)? {
            // Derive unchanged from previous version's alive count:
            // everyone alive before was either modified, deleted, or unchanged.
            let prev_alive = v22_query_prev_alive(conn, folder_item_id, scan_id)?;
            let unchanged = prev_alive - mods - dels;

            writes.push(V22FolderCountWrite {
                folder_item_id,
                adds,
                mods,
                dels,
                unchanged,
            });
        }
    }

    Ok((adds, mods, dels))
}

/// Write counts for a single folder.
///
/// - **Case A**: Folder already has a version with `first_scan_id = scan_id` → UPDATE counts.
/// - **Case B**: No version for this scan → close the pre-existing version by restoring
///   `last_scan_id` to `prev_scan_id`, then INSERT a new version carrying forward all
///   metadata with the computed counts (via INSERT...SELECT for simplicity).
fn v22_write_single_folder_count(
    conn: &Connection,
    scan_id: i64,
    prev_scan_id: Option<i64>,
    w: &V22FolderCountWrite,
) -> Result<(), FsPulseError> {
    // Check if folder already has a version for this scan
    let existing_version_id: Option<i64> = conn
        .query_row(
            "SELECT version_id FROM item_versions
             WHERE item_id = ? AND first_scan_id = ?",
            params![w.folder_item_id, scan_id],
            |row| row.get(0),
        )
        .optional()?;

    if let Some(version_id) = existing_version_id {
        // Case A: UPDATE the existing version's counts
        conn.execute(
            "UPDATE item_versions SET add_count = ?, modify_count = ?, delete_count = ?, unchanged_count = ?
             WHERE version_id = ?",
            params![w.adds, w.mods, w.dels, w.unchanged, version_id],
        )?;
    } else {
        // Case B: Folder metadata unchanged but descendants changed.
        // Close the current version's last_scan_id, then INSERT...SELECT to carry
        // forward all metadata with the computed counts — all in SQL, no Rust struct needed.
        if let Some(prev) = prev_scan_id {
            conn.execute(
                "UPDATE item_versions SET last_scan_id = ?
                 WHERE version_id = (
                     SELECT version_id FROM item_versions
                     WHERE item_id = ? ORDER BY first_scan_id DESC LIMIT 1
                 )",
                params![prev, w.folder_item_id],
            )?;
        }

        conn.execute(
            "INSERT INTO item_versions (
                item_id, first_scan_id, last_scan_id,
                is_added, is_deleted, access, mod_date, size,
                file_hash, val, val_error,
                last_hash_scan, last_val_scan,
                add_count, modify_count, delete_count, unchanged_count
             )
             SELECT
                item_id, ?, ?,
                0, is_deleted, access, mod_date, size,
                file_hash, val, val_error,
                last_hash_scan, last_val_scan,
                ?, ?, ?, ?
             FROM item_versions
             WHERE item_id = ?
             ORDER BY first_scan_id DESC
             LIMIT 1",
            params![scan_id, scan_id, w.adds, w.mods, w.dels, w.unchanged, w.folder_item_id],
        )?;
    }

    Ok(())
}

/// Apply folder count writes in batched transactions.
fn v22_apply_folder_count_writes(
    conn: &Connection,
    scan_id: i64,
    writes: &[V22FolderCountWrite],
    prev_scan_id: Option<i64>,
    interrupt_token: &Arc<AtomicBool>,
) -> Result<(), FsPulseError> {
    for batch in writes.chunks(BATCH_SIZE) {
        v22_check_interrupted(interrupt_token)?;
        Database::immediate_transaction(conn, |c| {
            for w in batch {
                v22_write_single_folder_count(c, scan_id, prev_scan_id, w)?;
            }
            Ok(())
        })?;
    }
    Ok(())
}

/// Worker function: performs the recursive walk and writes folder counts.
fn v22_scan_analysis_worker(
    root_id: i64,
    scan_id: i64,
    root_path: &str,
    interrupt_token: &Arc<AtomicBool>,
) -> Result<(), FsPulseError> {
    let conn = Database::get_connection()?;
    let prev_scan_id = v22_query_prev_completed_scan(&conn, root_id, scan_id)?;

    let mut writes = Vec::new();
    v22_walk_folder_counts(&conn, root_id, scan_id, root_path, interrupt_token, &mut writes)?;

    info!("v22 scan analysis: {} folders have descendant changes", writes.len());

    v22_apply_folder_count_writes(&conn, scan_id, &writes, prev_scan_id, interrupt_token)?;
    Ok(())
}

// ---- Migration entry point ----

/// Standalone migration v22→v23: backfill `unchanged_count` for folder versions.
///
/// Iterates through all completed scans (ordered by scan_id ascending),
/// running the archived recursive walk + write logic against the v22 schema.
/// Tracks progress via a high-water mark stored in the meta table.
pub fn run_backfill_unchanged_count(conn: &Connection) -> Result<(), FsPulseError> {
    // Read or create HWM. On first run, the key doesn't exist — start from 0.
    // On resume after crash, the key holds the last successfully processed scan_id.
    let hwm: i64 = match Database::get_meta_value_locked(conn, BACKFILL_META_KEY)? {
        Some(val) => val.parse().unwrap_or(0),
        None => {
            // First run — insert the HWM key
            Database::immediate_transaction(conn, |c| {
                Database::set_meta_value_locked(c, BACKFILL_META_KEY, "0")
            })?;
            0
        }
    };

    // Query all completed scans after the HWM
    let mut stmt = conn.prepare(
        "SELECT s.scan_id, s.root_id, r.root_path
         FROM scans s
         JOIN roots r ON r.root_id = s.root_id
         WHERE s.state = 4 AND s.scan_id > ?
         ORDER BY s.scan_id ASC",
    )?;

    let scans: Vec<(i64, i64, String)> = stmt
        .query_map(params![hwm], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?))
        })?
        .collect::<Result<Vec<_>, _>>()?;

    let total = scans.len();

    if total == 0 {
        // Nothing to backfill — clean up HWM key and bump version atomically
        Database::immediate_transaction(conn, |c| {
            Database::delete_meta_locked(c, BACKFILL_META_KEY)?;
            Database::set_meta_value_locked(c, "schema_version", "23")
        })?;
        info!("Migration 22→23: No completed scans to backfill.");
        return Ok(());
    }

    migration_info(&format!(
        "    Backfilling unchanged_count for {} completed scans...",
        total
    ));

    // Dummy interrupt token — backfill at startup is not interruptible
    let dummy_token = Arc::new(AtomicBool::new(false));

    for (completed, (scan_id, root_id, root_path)) in scans.iter().enumerate() {
        info!(
            "Migration 22→23: Processing scan {} ({}/{})",
            scan_id,
            completed + 1,
            total
        );

        v22_scan_analysis_worker(*root_id, *scan_id, root_path, &dummy_token)?;

        // Periodic console progress every 25 scans
        let done = completed + 1;
        if done % 25 == 0 {
            migration_info(&format!(
                "    Backfill progress: {}/{} scans processed",
                done, total
            ));
        }

        // Persist HWM after each scan completes
        Database::immediate_transaction(conn, |c| {
            Database::set_meta_value_locked(c, BACKFILL_META_KEY, &scan_id.to_string())
        })?;
    }

    // All scans processed — delete HWM key and bump version atomically
    Database::immediate_transaction(conn, |c| {
        Database::delete_meta_locked(c, BACKFILL_META_KEY)?;
        Database::set_meta_value_locked(c, "schema_version", "23")
    })?;

    migration_info(&format!(
        "    Backfilled unchanged_count for {} scans.",
        total
    ));

    Ok(())
}
