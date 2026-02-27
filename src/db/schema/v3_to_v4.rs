pub const UPGRADE_3_TO_4_SQL: &str = r#"
--
-- Schema Upgrade: Version 3 → 4
--
-- This migration creates the alerts table to support the new alerts feature
--

-- Verify schema version is exactly 3
SELECT 1 / (CASE WHEN (SELECT value FROM meta WHERE key = 'schema_version') = '3' THEN 1 ELSE 0 END);

-- Create the alerts table
CREATE TABLE alerts (
  alert_id INTEGER PRIMARY KEY AUTOINCREMENT,
  alert_type CHAR(1) NOT NULL,
  alert_status CHAR(1) NOT NULL,
  scan_id INTEGER NOT NULL,
  item_id INTEGER NOT NULL,
  created_at INTEGER NOT NULL,
  updated_at INTEGER DEFAULT NULL,

  -- suspicious hash
  prev_hash_scan INTEGER DEFAULT NULL,
  hash_old TEXT DEFAULT NULL,
  hash_new TEXT DEFAULT NULL,

  -- invalid file
  val_error TEXT DEFAULT NULL
);

-- Update schema version
UPDATE meta SET value = '4' WHERE key = 'schema_version';
"#;
