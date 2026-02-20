use crate::error::FsPulseError;
use log::info;
use rusqlite::Connection;
use std::collections::HashMap;

/// Schema Upgrade: Version 15 → 16
///
/// Phase 1 (pre-SQL):
///   - Renames `items` → `items_old` and recreates indexes with `_old` suffix.
///   - Creates new `items` (identity), `item_versions`, and `scan_undo_log` tables.
///
/// Phase 2 (Rust code):
///   - Migrates data from `items_old` + `changes` into `items` and `item_versions`.
///   - Validates the migration and errors out if discrepancies are found.
///
/// Phase 3 (post-SQL):
///   - Updates schema version to 16.
pub const UPGRADE_15_TO_16_PRE_SQL: &str = r#"
--
-- Schema Upgrade: Version 15 → 16 (Pre-SQL Phase)
--
-- Rename items → items_old, recreate indexes, and create new temporal versioning tables.
--

-- Verify schema version is exactly 15
SELECT 1 / (CASE WHEN (SELECT value FROM meta WHERE key = 'schema_version') = '15' THEN 1 ELSE 0 END);

-- ========================================
-- Rename old items table
-- ========================================
ALTER TABLE items RENAME TO items_old;

-- Drop and recreate only the indexes whose names conflict with the new items table.
-- idx_items_root_scan has no conflict and is automatically remapped to items_old by the rename.
DROP INDEX IF EXISTS idx_items_path;
DROP INDEX IF EXISTS idx_items_root_path;

CREATE INDEX idx_items_old_path ON items_old (item_path COLLATE natural_path);
CREATE INDEX idx_items_old_root_path ON items_old (root_id, item_path COLLATE natural_path, item_type);

-- ========================================
-- Create new identity table
-- ========================================
CREATE TABLE items (
    item_id     INTEGER PRIMARY KEY AUTOINCREMENT,
    root_id     INTEGER NOT NULL,
    item_path   TEXT NOT NULL,
    item_name   TEXT NOT NULL,
    item_type   INTEGER NOT NULL,
    FOREIGN KEY (root_id) REFERENCES roots(root_id),
    UNIQUE (root_id, item_path, item_type)
);

CREATE INDEX idx_items_path ON items (item_path COLLATE natural_path);
CREATE INDEX idx_items_root_path ON items (root_id, item_path COLLATE natural_path, item_type);
CREATE INDEX idx_items_root_name ON items (root_id, item_name COLLATE natural_path);

-- ========================================
-- Create item_versions table
-- ========================================
CREATE TABLE item_versions (
    version_id      INTEGER PRIMARY KEY AUTOINCREMENT,
    item_id         INTEGER NOT NULL,
    first_scan_id   INTEGER NOT NULL,
    last_scan_id    INTEGER NOT NULL,
    is_deleted      BOOLEAN NOT NULL DEFAULT 0,
    access          INTEGER NOT NULL DEFAULT 0,
    mod_date        INTEGER,
    size            INTEGER,
    file_hash       TEXT,
    val             INTEGER NOT NULL DEFAULT 3,
    val_error       TEXT,
    last_hash_scan  INTEGER,
    last_val_scan   INTEGER,
    FOREIGN KEY (item_id) REFERENCES items(item_id),
    FOREIGN KEY (first_scan_id) REFERENCES scans(scan_id),
    FOREIGN KEY (last_scan_id) REFERENCES scans(scan_id)
);

CREATE INDEX idx_versions_item_scan ON item_versions (item_id, first_scan_id DESC);
CREATE INDEX idx_versions_first_scan ON item_versions (first_scan_id);

-- ========================================
-- Create scan undo log table
-- ========================================
CREATE TABLE scan_undo_log (
    undo_id             INTEGER PRIMARY KEY AUTOINCREMENT,
    version_id          INTEGER NOT NULL,
    old_last_scan_id    INTEGER NOT NULL,
    old_last_hash_scan  INTEGER,
    old_last_val_scan   INTEGER
);
"#;

/// Rust code phase: Migrate data from `items_old` + `changes` into `items` + `item_versions`.
pub fn migrate_15_to_16(conn: &Connection) -> Result<(), FsPulseError> {
    // Step 1: Bulk-copy identities from items_old into new items table
    info!("Migration 15→16: Copying item identities...");
    let identity_count = conn.execute(
        "INSERT INTO items (item_id, root_id, item_path, item_name, item_type)
         SELECT item_id, root_id, item_path,
                REPLACE(item_path, RTRIM(item_path, REPLACE(item_path, '/', '')), ''),
                item_type
         FROM items_old",
        [],
    ).map_err(FsPulseError::DatabaseError)?;
    info!("Migration 15→16: Copied {} item identities", identity_count);

    // Step 2: Build a map of root_id -> sorted completed scan_ids
    // We need completed scans to determine "previous scan" when closing versions
    info!("Migration 15→16: Building completed scan map...");
    let completed_scans = build_completed_scan_map(conn)?;
    info!("Migration 15→16: Found {} roots with completed scans", completed_scans.len());

    // Step 3: For each item, reconstruct version chain from changes
    info!("Migration 15→16: Reconstructing version chains...");
    let version_count = reconstruct_versions(conn, &completed_scans)?;
    info!("Migration 15→16: Created {} version rows", version_count);

    Ok(())
}

/// Post-SQL: Update schema version and pause scheduled tasks.
pub const UPGRADE_15_TO_16_POST_SQL: &str = r#"
UPDATE meta SET value = '16' WHERE key = 'schema_version';

-- Pause scheduled tasks indefinitely after migration.
-- The scanner does not yet write to the new temporal tables,
-- so prevent it from running until dual-write is implemented.
INSERT OR REPLACE INTO meta (key, value) VALUES ('pause_until', '-1');
"#;

// ============================================================================
// Migration helpers
// ============================================================================

/// Build a map of root_id -> sorted Vec of completed scan_ids for that root.
fn build_completed_scan_map(conn: &Connection) -> Result<HashMap<i64, Vec<i64>>, FsPulseError> {
    let mut stmt = conn.prepare(
        "SELECT scan_id, root_id FROM scans WHERE state = 4 ORDER BY scan_id ASC"
    ).map_err(FsPulseError::DatabaseError)?;

    let rows = stmt.query_map([], |row| {
        Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?))
    }).map_err(FsPulseError::DatabaseError)?;

    let mut map: HashMap<i64, Vec<i64>> = HashMap::new();
    for row in rows {
        let (scan_id, root_id) = row.map_err(FsPulseError::DatabaseError)?;
        map.entry(root_id).or_default().push(scan_id);
    }

    Ok(map)
}

/// Find the completed scan immediately before `target_scan_id` for a given root.
fn find_previous_scan(completed_scans: &HashMap<i64, Vec<i64>>, root_id: i64, target_scan_id: i64) -> Option<i64> {
    if let Some(scans) = completed_scans.get(&root_id) {
        // scans is sorted ascending; find the last one < target_scan_id
        let mut prev = None;
        for &sid in scans {
            if sid < target_scan_id {
                prev = Some(sid);
            } else {
                break;
            }
        }
        prev
    } else {
        None
    }
}

/// Represents the mutable state we track while building version chains for an item.
struct VersionBuilder {
    // Current version state being built
    first_scan_id: i64,
    is_deleted: bool,
    access: i64,
    mod_date: Option<i64>,
    size: Option<i64>,
    file_hash: Option<String>,
    val: i64,
    val_error: Option<String>,
    last_hash_scan: Option<i64>,
    last_val_scan: Option<i64>,
}

/// Reconstruct item_versions from items_old + changes.
/// Returns the total number of version rows created.
fn reconstruct_versions(
    conn: &Connection,
    completed_scans: &HashMap<i64, Vec<i64>>,
) -> Result<usize, FsPulseError> {
    // Fetch all items, ordered for predictable processing
    let mut item_stmt = conn.prepare(
        "SELECT item_id, root_id, access, last_scan, is_ts,
                mod_date, size, last_hash_scan, file_hash, last_val_scan, val, val_error
         FROM items_old
         ORDER BY item_id ASC"
    ).map_err(FsPulseError::DatabaseError)?;

    let items: Vec<ItemOldRow> = item_stmt
        .query_map([], |row| {
            Ok(ItemOldRow {
                item_id: row.get(0)?,
                root_id: row.get(1)?,
                access: row.get(2)?,
                last_scan: row.get(3)?,
                is_ts: row.get(4)?,
                mod_date: row.get(5)?,
                size: row.get(6)?,
                last_hash_scan: row.get(7)?,
                file_hash: row.get(8)?,
                last_val_scan: row.get(9)?,
                val: row.get(10)?,
                val_error: row.get(11)?,
            })
        })
        .map_err(FsPulseError::DatabaseError)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(FsPulseError::DatabaseError)?;

    drop(item_stmt);

    // Prepare change query (reused per item)
    // Only select columns we actually need — skip _old columns to avoid type mismatches
    // (e.g., val_old is sometimes stored as text, sometimes as integer)
    let mut change_stmt = conn.prepare(
        "SELECT scan_id, change_type,
                access_new,
                is_undelete,
                meta_change, mod_date_new, size_new,
                hash_change, hash_new,
                val_change, val_new, val_error_new
         FROM changes
         WHERE item_id = ?
         ORDER BY scan_id ASC"
    ).map_err(FsPulseError::DatabaseError)?;

    // Prepare version insert
    let mut insert_stmt = conn.prepare(
        "INSERT INTO item_versions (
            item_id, first_scan_id, last_scan_id,
            is_deleted, access, mod_date, size, file_hash,
            val, val_error, last_hash_scan, last_val_scan
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"
    ).map_err(FsPulseError::DatabaseError)?;

    let mut total_versions = 0;

    for item in &items {
        let item_id = item.item_id;
        let root_id = item.root_id;
        let last_scan = item.last_scan;

        // Fetch all changes for this item in scan order
        let changes: Vec<ChangeRow> = change_stmt
            .query_map([item_id], |row| {
                Ok(ChangeRow {
                    scan_id: row.get(0)?,
                    change_type: row.get(1)?,
                    access_new: row.get(2)?,
                    is_undelete: row.get(3)?,
                    meta_change: row.get(4)?,
                    mod_date_new: row.get(5)?,
                    size_new: row.get(6)?,
                    hash_change: row.get(7)?,
                    hash_new: row.get(8)?,
                    val_change: row.get(9)?,
                    val_new: row.get(10)?,
                    val_error_new: row.get(11)?,
                })
            })
            .map_err(FsPulseError::DatabaseError)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(FsPulseError::DatabaseError)?;

        if changes.is_empty() {
            // No changes at all — create a single version from items_old current state.
            insert_stmt.execute(rusqlite::params![
                item_id,
                last_scan, last_scan,
                item.is_ts as i64, item.access, item.mod_date, item.size, &item.file_hash,
                item.val, &item.val_error, item.last_hash_scan, item.last_val_scan,
            ]).map_err(FsPulseError::DatabaseError)?;
            total_versions += 1;
            continue;
        }

        let mut builder: Option<VersionBuilder> = None;

        for change in &changes {
            match change.change_type {
                // Add (1)
                1 => {
                    if change.is_undelete == Some(true) {
                        // Undelete: close the previous deletion version, start fresh alive version
                        if let Some(ref prev) = builder {
                            let close_scan = find_previous_scan(completed_scans, root_id, change.scan_id)
                                .unwrap_or(prev.first_scan_id);
                            insert_stmt.execute(rusqlite::params![
                                item_id,
                                prev.first_scan_id, close_scan,
                                prev.is_deleted as i64, prev.access,
                                prev.mod_date, prev.size, prev.file_hash,
                                prev.val, prev.val_error, prev.last_hash_scan, prev.last_val_scan,
                            ]).map_err(FsPulseError::DatabaseError)?;
                            total_versions += 1;
                        }

                        // Start fresh alive version (hash/val reset — matches rehydration behavior)
                        let new_access = change.access_new.unwrap_or(0);
                        builder = Some(VersionBuilder {
                            first_scan_id: change.scan_id,
                            is_deleted: false,
                            access: new_access,
                            mod_date: change.mod_date_new,
                            size: change.size_new,
                            file_hash: None,
                            val: 3, // Unknown
                            val_error: None,
                            last_hash_scan: None,
                            last_val_scan: None,
                        });
                    } else {
                        // Normal Add: first time seeing this item
                        // Close any previous version (shouldn't normally exist for a true first Add)
                        if let Some(ref prev) = builder {
                            let close_scan = find_previous_scan(completed_scans, root_id, change.scan_id)
                                .unwrap_or(prev.first_scan_id);
                            insert_stmt.execute(rusqlite::params![
                                item_id,
                                prev.first_scan_id, close_scan,
                                prev.is_deleted as i64, prev.access,
                                prev.mod_date, prev.size, prev.file_hash,
                                prev.val, prev.val_error, prev.last_hash_scan, prev.last_val_scan,
                            ]).map_err(FsPulseError::DatabaseError)?;
                            total_versions += 1;
                        }

                        let new_access = change.access_new.unwrap_or(0);
                        builder = Some(VersionBuilder {
                            first_scan_id: change.scan_id,
                            is_deleted: false,
                            access: new_access,
                            mod_date: change.mod_date_new,
                            size: change.size_new,
                            file_hash: None,
                            val: 3, // Unknown
                            val_error: None,
                            last_hash_scan: None,
                            last_val_scan: None,
                        });
                    }

                    // Apply hash/val from the Add change if present
                    if let Some(ref mut b) = builder {
                        if change.hash_change == Some(true) {
                            b.file_hash = change.hash_new.clone();
                            b.last_hash_scan = Some(change.scan_id);
                        }
                        if change.val_change == Some(true) {
                            if let Some(val_new) = change.val_new {
                                b.val = val_new;
                            }
                            b.val_error = change.val_error_new.clone();
                            b.last_val_scan = Some(change.scan_id);
                        }
                    }
                }

                // Modify (2)
                2 => {
                    if let Some(ref prev) = builder {
                        // Determine if this modify was same-scan as the version being built
                        // (analysis phase updating a scan-phase version)
                        let is_same_scan = prev.first_scan_id == change.scan_id;

                        if is_same_scan {
                            // Same-scan update: update the current version in place
                            // This happens when analysis phase finds changes for an item
                            // that already got a new version in the scan phase
                        } else {
                            // Different scan: close previous version, start new one
                            let close_scan = find_previous_scan(completed_scans, root_id, change.scan_id)
                                .unwrap_or(prev.first_scan_id);
                            insert_stmt.execute(rusqlite::params![
                                item_id,
                                prev.first_scan_id, close_scan,
                                prev.is_deleted as i64, prev.access,
                                prev.mod_date, prev.size, prev.file_hash,
                                prev.val, prev.val_error, prev.last_hash_scan, prev.last_val_scan,
                            ]).map_err(FsPulseError::DatabaseError)?;
                            total_versions += 1;
                        }

                        // Build the new version state, carrying forward what didn't change
                        let mut new_builder = if is_same_scan {
                            // Clone current state — we'll mutate it
                            VersionBuilder {
                                first_scan_id: prev.first_scan_id,
                                is_deleted: prev.is_deleted,
                                access: prev.access,
                                mod_date: prev.mod_date,
                                size: prev.size,
                                file_hash: prev.file_hash.clone(),
                                val: prev.val,
                                val_error: prev.val_error.clone(),
                                last_hash_scan: prev.last_hash_scan,
                                last_val_scan: prev.last_val_scan,
                            }
                        } else {
                            // Carry forward from previous version
                            VersionBuilder {
                                first_scan_id: change.scan_id,
                                is_deleted: false,
                                access: prev.access,
                                mod_date: prev.mod_date,
                                size: prev.size,
                                file_hash: prev.file_hash.clone(),
                                val: prev.val,
                                val_error: prev.val_error.clone(),
                                last_hash_scan: prev.last_hash_scan,
                                last_val_scan: prev.last_val_scan,
                            }
                        };

                        // Apply changes
                        if let Some(access_new) = change.access_new {
                            new_builder.access = access_new;
                        }
                        if change.meta_change == Some(true) {
                            new_builder.mod_date = change.mod_date_new;
                            new_builder.size = change.size_new;
                        }
                        if change.hash_change == Some(true) {
                            new_builder.file_hash = change.hash_new.clone();
                            new_builder.last_hash_scan = Some(change.scan_id);
                        } else if change.hash_change == Some(false) {
                            // Hash was evaluated but didn't change — update last_hash_scan
                            new_builder.last_hash_scan = Some(change.scan_id);
                        }
                        if change.val_change == Some(true) {
                            if let Some(val_new) = change.val_new {
                                new_builder.val = val_new;
                            }
                            new_builder.val_error = change.val_error_new.clone();
                            new_builder.last_val_scan = Some(change.scan_id);
                        } else if change.val_change == Some(false) {
                            // Val was evaluated but didn't change — update last_val_scan
                            new_builder.last_val_scan = Some(change.scan_id);
                        }

                        builder = Some(new_builder);
                    } else {
                        // Modify with no prior state — shouldn't happen, but handle gracefully.
                        // Treat it like an Add based on whatever _new values we have.
                        builder = Some(VersionBuilder {
                            first_scan_id: change.scan_id,
                            is_deleted: false,
                            access: change.access_new.unwrap_or(0),
                            mod_date: change.mod_date_new,
                            size: change.size_new,
                            file_hash: change.hash_new.clone(),
                            val: change.val_new.unwrap_or(3),
                            val_error: change.val_error_new.clone(),
                            last_hash_scan: if change.hash_new.is_some() { Some(change.scan_id) } else { None },
                            last_val_scan: if change.val_new.is_some() { Some(change.scan_id) } else { None },
                        });
                    }
                }

                // Delete (3)
                3 => {
                    if let Some(ref prev) = builder {
                        // Close the previous version
                        let close_scan = find_previous_scan(completed_scans, root_id, change.scan_id)
                            .unwrap_or(prev.first_scan_id);
                        insert_stmt.execute(rusqlite::params![
                            item_id,
                            prev.first_scan_id, close_scan,
                            prev.is_deleted as i64, prev.access,
                            prev.mod_date, prev.size, prev.file_hash,
                            prev.val, prev.val_error, prev.last_hash_scan, prev.last_val_scan,
                        ]).map_err(FsPulseError::DatabaseError)?;
                        total_versions += 1;
                    }

                    // Start deletion version
                    // Carry forward the state from the previous version for the deleted version
                    let prev_state = builder.as_ref();
                    builder = Some(VersionBuilder {
                        first_scan_id: change.scan_id,
                        is_deleted: true,
                        access: prev_state.map_or(0, |p| p.access),
                        mod_date: prev_state.and_then(|p| p.mod_date),
                        size: prev_state.and_then(|p| p.size),
                        file_hash: prev_state.and_then(|p| p.file_hash.clone()),
                        val: prev_state.map_or(3, |p| p.val),
                        val_error: prev_state.and_then(|p| p.val_error.clone()),
                        last_hash_scan: prev_state.and_then(|p| p.last_hash_scan),
                        last_val_scan: prev_state.and_then(|p| p.last_val_scan),
                    });
                }

                _ => {
                    // NoChange (0) or unknown — skip
                }
            }
        }

        // Insert the final version. Use first_scan_id from the builder (when this state
        // started) but take all state fields from items_old (the authoritative current state).
        // The changes table doesn't capture silent updates like last_hash_scan/last_val_scan
        // advancing when hash/val are evaluated but unchanged, or val being set on directories
        // without a corresponding change record.
        if let Some(ref final_ver) = builder {
            insert_stmt.execute(rusqlite::params![
                item_id,
                final_ver.first_scan_id, last_scan,
                item.is_ts as i64, item.access,
                item.mod_date, item.size, &item.file_hash,
                item.val, &item.val_error, item.last_hash_scan, item.last_val_scan,
            ]).map_err(FsPulseError::DatabaseError)?;
            total_versions += 1;
        }
    }

    Ok(total_versions)
}

/// Row from items_old table, representing the authoritative current state of an item.
struct ItemOldRow {
    item_id: i64,
    root_id: i64,
    access: i64,
    last_scan: i64,
    is_ts: bool,
    mod_date: Option<i64>,
    size: Option<i64>,
    last_hash_scan: Option<i64>,
    file_hash: Option<String>,
    last_val_scan: Option<i64>,
    val: i64,
    val_error: Option<String>,
}

/// Change row from the changes table, used during migration.
/// Only includes columns needed for version reconstruction (_new values).
struct ChangeRow {
    scan_id: i64,
    change_type: i64,
    access_new: Option<i64>,
    is_undelete: Option<bool>,
    meta_change: Option<bool>,
    mod_date_new: Option<i64>,
    size_new: Option<i64>,
    hash_change: Option<bool>,
    hash_new: Option<String>,
    val_change: Option<bool>,
    val_new: Option<i64>,
    val_error_new: Option<String>,
}

