pub const UPGRADE_12_TO_13_SQL: &str = r#"
--
-- Schema Upgrade: Version 12 → 13
--
-- This migration adds an index on changes.item_id to optimize
-- queries that look up changes for a specific item.
--

PRAGMA foreign_keys = OFF;

BEGIN TRANSACTION;

-- Verify schema version is exactly 12
SELECT 1 / (CASE WHEN (SELECT value FROM meta WHERE key = 'schema_version') = '12' THEN 1 ELSE 0 END);

-- ========================================
-- Add index for item_id lookups
-- ========================================

CREATE INDEX IF NOT EXISTS idx_changes_item ON changes (item_id);
CREATE INDEX IF NOT EXISTS idx_alerts_item ON alerts (item_id);

-- ========================================
-- Finalize migration
-- ========================================

UPDATE meta SET value = '13' WHERE key = 'schema_version';

COMMIT;

PRAGMA foreign_keys = ON;
"#;
