pub const UPGRADE_4_TO_5_SQL: &str = r#"
--
-- Schema Upgrade: Version 4 → 5
--
-- This migration adds error tracking to the scans table
--

-- Verify schema version is exactly 4
SELECT 1 / (CASE WHEN (SELECT value FROM meta WHERE key = 'schema_version') = '4' THEN 1 ELSE 0 END);

-- Add error column to scans table
ALTER TABLE scans ADD COLUMN error TEXT DEFAULT NULL;

-- Update schema version
UPDATE meta SET value = '5' WHERE key = 'schema_version';
"#;
