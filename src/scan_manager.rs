use crate::database::Database;
use crate::error::FsPulseError;
use crate::progress::{BroadcastMessage, ProgressReporter, ScanStatus};
use crate::roots::Root;
use crate::scanner::Scanner;
use crate::scans::{HashMode, Scan, ValidateMode};
use crate::schedules::{QueueEntry, Schedule};
use log::{error, info};
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::broadcast;
use tokio::task::JoinHandle;

/// Global singleton instance
static SCAN_MANAGER: Lazy<Mutex<ScanManager>> = Lazy::new(|| {
    let (broadcaster, _) = broadcast::channel(1024);
    Mutex::new(ScanManager {
        current_scan: None,
        broadcaster,
    })
});

/// Manages the currently active scan with singleton semantics
pub struct ScanManager {
    current_scan: Option<ActiveScanInfo>,
    broadcaster: broadcast::Sender<BroadcastMessage>,
}

/// Information about the currently running scan
struct ActiveScanInfo {
    scan_id: i64,
    root_id: i64,
    root_path: String,
    cancel_token: Arc<AtomicBool>,
    reporter: Arc<ProgressReporter>,
    #[allow(dead_code)]
    task_handle: Option<JoinHandle<()>>,
    #[allow(dead_code)]
    broadcast_handle: Option<JoinHandle<()>>,
}

/// Information about current scan for status queries
#[derive(Debug, Serialize, Deserialize)]
pub struct CurrentScanInfo {
    pub scan_id: i64,
    pub root_id: i64,
    pub root_path: String,
}

impl ScanManager {
    /// Get the global singleton instance
    pub fn instance() -> &'static Mutex<ScanManager> {
        &SCAN_MANAGER
    }

    /// Entry point 1: Manual scan from UI
    /// Creates queue entry and immediately tries to start it
    /// Returns Ok if scan was scheduled, Err if scheduling failed
    /// UI should check GET /api/scans/current to see if scan started
    pub fn schedule_manual_scan(
        db: &Database,
        root_id: i64,
        hash_mode: HashMode,
        validate_mode: ValidateMode,
    ) -> Result<(), FsPulseError> {
        // Hold mutex for entire operation to avoid race
        let mut manager = Self::instance().lock().unwrap();

        // Create queue entry atomically with root existence check
        db.immediate_transaction(|conn| {
            QueueEntry::create_manual(conn, root_id, hash_mode, validate_mode)
        })?;

        // Try to start immediately (while still holding mutex)
        // Whether it starts or not, scheduling succeeded
        manager.try_start_next_scan(db)?;

        Ok(())
    }

    /// Create a new schedule
    /// Creates schedule and queue entry atomically
    /// Returns the created schedule with assigned schedule_id
    pub fn create_schedule(
        db: &Database,
        params: crate::schedules::CreateScheduleParams,
    ) -> Result<Schedule, FsPulseError> {
        // Hold mutex to prevent queue modifications during schedule creation
        let _manager = Self::instance().lock().unwrap();

        // Create schedule and queue entry in transaction
        db.immediate_transaction(|conn| Schedule::create_and_queue(conn, params))
    }

    /// Update an existing schedule
    /// Updates schedule and recalculates next_scan_time atomically
    pub fn update_schedule(db: &Database, schedule: &Schedule) -> Result<(), FsPulseError> {
        // Hold mutex to prevent queue modifications during schedule update
        let _manager = Self::instance().lock().unwrap();

        // Update schedule in transaction
        db.immediate_transaction(|conn| schedule.update(conn))
    }

    /// Delete a schedule
    /// Deletes schedule and associated queue entry atomically
    /// Fails if a scan is currently running for this schedule
    pub fn delete_schedule(db: &Database, schedule_id: i64) -> Result<(), FsPulseError> {
        // Hold mutex to prevent queue modifications during schedule deletion
        let _manager = Self::instance().lock().unwrap();

        // Delete schedule in transaction
        db.immediate_transaction(|conn| Schedule::delete(conn, schedule_id))
    }

    /// Set schedule enabled/disabled state
    /// When disabling: removes from queue (running scan completes normally)
    /// When enabling: recalculates next_scan_time and adds back to queue
    pub fn set_schedule_enabled(
        db: &Database,
        schedule_id: i64,
        enabled: bool,
    ) -> Result<(), FsPulseError> {
        // Hold mutex to prevent queue modifications during enable/disable
        let _manager = Self::instance().lock().unwrap();

        // set_enabled already creates its own transaction
        Schedule::set_enabled(db, schedule_id, enabled)
    }

    /// Entry point 2: Background polling (every 5 seconds)
    pub fn poll_queue(db: &Database) -> Result<(), FsPulseError> {
        let mut manager = Self::instance().lock().unwrap();

        // Try to start next scan - it's fine if nothing happens
        manager.try_start_next_scan(db)?;

        Ok(())
    }

    /// Shared logic: Find and start next scan
    /// Called with mutex already held
    /// Updates self.current_scan if scan started
    fn try_start_next_scan(&mut self, db: &Database) -> Result<(), FsPulseError> {
        // Already running?
        if self.current_scan.is_some() {
            return Ok(());
        }

        // Get next scan from queue (creates scan, updates queue)
        let mut scan = match QueueEntry::get_next_scan(db)? {
            Some(s) => s,
            None => return Ok(()), // No work available - not an error
        };

        let scan_id = scan.scan_id();
        let root_id = scan.root_id();

        // Get root for progress reporting
        let root = Root::get_by_id(db.conn(), root_id)?
            .ok_or_else(|| FsPulseError::Error("Root not found".to_string()))?;
        let root_path = root.root_path().to_string();

        // Create progress reporter that maintains scan state
        let reporter = Arc::new(ProgressReporter::new(scan_id, root_id, root_path.clone()));
        let cancel_token = Arc::new(AtomicBool::new(false));

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
                                active.reporter.get_status(),
                                ScanStatus::Completed
                                    | ScanStatus::Stopped
                                    | ScanStatus::Error { .. }
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

        // Store active scan state
        self.current_scan = Some(ActiveScanInfo {
            scan_id,
            root_id,
            root_path: root_path.clone(),
            cancel_token: Arc::clone(&cancel_token),
            reporter: Arc::clone(&reporter),
            task_handle: None,
            broadcast_handle: Some(broadcast_handle),
        });

        // Spawn scan task
        tokio::task::spawn_blocking(move || {
            let db = Database::new().expect("Failed to open database");
            let root = Root::get_by_id(db.conn(), root_id)
                .expect("Failed to get root")
                .expect("Root not found");

            // Run scan
            let mut db_mut = db;
            let scan_result = Scanner::do_scan_machine(
                &mut db_mut,
                &mut scan,
                &root,
                reporter.clone(),
                cancel_token,
            );

            // Handle result
            match scan_result {
                Ok(()) => {
                    reporter.mark_completed();
                }
                Err(ref e) if matches!(e, FsPulseError::ScanCancelled) => {
                    info!("Scan {} was cancelled, rolling back changes", scan_id);
                    if let Err(stop_err) = scan.set_state_stopped(&mut db_mut) {
                        error!("Failed to stop scan {}: {}", scan_id, stop_err);
                        reporter.mark_error(format!("Failed to stop scan: {}", stop_err));
                    } else {
                        reporter.mark_stopped();
                    }
                }
                Err(e) => {
                    error!("Scan {} failed: {}", scan_id, e);
                    let error_msg = e.to_string();
                    if let Err(stop_err) = Scan::stop_scan(&mut db_mut, &scan, Some(&error_msg)) {
                        error!("Failed to stop scan {} after error: {}", scan_id, stop_err);
                        reporter.mark_error(format!(
                            "Scan error: {}; Failed to stop: {}",
                            error_msg, stop_err
                        ));
                    } else {
                        reporter.mark_error(error_msg);
                    }
                }
            }

            // Clean up queue and ScanManager
            if let Err(e) = ScanManager::on_scan_complete(&db_mut, scan_id) {
                error!("Failed to complete scan {}: {}", scan_id, e);
            }
        });

        Ok(())
    }

    /// Called when scan finishes (from background task)
    /// Cleans up queue and clears active scan
    pub fn on_scan_complete(db: &Database, scan_id: i64) -> Result<(), FsPulseError> {
        // Clear active scan
        let mut manager = Self::instance().lock().unwrap();

        // Clean up queue (verifies state, deletes/clears entry)
        QueueEntry::complete_work(db, scan_id)?;

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
                log::info!("Scan {} completed, ScanManager now idle", scan_id);
            }
        }

        Ok(())
    }

    /// Request cancellation of the current scan
    pub fn request_cancellation(scan_id: i64) -> Result<(), String> {
        let manager = Self::instance().lock().unwrap();

        match &manager.current_scan {
            Some(active) if active.scan_id == scan_id => {
                let current_status = active.reporter.get_status();

                match current_status {
                    ScanStatus::Running => {
                        active.reporter.mark_cancelling();
                        active.cancel_token.store(true, Ordering::Release);
                        Ok(())
                    }
                    ScanStatus::Cancelling => Err("Scan is already cancelling".to_string()),
                    ScanStatus::Stopped => Err("Scan has already been stopped".to_string()),
                    ScanStatus::Completed => Err("Scan has already completed".to_string()),
                    ScanStatus::Error { .. } => Err("Scan has already errored".to_string()),
                }
            }
            Some(active) => Err(format!(
                "Scan {} is not the current scan (current: {})",
                scan_id, active.scan_id
            )),
            None => Err("No scan is currently running".to_string()),
        }
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
        let manager = Self::instance().lock().unwrap();

        manager.broadcast_current_state_locked(allow_send_terminal);
    }

    /// Broadcast current state immediately
    /// Called on WebSocket connection and by broadcast thread
    /// Thread-safe: acquires mutex to read current state
    pub fn broadcast_current_state_locked(&self, allow_send_terminal: bool) {
        if let Some(active) = &self.current_scan {
            let scan_progress_status = active.reporter.get_current_state();
            let scan_status = active.reporter.get_current_state().status;

            match scan_status {
                ScanStatus::Completed | ScanStatus::Error { .. } | ScanStatus::Stopped => {
                    if allow_send_terminal {
                        let _ = self.broadcaster.send(BroadcastMessage::ActiveScan {
                            scan: Box::new(scan_progress_status),
                        });
                    } else {
                        // Terminal state, and sending is not allowed → do nothing.
                    }
                }
                _ => {
                    // Non-terminal state → always broadcast.
                    let _ = self.broadcaster.send(BroadcastMessage::ActiveScan {
                        scan: Box::new(scan_progress_status),
                    });
                }
            }
        } else {
            // No active scan → broadcast as before.
            let _ = self.broadcaster.send(BroadcastMessage::NoActiveScan);
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
}
