use super::*;
use crate::progress::state::{PhaseInfo, ProgressInfo, ScanProgressState, ScanStatus, ThreadOperation};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tokio::sync::broadcast;

/// Web implementation of ProgressReporter using state snapshots
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
pub struct WebProgressReporter {
    state: Arc<Mutex<ScanProgressState>>,
    broadcaster: broadcast::Sender<ScanProgressState>,
    // Map ProgressId to thread index for thread-specific progress updates
    thread_map: Arc<Mutex<HashMap<ProgressId, usize>>>,
    // Map ProgressId to context for detecting scanning/file updates
    context_map: Arc<Mutex<HashMap<ProgressId, ProgressContext>>>,
}

#[derive(Debug, Clone)]
enum ProgressContext {
    DirectorySpinner,
    FileSpinner,
    AnalysisBar,
}

impl WebProgressReporter {
    /// Create a web progress reporter that broadcasts state snapshots every 250ms
    ///
    /// Uses the provided broadcaster channel to send state updates to all WebSocket clients
    pub fn new(
        scan_id: i64,
        root_id: i64,
        root_path: String,
        broadcaster: broadcast::Sender<ScanProgressState>,
    ) -> Self {
        let state = Arc::new(Mutex::new(ScanProgressState::new(scan_id, root_id, root_path)));

        // Spawn background task to periodically broadcast state
        let state_clone = Arc::clone(&state);
        let tx_clone = broadcaster.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(tokio::time::Duration::from_millis(250));
            loop {
                interval.tick().await;
                let current_state = {
                    let state_guard = state_clone.lock().unwrap();
                    state_guard.clone()
                };

                // Broadcast returns Err only if there are no receivers, which is fine
                let _ = tx_clone.send(current_state);

                // Stop broadcasting only when scan reaches a terminal state
                // Continue broadcasting during Cancelling to show thread cleanup progress
                let is_complete = {
                    let state_guard = state_clone.lock().unwrap();
                    matches!(
                        state_guard.status,
                        ScanStatus::Idle | ScanStatus::Stopped | ScanStatus::Completed | ScanStatus::Error { .. }
                    )
                };
                if is_complete {
                    // Send one final update and exit
                    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
                    let final_state = state_clone.lock().unwrap().clone();
                    let _ = tx_clone.send(final_state);
                    break;
                }
            }
        });

        Self {
            state,
            broadcaster,
            thread_map: Arc::new(Mutex::new(HashMap::new())),
            context_map: Arc::new(Mutex::new(HashMap::new())),
        }
    }

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

    /// Mark scan as cancelling (user requested stop, scanner hasn't detected yet)
    pub fn mark_cancelling(&self) {
        let mut state = self.state.lock().unwrap();
        state.status = ScanStatus::Cancelling;
    }

    /// Mark scan as stopped (scanner detected cancellation and rolled back)
    pub fn mark_stopped(&self) {
        let mut state = self.state.lock().unwrap();
        state.status = ScanStatus::Stopped;
    }

    /// Get current status (for checking before state transitions)
    pub fn get_status(&self) -> ScanStatus {
        let state = self.state.lock().unwrap();
        state.status.clone()
    }
}

impl ProgressReporter for WebProgressReporter {
    fn section_start(&self, stage_index: u32, message: &str) -> ProgressId {
        let id = ProgressId::new();

        // Parse phase from message
        let phase_name = if message.contains("scanning") {
            "scanning"
        } else if message.contains("Tombstoning") {
            "sweeping"
        } else {
            "analyzing"
        }
        .to_string();

        let mut state = self.state.lock().unwrap();

        // If there was a current phase, move it to completed
        if let Some(current) = state.current_phase.take() {
            // Create breadcrumb for completed phase
            let breadcrumb = match current.name.as_str() {
                "scanning" => {
                    if let Some(ref scan_progress) = state.scanning_progress {
                        format!(
                            "Scanned {} files in {} directories",
                            scan_progress.files_scanned, scan_progress.directories_scanned
                        )
                    } else {
                        "Scanning complete".to_string()
                    }
                }
                "sweeping" => "Tombstoned deleted items".to_string(),
                "analyzing" => {
                    if let Some(ref progress) = state.overall_progress {
                        format!("Analyzed {} files", progress.completed)
                    } else {
                        "Analysis complete".to_string()
                    }
                }
                _ => format!("{} complete", current.name),
            };
            state.completed_phases.push(breadcrumb);
        }

        state.current_phase = Some(PhaseInfo {
            name: phase_name,
            stage_index,
        });

        id
    }

    fn section_finish(&self, _id: ProgressId, _message: &str) {
        // Phase completion is handled when the next phase starts
        // or when scan completes
    }

    fn create(&self, config: ProgressConfig) -> ProgressId {
        let id = ProgressId::new();

        // Check if this is a thread-specific progress indicator (prefix like "[01/20]")
        let is_thread_bar = config.prefix.contains('[') && config.prefix.contains('/');

        // Only initialize overall progress for non-thread bars
        if !is_thread_bar {
            if let ProgressStyle::Bar { total } = config.style {
                let mut state = self.state.lock().unwrap();
                state.overall_progress = Some(ProgressInfo {
                    completed: 0,
                    total,
                    percentage: 0.0,
                });
            }
        }

        // Detect context from prefix/message
        let context = if config.prefix.contains("Directory") || config.message.contains("Directory") {
            Some(ProgressContext::DirectorySpinner)
        } else if config.prefix.contains("File")
            || config.prefix.contains("Item")
            || config.message.contains("File")
            || config.message.contains("Item")
        {
            Some(ProgressContext::FileSpinner)
        } else if let ProgressStyle::Bar { .. } = config.style {
            Some(ProgressContext::AnalysisBar)
        } else {
            None
        };

        if let Some(ctx) = context {
            self.context_map.lock().unwrap().insert(id, ctx);
        }

        // Store thread mapping if it's a thread bar
        if is_thread_bar {
            let parts: Vec<&str> = config
                .prefix
                .trim()
                .trim_matches(|c| c == '[' || c == ']')
                .split('/')
                .collect();
            if parts.len() == 2 {
                if let Ok(thread_index) = parts[0].parse::<usize>() {
                    // Store mapping from ProgressId to thread index (0-indexed)
                    self.thread_map.lock().unwrap().insert(id, thread_index - 1);
                }
            }
        }

        id
    }

    fn update_work(&self, id: ProgressId, work: WorkUpdate) {
        // Check for scanning updates (files/directories)
        let context_opt = self.context_map.lock().unwrap().get(&id).cloned();
        match (&work, context_opt) {
            (WorkUpdate::Directory { .. }, Some(ProgressContext::DirectorySpinner)) => {
                let mut state = self.state.lock().unwrap();
                state.increment_scanning(true);
                return;
            }
            (WorkUpdate::File { .. }, Some(ProgressContext::FileSpinner)) => {
                let mut state = self.state.lock().unwrap();
                state.increment_scanning(false);
                return;
            }
            _ => {}
        }

        // Look up thread index for this ProgressId
        let thread_index_opt = self.thread_map.lock().unwrap().get(&id).copied();

        if let Some(thread_index) = thread_index_opt {
            // This is a thread-specific update
            let mut state = self.state.lock().unwrap();
            let operation = match work {
                WorkUpdate::Hashing { file } => ThreadOperation::Hashing { file },
                WorkUpdate::Validating { file } => ThreadOperation::Validating { file },
                WorkUpdate::Idle => ThreadOperation::Idle,
                _ => return, // Not a thread-specific operation
            };
            state.update_thread(thread_index, operation);
        }
    }

    fn set_position(&self, id: ProgressId, position: u64) {
        // Only update overall progress if this is NOT a thread-specific bar
        let is_thread_bar = self.thread_map.lock().unwrap().contains_key(&id);
        if !is_thread_bar {
            let mut state = self.state.lock().unwrap();
            if let Some(ref mut progress) = state.overall_progress {
                progress.completed = position;
                progress.percentage = if progress.total > 0 {
                    (position as f64 / progress.total as f64) * 100.0
                } else {
                    0.0
                };
            }
        }
    }

    fn set_length(&self, id: ProgressId, length: u64) {
        // Only update overall progress if this is NOT a thread-specific bar
        let is_thread_bar = self.thread_map.lock().unwrap().contains_key(&id);
        if !is_thread_bar {
            let mut state = self.state.lock().unwrap();
            if let Some(ref mut progress) = state.overall_progress {
                progress.total = length;
                progress.percentage = if length > 0 {
                    (progress.completed as f64 / length as f64) * 100.0
                } else {
                    0.0
                };
            }
        }
    }

    fn inc(&self, id: ProgressId, delta: u64) {
        // Only update overall progress if this is NOT a thread-specific bar
        let is_thread_bar = self.thread_map.lock().unwrap().contains_key(&id);
        if !is_thread_bar {
            let mut state = self.state.lock().unwrap();
            if let Some(ref mut progress) = state.overall_progress {
                progress.completed += delta;
                progress.percentage = if progress.total > 0 {
                    (progress.completed as f64 / progress.total as f64) * 100.0
                } else {
                    0.0
                };
            }
        }
    }

    fn enable_steady_tick(&self, _id: ProgressId, _interval: Duration) {
        // No-op for web - steady ticks are visual only
    }

    fn disable_steady_tick(&self, _id: ProgressId) {
        // No-op for web
    }

    fn finish_and_clear(&self, id: ProgressId) {
        // Set thread to idle before cleanup if this is a thread-specific progress bar
        if let Some(thread_index) = self.thread_map.lock().unwrap().get(&id).copied() {
            let mut state = self.state.lock().unwrap();
            state.update_thread(thread_index, ThreadOperation::Idle);
        }

        // Clean up mappings
        self.thread_map.lock().unwrap().remove(&id);
        self.context_map.lock().unwrap().remove(&id);
    }

    fn println(&self, message: &str) -> Result<(), Box<dyn std::error::Error>> {
        let mut state = self.state.lock().unwrap();
        state.add_message(message.to_string());
        Ok(())
    }

    fn clone_reporter(&self) -> Arc<dyn ProgressReporter> {
        Arc::new(Self {
            state: Arc::clone(&self.state),
            broadcaster: self.broadcaster.clone(),
            thread_map: Arc::clone(&self.thread_map),
            context_map: Arc::clone(&self.context_map),
        })
    }
}
