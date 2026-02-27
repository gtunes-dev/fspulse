pub const UPGRADE_5_TO_6_SQL: &str = r#"
--
-- Schema Upgrade: Version 5 → 6
--
-- This migration adds denormalized count columns to the scans table:
-- - total_file_size: Sum of all file sizes seen in the scan (left NULL for historical scans)
-- - alert_count: Number of alerts created during the scan
-- - add_count, modify_count, delete_count: Counts of each change type
--
-- It also ensures file_count and folder_count are NULL for incomplete scans.
--

-- Verify schema version is exactly 5
SELECT 1 / (CASE WHEN (SELECT value FROM meta WHERE key = 'schema_version') = '5' THEN 1 ELSE 0 END);

-- Add new columns to scans table (all default to NULL)
ALTER TABLE scans ADD COLUMN total_file_size INTEGER DEFAULT NULL;
ALTER TABLE scans ADD COLUMN alert_count INTEGER DEFAULT NULL;
ALTER TABLE scans ADD COLUMN add_count INTEGER DEFAULT NULL;
ALTER TABLE scans ADD COLUMN modify_count INTEGER DEFAULT NULL;
ALTER TABLE scans ADD COLUMN delete_count INTEGER DEFAULT NULL;

-- For completed scans (state = 4), compute alert and change counts from persistent tables
UPDATE scans SET
  alert_count = (SELECT COUNT(*) FROM alerts WHERE alerts.scan_id = scans.scan_id),
  add_count = (SELECT COUNT(*) FROM changes WHERE changes.scan_id = scans.scan_id AND change_type = 'A'),
  modify_count = (SELECT COUNT(*) FROM changes WHERE changes.scan_id = scans.scan_id AND change_type = 'M'),
  delete_count = (SELECT COUNT(*) FROM changes WHERE changes.scan_id = scans.scan_id AND change_type = 'D')
WHERE state = 4;

-- For incomplete scans (state != 4), NULL out file_count and folder_count
UPDATE scans SET
  file_count = NULL,
  folder_count = NULL
WHERE state != 4;

-- Update schema version
UPDATE meta SET value = '6' WHERE key = 'schema_version';
"#;
