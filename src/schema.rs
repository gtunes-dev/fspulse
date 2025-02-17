pub const CREATE_SCHEMA_SQL: &str = r#"
BEGIN TRANSACTION;

CREATE TABLE IF NOT EXISTS meta (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

INSERT OR REPLACE INTO meta (key, value) VALUES ('schema_version', '2');

-- Root paths table stores unique root directories that have been scanned
CREATE TABLE IF NOT EXISTS root_paths (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    path TEXT NOT NULL UNIQUE
);

-- Scans table tracks individual scan sessions
CREATE TABLE IF NOT EXISTS scans (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    root_path_id INTEGER NOT NULL,     -- Links scan to a root path
    is_deep BOOLEAN NOT NULL,          -- Indicates if this scan included hash computation
    time_of_scan INTEGER NOT NULL,     -- Timestamp of when scan was performed (UTC)
    file_count INTEGER DEFAULT NULL,   -- Count of files found in the scan
    folder_count INTEGER DEFAULT NULL, -- Count of directories found in the scan
    is_complete BOOLEAN NOT NULL DEFAULT 0,  -- Whether the scan fully completed
    FOREIGN KEY (root_path_id) REFERENCES root_paths(id)
);

-- Entries table tracks files and directories discovered during scans
CREATE TABLE IF NOT EXISTS entries (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    root_path_id INTEGER NOT NULL,    -- Links each entry to a root path
    path TEXT NOT NULL,               -- Relative path from the root path
    is_tombstone BOOLEAN NOT NULL DEFAULT 0,  -- Indicates if entry was deleted
    item_type CHAR(1) NOT NULL,       -- ('F' for file, 'D' for directory, 'S' for symlink, 'O' for other)
    last_modified INTEGER,            -- Last modified timestamp
    file_size INTEGER,                -- File size in bytes (NULL for directories)
    file_hash TEXT,                    -- Hash of file contents (NULL for directories and if not computed)
    last_seen_scan_id INTEGER NOT NULL, -- Last scan where the entry was present
    FOREIGN KEY (root_path_id) REFERENCES root_paths(id),
    FOREIGN KEY (last_seen_scan_id) REFERENCES scans(id),
    UNIQUE (root_path_id, path)        -- Ensures uniqueness within each root path
);

-- Indexes to optimize queries
CREATE INDEX IF NOT EXISTS idx_entries_path ON entries (root_path_id, path);
CREATE INDEX IF NOT EXISTS idx_entries_scan ON entries (root_path_id, last_seen_scan_id, is_tombstone);

-- Changes table tracks modifications between scans
CREATE TABLE IF NOT EXISTS changes (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    scan_id INTEGER NOT NULL,          -- The scan in which the change was detected
    entry_id INTEGER NOT NULL,         -- The file or directory that changed
    change_type CHAR(1) NOT NULL,      -- ('A' for added, 'D' for deleted, 'M' for modified, 'T' for type changed)
    metadata_changed BOOLEAN DEFAULT NULL,  -- Indicates if metadata changed
    hash_changed BOOLEAN DEFAULT NULL,      -- Indicates if file contents changed
    prev_metadata INTEGER DEFAULT NULL,     -- Stores the previous last_modified timestamp (if applicable)
    prev_hash TEXT DEFAULT NULL,            -- Stores the previous hash value (if applicable)
    FOREIGN KEY (scan_id) REFERENCES scans(id),
    FOREIGN KEY (entry_id) REFERENCES entries(id)
);

COMMIT;
"#;
