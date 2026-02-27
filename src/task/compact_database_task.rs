use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::db::Database;
use crate::error::FsPulseError;
use crate::utils::Utils;

use super::progress::TaskProgress;
use super::task_type::TaskType;
use super::traits::Task;

// ============================================================================
// CompactDatabaseSettings - Compact-specific task settings
// ============================================================================

/// Settings for a compact database task.
///
/// This task requires no configuration, but each task type has its own
/// settings struct for consistency. Serializes to `{}`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CompactDatabaseSettings {}

impl CompactDatabaseSettings {
    /// Serialize to JSON string for storage in database
    pub fn to_json(&self) -> Result<String, FsPulseError> {
        serde_json::to_string(self).map_err(|e| {
            FsPulseError::Error(format!("Failed to serialize CompactDatabaseSettings: {}", e))
        })
    }

    /// Deserialize from JSON string retrieved from database
    #[allow(dead_code)]
    pub fn from_json(json: &str) -> Result<Self, FsPulseError> {
        serde_json::from_str(json).map_err(|e| {
            FsPulseError::Error(format!(
                "Failed to deserialize CompactDatabaseSettings: {}",
                e
            ))
        })
    }
}

// ============================================================================
// CompactDatabaseTask
// ============================================================================

/// A database compaction task that runs SQLite VACUUM.
///
/// VACUUM is atomic from SQLite's perspective — it either completes fully
/// or rolls back. The task spawns a worker thread for VACUUM so the main
/// task thread can update elapsed time in the progress display.
///
/// This task ignores the interrupt token because VACUUM cannot be safely
/// interrupted mid-execution. When VACUUM finishes, success is reported
/// regardless of whether an interrupt was requested.
pub struct CompactDatabaseTask {
    task_id: i64,
}

impl CompactDatabaseTask {
    pub fn new(task_id: i64) -> Self {
        Self { task_id }
    }
}

impl Task for CompactDatabaseTask {
    fn run(
        &mut self,
        progress: Arc<TaskProgress>,
        _interrupt_token: Arc<AtomicBool>,
    ) -> Result<(), FsPulseError> {
        progress.set_indeterminate_progress("Elapsed: 0s");

        let handle = std::thread::spawn(Database::compact);
        let start = Instant::now();

        loop {
            if handle.is_finished() {
                return handle.join().unwrap();
            }
            let elapsed = Utils::format_elapsed(start.elapsed());
            progress.set_indeterminate_progress(&format!("Elapsed: {}", elapsed));
            std::thread::sleep(Duration::from_secs(1));
        }
    }

    fn task_type(&self) -> TaskType {
        TaskType::CompactDatabase
    }

    fn task_id(&self) -> i64 {
        self.task_id
    }

    fn active_root_id(&self) -> Option<i64> {
        None
    }

    fn action(&self) -> &str {
        "Compacting database"
    }

    fn display_target(&self) -> String {
        String::new()
    }

    fn on_stopped(&mut self) -> Result<(), FsPulseError> {
        // VACUUM is atomic — nothing to rollback
        Ok(())
    }

    fn on_error(&mut self, _error_msg: &str) -> Result<(), FsPulseError> {
        // Nothing to clean up
        Ok(())
    }

    fn is_exclusive(&self) -> bool {
        true
    }

    fn is_stoppable(&self) -> bool {
        false
    }

    fn is_pausable(&self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compact_database_settings_round_trip() {
        let settings = CompactDatabaseSettings {};
        let json = settings.to_json().unwrap();
        assert_eq!(json, "{}");
        let restored = CompactDatabaseSettings::from_json(&json).unwrap();
        assert_eq!(settings, restored);
    }

    #[test]
    fn test_compact_database_settings_deserialize_with_extra_fields() {
        let json = r#"{"unknown_field":123}"#;
        let settings = CompactDatabaseSettings::from_json(json).unwrap();
        assert_eq!(settings, CompactDatabaseSettings {});
    }

    #[test]
    fn test_compact_database_task_metadata() {
        let task = CompactDatabaseTask::new(42);
        assert_eq!(task.task_type(), TaskType::CompactDatabase);
        assert_eq!(task.task_id(), 42);
        assert_eq!(task.active_root_id(), None);
        assert_eq!(task.action(), "Compacting database");
        assert_eq!(task.display_target(), "");
        assert!(task.is_exclusive());
    }
}
