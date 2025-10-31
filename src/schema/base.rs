pub const CREATE_SCHEMA_SQL: &str = r#"
BEGIN TRANSACTION;

CREATE TABLE IF NOT EXISTS meta (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

INSERT OR REPLACE INTO meta (key, value) VALUES ('schema_version', '7');

-- Roots table stores unique root directories that have been scanned
CREATE TABLE IF NOT EXISTS roots (
    root_id INTEGER PRIMARY KEY AUTOINCREMENT,
    root_path TEXT NOT NULL UNIQUE
);

-- Indexes to optimize queries
CREATE INDEX IF NOT EXISTS idx_roots_path ON roots (root_path);

-- Scans table tracks individual scan sessions
CREATE TABLE IF NOT EXISTS scans (
    scan_id INTEGER PRIMARY KEY AUTOINCREMENT,
    root_id INTEGER NOT NULL,          -- Links scan to a root path
    state INTEGER NOT NULL,            -- The state of the scan (1 = Scanning, 2 = Sweeping, 3 = Analyzing, 4 = Completed, 5 = Stopped, 6 = Error)
    is_hash BOOLEAN NOT NULL,     -- Hash new or changed files
    hash_all BOOLEAN NOT NULL,       -- Hash all items including unchanged and previously hashed
    is_val BOOLEAN NOT NULL,      -- Validate the contents of files
    val_all BOOLEAN NOT NULL,        -- Validate all items including unchanged and previously validated
    scan_time INTEGER NOT NULL,        -- Timestamp of when scan was performed (UTC)
    file_count INTEGER DEFAULT NULL,   -- Count of files found in the scan
    folder_count INTEGER DEFAULT NULL, -- Count of directories found in the scan
    total_file_size INTEGER DEFAULT NULL, -- Total size of all files seen in the scan
    alert_count INTEGER DEFAULT NULL,  -- Count of alerts created during the scan
    add_count INTEGER DEFAULT NULL,    -- Count of items added in the scan
    modify_count INTEGER DEFAULT NULL, -- Count of items modified in the scan
    delete_count INTEGER DEFAULT NULL, -- Count of items deleted in the scan
    error TEXT DEFAULT NULL,           -- Error message if scan failed
    FOREIGN KEY (root_id) REFERENCES roots(root_id)
);

-- Items table tracks files and directories discovered during scans
CREATE TABLE IF NOT EXISTS items (
    item_id INTEGER PRIMARY KEY AUTOINCREMENT,
    root_id INTEGER NOT NULL,                   -- Links each item to a root
    item_path TEXT NOT NULL,                         -- Absolute path of the item
    item_type INTEGER NOT NULL,                 -- Item type enum (0=File, 1=Directory, 2=Symlink, 3=Other)

    last_scan INTEGER NOT NULL,              -- Last scan where the item was present
    is_ts BOOLEAN NOT NULL DEFAULT 0,       -- Indicates if the item is a tombstone

    -- Medatadata Property Group
    mod_date INTEGER,                           -- Last mod_date timestamp
    file_size INTEGER,                          -- File size in bytes (NULL for directories)

    -- Hash Property Group
    last_hash_scan INTEGER,                  -- Id of last scan during which a hash was computed
    file_hash TEXT,                          -- Hash of file contents (NULL for directories and if not computed)

    -- Validation Property Group
    last_val_scan INTEGER,                  -- Id of last scan during which file was validated
    val INTEGER NOT NULL,                   -- Validation state enum (0=Valid, 1=Invalid, 2=NoValidator, 3=Unknown)
    val_error TEXT,                         -- Description of invalid state

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
    change_type INTEGER NOT NULL,               -- Change type enum (0=Add, 1=Modify, 2=Delete, 3=NoChange)

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
    last_hash_scan_old INTEGER DEFAULT NULL,   -- Id of last scan during which a hash was computed
    hash_old TEXT DEFAULT NULL,                 -- Stores the previous hash value (if changed)
    hash_new TEXT DEFAULT NULL,                 -- Stores the new hash (if changed)

    -- Validation Properties (Add or Modify)
    val_change BOOLEAN DEFAULT NULL,        -- Not Null if "A" or "M", True if hash changed
    last_val_scan_old INTEGER DEFAULT NULL,  -- Id of last scan during which validation was done
    val_old INTEGER DEFAULT NULL,           -- Stores the previous validation state enum (if changed)
    val_new INTEGER DEFAULT NULL,           -- If the validation state changes, current state is stored here
    val_error_old DEFAULT NULL,             -- Stores the previous validation error (if changed)
    val_error_new DEFAULT NULL,             -- Stores the new validation error (if changed)

    FOREIGN KEY (scan_id) REFERENCES scans(scan_id),
    FOREIGN KEY (item_id) REFERENCES items(item_id),
    UNIQUE (scan_id, item_id)
);

-- Indexes to optimize queries
CREATE INDEX IF NOT EXISTS idx_changes_scan_type ON changes (scan_id, change_type);

CREATE TABLE IF NOT EXISTS alerts (
  alert_id INTEGER PRIMARY KEY AUTOINCREMENT,
  alert_type INTEGER NOT NULL,              -- Alert type enum (0=SuspiciousHash, 1=InvalidItem)
  alert_status INTEGER NOT NULL,            -- Alert status enum (0=Open, 1=Flagged, 2=Dismissed)
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


COMMIT;
"#;
