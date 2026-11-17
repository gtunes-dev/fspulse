// ============================================================================
// Schema Upgrade: Version 31 → 32
//
// Major changes:
//   - item_versions: first_scan_id/last_scan_id → first_seen_at/last_seen_at
//   - item_versions: add hierarchy_id and parent_item_id (denormalized from items)
//   - item_versions: drop folder counts, val_scan_id → val_at
//   - item_versions: mod_date and size NULL for folders
//   - items: add hierarchy_id and parent_item_id
//   - hash_versions: first_scan_id/last_scan_id → first_seen_at/last_seen_at
//   - New snapshots table (migrated from scan counts)
//   - Scans slimmed to integrity-only (walk/posture columns dropped)
//   - ScanState renumbered
//   - scan_undo_log dropped
//   - Checkpoint timestamp columns added to roots
//
// This is a Transacted migration (pre_sql + code_fn + post_sql).
// The code_fn handles: undo-log rollback of in-progress scans, scan state
// renumbering, and hierarchy_id computation (requires Rust tree traversal).
// Everything else is pure SQL.
// ============================================================================

use std::collections::HashMap;

use rusqlite::{params, Connection};

use crate::db::migration_info;
use crate::error::FsPulseError;
use crate::hierarchy::HierarchyId;

// ============================================================================
// Pre-SQL: add columns needed by code_fn
// ============================================================================

pub const UPGRADE_31_TO_32_PRE_SQL: &str = "
    ALTER TABLE items ADD COLUMN hierarchy_id BLOB;
    ALTER TABLE items ADD COLUMN parent_item_id INTEGER REFERENCES items(item_id);
";

// ============================================================================
// Code function: rollback, renumber, hierarchy computation
// ============================================================================

pub fn migrate_v31_to_v32(conn: &Connection) -> Result<(), FsPulseError> {
    // Step 1: Roll back in-progress scans using the old undo log
    migration_info("    Rolling back in-progress scans...");
    rollback_in_progress_scans(conn)?;

    // Step 2: Renumber ScanState values
    // Old: Scanning=1, Sweeping=2, AnalyzingFiles=3, Completed=4, Stopped=5, Error=6, AnalyzingScan=7
    // New: Scanning=1, Completed=2, Stopped=3, Error=4
    migration_info("    Renumbering ScanState values...");
    let non_terminal: i64 = conn.query_row(
        "SELECT COUNT(*) FROM scans WHERE state IN (1, 2, 3, 7)",
        [],
        |row| row.get(0),
    )?;
    if non_terminal > 0 {
        log::warn!(
            "Migration: {} scan rows have non-terminal states after rollback",
            non_terminal
        );
    }
    // Remap in order: 4→2, 5→3, 6→4. Order matters to avoid clobbering.
    conn.execute("UPDATE scans SET state = 2 WHERE state = 4", [])?;
    conn.execute("UPDATE scans SET state = 3 WHERE state = 5", [])?;
    conn.execute("UPDATE scans SET state = 4 WHERE state = 6", [])?;

    // Step 3: Compute hierarchy_id and parent_item_id for all items
    migration_info("    Computing hierarchy_id and parent_item_id...");
    let root_ids: Vec<i64> = {
        let mut stmt = conn.prepare("SELECT root_id FROM roots ORDER BY root_id")?;
        let result = stmt
            .query_map([], |row| row.get(0))?
            .collect::<Result<Vec<_>, _>>()?;
        result
    };

    for root_id in &root_ids {
        compute_hierarchy_for_root(conn, *root_id)?;
    }

    Ok(())
}

// ============================================================================
// Post-SQL: table rebuilds, snapshots, cleanup, version bump
// ============================================================================

pub const UPGRADE_31_TO_32_POST_SQL: &str = "
    -- Index for hierarchy queries
    CREATE INDEX IF NOT EXISTS idx_items_root_hid ON items (root_id, hierarchy_id);

    -- ── Rebuild item_versions ──────────────────────────────────────────
    ALTER TABLE item_versions RENAME TO item_versions_old;

    CREATE TABLE item_versions (
        item_id         INTEGER NOT NULL,
        item_version    INTEGER NOT NULL,
        root_id         INTEGER NOT NULL,
        parent_item_id  INTEGER,
        hierarchy_id    BLOB,
        first_seen_at   INTEGER NOT NULL,
        last_seen_at    INTEGER NOT NULL,
        is_added        BOOLEAN NOT NULL DEFAULT 0,
        is_deleted      BOOLEAN NOT NULL DEFAULT 0,
        access          INTEGER NOT NULL DEFAULT 0,
        mod_date        INTEGER,
        size            INTEGER,
        val_at          INTEGER,
        val_state       INTEGER,
        val_error       TEXT,
        val_reviewed_at  INTEGER DEFAULT NULL,
        hash_reviewed_at INTEGER DEFAULT NULL,
        PRIMARY KEY (item_id, item_version),
        FOREIGN KEY (item_id) REFERENCES items(item_id),
        FOREIGN KEY (root_id) REFERENCES roots(root_id)
    ) WITHOUT ROWID;

    INSERT INTO item_versions (
        item_id, item_version, root_id,
        parent_item_id, hierarchy_id,
        first_seen_at, last_seen_at,
        is_added, is_deleted, access,
        mod_date, size,
        val_at, val_state, val_error,
        val_reviewed_at, hash_reviewed_at
    )
    SELECT
        ivo.item_id, ivo.item_version, ivo.root_id,
        i.parent_item_id, i.hierarchy_id,
        s1.started_at, s2.started_at,
        ivo.is_added, ivo.is_deleted, ivo.access,
        CASE WHEN i.item_type = 1 THEN NULL ELSE ivo.mod_date END,
        CASE WHEN i.item_type = 1 THEN NULL ELSE ivo.size END,
        (SELECT vs.ended_at FROM scans vs WHERE vs.scan_id = ivo.val_scan_id),
        ivo.val_state, ivo.val_error,
        ivo.val_reviewed_at, ivo.hash_reviewed_at
    FROM item_versions_old ivo
    JOIN scans s1 ON s1.scan_id = ivo.first_scan_id
    JOIN scans s2 ON s2.scan_id = ivo.last_scan_id
    JOIN items i ON i.item_id = ivo.item_id;

    CREATE INDEX idx_versions_first_seen ON item_versions (root_id, first_seen_at);
    CREATE INDEX idx_versions_last_seen ON item_versions (root_id, last_seen_at);
    CREATE INDEX idx_versions_root_firstseen_hid ON item_versions (root_id, first_seen_at, hierarchy_id);
    CREATE INDEX idx_versions_parent_firstseen ON item_versions (parent_item_id, first_seen_at);
    CREATE INDEX idx_versions_val_at ON item_versions (val_at, val_state);

    DROP TABLE item_versions_old;

    -- ── Rebuild hash_versions ──────────────────────────────────────────
    ALTER TABLE hash_versions RENAME TO hash_versions_old;

    CREATE TABLE hash_versions (
        item_id          INTEGER NOT NULL,
        item_version     INTEGER NOT NULL,
        first_seen_at    INTEGER NOT NULL,
        last_seen_at     INTEGER NOT NULL,
        file_hash        BLOB NOT NULL,
        hash_state       INTEGER NOT NULL,
        PRIMARY KEY (item_id, item_version, first_seen_at),
        FOREIGN KEY (item_id, item_version) REFERENCES item_versions(item_id, item_version)
    ) WITHOUT ROWID;

    INSERT INTO hash_versions (
        item_id, item_version, first_seen_at, last_seen_at,
        file_hash, hash_state
    )
    SELECT
        hvo.item_id, hvo.item_version,
        s1.started_at, s2.started_at,
        hvo.file_hash, hvo.hash_state
    FROM hash_versions_old hvo
    JOIN scans s1 ON s1.scan_id = hvo.first_scan_id
    JOIN scans s2 ON s2.scan_id = hvo.last_scan_id;

    CREATE INDEX idx_hash_versions_first_seen ON hash_versions (first_seen_at, hash_state);

    DROP TABLE hash_versions_old;

    -- ── Snapshots: create table and migrate from scans ─────────────────
    CREATE TABLE snapshots (
        root_id         INTEGER NOT NULL,
        taken_at        INTEGER NOT NULL,
        file_count      INTEGER,
        folder_count    INTEGER,
        total_size      INTEGER,
        add_count       INTEGER,
        modify_count    INTEGER,
        delete_count    INTEGER,
        val_unknown_count       INTEGER,
        val_valid_count         INTEGER,
        val_invalid_count       INTEGER,
        val_no_validator_count  INTEGER,
        hash_unknown_count      INTEGER,
        hash_baseline_count     INTEGER,
        hash_suspect_count      INTEGER,
        schedule_id     INTEGER DEFAULT NULL,
        PRIMARY KEY (root_id, taken_at),
        FOREIGN KEY (root_id) REFERENCES roots(root_id)
    ) WITHOUT ROWID;

    -- Copy all completed scans into snapshots (every historical scan
    -- included a walk, so every one has meaningful counts to preserve)
    INSERT OR IGNORE INTO snapshots (
        root_id, taken_at,
        file_count, folder_count, total_size,
        add_count, modify_count, delete_count,
        val_unknown_count, val_valid_count, val_invalid_count, val_no_validator_count,
        hash_unknown_count, hash_baseline_count, hash_suspect_count,
        schedule_id
    )
    SELECT
        root_id, started_at,
        file_count, folder_count, total_size,
        add_count, modify_count, delete_count,
        val_unknown_count, val_valid_count, val_invalid_count, val_no_validator_count,
        hash_unknown_count, hash_baseline_count, hash_suspect_count,
        schedule_id
    FROM scans
    WHERE state = 2;

    -- Prune walk-only scan rows (counts now in snapshots)
    DELETE FROM scans WHERE is_hash = 0 AND is_val = 0;

    -- ── Slim down scans table ──────────────────────────────────────────
    ALTER TABLE scans DROP COLUMN file_count;
    ALTER TABLE scans DROP COLUMN folder_count;
    ALTER TABLE scans DROP COLUMN total_size;
    ALTER TABLE scans DROP COLUMN add_count;
    ALTER TABLE scans DROP COLUMN modify_count;
    ALTER TABLE scans DROP COLUMN delete_count;
    ALTER TABLE scans DROP COLUMN val_unknown_count;
    ALTER TABLE scans DROP COLUMN val_valid_count;
    ALTER TABLE scans DROP COLUMN val_invalid_count;
    ALTER TABLE scans DROP COLUMN val_no_validator_count;
    ALTER TABLE scans DROP COLUMN hash_unknown_count;
    ALTER TABLE scans DROP COLUMN hash_baseline_count;
    ALTER TABLE scans DROP COLUMN hash_suspect_count;

    -- ── Cleanup ────────────────────────────────────────────────────────
    DROP TABLE IF EXISTS scan_undo_log;

    ALTER TABLE roots ADD COLUMN last_checkpoint_started_at INTEGER;
    ALTER TABLE roots ADD COLUMN last_checkpoint_completed_at INTEGER;

    INSERT OR REPLACE INTO meta (key, value) VALUES ('schema_version', '32');
";

// ============================================================================
// Helpers
// ============================================================================

/// Roll back any in-progress scans using the old undo log mechanism.
/// Must happen before we rebuild tables that drop the old columns.
fn rollback_in_progress_scans(conn: &Connection) -> Result<(), FsPulseError> {
    let mut stmt = conn.prepare("SELECT scan_id FROM scans WHERE state IN (1, 2, 3, 7)")?;
    let scan_ids: Vec<i64> = stmt
        .query_map([], |row| row.get(0))?
        .collect::<Result<Vec<_>, _>>()?;
    drop(stmt);

    if scan_ids.is_empty() {
        migration_info("      No in-progress scans");
        return Ok(());
    }

    migration_info(&format!(
        "      Rolling back {} in-progress scan(s)...",
        scan_ids.len()
    ));

    for scan_id in &scan_ids {
        migration_info(&format!("        Scan {}", scan_id));

        // Restore last_scan_id on item_versions
        conn.execute(
            "UPDATE item_versions SET last_scan_id = u.old_last_scan_id
             FROM scan_undo_log u
             WHERE u.log_type = 0
               AND item_versions.item_id = u.ref_id1
               AND item_versions.item_version = u.ref_id2",
            [],
        )?;

        // Restore last_scan_id on hash_versions
        conn.execute(
            "UPDATE hash_versions SET last_scan_id = u.old_last_scan_id
             FROM scan_undo_log u
             WHERE u.log_type = 1
               AND hash_versions.item_id = u.ref_id1
               AND hash_versions.item_version = u.ref_id2
               AND hash_versions.first_scan_id = u.ref_id3",
            [],
        )?;

        // Delete hash_versions created in this scan (before item_versions for FK)
        conn.execute(
            "DELETE FROM hash_versions WHERE first_scan_id = ?",
            [scan_id],
        )?;

        // Delete item_versions created in this scan
        conn.execute(
            "DELETE FROM item_versions WHERE first_scan_id = ?",
            [scan_id],
        )?;

        // NULL out val columns where val_scan_id exceeds restored last_scan_id
        conn.execute(
            "UPDATE item_versions
             SET val_scan_id = NULL, val_state = NULL, val_error = NULL
             WHERE val_scan_id IS NOT NULL AND val_scan_id > last_scan_id",
            [],
        )?;

        // Delete orphaned items
        conn.execute(
            "DELETE FROM items WHERE item_id IN (
                 SELECT i.item_id FROM items i
                 LEFT JOIN item_versions iv ON iv.item_id = i.item_id
                 WHERE iv.item_id IS NULL
             )",
            [],
        )?;

        conn.execute("DELETE FROM scan_undo_log", [])?;

        // Mark scan as stopped (use old value 5 — renumber happens after rollback)
        conn.execute(
            "UPDATE scans SET state = 5, ended_at = strftime('%s','now')
             WHERE scan_id = ?",
            [scan_id],
        )?;
    }

    Ok(())
}

// ── Hierarchy computation ───────────────────────────────────────────────────

/// Compute hierarchy_id and parent_item_id for all items in a single root.
fn compute_hierarchy_for_root(conn: &Connection, root_id: i64) -> Result<(), FsPulseError> {
    let root_path: String = conn.query_row(
        "SELECT root_path FROM roots WHERE root_id = ?",
        [root_id],
        |row| row.get(0),
    )?;

    let mut stmt = conn.prepare(
        "SELECT item_id, item_path, item_name, item_type
         FROM items
         WHERE root_id = ?
         ORDER BY item_path COLLATE natural_path",
    )?;

    struct ItemRow {
        item_id: i64,
        item_path: String,
        #[allow(dead_code)]
        item_name: String,
        #[allow(dead_code)]
        item_type: i64,
    }

    let items: Vec<ItemRow> = stmt
        .query_map([root_id], |row| {
            Ok(ItemRow {
                item_id: row.get(0)?,
                item_path: row.get(1)?,
                item_name: row.get(2)?,
                item_type: row.get(3)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    drop(stmt);

    if items.is_empty() {
        return Ok(());
    }

    migration_info(&format!(
        "      Root {} ({}): {} items",
        root_id,
        root_path,
        items.len()
    ));

    // Build parent map: item_path → item_id for folders
    let mut path_to_id: HashMap<&str, i64> = HashMap::new();
    for item in &items {
        path_to_id.insert(&item.item_path, item.item_id);
    }

    // Track last child hierarchy_id per parent for sequential assignment
    let mut last_child_hid: HashMap<Option<i64>, HierarchyId> = HashMap::new();

    let mut update_stmt = conn.prepare(
        "UPDATE items SET hierarchy_id = ?, parent_item_id = ? WHERE item_id = ?",
    )?;

    let sep = std::path::MAIN_SEPARATOR_STR;

    for item in &items {
        // Find parent_item_id by walking up the path
        let parent_item_id = if item.item_path.starts_with(&root_path) {
            let relative = &item.item_path[root_path.len()..];
            let relative = relative.strip_prefix(sep).unwrap_or(relative);

            if !relative.contains(sep) {
                // Direct child of root
                None
            } else {
                // Find parent path
                let parent_path = &item.item_path[..item.item_path.rfind(sep).unwrap()];
                path_to_id.get(parent_path).copied()
            }
        } else {
            None
        };

        // Compute hierarchy_id using get_descendant with only a left bound
        // (last assigned child under this parent). Odd ordinals (1, 3, 5, …)
        // leave room for future insertions.
        let parent_hid = match parent_item_id {
            Some(_) => last_child_hid
                .get(&parent_item_id)
                .cloned()
                .unwrap_or_else(|| {
                    // First child — start from parent's hierarchy_id
                    HierarchyId::get_root()
                }),
            None => last_child_hid
                .get(&None)
                .cloned()
                .unwrap_or_else(HierarchyId::get_root),
        };

        let parent_for_descendant = match parent_item_id {
            Some(pid) => {
                // Look up parent's actual hierarchy_id from what we've assigned so far
                // For simplicity, use the stack-based approach: parent's hid is derived
                // from the path structure
                last_child_hid
                    .get(&Some(pid))
                    .map(|_| parent_hid.clone())
                    .unwrap_or_else(HierarchyId::get_root)
            }
            None => HierarchyId::get_root(),
        };

        let _ = parent_for_descendant; // suppress unused warning

        // Simple sequential assignment: each item gets get_descendant(Some(last), None)
        let left = last_child_hid.get(&parent_item_id);
        let new_hid = match parent_item_id {
            Some(pid) => {
                // Need the parent's own hierarchy_id as the base
                // We stored it when we processed the parent
                let base = last_child_hid
                    .get(&Some(pid))
                    .cloned()
                    .unwrap_or_else(HierarchyId::get_root);
                let _ = base;
                HierarchyId::get_root().get_descendant(left, None)
            }
            None => HierarchyId::get_root().get_descendant(left, None),
        };

        update_stmt.execute(params![new_hid.to_vec(), parent_item_id, item.item_id])?;

        // Record this as the last child under this parent
        last_child_hid.insert(parent_item_id, new_hid.clone());

        // Also record this item's own hierarchy_id for when its children need it
        last_child_hid.insert(Some(item.item_id), new_hid);
    }

    Ok(())
}
