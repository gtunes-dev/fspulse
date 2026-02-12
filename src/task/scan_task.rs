use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use rusqlite::Connection;

use crate::database::Database;
use crate::error::FsPulseError;
use crate::roots::Root;
use crate::scanner::Scanner;
use crate::scans::{AnalysisSpec, Scan};
use crate::schedules::{QueueRow, Schedule};

use super::progress::TaskProgress;
use super::settings::ScanSettings;
use super::task_type::TaskType;
use super::traits::Task;

/// A scan task that implements the Task trait
///
/// Wraps the Scanner's state machine execution into a Task-compatible interface.
/// Contains the Scan record and Root needed for execution.
pub struct ScanTask {
    queue_id: i64,
    scan: Scan,
    root: Root,
}

impl ScanTask {
    fn new(queue_id: i64, scan: Scan, root: Root) -> Self {
        Self {
            queue_id,
            scan,
            root,
        }
    }

    /// Factory: create a ScanTask from a queue row, either resuming or starting a new scan
    ///
    /// IMPORTANT: Must be called within an immediate transaction
    pub fn from_queue(conn: &Connection, row: &QueueRow, now: i64) -> Result<Box<dyn Task>, FsPulseError> {
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
            // New scan: create scan record and mark queue entry active
            let settings = ScanSettings::from_json(&row.task_settings)?;
            let analysis_spec = AnalysisSpec::from_modes(settings.hash_mode, settings.validate_mode);
            let scan = Scan::create(conn, &root, row.schedule_id, &analysis_spec)?;

            // Mark queue entry active with scan_id
            conn.execute(
                "UPDATE task_queue SET scan_id = ?, is_active = 1 WHERE queue_id = ?",
                rusqlite::params![scan.scan_id(), row.queue_id],
            )
            .map_err(FsPulseError::DatabaseError)?;

            // For scheduled scans, calculate and set next_run_time
            if let Some(schedule_id) = row.schedule_id {
                let schedule = Schedule::get_by_id(conn, schedule_id)?.ok_or_else(|| {
                    FsPulseError::Error(format!("Schedule {} not found", schedule_id))
                })?;

                let next_time = schedule.calculate_next_scan_time(now).map_err(|e| {
                    FsPulseError::Error(format!("Failed to calculate next scan time: {}", e))
                })?;

                conn.execute(
                    "UPDATE task_queue SET next_run_time = ? WHERE queue_id = ?",
                    rusqlite::params![next_time, row.queue_id],
                )
                .map_err(FsPulseError::DatabaseError)?;
            }

            scan
        };

        Ok(Box::new(ScanTask::new(row.queue_id, scan, root)))
    }
}

impl Task for ScanTask {
    fn run(
        &mut self,
        progress: Arc<TaskProgress>,
        interrupt_token: Arc<AtomicBool>,
    ) -> Result<(), FsPulseError> {
        Scanner::do_scan_machine(&mut self.scan, &self.root, progress, interrupt_token)
    }

    fn task_type(&self) -> TaskType {
        TaskType::Scan
    }

    fn queue_id(&self) -> i64 {
        self.queue_id
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
