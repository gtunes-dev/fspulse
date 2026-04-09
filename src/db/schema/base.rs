// ============================================================================
// Query Pattern Guidelines
// ============================================================================
//
// Performance-critical queries must follow these patterns to use indexes
// effectively and avoid full table scans on large tables (item_versions
// and hash_versions can have millions of rows).
//
// Pattern 1: "Alive versions at a point in time"
//   Used by: browse view, analysis, MCP queries.
//   For each item, the current version at time T has:
//     first_seen_at <= T AND last_seen_at >= T
//   Use MAX(item_version) with first_seen_at constraint:
//     WHERE item_id = ? AND first_seen_at <= T
//     ORDER BY item_version DESC LIMIT 1
//
// Pattern 2: "Changed versions in a time range"
//   Used by: browse descendant counts, change summaries, scan completion.
//   Drive from item_versions using idx_versions_root_firstseen_hid:
//     FROM item_versions iv
//     WHERE iv.root_id = ? AND iv.first_seen_at BETWEEN ? AND ?
//
// Pattern 3: "Descendant changes in a subtree"
//   Used by: browse folder decoration, folder change counts.
//   Use hierarchy_id range on item_versions (no join to items needed):
//     FROM item_versions iv
//     WHERE iv.root_id = ?
//       AND iv.first_seen_at BETWEEN ? AND ?
//       AND iv.hierarchy_id > ? AND iv.hierarchy_id < ?
//   Index: idx_versions_root_firstseen_hid (root_id, first_seen_at, hierarchy_id)
//
// Pattern 4: "Direct children of a folder"
//   Used by: browse folder view, child counts.
//   Drive from item_versions using parent_item_id:
//     FROM item_versions iv WHERE iv.parent_item_id = ?
//   Index: idx_versions_parent_firstseen (parent_item_id, first_seen_at)
//
// Pattern 5: "Version history for an item"
//   Used by: item detail views, get_current().
//   Drive from item_versions using PK (item_id, item_version):
//     FROM item_versions WHERE item_id = ? ORDER BY item_version DESC
//
// Pattern 6: "Latest hash for a version"
//   Used by: analysis queries, integrity state.
//   Use hash_versions PK prefix (item_id, item_version):
//     LEFT JOIN hash_versions hv ON hv.item_id = iv.item_id
//       AND hv.item_version = iv.item_version
//       AND hv.first_seen_at = (
//         SELECT MAX(first_seen_at) FROM hash_versions
//         WHERE item_id = iv.item_id AND item_version = iv.item_version)
//
// Pattern 7: "Scans for a root"
//   Used by: trends, browse, scan picker.
//   Drive from scans using idx_scans_root:
//     FROM scans WHERE root_id = ?
//
// Pattern 8: "Sweep for deletions"
//   Used by: scan sweep phase.
//   Items not seen since before the scan started are candidates for deletion:
//     FROM item_versions iv
//     WHERE iv.root_id = ? AND iv.last_seen_at < ? AND iv.is_deleted = 0
// ============================================================================

pub const CREATE_SCHEMA_SQL: &str = r#"
BEGIN TRANSACTION;

CREATE TABLE IF NOT EXISTS meta (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

INSERT OR REPLACE INTO meta (key, value) VALUES ('schema_version', '32');

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
    file_count INTEGER DEFAULT NULL,   -- Count of files found in the scan
    folder_count INTEGER DEFAULT NULL, -- Count of directories found in the scan
    total_size INTEGER DEFAULT NULL,   -- Total size of all items (files and directories) seen in the scan
    new_hash_suspect_count INTEGER DEFAULT NULL, -- Count of hash_versions with hash_state=2 first seen in this scan
    new_val_invalid_count INTEGER DEFAULT NULL,  -- Count of item_versions with val_state=2 validated in this scan
    add_count INTEGER DEFAULT NULL,    -- Count of items added in the scan
    modify_count INTEGER DEFAULT NULL, -- Count of items modified in the scan
    delete_count INTEGER DEFAULT NULL, -- Count of items deleted in the scan
    val_unknown_count INTEGER DEFAULT NULL,       -- Count of files with unknown validation state
    val_valid_count INTEGER DEFAULT NULL,         -- Count of files with valid validation state
    val_invalid_count INTEGER DEFAULT NULL,       -- Count of files with invalid validation state
    val_no_validator_count INTEGER DEFAULT NULL,   -- Count of files with no available validator
    hash_unknown_count INTEGER DEFAULT NULL,       -- Count of files with unknown hash state
    hash_baseline_count INTEGER DEFAULT NULL,       -- Count of files with baseline (unchanged) hash state
    hash_suspect_count INTEGER DEFAULT NULL,    -- Count of files with suspicious (changed) hash state
    error TEXT DEFAULT NULL,           -- Error message if scan failed
    FOREIGN KEY (root_id) REFERENCES roots(root_id),
    FOREIGN KEY (schedule_id) REFERENCES scan_schedules(schedule_id)
);

CREATE INDEX IF NOT EXISTS idx_scans_root ON scans (root_id);

-- ========================================
-- Item identity table
-- ========================================
-- Lightweight stable identity for each item across all its versions.
CREATE TABLE IF NOT EXISTS items (
    item_id         INTEGER PRIMARY KEY AUTOINCREMENT,
    root_id         INTEGER NOT NULL,
    parent_item_id  INTEGER,
    item_path       TEXT NOT NULL,
    item_name       TEXT NOT NULL,
    hierarchy_id    BLOB,
    item_type       INTEGER NOT NULL,
    file_extension  TEXT,                          -- lowercase extension (e.g. 'pdf', 'jpg'), NULL for folders/extensionless
    has_validator   INTEGER NOT NULL DEFAULT 0,   -- 1 if a structural validator exists for this file type
    do_not_validate INTEGER NOT NULL DEFAULT 0,   -- 1 if user has opted this item out of validation
    FOREIGN KEY (root_id) REFERENCES roots(root_id),
    FOREIGN KEY (parent_item_id) REFERENCES items(item_id),
    UNIQUE (root_id, item_path, item_type)
);

CREATE INDEX IF NOT EXISTS idx_items_root_path ON items (root_id, item_path COLLATE natural_path, item_type);
CREATE INDEX IF NOT EXISTS idx_items_root_name ON items (root_id, item_name COLLATE natural_path);
CREATE INDEX IF NOT EXISTS idx_items_root_ext ON items (root_id, file_extension);
CREATE INDEX IF NOT EXISTS idx_items_root_hid ON items (root_id, hierarchy_id);

-- ========================================
-- Temporal item versions table
-- ========================================
-- One row per distinct state of an item. Identity (path, type, root) lives in items.
-- item_version is a per-item sequence number (1, 2, 3, …, n) assigned chronologically.
--
-- Temporal range: first_seen_at..last_seen_at (epoch seconds).
-- Versions are created by checkpoints (scans) or file watchers.
-- first_seen_at: when this state was first observed.
-- last_seen_at:  when this state was last confirmed current.
--
-- Folder versions are created only for structural events (add, delete).
-- Folder mtime/size are not tracked. Descendant counts are computed on demand
-- using hierarchy_id range queries — no precomputed counts stored.
CREATE TABLE IF NOT EXISTS item_versions (
    item_id         INTEGER NOT NULL,
    item_version    INTEGER NOT NULL,
    root_id         INTEGER NOT NULL,
    parent_item_id  INTEGER,
    hierarchy_id    BLOB,
    first_seen_at   INTEGER NOT NULL,
    last_seen_at    INTEGER NOT NULL,

    -- State
    is_added        BOOLEAN NOT NULL DEFAULT 0,
    is_deleted      BOOLEAN NOT NULL DEFAULT 0,
    access          INTEGER NOT NULL DEFAULT 0,

    -- File-specific fields (NULL for folders)
    mod_date        INTEGER,
    size            INTEGER,

    -- Validation state (files only, NULL for folders and unvalidated files)
    val_scan_id     INTEGER,            -- scan in which this version was validated
    val_state       INTEGER,            -- 1=Valid, 2=Invalid
    val_error       TEXT,               -- error details when val_state=Invalid

    -- User review of integrity issues
    val_reviewed_at  INTEGER DEFAULT NULL,
    hash_reviewed_at INTEGER DEFAULT NULL,

    PRIMARY KEY (item_id, item_version),
    FOREIGN KEY (item_id) REFERENCES items(item_id),
    FOREIGN KEY (root_id) REFERENCES roots(root_id)
) WITHOUT ROWID;

CREATE INDEX IF NOT EXISTS idx_versions_first_seen ON item_versions (root_id, first_seen_at);
CREATE INDEX IF NOT EXISTS idx_versions_last_seen ON item_versions (root_id, last_seen_at);
CREATE INDEX IF NOT EXISTS idx_versions_root_firstseen_hid ON item_versions (root_id, first_seen_at, hierarchy_id);
CREATE INDEX IF NOT EXISTS idx_versions_parent_firstseen ON item_versions (parent_item_id, first_seen_at);
CREATE INDEX IF NOT EXISTS idx_versions_val_scan ON item_versions (val_scan_id, val_state);

-- ========================================
-- Hash versions table (integrity observation log)
-- ========================================
-- Zero or more hash observations per item_version. Forms a log of hash checks.
-- Absence of any row for a version means it has never been hashed.
-- Multiple rows for the same version track hash changes over time (e.g., bit rot).
CREATE TABLE IF NOT EXISTS hash_versions (
    item_id          INTEGER NOT NULL,     -- leading key for item-level queries
    item_version     INTEGER NOT NULL,     -- which item_version this hash observes
    first_seen_at    INTEGER NOT NULL,     -- epoch: when this hash was first computed
    last_seen_at     INTEGER NOT NULL,     -- epoch: when this hash was last confirmed
    file_hash        BLOB NOT NULL,
    hash_state       INTEGER NOT NULL,     -- 1=Baseline, 2=Suspect
    PRIMARY KEY (item_id, item_version, first_seen_at),
    FOREIGN KEY (item_id, item_version) REFERENCES item_versions(item_id, item_version)
) WITHOUT ROWID;

CREATE INDEX IF NOT EXISTS idx_hash_versions_first_seen ON hash_versions (first_seen_at, hash_state);

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
    is_val BOOLEAN NOT NULL DEFAULT 0,
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
