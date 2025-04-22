pub const CREATE_SCHEMA_SQL: &str = r#"
BEGIN TRANSACTION;

CREATE TABLE IF NOT EXISTS meta (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

INSERT OR REPLACE INTO meta (key, value) VALUES ('schema_version', '3');

-- Roots table stores unique root directories that have been scanned
CREATE TABLE IF NOT EXISTS roots (
    root_id INTEGER PRIMARY KEY AUTOINCREMENT,
    root_path TEXT NOT NULL UNIQUE
);

-- Indexes to optimize queries
CREATE INDEX IF NOT EXISTS idx_roots_path ON roots (root_path);

-- Scans table tracks individual scan sessions
-- Scans table tracks individual scan sessions
CREATE TABLE IF NOT EXISTS scans (
    scan_id INTEGER PRIMARY KEY AUTOINCREMENT,
    root_id INTEGER NOT NULL,          -- Links scan to a root path
    state INTEGER NOT NULL,            -- The state of the scan (0 = Pending, 1 = Scanning, 2 = Sweeping, 3 = Analyzing, 4 = Completed, 5 = Stopped)
    is_hash BOOLEAN NOT NULL,     -- Hash new or changed files
    hash_all BOOLEAN NOT NULL,       -- Hash all items including unchanged and previously hashed
    is_val BOOLEAN NOT NULL,      -- Validate the contents of files
    val_all BOOLEAN NOT NULL,        -- Validate all items including unchanged and previously validated
    scan_time INTEGER NOT NULL,        -- Timestamp of when scan was performed (UTC)
    file_count INTEGER DEFAULT NULL,   -- Count of files found in the scan
    folder_count INTEGER DEFAULT NULL, -- Count of directories found in the scan
    FOREIGN KEY (root_id) REFERENCES roots(root_id)
);

-- Items table tracks files and directories discovered during scans
CREATE TABLE IF NOT EXISTS items (
    item_id INTEGER PRIMARY KEY AUTOINCREMENT,
    root_id INTEGER NOT NULL,                   -- Links each item to a root
    item_path TEXT NOT NULL,                         -- Absolute path of the item
    item_type CHAR(1) NOT NULL,                 -- ('F' for file, 'D' for directory, 'S' for symlink, 'O' for other)

    last_scan INTEGER NOT NULL,              -- Last scan where the item was present
    is_ts BOOLEAN NOT NULL DEFAULT 0,       -- Indicates if the item is a tombstone
    
    -- Medatadata Property Group
    mod_date INTEGER,                           -- Last mod_date timestamp
    file_size INTEGER,                          -- File size in bytes (NULL for directories)

    -- Hash Property Group
    last_hash_scan INTEGER,                  -- Id of last scan during which a hash was computed
    file_hash TEXT,                             -- Hash of file contents (NULL for directories and if not computed)

    -- Validation Property Group
    last_val_scan INTEGER,                  -- Id of last scan during which file was validated
    val CHAR(1) NOT NULL,                   -- Validation state of file
    val_error TEXT,                          -- Description of invalid state

    FOREIGN KEY (root_id) REFERENCES roots(root_id),
    FOREIGN KEY (last_scan) REFERENCES scans(scan_id),
    FOREIGN KEY (last_hash_scan) REFERENCES scans(scan_id),
    FOREIGN KEY (last_val_scan) REFERENCES scans(scan_id),
    UNIQUE (root_id, item_path, item_type)           -- Ensures uniqueness within each root path
);

-- Indexes to optimize queries
CREATE INDEX IF NOT EXISTS idx_items_path ON items (root_id, item_path, item_type);
CREATE INDEX IF NOT EXISTS idx_items_scan ON items (root_id, last_scan, is_ts);

-- Changes table tracks modifications between scans
CREATE TABLE IF NOT EXISTS changes (
    change_id INTEGER PRIMARY KEY AUTOINCREMENT,
    scan_id INTEGER NOT NULL,                   -- The scan in which the change was detected
    item_id INTEGER NOT NULL,                   -- The file or directory that changed
    change_type CHAR(1) NOT NULL,               -- ('A' for added, 'D' for deleted, 'M' for mod_date)

    -- Add specific properties
    is_undelete BOOLEAN DEFAULT NULL,           -- Not Null if "A". True if item was tombstone

    -- Metadata Changed (Modify)
    meta_change BOOLEAN DEFAULT NULL,      -- Not Null if "M". True if metadata changed
    mod_date_old INTEGER DEFAULT NULL,          -- Stores the previous mod_date timestamp (if changed)
    mod_date_new INTEGER DEFAULT NULL,          -- Stores the new mod_date timestamp (if changed)
    file_size_old INTEGER DEFAULT NULL,         -- Stores the previous file_size (if changed)
    file_size_new INTEGER DEFAULT NULL,         -- Stores the new file_size (if changed)

    -- Hash Properties (Add, Modify)
    hash_change BOOLEAN DEFAULT NULL,          -- Not Null if "A" or "M". True if hash changed
    last_hash_scan_old INTEGER DEFAULT NULL, -- Id of last scan during which a hash was computed
    hash_old TEXT DEFAULT NULL,                 -- Stores the previous hash value (if changed)
    hash_new TEXT DEFAULT NULL,                 -- Stores the new hash (if changed)

    -- Validation Properties (Add or Modify)
    val_change BOOLEAN DEFAULT NULL,        -- Not Null if "A" or "M", True if hash changed
    last_val_scan_old INTEGER DEFAULT NULL,  -- Id of last scan during which validation was done
    val_old CHAR(1) DEFAULT NULL,           -- Stores the previous validation state (if changed)
    val_new CHAR(1) DEFAULT NULL,           -- If the validation state changes, current state is stored here
    val_error_old DEFAULT NULL,             -- Stores the previous validation error (if changed)
    val_error_new DEFAULT NULL,             -- Stores the new validation error (if changed)

    FOREIGN KEY (scan_id) REFERENCES scans(scan_id),
    FOREIGN KEY (item_id) REFERENCES items(item_id),
    UNIQUE (scan_id, item_id)
);

-- Indexes to optimize queries
CREATE INDEX IF NOT EXISTS idx_changes_scan_type ON changes (scan_id, change_type);

COMMIT;
"#;

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
-- 1. Disable foreign keys BEFORE the transaction.
-- 2. Create a new version of 'scans' as 'new_scans'.
-- 3. Copy the old contents into 'new_scans'.
-- 4. Drop the original 'scans'.
-- 5. Rename 'new_scans' to 'scans'.
--
-- This preserves foreign key relationships in dependent tables like 'items' and 'changes'.
--
-- Disable foreign key constraints BEFORE transaction starts
PRAGMA foreign_keys = OFF;

BEGIN TRANSACTION;

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

COMMIT;

-- Re-enable foreign key constraints
PRAGMA foreign_keys = ON;
"#;
