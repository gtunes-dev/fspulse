pub const UPGRADE_6_TO_7_SQL: &str = r#"
--
-- Schema Upgrade: Version 6 → 7
--
-- This migration converts character-based enum columns to integer-based enums:
-- - item_type: CHAR(1) → INTEGER ('F'→0, 'D'→1, 'S'→2, 'O'→3)
-- - change_type: CHAR(1) → INTEGER ('N'→0, 'A'→1, 'M'→2, 'D'→3)
-- - alert_type: CHAR(1) → INTEGER ('H'→0, 'I'→1)
-- - alert_status: CHAR(1) → INTEGER ('O'→0, 'F'→1, 'D'→2)
-- - val: CHAR(1) → INTEGER ('U'→0, 'V'→1, 'I'→2, 'N'→3)
--
-- Following SQLite's table reconstruction pattern (same as v2→v3 migration)
--

-- Disable foreign key constraints BEFORE transaction starts
PRAGMA foreign_keys = OFF;

BEGIN TRANSACTION;

-- Verify schema version is exactly 6
SELECT 1 / (CASE WHEN (SELECT value FROM meta WHERE key = 'schema_version') = '6' THEN 1 ELSE 0 END);

-- ========================================
-- Migrate items table
-- ========================================

CREATE TABLE new_items (
    item_id INTEGER PRIMARY KEY AUTOINCREMENT,
    root_id INTEGER NOT NULL,
    item_path TEXT NOT NULL,
    item_type INTEGER NOT NULL,

    last_scan INTEGER NOT NULL,
    is_ts BOOLEAN NOT NULL DEFAULT 0,

    -- Metadata Property Group
    mod_date INTEGER,
    file_size INTEGER,

    -- Hash Property Group
    last_hash_scan INTEGER,
    file_hash TEXT,

    -- Validation Property Group
    last_val_scan INTEGER,
    val INTEGER NOT NULL,
    val_error TEXT,

    FOREIGN KEY (root_id) REFERENCES roots(root_id),
    FOREIGN KEY (last_scan) REFERENCES scans(scan_id),
    FOREIGN KEY (last_hash_scan) REFERENCES scans(scan_id),
    FOREIGN KEY (last_val_scan) REFERENCES scans(scan_id),
    UNIQUE (root_id, item_path, item_type)
);

INSERT INTO new_items (
    item_id, root_id, item_path, item_type,
    last_scan, is_ts,
    mod_date, file_size,
    last_hash_scan, file_hash,
    last_val_scan, val, val_error
)
SELECT
    item_id, root_id, item_path,
    CASE item_type
        WHEN 'F' THEN 0
        WHEN 'D' THEN 1
        WHEN 'S' THEN 2
        WHEN 'O' THEN 3
    END,
    last_scan, is_ts,
    mod_date, file_size,
    last_hash_scan, file_hash,
    last_val_scan,
    CASE val
        WHEN 'U' THEN 0
        WHEN 'V' THEN 1
        WHEN 'I' THEN 2
        WHEN 'N' THEN 3
    END,
    val_error
FROM items;

DROP TABLE items;

ALTER TABLE new_items RENAME TO items;

-- Recreate items indexes
CREATE INDEX idx_items_path ON items (root_id, item_path, item_type);
CREATE INDEX idx_items_scan ON items (root_id, last_scan, is_ts);

-- Update sqlite_sequence for items
UPDATE sqlite_sequence SET seq = (SELECT MAX(item_id) FROM items) WHERE name = 'items';

-- ========================================
-- Migrate changes table
-- ========================================

CREATE TABLE new_changes (
    change_id INTEGER PRIMARY KEY AUTOINCREMENT,
    scan_id INTEGER NOT NULL,
    item_id INTEGER NOT NULL,
    change_type INTEGER NOT NULL,

    -- Add specific properties
    is_undelete BOOLEAN DEFAULT NULL,

    -- Metadata Changed (Modify)
    meta_change BOOLEAN DEFAULT NULL,
    mod_date_old INTEGER DEFAULT NULL,
    mod_date_new INTEGER DEFAULT NULL,
    file_size_old INTEGER DEFAULT NULL,
    file_size_new INTEGER DEFAULT NULL,

    -- Hash Properties (Add, Modify)
    hash_change BOOLEAN DEFAULT NULL,
    last_hash_scan_old INTEGER DEFAULT NULL,
    hash_old TEXT DEFAULT NULL,
    hash_new TEXT DEFAULT NULL,

    -- Validation Properties (Add or Modify)
    val_change BOOLEAN DEFAULT NULL,
    last_val_scan_old INTEGER DEFAULT NULL,
    val_old INTEGER DEFAULT NULL,
    val_new INTEGER DEFAULT NULL,
    val_error_old DEFAULT NULL,
    val_error_new DEFAULT NULL,

    FOREIGN KEY (scan_id) REFERENCES scans(scan_id),
    FOREIGN KEY (item_id) REFERENCES items(item_id),
    UNIQUE (scan_id, item_id)
);

INSERT INTO new_changes (
    change_id, scan_id, item_id, change_type,
    is_undelete,
    meta_change, mod_date_old, mod_date_new, file_size_old, file_size_new,
    hash_change, last_hash_scan_old, hash_old, hash_new,
    val_change, last_val_scan_old, val_old, val_new, val_error_old, val_error_new
)
SELECT
    change_id, scan_id, item_id,
    CASE change_type
        WHEN 'N' THEN 0
        WHEN 'A' THEN 1
        WHEN 'M' THEN 2
        WHEN 'D' THEN 3
    END,
    is_undelete,
    meta_change, mod_date_old, mod_date_new, file_size_old, file_size_new,
    hash_change, last_hash_scan_old, hash_old, hash_new,
    val_change, last_val_scan_old,
    CASE val_old
        WHEN 'U' THEN 0
        WHEN 'V' THEN 1
        WHEN 'I' THEN 2
        WHEN 'N' THEN 3
        ELSE NULL
    END,
    CASE val_new
        WHEN 'U' THEN 0
        WHEN 'V' THEN 1
        WHEN 'I' THEN 2
        WHEN 'N' THEN 3
        ELSE NULL
    END,
    val_error_old, val_error_new
FROM changes;

DROP TABLE changes;

ALTER TABLE new_changes RENAME TO changes;

-- Recreate changes indexes
CREATE INDEX idx_changes_scan_type ON changes (scan_id, change_type);

-- Update sqlite_sequence for changes
UPDATE sqlite_sequence SET seq = (SELECT MAX(change_id) FROM changes) WHERE name = 'changes';

-- ========================================
-- Migrate alerts table
-- ========================================

CREATE TABLE new_alerts (
    alert_id INTEGER PRIMARY KEY AUTOINCREMENT,
    alert_type INTEGER NOT NULL,
    alert_status INTEGER NOT NULL,
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

INSERT INTO new_alerts (
    alert_id, alert_type, alert_status,
    scan_id, item_id, created_at, updated_at,
    prev_hash_scan, hash_old, hash_new,
    val_error
)
SELECT
    alert_id,
    CASE alert_type
        WHEN 'H' THEN 0
        WHEN 'I' THEN 1
    END,
    CASE alert_status
        WHEN 'O' THEN 0
        WHEN 'F' THEN 1
        WHEN 'D' THEN 2
    END,
    scan_id, item_id, created_at, updated_at,
    prev_hash_scan, hash_old, hash_new,
    val_error
FROM alerts;

DROP TABLE alerts;

ALTER TABLE new_alerts RENAME TO alerts;

-- No indexes to recreate for alerts table

-- Update sqlite_sequence for alerts
UPDATE sqlite_sequence SET seq = (SELECT MAX(alert_id) FROM alerts) WHERE name = 'alerts';

-- ========================================
-- Finalize migration
-- ========================================

-- Update schema version
UPDATE meta SET value = '7' WHERE key = 'schema_version';

COMMIT;

-- Re-enable foreign key constraints
PRAGMA foreign_keys = ON;
"#;
