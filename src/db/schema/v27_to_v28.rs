// ============================================================================
// Schema Upgrade: Version 27 → 28 — Project Integrity
//
// Separates hash state into a dedicated hash_versions table keyed on
// item_version_id. Moves validation state onto item_versions as columns.
// Removes phantom item_versions created by the old analysis phase.
// Removes val_all from scans. Renames validate_mode to is_val in scan_schedules.
//
// This is a Standalone migration. FK enforcement is disabled for the duration.
//
// ── Phantom versions (files only) ────────────────────────────────────────────
// The old v27 code created "phantom" item_versions when hash/val was first
// acquired on a pre-existing file version. A phantom has:
//   - is_added = 0, is_deleted = 0
//   - Identical access, mod_date, size to the immediately preceding real version
// These are detected by comparing each version to the last real (non-phantom)
// predecessor. Chains of phantoms (hash acquired in scan N, val in scan N+1)
// are handled correctly because we compare against the last *real* version.
//
// Folders are NOT subject to phantom detection. Folder versions may have
// identical metadata to their predecessor when only descendant counts changed
// (Case B in the scanner). These are legitimate versions. However, folder
// counts may be inflated by file phantoms counted as modifications, so Phase 3
// recomputes all folder counts and removes folder versions that become
// unnecessary (zero structural changes + metadata matches predecessor).
//
// ── Hash history ──────────────────────────────────────────────────────────────
// Versions are grouped into (real_version, phantom_cluster) pairs. Each group
// that contains hash data produces one hash_versions row:
//   item_version_id = the real version's version_id
//   first_scan_id = MIN(last_hash_scan) in the cluster (when hash was first got)
//   last_scan_id  = MAX(last_scan_id)   in the cluster (when it was last seen)
//   file_hash from the version with MIN(last_hash_scan) (baseline hash)
//   hash_state from the version with MAX(last_hash_scan) (most recent state)
//
// ── Validation ────────────────────────────────────────────────────────────────
// Val data is written directly onto the item_version row as val_scan_id,
// val_state, and val_error. No separate val_versions table.
//
// ── Idempotency / crash recovery ─────────────────────────────────────────────
// Phase 2 uses "lift and shift": each batch atomically INSERTs into the new
// tables AND DELETEs from item_versions_old. If the process crashes, the
// transaction rolls back and those items remain in item_versions_old. On
// restart, only unprocessed items (still in item_versions_old) are migrated.
// No high-water mark needed — item_versions_old IS the progress tracker.
//
// Phase 3 uses a high-water mark (meta key "v28_count_hwm") to track the last
// successfully processed scan_id. On restart it skips already-processed scans.
//
// Phases:
//   1. DDL (one transaction): roll back in-progress scans, rebuild items,
//      rebuild scans (drop val_all), create hash_versions, rename
//      item_versions → item_versions_old, create new item_versions (with val
//      columns), rebuild scan_undo_log, fix scan_schedules.
//   2. Data (batched transactions): lift-and-shift per batch of BATCH_SIZE items.
//   3. Count recomputation (per-scan transactions): recomputes add/modify/delete/
//      unchanged for all folder versions across all completed scans. Removes
//      folder versions that become unnecessary after recomputation. HWM-based.
//   4. Cleanup (one transaction): drop item_versions_old, bump version, delete HWM.
// ============================================================================

use std::collections::HashSet;
use std::path::MAIN_SEPARATOR_STR;
use std::time::Instant;

use rusqlite::{params, Connection, OptionalExtension};

use crate::db::{migration_info, Database};
use crate::error::FsPulseError;

const BATCH_SIZE: i64 = 5_000;
const COUNT_HWM_KEY: &str = "v28_count_hwm";

// ── Data structures ──────────────────────────────────────────────────────────

struct OldVersion {
    version_id: i64,
    first_scan_id: i64,
    last_scan_id: i64,
    is_added: i64,
    is_deleted: i64,
    access: i64,
    mod_date: Option<i64>,
    size: Option<i64>,
    add_count: Option<i64>,
    modify_count: Option<i64>,
    delete_count: Option<i64>,
    unchanged_count: Option<i64>,
    file_hash: Option<Vec<u8>>,
    last_hash_scan: Option<i64>,
    hash_state: Option<i64>,
    val_state: Option<i64>,
    val_error: Option<String>,
    last_val_scan: Option<i64>,
}

/// One real version and its following cluster of phantom versions.
/// effective_last_scan is the max last_scan_id across real + all phantoms.
struct Group {
    real_idx: usize,
    phantom_indices: Vec<usize>,
    effective_last_scan: i64,
}

struct FolderCountWrite {
    folder_item_id: i64,
    adds: i64,
    mods: i64,
    dels: i64,
    unchanged: i64,
}

/// Accumulated scan-level counts, built up during the recursive folder walk.
/// These are the counts that get stamped on the scan row.
#[derive(Default)]
struct ScanCounts {
    // Change counts (from the recursive walk)
    adds: i64,
    mods: i64,
    dels: i64,
    // Population counts
    file_count: i64,
    folder_count: i64,
    // Integrity counts (files only)
    hash_unknown: i64,
    hash_baseline: i64,
    hash_suspect: i64,
    val_unknown: i64,
    val_valid: i64,
    val_invalid: i64,
    val_no_validator: i64,
}

struct FolderVersionAtScan {
    version_id: i64,
    item_id: i64,
    is_added: i64,
    is_deleted: i64,
    access: i64,
    mod_date: Option<i64>,
    size: Option<i64>,
    last_scan_id: i64,
}

// ── Helpers ──────────────────────────────────────────────────────────────────

fn table_exists(conn: &Connection, name: &str) -> Result<bool, FsPulseError> {
    let n: i64 = conn.query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?",
        [name],
        |r| r.get(0),
    )?;
    Ok(n > 0)
}

fn rollback_in_progress_scans(conn: &Connection) -> Result<(), FsPulseError> {
    let mut stmt = conn.prepare("SELECT scan_id FROM scans WHERE state IN (1, 2, 3, 7)")?;
    let scan_ids: Vec<i64> = stmt
        .query_map([], |row| row.get(0))?
        .collect::<Result<Vec<_>, _>>()?;

    if scan_ids.is_empty() {
        return Ok(());
    }

    migration_info(&format!(
        "    Rolling back {} in-progress scan(s)...",
        scan_ids.len()
    ));

    for scan_id in &scan_ids {
        migration_info(&format!("      Scan {}", scan_id));
        conn.execute(
            "UPDATE item_versions SET
                last_scan_id   = u.old_last_scan_id,
                last_hash_scan = u.old_last_hash_scan,
                last_val_scan  = u.old_last_val_scan
             FROM scan_undo_log u
             WHERE item_versions.version_id = u.version_id",
            [],
        )?;
        conn.execute("DELETE FROM item_versions WHERE first_scan_id = ?", [scan_id])?;
        conn.execute(
            "DELETE FROM items WHERE NOT EXISTS (
                 SELECT 1 FROM item_versions iv WHERE iv.item_id = items.item_id
             )",
            [],
        )?;
        conn.execute("DELETE FROM scan_undo_log", [])?;
        conn.execute("DELETE FROM alerts WHERE scan_id = ?", [scan_id])?;
        conn.execute(
            "UPDATE scans SET state = 5, total_size = NULL, ended_at = strftime('%s','now')
             WHERE scan_id = ?",
            [scan_id],
        )?;
    }
    Ok(())
}

// ── Phase 1 ──────────────────────────────────────────────────────────────────

const P1_REBUILD_ITEMS: &str = r#"
ALTER TABLE items RENAME TO items_old;

CREATE TABLE items (
    item_id       INTEGER PRIMARY KEY AUTOINCREMENT,
    root_id       INTEGER NOT NULL,
    item_path     TEXT NOT NULL,
    item_name     TEXT NOT NULL,
    item_type     INTEGER NOT NULL,
    has_validator INTEGER NOT NULL DEFAULT 0,
    FOREIGN KEY (root_id) REFERENCES roots(root_id),
    UNIQUE (root_id, item_path, item_type)
);

INSERT INTO items (item_id, root_id, item_path, item_name, item_type, has_validator)
SELECT item_id, root_id, item_path, item_name, item_type,
    CASE WHEN item_type = 0 AND (
        LOWER(SUBSTR(item_name, -5)) IN ('.flac', '.jpeg', '.tiff')
        OR LOWER(SUBSTR(item_name, -4)) IN ('.jpg', '.png', '.gif', '.bmp', '.pdf')
    ) THEN 1 ELSE 0 END
FROM items_old;

DROP TABLE items_old;

CREATE INDEX idx_items_root_path ON items (root_id, item_path COLLATE natural_path, item_type);
CREATE INDEX idx_items_root_name ON items (root_id, item_name COLLATE natural_path);
"#;

const P1_REBUILD_SCANS: &str = r#"
ALTER TABLE scans RENAME TO scans_old;

CREATE TABLE scans (
    scan_id INTEGER PRIMARY KEY AUTOINCREMENT,
    root_id INTEGER NOT NULL,
    schedule_id INTEGER DEFAULT NULL,
    started_at INTEGER NOT NULL,
    ended_at INTEGER DEFAULT NULL,
    was_restarted BOOLEAN NOT NULL DEFAULT 0,
    state INTEGER NOT NULL,
    is_hash BOOLEAN NOT NULL,
    hash_all BOOLEAN NOT NULL,
    is_val BOOLEAN NOT NULL,
    file_count INTEGER DEFAULT NULL,
    folder_count INTEGER DEFAULT NULL,
    total_size INTEGER DEFAULT NULL,
    alert_count INTEGER DEFAULT NULL,
    add_count INTEGER DEFAULT NULL,
    modify_count INTEGER DEFAULT NULL,
    delete_count INTEGER DEFAULT NULL,
    val_unknown_count INTEGER DEFAULT NULL,
    val_valid_count INTEGER DEFAULT NULL,
    val_invalid_count INTEGER DEFAULT NULL,
    val_no_validator_count INTEGER DEFAULT NULL,
    hash_unknown_count INTEGER DEFAULT NULL,
    hash_baseline_count INTEGER DEFAULT NULL,
    hash_suspect_count INTEGER DEFAULT NULL,
    error TEXT DEFAULT NULL,
    FOREIGN KEY (root_id) REFERENCES roots(root_id),
    FOREIGN KEY (schedule_id) REFERENCES scan_schedules(schedule_id)
);

INSERT INTO scans (
    scan_id, root_id, schedule_id, started_at, ended_at, was_restarted, state,
    is_hash, hash_all, is_val,
    file_count, folder_count, total_size, alert_count,
    add_count, modify_count, delete_count,
    val_unknown_count, val_valid_count, val_invalid_count, val_no_validator_count,
    hash_unknown_count, hash_baseline_count, hash_suspect_count,
    error
)
SELECT
    scan_id, root_id, schedule_id, started_at, ended_at, was_restarted, state,
    is_hash, hash_all, is_val,
    file_count, folder_count, total_size, alert_count,
    add_count, modify_count, delete_count,
    val_unknown_count, val_valid_count, val_invalid_count, val_no_validator_count,
    hash_unknown_count, hash_valid_count, hash_suspect_count,
    error
FROM scans_old;

DROP TABLE scans_old;

CREATE INDEX IF NOT EXISTS idx_scans_root ON scans (root_id);
"#;

const P1_PREPARE_VERSIONS: &str = r#"
ALTER TABLE item_versions RENAME TO item_versions_old;

-- The rename moved indexes to item_versions_old. Drop them so we can recreate.
DROP INDEX IF EXISTS idx_versions_item_scan;
DROP INDEX IF EXISTS idx_versions_first_scan;
"#;

const P1_INDEX_OLD_VERSIONS: &str = r#"
-- Recreate indexes on item_versions_old (needed for Phase 2 reads)
CREATE INDEX idx_versions_old_item_scan  ON item_versions_old (item_id, first_scan_id DESC);
CREATE INDEX idx_versions_old_first_scan ON item_versions_old (first_scan_id);
"#;

const P1_CREATE_NEW_TABLES: &str = r#"
-- New item_versions with root_id + val columns
CREATE TABLE item_versions (
    version_id      INTEGER PRIMARY KEY AUTOINCREMENT,
    item_id         INTEGER NOT NULL,
    root_id         INTEGER NOT NULL,
    first_scan_id   INTEGER NOT NULL,
    last_scan_id    INTEGER NOT NULL,
    is_added        BOOLEAN NOT NULL DEFAULT 0,
    is_deleted      BOOLEAN NOT NULL DEFAULT 0,
    access          INTEGER NOT NULL DEFAULT 0,
    mod_date        INTEGER,
    size            INTEGER,
    add_count       INTEGER,
    modify_count    INTEGER,
    delete_count    INTEGER,
    unchanged_count INTEGER,
    val_scan_id     INTEGER,
    val_state       INTEGER,
    val_error       TEXT,
    FOREIGN KEY (item_id)       REFERENCES items(item_id),
    FOREIGN KEY (root_id)       REFERENCES roots(root_id),
    FOREIGN KEY (first_scan_id) REFERENCES scans(scan_id),
    FOREIGN KEY (last_scan_id)  REFERENCES scans(scan_id)
);

-- Indexes on the new table (needed for Phase 2 writes/Phase 3 reads)
CREATE INDEX idx_versions_item_scan      ON item_versions (item_id, first_scan_id DESC);
CREATE INDEX idx_versions_first_scan     ON item_versions (first_scan_id);
CREATE INDEX idx_versions_root_lastscan  ON item_versions (root_id, last_scan_id);

-- Hash versions table — created AFTER new item_versions so FK binds to the
-- new table, not item_versions_old (SQLite FKs follow table renames).
CREATE TABLE hash_versions (
    item_id          INTEGER NOT NULL,
    item_version_id  INTEGER NOT NULL,
    first_scan_id    INTEGER NOT NULL,
    last_scan_id     INTEGER NOT NULL,
    file_hash        BLOB NOT NULL,
    hash_state       INTEGER NOT NULL,
    PRIMARY KEY (item_id, item_version_id, first_scan_id),
    FOREIGN KEY (item_id) REFERENCES items(item_id),
    FOREIGN KEY (item_version_id) REFERENCES item_versions(version_id),
    FOREIGN KEY (first_scan_id) REFERENCES scans(scan_id),
    FOREIGN KEY (last_scan_id) REFERENCES scans(scan_id)
) WITHOUT ROWID;

CREATE INDEX idx_hash_versions_item_version ON hash_versions (item_version_id);

DROP TABLE scan_undo_log;

CREATE TABLE scan_undo_log (
    log_type         INTEGER NOT NULL,
    ref_id1          INTEGER NOT NULL,
    ref_id2          INTEGER NOT NULL DEFAULT 0,
    old_last_scan_id INTEGER NOT NULL,
    PRIMARY KEY (log_type, ref_id1, ref_id2)
) WITHOUT ROWID;

-- Convert validate_mode=2 (All) to 1 (New), then rename column to is_val (boolean)
UPDATE scan_schedules SET validate_mode = 1 WHERE validate_mode = 2;
ALTER TABLE scan_schedules RENAME COLUMN validate_mode TO is_val;
"#;

fn phase1_ddl_setup(conn: &Connection) -> Result<(), FsPulseError> {
    migration_info("  Phase 1/4: Rebuilding tables...");
    let t = Instant::now();
    conn.execute_batch("BEGIN IMMEDIATE;")?;

    rollback_in_progress_scans(conn)?;

    migration_info("    Rebuilding items table...");
    conn.execute_batch(P1_REBUILD_ITEMS)?;
    migration_info(&format!("    Items rebuilt ({:.1}s)", t.elapsed().as_secs_f64()));

    migration_info("    Rebuilding scans table...");
    conn.execute_batch(P1_REBUILD_SCANS)?;
    migration_info(&format!("    Scans rebuilt ({:.1}s)", t.elapsed().as_secs_f64()));

    migration_info("    Renaming item_versions to item_versions_old...");
    conn.execute_batch(P1_PREPARE_VERSIONS)?;
    migration_info(&format!("    Renamed ({:.1}s)", t.elapsed().as_secs_f64()));

    migration_info("    Indexing item_versions_old...");
    conn.execute_batch(P1_INDEX_OLD_VERSIONS)?;
    migration_info(&format!("    Indexed ({:.1}s)", t.elapsed().as_secs_f64()));

    migration_info("    Creating new tables (item_versions, hash_versions, scan_undo_log)...");
    conn.execute_batch(P1_CREATE_NEW_TABLES)?;

    conn.execute_batch("COMMIT;")?;
    migration_info(&format!("  Phase 1/4 complete ({:.1}s)", t.elapsed().as_secs_f64()));
    Ok(())
}

// ── Phase 2 ──────────────────────────────────────────────────────────────────

/// Group an item's chronologically-ordered versions into (real, phantom_cluster) pairs.
///
/// A version is a phantom if all three hold:
///   1. is_added = 0, is_deleted = 0
///   2. The last real (non-phantom) predecessor has is_deleted = 0
///   3. access, mod_date, size all match that predecessor
///
/// Each group's effective_last_scan = max(last_scan_id across real + all phantoms).
fn build_groups(versions: &[OldVersion]) -> Vec<Group> {
    let n = versions.len();
    let mut is_phantom = vec![false; n];
    let mut last_real: Option<usize> = None;

    for i in 0..n {
        let v = &versions[i];
        let phantom = match last_real {
            None => false,
            Some(ri) => {
                let r = &versions[ri];
                v.is_added == 0
                    && v.is_deleted == 0
                    && r.is_deleted == 0
                    && v.access == r.access
                    && v.mod_date == r.mod_date
                    && v.size == r.size
            }
        };
        is_phantom[i] = phantom;
        if !phantom {
            last_real = Some(i);
        }
    }

    let mut groups = Vec::new();
    let mut i = 0;
    while i < n {
        if !is_phantom[i] {
            let mut max_ls = versions[i].last_scan_id;
            let mut phantom_indices = Vec::new();
            let mut j = i + 1;
            while j < n && is_phantom[j] {
                if versions[j].last_scan_id > max_ls {
                    max_ls = versions[j].last_scan_id;
                }
                phantom_indices.push(j);
                j += 1;
            }
            groups.push(Group { real_idx: i, phantom_indices, effective_last_scan: max_ls });
            i = j;
        } else {
            i += 1; // unreachable in practice; guarded by the while condition above
        }
    }
    groups
}

/// Extract the hash_versions row for one group, if the group has hash data.
///
/// Returns (item_version_id, first_scan_id, last_scan_id, file_hash, hash_state).
/// item_version_id = the real version's version_id.
/// first_scan_id = MIN(last_hash_scan) across real + phantoms with hash (baseline).
/// last_scan_id  = group.effective_last_scan.
/// file_hash from the version with MIN(last_hash_scan) (baseline hash).
/// hash_state from the version with MAX(last_hash_scan) (most recent observation).
fn extract_hash(
    versions: &[OldVersion],
    group: &Group,
) -> Option<(i64, i64, i64, Vec<u8>, i64)> {
    let candidates: Vec<&OldVersion> = std::iter::once(group.real_idx)
        .chain(group.phantom_indices.iter().copied())
        .map(|idx| &versions[idx])
        .filter(|v| v.file_hash.is_some() && v.last_hash_scan.is_some())
        .collect();

    if candidates.is_empty() {
        return None;
    }

    let first = candidates.iter().min_by_key(|v| v.last_hash_scan.unwrap())?;
    let last  = candidates.iter().max_by_key(|v| v.last_hash_scan.unwrap())?;

    let real_version_id = versions[group.real_idx].version_id;

    Some((
        real_version_id,                     // item_version_id
        first.last_hash_scan.unwrap(),       // first_scan_id
        group.effective_last_scan,           // last_scan_id
        first.file_hash.clone().unwrap(),    // hash value (baseline)
        last.hash_state.unwrap_or(1),        // most recent hash_state
    ))
}

/// Extract val data for one group, to be written onto the item_version row.
///
/// Returns (val_scan_id, val_state, val_error).
/// val_scan_id = MAX(last_val_scan) across real + phantoms with meaningful val.
/// val_state/val_error from the version with MAX(last_val_scan).
/// val_state 0 (not applicable) and 3 (pending) are skipped.
fn extract_val(
    versions: &[OldVersion],
    group: &Group,
) -> Option<(i64, i64, Option<String>)> {
    let is_meaningful = |v: &&OldVersion| {
        matches!(v.val_state, Some(s) if s != 0 && s != 3) && v.last_val_scan.is_some()
    };

    let candidates: Vec<&OldVersion> = std::iter::once(group.real_idx)
        .chain(group.phantom_indices.iter().copied())
        .map(|idx| &versions[idx])
        .filter(is_meaningful)
        .collect();

    if candidates.is_empty() {
        return None;
    }

    let last = candidates.iter().max_by_key(|v| v.last_val_scan.unwrap())?;

    Some((
        last.last_val_scan.unwrap(),  // val_scan_id
        last.val_state.unwrap(),      // val_state
        last.val_error.clone(),       // val_error
    ))
}

/// Migrate one item: insert versions + hash rows, then delete from old table.
///
/// For files: detect and remove phantom versions (hash/val-only changes), extract
/// hash data into hash_versions, write val data onto item_version rows.
/// For folders: copy all versions as-is. Counts will be corrected in Phase 3.
///
/// Returns (total_versions_in_old, phantom_count).
fn migrate_one_item(conn: &Connection, item_id: i64, root_id: i64, is_folder: bool) -> Result<(i64, i64), FsPulseError> {
    let mut stmt = conn.prepare_cached("
        SELECT version_id, first_scan_id, last_scan_id,
               is_added, is_deleted, access, mod_date, size,
               add_count, modify_count, delete_count, unchanged_count,
               file_hash, last_hash_scan, hash_state,
               val_state, val_error, last_val_scan
        FROM item_versions_old
        WHERE item_id = ?
        ORDER BY first_scan_id
    ")?;
    let versions: Vec<OldVersion> = stmt
        .query_map([item_id], |row| {
            Ok(OldVersion {
                version_id:      row.get(0)?,
                first_scan_id:   row.get(1)?,
                last_scan_id:    row.get(2)?,
                is_added:        row.get(3)?,
                is_deleted:      row.get(4)?,
                access:          row.get(5)?,
                mod_date:        row.get(6)?,
                size:            row.get(7)?,
                add_count:       row.get(8)?,
                modify_count:    row.get(9)?,
                delete_count:    row.get(10)?,
                unchanged_count: row.get(11)?,
                file_hash:       row.get(12)?,
                last_hash_scan:  row.get(13)?,
                hash_state:      row.get(14)?,
                val_state:       row.get(15)?,
                val_error:       row.get(16)?,
                last_val_scan:   row.get(17)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;

    let total = versions.len() as i64;
    if total == 0 {
        return Ok((0, 0));
    }

    let phantom_count;

    if is_folder {
        // ── Folders: copy all versions as-is ─────────────────────────────────
        // Folder counts may be inflated by file phantoms; Phase 3 will recompute.
        // No phantom detection — folders never had hash/val computed directly,
        // so phantom logic doesn't apply. Case B versions (descendant count
        // changes) have identical metadata to predecessors and would be
        // incorrectly flagged as phantoms.
        let mut iv = conn.prepare_cached("
            INSERT INTO item_versions (
                version_id, item_id, root_id, first_scan_id, last_scan_id,
                is_added, is_deleted, access, mod_date, size,
                add_count, modify_count, delete_count, unchanged_count
            ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14)
        ")?;
        for v in &versions {
            iv.execute(rusqlite::params![
                v.version_id, item_id, root_id, v.first_scan_id, v.last_scan_id,
                v.is_added, v.is_deleted, v.access, v.mod_date, v.size,
                v.add_count, v.modify_count, v.delete_count, v.unchanged_count,
            ])?;
        }
        phantom_count = 0;
    } else {
        // ── Files: phantom detection + hash/val extraction ───────────────────
        let groups = build_groups(&versions);
        phantom_count = total - groups.len() as i64;

        {
            let mut iv = conn.prepare_cached("
                INSERT INTO item_versions (
                    version_id, item_id, root_id, first_scan_id, last_scan_id,
                    is_added, is_deleted, access, mod_date, size,
                    add_count, modify_count, delete_count, unchanged_count,
                    val_scan_id, val_state, val_error
                ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17)
            ")?;
            for group in &groups {
                let r = &versions[group.real_idx];
                let val = extract_val(&versions, group);
                let (val_scan_id, val_state, val_error) = match val {
                    Some((scan_id, state, error)) => (Some(scan_id), Some(state), error),
                    None => (None, None, None),
                };
                iv.execute(rusqlite::params![
                    r.version_id, item_id, root_id, r.first_scan_id, group.effective_last_scan,
                    r.is_added, r.is_deleted, r.access, r.mod_date, r.size,
                    r.add_count, r.modify_count, r.delete_count, r.unchanged_count,
                    val_scan_id, val_state, val_error,
                ])?;
            }
        }

        {
            let mut hv = conn.prepare_cached("
                INSERT OR IGNORE INTO hash_versions
                    (item_id, item_version_id, first_scan_id, last_scan_id, file_hash, hash_state)
                VALUES (?1,?2,?3,?4,?5,?6)
            ")?;
            for group in &groups {
                if let Some((version_id, first, last, hash, state)) = extract_hash(&versions, group) {
                    hv.execute(rusqlite::params![item_id, version_id, first, last, hash, state])?;
                }
            }
        }
    }

    // ── Lift and shift: remove this item's rows from the old table ───────────
    conn.execute("DELETE FROM item_versions_old WHERE item_id = ?", [item_id])?;

    Ok((total, phantom_count))
}

fn phase2_migrate_data(conn: &Connection) -> Result<(), FsPulseError> {
    // Count what remains to be processed (accounts for partial previous runs)
    let items_remaining: i64 = conn.query_row(
        "SELECT COUNT(DISTINCT item_id) FROM item_versions_old",
        [],
        |r| r.get(0),
    )?;
    let ivs_remaining: i64 = conn.query_row(
        "SELECT COUNT(*) FROM item_versions_old",
        [],
        |r| r.get(0),
    )?;

    if items_remaining == 0 {
        migration_info("  Phase 2/4: Already complete, skipping.");
    } else {
        migration_info(&format!(
            "  Phase 2/4: {} items / {} versions remaining...",
            items_remaining, ivs_remaining
        ));

        let total_start = Instant::now();
        let mut items_done: i64 = 0;
        let mut versions_done: i64 = 0;
        let mut phantoms_removed: i64 = 0;
        let report_interval = (items_remaining / 10).max(BATCH_SIZE);

        loop {
            // Select next batch of distinct item_ids still present in item_versions_old,
            // along with item_type so we can branch on file vs folder.
            let mut id_stmt = conn.prepare_cached(
                "SELECT DISTINCT ivo.item_id, i.item_type, i.root_id
                 FROM item_versions_old ivo
                 JOIN items i ON i.item_id = ivo.item_id
                 ORDER BY ivo.item_id LIMIT ?",
            )?;
            let batch: Vec<(i64, i64, i64)> = id_stmt
                .query_map([BATCH_SIZE], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
                .collect::<Result<Vec<_>, _>>()?;

            if batch.is_empty() {
                break;
            }

            // Each batch: insert new rows + delete old rows atomically.
            // A crash here rolls back the entire batch; those items stay in
            // item_versions_old and are retried on restart.
            conn.execute_batch("BEGIN IMMEDIATE;")?;
            for &(item_id, item_type, root_id) in &batch {
                let (ver_count, phantom_count) = migrate_one_item(conn, item_id, root_id, item_type == 1)?;
                versions_done += ver_count;
                phantoms_removed += phantom_count;
            }
            conn.execute_batch("COMMIT;")?;

            items_done += batch.len() as i64;

            if items_done % report_interval < BATCH_SIZE || items_done >= items_remaining {
                migration_info(&format!(
                    "    {}/{} items  |  {} versions  |  {} pruned  |  {:.0}s",
                    items_done, items_remaining,
                    versions_done, phantoms_removed,
                    total_start.elapsed().as_secs_f64()
                ));
            }
        }

        migration_info(&format!(
            "  Phase 2/4 complete: {} kept, {} pruned ({:.1}%) in {:.1}s",
            versions_done - phantoms_removed,
            phantoms_removed,
            if ivs_remaining > 0 {
                phantoms_removed as f64 / ivs_remaining as f64 * 100.0
            } else {
                0.0
            },
            total_start.elapsed().as_secs_f64()
        ));
    }

    Ok(())
}

// ── Phase 3 — Count recomputation ────────────────────────────────────────────

/// Find all folder versions with first_scan_id = scan_id for the given root.
fn p3_folder_versions_at_scan(
    conn: &Connection,
    root_id: i64,
    scan_id: i64,
) -> Result<Vec<FolderVersionAtScan>, FsPulseError> {
    let mut stmt = conn.prepare(
        "SELECT iv.version_id, iv.item_id, iv.is_added, iv.is_deleted,
                iv.access, iv.mod_date, iv.size, iv.last_scan_id
         FROM item_versions iv
         JOIN items i ON i.item_id = iv.item_id
         WHERE i.root_id = ? AND i.item_type = 1 AND iv.first_scan_id = ?",
    )?;
    let rows = stmt
        .query_map(params![root_id, scan_id], |row| {
            Ok(FolderVersionAtScan {
                version_id:   row.get(0)?,
                item_id:      row.get(1)?,
                is_added:     row.get(2)?,
                is_deleted:   row.get(3)?,
                access:       row.get(4)?,
                mod_date:     row.get(5)?,
                size:         row.get(6)?,
                last_scan_id: row.get(7)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// Query immediate folder children of parent_path that are alive at scan_id
/// (or deleted AT scan_id, so we can recurse into deleted subtrees).
fn p3_immediate_dir_children(
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

/// Count direct children (files and folders) of parent_path that changed in this scan,
/// classified as add/modify/delete.
fn p3_direct_change_counts(
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

#[allow(clippy::type_complexity)]
/// Count alive direct children of parent_path at scan_id for population and
/// integrity accumulation. Returns counts that feed into ScanCounts.
///
/// Population: alive files and alive folders (counted separately).
/// Integrity: alive files — val from item_versions columns, hash from hash_versions.
fn p3_direct_population_counts(
    conn: &Connection,
    root_id: i64,
    parent_path: &str,
    scan_id: i64,
) -> Result<(i64, i64, i64, i64, i64, i64, i64, i64, i64), FsPulseError> {
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

    // Query alive direct children (immediate, not recursive).
    // For files: count population and integrity states.
    //   - Val comes from item_versions columns directly (no join needed).
    //   - Hash comes from hash_versions joined via item_version_id.
    // For folders: count population only (integrity doesn't apply).
    let sql = format!(
        "SELECT
            -- Population
            COALESCE(SUM(CASE WHEN i.item_type = 0 THEN 1 ELSE 0 END), 0),
            COALESCE(SUM(CASE WHEN i.item_type = 1 THEN 1 ELSE 0 END), 0),
            -- Hash integrity (files only)
            COALESCE(SUM(CASE WHEN i.item_type = 0 AND hv.hash_state IS NULL THEN 1 ELSE 0 END), 0),
            COALESCE(SUM(CASE WHEN i.item_type = 0 AND hv.hash_state = 1 THEN 1 ELSE 0 END), 0),
            COALESCE(SUM(CASE WHEN i.item_type = 0 AND hv.hash_state = 2 THEN 1 ELSE 0 END), 0),
            -- Val integrity (files only, from item_versions columns)
            COALESCE(SUM(CASE WHEN i.item_type = 0 AND i.has_validator = 1 AND iv.val_state IS NULL THEN 1 ELSE 0 END), 0),
            COALESCE(SUM(CASE WHEN i.item_type = 0 AND iv.val_state = 1 THEN 1 ELSE 0 END), 0),
            COALESCE(SUM(CASE WHEN i.item_type = 0 AND iv.val_state = 2 THEN 1 ELSE 0 END), 0),
            COALESCE(SUM(CASE WHEN i.item_type = 0 AND i.has_validator = 0 THEN 1 ELSE 0 END), 0)
         FROM items i
         JOIN item_versions iv ON iv.item_id = i.item_id
         LEFT JOIN hash_versions hv ON hv.item_id = i.item_id
             AND hv.item_version_id = iv.version_id
             AND hv.first_scan_id = (
                 SELECT MAX(first_scan_id) FROM hash_versions
                 WHERE item_id = i.item_id AND item_version_id = iv.version_id AND first_scan_id <= ?2
             )
         WHERE i.root_id = ?1
           AND iv.last_scan_id >= ?2
           AND iv.is_deleted = 0
           AND iv.first_scan_id = (
               SELECT MAX(first_scan_id) FROM item_versions
               WHERE item_id = i.item_id AND first_scan_id <= ?2
           )
           AND i.item_path >= ?3
           AND i.item_path < ?4
           AND i.item_path != ?5
           AND SUBSTR(i.item_path, LENGTH(?3) + 1) NOT LIKE '%{}%'",
        MAIN_SEPARATOR_STR
    );

    let mut stmt = conn.prepare(&sql)?;
    let result = stmt.query_row(
        params![root_id, scan_id, &path_prefix, &path_upper, parent_path],
        |row| Ok((
            row.get(0)?, row.get(1)?,
            row.get(2)?, row.get(3)?, row.get(4)?,
            row.get(5)?, row.get(6)?, row.get(7)?, row.get(8)?,
        )),
    )?;

    Ok(result)
}

/// Look up the item_id for a folder by its path.
fn p3_lookup_folder_item_id(
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

/// Query the total alive descendant count from a folder's version just before scan_id.
///
/// Returns add_count + modify_count + unchanged_count from that version.
/// Returns 0 if no previous version exists (first scan or new folder).
///
/// Used to derive: unchanged = prev_alive - mods - dels
fn p3_prev_alive(
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

/// Recursive depth-first walk of the folder tree, computing descendant change counts
/// and accumulating scan-level population/integrity counts.
///
/// Returns cumulative (adds, mods, dels) for all descendants under parent_path.
/// Appends a FolderCountWrite entry for each folder with non-zero change counts.
/// Accumulates population and integrity counts into scan_counts.
///
/// unchanged is derived per-folder at write time:
///   unchanged = prev_alive - mods - dels
fn p3_walk_folder_counts(
    conn: &Connection,
    root_id: i64,
    scan_id: i64,
    parent_path: &str,
    writes: &mut Vec<FolderCountWrite>,
    scan_counts: &mut ScanCounts,
) -> Result<(i64, i64, i64), FsPulseError> {
    let mut adds = 0i64;
    let mut mods = 0i64;
    let mut dels = 0i64;

    // 1. Get immediate folder children alive (or deleted) at this scan
    let dir_children = p3_immediate_dir_children(conn, root_id, parent_path, scan_id)?;

    // 2. Recurse into each folder child
    for (_child_id, child_path) in &dir_children {
        let (sa, sm, sd) = p3_walk_folder_counts(conn, root_id, scan_id, child_path, writes, scan_counts)?;
        adds += sa;
        mods += sm;
        dels += sd;
    }

    // 3. Count direct children (files and folders) that changed in this scan
    let (da, dm, dd) = p3_direct_change_counts(conn, root_id, parent_path, scan_id)?;
    adds += da;
    mods += dm;
    dels += dd;

    // 4. Accumulate population and integrity counts for direct children at this level
    let (files, folders, hu, hv, hs, vu, vv, vi, vn) =
        p3_direct_population_counts(conn, root_id, parent_path, scan_id)?;
    scan_counts.file_count += files;
    scan_counts.folder_count += folders;
    scan_counts.hash_unknown += hu;
    scan_counts.hash_baseline += hv;
    scan_counts.hash_suspect += hs;
    scan_counts.val_unknown += vu;
    scan_counts.val_valid += vv;
    scan_counts.val_invalid += vi;
    scan_counts.val_no_validator += vn;

    // 5. Record write if any descendant changed
    if adds > 0 || mods > 0 || dels > 0 {
        if let Some(folder_item_id) = p3_lookup_folder_item_id(conn, root_id, parent_path)? {
            let prev_alive = p3_prev_alive(conn, folder_item_id, scan_id)?;
            let unchanged = prev_alive - mods - dels;

            writes.push(FolderCountWrite {
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

/// Update counts on an existing folder version, or create a new one (Case B split).
///
/// Case A: A version exists with first_scan_id = scan_id — update its counts.
/// Case B: No version starts at scan_id, but a spanning version covers it.
///         Truncate the spanning version to end at scan_id - 1, then insert a
///         new version at scan_id with the same metadata and the computed counts.
fn p3_update_folder_counts(
    conn: &Connection,
    root_id: i64,
    scan_id: i64,
    w: &FolderCountWrite,
) -> Result<(), FsPulseError> {
    // Case A: version starts at this scan
    let updated = conn.execute(
        "UPDATE item_versions
         SET add_count = ?, modify_count = ?, delete_count = ?, unchanged_count = ?
         WHERE item_id = ? AND first_scan_id = ?",
        params![w.adds, w.mods, w.dels, w.unchanged, w.folder_item_id, scan_id],
    )?;
    if updated > 0 {
        return Ok(());
    }

    // Case B: find the spanning version (first_scan_id < scan_id AND last_scan_id >= scan_id)
    #[allow(clippy::type_complexity)]
    let spanning: Option<(i64, i64, i64, i64, Option<i64>, Option<i64>)> = conn
        .query_row(
            "SELECT version_id, is_added, is_deleted, access, mod_date, size
             FROM item_versions
             WHERE item_id = ?1 AND first_scan_id < ?2 AND last_scan_id >= ?2
             ORDER BY first_scan_id DESC LIMIT 1",
            params![w.folder_item_id, scan_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?, row.get(5)?)),
        )
        .optional()?;

    match spanning {
        Some((span_vid, _is_added, is_deleted, access, mod_date, size)) => {
            // Get the spanning version's last_scan_id before we truncate it
            let span_last: i64 = conn.query_row(
                "SELECT last_scan_id FROM item_versions WHERE version_id = ?",
                [span_vid],
                |row| row.get(0),
            )?;

            // Truncate spanning version to end just before this scan
            conn.execute(
                "UPDATE item_versions SET last_scan_id = ? WHERE version_id = ?",
                params![scan_id - 1, span_vid],
            )?;

            // Insert new version at scan_id with same metadata but computed counts.
            // is_added is always 0 (this is a continuation); is_deleted is carried
            // forward from the spanning version per spec.
            conn.execute(
                "INSERT INTO item_versions (item_id, root_id, first_scan_id, last_scan_id,
                    is_added, is_deleted, access, mod_date, size,
                    add_count, modify_count, delete_count, unchanged_count)
                 VALUES (?1, ?2, ?3, ?4, 0, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
                params![
                    w.folder_item_id, root_id, scan_id, span_last,
                    is_deleted, access, mod_date, size,
                    w.adds, w.mods, w.dels, w.unchanged
                ],
            )?;
        }
        None => {
            // No spanning version found — item may have been deleted before this scan
        }
    }
    Ok(())
}

/// Clean up a folder version that has zero descendant changes after recomputation.
///
/// If the version represents a meaningful state change (is_added, is_deleted, or
/// metadata differs from predecessor), keep it but set counts to reflect zero
/// structural changes: adds=0, mods=0, dels=0, unchanged=prev_alive.
/// If metadata matches predecessor, the version only existed because of hash/val
/// count changes — delete it and extend the predecessor's last_scan_id.
///
/// Returns true if the version was deleted.
fn p3_cleanup_zero_change_folder(
    conn: &Connection,
    fv: &FolderVersionAtScan,
    scan_id: i64,
) -> Result<bool, FsPulseError> {
    // Compute prev_alive so we can set unchanged_count correctly on kept versions
    let prev_alive = p3_prev_alive(conn, fv.item_id, scan_id)?;

    // Versions representing adds/deletes are always meaningful
    if fv.is_added != 0 || fv.is_deleted != 0 {
        conn.execute(
            "UPDATE item_versions
             SET add_count = 0, modify_count = 0, delete_count = 0, unchanged_count = ?
             WHERE version_id = ?",
            params![prev_alive, fv.version_id],
        )?;
        return Ok(false);
    }

    // Look up predecessor to check if metadata matches
    let pred: Option<(i64, i64, Option<i64>, Option<i64>)> = conn
        .query_row(
            "SELECT version_id, access, mod_date, size
             FROM item_versions
             WHERE item_id = ? AND first_scan_id < (
                 SELECT first_scan_id FROM item_versions WHERE version_id = ?
             )
             ORDER BY first_scan_id DESC LIMIT 1",
            params![fv.item_id, fv.version_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .optional()?;

    match pred {
        Some((pred_vid, pred_access, pred_mod_date, pred_size))
            if pred_access == fv.access
                && pred_mod_date == fv.mod_date
                && pred_size == fv.size =>
        {
            // Metadata matches predecessor: version is unnecessary.
            // Extend predecessor's last_scan_id and delete this version.
            conn.execute(
                "UPDATE item_versions SET last_scan_id = ? WHERE version_id = ?",
                params![fv.last_scan_id, pred_vid],
            )?;
            conn.execute(
                "DELETE FROM item_versions WHERE version_id = ?",
                [fv.version_id],
            )?;
            Ok(true)
        }
        _ => {
            // Metadata differs from predecessor (or no predecessor): keep with
            // zero structural changes but correct unchanged count
            conn.execute(
                "UPDATE item_versions
                 SET add_count = 0, modify_count = 0, delete_count = 0, unchanged_count = ?
                 WHERE version_id = ?",
                params![prev_alive, fv.version_id],
            )?;
            Ok(false)
        }
    }
}

/// Fix overlapping folder versions left by the old Case B logic.
///
/// The old scanner created phantom folder versions when hash/val analysis first ran
/// on descendant files. These phantoms overlap temporally with earlier versions.
///
/// For each item with overlaps, load ALL its versions sorted by first_scan_id and
/// chain them: each version's last_scan_id is set to next.first_scan_id - 1, and
/// the final version gets the maximum last_scan_id from the original set. This
/// ensures no overlaps AND no gaps.
fn cleanup_overlapping_versions(conn: &Connection) -> Result<(), FsPulseError> {
    // Find all items that have overlapping versions
    let mut stmt = conn.prepare(
        "SELECT DISTINCT a.item_id
         FROM item_versions a
         JOIN item_versions b ON a.item_id = b.item_id
           AND a.version_id < b.version_id
           AND b.first_scan_id <= a.last_scan_id
           AND a.first_scan_id <= b.last_scan_id
         ORDER BY a.item_id"
    )?;
    let item_ids: Vec<i64> = stmt
        .query_map([], |row| row.get(0))?
        .collect::<Result<Vec<_>, _>>()?;

    if item_ids.is_empty() {
        return Ok(());
    }

    let mut total_fixes = 0;

    for &item_id in &item_ids {
        // Load ALL versions for this item, sorted by first_scan_id
        let mut vstmt = conn.prepare(
            "SELECT version_id, first_scan_id, last_scan_id
             FROM item_versions
             WHERE item_id = ?
             ORDER BY first_scan_id"
        )?;
        let versions: Vec<(i64, i64, i64)> = vstmt
            .query_map([item_id], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))?
            .collect::<Result<Vec<_>, _>>()?;

        if versions.len() < 2 {
            continue;
        }

        // Find the maximum last_scan_id across all versions
        let max_last = versions.iter().map(|v| v.2).max().unwrap();

        // Chain versions: each gets last_scan_id = next.first_scan_id - 1,
        // final version gets max_last
        Database::immediate_transaction(conn, |c| {
            for i in 0..versions.len() {
                let (vid, _first, old_last) = versions[i];
                let new_last = if i + 1 < versions.len() {
                    versions[i + 1].1 - 1 // next.first_scan_id - 1
                } else {
                    max_last
                };

                if new_last != old_last {
                    c.execute(
                        "UPDATE item_versions SET last_scan_id = ? WHERE version_id = ?",
                        params![new_last, vid],
                    )?;
                    total_fixes += 1;
                }
            }
            Ok(())
        })?;
    }

    // Delete any versions that ended up with first_scan_id > last_scan_id
    // (pre-existing corrupt data where last_scan_id was already invalid)
    let inverted_deleted: usize = conn.execute(
        "DELETE FROM item_versions
         WHERE version_id IN (
             SELECT iv.version_id FROM item_versions iv
             JOIN items i ON i.item_id = iv.item_id
             WHERE i.item_type = 1 AND iv.first_scan_id > iv.last_scan_id
         )",
        [],
    )?;
    migration_info(&format!(
        "    Overlap cleanup: {} fixes across {} items, {} inverted deleted",
        total_fixes, item_ids.len(), inverted_deleted
    ));
    Ok(())
}

/// Merge adjacent folder versions that are identical in all fields.
///
/// Two versions are "adjacent" if they belong to the same item and no other version
/// falls between them (i.e., first version's last_scan_id + 1 >= second version's
/// first_scan_id, with no intervening scans).
///
/// For each folder item, load all versions ordered by first_scan_id, walk the list,
/// and merge consecutive identical versions by extending the first and deleting the second.
fn p3_merge_adjacent_duplicates(conn: &Connection) -> Result<i64, FsPulseError> {

    // Get all folder items that have more than one version
    let mut stmt = conn.prepare(
        "SELECT DISTINCT iv.item_id
         FROM item_versions iv
         JOIN items i ON i.item_id = iv.item_id
         WHERE i.item_type = 1
         GROUP BY iv.item_id
         HAVING COUNT(*) > 1"
    )?;
    let folder_ids: Vec<i64> = stmt
        .query_map([], |row| row.get(0))?
        .collect::<Result<Vec<_>, _>>()?;

    let mut total_merged: i64 = 0;

    for folder_id in &folder_ids {
        // Load all versions for this folder, ordered by first_scan_id
        let mut vstmt = conn.prepare(
            "SELECT version_id, first_scan_id, last_scan_id,
                    is_added, is_deleted, access, mod_date, size,
                    add_count, modify_count, delete_count, unchanged_count
             FROM item_versions
             WHERE item_id = ?
             ORDER BY first_scan_id"
        )?;

        #[allow(clippy::type_complexity)]
        let versions: Vec<(i64, i64, i64, i64, i64, i64, Option<i64>, Option<i64>, i64, i64, i64, i64)> = vstmt
            .query_map([folder_id], |row| {
                Ok((
                    row.get(0)?, row.get(1)?, row.get(2)?,
                    row.get(3)?, row.get(4)?, row.get(5)?,
                    row.get(6)?, row.get(7)?, row.get(8)?,
                    row.get(9)?, row.get(10)?, row.get(11)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;

        if versions.len() < 2 {
            continue;
        }

        // Walk the list and merge adjacent identical versions
        let mut i = 0;
        while i < versions.len() - 1 {
            let cur = &versions[i];
            let next = &versions[i + 1];

            // Check if all fields match (except version_id, first_scan_id, last_scan_id)
            let same = cur.3 == next.3     // is_added
                && cur.4 == next.4         // is_deleted
                && cur.5 == next.5         // access
                && cur.6 == next.6         // mod_date
                && cur.7 == next.7         // size
                && cur.8 == next.8         // add_count
                && cur.9 == next.9         // modify_count
                && cur.10 == next.10       // delete_count
                && cur.11 == next.11;      // unchanged_count

            if same {
                // Extend current's last_scan_id to cover next, delete next
                Database::immediate_transaction(conn, |c| {
                    c.execute(
                        "UPDATE item_versions SET last_scan_id = ? WHERE version_id = ?",
                        params![next.2, cur.0],
                    )?;
                    c.execute(
                        "DELETE FROM item_versions WHERE version_id = ?",
                        [next.0],
                    )?;
                    Ok(())
                })?;
                total_merged += 1;
                // Don't advance i — re-check current against the new next
                // But we need to reload versions since we modified the data
                // For simplicity, break and let the outer loop re-process this folder
                break;
            } else {
                i += 1;
            }
        }
    }

    // If we merged anything, do another pass (merges may create new adjacent pairs)
    // Keep going until no more merges happen
    if total_merged > 0 {
        let mut more = true;
        while more {
            more = false;
            for folder_id in &folder_ids {
                let mut vstmt = conn.prepare(
                    "SELECT version_id, first_scan_id, last_scan_id,
                            is_added, is_deleted, access, mod_date, size,
                            add_count, modify_count, delete_count, unchanged_count
                     FROM item_versions
                     WHERE item_id = ?
                     ORDER BY first_scan_id"
                )?;

                #[allow(clippy::type_complexity)]
                let versions: Vec<(i64, i64, i64, i64, i64, i64, Option<i64>, Option<i64>, i64, i64, i64, i64)> = vstmt
                    .query_map([folder_id], |row| {
                        Ok((
                            row.get(0)?, row.get(1)?, row.get(2)?,
                            row.get(3)?, row.get(4)?, row.get(5)?,
                            row.get(6)?, row.get(7)?, row.get(8)?,
                            row.get(9)?, row.get(10)?, row.get(11)?,
                        ))
                    })?
                    .collect::<Result<Vec<_>, _>>()?;

                if versions.len() < 2 {
                    continue;
                }

                let mut i = 0;
                while i < versions.len() - 1 {
                    let cur = &versions[i];
                    let next = &versions[i + 1];

                    let same = cur.3 == next.3
                        && cur.4 == next.4
                        && cur.5 == next.5
                        && cur.6 == next.6
                        && cur.7 == next.7
                        && cur.8 == next.8
                        && cur.9 == next.9
                        && cur.10 == next.10
                        && cur.11 == next.11;

                    if same {
                        Database::immediate_transaction(conn, |c| {
                            c.execute(
                                "UPDATE item_versions SET last_scan_id = ? WHERE version_id = ?",
                                params![next.2, cur.0],
                            )?;
                            c.execute(
                                "DELETE FROM item_versions WHERE version_id = ?",
                                [next.0],
                            )?;
                            Ok(())
                        })?;
                        total_merged += 1;
                        more = true;
                        break;
                    } else {
                        i += 1;
                    }
                }
            }
        }
    }

    if total_merged > 0 {
        migration_info(&format!("    Merged {} adjacent duplicate versions", total_merged));
    }
    Ok(total_merged)
}

fn phase3_recompute_folder_counts(conn: &Connection) -> Result<(), FsPulseError> {
    migration_info("  Phase 3/4: Recomputing folder counts...");
    let t = Instant::now();

    // Clean up overlapping phantom versions before recomputing counts
    cleanup_overlapping_versions(conn)?;

    // Read or create HWM
    let hwm: i64 = match Database::get_meta_value_locked(conn, COUNT_HWM_KEY)? {
        Some(val) => val.parse().unwrap_or(0),
        None => {
            Database::immediate_transaction(conn, |c| {
                Database::set_meta_value_locked(c, COUNT_HWM_KEY, "0")
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
        migration_info("  Phase 3/4: No scans to recompute.");
        return Ok(());
    }

    let mut total_removed: i64 = 0;

    for (completed, (scan_id, root_id, root_path)) in scans.iter().enumerate() {
        // Walk tree, collecting folder count writes and accumulating scan-level
        // counts (changes, population, integrity) in a single recursive pass.
        let mut writes = Vec::new();
        let mut sc = ScanCounts::default();
        let (scan_adds, scan_mods, scan_dels) =
            p3_walk_folder_counts(conn, *root_id, *scan_id, root_path, &mut writes, &mut sc)?;
        sc.adds = scan_adds;
        sc.mods = scan_mods;
        sc.dels = scan_dels;

        // Build set of folder item_ids that have real changes
        let changed_ids: HashSet<i64> = writes.iter().map(|w| w.folder_item_id).collect();

        // Find ALL folder versions at this scan for this root
        let all_folder_versions = p3_folder_versions_at_scan(conn, *root_id, *scan_id)?;

        let mut scan_removed: i64 = 0;

        // Apply updates + cleanup atomically per scan
        Database::immediate_transaction(conn, |c| {
            // Update counts on folders with real structural changes
            for w in &writes {
                p3_update_folder_counts(c, *root_id, *scan_id, w)?;
            }

            // Clean up folder versions with zero changes after recomputation
            for fv in &all_folder_versions {
                if !changed_ids.contains(&fv.item_id)
                    && p3_cleanup_zero_change_folder(c, fv, *scan_id)? {
                    scan_removed += 1;
                }
            }

            // Stamp all scan-level counts from the accumulated walk results
            c.execute(
                "UPDATE scans SET
                    add_count = ?, modify_count = ?, delete_count = ?,
                    file_count = ?, folder_count = ?,
                    val_unknown_count = ?, val_valid_count = ?,
                    val_invalid_count = ?, val_no_validator_count = ?,
                    hash_unknown_count = ?, hash_baseline_count = ?, hash_suspect_count = ?
                 WHERE scan_id = ?",
                params![
                    sc.adds, sc.mods, sc.dels,
                    sc.file_count, sc.folder_count,
                    sc.val_unknown, sc.val_valid,
                    sc.val_invalid, sc.val_no_validator,
                    sc.hash_unknown, sc.hash_baseline, sc.hash_suspect,
                    scan_id,
                ],
            )?;

            Database::set_meta_value_locked(c, COUNT_HWM_KEY, &scan_id.to_string())
        })?;

        total_removed += scan_removed;

        if (completed + 1) % 25 == 0 || completed + 1 == total {
            migration_info(&format!(
                "    {}/{} scans ({:.0}%), {} pruned, {:.1}s",
                completed + 1, total,
                (completed + 1) as f64 / total as f64 * 100.0,
                total_removed, t.elapsed().as_secs_f64()
            ));
        }
    }

    let merged = p3_merge_adjacent_duplicates(conn)?;

    migration_info(&format!(
        "  Phase 3/4 complete: {} scans, {} pruned, {} merged ({:.1}s)",
        total, total_removed, merged, t.elapsed().as_secs_f64()
    ));
    Ok(())
}

// ── Phase 4 ──────────────────────────────────────────────────────────────────

fn phase4_cleanup(conn: &Connection) -> Result<(), FsPulseError> {
    migration_info("  Phase 4/4: Finalizing...");
    let t = Instant::now();
    Database::immediate_transaction(conn, |c| {
        c.execute_batch("
            DROP TABLE item_versions_old;
        ")?;
        Database::delete_meta_locked(c, COUNT_HWM_KEY)?;
        Database::set_meta_value_locked(c, "schema_version", "28")
    })?;
    migration_info(&format!("  Phase 4/4 complete ({:.1}s)", t.elapsed().as_secs_f64()));
    Ok(())
}

// ── Public entry point ────────────────────────────────────────────────────────

fn run_migration(conn: &Connection) -> Result<(), FsPulseError> {
    let total_start = Instant::now();

    if !table_exists(conn, "item_versions_old")? {
        // Fresh start: Phase 1 not yet done
        phase1_ddl_setup(conn)?;
    } else {
        migration_info("  Phase 1/4: Already complete (resuming).");
    }

    // Phase 2 is always safe to call: it counts what remains in item_versions_old
    // and skips immediately if the table is empty. Creates indexes at end.
    phase2_migrate_data(conn)?;

    // Phase 3 recomputes folder counts using the clean item_versions table.
    // HWM-based; idempotent if called multiple times.
    // Create temporary index to speed up Phase 3 queries that join on
    // (first_scan_id, item_id) — the permanent index has these columns reversed.
    migration_info("    Creating temporary index for Phase 3...");
    conn.execute_batch(
        "CREATE INDEX IF NOT EXISTS idx_tmp_versions_scan_item
             ON item_versions (first_scan_id, item_id);"
    )?;

    phase3_recompute_folder_counts(conn)?;

    conn.execute_batch("DROP INDEX IF EXISTS idx_tmp_versions_scan_item;")?;

    phase4_cleanup(conn)?;

    migration_info(&format!(
        "  Migration v27→v28 complete in {:.1}s",
        total_start.elapsed().as_secs_f64()
    ));
    Ok(())
}

/// Standalone migration entry point.
/// Disables FK enforcement for the duration (DDL renames + scan rollback require it).
pub fn migrate_v27_to_v28(conn: &Connection) -> Result<(), FsPulseError> {
    conn.execute("PRAGMA foreign_keys = OFF", [])
        .map_err(FsPulseError::DatabaseError)?;

    let result = run_migration(conn);

    let _ = conn.execute("PRAGMA foreign_keys = ON", []);
    result
}
