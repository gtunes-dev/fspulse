// ============================================================================
// Schema Upgrade: Version 25 → 26
//
// Rebuilds scan_undo_log with version_id as the PRIMARY KEY instead of a
// separate undo_id auto-increment column. Since there is at most one undo
// entry per version, version_id is the natural key. This makes lookups by
// version_id an O(1) rowid seek and eliminates the secondary index.
// ============================================================================

pub const UPGRADE_25_TO_26_SQL: &str = r#"
-- Rebuild scan_undo_log with version_id as PRIMARY KEY
ALTER TABLE scan_undo_log RENAME TO scan_undo_log_old;

CREATE TABLE scan_undo_log (
    version_id          INTEGER PRIMARY KEY,
    old_last_scan_id    INTEGER NOT NULL,
    old_last_hash_scan  INTEGER,
    old_last_val_scan   INTEGER
);

INSERT INTO scan_undo_log (version_id, old_last_scan_id, old_last_hash_scan, old_last_val_scan)
    SELECT version_id, old_last_scan_id, old_last_hash_scan, old_last_val_scan
    FROM scan_undo_log_old;

DROP TABLE scan_undo_log_old;

UPDATE meta SET value = '26' WHERE key = 'schema_version';
"#;
