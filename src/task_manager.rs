use crate::database::Database;
use crate::error::FsPulseError;
use crate::task::{BroadcastMessage, TaskProgress, TaskStatus, TaskType};
use crate::roots::Root;
use crate::scanner::Scanner;
use crate::scans::{HashMode, Scan, ValidateMode};
use crate::schedules::{QueueEntry, Schedule};
use log::{error, info, Level};
use logging_timer::timer;
use once_cell::sync::Lazy;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::broadcast;
use tokio::task::JoinHandle;

/// Global singleton instance
static TASK_MANAGER: Lazy<Mutex<TaskManager>> = Lazy::new(|| {
    let (broadcaster, _) = broadcast::channel(1024);
    Mutex::new(TaskManager {
        current_scan: None,
        broadcaster,
        db_is_compacting: false,
        is_shutting_down: false,
        is_paused: false,
        pause_until: None,
    })
});

/// Manages the currently active scan with singleton semantics
pub struct TaskManager {
    current_scan: Option<ActiveTaskInfo>,
    broadcaster: broadcast::Sender<BroadcastMessage>,
    db_is_compacting: bool,
    is_shutting_down: bool,
    is_paused: bool,
    pause_until: Option<i64>,
}

/// Information about the currently running scan
struct ActiveTaskInfo {
    scan_id: i64,
    #[allow(dead_code)]
    root_id: i64,
    #[allow(dead_code)]
    root_path: String,
    interrupt_token: Arc<AtomicBool>,
    task_progress: Arc<TaskProgress>,
    task_handle: Option<JoinHandle<()>>,
    broadcast_handle: Option<JoinHandle<()>>,
}

/// Information about current scan for status queries
#[derive(Debug, Serialize, Deserialize)]
pub struct CurrentScanInfo {
    pub scan_id: i64,
    pub root_id: i64,
    pub root_path: String,
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
            QueueEntry::create_manual(conn, root_id, hash_mode, validate_mode)
        })?;

        // Try to start immediately (while still holding mutex)
        // Whether it starts or not, scheduling succeeded
        manager.try_start_next_scan_locked(conn)?;

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
    /// Updates schedule and recalculates next_scan_time atomically
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
    /// When enabling: recalculates next_scan_time and adds back to queue
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

        // Try to start next scan - it's fine if nothing happens
        manager.try_start_next_scan_locked(conn)?;

        Ok(())
    }

    pub async fn do_shutdown() {
        // Extract handles in a separate scope to ensure mutex is released
        let (task_handle, broadcast_handle) = {
            let mut manager = Self::instance().lock().unwrap();

            // setting this to true will prevent additional scans from starting
            manager.is_shutting_down = true;

            // Signal interrupt and extract task handle
            if let Some(active) = &mut manager.current_scan {
                active.interrupt_token.store(true, Ordering::Release);
                let task_handle = active.task_handle.take(); // Move the task handle out, leaving None
                let broadcast_handle = active.broadcast_handle.take(); // Move the broadcast handle out, leaving none
                (task_handle, broadcast_handle)
            } else {
                (None, None)
            }
            // MutexGuard drops here at end of block
        };

        // Wait for scan task to complete
        if let Some(handle) = task_handle {
            log::info!("Waiting for active scan to complete...");
            // Await the task completion
            let _ = handle.await;
            log::info!("Active scan completed");
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

    /// Shared logic: Find and start next scan
    /// Called with mutex already held
    /// Updates self.current_scan if scan started
    fn try_start_next_scan_locked(&mut self, conn: &Connection) -> Result<(), FsPulseError> {
        // Already running a scan, compacting the database, or shutting down?
        if self.current_scan.is_some() || self.db_is_compacting || self.is_shutting_down {
            return Ok(());
        }

        // If paused, check to see if the pause has expired
        if self.is_paused && !self.clear_pause_if_expired_locked(conn)? {
            return Ok(());
        }

        // Get next scan from queue (creates scan, updates queue)
        let mut scan = match QueueEntry::get_next_scan(conn)? {
            Some(s) => s,
            None => return Ok(()), // No work available - not an error
        };

        let scan_id = scan.scan_id();
        let root_id = scan.root_id();

        // Get root for progress reporting
        let root = Root::get_by_id(conn, root_id)?
            .ok_or_else(|| FsPulseError::Error("Root not found".to_string()))?;
        let root_path = root.root_path().to_string();

        // Create task progress reporter
        let task_progress = TaskProgress::new(
            scan_id,
            TaskType::Scan,
            Some(root_id),
            "Scanning",
            &root_path,
        );
        let interrupt_token = Arc::new(AtomicBool::new(false));

        // Spawn per-scan broadcast thread
        // This thread polls state every 250ms and broadcasts to all connected clients
        // It exits when the scan reaches a terminal state
        let broadcast_handle = tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_millis(250));

            log::info!("Broadcast thread started for scan {}", scan_id);

            loop {
                interval.tick().await;

                // Broadcast current state. Only on_scan_complete is allowed to send terminal messages
                Self::broadcast_current_state(false);

                // Check if scan reached terminal state
                let is_terminal = {
                    let manager = Self::instance().lock().unwrap();
                    if let Some(active) = &manager.current_scan {
                        if active.scan_id == scan_id {
                            matches!(
                                active.task_progress.get_status(),
                                TaskStatus::Completed | TaskStatus::Stopped | TaskStatus::Error
                            )
                        } else {
                            // Scan was replaced, exit this broadcast thread
                            true
                        }
                    } else {
                        // Scan was cleared, exit
                        true
                    }
                };

                if is_terminal {
                    log::info!(
                        "Broadcast thread exiting for scan {} (terminal state reached)",
                        scan_id
                    );
                    break;
                }
            }
        });

        // Clone references we need to keep after moving into spawn_blocking
        let interrupt_token_for_storage = Arc::clone(&interrupt_token);
        let task_progress_for_storage = Arc::clone(&task_progress);
        let task_progress_for_scan = Arc::clone(&task_progress);

        // Spawn scan task
        let task_handle = tokio::task::spawn_blocking(move || {
            let conn = match Database::get_connection() {
                Ok(conn) => conn,
                Err(e) => {
                    error!("Failed to get database connection in scan task: {}", e);
                    task_progress.set_error(&format!("Failed to get database connection: {}", e));
                    return;
                }
            };

            let root = match Root::get_by_id(&conn, root_id) {
                Ok(Some(root)) => root,
                Ok(None) => {
                    error!("Root {} not found", root_id);
                    task_progress.set_error(&format!("Root {} not found", root_id));
                    return;
                }
                Err(e) => {
                    error!("Failed to get root {}: {}", root_id, e);
                    task_progress.set_error(&format!("Failed to get root: {}", e));
                    return;
                }
            };

            let scan_result =
                Scanner::do_scan_machine(&mut scan, &root, task_progress_for_scan, interrupt_token);

            // Handle result
            match scan_result {
                Ok(()) => {
                    task_progress.set_status(TaskStatus::Completed);
                }
                Err(ref e) if matches!(e, FsPulseError::ScanInterrupted) => {
                    // If the scan is shutting down, we always treat ScanInterrupted as if it's a result
                    // of the shutdown. There's a chance that the user was trying to stop the scan
                    // and if that's the case, the scan will unexpectedly resume the next time the
                    // process starts. We accept that.
                    if !TaskManager::is_shutting_down() {
                        // For the purpose of reporting to the web ui, when a scan is interrrupted
                        // and the app is pausing, we don't bother to differentiate between an
                        // explicit stop and a pause - we just treat it like a pause.
                        if TaskManager::is_paused() {
                            info!("Scan {} was paused", scan_id);
                            task_progress.set_status(TaskStatus::Completed);
                        } else {
                            info!("Scan {} was stopped, rolling back changes", scan_id);
                            if let Err(stop_err) = scan.set_state_stopped(&conn) {
                                error!("Failed to stop scan {}: {}", scan_id, stop_err);
                                task_progress.set_error(&format!("Failed to stop scan: {}", stop_err));
                            } else {
                                task_progress.set_status(TaskStatus::Stopped);
                            }
                        }
                    }
                }
                Err(e) => {
                    error!("Scan {} failed: {}", scan_id, e);
                    let error_msg = e.to_string();
                    if let Err(stop_err) = Scan::stop_scan(&conn, &scan, Some(&error_msg)) {
                        error!("Failed to stop scan {} after error: {}", scan_id, stop_err);
                        task_progress.set_error(&format!(
                            "Scan error: {}; Failed to stop: {}",
                            error_msg, stop_err
                        ));
                    } else {
                        task_progress.set_error(&error_msg);
                    }
                }
            }

            // Clean up queue and TaskManager
            if let Err(e) = TaskManager::on_scan_complete(&conn, scan_id) {
                error!("Failed to complete scan {}: {}", scan_id, e);
            }
        });

        // Store active scan state with task handle
        self.current_scan = Some(ActiveTaskInfo {
            scan_id,
            root_id,
            root_path: root_path.clone(),
            interrupt_token: interrupt_token_for_storage,
            task_progress: task_progress_for_storage,
            task_handle: Some(task_handle),
            broadcast_handle: Some(broadcast_handle),
        });

        Ok(())
    }

    /// Called when scan finishes (from background task)
    /// Cleans up queue and clears active scan
    pub fn on_scan_complete(conn: &Connection, scan_id: i64) -> Result<(), FsPulseError> {
        // Clear active scan
        let _tmr = timer!(Level::Trace; "TaskManager::on_scan_complete mutex");
        let mut manager = Self::instance().lock().unwrap();

        // Clean up queue (verifies state, deletes/clears entry)
        // If we're shutting down, we don't clear the queue entry. It will be taken care
        // of on the next run
        // Note: QueueEntry::complete_work gets its own connection internally
        if !manager.is_shutting_down && !manager.is_paused {
            QueueEntry::complete_work(conn, scan_id)?;
        }

        if let Some(active) = &manager.current_scan {
            if active.scan_id == scan_id {
                // Notify the UI that the scan is complete. It should be in a terminal state
                // This is the only place from which we send terminal messages
                // Terminal messages are a "best effort". We assume that one of two things is
                // true when the web UI is trying to show progress:
                // A) the web ui is connected and will receive this terminal message when it is sent
                // B) the web ui is connecting or is not connected, in which case they will receeive
                // a "NoActiveScan" message when they do connect
                // If the web UI is in a state in which it thinks an active scan is occuring, these
                // messages are enough to get it into a corrected state
                manager.broadcast_current_state_locked(true);
                manager.current_scan = None;
                log::info!("Scan {} completed or exited, TaskManager now idle", scan_id);
            }
        }

        Ok(())
    }

    /// Request interrupt of the current scan
    pub fn request_stop(scan_id: i64) -> Result<(), String> {
        let manager: std::sync::MutexGuard<'_, TaskManager> = Self::instance().lock().unwrap();

        match &manager.current_scan {
            Some(active) if active.scan_id == scan_id => {
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
                    TaskStatus::Stopping => Err("Scan is already stopping".to_string()),
                    TaskStatus::Stopped => Err("Scan has already been stopped".to_string()),
                    TaskStatus::Pausing => Err("Scan is currently pausing".to_string()),
                    TaskStatus::Completed => Err("Scan has already completed".to_string()),
                    TaskStatus::Error => Err("Scan has already errored".to_string()),
                }
            }
            Some(active) => Err(format!(
                "Scan {} is not the current scan (current: {})",
                scan_id, active.scan_id
            )),
            None => Err("No scan is currently running".to_string()),
        }
    }

    /// Set pause state with duration
    /// duration_seconds: -1 for indefinite, positive value for timed pause
    /// If a scan is currently running, it will be interrupted
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

        // always update the pause_intil time
        manager.pause_until = Some(pause_until);

        // If already paused, there's no need to do any additional work
        if !manager.is_paused {
            manager.is_paused = true;

            // Interrupt current scan if running
            if let Some(active) = &manager.current_scan {
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

        // Cannot pause during shutdown or database compaction
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

        // if a pause was requested, and there is an active scan, that scan is going
        // to be in the process of unwinding. We need to let that complete before
        // allowing the user to unpause
        if manager.current_scan.is_some() {
            info!("Clear pause requested while pausing an in-progress scan");
            return Err(FsPulseError::Error(
                "Can't unpause when pausing an active scan".into(),
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

        manager.try_start_next_scan_locked(conn)?;

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

    /// Subscribe to scan state updates
    /// Returns a receiver that will receive broadcast messages for all scan events
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

        if let Some(active) = &self.current_scan {
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

    /// Get current scan info
    pub fn get_current_scan_info() -> Option<CurrentScanInfo> {
        let manager = Self::instance().lock().unwrap();
        manager.current_scan.as_ref().map(|active| CurrentScanInfo {
            scan_id: active.scan_id,
            root_id: active.root_id,
            root_path: active.root_path.clone(),
        })
    }

    /// Get upcoming scans, synchronized with scan manager state
    /// If paused, includes the in-progress scan as first entry
    pub fn get_upcoming_scans(
        limit: i64,
    ) -> Result<Vec<crate::schedules::UpcomingScan>, FsPulseError> {
        let manager = Self::instance().lock().unwrap();
        let scans_are_paused = manager.is_paused;

        // Note: QueueEntry::get_upcoming_scans gets its own connection internally
        // Call schedules method while holding mutex to synchronize with try_start_next_scan
        QueueEntry::get_upcoming_scans(limit, scans_are_paused)
    }

    /// Compact the database
    /// Returns error if a scan is currently running
    /// Blocks until compaction is complete
    pub fn compact_db() -> Result<(), String> {
        // Acquire mutex and check state
        let mut manager = Self::instance().lock().unwrap();
        if manager.current_scan.is_some() {
            return Err("Cannot compact: scan in progress".to_string());
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
    /// Must be called with the scan manager mutex
    fn check_shutting_down_locked(&self) -> Result<(), FsPulseError> {
        if self.is_shutting_down {
            Err(FsPulseError::ShuttingDown)
        } else {
            Ok(())
        }
    }
}
