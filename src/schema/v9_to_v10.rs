pub const UPGRADE_9_TO_10_SQL: &str = r#"
--
-- Schema Upgrade: Version 9 → 10
--
-- This migration generalizes file_size columns to size columns to support
-- storing computed folder sizes in addition to file sizes.
--
-- Changes:
-- - items.file_size → items.size (now stores size for both files and directories)
-- - changes.file_size_old → changes.size_old
-- - changes.file_size_new → changes.size_new
-- - scans.total_file_size → scans.total_size (now includes all sizes, not just files)
--

-- Verify schema version is exactly 9
SELECT 1 / (CASE WHEN (SELECT value FROM meta WHERE key = 'schema_version') = '9' THEN 1 ELSE 0 END);

-- ========================================
-- Rename columns
-- ========================================

-- Items table: file_size → size
ALTER TABLE items RENAME COLUMN file_size TO size;

-- Changes table: file_size_old → size_old, file_size_new → size_new
ALTER TABLE changes RENAME COLUMN file_size_old TO size_old;
ALTER TABLE changes RENAME COLUMN file_size_new TO size_new;

-- Scans table: total_file_size → total_size
ALTER TABLE scans RENAME COLUMN total_file_size TO total_size;

-- ========================================
-- Finalize migration
-- ========================================

UPDATE meta SET value = '10' WHERE key = 'schema_version';
"#;
