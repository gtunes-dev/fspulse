pub const CREATE_SCHEMA_SQL: &str = r#"
BEGIN TRANSACTION;

CREATE TABLE IF NOT EXISTS meta (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

INSERT OR REPLACE INTO meta (key, value) VALUES ('schema_version', '2');

-- Roots table stores unique root directories that have been scanned
CREATE TABLE IF NOT EXISTS roots (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    path TEXT NOT NULL UNIQUE
);

-- Indexes to optimize queries
CREATE INDEX IF NOT EXISTS idx_roots_path ON roots (path);

-- Scans table tracks individual scan sessions
CREATE TABLE IF NOT EXISTS scans (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    root_id INTEGER NOT NULL,          -- Links scan to a root path
    state INTEGER NOT NULL,            -- The state of the scan (0 = Pending, 1 = Scanning, 2 = Sweeping, 3 = Analyzing, 4 = Completed, 5 = Stopped)
    hashing BOOLEAN NOT NULL,          -- Indicated the scan computes hashes for files
    validating BOOLEAN NOT NULL,       -- Indicates the scan validates file contents
    time_of_scan INTEGER NOT NULL,     -- Timestamp of when scan was performed (UTC)
    file_count INTEGER DEFAULT NULL,   -- Count of files found in the scan
    folder_count INTEGER DEFAULT NULL, -- Count of directories found in the scan
    FOREIGN KEY (root_id) REFERENCES roots(id)
);

-- Items table tracks files and directories discovered during scans
CREATE TABLE IF NOT EXISTS items (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    root_id INTEGER NOT NULL,                 -- Links each item to a root
    path TEXT NOT NULL,                       -- Relative path from the root path
    is_tombstone BOOLEAN NOT NULL DEFAULT 0,  -- Indicates if the item was deleted
    item_type CHAR(1) NOT NULL,               -- ('F' for file, 'D' for directory, 'S' for symlink, 'O' for other)
    last_modified INTEGER,                    -- Last modified timestamp
    file_size INTEGER,                        -- File size in bytes (NULL for directories)
    file_hash TEXT,                           -- Hash of file contents (NULL for directories and if not computed)
    validation_state CHAR(1) NOT NULL,        -- Validation state of file
    validation_state_desc TEXT,                     -- Description of invalid state
    last_scan_id INTEGER NOT NULL,            -- Last scan where the item was present
    last_hash_scan_id INTEGER,                -- Id of last scan during which a hash was computed
    last_validation_scan_id INTEGER,    -- Id of last scan during which file was validated
    FOREIGN KEY (root_id) REFERENCES roots(id),
    FOREIGN KEY (last_scan_id) REFERENCES scans(id),
    FOREIGN KEY (last_hash_scan_id) REFERENCES scans(id),
    FOREIGN KEY (last_validation_scan_id) REFERENCES scans(id),
    UNIQUE (root_id, path)              -- Ensures uniqueness within each root path
);

-- Indexes to optimize queries
CREATE INDEX IF NOT EXISTS idx_items_path ON items (root_id, path);
CREATE INDEX IF NOT EXISTS idx_items_scan ON items (root_id, last_scan_id, is_tombstone);

-- Changes table tracks modifications between scans
CREATE TABLE IF NOT EXISTS changes (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    scan_id INTEGER NOT NULL,                       -- The scan in which the change was detected
    item_id INTEGER NOT NULL,                       -- The file or directory that changed
    change_type CHAR(1) NOT NULL,                   -- ('A' for added, 'D' for deleted, 'M' for modified, 'T' for type changed)
    prev_last_modified INTEGER DEFAULT NULL,        -- Stores the previous last_modified timestamp (if changed)
    prev_file_size INTEGER DEFAULT NULL,            -- Stores the previous file_size (if changed)
    prev_hash TEXT DEFAULT NULL,                    -- Stores the previous hash value (if changed)
    prev_validation_state CHAR(1) DEFAULT NULL,     -- Stores the previous validation state (if changed)
    prev_validation_state_desc DEFAULT NULL,        -- Stores the previous validation description (if changed)
    FOREIGN KEY (scan_id) REFERENCES scans(id),
    FOREIGN KEY (item_id) REFERENCES items(id),
    UNIQUE (scan_id, item_id, change_type)
);

COMMIT;
"#;
