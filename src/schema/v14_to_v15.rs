use crate::error::FsPulseError;
use rusqlite::Connection;

/// Pre-SQL: Create the new `tasks` table with lifecycle columns.
/// The actual data migration happens in Rust code (Phase 2).
pub const UPGRADE_14_TO_15_PRE_SQL: &str = r#"
--
-- Schema Upgrade: Version 14 → 15 (Pre-SQL Phase)
--
-- This migration evolves task_queue into tasks:
-- - queue_id → task_id
-- - is_active BOOLEAN → status INTEGER (TaskStatus enum)
-- - next_run_time → run_at (0 = immediately)
-- - analysis_hwm → task_state (generic JSON)
-- - Added: started_at, completed_at lifecycle timestamps
-- - FK schedule_id now ON DELETE SET NULL
-- - Rows are never deleted — completed tasks become historical records
--

-- Verify schema version is exactly 14
SELECT 1 / (CASE WHEN (SELECT value FROM meta WHERE key = 'schema_version') = '14' THEN 1 ELSE 0 END);

-- ========================================
-- Create new tasks table
-- ========================================

CREATE TABLE tasks (
    task_id INTEGER PRIMARY KEY AUTOINCREMENT,
    task_type INTEGER NOT NULL DEFAULT 0,          -- TaskType enum: 0=Scan

    -- Lifecycle status (replaces is_active BOOLEAN)
    status INTEGER NOT NULL DEFAULT 0,             -- TaskStatus enum: 0=Pending, 1=Running, 2=Completed, 3=Stopped, 4=Error

    -- Root reference
    root_id INTEGER,

    -- Schedule reference (ON DELETE SET NULL so completed tasks survive schedule deletion)
    schedule_id INTEGER,

    -- Scan-specific: FK to scans table
    scan_id INTEGER,

    -- Scheduling
    run_at INTEGER NOT NULL DEFAULT 0,             -- When eligible to run (0 = immediately)
    source INTEGER NOT NULL CHECK(source IN (0, 1)), -- 0=manual, 1=scheduled

    -- Task configuration (immutable JSON — permanent artifact)
    task_settings TEXT NOT NULL,

    -- Transient execution state (generic JSON, NULL when not running)
    -- For scan tasks: {"high_water_mark": 12345}
    task_state TEXT,

    -- Timestamps
    created_at INTEGER NOT NULL,
    started_at INTEGER,                            -- When execution began
    completed_at INTEGER,                          -- When terminal status was reached

    FOREIGN KEY (schedule_id) REFERENCES scan_schedules(schedule_id) ON DELETE SET NULL,
    FOREIGN KEY (root_id) REFERENCES roots(root_id),
    FOREIGN KEY (scan_id) REFERENCES scans(scan_id)
);
"#;

/// Rust code phase: Migrate data from task_queue to tasks.
/// Transforms is_active → status, next_run_time → run_at, analysis_hwm → task_state JSON.
pub fn migrate_14_to_15(conn: &Connection) -> Result<(), FsPulseError> {
    // Read all rows from task_queue
    let mut stmt = conn
        .prepare(
            "SELECT queue_id, task_type, is_active, root_id, schedule_id, scan_id,
                    next_run_time, source, task_settings, analysis_hwm, created_at
             FROM task_queue",
        )
        .map_err(FsPulseError::DatabaseError)?;

    let rows: Vec<_> = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,         // queue_id → task_id
                row.get::<_, i64>(1)?,         // task_type
                row.get::<_, bool>(2)?,        // is_active
                row.get::<_, Option<i64>>(3)?, // root_id
                row.get::<_, Option<i64>>(4)?, // schedule_id
                row.get::<_, Option<i64>>(5)?, // scan_id
                row.get::<_, Option<i64>>(6)?, // next_run_time
                row.get::<_, i32>(7)?,         // source
                row.get::<_, String>(8)?,      // task_settings
                row.get::<_, Option<i64>>(9)?, // analysis_hwm
                row.get::<_, i64>(10)?,        // created_at
            ))
        })
        .map_err(FsPulseError::DatabaseError)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(FsPulseError::DatabaseError)?;

    drop(stmt);

    // Insert into tasks with transformed data
    let mut insert_stmt = conn
        .prepare(
            "INSERT INTO tasks (
                task_id, task_type, status, root_id, schedule_id, scan_id,
                run_at, source, task_settings, task_state, created_at, started_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .map_err(FsPulseError::DatabaseError)?;

    for (
        queue_id,
        task_type,
        is_active,
        root_id,
        schedule_id,
        scan_id,
        next_run_time,
        source,
        task_settings,
        analysis_hwm,
        created_at,
    ) in rows
    {
        // status: Running (1) if active, Pending (0) otherwise
        let status: i64 = if is_active { 1 } else { 0 };

        // run_at: next_run_time if set, 0 (immediately) otherwise
        let run_at: i64 = next_run_time.unwrap_or(0);

        // task_state: convert analysis_hwm integer to ScanTaskState JSON if present
        let task_state: Option<String> = analysis_hwm.map(|hwm| format!("{{\"high_water_mark\":{}}}", hwm));

        // started_at: best approximation — use created_at if active
        let started_at: Option<i64> = if is_active { Some(created_at) } else { None };

        insert_stmt
            .execute(rusqlite::params![
                queue_id,    // task_id preserves original queue_id
                task_type,
                status,
                root_id,
                schedule_id,
                scan_id,
                run_at,
                source,
                task_settings,
                task_state,
                created_at,
                started_at,
            ])
            .map_err(FsPulseError::DatabaseError)?;
    }

    Ok(())
}

/// Post-SQL: Drop old table, create indexes, update schema version.
pub const UPGRADE_14_TO_15_POST_SQL: &str = r#"
--
-- Schema Upgrade: Version 14 → 15 (Post-SQL Phase)
--
-- Clean up old task_queue table and create indexes for tasks.
--

-- Drop old table
DROP TABLE task_queue;

-- Create indexes for new table
CREATE INDEX idx_tasks_status_source_runat ON tasks(status, source, run_at, task_id);
CREATE INDEX idx_tasks_schedule ON tasks(schedule_id) WHERE schedule_id IS NOT NULL;
CREATE INDEX idx_tasks_root ON tasks(root_id);

-- ========================================
-- Finalize migration
-- ========================================

UPDATE meta SET value = '15' WHERE key = 'schema_version';
"#;
