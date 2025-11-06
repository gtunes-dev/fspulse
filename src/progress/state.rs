use serde::{Deserialize, Serialize};

/// Complete state snapshot of scan progress
/// This is the single source of truth broadcast to web clients
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanProgressState {
    pub scan_id: Option<i64>,  // None when idle
    pub root_id: Option<i64>,  // None when idle
    pub root_path: String,     // Empty string when idle
    pub status: ScanStatus,
    pub current_phase: Option<PhaseInfo>,
    pub completed_phases: Vec<String>, // For breadcrumb display
    pub overall_progress: Option<ProgressInfo>,
    pub scanning_progress: Option<ScanningProgress>, // For phase 1
    pub thread_states: Vec<ThreadState>,
    pub messages: Vec<String>, // Recent log messages (keep last 20)
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
            messages: Vec::new(),
        }
    }

    /// Add a log message (keeps only last 20)
    pub fn add_message(&mut self, message: String) {
        self.messages.push(message);
        if self.messages.len() > 20 {
            self.messages.remove(0);
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
    Cancelling, // Scan cancellation requested
    Stopped,    // Scan was stopped
    Completed,  // Scan completed successfully
    Error { message: String }, // Scan failed with error
}

/// Broadcast message type for WebSocket protocol
/// Represents either an active scan or no active scan
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum BroadcastMessage {
    ActiveScan { scan: ScanProgressState },
    NoActiveScan,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PhaseInfo {
    pub name: String, // "scanning", "sweeping", "analyzing"
    pub stage_index: u32,
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
    fn test_add_message_limits_to_20() {
        let mut state = ScanProgressState::new(1, 100, "/test".to_string());
        for i in 0..30 {
            state.add_message(format!("Message {}", i));
        }
        assert_eq!(state.messages.len(), 20);
        assert_eq!(state.messages[0], "Message 10");
        assert_eq!(state.messages[19], "Message 29");
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
}
