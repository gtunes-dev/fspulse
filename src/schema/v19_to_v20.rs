/// Schema Upgrade: Version 19 → 20
///
/// Adds `is_added` boolean column to `item_versions`. This column is `true`
/// when the version represents a newly discovered item (no prior version) or
/// an item reappearing after deletion (prior version had `is_deleted = 1`).
///
/// Phase 1 (pre-SQL):
///   - Verifies schema version is exactly 19.
///   - Adds `is_added BOOLEAN NOT NULL DEFAULT 0` to item_versions.
///   - Backfills: sets `is_added = 1` for every version whose previous version
///     is either absent or deleted.
///
/// Phase 2 (post-SQL):
///   - Updates schema version to 20.
pub const UPGRADE_19_TO_20_PRE_SQL: &str = r#"
-- Schema Upgrade: Version 19 → 20 (Pre-SQL Phase)
-- Verify schema version is exactly 19
SELECT 1 / (CASE WHEN (SELECT value FROM meta WHERE key = 'schema_version') = '19' THEN 1 ELSE 0 END);

-- Add is_added column (default 0 = not an add)
ALTER TABLE item_versions ADD COLUMN is_added BOOLEAN NOT NULL DEFAULT 0;

-- Backfill: mark versions that are adds (no previous version, or previous was deleted)
UPDATE item_versions SET is_added = 1
WHERE version_id IN (
    SELECT cv.version_id
    FROM item_versions cv
    LEFT JOIN item_versions pv
        ON pv.item_id = cv.item_id
        AND pv.first_scan_id = (
            SELECT MAX(first_scan_id)
            FROM item_versions
            WHERE item_id = cv.item_id
              AND first_scan_id < cv.first_scan_id
        )
    WHERE pv.version_id IS NULL OR pv.is_deleted = 1
);
"#;

/// Post-SQL: Update schema version.
pub const UPGRADE_19_TO_20_POST_SQL: &str = r#"
UPDATE meta SET value = '20' WHERE key = 'schema_version';
"#;
