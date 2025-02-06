pub const CREATE_SCHEMA_SQL: &str = r#"
BEGIN TRANSACTION;

CREATE TABLE IF NOT EXISTS meta (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

INSERT OR REPLACE INTO meta (key, value) VALUES ('schema_version', '1');

CREATE TABLE IF NOT EXISTS root_paths (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    path TEXT NOT NULL UNIQUE
);

CREATE TABLE IF NOT EXISTS scans (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    root_path_id INTEGER NOT NULL,
    scan_time INTEGER NOT NULL,
    FOREIGN KEY (root_path_id) REFERENCES root_paths(id)
);

CREATE TABLE IF NOT EXISTS entries (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    path TEXT NOT NULL UNIQUE,
    is_directory BOOLEAN NOT NULL,
    last_modified INTEGER,
    file_size INTEGER,
    last_seen_scan_id INTEGER NOT NULL, -- Tracks the last scan where the file was seen
    FOREIGN KEY (last_seen_scan_id) REFERENCES scans(id)
);

CREATE TABLE IF NOT EXISTS changes (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    scan_id INTEGER NOT NULL,
    entry_id INTEGER NOT NULL,
    change_type CHAR(1) NOT NULL, -- Single-character storage for change type ('A', 'D', 'C')
    FOREIGN KEY (scan_id) REFERENCES scans(id),
    FOREIGN KEY (entry_id) REFERENCES entries(id)
);

COMMIT;
"#;
