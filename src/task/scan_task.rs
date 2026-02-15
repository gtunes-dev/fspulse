use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

use crate::database::Database;
use crate::error::FsPulseError;
use crate::roots::Root;
use crate::scanner::Scanner;
use crate::schedules::TaskEntry;
use crate::scans::{AnalysisSpec, HashMode, Scan, ValidateMode};

use super::progress::TaskProgress;
use super::task_type::TaskType;
use super::traits::Task;

// ============================================================================
// ScanSettings - Scan-specific task settings
// ============================================================================

/// Settings for a scan task.
///
/// This struct is serialized to JSON and stored in the `task_settings` column
/// of the `tasks` table. Using a typed struct instead of raw JSON provides:
/// - Type safety at compile time
/// - Automatic validation during deserialization
/// - Easy evolution with `#[serde(default)]` for new fields
///
/// # Evolution
/// When adding new fields:
/// 1. Add the field with `#[serde(default)]` or `#[serde(default = "default_fn")]`
/// 2. Existing rows in the database will deserialize correctly with the default value
/// 3. No migration needed for backwards compatibility
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ScanSettings {
    pub hash_mode: HashMode,
    pub validate_mode: ValidateMode,
}

impl ScanSettings {
    /// Create new scan settings
    pub fn new(hash_mode: HashMode, validate_mode: ValidateMode) -> Self {
        Self {
            hash_mode,
            validate_mode,
        }
    }

    /// Serialize to JSON string for storage in database
    pub fn to_json(&self) -> Result<String, FsPulseError> {
        serde_json::to_string(self)
            .map_err(|e| FsPulseError::Error(format!("Failed to serialize ScanSettings: {}", e)))
    }

    /// Deserialize from JSON string retrieved from database
    pub fn from_json(json: &str) -> Result<Self, FsPulseError> {
        serde_json::from_str(json)
            .map_err(|e| FsPulseError::Error(format!("Failed to deserialize ScanSettings: {}", e)))
    }
}

// ============================================================================
// ScanTaskState - Scan-specific task state for restart resilience
// ============================================================================

/// Scan-specific task state stored in the tasks.task_state JSON column.
/// Contains state needed to resume a scan after interruption.
/// Preserved at completion as a historical artifact.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanTaskState {
    #[serde(default)]
    pub scan_id: Option<i64>,
    pub high_water_mark: i64,
}

impl ScanTaskState {
    pub fn new() -> Self {
        Self {
            scan_id: None,
            high_water_mark: 0,
        }
    }

    pub fn from_task_state(task_state: Option<&str>) -> Result<Self, FsPulseError> {
        match task_state {
            Some(json) => serde_json::from_str(json)
                .map_err(|e| FsPulseError::Error(format!("Invalid ScanTaskState JSON: {}", e))),
            None => Ok(Self::new()),
        }
    }

    pub fn to_json(&self) -> Result<String, FsPulseError> {
        serde_json::to_string(self)
            .map_err(|e| FsPulseError::Error(format!("Failed to serialize ScanTaskState: {}", e)))
    }
}

// ============================================================================
// AnalysisTracker - Tracks in-flight analysis items for HWM management
// ============================================================================

/// Internal state protected by the AnalysisTracker mutex
struct AnalysisTrackerInner {
    /// Sorted vector of item IDs currently being processed
    in_flight: Vec<i64>,
    /// Scan-specific task state (persisted to DB as JSON)
    state: ScanTaskState,
}

/// Tracks in-flight analysis items and manages the high water mark (HWM) for
/// restart resilience. The HWM represents the highest item_id such that all
/// items with id <= HWM have been fully processed.
///
/// The HWM is persisted in the `task_state` JSON column via `ScanTaskState`.
///
/// Thread-safe: wrapped in Arc and shared between the main thread (which adds
/// batches) and worker threads (which complete items). Internal Mutex protects
/// both the in-flight vector and the ScanTaskState.
pub struct AnalysisTracker {
    inner: Mutex<AnalysisTrackerInner>,
    /// The task_id for this analysis session
    task_id: i64,
}

impl AnalysisTracker {
    /// Create a new tracker for the given task with initial state.
    pub fn new(task_id: i64, initial_state: ScanTaskState) -> Self {
        Self {
            inner: Mutex::new(AnalysisTrackerInner {
                in_flight: Vec::new(),
                state: initial_state,
            }),
            task_id,
        }
    }

    /// Get the current high water mark
    #[allow(dead_code)]
    pub fn high_water_mark(&self) -> i64 {
        self.inner.lock().unwrap().state.high_water_mark
    }

    /// Add a batch of item IDs to track. Called by main thread after fetching.
    pub fn add_batch(&self, ids: impl Iterator<Item = i64>) {
        let mut inner = self.inner.lock().unwrap();
        inner.in_flight.extend(ids);
        inner.in_flight.sort_unstable();
    }

    /// Mark an item as complete and update HWM if appropriate.
    /// Called by worker threads after finishing processing.
    ///
    /// The HWM update logic:
    /// - If this is not the smallest ID, just remove it (items before it are still in-flight)
    /// - If this is the smallest ID:
    ///   - If it's the only ID, set HWM to this ID (all work complete up to here)
    ///   - Otherwise, set HWM to (next_smallest - 1), indicating all IDs before
    ///     the next in-flight item are complete
    pub fn complete_item(&self, id: i64) -> Result<(), FsPulseError> {
        let mut inner = self.inner.lock().unwrap();

        if let Some(pos) = inner.in_flight.iter().position(|&x| x == id) {
            let new_hwm = if pos == 0 {
                // This is the smallest item
                if inner.in_flight.len() == 1 {
                    // Only item - all work up to this ID is complete
                    Some(id)
                } else {
                    // More items remain - HWM is one less than the next in-flight item
                    Some(inner.in_flight[1] - 1)
                }
            } else {
                // Not the smallest - can't advance HWM yet
                None
            };

            inner.in_flight.remove(pos);

            // Update DB if we have a new HWM (still holding lock to ensure ordering)
            if let Some(hwm) = new_hwm {
                inner.state.high_water_mark = hwm;
                let task_state_json = inner.state.to_json()?;
                TaskEntry::set_task_state(self.task_id, &task_state_json)?;
            }
        } else {
            log::warn!(
                "AnalysisTracker: Item {} not found in in-flight vector for task_id {}",
                id,
                self.task_id
            );
        }

        Ok(())
    }

    /// Warn if there are still items in the in-flight vector.
    /// Should be called after analysis completes successfully to verify all items were processed.
    pub fn warn_if_not_empty(&self) {
        let inner = self.inner.lock().unwrap();
        if !inner.in_flight.is_empty() {
            log::warn!(
                "AnalysisTracker: {} items still in-flight after analysis completed for task_id {}. Item IDs: {:?}",
                inner.in_flight.len(),
                self.task_id,
                &inner.in_flight[..inner.in_flight.len().min(10)] // Show at most first 10 IDs
            );
        }
    }
}

// ============================================================================
// ScanTask - Task trait implementation for scan operations
// ============================================================================

/// A scan task that implements the Task trait
///
/// Constructed with parsed settings and state (no I/O). All scan-specific
/// database operations happen in `run()`.
pub struct ScanTask {
    task_id: i64,
    root_id: i64,
    root_path: String,
    schedule_id: Option<i64>,
    settings: ScanSettings,
    initial_state: ScanTaskState,
    // Populated during run()
    scan: Option<Scan>,
    root: Option<Root>,
}

impl ScanTask {
    /// Pure constructor — parses JSON settings and state, no database I/O.
    pub fn new(
        task_id: i64,
        root_id: i64,
        root_path: String,
        schedule_id: Option<i64>,
        settings_json: &str,
        task_state_json: Option<&str>,
    ) -> Result<Self, FsPulseError> {
        let settings = ScanSettings::from_json(settings_json)?;
        let initial_state = ScanTaskState::from_task_state(task_state_json)?;

        Ok(Self {
            task_id,
            root_id,
            root_path,
            schedule_id,
            settings,
            initial_state,
            scan: None,
            root: None,
        })
    }

    /// Initialize the scan — load Root, create or resume Scan record,
    /// and persist scan_id to task_state. Called at the start of run().
    fn initialize_scan(&mut self) -> Result<(), FsPulseError> {
        let conn = Database::get_connection()?;

        let root = Root::get_by_id(&conn, self.root_id)?
            .ok_or_else(|| FsPulseError::Error(format!("Root {} not found", self.root_id)))?;

        let scan = if let Some(scan_id) = self.initial_state.scan_id {
            // Resume case: mark restarted and load existing scan
            conn.execute(
                "UPDATE scans SET was_restarted = 1 WHERE scan_id = ?",
                rusqlite::params![scan_id],
            )
            .map_err(FsPulseError::DatabaseError)?;

            Scan::get_by_id_or_latest(&conn, Some(scan_id), None)?
                .ok_or_else(|| FsPulseError::Error(format!("Scan {} not found", scan_id)))?
        } else {
            // New scan: create scan record and persist scan_id to task_state
            let analysis_spec =
                AnalysisSpec::from_modes(self.settings.hash_mode, self.settings.validate_mode);

            Database::immediate_transaction(&conn, |c| {
                let scan = Scan::create(c, &root, self.schedule_id, &analysis_spec)?;

                // Write scan_id into task_state atomically with scan creation
                self.initial_state.scan_id = Some(scan.scan_id());
                let state_json = self.initial_state.to_json()?;
                c.execute(
                    "UPDATE tasks SET task_state = ? WHERE task_id = ? AND status = 1",
                    rusqlite::params![state_json, self.task_id],
                )
                .map_err(FsPulseError::DatabaseError)?;

                Ok(scan)
            })?
        };

        self.scan = Some(scan);
        self.root = Some(root);
        Ok(())
    }
}

impl Task for ScanTask {
    fn run(
        &mut self,
        progress: Arc<TaskProgress>,
        interrupt_token: Arc<AtomicBool>,
    ) -> Result<(), FsPulseError> {
        // Perform all scan-specific I/O: load Root, create/resume Scan
        self.initialize_scan()?;

        let scan = self.scan.as_mut().unwrap();
        let root = self.root.as_ref().unwrap();

        Scanner::do_scan_machine(
            scan,
            root,
            self.task_id,
            // Pass the initial_state as JSON string for the scanner's HWM logic
            Some(self.initial_state.to_json()?),
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
        Some(self.root_id)
    }

    fn action(&self) -> &str {
        "Scanning"
    }

    fn display_target(&self) -> String {
        self.root_path.clone()
    }

    fn on_stopped(&mut self) -> Result<(), FsPulseError> {
        if let Some(ref mut scan) = self.scan {
            let conn = Database::get_connection()?;
            scan.set_state_stopped(&conn)?;
        }
        Ok(())
    }

    fn on_error(&mut self, error_msg: &str) -> Result<(), FsPulseError> {
        if let Some(ref scan) = self.scan {
            let conn = Database::get_connection()?;
            Scan::stop_scan(&conn, scan, Some(error_msg))?;
        }
        Ok(())
    }

    fn is_exclusive(&self) -> bool {
        false
    }

    fn is_stoppable(&self) -> bool {
        true
    }

    fn is_pausable(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scan_settings_round_trip() {
        let settings = ScanSettings::new(HashMode::New, ValidateMode::All);
        let json = settings.to_json().unwrap();
        let restored = ScanSettings::from_json(&json).unwrap();
        assert_eq!(settings, restored);
    }

    #[test]
    fn test_scan_settings_json_format() {
        let settings = ScanSettings::new(HashMode::All, ValidateMode::None);
        let json = settings.to_json().unwrap();
        assert!(json.contains("hash_mode"));
        assert!(json.contains("validate_mode"));
    }

    #[test]
    fn test_scan_settings_deserialize_with_extra_fields() {
        let json = r#"{"hash_mode":"All","validate_mode":"None","unknown_field":123}"#;
        let settings = ScanSettings::from_json(json).unwrap();
        assert_eq!(settings.hash_mode, HashMode::All);
        assert_eq!(settings.validate_mode, ValidateMode::None);
    }
}
