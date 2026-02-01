pub const UPGRADE_8_TO_9_SQL: &str = r#"
--
-- Schema Upgrade: Version 8 → 9
--
-- This migration adds recurring scan scheduling infrastructure:
-- - scan_schedules: Persistent schedule configurations (daily/weekly/interval)
-- - scan_queue: Active work registry (scheduled + manual scans)
--
-- Design principles:
-- - Multiple schedules per root allowed (no UNIQUE constraint on root_id)
-- - Queue acts as persistent registry (1 enabled schedule = 1 queue entry)
-- - Fairness via next_scan_time (most overdue work runs first)
-- - Numeric enums for consistency with existing patterns
--

-- Verify schema version is exactly 8
SELECT 1 / (CASE WHEN (SELECT value FROM meta WHERE key = 'schema_version') = '8' THEN 1 ELSE 0 END);

-- ========================================
-- Create scan_schedules table
-- ========================================

CREATE TABLE scan_schedules (
    schedule_id INTEGER PRIMARY KEY AUTOINCREMENT,
    root_id INTEGER NOT NULL,
    enabled BOOLEAN NOT NULL DEFAULT 1,

    -- User-friendly name for this schedule
    schedule_name TEXT NOT NULL,

    -- Schedule type: 0=daily, 1=weekly, 2=interval, 3=monthly
    schedule_type INTEGER NOT NULL CHECK(schedule_type IN (0, 1, 2, 3)),

    -- For daily/weekly/monthly schedules: 'HH:MM' format (24-hour)
    time_of_day TEXT,

    -- For weekly schedules: JSON array of day names
    -- Example: '["Mon","Wed","Fri"]'
    days_of_week TEXT,

    -- For monthly schedules: day of month (1-31)
    -- If day doesn't exist in month (e.g., 31 in Feb), skip to next valid occurrence
    day_of_month INTEGER,

    -- For interval schedules: repeat every N minutes/hours/days/weeks
    interval_value INTEGER,
    interval_unit INTEGER CHECK(interval_unit IN (0, 1, 2, 3)),  -- 0=minutes, 1=hours, 2=days, 3=weeks

    -- Scan options (numeric enums matching existing patterns)
    hash_mode INTEGER NOT NULL CHECK(hash_mode IN (0, 1, 2)),     -- 0=None, 1=New, 2=All
    validate_mode INTEGER NOT NULL CHECK(validate_mode IN (0, 1, 2)),  -- 0=None, 1=New, 2=All

    -- Metadata
    created_at INTEGER NOT NULL,  -- Unix timestamp (UTC)
    updated_at INTEGER NOT NULL,  -- Unix timestamp (UTC)

    FOREIGN KEY (root_id) REFERENCES roots(root_id)
    -- Note: NO ON DELETE CASCADE - explicit cleanup required in code
    -- Note: NO UNIQUE(root_id) - multiple schedules per root allowed
);

-- Find enabled schedules efficiently (used by queue sync)
CREATE INDEX idx_scan_schedules_enabled ON scan_schedules(enabled);

-- Find schedules by root (used by UI and API)
CREATE INDEX idx_scan_schedules_root ON scan_schedules(root_id);

-- ========================================
-- Create scan_queue table
-- ========================================

CREATE TABLE scan_queue (
    queue_id INTEGER PRIMARY KEY AUTOINCREMENT,
    root_id INTEGER NOT NULL,
    schedule_id INTEGER,  -- NULL for manual scans
    scan_id INTEGER,      -- NULL until scan starts, set when TaskManager starts the scan

    -- When this work should run (Unix timestamp, UTC)
    -- For scheduled work: calculated when scan starts, updated after completion, NULL when disabled
    -- For manual work: set to now() for immediate execution
    next_scan_time INTEGER,

    -- Scan configuration
    hash_mode INTEGER NOT NULL CHECK(hash_mode IN (0, 1, 2)),
    validate_mode INTEGER NOT NULL CHECK(validate_mode IN (0, 1, 2)),

    -- Source: 0=manual, 1=scheduled
    source INTEGER NOT NULL CHECK(source IN (0, 1)),

    -- Metadata
    created_at INTEGER NOT NULL,  -- Unix timestamp (UTC)

    FOREIGN KEY (root_id) REFERENCES roots(root_id),
    FOREIGN KEY (schedule_id) REFERENCES scan_schedules(schedule_id),
    FOREIGN KEY (scan_id) REFERENCES scans(scan_id)
    -- Note: NO ON DELETE CASCADE - explicit cleanup required in code
    -- Invariant: At most ONE row can have scan_id NOT NULL at any time
);

-- Find next work to run (filter by source, order by next_scan_time)
CREATE INDEX idx_scan_queue_source_next ON scan_queue(source, next_scan_time);

-- Ensure only one queue entry per schedule
CREATE UNIQUE INDEX idx_scan_queue_schedule ON scan_queue(schedule_id)
    WHERE schedule_id IS NOT NULL;

-- Find queue entries by root (for UI display)
CREATE INDEX idx_scan_queue_root ON scan_queue(root_id);

-- ========================================
-- Finalize migration
-- ========================================

UPDATE meta SET value = '9' WHERE key = 'schema_version';
"#;
