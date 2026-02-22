use crate::error::FsPulseError;
use log::{info, warn};
use rusqlite::Connection;

/// Schema Upgrade: Version 16 → 17
///
/// Phase 1 (pre-SQL):
///   - Verifies schema version is exactly 16.
///
/// Phase 2 (Rust code):
///   - Validates that the temporal model (`items` + `item_versions`) is consistent
///     with the old model (`items_old` + `changes`). Logs warnings for any
///     discrepancies but does NOT fail the migration.
///
/// Phase 3 (post-SQL):
///   - Drops `items_old`, `changes`, and their indexes.
///   - Updates schema version to 17.
pub const UPGRADE_16_TO_17_PRE_SQL: &str = r#"
-- Schema Upgrade: Version 16 → 17 (Pre-SQL Phase)
-- Verify schema version is exactly 16
SELECT 1 / (CASE WHEN (SELECT value FROM meta WHERE key = 'schema_version') = '16' THEN 1 ELSE 0 END);
"#;

/// Rust code phase: Validate old-vs-new model consistency before dropping old tables.
/// All checks log warnings but never return errors — the migration always proceeds.
pub fn migrate_16_to_17(conn: &Connection) -> Result<(), FsPulseError> {
    info!("Migration 16→17: Validating temporal model before dropping old tables...");

    let mut issues = 0;

    // Check 1: Every item in items_old should have a corresponding identity in items
    issues += check_old_items_have_identities(conn);

    // Check 2: Every item in items should have at least one version
    issues += check_items_have_versions(conn);

    // Check 3: For each item, the latest version's is_deleted should match items_old.is_ts
    issues += check_deleted_state_matches(conn);

    // Check 4: Item counts should match between old and new model
    issues += check_item_counts_match(conn);

    // Check 5: Total version count should be >= item count (every item needs at least one version)
    issues += check_version_count(conn);

    if issues == 0 {
        info!("Migration 16→17: Validation passed — no discrepancies found.");
    } else {
        warn!(
            "Migration 16→17: Validation completed with {} issue(s). \
             Old tables will still be dropped. Review logs above for details.",
            issues
        );
    }

    Ok(())
}

/// Post-SQL: Drop old tables and update schema version.
pub const UPGRADE_16_TO_17_POST_SQL: &str = r#"
-- Drop indexes on old tables
DROP INDEX IF EXISTS idx_items_old_path;
DROP INDEX IF EXISTS idx_items_old_root_path;
DROP INDEX IF EXISTS idx_items_root_scan;
DROP INDEX IF EXISTS idx_changes_scan_type;
DROP INDEX IF EXISTS idx_changes_item;

-- Drop old tables
DROP TABLE IF EXISTS changes;
DROP TABLE IF EXISTS items_old;

-- Update schema version
UPDATE meta SET value = '17' WHERE key = 'schema_version';
"#;

// ============================================================================
// Validation helpers — each returns the number of issues found
// ============================================================================

/// Check that every item_id in items_old has a matching row in items.
fn check_old_items_have_identities(conn: &Connection) -> usize {
    let result: Result<i64, _> = conn.query_row(
        "SELECT COUNT(*) FROM items_old o
         WHERE NOT EXISTS (SELECT 1 FROM items i WHERE i.item_id = o.item_id)",
        [],
        |row| row.get(0),
    );

    match result {
        Ok(0) => {
            info!("  [OK] All items_old rows have matching identities in items.");
            0
        }
        Ok(n) => {
            warn!(
                "  [WARN] {} item(s) in items_old have no matching identity in items.",
                n
            );
            1
        }
        Err(e) => {
            warn!("  [WARN] Failed to check old item identities: {}", e);
            1
        }
    }
}

/// Check that every item in items has at least one version in item_versions.
fn check_items_have_versions(conn: &Connection) -> usize {
    let result: Result<i64, _> = conn.query_row(
        "SELECT COUNT(*) FROM items i
         WHERE NOT EXISTS (SELECT 1 FROM item_versions v WHERE v.item_id = i.item_id)",
        [],
        |row| row.get(0),
    );

    match result {
        Ok(0) => {
            info!("  [OK] All items have at least one version.");
            0
        }
        Ok(n) => {
            warn!(
                "  [WARN] {} item(s) in items have no versions in item_versions.",
                n
            );
            1
        }
        Err(e) => {
            warn!("  [WARN] Failed to check item versions: {}", e);
            1
        }
    }
}

/// Check that the latest version's is_deleted flag matches items_old.is_ts for each item.
fn check_deleted_state_matches(conn: &Connection) -> usize {
    let result: Result<i64, _> = conn.query_row(
        "SELECT COUNT(*) FROM items_old o
         JOIN item_versions v ON v.item_id = o.item_id
         WHERE v.last_scan_id = (
             SELECT MAX(v2.last_scan_id) FROM item_versions v2 WHERE v2.item_id = o.item_id
         )
         AND v.is_deleted != o.is_ts",
        [],
        |row| row.get(0),
    );

    match result {
        Ok(0) => {
            info!("  [OK] Deleted state matches between old and new model for all items.");
            0
        }
        Ok(n) => {
            warn!(
                "  [WARN] {} item(s) have mismatched deleted state between items_old.is_ts and latest version.",
                n
            );
            1
        }
        Err(e) => {
            warn!("  [WARN] Failed to check deleted state consistency: {}", e);
            1
        }
    }
}

/// Check that the total item count matches between items_old and items.
fn check_item_counts_match(conn: &Connection) -> usize {
    let old_count: Result<i64, _> = conn.query_row(
        "SELECT COUNT(*) FROM items_old",
        [],
        |row| row.get(0),
    );
    let new_count: Result<i64, _> = conn.query_row(
        "SELECT COUNT(*) FROM items",
        [],
        |row| row.get(0),
    );

    match (old_count, new_count) {
        (Ok(old), Ok(new)) if old == new => {
            info!("  [OK] Item counts match: {} in both tables.", old);
            0
        }
        (Ok(old), Ok(new)) => {
            warn!(
                "  [WARN] Item count mismatch: items_old has {}, items has {}.",
                old, new
            );
            1
        }
        (Err(e), _) | (_, Err(e)) => {
            warn!("  [WARN] Failed to check item counts: {}", e);
            1
        }
    }
}

/// Check that every item has at least one version (version_count >= item_count).
fn check_version_count(conn: &Connection) -> usize {
    let item_count: Result<i64, _> = conn.query_row(
        "SELECT COUNT(*) FROM items",
        [],
        |row| row.get(0),
    );
    let version_count: Result<i64, _> = conn.query_row(
        "SELECT COUNT(*) FROM item_versions",
        [],
        |row| row.get(0),
    );

    match (item_count, version_count) {
        (Ok(items), Ok(versions)) if versions >= items => {
            info!(
                "  [OK] Version count ({}) >= item count ({}).",
                versions, items
            );
            0
        }
        (Ok(items), Ok(versions)) => {
            warn!(
                "  [WARN] Version count ({}) is less than item count ({}).",
                versions, items
            );
            1
        }
        (Err(e), _) | (_, Err(e)) => {
            warn!("  [WARN] Failed to check version counts: {}", e);
            1
        }
    }
}
