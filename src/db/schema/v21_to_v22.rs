use log::info;
use rusqlite::Connection;

use crate::error::FsPulseError;

/// Schema Upgrade: Version 21 → 22
///
/// Adds `unchanged_count` to `item_versions`, completing the four-count
/// descriptor for folder descendant changes: add, modify, delete, unchanged.
///
/// With all four counts the tuple is self-describing:
///   - Total alive descendants = add_count + modify_count + unchanged_count
///   - delete_count tracks descendants that became not-alive in that scan
///
/// Phase 1 (pre-SQL):
///   - Verifies schema version is exactly 21.
///   - Adds nullable INTEGER column `unchanged_count` to item_versions.
///   - Sets all existing folder versions to 0 (non-folder versions remain NULL).
///
/// Phase 2 (Rust code):
///   - No-op log message. Historical backfill is handled by the v22→v23 migration.
///
/// Phase 3 (post-SQL):
///   - Updates schema version to 22.
pub const UPGRADE_21_TO_22_PRE_SQL: &str = r#"
-- Schema Upgrade: Version 21 → 22 (Pre-SQL Phase)
-- Verify schema version is exactly 21
SELECT 1 / (CASE WHEN (SELECT value FROM meta WHERE key = 'schema_version') = '21' THEN 1 ELSE 0 END);

-- Add nullable column (default NULL — correct for non-folder items)
ALTER TABLE item_versions ADD COLUMN unchanged_count INTEGER;

-- Set all existing folder versions to 0 (invariant: folders always non-null)
UPDATE item_versions SET unchanged_count = 0
WHERE item_id IN (SELECT item_id FROM items WHERE item_type = 1);
"#;

/// Post-SQL: Update schema version.
pub const UPGRADE_21_TO_22_POST_SQL: &str = r#"
UPDATE meta SET value = '22' WHERE key = 'schema_version';
"#;

/// Rust code phase for v21→v22: just a log message.
/// Historical backfill is handled by the standalone v22→v23 migration.
pub fn migrate_21_to_22(_conn: &Connection) -> Result<(), FsPulseError> {
    info!("Migration 21→22: unchanged_count column added, existing folders set to 0. Backfill runs in v22→v23.");
    Ok(())
}
