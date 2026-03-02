use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use log::info;
use rusqlite::{params, Connection, OptionalExtension};
use std::path::MAIN_SEPARATOR_STR;

use crate::db::{migration_info, Database};
use crate::error::FsPulseError;

// ============================================================================
// Schema Upgrade: Version 23 → 24 (Standalone)
//
// Adds:
//   - hash_state column to item_versions (file rows)
//   - val renamed to val_state
//   - 7 validation/hash state snapshot count columns to item_versions (folder rows)
//   - 7 validation/hash state snapshot count columns to scans
//   - Column reordering (validation group, then hash group)
//
// Phases:
//   1. DDL: Recreate item_versions with new schema (rename, new columns,
//      column reorder, hash_state backfill inline) + ALTER TABLE scans
//   2. For each completed scan (chronological order):
//      a. Compute and write scan-level state counts
//      b. Walk folder tree, update existing folder versions with state counts
//
// Crash recovery: Phase 1 uses a meta key. Phase 2 uses a high-water mark.
//
// NOTE on folder state counts: This migration only updates folder versions that
// already have first_scan_id = scan_id (created by the scanner or a prior
// migration). It does NOT create new versions for folders that had no
// descendant changes. Going forward, the scanner handles all cases.
// ============================================================================

const BACKFILL_META_KEY: &str = "v24_backfill_hwm";
const DDL_DONE_META_KEY: &str = "v24_ddl_done";

const BATCH_SIZE: usize = 500;

// ---- Archived data types ----

struct StateCountUpdate {
    version_id: i64,
    val_unknown: i64,
    val_valid: i64,
    val_invalid: i64,
    val_no_validator: i64,
    hash_unknown: i64,
    hash_valid: i64,
    hash_suspicious: i64,
}

// ---- Phase 1: DDL ----
//
// Recreate item_versions with:
//   - val renamed to val_state
//   - hash_state backfilled inline (NULL for folders, 0/1 for files based on file_hash)
//   - 7 state count columns added (all NULL initially)
//   - Columns reordered (validation group, then hash group)
// Also ALTER TABLE scans to add 7 state count columns.

fn run_ddl(conn: &Connection) -> Result<(), FsPulseError> {
    if let Some(val) = Database::get_meta_value_locked(conn, DDL_DONE_META_KEY)? {
        if val == "1" {
            info!("Migration 23→24: DDL already applied, skipping.");
            return Ok(());
        }
    }

    Database::immediate_transaction(conn, |c| {
        // Recreate item_versions with new schema
        c.execute_batch(
            "CREATE TABLE item_versions_new (
                version_id      INTEGER PRIMARY KEY AUTOINCREMENT,
                item_id         INTEGER NOT NULL,
                first_scan_id   INTEGER NOT NULL,
                last_scan_id    INTEGER NOT NULL,

                -- Shared fields (all item types)
                is_added        BOOLEAN NOT NULL DEFAULT 0,
                is_deleted      BOOLEAN NOT NULL DEFAULT 0,
                access          INTEGER NOT NULL DEFAULT 0,
                mod_date        INTEGER,
                size            INTEGER,

                -- Validation fields (NULL for folders)
                last_val_scan   INTEGER,
                val_state       INTEGER,
                val_error       TEXT,

                -- Hash fields (NULL for folders)
                last_hash_scan  INTEGER,
                file_hash       TEXT,
                hash_state      INTEGER,

                -- Folder-specific descendant change counts (NULL for files)
                add_count       INTEGER,
                modify_count    INTEGER,
                delete_count    INTEGER,
                unchanged_count INTEGER,

                -- Folder-specific descendant state snapshot counts (NULL for files)
                val_unknown_count        INTEGER,
                val_valid_count          INTEGER,
                val_invalid_count        INTEGER,
                val_no_validator_count   INTEGER,
                hash_unknown_count       INTEGER,
                hash_valid_count         INTEGER,
                hash_suspicious_count    INTEGER,

                FOREIGN KEY (item_id) REFERENCES items(item_id),
                FOREIGN KEY (first_scan_id) REFERENCES scans(scan_id),
                FOREIGN KEY (last_scan_id) REFERENCES scans(scan_id)
            );

            INSERT INTO item_versions_new (
                version_id, item_id, first_scan_id, last_scan_id,
                is_added, is_deleted, access, mod_date, size,
                last_val_scan, val_state, val_error,
                last_hash_scan, file_hash, hash_state,
                add_count, modify_count, delete_count, unchanged_count
            )
            SELECT
                iv.version_id, iv.item_id, iv.first_scan_id, iv.last_scan_id,
                iv.is_added, iv.is_deleted, iv.access, iv.mod_date, iv.size,
                iv.last_val_scan, iv.val, iv.val_error,
                iv.last_hash_scan, iv.file_hash,
                CASE
                    WHEN i.item_type = 1 THEN NULL
                    WHEN iv.file_hash IS NULL THEN 0
                    ELSE 1
                END,
                iv.add_count, iv.modify_count, iv.delete_count, iv.unchanged_count
            FROM item_versions iv
            JOIN items i ON i.item_id = iv.item_id;

            DROP TABLE item_versions;
            ALTER TABLE item_versions_new RENAME TO item_versions;

            UPDATE sqlite_sequence
            SET seq = (SELECT MAX(version_id) FROM item_versions)
            WHERE name = 'item_versions';

            CREATE INDEX idx_versions_item_scan ON item_versions (item_id, first_scan_id DESC);
            CREATE INDEX idx_versions_first_scan ON item_versions (first_scan_id);"
        )?;

        // scans: 7 state count columns
        c.execute_batch(
            "ALTER TABLE scans ADD COLUMN val_unknown_count INTEGER DEFAULT NULL;",
        )?;
        c.execute_batch(
            "ALTER TABLE scans ADD COLUMN val_valid_count INTEGER DEFAULT NULL;",
        )?;
        c.execute_batch(
            "ALTER TABLE scans ADD COLUMN val_invalid_count INTEGER DEFAULT NULL;",
        )?;
        c.execute_batch(
            "ALTER TABLE scans ADD COLUMN val_no_validator_count INTEGER DEFAULT NULL;",
        )?;
        c.execute_batch(
            "ALTER TABLE scans ADD COLUMN hash_unknown_count INTEGER DEFAULT NULL;",
        )?;
        c.execute_batch(
            "ALTER TABLE scans ADD COLUMN hash_valid_count INTEGER DEFAULT NULL;",
        )?;
        c.execute_batch(
            "ALTER TABLE scans ADD COLUMN hash_suspicious_count INTEGER DEFAULT NULL;",
        )?;

        Database::set_meta_value_locked(c, DDL_DONE_META_KEY, "1")?;
        Ok(())
    })?;

    migration_info("    item_versions recreated (val→val_state, hash_state backfilled, columns reordered); scans columns added.");
    Ok(())
}

// ---- Phase 2: Scan-level and folder-level state count backfill ----

fn v24_check_interrupted(interrupt_token: &Arc<AtomicBool>) -> Result<(), FsPulseError> {
    if interrupt_token.load(Ordering::Acquire) {
        Err(FsPulseError::TaskInterrupted)
    } else {
        Ok(())
    }
}

/// Compute and write scan-level state counts for a single scan.
fn backfill_scan_state_counts(
    conn: &Connection,
    scan_id: i64,
    root_id: i64,
) -> Result<(), FsPulseError> {
    let counts: (i64, i64, i64, i64, i64, i64, i64) = conn.query_row(
        "SELECT
            COALESCE(SUM(CASE WHEN COALESCE(iv.val_state, 0) = 0 THEN 1 ELSE 0 END), 0),
            COALESCE(SUM(CASE WHEN iv.val_state = 1 THEN 1 ELSE 0 END), 0),
            COALESCE(SUM(CASE WHEN iv.val_state = 2 THEN 1 ELSE 0 END), 0),
            COALESCE(SUM(CASE WHEN iv.val_state = 3 THEN 1 ELSE 0 END), 0),
            COALESCE(SUM(CASE WHEN COALESCE(iv.hash_state, 0) = 0 THEN 1 ELSE 0 END), 0),
            COALESCE(SUM(CASE WHEN iv.hash_state = 1 THEN 1 ELSE 0 END), 0),
            COALESCE(SUM(CASE WHEN iv.hash_state = 2 THEN 1 ELSE 0 END), 0)
         FROM items i
         JOIN item_versions iv ON iv.item_id = i.item_id
         WHERE i.root_id = ?1
           AND i.item_type = 0
           AND iv.is_deleted = 0
           AND iv.first_scan_id <= ?2
           AND iv.last_scan_id >= ?2",
        params![root_id, scan_id],
        |row| {
            Ok((
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
                row.get(4)?,
                row.get(5)?,
                row.get(6)?,
            ))
        },
    )?;

    conn.execute(
        "UPDATE scans SET
            val_unknown_count = ?,
            val_valid_count = ?,
            val_invalid_count = ?,
            val_no_validator_count = ?,
            hash_unknown_count = ?,
            hash_valid_count = ?,
            hash_suspicious_count = ?
         WHERE scan_id = ?",
        params![
            counts.0, counts.1, counts.2, counts.3,
            counts.4, counts.5, counts.6,
            scan_id
        ],
    )?;

    Ok(())
}

// ---- Archived folder walk logic (state counts only) ----

fn v24_query_immediate_dir_children(
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

/// Query state counts for alive direct file children of a folder.
#[allow(clippy::type_complexity)]
fn v24_query_direct_file_state_counts(
    conn: &Connection,
    root_id: i64,
    parent_path: &str,
    scan_id: i64,
) -> Result<(i64, i64, i64, i64, i64, i64, i64), FsPulseError> {
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
            COALESCE(SUM(CASE WHEN COALESCE(iv.val_state, 0) = 0 THEN 1 ELSE 0 END), 0),
            COALESCE(SUM(CASE WHEN iv.val_state = 1 THEN 1 ELSE 0 END), 0),
            COALESCE(SUM(CASE WHEN iv.val_state = 2 THEN 1 ELSE 0 END), 0),
            COALESCE(SUM(CASE WHEN iv.val_state = 3 THEN 1 ELSE 0 END), 0),
            COALESCE(SUM(CASE WHEN COALESCE(iv.hash_state, 0) = 0 THEN 1 ELSE 0 END), 0),
            COALESCE(SUM(CASE WHEN iv.hash_state = 1 THEN 1 ELSE 0 END), 0),
            COALESCE(SUM(CASE WHEN iv.hash_state = 2 THEN 1 ELSE 0 END), 0)
         FROM items i
         JOIN item_versions iv ON iv.item_id = i.item_id
         WHERE i.root_id = ?1
           AND i.item_type = 0
           AND iv.is_deleted = 0
           AND iv.first_scan_id <= ?2
           AND iv.last_scan_id >= ?2
           AND i.item_path >= ?3
           AND i.item_path < ?4
           AND SUBSTR(i.item_path, LENGTH(?3) + 1) NOT LIKE '%{}%'",
        MAIN_SEPARATOR_STR
    );

    let mut stmt = conn.prepare(&sql)?;
    let result = stmt.query_row(
        params![root_id, scan_id, &path_prefix, &path_upper],
        |row| {
            Ok((
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
                row.get(4)?,
                row.get(5)?,
                row.get(6)?,
            ))
        },
    )?;

    Ok(result)
}

fn v24_lookup_folder_item_id(
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

/// Recursive depth-first walk computing state counts for folders.
///
/// Returns the 7 state counts for all alive descendant files under parent_path.
/// For each folder that has an existing version at this scan (first_scan_id = scan_id),
/// records an update entry. Folders without a version at this scan are skipped
/// (their state counts still contribute to the parent).
#[allow(clippy::type_complexity)]
fn v24_walk_state_counts(
    conn: &Connection,
    root_id: i64,
    scan_id: i64,
    parent_path: &str,
    interrupt_token: &Arc<AtomicBool>,
    updates: &mut Vec<StateCountUpdate>,
) -> Result<(i64, i64, i64, i64, i64, i64, i64), FsPulseError> {
    v24_check_interrupted(interrupt_token)?;

    let mut vu = 0i64;
    let mut vv = 0i64;
    let mut vi = 0i64;
    let mut vnv = 0i64;
    let mut hu = 0i64;
    let mut hv = 0i64;
    let mut hs = 0i64;

    // 1. Get immediate directory children alive at this scan
    let dir_children = v24_query_immediate_dir_children(conn, root_id, parent_path, scan_id)?;

    // 2. Recurse into each directory child
    for (_child_id, child_path) in &dir_children {
        let (cvu, cvv, cvi, cvnv, chu, chv, chs) =
            v24_walk_state_counts(conn, root_id, scan_id, child_path, interrupt_token, updates)?;
        vu += cvu;
        vv += cvv;
        vi += cvi;
        vnv += cvnv;
        hu += chu;
        hv += chv;
        hs += chs;
    }

    // 3. Count direct file children's state counts
    let (dvu, dvv, dvi, dvnv, dhu, dhv, dhs) =
        v24_query_direct_file_state_counts(conn, root_id, parent_path, scan_id)?;
    vu += dvu;
    vv += dvv;
    vi += dvi;
    vnv += dvnv;
    hu += dhu;
    hv += dhv;
    hs += dhs;

    // 4. If folder has a version created at this scan, record update
    if let Some(folder_item_id) = v24_lookup_folder_item_id(conn, root_id, parent_path)? {
        let version_id: Option<i64> = conn
            .query_row(
                "SELECT version_id FROM item_versions
                 WHERE item_id = ? AND first_scan_id = ?",
                params![folder_item_id, scan_id],
                |row| row.get(0),
            )
            .optional()?;

        if let Some(vid) = version_id {
            updates.push(StateCountUpdate {
                version_id: vid,
                val_unknown: vu,
                val_valid: vv,
                val_invalid: vi,
                val_no_validator: vnv,
                hash_unknown: hu,
                hash_valid: hv,
                hash_suspicious: hs,
            });
        }
    }

    Ok((vu, vv, vi, vnv, hu, hv, hs))
}

/// Apply state count updates in batched transactions.
fn v24_apply_state_count_updates(
    conn: &Connection,
    updates: &[StateCountUpdate],
    interrupt_token: &Arc<AtomicBool>,
) -> Result<(), FsPulseError> {
    for batch in updates.chunks(BATCH_SIZE) {
        v24_check_interrupted(interrupt_token)?;
        Database::immediate_transaction(conn, |c| {
            for u in batch {
                c.execute(
                    "UPDATE item_versions SET
                        val_unknown_count = ?,
                        val_valid_count = ?,
                        val_invalid_count = ?,
                        val_no_validator_count = ?,
                        hash_unknown_count = ?,
                        hash_valid_count = ?,
                        hash_suspicious_count = ?
                     WHERE version_id = ?",
                    params![
                        u.val_unknown,
                        u.val_valid,
                        u.val_invalid,
                        u.val_no_validator,
                        u.hash_unknown,
                        u.hash_valid,
                        u.hash_suspicious,
                        u.version_id
                    ],
                )?;
            }
            Ok(())
        })?;
    }
    Ok(())
}

/// Worker: compute and write folder-level state counts for one scan.
fn v24_scan_state_count_worker(
    root_id: i64,
    scan_id: i64,
    root_path: &str,
    interrupt_token: &Arc<AtomicBool>,
) -> Result<(), FsPulseError> {
    let conn = Database::get_connection()?;

    let mut updates = Vec::new();
    v24_walk_state_counts(
        &conn,
        root_id,
        scan_id,
        root_path,
        interrupt_token,
        &mut updates,
    )?;

    info!(
        "v24 scan {}: {} folder versions updated with state counts",
        scan_id,
        updates.len()
    );

    v24_apply_state_count_updates(&conn, &updates, interrupt_token)?;
    Ok(())
}

// ---- Migration entry point ----

pub fn run_migration_v23_to_v24(conn: &Connection) -> Result<(), FsPulseError> {
    // Phase 1: DDL (recreate item_versions + scans ALTER TABLEs)
    run_ddl(conn)?;

    // Phase 2: Backfill scan-level + folder-level state counts
    let hwm: i64 = match Database::get_meta_value_locked(conn, BACKFILL_META_KEY)? {
        Some(val) => val.parse().unwrap_or(0),
        None => {
            Database::immediate_transaction(conn, |c| {
                Database::set_meta_value_locked(c, BACKFILL_META_KEY, "0")
            })?;
            0
        }
    };

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

    if total > 0 {
        migration_info(&format!(
            "    Backfilling state counts for {} completed scans...",
            total
        ));

        let dummy_token = Arc::new(AtomicBool::new(false));

        for (completed, (scan_id, root_id, root_path)) in scans.iter().enumerate() {
            info!(
                "Migration 23→24: Processing scan {} ({}/{})",
                scan_id,
                completed + 1,
                total
            );

            // Backfill scan-level state counts
            Database::immediate_transaction(conn, |c| {
                backfill_scan_state_counts(c, *scan_id, *root_id)
            })?;

            // Backfill folder-level state counts
            v24_scan_state_count_worker(*root_id, *scan_id, root_path, &dummy_token)?;

            // Progress reporting
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

        migration_info(&format!(
            "    Backfilled state counts for {} scans.",
            total
        ));
    } else {
        info!("Migration 23→24: No completed scans to backfill state counts.");
    }

    // All done — clean up and bump version
    Database::immediate_transaction(conn, |c| {
        Database::delete_meta_locked(c, BACKFILL_META_KEY)?;
        Database::delete_meta_locked(c, DDL_DONE_META_KEY)?;
        Database::set_meta_value_locked(c, "schema_version", "24")
    })?;

    Ok(())
}
