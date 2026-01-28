pub const CREATE_SCHEMA_SQL: &str = r#"
BEGIN TRANSACTION;

CREATE TABLE IF NOT EXISTS meta (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

INSERT OR REPLACE INTO meta (key, value) VALUES ('schema_version', '14');

-- Roots table stores unique root directories that have been scanned
CREATE TABLE IF NOT EXISTS roots (
    root_id INTEGER PRIMARY KEY AUTOINCREMENT,
    root_path TEXT NOT NULL UNIQUE
);

-- Indexes to optimize queries
CREATE INDEX IF NOT EXISTS idx_roots_path ON roots (root_path COLLATE natural_path);

-- Scans table tracks individual scan sessions
CREATE TABLE IF NOT EXISTS scans (
    scan_id INTEGER PRIMARY KEY AUTOINCREMENT,
    root_id INTEGER NOT NULL,          -- Links scan to a root path
    schedule_id INTEGER DEFAULT NULL,  -- Links scan to schedule (NULL for manual scans)
    started_at INTEGER NOT NULL,       -- Timestamp when scan started (UTC)
    ended_at INTEGER DEFAULT NULL,     -- Timestamp when scan completed (UTC, NULL if in progress or incomplete)
    was_restarted BOOLEAN NOT NULL DEFAULT 0,  -- True if scan was interrupted and resumed
    state INTEGER NOT NULL,            -- The state of the scan (1 = Scanning, 2 = Sweeping, 3 = Analyzing, 4 = Completed, 5 = Stopped, 6 = Error)
    is_hash BOOLEAN NOT NULL,          -- Hash new or changed files
    hash_all BOOLEAN NOT NULL,         -- Hash all items including unchanged and previously hashed
    is_val BOOLEAN NOT NULL,           -- Validate the contents of files
    val_all BOOLEAN NOT NULL,          -- Validate all items including unchanged and previously validated
    file_count INTEGER DEFAULT NULL,   -- Count of files found in the scan
    folder_count INTEGER DEFAULT NULL, -- Count of directories found in the scan
    total_size INTEGER DEFAULT NULL,   -- Total size of all items (files and directories) seen in the scan
    alert_count INTEGER DEFAULT NULL,  -- Count of alerts created during the scan
    add_count INTEGER DEFAULT NULL,    -- Count of items added in the scan
    modify_count INTEGER DEFAULT NULL, -- Count of items modified in the scan
    delete_count INTEGER DEFAULT NULL, -- Count of items deleted in the scan
    error TEXT DEFAULT NULL,           -- Error message if scan failed
    FOREIGN KEY (root_id) REFERENCES roots(root_id),
    FOREIGN KEY (schedule_id) REFERENCES scan_schedules(schedule_id)
);

-- Items table tracks files and directories discovered during scans
CREATE TABLE IF NOT EXISTS items (
    item_id INTEGER PRIMARY KEY AUTOINCREMENT,
    root_id INTEGER NOT NULL,                   -- Links each item to a root
    item_path TEXT NOT NULL,                         -- Absolute path of the item
    item_type INTEGER NOT NULL,                 -- Item type enum (0=File, 1=Directory, 2=Symlink, 3=Other)

    -- Access State Property Group
    access INTEGER NOT NULL DEFAULT 0,          -- Access state enum (0=Ok, 1=MetaError, 2=ReadError)

    last_scan INTEGER NOT NULL,              -- Last scan where the item was present
    is_ts BOOLEAN NOT NULL DEFAULT 0,       -- Indicates if the item is a tombstone

    -- Medatadata Property Group
    mod_date INTEGER,                           -- Last mod_date timestamp
    size INTEGER,                               -- Size in bytes (file size for files, computed size for directories)

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
CREATE INDEX IF NOT EXISTS idx_items_path ON items (item_path COLLATE natural_path);
CREATE INDEX IF NOT EXISTS idx_items_root_path ON items (root_id, item_path COLLATE natural_path, item_type);
CREATE INDEX IF NOT EXISTS idx_items_root_scan ON items (root_id, last_scan, is_ts);

-- Changes table tracks modifications between scans
CREATE TABLE IF NOT EXISTS changes (
    change_id INTEGER PRIMARY KEY AUTOINCREMENT,
    scan_id INTEGER NOT NULL,                   -- The scan in which the change was detected
    item_id INTEGER NOT NULL,                   -- The file or directory that changed
    change_type INTEGER NOT NULL,               -- Change type enum (0=NoChange, 1=Add, 2=Modify, 3=Delete)

    -- Access State Properties (any change type)
    access_old INTEGER DEFAULT NULL,            -- Previous access state enum (if changed)
    access_new INTEGER DEFAULT NULL,            -- New access state enum (if changed)

    -- Add specific properties
    is_undelete BOOLEAN DEFAULT NULL,           -- Not Null if "A". True if item was tombstone

    -- Metadata Changed (Modify)
    meta_change BOOLEAN DEFAULT NULL,      -- Not Null if "M". True if metadata changed
    mod_date_old INTEGER DEFAULT NULL,          -- Stores the previous mod_date timestamp (if changed)
    mod_date_new INTEGER DEFAULT NULL,          -- Stores the new mod_date timestamp (if changed)
    size_old INTEGER DEFAULT NULL,              -- Stores the previous size (if changed)
    size_new INTEGER DEFAULT NULL,              -- Stores the new size (if changed)

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
CREATE INDEX IF NOT EXISTS idx_changes_item ON changes (item_id);

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

-- Indexes to optimize queries
CREATE INDEX IF NOT EXISTS idx_alerts_item ON alerts (item_id);

-- Scan schedules table stores recurring scan configurations
CREATE TABLE IF NOT EXISTS scan_schedules (
    schedule_id INTEGER PRIMARY KEY AUTOINCREMENT,
    root_id INTEGER NOT NULL,
    enabled BOOLEAN NOT NULL DEFAULT 1,
    schedule_name TEXT NOT NULL,
    schedule_type INTEGER NOT NULL CHECK(schedule_type IN (0, 1, 2, 3)),  -- 0=daily, 1=weekly, 2=interval, 3=monthly
    time_of_day TEXT,                                                   -- 'HH:MM' format for daily/weekly/monthly
    days_of_week TEXT,                                                  -- JSON array for weekly schedules
    day_of_month INTEGER,                                               -- Day (1-31) for monthly schedules
    interval_value INTEGER,                                             -- For interval schedules
    interval_unit INTEGER CHECK(interval_unit IN (0, 1, 2, 3)),       -- 0=minutes, 1=hours, 2=days, 3=weeks
    hash_mode INTEGER NOT NULL CHECK(hash_mode IN (0, 1, 2)),
    validate_mode INTEGER NOT NULL CHECK(validate_mode IN (0, 1, 2)),
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    deleted_at INTEGER DEFAULT NULL,                                    -- Soft delete timestamp (NULL for active schedules)
    FOREIGN KEY (root_id) REFERENCES roots(root_id)
);

CREATE INDEX IF NOT EXISTS idx_scan_schedules_enabled ON scan_schedules(enabled);
CREATE INDEX IF NOT EXISTS idx_scan_schedules_root ON scan_schedules(root_id);
CREATE INDEX IF NOT EXISTS idx_scan_schedules_deleted ON scan_schedules(deleted_at);

-- Task queue table stores active work items (scans and other tasks)
CREATE TABLE IF NOT EXISTS task_queue (
    queue_id INTEGER PRIMARY KEY AUTOINCREMENT,
    task_type INTEGER NOT NULL DEFAULT 0,          -- TaskType enum: 0=Scan, 1=DatabaseCompact, etc.

    -- Active/running indicator (applies to ALL task types)
    is_active BOOLEAN NOT NULL DEFAULT 0,          -- True when task is currently executing

    -- Root reference (set at queue time for tasks that operate on a root, NULL otherwise)
    root_id INTEGER,                               -- Some task types operate on a root, some don't

    -- Schedule reference (for scheduled tasks - currently only scans are schedulable)
    schedule_id INTEGER,                           -- FK to scan_schedules

    -- Scan-specific: FK to scans table (set when scan task starts, NULL for non-scan tasks)
    scan_id INTEGER,                               -- Created when scan begins, used for resume

    -- Scheduling
    next_run_time INTEGER,                         -- Unix timestamp (UTC), NULL when disabled
    source INTEGER NOT NULL CHECK(source IN (0, 1)), -- 0=manual, 1=scheduled

    -- Task settings (JSON for task-specific config)
    -- For scans: {"hash_mode":"New","validate_mode":"None"}
    -- For other tasks: task-specific settings
    task_settings TEXT NOT NULL,

    -- Resume tracking (for scan tasks)
    analysis_hwm INTEGER DEFAULT NULL,             -- High water mark for analysis restart resilience

    -- Metadata
    created_at INTEGER NOT NULL,

    FOREIGN KEY (schedule_id) REFERENCES scan_schedules(schedule_id),
    FOREIGN KEY (root_id) REFERENCES roots(root_id),
    FOREIGN KEY (scan_id) REFERENCES scans(scan_id)
);

CREATE INDEX IF NOT EXISTS idx_task_queue_type_next ON task_queue(task_type, next_run_time);
CREATE INDEX IF NOT EXISTS idx_task_queue_source_next ON task_queue(source, next_run_time);
CREATE INDEX IF NOT EXISTS idx_task_queue_root ON task_queue(root_id);
CREATE UNIQUE INDEX IF NOT EXISTS idx_task_queue_schedule ON task_queue(schedule_id) WHERE schedule_id IS NOT NULL;

COMMIT;
"#;
