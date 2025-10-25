use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        Path, State,
    },
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use log::error;
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};

use crate::database::Database;
use crate::progress::web::WebProgressReporter;
use crate::progress::ProgressReporter;
use crate::roots::Root;
use crate::scanner::Scanner;
use crate::scans::{AnalysisSpec, HashMode, Scan, ScanState, ValidateMode};
use crate::web::scan_manager::ScanManager;

/// Response structure for scans status endpoint
#[derive(Debug, Serialize)]
pub struct ScansStatusResponse {
    pub roots: Vec<RootScanStatus>,
}

#[derive(Debug, Serialize)]
pub struct RootScanStatus {
    pub root_id: i64,
    pub root_path: String,
    pub has_incomplete_scan: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scan_options: Option<ScanOptions>,
}

#[derive(Debug, Serialize)]
pub struct ScanOptions {
    pub hash_mode: String,    // "None", "New", "All"
    pub validate_mode: String, // "None", "New", "All"
}

/// GET /api/scans/status
/// Returns list of roots with their incomplete scan status
pub async fn get_scans_status() -> Result<Json<ScansStatusResponse>, StatusCode> {
    let db = Database::new().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // Get all roots
    let roots = Root::roots_as_vec(&db).map_err(|e| {
        error!("Failed to get roots: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let mut root_statuses = Vec::new();

    for root in roots {
        // Check for incomplete scan
        let incomplete_scan = match Scan::get_latest_for_root(&db, root.root_id()) {
            Ok(Some(scan))
                if scan.state() != ScanState::Completed
                    && scan.state() != ScanState::Stopped
                    && scan.state() != ScanState::Error =>
            {
                Some(scan)
            }
            Ok(_) => None,
            Err(e) => {
                error!(
                    "Failed to get latest scan for root {}: {}",
                    root.root_id(),
                    e
                );
                None
            }
        };

        let (has_incomplete_scan, scan_options) = if let Some(scan) = incomplete_scan {
            let hash_mode_str = match scan.analysis_spec().hash_mode() {
                HashMode::None => "None",
                HashMode::New => "New",
                HashMode::All => "All",
            };

            let validate_mode_str = match scan.analysis_spec().val_mode() {
                ValidateMode::None => "None",
                ValidateMode::New => "New",
                ValidateMode::All => "All",
            };

            (
                true,
                Some(ScanOptions {
                    hash_mode: hash_mode_str.to_string(),
                    validate_mode: validate_mode_str.to_string(),
                }),
            )
        } else {
            (false, None)
        };

        root_statuses.push(RootScanStatus {
            root_id: root.root_id(),
            root_path: root.root_path().to_string(),
            has_incomplete_scan,
            scan_options,
        });
    }

    Ok(Json(ScansStatusResponse {
        roots: root_statuses,
    }))
}

/// Shared application state for managing active scans
#[derive(Clone)]
pub struct AppState {
    pub scan_manager: Arc<Mutex<ScanManager>>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            scan_manager: Arc::new(Mutex::new(ScanManager::new())),
        }
    }
}

/// Request structure for scan initiation
#[derive(Debug, Deserialize)]
pub struct InitiateScanRequest {
    pub root_id: i64,
    pub hash_mode: String,    // "None", "New", "All"
    pub validate_mode: String, // "None", "New", "All"
}

/// Response structure for scan initiation
#[derive(Debug, Serialize)]
pub struct InitiateScanResponse {
    pub scan_id: i64,
}

/// POST /api/scans/start
/// Initiates a new scan or resumes an existing one
pub async fn initiate_scan(
    State(state): State<AppState>,
    Json(req): Json<InitiateScanRequest>,
) -> Result<Json<InitiateScanResponse>, StatusCode> {
    let db = Database::new()
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // Get the root
    let root = Root::get_by_id(&db, req.root_id).map_err(|e| {
        error!("Failed to get root {}: {}", req.root_id, e);
        StatusCode::NOT_FOUND
    })?;

    let root = root.ok_or_else(|| {
        error!("Root {} not found", req.root_id);
        StatusCode::NOT_FOUND
    })?;

    // Check for existing incomplete scan
    let existing_scan = Scan::get_latest_for_root(&db, root.root_id())
        .map_err(|e| {
            error!("Failed to check for existing scan: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .filter(|s| s.state() != ScanState::Completed
            && s.state() != ScanState::Stopped
            && s.state() != ScanState::Error);

    // Determine the scan to use
    let scan = if let Some(existing_scan) = existing_scan {
        // Resume existing scan - use existing options
        log::info!(
            "Resuming scan {} for root {} with Hash={:?}, Validate={:?}",
            existing_scan.scan_id(),
            root.root_path(),
            existing_scan.analysis_spec().hash_mode(),
            existing_scan.analysis_spec().val_mode()
        );
        existing_scan
    } else {
        // Create new scan with provided options
        // Translate string values to AnalysisSpec parameters
        let no_hash = req.hash_mode.as_str() == "None";
        let hash_new = req.hash_mode.as_str() == "New";
        let no_validate = req.validate_mode.as_str() == "None";
        let validate_all = req.validate_mode.as_str() == "All";

        let analysis_spec = AnalysisSpec::new(no_hash, hash_new, no_validate, validate_all);

        log::info!(
            "Starting new scan for root {} with options: Hash={:?} (from '{}'), Validate={:?} (from '{}')",
            root.root_path(),
            analysis_spec.hash_mode(),
            req.hash_mode,
            analysis_spec.val_mode(),
            req.validate_mode
        );

        Scan::create(&db, &root, &analysis_spec).map_err(|e| {
            error!("Failed to create scan: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?
    };

    let scan_id = scan.scan_id();
    let root_id = root.root_id();
    let root_path = root.root_path().to_string();

    // Create web progress reporter (which creates the broadcaster)
    let (web_reporter, broadcaster) = WebProgressReporter::new(scan_id, root_path.clone());
    let web_reporter = Arc::new(web_reporter);
    let reporter: Arc<dyn ProgressReporter> = web_reporter.clone();

    // Atomically try to start the scan using ScanManager
    let cancel_token = {
        let mut manager = state.scan_manager.lock().unwrap();
        manager
            .try_start_scan(scan_id, root_id, root_path.clone(), broadcaster, web_reporter.clone())
            .map_err(|e| {
                error!("Failed to start scan: {}", e);
                StatusCode::CONFLICT // 409 Conflict - scan already in progress
            })?
    };

    // Clone state for the background task
    let state_clone = state.clone();
    let mut scan_copy = scan;

    // Spawn scan in background task
    tokio::task::spawn_blocking(move || {
        let mut db = Database::new().expect("Failed to open database");
        let root = Root::get_by_id(&db, root_id)
            .expect("Failed to get root")
            .expect("Root not found");

        // Run the scan with the cancel token
        let scan_result =
            Scanner::do_scan_machine(&mut db, &mut scan_copy, &root, reporter.clone(), cancel_token);

        // Handle scan result and update state status
        match scan_result {
            Ok(()) => {
                let _ = reporter.println("Scan completed successfully");
                web_reporter.mark_completed();
            }
            Err(ref e) if matches!(e, crate::error::FsPulseError::ScanCancelled) => {
                // Scan was cancelled - call set_state_stopped to rollback (state=Stopped, no error message)
                log::info!("Scan {} was cancelled, rolling back changes", scan_id);
                let _ = reporter.println("Scan cancelled, rolling back changes...");
                if let Err(stop_err) = scan_copy.set_state_stopped(&mut db) {
                    error!("Failed to stop scan {}: {}", scan_id, stop_err);
                    let _ = reporter.println(&format!("Error stopping scan: {}", stop_err));
                    web_reporter.mark_error(format!("Failed to stop scan: {}", stop_err));
                } else {
                    let _ = reporter.println("Scan stopped and rolled back");
                    web_reporter.mark_stopped();
                }
            }
            Err(e) => {
                // Scan failed with error - rollback and mark as Error with message
                error!("Scan {} failed: {}", scan_id, e);
                let error_msg = e.to_string();
                let _ = reporter.println(&format!("Scan error: {}", error_msg));

                // Call stop_scan to rollback and store error (state=Error, error message stored)
                if let Err(stop_err) = Scan::stop_scan(&mut db, &scan_copy, Some(&error_msg)) {
                    error!("Failed to stop scan {} after error: {}", scan_id, stop_err);
                    let _ = reporter.println(&format!("Error stopping scan: {}", stop_err));
                    web_reporter.mark_error(format!("Scan error: {}; Failed to stop: {}", error_msg, stop_err));
                } else {
                    web_reporter.mark_error(error_msg);
                }
            }
        }

        // Mark scan as complete in ScanManager
        state_clone.scan_manager.lock().unwrap().mark_complete(scan_id);
    });

    Ok(Json(InitiateScanResponse { scan_id }))
}

/// WebSocket endpoint for streaming scan progress
/// GET /ws/scans/progress
pub async fn scan_progress_ws(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_scan_progress(socket, state))
}

async fn handle_scan_progress(mut socket: WebSocket, state: AppState) {
    // Subscribe to scan progress state snapshots
    let receiver_result = {
        let manager = state.scan_manager.lock().unwrap();
        manager.subscribe()
    };

    let mut receiver = match receiver_result {
        Ok(rx) => rx,
        Err(e) => {
            let error_msg = format!(r#"{{"error":"{}"}}"#, e);
            let _ = socket.send(Message::Text(error_msg.into())).await;
            return;
        }
    };

    // Stream state snapshots
    loop {
        tokio::select! {
            result = receiver.recv() => {
                match result {
                    Ok(state_snapshot) => {
                        if let Ok(json) = serde_json::to_string(&state_snapshot) {
                            if socket.send(Message::Text(json.into())).await.is_err() {
                                break; // Client disconnected
                            }
                        }
                    }
                    Err(_) => {
                        // Channel closed - scan completed
                        break;
                    }
                }
            }
            _ = tokio::time::sleep(tokio::time::Duration::from_secs(30)) => {
                // Send ping to keep connection alive
                if socket.send(Message::Ping(vec![].into())).await.is_err() {
                    break;
                }
            }
        }
    }
    // WebSocket will close automatically when dropped
}

/// POST /api/scans/{scan_id}/cancel
/// Request cancellation of a running scan
pub async fn cancel_scan(
    State(state): State<AppState>,
    Path(scan_id): Path<i64>,
) -> Result<StatusCode, StatusCode> {
    let mut manager = state.scan_manager.lock().unwrap();

    manager.request_cancellation(scan_id).map_err(|e| {
        error!("Failed to cancel scan {}: {}", scan_id, e);
        StatusCode::NOT_FOUND
    })?;

    log::info!("Cancellation requested for scan {}", scan_id);
    Ok(StatusCode::ACCEPTED) // 202 Accepted - cancellation requested, will complete async
}

/// GET /api/scans/current
/// Get information about the currently running scan, if any
pub async fn get_current_scan(
    State(state): State<AppState>,
) -> Result<Json<Option<crate::web::scan_manager::CurrentScanInfo>>, StatusCode> {
    let manager = state.scan_manager.lock().unwrap();
    let current = manager.get_current_scan_info();
    Ok(Json(current))
}
