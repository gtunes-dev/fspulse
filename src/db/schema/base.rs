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
//   A version is in effect from its first_seen_at until the next version
//   of the same item begins. There is no per-version end timestamp.
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
// Pattern 8: "Siblings of a folder"
//   Used by: hierarchy_id assignment on insert, reconcile directory diff.
//   Drive from items using idx_items_parent_name:
//     FROM items WHERE root_id = ? AND parent_item_id IS ?
//       AND item_name COLLATE natural_path < ?
// ============================================================================

pub const CREATE_SCHEMA_SQL: &str = r#"
BEGIN TRANSACTION;

CREATE TABLE IF NOT EXISTS meta (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

INSERT OR REPLACE INTO meta (key, value) VALUES ('schema_version', '32');

-- ========================================
-- Roots
-- ========================================
-- Monitored filesystem root directories.
CREATE TABLE IF NOT EXISTS roots (
    root_id INTEGER PRIMARY KEY AUTOINCREMENT,
    root_path TEXT NOT NULL UNIQUE,
    -- Set under the SQLite write lock at the start of a checkpoint and
    -- cleared under the lock at the end of a successful checkpoint.
    -- Survives an interrupted checkpoint as a debugging aid (next run
    -- overwrites it on entry).
    last_checkpoint_started_at INTEGER,
    -- Set under the lock at the end of a successful checkpoint. NULL
    -- means no checkpoint has ever completed for this root, OR the
    -- most recent checkpoint was interrupted before completion.
    -- This is the root-level "everything confirmed as of" timestamp;
    -- item_versions carries no per-row confirmation time.
    last_checkpoint_completed_at INTEGER,
    -- User setting: 1 = watch this root for realtime changes when the
    -- platform supports it, 0 = reconcile walks only. Watcher health
    -- (watching / degraded / unsupported) is runtime state, not stored.
    watch_enabled   INTEGER NOT NULL DEFAULT 1,
    -- JSON array of glob patterns excluded from watching and reconcile.
    -- NULL means no exclusions.
    ignore_patterns TEXT
);

-- Indexes to optimize queries
CREATE INDEX IF NOT EXISTS idx_roots_path ON roots (root_path COLLATE natural_path);

-- ========================================
-- Scans
-- ========================================
-- Integrity scan sessions (hash and/or validation).
CREATE TABLE IF NOT EXISTS scans (
    scan_id INTEGER PRIMARY KEY AUTOINCREMENT,
    root_id INTEGER NOT NULL,
    started_at INTEGER NOT NULL,
    ended_at INTEGER DEFAULT NULL,
    was_restarted BOOLEAN NOT NULL DEFAULT 0,
    state INTEGER NOT NULL,            -- 1 = Scanning, 2 = Completed, 3 = Stopped, 4 = Error
    is_hash BOOLEAN NOT NULL,
    hash_all BOOLEAN NOT NULL,
    is_val BOOLEAN NOT NULL,

    -- What this scan discovered (deltas only)
    new_hash_suspect_count INTEGER DEFAULT NULL,
    new_val_invalid_count INTEGER DEFAULT NULL,

    error TEXT DEFAULT NULL,

    -- Schedule reference
    schedule_id INTEGER DEFAULT NULL,
    FOREIGN KEY (root_id) REFERENCES roots(root_id),
    FOREIGN KEY (schedule_id) REFERENCES scan_schedules(schedule_id)
);

CREATE INDEX IF NOT EXISTS idx_scans_root ON scans (root_id);

-- ========================================
-- Snapshots
-- ========================================
-- Per-root rollups of filesystem state and integrity posture.
-- For rows written by this schema version onward, taken_at is the start
-- of an hour bucket and the row describes the state at the end of that
-- bucket; hours with no activity have no row. Rows migrated from old
-- scans keep the scan's start time.
CREATE TABLE IF NOT EXISTS snapshots (
    root_id         INTEGER NOT NULL,
    taken_at        INTEGER NOT NULL,

    -- Filesystem state (alive items at snapshot time)
    file_count      INTEGER,
    folder_count    INTEGER,
    total_size      INTEGER,

    -- Change counts since previous snapshot for this root
    add_count       INTEGER,
    modify_count    INTEGER,
    delete_count    INTEGER,

    -- Integrity posture (alive files at snapshot time)
    val_unknown_count       INTEGER,
    val_valid_count         INTEGER,
    val_invalid_count       INTEGER,
    val_no_validator_count  INTEGER,
    hash_unknown_count      INTEGER,
    hash_baseline_count     INTEGER,
    hash_suspect_count      INTEGER,

    -- Schedule reference
    schedule_id     INTEGER DEFAULT NULL,

    PRIMARY KEY (root_id, taken_at),
    FOREIGN KEY (root_id) REFERENCES roots(root_id)
) WITHOUT ROWID;

-- ========================================
-- Items
-- ========================================
-- Stable identity for each filesystem item across all its versions.
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
CREATE INDEX IF NOT EXISTS idx_items_parent_name ON items (root_id, parent_item_id, item_name COLLATE natural_path);

-- ========================================
-- Item versions
-- ========================================
-- One row per distinct state of an item. Identity (path, type, root) lives in items.
-- item_version is a per-item sequence number (1, 2, 3, …, n) assigned chronologically.
--
-- A version is in effect from first_seen_at (epoch seconds) until the
-- next version of the same item begins. Unchanged items are never
-- rewritten; "confirmed as of" lives on roots.last_checkpoint_completed_at.
--
-- Hourly coalescing: an alive item gets at most one version per hour
-- bucket (first_seen_at / 3600). Further changes inside that bucket
-- update the row in place (mod_date, size, access, last_changed_at) and
-- increment change_count. A version is never modified after its bucket
-- ends. Alive<->deleted transitions always create a new version.
--
-- first_seen_at:   when this version began.
-- last_changed_at: when its state was last updated (= first_seen_at
--                  unless coalesced).
-- change_count:    distinct observed states folded into this version.
-- source:          1=Reconcile walk, 2=Watcher, 3=Integrity check.
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
    last_changed_at INTEGER NOT NULL,
    change_count    INTEGER NOT NULL DEFAULT 1,
    source          INTEGER NOT NULL DEFAULT 1,

    -- State
    is_added        BOOLEAN NOT NULL DEFAULT 0,
    is_deleted      BOOLEAN NOT NULL DEFAULT 0,
    access          INTEGER NOT NULL DEFAULT 0,

    -- File-specific fields (NULL for folders)
    mod_date        INTEGER,
    size            INTEGER,

    -- Validation state (files only, NULL for folders and unvalidated files)
    val_at          INTEGER,            -- epoch: when this version was validated
    val_state       INTEGER,            -- 1=Valid, 2=Invalid
    val_error       TEXT,               -- error details when val_state=Invalid

    -- User review of integrity issues
    val_reviewed_at  INTEGER DEFAULT NULL,
    hash_reviewed_at INTEGER DEFAULT NULL,

    PRIMARY KEY (item_id, item_version),
    FOREIGN KEY (item_id) REFERENCES items(item_id),
    FOREIGN KEY (root_id) REFERENCES roots(root_id)
) WITHOUT ROWID;

CREATE INDEX IF NOT EXISTS idx_versions_root_firstseen_hid ON item_versions (root_id, first_seen_at, hierarchy_id);
CREATE INDEX IF NOT EXISTS idx_versions_parent_firstseen ON item_versions (parent_item_id, first_seen_at);
CREATE INDEX IF NOT EXISTS idx_versions_val_at ON item_versions (val_at, val_state);

-- ========================================
-- Hash versions
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

-- ========================================
-- Scan schedules
-- ========================================
-- Recurring scan configurations.
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

-- ========================================
-- Tasks
-- ========================================
-- Work items and their execution history.
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
