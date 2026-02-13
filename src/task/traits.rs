use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use crate::error::FsPulseError;

use super::progress::TaskProgress;
use super::task_type::TaskType;

/// Trait for long-running, pausable, stoppable tasks
///
/// Methods on this trait were discovered bottom-up from what TaskManager
/// needs to interact with tasks generically:
///
/// - `run`: Execute the task (TaskManager calls this in spawn_blocking)
/// - `task_type`, `task_id`: Identity for progress tracking and task cleanup
/// - `active_root_id`, `action`, `display_target`: Metadata for TaskProgress creation
/// - `on_stopped`, `on_error`: Cleanup handlers called by TaskManager on interrupt/failure
///
/// The trait is object-safe so TaskManager can work with Box<dyn Task>
pub trait Task: Send {
    /// Execute the task
    fn run(
        &mut self,
        progress: Arc<TaskProgress>,
        interrupt_token: Arc<AtomicBool>,
    ) -> Result<(), FsPulseError>;

    /// The task type
    fn task_type(&self) -> TaskType;

    /// The task_id from the tasks table
    fn task_id(&self) -> i64;

    /// The root_id associated with this task, if any
    fn active_root_id(&self) -> Option<i64>;

    /// Human-readable action name for progress display (e.g., "Scanning")
    fn action(&self) -> &str;

    /// Human-readable target for progress display (e.g., root path for scans)
    fn display_target(&self) -> String;

    /// Handle task stopped by user (rollback changes)
    /// Called by TaskManager when interrupt is detected and system is not paused/shutting down
    fn on_stopped(&mut self) -> Result<(), FsPulseError>;

    /// Handle task error (stop with error message)
    /// Called by TaskManager when task.run() returns an error that isn't an interrupt
    fn on_error(&mut self, error_msg: &str) -> Result<(), FsPulseError>;
}
