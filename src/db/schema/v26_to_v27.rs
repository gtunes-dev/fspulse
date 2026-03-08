// ============================================================================
// Schema Upgrade: Version 26 → 27
//
// 1. Converts hash columns from hex TEXT to binary BLOB storage.
//    Affected columns:
//      - item_versions.file_hash: TEXT → BLOB
//      - alerts.hash_old: TEXT → BLOB
//      - alerts.hash_new: TEXT → BLOB
//    Both tables are rebuilt (rename → create new → INSERT...SELECT with UNHEX →
//    drop old) to change the declared column type. Indexes are recreated.
//    UNHEX() requires SQLite 3.38.0+ (bundled with rusqlite).
//
// 2. Drops redundant idx_items_path index. All queries that filter/sort on
//    item_path also filter on root_id, so idx_items_root_path covers them.
// ============================================================================

pub const UPGRADE_26_TO_27_PRE_SQL: &str = r#"
-- ============================================================
-- Rebuild item_versions: file_hash TEXT → BLOB
-- ============================================================

ALTER TABLE item_versions RENAME TO item_versions_old;

CREATE TABLE item_versions (
    version_id      INTEGER PRIMARY KEY AUTOINCREMENT,
    item_id         INTEGER NOT NULL,
    first_scan_id   INTEGER NOT NULL,
    last_scan_id    INTEGER NOT NULL,

    -- Shared fields (all item types)
    is_added        BOOLEAN NOT NULL DEFAULT 0,
    is_deleted      BOOLEAN NOT NULL DEFAULT 0,
    access          INTEGER NOT NULL DEFAULT 0,
    mod_date        INTEGER,
    size            INTEGER,

    -- Validation fields (NULL for folders)
    last_val_scan   INTEGER,
    val_state       INTEGER,
    val_error       TEXT,

    -- Hash fields (NULL for folders)
    last_hash_scan  INTEGER,
    file_hash       BLOB,
    hash_state      INTEGER,

    -- Folder-specific descendant change counts (NULL for files)
    add_count       INTEGER,
    modify_count    INTEGER,
    delete_count    INTEGER,
    unchanged_count INTEGER,

    -- Folder-specific descendant state snapshot counts (NULL for files)
    val_unknown_count        INTEGER,
    val_valid_count          INTEGER,
    val_invalid_count        INTEGER,
    val_no_validator_count   INTEGER,
    hash_unknown_count       INTEGER,
    hash_valid_count         INTEGER,
    hash_suspect_count       INTEGER,

    FOREIGN KEY (item_id) REFERENCES items(item_id),
    FOREIGN KEY (first_scan_id) REFERENCES scans(scan_id),
    FOREIGN KEY (last_scan_id) REFERENCES scans(scan_id)
);

INSERT INTO item_versions (
    version_id, item_id, first_scan_id, last_scan_id,
    is_added, is_deleted, access, mod_date, size,
    last_val_scan, val_state, val_error,
    last_hash_scan, file_hash, hash_state,
    add_count, modify_count, delete_count, unchanged_count,
    val_unknown_count, val_valid_count, val_invalid_count, val_no_validator_count,
    hash_unknown_count, hash_valid_count, hash_suspect_count
)
SELECT
    version_id, item_id, first_scan_id, last_scan_id,
    is_added, is_deleted, access, mod_date, size,
    last_val_scan, val_state, val_error,
    last_hash_scan, UNHEX(file_hash), hash_state,
    add_count, modify_count, delete_count, unchanged_count,
    val_unknown_count, val_valid_count, val_invalid_count, val_no_validator_count,
    hash_unknown_count, hash_valid_count, hash_suspect_count
FROM item_versions_old;

DROP TABLE item_versions_old;

CREATE INDEX idx_versions_item_scan ON item_versions (item_id, first_scan_id DESC);
CREATE INDEX idx_versions_first_scan ON item_versions (first_scan_id);

-- ============================================================
-- Rebuild alerts: hash_old/hash_new TEXT → BLOB
-- ============================================================

ALTER TABLE alerts RENAME TO alerts_old;

CREATE TABLE alerts (
    alert_id INTEGER PRIMARY KEY AUTOINCREMENT,
    alert_type INTEGER NOT NULL,
    alert_status INTEGER NOT NULL,
    scan_id INTEGER NOT NULL,
    item_id INTEGER NOT NULL,
    created_at INTEGER NOT NULL,
    updated_at INTEGER DEFAULT NULL,

    -- suspect hash
    prev_hash_scan INTEGER DEFAULT NULL,
    hash_old BLOB DEFAULT NULL,
    hash_new BLOB DEFAULT NULL,

    -- invalid file
    val_error TEXT DEFAULT NULL
);

INSERT INTO alerts (
    alert_id, alert_type, alert_status, scan_id, item_id, created_at, updated_at,
    prev_hash_scan, hash_old, hash_new, val_error
)
SELECT
    alert_id, alert_type, alert_status, scan_id, item_id, created_at, updated_at,
    prev_hash_scan, UNHEX(hash_old), UNHEX(hash_new), val_error
FROM alerts_old;

DROP TABLE alerts_old;

CREATE INDEX idx_alerts_item ON alerts (item_id);

-- ============================================================
-- Drop redundant standalone path index (idx_items_root_path covers all queries)
-- ============================================================
DROP INDEX IF EXISTS idx_items_path;

-- ============================================================
-- Bump schema version
-- ============================================================
UPDATE meta SET value = '27' WHERE key = 'schema_version';
"#;
