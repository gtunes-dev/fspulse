use log::info;
use rusqlite::Connection;

use crate::error::FsPulseError;

/// Schema Upgrade: Version 17 → 18
///
/// Adds folder descendant change counts (`add_count`, `modify_count`, `delete_count`)
/// to `item_versions`. These columns record how many descendant items were added,
/// modified, or deleted within a folder during the scan that created the version.
///
/// Phase 1 (pre-SQL):
///   - Verifies schema version is exactly 17.
///   - Adds three nullable INTEGER columns to item_versions.
///   - Sets all existing folder versions to 0 (non-folder versions remain NULL).
///
/// Phase 2 (Rust code):
///   - No-op log message. Historical backfill is handled by the v18→v19 migration.
///
/// Phase 3 (post-SQL):
///   - Updates schema version to 18.
pub const UPGRADE_17_TO_18_PRE_SQL: &str = r#"
-- Schema Upgrade: Version 17 → 18 (Pre-SQL Phase)
-- Verify schema version is exactly 17
SELECT 1 / (CASE WHEN (SELECT value FROM meta WHERE key = 'schema_version') = '17' THEN 1 ELSE 0 END);

-- Add nullable columns (default NULL — correct for non-folder items)
ALTER TABLE item_versions ADD COLUMN add_count INTEGER;
ALTER TABLE item_versions ADD COLUMN modify_count INTEGER;
ALTER TABLE item_versions ADD COLUMN delete_count INTEGER;

-- Set all existing folder versions to 0 (invariant: folders always non-null)
UPDATE item_versions SET add_count = 0, modify_count = 0, delete_count = 0
WHERE item_id IN (SELECT item_id FROM items WHERE item_type = 1);
"#;

/// Post-SQL: Update schema version.
pub const UPGRADE_17_TO_18_POST_SQL: &str = r#"
UPDATE meta SET value = '18' WHERE key = 'schema_version';
"#;

/// Rust code phase for v17→v18: just a log message.
/// Historical backfill is handled by the standalone v18→v19 migration.
pub fn migrate_17_to_18(_conn: &Connection) -> Result<(), FsPulseError> {
    info!("Migration 17→18: Columns added, existing folders set to 0. Backfill runs in v18→v19.");
    Ok(())
}
