use super::*;
use crossbeam_channel::{unbounded, Receiver, Sender};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// Progress events that can be sent to web clients
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ProgressEvent {
    ScanStarted {
        scan_id: i64,
        root_path: String,
    },
    PhaseStarted {
        phase: String, // "scanning", "sweeping", "analyzing"
        stage_index: u32,
    },
    PhaseCompleted {
        phase: String,
    },
    DirectoryScanning {
        path: String,
    },
    FileScanning {
        path: String,
    },
    AnalysisProgress {
        completed: u64,
        total: u64,
        percentage: f64,
    },
    ThreadProgress {
        thread_index: usize,
        thread_count: usize,
        operation: String, // "hashing", "validating", "scanning", "idle"
        file: String,
    },
    ScanCompleted {
        scan_id: i64,
    },
    ScanError {
        error: String,
    },
    Message {
        text: String,
    },
}

/// State tracking for web reporter
struct ProgressState {
    current_phase: Option<String>,
    analysis_total: u64,
    analysis_completed: u64,
}

/// Web implementation of ProgressReporter using event channels
///
/// Emits structured events that can be consumed via WebSockets or stored for later retrieval.
/// Each progress update is converted into a semantic event that the web UI can understand.
pub struct WebProgressReporter {
    sender: Sender<ProgressEvent>,
    state: Arc<Mutex<ProgressState>>,
    // Map ProgressId to semantic meaning for event generation
    progress_map: Arc<Mutex<HashMap<ProgressId, ProgressContext>>>,
}

#[derive(Debug, Clone)]
enum ProgressContext {
    Section { phase: String },
    DirectorySpinner,
    FileSpinner,
    AnalysisBar,
    ThreadSpinner { thread_index: usize, thread_count: usize },
}

impl WebProgressReporter {
    /// Create a new web progress reporter
    /// Returns the reporter and a receiver for consuming events
    pub fn new(scan_id: i64, root_path: String) -> (Self, Receiver<ProgressEvent>) {
        let (sender, receiver) = unbounded();

        let reporter = Self {
            sender: sender.clone(),
            state: Arc::new(Mutex::new(ProgressState {
                current_phase: None,
                analysis_total: 0,
                analysis_completed: 0,
            })),
            progress_map: Arc::new(Mutex::new(HashMap::new())),
        };

        // Send initial event
        let _ = sender.send(ProgressEvent::ScanStarted { scan_id, root_path });

        (reporter, receiver)
    }

    fn emit(&self, event: ProgressEvent) {
        let _ = self.sender.send(event);
    }
}

impl ProgressReporter for WebProgressReporter {
    fn section_start(&self, stage_index: u32, message: &str) -> ProgressId {
        let id = ProgressId::new();

        // Parse phase from message
        let phase = if message.contains("scanning") {
            "scanning"
        } else if message.contains("Tombstoning") {
            "sweeping"
        } else {
            "analyzing"
        }
        .to_string();

        self.state.lock().unwrap().current_phase = Some(phase.clone());
        self.progress_map
            .lock()
            .unwrap()
            .insert(id, ProgressContext::Section { phase: phase.clone() });

        self.emit(ProgressEvent::PhaseStarted {
            phase,
            stage_index,
        });

        id
    }

    fn section_finish(&self, id: ProgressId, _message: &str) {
        if let Some(ProgressContext::Section { phase }) = self.progress_map.lock().unwrap().get(&id) {
            self.emit(ProgressEvent::PhaseCompleted {
                phase: phase.clone(),
            });
        }
        self.progress_map.lock().unwrap().remove(&id);
    }

    fn create(&self, config: ProgressConfig) -> ProgressId {
        let id = ProgressId::new();

        // If it's a Bar style, set the total regardless of context
        if let ProgressStyle::Bar { total } = config.style {
            self.state.lock().unwrap().analysis_total = total;
        }

        // Infer context from prefix/message
        let context = if config.prefix.contains("Directory") || config.message.contains("Directory") {
            ProgressContext::DirectorySpinner
        } else if config.prefix.contains("File")
            || config.prefix.contains("Item")
            || config.message.contains("File")
            || config.message.contains("Item")
        {
            ProgressContext::FileSpinner
        } else if let ProgressStyle::Bar { total: _ } = config.style {
            // Analysis progress bar (total already set above)
            ProgressContext::AnalysisBar
        } else if config.prefix.contains('[') {
            // Thread progress: "[01/20]" format
            let parts: Vec<&str> = config
                .prefix
                .trim()  // Remove leading/trailing whitespace first
                .trim_matches(|c| c == '[' || c == ']')
                .split('/')
                .collect();
            if parts.len() == 2 {
                if let (Ok(idx), Ok(count)) = (parts[0].parse::<usize>(), parts[1].parse::<usize>())
                {
                    ProgressContext::ThreadSpinner {
                        thread_index: idx - 1, // Convert to 0-indexed
                        thread_count: count,
                    }
                } else {
                    return id; // Parse failure - return without tracking context
                }
            } else {
                return id; // Wrong format - return without tracking context
            }
        } else {
            return id; // Unknown context, just track ID
        };

        self.progress_map.lock().unwrap().insert(id, context);
        id
    }

    fn update_work(&self, id: ProgressId, work: WorkUpdate) {
        // Directly emit structured events based on semantic work updates
        let context = self.progress_map.lock().unwrap().get(&id).cloned();

        match (work, context) {
            (WorkUpdate::Directory { path }, Some(ProgressContext::DirectorySpinner)) => {
                self.emit(ProgressEvent::DirectoryScanning { path });
            }
            (WorkUpdate::File { path }, Some(ProgressContext::FileSpinner)) => {
                self.emit(ProgressEvent::FileScanning { path });
            }
            (WorkUpdate::Hashing { file }, Some(ProgressContext::ThreadSpinner { thread_index, thread_count })) => {
                self.emit(ProgressEvent::ThreadProgress {
                    thread_index,
                    thread_count,
                    operation: "hashing".to_string(),
                    file,
                });
            }
            (WorkUpdate::Validating { file }, Some(ProgressContext::ThreadSpinner { thread_index, thread_count })) => {
                self.emit(ProgressEvent::ThreadProgress {
                    thread_index,
                    thread_count,
                    operation: "validating".to_string(),
                    file,
                });
            }
            (WorkUpdate::Idle, Some(ProgressContext::ThreadSpinner { thread_index, thread_count })) => {
                self.emit(ProgressEvent::ThreadProgress {
                    thread_index,
                    thread_count,
                    operation: "idle".to_string(),
                    file: "-".to_string(),
                });
            }
            _ => {
                // No event emission for mismatched context or unknown combinations
            }
        }
    }

    fn set_position(&self, id: ProgressId, position: u64) {
        let context = self.progress_map.lock().unwrap().get(&id).cloned();
        match context {
            Some(ProgressContext::AnalysisBar) | Some(ProgressContext::FileSpinner) => {
                let state = self.state.lock().unwrap();
                let percentage = if state.analysis_total > 0 {
                    (position as f64 / state.analysis_total as f64) * 100.0
                } else {
                    0.0
                };

                self.emit(ProgressEvent::AnalysisProgress {
                    completed: position,
                    total: state.analysis_total,
                    percentage,
                });
            }
            _ => {}
        }
    }

    fn set_length(&self, id: ProgressId, length: u64) {
        let context = self.progress_map.lock().unwrap().get(&id).cloned();
        match context {
            Some(ProgressContext::AnalysisBar) | Some(ProgressContext::FileSpinner) => {
                self.state.lock().unwrap().analysis_total = length;
            }
            _ => {}
        }
    }

    fn inc(&self, id: ProgressId, delta: u64) {
        let context = self.progress_map.lock().unwrap().get(&id).cloned();
        match context {
            Some(ProgressContext::AnalysisBar) | Some(ProgressContext::FileSpinner) => {
                let mut state = self.state.lock().unwrap();
                state.analysis_completed += delta;

                let percentage = if state.analysis_total > 0 {
                    (state.analysis_completed as f64 / state.analysis_total as f64) * 100.0
                } else {
                    0.0
                };

                self.emit(ProgressEvent::AnalysisProgress {
                    completed: state.analysis_completed,
                    total: state.analysis_total,
                    percentage,
                });
            }
            _ => {}
        }
    }

    fn enable_steady_tick(&self, _id: ProgressId, _interval: Duration) {
        // No-op for web - steady ticks are visual only
    }

    fn disable_steady_tick(&self, _id: ProgressId) {
        // No-op for web
    }

    fn finish_and_clear(&self, id: ProgressId) {
        self.progress_map.lock().unwrap().remove(&id);
    }

    fn println(&self, message: &str) -> Result<(), Box<dyn std::error::Error>> {
        self.emit(ProgressEvent::Message {
            text: message.to_string(),
        });
        Ok(())
    }

    fn clone_reporter(&self) -> Arc<dyn ProgressReporter> {
        Arc::new(Self {
            sender: self.sender.clone(),
            state: Arc::clone(&self.state),
            progress_map: Arc::clone(&self.progress_map),
        })
    }
}
