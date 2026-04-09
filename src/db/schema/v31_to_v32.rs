// ============================================================================
// Schema Upgrade: Version 31 → 32 — Temporal timestamps, hierarchy, drop folder counts
//
// Major data model evolution:
//   - item_versions: replace first_scan_id/last_scan_id with first_seen_at/last_seen_at
//   - item_versions: add hierarchy_id and parent_item_id (denormalized from items)
//   - item_versions: drop precomputed folder counts (add_count, modify_count,
//     delete_count, unchanged_count) — now computed on demand via hierarchy_id
//   - item_versions: mod_date and size become file-only (NULL for folders)
//   - items: add hierarchy_id and parent_item_id
//   - hash_versions: replace first_scan_id/last_scan_id with first_seen_at/last_seen_at
//   - Drop scan_undo_log table (no longer needed — each item operation is atomic)
//
// This is a Standalone migration. Manages its own transactions.
//
// Phases:
//   1. Roll back any in-progress scans using the old undo log (before we drop it).
//   2. Compute hierarchy_id and parent_item_id for all items (Rust code, per root).
//      Items are loaded into memory per-root to build the tree and assign ordinals.
//   3. DDL: Rebuild item_versions (rename → old, create new, migrate data, drop old).
//   4. DDL: Rebuild hash_versions similarly.
//   5. Cleanup: Drop scan_undo_log, create new indexes, bump schema version.
// ============================================================================

use std::collections::HashMap;

use rusqlite::{params, Connection};

use crate::db::{migration_info, Database};
use crate::error::FsPulseError;
use crate::hierarchy::HierarchyId;

const BATCH_SIZE: i64 = 10_000;

pub fn migrate_v31_to_v32(conn: &Connection) -> Result<(), FsPulseError> {
    // ── Phase 1: Roll back in-progress scans ────────────────────────────
    migration_info("  Phase 1: Rolling back in-progress scans...");
    rollback_in_progress_scans(conn)?;

    // ── Phase 2: Compute hierarchy_id and parent_item_id on items ───────
    migration_info("  Phase 2: Computing hierarchy_id and parent_item_id...");

    // Add columns to items (idempotent — IF NOT EXISTS not available for
    // ALTER TABLE, so we check manually)
    Database::immediate_transaction(conn, |conn| {
        if !column_exists(conn, "items", "hierarchy_id")? {
            conn.execute_batch(
                "ALTER TABLE items ADD COLUMN hierarchy_id BLOB;
                 ALTER TABLE items ADD COLUMN parent_item_id INTEGER REFERENCES items(item_id);",
            )?;
        }
        Ok(())
    })?;

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

    // Create index on items hierarchy_id
    Database::immediate_transaction(conn, |conn| {
        conn.execute_batch(
            "CREATE INDEX IF NOT EXISTS idx_items_root_hid ON items (root_id, hierarchy_id);",
        )?;
        Ok(())
    })?;

    // ── Phase 3: Rebuild item_versions ──────────────────────────────────
    migration_info("  Phase 3: Rebuilding item_versions...");
    rebuild_item_versions(conn)?;

    // ── Phase 4: Rebuild hash_versions ──────────────────────────────────
    migration_info("  Phase 4: Rebuilding hash_versions...");
    rebuild_hash_versions(conn)?;

    // ── Phase 5: Cleanup ────────────────────────────────────────────────
    migration_info("  Phase 5: Cleanup...");
    Database::immediate_transaction(conn, |conn| {
        // Drop the undo log — no longer needed
        conn.execute_batch("DROP TABLE IF EXISTS scan_undo_log;")?;

        // Bump schema version
        conn.execute_batch(
            "INSERT OR REPLACE INTO meta (key, value) VALUES ('schema_version', '32');",
        )?;

        Ok(())
    })?;

    migration_info("  Migration v31→v32 complete");
    Ok(())
}

// ── Helpers ─────────────────────────────────────────────────────────────────

fn column_exists(conn: &Connection, table: &str, column: &str) -> Result<bool, FsPulseError> {
    let sql = format!("PRAGMA table_info({})", table);
    let mut stmt = conn.prepare(&sql)?;
    let found = stmt
        .query_map([], |row| row.get::<_, String>(1))?
        .any(|r| r.map(|name| name == column).unwrap_or(false));
    Ok(found)
}

/// Roll back any in-progress scans using the old undo log mechanism.
/// Must happen before we drop the undo log and scan_id columns.
fn rollback_in_progress_scans(conn: &Connection) -> Result<(), FsPulseError> {
    let mut stmt = conn.prepare("SELECT scan_id FROM scans WHERE state IN (1, 2, 3, 7)")?;
    let scan_ids: Vec<i64> = stmt
        .query_map([], |row| row.get(0))?
        .collect::<Result<Vec<_>, _>>()?;
    drop(stmt);

    if scan_ids.is_empty() {
        migration_info("    No in-progress scans");
        return Ok(());
    }

    migration_info(&format!(
        "    Rolling back {} in-progress scan(s)...",
        scan_ids.len()
    ));

    Database::immediate_transaction(conn, |conn| {
        for scan_id in &scan_ids {
            migration_info(&format!("      Scan {}", scan_id));

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

            // Mark scan as stopped
            conn.execute(
                "UPDATE scans SET state = 5, ended_at = strftime('%s','now')
                 WHERE scan_id = ?",
                [scan_id],
            )?;
        }
        Ok(())
    })?;

    Ok(())
}

/// Rebuild item_versions: rename old table, create new with timestamp columns,
/// migrate data in batches, drop old table.
fn rebuild_item_versions(conn: &Connection) -> Result<(), FsPulseError> {
    // Phase 3a: DDL — rename old, create new
    migration_info("    Renaming item_versions → item_versions_old...");
    Database::immediate_transaction(conn, |conn| {
        conn.execute_batch("PRAGMA foreign_keys = OFF;")?;

        conn.execute_batch("ALTER TABLE item_versions RENAME TO item_versions_old;")?;

        conn.execute_batch(
            "CREATE TABLE item_versions (
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
                val_scan_id     INTEGER,
                val_state       INTEGER,
                val_error       TEXT,
                val_reviewed_at  INTEGER DEFAULT NULL,
                hash_reviewed_at INTEGER DEFAULT NULL,
                PRIMARY KEY (item_id, item_version),
                FOREIGN KEY (item_id) REFERENCES items(item_id),
                FOREIGN KEY (root_id) REFERENCES roots(root_id)
            ) WITHOUT ROWID;",
        )?;

        conn.execute_batch("PRAGMA foreign_keys = ON;")?;
        Ok(())
    })?;

    // Phase 3b: Migrate data in batches
    // Convert first_scan_id/last_scan_id → timestamps via scans.started_at
    // Pull hierarchy_id and parent_item_id from items
    // Drop folder count columns (not copied)
    // NULL out mod_date and size for folders (item_type = 1)
    migration_info("    Migrating item_versions data...");

    let total: i64 = conn.query_row(
        "SELECT COUNT(*) FROM item_versions_old",
        [],
        |row| row.get(0),
    )?;

    migration_info(&format!("    {} rows to migrate", total));

    let mut offset: i64 = 0;
    loop {
        let migrated = Database::immediate_transaction(conn, |conn| {
            // Use rowid ordering from the old table for stable batching.
            // Join to scans for timestamp conversion, items for hierarchy columns.
            let count = conn.execute(
                "INSERT INTO item_versions (
                    item_id, item_version, root_id,
                    parent_item_id, hierarchy_id,
                    first_seen_at, last_seen_at,
                    is_added, is_deleted, access,
                    mod_date, size,
                    val_scan_id, val_state, val_error,
                    val_reviewed_at, hash_reviewed_at
                )
                SELECT
                    ivo.item_id, ivo.item_version, ivo.root_id,
                    i.parent_item_id, i.hierarchy_id,
                    s1.started_at, s2.started_at,
                    ivo.is_added, ivo.is_deleted, ivo.access,
                    CASE WHEN i.item_type = 1 THEN NULL ELSE ivo.mod_date END,
                    CASE WHEN i.item_type = 1 THEN NULL ELSE ivo.size END,
                    ivo.val_scan_id, ivo.val_state, ivo.val_error,
                    ivo.val_reviewed_at, ivo.hash_reviewed_at
                FROM item_versions_old ivo
                JOIN scans s1 ON s1.scan_id = ivo.first_scan_id
                JOIN scans s2 ON s2.scan_id = ivo.last_scan_id
                JOIN items i ON i.item_id = ivo.item_id
                ORDER BY ivo.item_id, ivo.item_version
                LIMIT ? OFFSET ?",
                params![BATCH_SIZE, offset],
            )?;
            Ok(count)
        })?;

        offset += migrated as i64;

        if offset % 100_000 < BATCH_SIZE || offset >= total {
            migration_info(&format!("    {}/{} rows migrated", offset.min(total), total));
        }

        if (migrated as i64) < BATCH_SIZE {
            break;
        }
    }

    // Phase 3c: Create indexes and drop old table
    migration_info("    Creating indexes on item_versions...");
    Database::immediate_transaction(conn, |conn| {
        conn.execute_batch(
            "CREATE INDEX idx_versions_first_seen ON item_versions (root_id, first_seen_at);
             CREATE INDEX idx_versions_last_seen ON item_versions (root_id, last_seen_at);
             CREATE INDEX idx_versions_root_firstseen_hid ON item_versions (root_id, first_seen_at, hierarchy_id);
             CREATE INDEX idx_versions_parent_firstseen ON item_versions (parent_item_id, first_seen_at);
             CREATE INDEX idx_versions_val_scan ON item_versions (val_scan_id, val_state);",
        )?;
        Ok(())
    })?;

    migration_info("    Dropping item_versions_old...");
    Database::immediate_transaction(conn, |conn| {
        conn.execute_batch("DROP TABLE item_versions_old;")?;
        Ok(())
    })?;

    Ok(())
}

/// Rebuild hash_versions: rename old, create new with timestamp columns,
/// migrate data, drop old.
fn rebuild_hash_versions(conn: &Connection) -> Result<(), FsPulseError> {
    migration_info("    Renaming hash_versions → hash_versions_old...");
    Database::immediate_transaction(conn, |conn| {
        conn.execute_batch("PRAGMA foreign_keys = OFF;")?;

        conn.execute_batch("ALTER TABLE hash_versions RENAME TO hash_versions_old;")?;

        conn.execute_batch(
            "CREATE TABLE hash_versions (
                item_id          INTEGER NOT NULL,
                item_version     INTEGER NOT NULL,
                first_seen_at    INTEGER NOT NULL,
                last_seen_at     INTEGER NOT NULL,
                file_hash        BLOB NOT NULL,
                hash_state       INTEGER NOT NULL,
                PRIMARY KEY (item_id, item_version, first_seen_at),
                FOREIGN KEY (item_id, item_version) REFERENCES item_versions(item_id, item_version)
            ) WITHOUT ROWID;",
        )?;

        conn.execute_batch("PRAGMA foreign_keys = ON;")?;
        Ok(())
    })?;

    let total: i64 = conn.query_row(
        "SELECT COUNT(*) FROM hash_versions_old",
        [],
        |row| row.get(0),
    )?;

    migration_info(&format!("    {} rows to migrate", total));

    let mut offset: i64 = 0;
    loop {
        let migrated = Database::immediate_transaction(conn, |conn| {
            let count = conn.execute(
                "INSERT INTO hash_versions (
                    item_id, item_version, first_seen_at, last_seen_at,
                    file_hash, hash_state
                )
                SELECT
                    hvo.item_id, hvo.item_version,
                    s1.started_at, s2.started_at,
                    hvo.file_hash, hvo.hash_state
                FROM hash_versions_old hvo
                JOIN scans s1 ON s1.scan_id = hvo.first_scan_id
                JOIN scans s2 ON s2.scan_id = hvo.last_scan_id
                ORDER BY hvo.item_id, hvo.item_version, hvo.first_scan_id
                LIMIT ? OFFSET ?",
                params![BATCH_SIZE, offset],
            )?;
            Ok(count)
        })?;

        offset += migrated as i64;

        if offset % 100_000 < BATCH_SIZE || offset >= total {
            migration_info(&format!("    {}/{} rows migrated", offset.min(total), total));
        }

        if (migrated as i64) < BATCH_SIZE {
            break;
        }
    }

    migration_info("    Creating indexes on hash_versions...");
    Database::immediate_transaction(conn, |conn| {
        conn.execute_batch(
            "CREATE INDEX idx_hash_versions_first_seen ON hash_versions (first_seen_at, hash_state);",
        )?;
        Ok(())
    })?;

    migration_info("    Dropping hash_versions_old...");
    Database::immediate_transaction(conn, |conn| {
        conn.execute_batch("DROP TABLE hash_versions_old;")?;
        Ok(())
    })?;

    Ok(())
}

// ── Hierarchy computation ───────────────────────────────────────────────────

/// Compute hierarchy_id and parent_item_id for all items in a single root.
///
/// Loads all items for the root into memory to build the parent-child tree
/// and assign ordpath ordinals depth-first. Items are sorted alphabetically
/// within each parent folder. Ordinals use odd numbers (1, 3, 5, …) to
/// leave room for future insertions between siblings.
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

    let items: Vec<(i64, String, String, i64)> = stmt
        .query_map([root_id], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    drop(stmt);

    if items.is_empty() {
        migration_info(&format!(
            "    Root {} ({}): 0 items, skipping",
            root_id, root_path
        ));
        return Ok(());
    }

    migration_info(&format!(
        "    Root {} ({}): {} items",
        root_id, root_path, items.len()
    ));

    // Build parent_path → Vec<(item_id, item_name)> map
    let mut children_by_parent: HashMap<String, Vec<(i64, String)>> = HashMap::new();

    // Build path → item_id map for folders (to resolve parent_item_id)
    let mut folder_id_by_path: HashMap<String, i64> = HashMap::new();

    for (item_id, item_path, item_name, item_type) in &items {
        let parent_path = match item_path.rfind('/') {
            Some(pos) => &item_path[..pos],
            None => "",
        };
        children_by_parent
            .entry(parent_path.to_string())
            .or_default()
            .push((*item_id, item_name.clone()));

        if *item_type == 1 {
            folder_id_by_path.insert(item_path.clone(), *item_id);
        }
    }

    // Walk the tree depth-first, assigning hierarchy_ids and parent IDs.
    // (item_id, hierarchy_id_bytes, parent_item_id)
    let mut assignments: Vec<(i64, Vec<u8>, Option<i64>)> = Vec::with_capacity(items.len());

    assign_hierarchy_recursive(
        &root_path,
        &HierarchyId::get_root(),
        None,
        &children_by_parent,
        &folder_id_by_path,
        &mut assignments,
    );

    // Batch update items
    let total = assignments.len();
    let mut idx = 0;

    while idx < total {
        let batch_end = (idx + 5000).min(total);
        Database::immediate_transaction(conn, |conn| {
            let mut update_stmt = conn.prepare_cached(
                "UPDATE items SET hierarchy_id = ?, parent_item_id = ? WHERE item_id = ?",
            )?;
            for (item_id, hid_bytes, parent_id) in &assignments[idx..batch_end] {
                update_stmt.execute(params![hid_bytes, parent_id, item_id])?;
            }
            Ok(())
        })?;

        idx = batch_end;
        if idx % 100_000 < 5000 || idx == total {
            migration_info(&format!("      {}/{} items assigned", idx, total));
        }
    }

    Ok(())
}

/// Recursively assign hierarchy_ids to children of `parent_hid` in order.
/// Each child is positioned just after the previous sibling via
/// `get_descendant`, producing the same odd-ordinal sequence (1, 3, 5, …)
/// the prior implementation built directly.
fn assign_hierarchy_recursive(
    parent_path: &str,
    parent_hid: &HierarchyId,
    parent_item_id: Option<i64>,
    children_by_parent: &HashMap<String, Vec<(i64, String)>>,
    folder_id_by_path: &HashMap<String, i64>,
    assignments: &mut Vec<(i64, Vec<u8>, Option<i64>)>,
) {
    let children = match children_by_parent.get(parent_path) {
        Some(c) => c,
        None => return,
    };

    let mut prev_sibling: Option<HierarchyId> = None;
    for (item_id, item_name) in children {
        let hid = parent_hid.get_descendant(prev_sibling.as_ref(), None);
        assignments.push((*item_id, hid.to_vec(), parent_item_id));

        let child_path = format!("{}/{}", parent_path, item_name);
        assign_hierarchy_recursive(
            &child_path,
            &hid,
            Some(*item_id),
            children_by_parent,
            folder_id_by_path,
            assignments,
        );

        prev_sibling = Some(hid);
    }
}
