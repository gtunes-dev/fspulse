use crate::error::FsPulseError;
use crate::scans::{HashMode, ValidateMode};
use crate::task::{ScanSettings, TaskType};
use rusqlite::Connection;

/// Pre-SQL: Create the new task_queue table with the new schema.
/// We create it as a new table because we need to transform data in Rust code.
pub const UPGRADE_13_TO_14_PRE_SQL: &str = r#"
--
-- Schema Upgrade: Version 13 → 14 (Pre-SQL Phase)
--
-- This migration evolves scan_queue into task_queue to support multiple task types.
-- The actual data migration happens in Rust code (Phase 2).
--

-- Verify schema version is exactly 13
SELECT 1 / (CASE WHEN (SELECT value FROM meta WHERE key = 'schema_version') = '13' THEN 1 ELSE 0 END);

-- ========================================
-- Create new task_queue table
-- ========================================

CREATE TABLE task_queue (
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
"#;

/// Rust code phase: Migrate data from scan_queue to task_queue.
/// This generates JSON task_settings from hash_mode and validate_mode columns.
pub fn migrate_13_to_14(conn: &Connection) -> Result<(), FsPulseError> {
    // Read all rows from scan_queue
    let mut stmt = conn
        .prepare(
            "SELECT queue_id, root_id, schedule_id, scan_id, next_scan_time,
                    hash_mode, validate_mode, source, created_at, analysis_hwm
             FROM scan_queue",
        )
        .map_err(FsPulseError::DatabaseError)?;

    let rows: Vec<_> = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,         // queue_id
                row.get::<_, i64>(1)?,         // root_id
                row.get::<_, Option<i64>>(2)?, // schedule_id
                row.get::<_, Option<i64>>(3)?, // scan_id
                row.get::<_, Option<i64>>(4)?, // next_scan_time
                row.get::<_, i32>(5)?,         // hash_mode
                row.get::<_, i32>(6)?,         // validate_mode
                row.get::<_, i32>(7)?,         // source
                row.get::<_, i64>(8)?,         // created_at
                row.get::<_, Option<i64>>(9)?, // analysis_hwm
            ))
        })
        .map_err(FsPulseError::DatabaseError)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(FsPulseError::DatabaseError)?;

    drop(stmt);

    // Insert into task_queue with transformed data
    let mut insert_stmt = conn
        .prepare(
            "INSERT INTO task_queue (
                queue_id, task_type, is_active, root_id, schedule_id, scan_id,
                next_run_time, source, task_settings, analysis_hwm, created_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .map_err(FsPulseError::DatabaseError)?;

    for (
        queue_id,
        root_id,
        schedule_id,
        scan_id,
        next_scan_time,
        hash_mode,
        validate_mode,
        source,
        created_at,
        analysis_hwm,
    ) in rows
    {
        // All existing entries are Scan tasks
        let task_type = TaskType::Scan.as_i64();

        // is_active = true if scan_id IS NOT NULL (in-progress scan)
        let is_active = scan_id.is_some();

        // Convert integer modes to enums and create typed settings
        let hash_mode_enum = HashMode::from_i32(hash_mode).ok_or_else(|| {
            FsPulseError::Error(format!("Invalid hash_mode value: {}", hash_mode))
        })?;
        let validate_mode_enum = ValidateMode::from_i32(validate_mode).ok_or_else(|| {
            FsPulseError::Error(format!("Invalid validate_mode value: {}", validate_mode))
        })?;

        // Generate JSON task_settings using typed struct for consistent format
        let settings = ScanSettings::new(hash_mode_enum, validate_mode_enum);
        let task_settings = settings.to_json()?;

        insert_stmt
            .execute(rusqlite::params![
                queue_id,
                task_type,
                is_active,
                root_id,
                schedule_id,
                scan_id,
                next_scan_time,
                source,
                task_settings,
                analysis_hwm,
                created_at,
            ])
            .map_err(FsPulseError::DatabaseError)?;
    }

    Ok(())
}

/// Post-SQL: Drop old table and create indexes.
pub const UPGRADE_13_TO_14_POST_SQL: &str = r#"
--
-- Schema Upgrade: Version 13 → 14 (Post-SQL Phase)
--
-- Clean up old scan_queue table and create indexes for task_queue.
--

-- Drop old table
DROP TABLE scan_queue;

-- Create indexes for new table
CREATE INDEX idx_task_queue_type_next ON task_queue(task_type, next_run_time);
CREATE INDEX idx_task_queue_source_next ON task_queue(source, next_run_time);
CREATE INDEX idx_task_queue_root ON task_queue(root_id);
CREATE UNIQUE INDEX idx_task_queue_schedule ON task_queue(schedule_id) WHERE schedule_id IS NOT NULL;

-- ========================================
-- Finalize migration
-- ========================================

UPDATE meta SET value = '14' WHERE key = 'schema_version';
"#;
