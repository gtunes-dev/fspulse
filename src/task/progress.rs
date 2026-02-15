use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};

use super::task_status::TaskStatus;
use super::task_type::TaskType;

// ============================================================================
// Task Protocol Types (for WebSocket broadcasting)
// These types define the wire format sent to frontend clients
// ============================================================================

/// Progress bar state for the protocol
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskProgressBar {
    /// Percentage complete (0-100), or None for indeterminate progress
    pub percentage: Option<f64>,
    /// Pre-formatted message to display below the progress bar
    pub message: Option<String>,
}

/// Thread state for the protocol
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskThreadState {
    /// Badge text: "Hashing", "Validating", "Idle", etc.
    pub status: String,
    /// Badge style variant: "info", "info-alternate", "success", "secondary"
    pub status_style: String,
    /// Optional detail like file path
    pub detail: Option<String>,
}

impl TaskThreadState {
    fn idle() -> Self {
        Self {
            status: "Idle".to_string(),
            status_style: "secondary".to_string(),
            detail: None,
        }
    }
}

/// Complete task progress state for the protocol
/// This is what gets broadcast to web clients
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskProgressState {
    pub task_id: i64,
    pub task_type: TaskType,
    pub active_root_id: Option<i64>,
    pub is_exclusive: bool,
    pub action: String,
    pub target: String,
    pub status: TaskStatus,
    pub error_message: Option<String>,
    pub breadcrumbs: Option<Vec<String>>,
    pub phase: Option<String>,
    pub progress_bar: Option<TaskProgressBar>,
    pub thread_states: Option<Vec<TaskThreadState>>,
}

/// Broadcast message type for WebSocket protocol
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum BroadcastMessage {
    ActiveTask { task: Box<TaskProgressState> },
    NoActiveTask,
    Paused { pause_until: i64 },
}

// ============================================================================
// Task Progress State Management
// ============================================================================

/// Internal state for TaskProgress
struct TaskProgressInternalState {
    // Identity (immutable after construction)
    task_id: i64,
    task_type: TaskType,
    active_root_id: Option<i64>,
    is_exclusive: bool,
    action: String,
    target: String,

    // Status
    status: TaskStatus,
    error_message: Option<String>,

    // Phase/Breadcrumbs
    phase: Option<String>,
    breadcrumbs: Vec<String>,

    // Progress bar
    progress_bar: Option<TaskProgressBar>,

    // Progress tracking (for incremental updates during analysis)
    progress_completed: u64,
    progress_total: u64,
    progress_unit: Option<String>,

    // Thread states
    thread_states: Vec<TaskThreadState>,
}

impl TaskProgressInternalState {
    /// Update the progress bar based on current counter state
    fn update_counter_progress_bar(&mut self) {
        let completed = self.progress_completed;
        let total = self.progress_total;
        let percentage = if total > 0 {
            (completed as f64 / total as f64) * 100.0
        } else {
            0.0
        };
        let message = match &self.progress_unit {
            Some(unit) => format!("{} / {} {}", completed, total, unit),
            None => format!("{} / {}", completed, total),
        };
        self.progress_bar = Some(TaskProgressBar {
            percentage: Some(percentage),
            message: Some(message),
        });
    }
}

/// Progress reporter for tasks
///
/// Tasks use this struct to report their progress. The state is periodically
/// read by TaskManager and broadcast to connected WebSocket clients.
pub struct TaskProgress {
    state: Mutex<TaskProgressInternalState>,
}

impl TaskProgress {
    /// Create a new task progress reporter
    pub fn new(
        task_id: i64,
        task_type: TaskType,
        active_root_id: Option<i64>,
        is_exclusive: bool,
        action: &str,
        target: &str,
    ) -> Arc<Self> {
        Arc::new(Self {
            state: Mutex::new(TaskProgressInternalState {
                task_id,
                task_type,
                active_root_id,
                is_exclusive,
                action: action.to_string(),
                target: target.to_string(),
                status: TaskStatus::Running,
                error_message: None,
                phase: None,
                breadcrumbs: Vec::new(),
                progress_bar: None,
                progress_completed: 0,
                progress_total: 0,
                progress_unit: None,
                thread_states: Vec::new(),
            }),
        })
    }

    // ========================================================================
    // Phase & Breadcrumbs
    // ========================================================================

    /// Set the current phase (pre-formatted by caller)
    pub fn set_phase(&self, phase: &str) {
        self.state.lock().unwrap().phase = Some(phase.to_string());
    }

    /// Add a breadcrumb for a completed phase
    pub fn add_breadcrumb(&self, text: &str) {
        self.state.lock().unwrap().breadcrumbs.push(text.to_string());
    }

    // ========================================================================
    // Progress Bar
    // ========================================================================

    /// Set determinate progress (percentage-based)
    #[allow(dead_code)]
    pub fn set_progress(&self, percentage: f64, message: &str) {
        self.state.lock().unwrap().progress_bar = Some(TaskProgressBar {
            percentage: Some(percentage),
            message: Some(message.to_string()),
        });
    }

    /// Set indeterminate progress (no percentage, just message)
    pub fn set_indeterminate_progress(&self, message: &str) {
        self.state.lock().unwrap().progress_bar = Some(TaskProgressBar {
            percentage: None,
            message: Some(message.to_string()),
        });
    }

    /// Clear the progress bar
    #[allow(dead_code)]
    pub fn clear_progress(&self) {
        self.state.lock().unwrap().progress_bar = None;
    }

    /// Set up counter-based progress tracking
    ///
    /// The optional `unit` parameter (e.g., "files", "items") is used in progress messages.
    /// If not provided, messages will show just "X / Y".
    pub fn set_progress_total(&self, total: u64, initial_completed: u64, unit: Option<&str>) {
        let mut state = self.state.lock().unwrap();
        state.progress_total = total;
        state.progress_completed = initial_completed;
        state.progress_unit = unit.map(|s| s.to_string());
        state.update_counter_progress_bar();
    }

    /// Increment the progress counter
    pub fn increment_progress(&self) {
        let mut state = self.state.lock().unwrap();
        state.progress_completed += 1;
        state.update_counter_progress_bar();
    }

    // ========================================================================
    // Thread States
    // ========================================================================

    /// Initialize thread states for parallel operations
    pub fn set_thread_count(&self, count: usize) {
        let mut state = self.state.lock().unwrap();
        state.thread_states = vec![TaskThreadState::idle(); count];
    }

    /// Set a thread to idle state
    pub fn set_thread_idle(&self, index: usize) {
        let mut state = self.state.lock().unwrap();
        if index < state.thread_states.len() {
            state.thread_states[index] = TaskThreadState::idle();
        }
    }

    /// Set a thread to a specific state
    pub fn set_thread_state(&self, index: usize, status: &str, style: &str, detail: Option<&str>) {
        let mut state = self.state.lock().unwrap();
        if index < state.thread_states.len() {
            state.thread_states[index] = TaskThreadState {
                status: status.to_string(),
                status_style: style.to_string(),
                detail: detail.map(|s| s.to_string()),
            };
        }
    }

    /// Clear all thread states
    pub fn clear_thread_states(&self) {
        self.state.lock().unwrap().thread_states.clear();
    }

    // ========================================================================
    // Status
    // ========================================================================

    /// Set the task status
    pub fn set_status(&self, status: TaskStatus) {
        let mut state = self.state.lock().unwrap();
        state.status = status;
        if status != TaskStatus::Error {
            state.error_message = None;
        }
    }

    /// Set error status with message
    pub fn set_error(&self, message: &str) {
        let mut state = self.state.lock().unwrap();
        state.status = TaskStatus::Error;
        state.error_message = Some(message.to_string());
    }

    /// Get current status (for checking terminal states)
    pub fn get_status(&self) -> TaskStatus {
        self.state.lock().unwrap().status
    }

    // ========================================================================
    // Snapshot
    // ========================================================================

    /// Get a snapshot of the current progress state for broadcasting
    pub fn get_snapshot(&self) -> TaskProgressState {
        let state = self.state.lock().unwrap();

        TaskProgressState {
            task_id: state.task_id,
            task_type: state.task_type,
            active_root_id: state.active_root_id,
            is_exclusive: state.is_exclusive,
            action: state.action.clone(),
            target: state.target.clone(),
            status: state.status,
            error_message: state.error_message.clone(),
            breadcrumbs: if state.breadcrumbs.is_empty() {
                None
            } else {
                Some(state.breadcrumbs.clone())
            },
            phase: state.phase.clone(),
            progress_bar: state.progress_bar.clone(),
            thread_states: if state.thread_states.is_empty() {
                None
            } else {
                Some(state.thread_states.clone())
            },
        }
    }
}
