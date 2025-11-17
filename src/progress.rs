use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};

// ============================================================================
// State Types (for WebSocket broadcasting)
// ============================================================================

/// Scan phase enum
/// Serializes as lowercase string: "scanning" | "sweeping" | "analyzing"
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ScanPhase {
    Scanning,
    Sweeping,
    Analyzing,
}

/// Complete state snapshot of scan progress
/// This is the single source of truth broadcast to web clients
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanProgressState {
    pub scan_id: Option<i64>,  // None when idle
    pub root_id: Option<i64>,  // None when idle
    pub root_path: String,     // Empty string when idle
    pub status: ScanStatus,
    pub current_phase: Option<ScanPhase>,
    pub completed_phases: Vec<String>, // For breadcrumb display
    pub overall_progress: Option<ProgressInfo>,
    pub scanning_progress: Option<ScanningProgress>, // For phase 1
    pub thread_states: Vec<ThreadState>,
}

impl ScanProgressState {
    /// Create state for an active scan with the given scan ID, root ID, and path
    pub fn new(scan_id: i64, root_id: i64, root_path: String) -> Self {
        Self {
            scan_id: Some(scan_id),
            root_id: Some(root_id),
            root_path,
            status: ScanStatus::Running,
            current_phase: None,
            completed_phases: Vec::new(),
            overall_progress: None,
            scanning_progress: None,
            thread_states: Vec::new(),
        }
    }

    /// Update thread state (creates entry if it doesn't exist)
    pub fn update_thread(&mut self, thread_index: usize, operation: ThreadOperation) {
        // Ensure we have enough thread state slots
        while self.thread_states.len() <= thread_index {
            self.thread_states.push(ThreadState {
                thread_index: self.thread_states.len(),
                operation: ThreadOperation::Idle,
            });
        }

        self.thread_states[thread_index] = ThreadState {
            thread_index,
            operation,
        };
    }

    /// Increment scanning progress counters
    pub fn increment_scanning(&mut self, is_directory: bool) {
        if self.scanning_progress.is_none() {
            self.scanning_progress = Some(ScanningProgress {
                files_scanned: 0,
                directories_scanned: 0,
            });
        }

        if let Some(ref mut progress) = self.scanning_progress {
            if is_directory {
                progress.directories_scanned += 1;
            } else {
                progress.files_scanned += 1;
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "lowercase")]
pub enum ScanStatus {
    Running,    // Scan is actively running
    Stopping, // Scan stop requested
    Stopped,    // Scan was stopped
    Pausing,
    Completed,  // Scan completed successfully
    Error { message: String }, // Scan failed with error
}

/// Broadcast message type for WebSocket protocol
/// Represents the current system state: active scan, paused, or idle
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum BroadcastMessage {
    ActiveScan { scan: Box<ScanProgressState> },
    NoActiveScan,
    Paused { pause_until: i64 },  // -1 for indefinite pause, or timestamp when pause expires
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProgressInfo {
    pub completed: u64,
    pub total: u64,
    pub percentage: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanningProgress {
    pub files_scanned: u64,
    pub directories_scanned: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThreadState {
    pub thread_index: usize,
    pub operation: ThreadOperation,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum ThreadOperation {
    Idle,
    Hashing { file: String },
    Validating { file: String },
}

// ============================================================================
// Progress Reporter
// ============================================================================

/// Progress reporter that maintains scan state snapshots
///
/// Instead of emitting discrete events, this reporter maintains a complete
/// state snapshot that represents the current progress. A background task
/// periodically broadcasts this state to all connected WebSocket clients.
///
/// This approach provides:
/// - Single source of truth for progress state
/// - Accurate thread state even after reconnection
/// - No event flooding or buffer management complexity
/// - Simplified frontend (just render the state)
pub struct ProgressReporter {
    state: Arc<Mutex<ScanProgressState>>,
}

impl ProgressReporter {
    /// Create a progress reporter that maintains scan state
    ///
    /// ScanManager's persistent broadcast task reads this state and broadcasts it to WebSocket clients
    pub fn new(scan_id: i64, root_id: i64, root_path: String) -> Self {
        let state = Arc::new(Mutex::new(ScanProgressState::new(scan_id, root_id, root_path)));

        Self { state }
    }

    // ========================================================================
    // Phase Transitions
    // ========================================================================

    /// Start scanning phase (phase 1)
    /// Automatically completes previous phase if any
    pub fn start_scanning_phase(&self) {
        let mut state = self.state.lock().unwrap();

        // Complete previous phase if exists
        self.complete_current_phase(&mut state);

        state.current_phase = Some(ScanPhase::Scanning);
    }

    /// Start sweeping phase (phase 2)
    /// Automatically completes previous phase with breadcrumb
    pub fn start_sweeping_phase(&self) {
        let mut state = self.state.lock().unwrap();

        // Complete previous phase if exists
        self.complete_current_phase(&mut state);

        state.current_phase = Some(ScanPhase::Sweeping);
    }

    /// Start analyzing phase (phase 3)
    /// Automatically completes previous phase with breadcrumb
    pub fn start_analyzing_phase(&self, total_items: u64, initial_completed: u64) {
        let mut state = self.state.lock().unwrap();

        // Complete previous phase if exists
        self.complete_current_phase(&mut state);

        state.current_phase = Some(ScanPhase::Analyzing);
        state.overall_progress = Some(ProgressInfo {
            completed: initial_completed,
            total: total_items,
            percentage: if total_items > 0 {
                (initial_completed as f64 / total_items as f64) * 100.0
            } else {
                0.0
            },
        });
    }

    /// Internal helper to complete current phase and generate breadcrumb
    fn complete_current_phase(&self, state: &mut ScanProgressState) {
        if let Some(current) = &state.current_phase {
            let breadcrumb = match current {
                ScanPhase::Scanning => {
                    if let Some(ref scan_progress) = state.scanning_progress {
                        format!(
                            "Scanned {} files in {} directories",
                            scan_progress.files_scanned, scan_progress.directories_scanned
                        )
                    } else {
                        "Scanning complete".to_string()
                    }
                }
                ScanPhase::Sweeping => "Tombstoned deleted items".to_string(),
                ScanPhase::Analyzing => {
                    if let Some(ref progress) = state.overall_progress {
                        format!("Analyzed {} files", progress.completed)
                    } else {
                        "Analysis complete".to_string()
                    }
                }
            };
            state.completed_phases.push(breadcrumb);
        }
    }

    // ========================================================================
    // Scanning Phase (Phase 1)
    // ========================================================================

    /// Increment files scanned counter
    pub fn increment_files_scanned(&self) {
        let mut state = self.state.lock().unwrap();
        state.increment_scanning(false);
    }

    /// Increment directories scanned counter
    pub fn increment_directories_scanned(&self) {
        let mut state = self.state.lock().unwrap();
        state.increment_scanning(true);
    }

    // ========================================================================
    // Analysis Phase (Phase 3)
    // ========================================================================

    /// Increment analysis completed counter
    pub fn increment_analysis_completed(&self) {
        let mut state = self.state.lock().unwrap();
        if let Some(ref mut progress) = state.overall_progress {
            progress.completed += 1;
            progress.percentage = if progress.total > 0 {
                (progress.completed as f64 / progress.total as f64) * 100.0
            } else {
                0.0
            };
        }
    }

    /// Set thread to idle state
    pub fn set_thread_idle(&self, thread_index: usize) {
        let mut state = self.state.lock().unwrap();
        state.update_thread(thread_index, ThreadOperation::Idle);
    }

    /// Set thread to hashing state
    pub fn set_thread_hashing(&self, thread_index: usize, file: String) {
        let mut state = self.state.lock().unwrap();
        state.update_thread(thread_index, ThreadOperation::Hashing { file });
    }

    /// Set thread to validating state
    pub fn set_thread_validating(&self, thread_index: usize, file: String) {
        let mut state = self.state.lock().unwrap();
        state.update_thread(thread_index, ThreadOperation::Validating { file });
    }

    // ========================================================================
    // Terminal States
    // ========================================================================

    /// Mark scan as completed
    pub fn mark_completed(&self) {
        let mut state = self.state.lock().unwrap();
        state.status = ScanStatus::Completed;
    }

    /// Mark scan as error
    pub fn mark_error(&self, message: String) {
        let mut state = self.state.lock().unwrap();
        state.status = ScanStatus::Error { message };
    }

    /// Mark scan as interrupting (user requested stop, scanner hasn't detected yet)
    pub fn mark_stopping(&self) {
        let mut state = self.state.lock().unwrap();
        state.status = ScanStatus::Stopping;
    }

    /// Mark scan as stopped (scanner detected interrupt and rolled back)
    pub fn mark_stopped(&self) {
        let mut state = self.state.lock().unwrap();
        state.status = ScanStatus::Stopped;
    }

    /// Mark scan as pausing
    pub fn mark_pausing(&self) {
        let mut state = self.state.lock().unwrap();
        state.status = ScanStatus::Pausing;        
    }

    // ========================================================================
    // State Access
    // ========================================================================

    /// Get current status (for checking before state transitions)
    pub fn get_status(&self) -> ScanStatus {
        let state = self.state.lock().unwrap();
        state.status.clone()
    }

    /// Get a clone of the current state for broadcasting
    pub fn get_current_state(&self) -> ScanProgressState {
        let state = self.state.lock().unwrap();
        state.clone()
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_state() {
        let state = ScanProgressState::new(1, 100, "/test".to_string());
        assert_eq!(state.scan_id, Some(1));
        assert_eq!(state.root_id, Some(100));
        assert_eq!(state.root_path, "/test");
        assert!(matches!(state.status, ScanStatus::Running));
        assert!(state.current_phase.is_none());
        assert_eq!(state.completed_phases.len(), 0);
        assert_eq!(state.thread_states.len(), 0);
    }

    #[test]
    fn test_update_thread_creates_slots() {
        let mut state = ScanProgressState::new(1, 100, "/test".to_string());
        state.update_thread(2, ThreadOperation::Hashing {
            file: "test.txt".to_string()
        });

        assert_eq!(state.thread_states.len(), 3);
        assert!(matches!(state.thread_states[0].operation, ThreadOperation::Idle));
        assert!(matches!(state.thread_states[1].operation, ThreadOperation::Idle));
        assert!(matches!(state.thread_states[2].operation, ThreadOperation::Hashing { .. }));
    }

    #[test]
    fn test_update_thread_replaces_existing() {
        let mut state = ScanProgressState::new(1, 100, "/test".to_string());
        state.update_thread(0, ThreadOperation::Hashing {
            file: "file1.txt".to_string()
        });
        state.update_thread(0, ThreadOperation::Validating {
            file: "file2.txt".to_string()
        });

        assert_eq!(state.thread_states.len(), 1);
        assert!(matches!(state.thread_states[0].operation, ThreadOperation::Validating { .. }));
    }

    #[test]
    fn test_increment_scanning() {
        let mut state = ScanProgressState::new(1, 100, "/test".to_string());

        state.increment_scanning(false); // file
        state.increment_scanning(true);  // directory
        state.increment_scanning(false); // file

        let progress = state.scanning_progress.unwrap();
        assert_eq!(progress.files_scanned, 2);
        assert_eq!(progress.directories_scanned, 1);
    }

    #[test]
    fn test_phase_transitions() {
        let reporter = ProgressReporter::new(1, 100, "/test".to_string());

        // Start scanning phase
        reporter.start_scanning_phase();
        {
            let state = reporter.state.lock().unwrap();
            assert!(matches!(state.current_phase, Some(ScanPhase::Scanning)));
            assert_eq!(state.completed_phases.len(), 0);
        }

        // Increment scanning counts
        reporter.increment_files_scanned();
        reporter.increment_directories_scanned();

        // Start sweeping phase - should complete scanning with breadcrumb
        reporter.start_sweeping_phase();
        {
            let state = reporter.state.lock().unwrap();
            assert!(matches!(state.current_phase, Some(ScanPhase::Sweeping)));
            assert_eq!(state.completed_phases.len(), 1);
            assert_eq!(state.completed_phases[0], "Scanned 1 files in 1 directories");
        }

        // Start analyzing phase
        reporter.start_analyzing_phase(100, 0);
        {
            let state = reporter.state.lock().unwrap();
            assert!(matches!(state.current_phase, Some(ScanPhase::Analyzing)));
            assert_eq!(state.completed_phases.len(), 2);
            assert_eq!(state.completed_phases[1], "Tombstoned deleted items");
            assert!(state.overall_progress.is_some());
            let progress = state.overall_progress.as_ref().unwrap();
            assert_eq!(progress.total, 100);
            assert_eq!(progress.completed, 0);
        }
    }

    #[test]
    fn test_thread_operations() {
        let reporter = ProgressReporter::new(1, 100, "/test".to_string());

        reporter.set_thread_hashing(0, "file1.txt".to_string());
        reporter.set_thread_validating(1, "file2.txt".to_string());
        reporter.set_thread_idle(2);

        let state = reporter.state.lock().unwrap();
        assert_eq!(state.thread_states.len(), 3);
        assert!(matches!(state.thread_states[0].operation, ThreadOperation::Hashing { .. }));
        assert!(matches!(state.thread_states[1].operation, ThreadOperation::Validating { .. }));
        assert!(matches!(state.thread_states[2].operation, ThreadOperation::Idle));
    }

    #[test]
    fn test_analysis_progress() {
        let reporter = ProgressReporter::new(1, 100, "/test".to_string());
        reporter.start_analyzing_phase(10, 0);

        reporter.increment_analysis_completed();
        reporter.increment_analysis_completed();

        let state = reporter.state.lock().unwrap();
        let progress = state.overall_progress.as_ref().unwrap();
        assert_eq!(progress.completed, 2);
        assert_eq!(progress.total, 10);
        assert_eq!(progress.percentage, 20.0);
    }

    #[test]
    fn test_phase_serialization() {
        let reporter = ProgressReporter::new(1, 100, "/test".to_string());
        reporter.start_scanning_phase();

        let state = reporter.get_current_state();
        let json = serde_json::to_string(&state).unwrap();

        // Verify that current_phase serializes as just "scanning" (not wrapped in object)
        assert!(json.contains(r#""current_phase":"scanning""#));
    }
}
