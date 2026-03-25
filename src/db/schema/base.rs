// ============================================================================
// Query Pattern Guidelines
// ============================================================================
//
// Performance-critical queries must follow these patterns to use indexes
// effectively and avoid full table scans on large tables (item_versions
// and hash_versions can have millions of rows).
//
// Pattern 1: "Alive versions for a root at a scan"
//   Used by: analysis batch fetch, analysis counts, scan completion counts.
//   Drive from item_versions using idx_versions_root_lastscan:
//     FROM item_versions cv
//     JOIN items i ON i.item_id = cv.item_id
//     WHERE cv.root_id = ? AND cv.last_scan_id = ?
//
// Pattern 2: "Versions created in a scan" / "Versions validated in a scan"
//   Used by: change counts (add/modify/delete) and new_val_invalid_count at scan completion.
//   Drive from item_versions using idx_versions_first_scan or idx_versions_val_scan:
//     FROM item_versions iv WHERE iv.first_scan_id = ?
//     FROM item_versions iv WHERE iv.val_scan_id = ? AND iv.val_state = ?
//   scan_id already uniquely identifies a root; no root_id predicate needed.
//
// Pattern 3: "Latest hash for a version"
//   Used by: analysis queries, scan completion hash state counts.
//   Use hash_versions PK prefix (item_id, item_version):
//     LEFT JOIN hash_versions hv ON hv.item_id = cv.item_id
//       AND hv.item_version = cv.item_version
//       AND hv.first_scan_id = (
//         SELECT MAX(first_scan_id) FROM hash_versions
//         WHERE item_id = cv.item_id AND item_version = cv.item_version)
//
// Pattern 4: "Version history for an item"
//   Used by: item detail views, get_current().
//   Drive from item_versions using PK (item_id, item_version):
//     FROM item_versions WHERE item_id = ? ORDER BY item_version DESC
//   No root_id filter needed — item_id is globally unique.
//
// Pattern 5: "Scans for a root"
//   Used by: trends, browse, scan picker.
//   Drive from scans using idx_scans_root:
//     FROM scans WHERE root_id = ?
//
// Anti-pattern: Scanning all items then probing item_versions per item.
//   BAD:  FROM items i JOIN item_versions cv ON cv.item_id = i.item_id
//           AND cv.last_scan_id = ?
//   GOOD: FROM item_versions cv JOIN items i ON i.item_id = cv.item_id
//           WHERE cv.root_id = ? AND cv.last_scan_id = ?
// ============================================================================

pub const CREATE_SCHEMA_SQL: &str = r#"
BEGIN TRANSACTION;

CREATE TABLE IF NOT EXISTS meta (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

INSERT OR REPLACE INTO meta (key, value) VALUES ('schema_version', '29');

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
    item_path       TEXT NOT NULL,
    item_name       TEXT NOT NULL,
    file_extension  TEXT,                          -- lowercase extension (e.g. 'pdf', 'jpg'), NULL for folders/extensionless
    item_type       INTEGER NOT NULL,
    has_validator   INTEGER NOT NULL DEFAULT 0,   -- 1 if a structural validator exists for this file type
    do_not_validate INTEGER NOT NULL DEFAULT 0,   -- 1 if user has opted this item out of validation
    FOREIGN KEY (root_id) REFERENCES roots(root_id),
    UNIQUE (root_id, item_path, item_type)
);

CREATE INDEX IF NOT EXISTS idx_items_root_path ON items (root_id, item_path COLLATE natural_path, item_type);
CREATE INDEX IF NOT EXISTS idx_items_root_name ON items (root_id, item_name COLLATE natural_path);
CREATE INDEX IF NOT EXISTS idx_items_root_ext ON items (root_id, file_extension);

-- ========================================
-- Temporal item versions table
-- ========================================
-- One row per distinct state of an item. Identity (path, type, root) lives in items.
-- item_version is a per-item sequence number (1, 2, 3, …, n) assigned chronologically.
CREATE TABLE IF NOT EXISTS item_versions (
    item_id         INTEGER NOT NULL,
    item_version    INTEGER NOT NULL,
    root_id         INTEGER NOT NULL,
    first_scan_id   INTEGER NOT NULL,
    last_scan_id    INTEGER NOT NULL,

    -- Shared fields (all item types)
    is_added        BOOLEAN NOT NULL DEFAULT 0,
    is_deleted      BOOLEAN NOT NULL DEFAULT 0,
    access          INTEGER NOT NULL DEFAULT 0,
    mod_date        INTEGER,
    size            INTEGER,

    -- Folder-specific descendant change counts (NULL for files).
    -- Each count reflects the scan that created this version:
    --   add_count       — descendants that were added (new or restored)
    --   modify_count    — descendants that were modified (alive before and after)
    --   delete_count    — descendants that were deleted (alive → not alive)
    --   unchanged_count — descendants that were alive but didn't change
    -- Total alive descendants = add_count + modify_count + unchanged_count.
    -- For an unchanged folder whose temporal version is from scan M,
    -- the real unchanged count at scan N is: adds_M + mods_M + unchanged_M.
    add_count       INTEGER,
    modify_count    INTEGER,
    delete_count    INTEGER,
    unchanged_count INTEGER,

    -- Validation state (files only, NULL for folders and unvalidated files).
    -- Tightly coupled to this version: validated once when version is created.
    val_scan_id     INTEGER,            -- scan in which this version was validated
    val_state       INTEGER,            -- 1=Valid, 2=Invalid
    val_error       TEXT,               -- error details when val_state=Invalid

    -- User review of integrity issues on this version.
    -- val_reviewed_at: set when user marks this version's validation issue as reviewed.
    --   Never auto-cleared (validation is one-time per version).
    -- hash_reviewed_at: set when user marks this version's hash integrity issue as reviewed.
    --   Auto-cleared only when the FIRST Suspect hash_version is created for
    --   this item_version (new integrity evidence). NOT cleared on subsequent
    --   suspect hash observations (ongoing drift is not a new signal).
    val_reviewed_at  INTEGER DEFAULT NULL,
    hash_reviewed_at INTEGER DEFAULT NULL,

    PRIMARY KEY (item_id, item_version),
    FOREIGN KEY (item_id) REFERENCES items(item_id),
    FOREIGN KEY (root_id) REFERENCES roots(root_id),
    FOREIGN KEY (first_scan_id) REFERENCES scans(scan_id),
    FOREIGN KEY (last_scan_id) REFERENCES scans(scan_id)
) WITHOUT ROWID;

CREATE INDEX IF NOT EXISTS idx_versions_first_scan ON item_versions (first_scan_id);
CREATE INDEX IF NOT EXISTS idx_versions_root_lastscan ON item_versions (root_id, last_scan_id);
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
    first_scan_id    INTEGER NOT NULL,
    last_scan_id     INTEGER NOT NULL,
    file_hash        BLOB NOT NULL,
    hash_state       INTEGER NOT NULL,     -- 1=Baseline, 2=Suspect
    PRIMARY KEY (item_id, item_version, first_scan_id),
    FOREIGN KEY (item_id, item_version) REFERENCES item_versions(item_id, item_version),
    FOREIGN KEY (first_scan_id) REFERENCES scans(scan_id),
    FOREIGN KEY (last_scan_id) REFERENCES scans(scan_id)
) WITHOUT ROWID;

CREATE INDEX IF NOT EXISTS idx_hash_versions_first_scan ON hash_versions (first_scan_id, hash_state);

-- ========================================
-- Scan undo log (transient, for rollback support)
-- ========================================
-- log_type: 0=item_version, 1=hash_version
-- For type 0: ref_id1=item_id, ref_id2=item_version, ref_id3=0
-- For type 1: ref_id1=item_id, ref_id2=item_version, ref_id3=first_scan_id
CREATE TABLE IF NOT EXISTS scan_undo_log (
    log_type            INTEGER NOT NULL,
    ref_id1             INTEGER NOT NULL,
    ref_id2             INTEGER NOT NULL,
    ref_id3             INTEGER NOT NULL DEFAULT 0,
    old_last_scan_id    INTEGER NOT NULL,
    PRIMARY KEY (log_type, ref_id1, ref_id2, ref_id3)
) WITHOUT ROWID;

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
