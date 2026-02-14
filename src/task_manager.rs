use crate::database::Database;
use crate::error::FsPulseError;
use crate::task::{BroadcastMessage, TaskProgress, TaskStatus};
use crate::scans::{HashMode, ValidateMode};
use crate::schedules::{TaskEntry, Schedule};
use log::{error, info, Level};
use logging_timer::timer;
use once_cell::sync::Lazy;
use rusqlite::Connection;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::broadcast;
use tokio::task::JoinHandle;

/// Global singleton instance
static TASK_MANAGER: Lazy<Mutex<TaskManager>> = Lazy::new(|| {
    let (broadcaster, _) = broadcast::channel(1024);
    Mutex::new(TaskManager {
        current_task: None,
        broadcaster,
        db_is_compacting: false,
        is_shutting_down: false,
        is_paused: false,
        pause_until: None,
    })
});

/// Manages the currently active task with singleton semantics
pub struct TaskManager {
    current_task: Option<ActiveTaskInfo>,
    broadcaster: broadcast::Sender<BroadcastMessage>,
    db_is_compacting: bool,
    is_shutting_down: bool,
    is_paused: bool,
    pause_until: Option<i64>,
}

/// Information about the currently running task
/// Fields are extracted from the Task trait before the task is moved into spawn_blocking
struct ActiveTaskInfo {
    task_id: i64,
    interrupt_token: Arc<AtomicBool>,
    task_progress: Arc<TaskProgress>,
    task_handle: Option<JoinHandle<()>>,
    broadcast_handle: Option<JoinHandle<()>>,
}


impl TaskManager {
    /// Get the global singleton instance
    pub fn instance() -> &'static Mutex<TaskManager> {
        &TASK_MANAGER
    }

    /// Initialize pause state from database
    /// Should be called once at server startup
    pub fn init_pause_state(conn: &Connection) -> Result<(), FsPulseError> {
        let mut manager = Self::instance().lock().unwrap();

        // Read pause state from database
        let pause_until_opt = Database::immediate_transaction(conn, |conn| {
            Database::get_meta_value_locked(conn, "pause_until")
        })?;

        match pause_until_opt {
            Some(s) => {
                let timestamp = s.parse::<i64>().map_err(|_| {
                    FsPulseError::Error("Invalid pause_until value in database".into())
                })?;

                manager.is_paused = true;
                manager.pause_until = Some(timestamp);

                if timestamp == -1 {
                    info!("Initialized: system is paused indefinitely");
                } else {
                    info!(
                        "Initialized: system is paused until timestamp {}",
                        timestamp
                    );
                }
            }
            None => {
                manager.is_paused = false;
                manager.pause_until = None;
                info!("Initialized: system is not paused");
            }
        }

        Ok(())
    }

    /// Entry point 1: Manual scan from UI
    /// Creates queue entry and immediately tries to start it
    /// Returns Ok if scan was scheduled, Err if scheduling failed
    /// UI should check GET /api/scans/current to see if scan started
    pub fn schedule_manual_scan(
        conn: &Connection,
        root_id: i64,
        hash_mode: HashMode,
        validate_mode: ValidateMode,
    ) -> Result<(), FsPulseError> {
        // Hold mutex for entire operation to avoid race
        let mut manager = Self::instance().lock().unwrap();

        manager.check_shutting_down_locked()?;

        // Create queue entry atomically with root existence check
        Database::immediate_transaction(conn, |conn| {
            TaskEntry::create_manual(conn, root_id, hash_mode, validate_mode)
        })?;

        // Try to start immediately (while still holding mutex)
        // Whether it starts or not, scheduling succeeded
        manager.try_start_next_task_locked(conn)?;

        Ok(())
    }

    /// Create a new schedule
    /// Creates schedule and queue entry atomically
    /// Returns the created schedule with assigned schedule_id
    pub fn create_schedule(
        conn: &Connection,
        params: crate::schedules::CreateScheduleParams,
    ) -> Result<Schedule, FsPulseError> {
        // Hold mutex to prevent queue modifications during schedule creation
        let manager = Self::instance().lock().unwrap();

        manager.check_shutting_down_locked()?;

        // Create schedule and queue entry in transaction
        Database::immediate_transaction(conn, |conn| Schedule::create_and_queue(conn, params))
    }

    /// Update an existing schedule
    /// Updates schedule and recalculates run_at atomically
    pub fn update_schedule(conn: &Connection, schedule: &Schedule) -> Result<(), FsPulseError> {
        // Hold mutex to prevent queue modifications during schedule update
        let manager = Self::instance().lock().unwrap();

        manager.check_shutting_down_locked()?;

        // Update schedule in transaction
        Database::immediate_transaction(conn, |conn| schedule.update(conn))
    }

    /// Delete a schedule
    /// Deletes schedule and associated queue entry atomically
    /// Fails if a scan is currently running for this schedule
    pub fn delete_schedule(conn: &Connection, schedule_id: i64) -> Result<(), FsPulseError> {
        // Hold mutex to prevent queue modifications during schedule deletion
        let manager = Self::instance().lock().unwrap();

        manager.check_shutting_down_locked()?;

        // Delete schedule in transaction
        Database::immediate_transaction(conn, |conn| Schedule::delete_immediate(conn, schedule_id))
    }

    /// Set schedule enabled/disabled state
    /// When disabling: removes from queue (running scan completes normally)
    /// When enabling: recalculates run_at and creates a Pending task
    pub fn set_schedule_enabled(schedule_id: i64, enabled: bool) -> Result<(), FsPulseError> {
        // Hold mutex to prevent queue modifications during enable/disable
        let manager = Self::instance().lock().unwrap();

        manager.check_shutting_down_locked()?;

        // Note: Schedule::set_enabled gets its own connection internally
        Schedule::set_enabled(schedule_id, enabled)
    }

    /// Entry point 2: Background polling (every 5 seconds)
    pub fn poll_queue(conn: &Connection) -> Result<(), FsPulseError> {
        let _tmr = timer!(Level::Trace; "TaskManager::poll_queue");
        let mut manager = Self::instance().lock().unwrap();

        // Try to start next task - it's fine if nothing happens
        manager.try_start_next_task_locked(conn)?;

        Ok(())
    }

    pub async fn do_shutdown() {
        // Extract handles in a separate scope to ensure mutex is released
        let (task_handle, broadcast_handle) = {
            let mut manager = Self::instance().lock().unwrap();

            // setting this to true will prevent additional tasks from starting
            manager.is_shutting_down = true;

            // Signal interrupt and extract task handle
            if let Some(active) = &mut manager.current_task {
                active.interrupt_token.store(true, Ordering::Release);
                let task_handle = active.task_handle.take(); // Move the task handle out, leaving None
                let broadcast_handle = active.broadcast_handle.take(); // Move the broadcast handle out, leaving none
                (task_handle, broadcast_handle)
            } else {
                (None, None)
            }
            // MutexGuard drops here at end of block
        };

        // Wait for task to complete
        if let Some(handle) = task_handle {
            log::info!("Waiting for active task to complete...");
            // Await the task completion
            let _ = handle.await;
            log::info!("Active task completed");
        }

        if let Some(handle) = broadcast_handle {
            log::info!("Waiting for active broadcast thread to complete...");
            let _ = handle.await;
            log::info!("Broadcast thread completed");
        }
    }

    pub fn is_shutting_down() -> bool {
        let manager = Self::instance().lock().unwrap();
        manager.is_shutting_down
    }

    pub fn is_paused() -> bool {
        let manager = Self::instance().lock().unwrap();
        manager.is_paused
    }

    /// Shared logic: Find and start next task
    /// Called with mutex already held
    /// Uses the factory function to create a task via the Task trait
    fn try_start_next_task_locked(&mut self, conn: &Connection) -> Result<(), FsPulseError> {
        // Already running a task, compacting the database, or shutting down?
        if self.current_task.is_some() || self.db_is_compacting || self.is_shutting_down {
            return Ok(());
        }

        // If paused, check to see if the pause has expired
        if self.is_paused && !self.clear_pause_if_expired_locked(conn)? {
            return Ok(());
        }

        // Get next task from queue via generic factory
        let mut task = match TaskEntry::get_next_task(conn)? {
            Some(t) => t,
            None => return Ok(()), // No work available - not an error
        };

        // Extract metadata from the task before moving it into spawn_blocking
        let task_id = task.task_id();
        let task_type = task.task_type();
        let root_id = task.active_root_id();
        let action = task.action().to_string();
        let display_target = task.display_target();

        // Create task progress reporter
        let task_progress = TaskProgress::new(
            task_id,
            task_type,
            root_id,
            &action,
            &display_target,
        );
        let interrupt_token = Arc::new(AtomicBool::new(false));

        // Spawn per-task broadcast thread
        // This thread polls state every 250ms and broadcasts to all connected clients
        // It exits when the task reaches a terminal state
        let broadcast_handle = tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_millis(250));

            log::info!("Broadcast thread started for task_id {}", task_id);

            loop {
                interval.tick().await;

                // Broadcast current state. Only on_task_complete is allowed to send terminal messages
                Self::broadcast_current_state(false);

                // Check if task reached terminal state
                let is_terminal = {
                    let manager = Self::instance().lock().unwrap();
                    if let Some(active) = &manager.current_task {
                        if active.task_id == task_id {
                            matches!(
                                active.task_progress.get_status(),
                                TaskStatus::Completed | TaskStatus::Stopped | TaskStatus::Error
                            )
                        } else {
                            // Task was replaced, exit this broadcast thread
                            true
                        }
                    } else {
                        // Task was cleared, exit
                        true
                    }
                };

                if is_terminal {
                    log::info!(
                        "Broadcast thread exiting for task_id {} (terminal state reached)",
                        task_id
                    );
                    break;
                }
            }
        });

        // Clone references we need to keep after moving into spawn_blocking
        let interrupt_token_for_storage = Arc::clone(&interrupt_token);
        let task_progress_for_storage = Arc::clone(&task_progress);
        let task_progress_for_task = Arc::clone(&task_progress);

        // Spawn task execution
        let task_handle = tokio::task::spawn_blocking(move || {
            let task_result =
                task.run(task_progress_for_task, Arc::clone(&interrupt_token));

            // Handle result
            match task_result {
                Ok(()) => {
                    task_progress.set_status(TaskStatus::Completed);
                }
                Err(ref e) if matches!(e, FsPulseError::TaskInterrupted) => {
                    // Task was interrupted. If we're shutting down, we always treat this
                    // as a result of the shutdown. There's a chance the user was trying to
                    // stop the task, and if so, it will unexpectedly resume next time the
                    // process starts. We accept that.
                    if !TaskManager::is_shutting_down() {
                        // When interrupted and the app is pausing, we don't differentiate
                        // between an explicit stop and a pause - we just treat it like a pause.
                        if TaskManager::is_paused() {
                            info!("Task (task_id {}) was paused", task_id);
                            task_progress.set_status(TaskStatus::Completed);
                        } else {
                            info!("Task (task_id {}) was stopped, rolling back changes", task_id);
                            if let Err(stop_err) = task.on_stopped() {
                                error!("Failed to stop task (task_id {}): {}", task_id, stop_err);
                                task_progress
                                    .set_error(&format!("Failed to stop task: {}", stop_err));
                            } else {
                                task_progress.set_status(TaskStatus::Stopped);
                            }
                        }
                    }
                }
                Err(e) => {
                    error!("Task (task_id {}) failed: {}", task_id, e);
                    let error_msg = e.to_string();
                    if let Err(stop_err) = task.on_error(&error_msg) {
                        error!("Failed to stop task (task_id {}) after error: {}", task_id, stop_err);
                        task_progress.set_error(&format!(
                            "Task error: {}; Failed to stop: {}",
                            error_msg, stop_err
                        ));
                    } else {
                        task_progress.set_error(&error_msg);
                    }
                }
            }

            // Clean up queue and TaskManager
            let conn = match Database::get_connection() {
                Ok(conn) => conn,
                Err(e) => {
                    error!("Failed to get connection for task cleanup: {}", e);
                    return;
                }
            };

            if let Err(e) = TaskManager::on_task_complete(&conn, task_id) {
                error!("Failed to complete task (task_id {}): {}", task_id, e);
            }
        });

        // Store active task state with task handle
        self.current_task = Some(ActiveTaskInfo {
            task_id,
            interrupt_token: interrupt_token_for_storage,
            task_progress: task_progress_for_storage,
            task_handle: Some(task_handle),
            broadcast_handle: Some(broadcast_handle),
        });

        Ok(())
    }

    /// Called when task finishes (from background task)
    /// Cleans up queue and clears active task
    pub fn on_task_complete(conn: &Connection, task_id: i64) -> Result<(), FsPulseError> {
        let _tmr = timer!(Level::Trace; "TaskManager::on_task_complete mutex");
        let mut manager = Self::instance().lock().unwrap();

        // Complete the task (set terminal status and timestamp)
        // If we're shutting down or paused, leave the task as Running for resume on next start
        if !manager.is_shutting_down && !manager.is_paused {
            // Determine terminal status from task progress.
            // We expect the status to already be terminal (Completed/Stopped/Error) since
            // the spawn_blocking closure sets it before calling on_task_complete. The
            // non-terminal arms below handle unexpected states defensively.
            let terminal_status = match manager.current_task.as_ref() {
                Some(active) if active.task_id == task_id => {
                    match active.task_progress.get_status() {
                        TaskStatus::Completed => TaskStatus::Completed,
                        TaskStatus::Stopped => TaskStatus::Stopped,
                        TaskStatus::Error => TaskStatus::Error,
                        // Non-terminal states should not occur here, but if they do,
                        // treat as completed since the task has in fact finished.
                        TaskStatus::Pending
                        | TaskStatus::Running
                        | TaskStatus::Pausing
                        | TaskStatus::Stopping => {
                            log::warn!(
                                "Task {} finishing with unexpected non-terminal status {:?}, recording as Completed",
                                task_id,
                                active.task_progress.get_status()
                            );
                            TaskStatus::Completed
                        }
                    }
                }
                // Task ID mismatch or no active task — should not happen, but the task
                // did finish, so record it as completed.
                _ => {
                    log::warn!(
                        "on_task_complete called for task {} but it is not the active task, recording as Completed",
                        task_id
                    );
                    TaskStatus::Completed
                }
            };

            TaskEntry::complete_task(conn, task_id, terminal_status)?;
        }

        if let Some(active) = &manager.current_task {
            if active.task_id == task_id {
                // Notify the UI that the task is complete. It should be in a terminal state
                // This is the only place from which we send terminal messages
                // Terminal messages are a "best effort". We assume that one of two things is
                // true when the web UI is trying to show progress:
                // A) the web ui is connected and will receive this terminal message when it is sent
                // B) the web ui is connecting or is not connected, in which case they will receive
                // a "NoActiveScan" message when they do connect
                // If the web UI is in a state in which it thinks an active task is occurring, these
                // messages are enough to get it into a corrected state
                manager.broadcast_current_state_locked(true);
                manager.current_task = None;
                log::info!(
                    "Task (task_id {}) completed or exited, TaskManager now idle",
                    task_id
                );
            }
        }

        Ok(())
    }

    /// Request interrupt of the current task
    /// The caller must provide the task_id of the task they intend to stop.
    /// This guards against race conditions where the task the caller saw has
    /// already completed and a different task is now running.
    pub fn request_stop(task_id: i64) -> Result<(), String> {
        let manager = Self::instance().lock().unwrap();

        match &manager.current_task {
            Some(active) => {
                if active.task_id != task_id {
                    return Err(format!(
                        "Task {} is not running (current task is {})",
                        task_id, active.task_id
                    ));
                }

                if manager.is_shutting_down {
                    return Err("Shutting down".to_string());
                }

                let current_status = active.task_progress.get_status();

                match current_status {
                    TaskStatus::Running => {
                        active.task_progress.set_status(TaskStatus::Stopping);
                        active.interrupt_token.store(true, Ordering::Release);
                        Ok(())
                    }
                    TaskStatus::Pending => Err("Task has not started yet".to_string()),
                    TaskStatus::Stopping => Err("Task is already stopping".to_string()),
                    TaskStatus::Stopped => Err("Task has already been stopped".to_string()),
                    TaskStatus::Pausing => Err("Task is currently pausing".to_string()),
                    TaskStatus::Completed => Err("Task has already completed".to_string()),
                    TaskStatus::Error => Err("Task has already errored".to_string()),
                }
            }
            None => Err("No task is currently running".to_string()),
        }
    }

    /// Set pause state with duration
    /// duration_seconds: -1 for indefinite, positive value for timed pause
    /// If a task is currently running, it will be interrupted
    pub fn set_pause(conn: &Connection, duration_seconds: i64) -> Result<(), FsPulseError> {
        let mut manager = Self::instance().lock().unwrap();

        // Cannot pause during shutdown or database compaction
        if manager.is_shutting_down {
            return Err(FsPulseError::Error(
                "Cannot pause during shutdown".to_string(),
            ));
        }
        if manager.db_is_compacting {
            return Err(FsPulseError::Error(
                "Cannot pause during database compaction".to_string(),
            ));
        }

        // Calculate pause_until timestamp
        let pause_until = if duration_seconds == -1 {
            -1
        } else {
            chrono::Utc::now().timestamp() + duration_seconds
        };

        // Update database
        Database::immediate_transaction(conn, |conn| {
            Database::set_meta_value_locked(conn, "pause_until", &pause_until.to_string())
        })?;

        // always update the pause_until time
        manager.pause_until = Some(pause_until);

        // If already paused, there's no need to do any additional work
        if !manager.is_paused {
            manager.is_paused = true;

            // Interrupt current task if running
            if let Some(active) = &manager.current_task {
                let current_status = active.task_progress.get_status();
                if matches!(current_status, TaskStatus::Running) {
                    active.task_progress.set_status(TaskStatus::Pausing);
                    active.interrupt_token.store(true, Ordering::Release);
                }
            }
        }

        manager.broadcast_current_state_locked(false);

        info!(
            "Pause set until: {}",
            if pause_until == -1 {
                "indefinite".to_string()
            } else {
                pause_until.to_string()
            }
        );

        Ok(())
    }

    /// Clear pause state
    pub fn clear_pause(conn: &Connection) -> Result<(), FsPulseError> {
        let mut manager = Self::instance().lock().unwrap();

        if !manager.is_paused {
            info!("Clear pause requested when not paused");
            return Ok(());
        }

        // Cannot clear pause during shutdown or database compaction
        if manager.is_shutting_down {
            return Err(FsPulseError::Error(
                "Cannot clear pause during shutdown".to_string(),
            ));
        }
        if manager.db_is_compacting {
            return Err(FsPulseError::Error(
                "Cannot clear pause during database compaction".to_string(),
            ));
        }

        // if a pause was requested, and there is an active task, that task is going
        // to be in the process of unwinding. We need to let that complete before
        // allowing the user to unpause
        if manager.current_task.is_some() {
            info!("Clear pause requested while pausing an in-progress task");
            return Err(FsPulseError::Error(
                "Can't unpause when pausing an active task".into(),
            ));
        }

        // Update database
        Database::immediate_transaction(conn, |conn| {
            Database::delete_meta_locked(conn, "pause_until")
        })?;

        // Update local state
        manager.is_paused = false;
        manager.pause_until = None;

        manager.broadcast_current_state_locked(false);

        manager.try_start_next_task_locked(conn)?;

        info!("Pause cleared");

        Ok(())
    }

    /// Check if pause has expired and clear it if so
    /// Returns true if pause was cleared (became unpaused)
    /// This function expects to be called with the TaskManager mutex already held
    fn clear_pause_if_expired_locked(&mut self, conn: &Connection) -> Result<bool, FsPulseError> {
        // If pause_until is -1, still paused indefinitely
        if self.pause_until == Some(-1) {
            return Ok(false); // Still paused
        }

        // If we have a timestamp, check if it's passed
        if let Some(until) = self.pause_until {
            let now = chrono::Utc::now().timestamp();
            if until <= now {
                // Expired, clear it
                Database::immediate_transaction(conn, |conn| {
                    Database::delete_meta_locked(conn, "pause_until")
                })?;
                self.is_paused = false;
                self.pause_until = None;
                info!("Pause expired and cleared");
                return Ok(true); // Just became unpaused
            }
        }

        Ok(false) // Still paused
    }

    /// Subscribe to task state updates
    /// Returns a receiver that will receive broadcast messages for all task events
    pub fn subscribe() -> broadcast::Receiver<BroadcastMessage> {
        let manager = Self::instance().lock().unwrap();
        manager.broadcaster.subscribe()
    }

    /// Broadcast current state immediately
    /// Called on WebSocket connection and by broadcast thread
    /// Thread-safe: acquires mutex to read current state
    pub fn broadcast_current_state(allow_send_terminal: bool) {
        let _tmr = timer!(Level::Trace; "TaskManager::broadcast_current_state mutex");
        let manager = Self::instance().lock().unwrap();

        manager.broadcast_current_state_locked(allow_send_terminal);
    }

    /// Broadcast current state immediately
    /// Called on WebSocket connection and by broadcast thread
    /// Thread-safe: acquires mutex to read current state
    pub fn broadcast_current_state_locked(&self, allow_send_terminal: bool) {
        // if shutting down, don't broadcast
        if self.is_shutting_down {
            return;
        }

        if let Some(active) = &self.current_task {
            let task_state = active.task_progress.get_snapshot();
            let task_status = task_state.status;

            match task_status {
                TaskStatus::Completed | TaskStatus::Error | TaskStatus::Stopped => {
                    if allow_send_terminal {
                        let _ = self.broadcaster.send(BroadcastMessage::ActiveTask {
                            task: Box::new(task_state),
                        });
                    }
                    // Terminal state and sending not allowed → do nothing
                }
                _ => {
                    // Non-terminal state → always broadcast
                    let _ = self.broadcaster.send(BroadcastMessage::ActiveTask {
                        task: Box::new(task_state),
                    });
                }
            }
        } else if self.is_paused {
            // System is paused
            let pause_until = self.pause_until.unwrap_or(-1);
            let _ = self.broadcaster.send(BroadcastMessage::Paused { pause_until });
        } else {
            // System is idle
            let _ = self.broadcaster.send(BroadcastMessage::NoActiveTask);
        }
    }

    /// Get upcoming scans, synchronized with task manager state
    /// If paused, includes the in-progress scan as first entry
    pub fn get_upcoming_scans(
        limit: i64,
    ) -> Result<Vec<crate::schedules::UpcomingScan>, FsPulseError> {
        let manager = Self::instance().lock().unwrap();
        let scans_are_paused = manager.is_paused;

        // Note: TaskEntry::get_upcoming_scans gets its own connection internally
        // Call schedules method while holding mutex to synchronize with try_start_next_task
        TaskEntry::get_upcoming_scans(limit, scans_are_paused)
    }

    /// Compact the database
    /// Returns error if a task is currently running
    /// Blocks until compaction is complete
    pub fn compact_db() -> Result<(), String> {
        // Acquire mutex and check state
        let mut manager = Self::instance().lock().unwrap();
        if manager.current_task.is_some() {
            return Err("Cannot compact: task in progress".to_string());
        }
        manager.db_is_compacting = true;
        drop(manager); // Release mutex before long operation

        let result = Database::compact();

        // Set flag back (whether success or failure)
        let mut manager = Self::instance().lock().unwrap();
        manager.db_is_compacting = false;

        result.map_err(|e| format!("Compaction failed: {}", e))
    }

    /// Check if the process is shutting down, returning error if so
    /// Must be called with the task manager mutex
    fn check_shutting_down_locked(&self) -> Result<(), FsPulseError> {
        if self.is_shutting_down {
            Err(FsPulseError::ShuttingDown)
        } else {
            Ok(())
        }
    }
}
