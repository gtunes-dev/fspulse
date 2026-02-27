pub const UPGRADE_2_TO_3_SQL: &str = r#"
--
-- Schema Upgrade: Version 2 → 3
--
-- This migration modifies the 'scans' table to add hash_all/val_all flags
-- and renames 'hashing' → 'is_hash' and 'validating' → 'is_val'.
--
-- Following SQLite's official guidance:
-- https://www.sqlite.org/lang_altertable.html
--
-- We avoid renaming the original table. Instead, we:
-- 1. Create a new version of 'scans' as 'new_scans'.
-- 2. Copy the old contents into 'new_scans'.
-- 3. Drop the original 'scans'.
-- 4. Rename 'new_scans' to 'scans'.
--
-- This preserves foreign key relationships in dependent tables like 'items' and 'changes'.
--

-- Verify schema version is exactly 2
SELECT 1 / (CASE WHEN (SELECT value FROM meta WHERE key = 'schema_version') = '2' THEN 1 ELSE 0 END);

-- Create new_scans table with updated schema
CREATE TABLE new_scans (
    scan_id INTEGER PRIMARY KEY AUTOINCREMENT,
    root_id INTEGER NOT NULL,
    state INTEGER NOT NULL,
    is_hash BOOLEAN NOT NULL,
    hash_all BOOLEAN NOT NULL,
    is_val BOOLEAN NOT NULL,
    val_all BOOLEAN NOT NULL,
    scan_time INTEGER NOT NULL,
    file_count INTEGER DEFAULT NULL,
    folder_count INTEGER DEFAULT NULL,
    FOREIGN KEY (root_id) REFERENCES roots(root_id)
);

-- Copy data from old scans table into new_scans
INSERT INTO new_scans (
    scan_id, root_id, state,
    is_hash, hash_all,
    is_val, val_all,
    scan_time, file_count, folder_count
)
SELECT
    scan_id, root_id, state,
    hashing, hashing,
    validating, validating,
    scan_time, file_count, folder_count
FROM scans;

-- Drop old scans table
DROP TABLE scans;

-- Rename new_scans to scans
ALTER TABLE new_scans RENAME TO scans;

-- Recreate any indexes if needed (none currently defined on scans)

-- Update schema version
UPDATE meta SET value = '3' WHERE key = 'schema_version';

-- Update sqlite_sequence to preserve AUTOINCREMENT
UPDATE sqlite_sequence SET seq = (SELECT MAX(scan_id) FROM scans) WHERE name = 'scans';
"#;
