pub const CREATE_SCHEMA_SQL: &str = r#"
BEGIN TRANSACTION;

CREATE TABLE IF NOT EXISTS meta (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

INSERT OR REPLACE INTO meta (key, value) VALUES ('schema_version', '19');

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
    state INTEGER NOT NULL,            -- The state of the scan (1 = Scanning, 2 = Sweeping, 3 = Analyzing Files, 7 = Analyzing Scan, 4 = Completed, 5 = Stopped, 6 = Error)
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

-- ========================================
-- Item identity table
-- ========================================
-- Lightweight stable identity for each item across all its versions.
CREATE TABLE IF NOT EXISTS items (
    item_id     INTEGER PRIMARY KEY AUTOINCREMENT,
    root_id     INTEGER NOT NULL,
    item_path   TEXT NOT NULL,
    item_name   TEXT NOT NULL,
    item_type   INTEGER NOT NULL,
    FOREIGN KEY (root_id) REFERENCES roots(root_id),
    UNIQUE (root_id, item_path, item_type)
);

CREATE INDEX IF NOT EXISTS idx_items_path ON items (item_path COLLATE natural_path);
CREATE INDEX IF NOT EXISTS idx_items_root_path ON items (root_id, item_path COLLATE natural_path, item_type);
CREATE INDEX IF NOT EXISTS idx_items_root_name ON items (root_id, item_name COLLATE natural_path);

-- ========================================
-- Temporal item versions table
-- ========================================
-- One row per distinct state of an item. Identity (path, type, root) lives in items.
CREATE TABLE IF NOT EXISTS item_versions (
    version_id      INTEGER PRIMARY KEY AUTOINCREMENT,
    item_id         INTEGER NOT NULL,
    first_scan_id   INTEGER NOT NULL,
    last_scan_id    INTEGER NOT NULL,
    is_added        BOOLEAN NOT NULL DEFAULT 0,
    is_deleted      BOOLEAN NOT NULL DEFAULT 0,
    access          INTEGER NOT NULL DEFAULT 0,
    mod_date        INTEGER,
    size            INTEGER,
    file_hash       TEXT,
    val             INTEGER NOT NULL DEFAULT 3,
    val_error       TEXT,
    last_hash_scan  INTEGER,
    last_val_scan   INTEGER,
    add_count       INTEGER,
    modify_count    INTEGER,
    delete_count    INTEGER,
    FOREIGN KEY (item_id) REFERENCES items(item_id),
    FOREIGN KEY (first_scan_id) REFERENCES scans(scan_id),
    FOREIGN KEY (last_scan_id) REFERENCES scans(scan_id)
);

CREATE INDEX IF NOT EXISTS idx_versions_item_scan ON item_versions (item_id, first_scan_id DESC);
CREATE INDEX IF NOT EXISTS idx_versions_first_scan ON item_versions (first_scan_id);

-- ========================================
-- Scan undo log (transient, for rollback support)
-- ========================================
CREATE TABLE IF NOT EXISTS scan_undo_log (
    undo_id             INTEGER PRIMARY KEY AUTOINCREMENT,
    version_id          INTEGER NOT NULL,
    old_last_scan_id    INTEGER NOT NULL,
    old_last_hash_scan  INTEGER,
    old_last_val_scan   INTEGER
);

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

-- Tasks table stores work items and their execution history
CREATE TABLE IF NOT EXISTS tasks (
    task_id INTEGER PRIMARY KEY AUTOINCREMENT,
    task_type INTEGER NOT NULL DEFAULT 0,          -- TaskType enum: 0=Scan

    -- Lifecycle status
    status INTEGER NOT NULL DEFAULT 0,             -- TaskStatus enum: 0=Pending, 1=Running, 2=Completed, 3=Stopped, 4=Error

    -- Root reference
    root_id INTEGER,

    -- Schedule reference (ON DELETE SET NULL so completed tasks survive schedule deletion)
    schedule_id INTEGER,

    -- Scheduling
    run_at INTEGER NOT NULL DEFAULT 0,             -- When eligible to run (0 = immediately)
    source INTEGER NOT NULL CHECK(source IN (0, 1)), -- 0=manual, 1=scheduled

    -- Task configuration (immutable JSON — permanent artifact)
    task_settings TEXT NOT NULL,

    -- Execution state (generic JSON, NULL until first set, preserved at completion)
    task_state TEXT,

    -- Timestamps
    created_at INTEGER NOT NULL,
    started_at INTEGER,                            -- When execution began
    completed_at INTEGER,                          -- When terminal status was reached

    FOREIGN KEY (schedule_id) REFERENCES scan_schedules(schedule_id) ON DELETE SET NULL,
    FOREIGN KEY (root_id) REFERENCES roots(root_id)
);

CREATE INDEX IF NOT EXISTS idx_tasks_status_source_runat ON tasks(status, source, run_at, task_id);
CREATE INDEX IF NOT EXISTS idx_tasks_schedule ON tasks(schedule_id) WHERE schedule_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_tasks_root ON tasks(root_id);

COMMIT;
"#;
