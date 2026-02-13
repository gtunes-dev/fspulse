use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use rusqlite::Connection;

use crate::database::Database;
use crate::error::FsPulseError;
use crate::roots::Root;
use crate::scanner::Scanner;
use crate::scans::{AnalysisSpec, Scan};
use crate::schedules::{Schedule, TaskRow};

use super::progress::TaskProgress;
use super::settings::ScanSettings;
use super::task_type::TaskType;
use super::traits::Task;

/// A scan task that implements the Task trait
///
/// Wraps the Scanner's state machine execution into a Task-compatible interface.
/// Contains the Scan record and Root needed for execution.
pub struct ScanTask {
    task_id: i64,
    scan: Scan,
    root: Root,
    /// Initial task_state JSON loaded from the tasks table (for resume).
    /// Consumed (taken) when `run()` is called.
    initial_task_state: Option<String>,
}

impl ScanTask {
    fn new(task_id: i64, scan: Scan, root: Root, initial_task_state: Option<String>) -> Self {
        Self {
            task_id,
            scan,
            root,
            initial_task_state,
        }
    }

    /// Factory: create a ScanTask from a task row, either resuming or starting a new scan
    ///
    /// IMPORTANT: Must be called within an immediate transaction
    pub fn from_task_row(conn: &Connection, row: &TaskRow, now: i64) -> Result<Box<dyn Task>, FsPulseError> {
        let root = Root::get_by_id(conn, row.root_id)?
            .ok_or_else(|| FsPulseError::Error(format!("Root {} not found", row.root_id)))?;

        let scan = if let Some(scan_id) = row.scan_id {
            // Resume case: mark restarted and load existing scan
            conn.execute(
                "UPDATE scans SET was_restarted = 1 WHERE scan_id = ?",
                rusqlite::params![scan_id],
            )
            .map_err(FsPulseError::DatabaseError)?;

            Scan::get_by_id_or_latest(conn, Some(scan_id), None)?
                .ok_or_else(|| FsPulseError::Error(format!("Scan {} not found", scan_id)))?
        } else {
            // New scan: create scan record and mark task as running
            let settings = ScanSettings::from_json(&row.task_settings)?;
            let analysis_spec = AnalysisSpec::from_modes(settings.hash_mode, settings.validate_mode);
            let scan = Scan::create(conn, &root, row.schedule_id, &analysis_spec)?;

            // Mark task as Running with scan_id and started_at
            conn.execute(
                "UPDATE tasks SET scan_id = ?, status = 1, started_at = ? WHERE task_id = ?",
                rusqlite::params![scan.scan_id(), now, row.task_id],
            )
            .map_err(FsPulseError::DatabaseError)?;

            // For scheduled scans, create a new Pending row for the next occurrence
            if let Some(schedule_id) = row.schedule_id {
                let schedule = Schedule::get_by_id(conn, schedule_id)?.ok_or_else(|| {
                    FsPulseError::Error(format!("Schedule {} not found", schedule_id))
                })?;

                let next_time = schedule.calculate_next_scan_time(now).map_err(|e| {
                    FsPulseError::Error(format!("Failed to calculate next scan time: {}", e))
                })?;

                // Verify no other Pending row exists for this schedule_id
                let pending_exists: bool = conn
                    .query_row(
                        "SELECT COUNT(*) FROM tasks WHERE schedule_id = ? AND status = 0 AND task_id != ?",
                        rusqlite::params![schedule_id, row.task_id],
                        |row| row.get::<_, i64>(0),
                    )
                    .map(|count| count > 0)
                    .map_err(FsPulseError::DatabaseError)?;

                if !pending_exists {
                    conn.execute(
                        "INSERT INTO tasks (
                            task_type, status, root_id, schedule_id, scan_id, run_at,
                            source, task_settings, created_at
                        ) VALUES (?, 0, ?, ?, NULL, ?, ?, ?, ?)",
                        rusqlite::params![
                            TaskType::Scan.as_i64(),
                            row.root_id,
                            schedule_id,
                            next_time,
                            crate::schedules::SourceType::Scheduled.as_i32(),
                            row.task_settings,
                            now,
                        ],
                    )
                    .map_err(FsPulseError::DatabaseError)?;
                }
            }

            scan
        };

        Ok(Box::new(ScanTask::new(row.task_id, scan, root, row.task_state.clone())))
    }
}

impl Task for ScanTask {
    fn run(
        &mut self,
        progress: Arc<TaskProgress>,
        interrupt_token: Arc<AtomicBool>,
    ) -> Result<(), FsPulseError> {
        Scanner::do_scan_machine(
            &mut self.scan,
            &self.root,
            self.task_id,
            self.initial_task_state.take(),
            progress,
            interrupt_token,
        )
    }

    fn task_type(&self) -> TaskType {
        TaskType::Scan
    }

    fn task_id(&self) -> i64 {
        self.task_id
    }

    fn active_root_id(&self) -> Option<i64> {
        Some(self.scan.root_id())
    }

    fn action(&self) -> &str {
        "Scanning"
    }

    fn display_target(&self) -> String {
        self.root.root_path().to_string()
    }

    fn on_stopped(&mut self) -> Result<(), FsPulseError> {
        let conn = Database::get_connection()?;
        self.scan.set_state_stopped(&conn)
    }

    fn on_error(&mut self, error_msg: &str) -> Result<(), FsPulseError> {
        let conn = Database::get_connection()?;
        Scan::stop_scan(&conn, &self.scan, Some(error_msg))
    }
}
