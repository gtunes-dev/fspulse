use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        Path, State,
    },
    http::StatusCode,
    response::IntoResponse,
    Extension, Json,
};
use log::error;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use crate::database::Database;
use crate::progress::web::{ProgressEvent, WebProgressReporter};
use crate::progress::ProgressReporter;
use crate::roots::Root;
use crate::scanner::Scanner;
use crate::scans::{AnalysisSpec, HashMode, Scan, ScanState, ValidateMode};
use crossbeam_channel::Receiver;

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
pub async fn get_scans_status(
    Extension(db_path): Extension<Option<PathBuf>>,
) -> Result<Json<ScansStatusResponse>, StatusCode> {
    let db = Database::new(db_path).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

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
                if scan.state() != ScanState::Completed && scan.state() != ScanState::Stopped =>
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
    pub db_path: Option<PathBuf>,
    pub active_scans: Arc<Mutex<HashMap<i64, Receiver<ProgressEvent>>>>,
}

impl AppState {
    pub fn new(db_path: Option<PathBuf>) -> Self {
        Self {
            db_path,
            active_scans: Arc::new(Mutex::new(HashMap::new())),
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
    let db = Database::new(state.db_path.clone())
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
        .filter(|s| s.state() != ScanState::Completed && s.state() != ScanState::Stopped);

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

    // Create web progress reporter
    let (reporter, event_receiver) =
        WebProgressReporter::new(scan_id, root.root_path().to_string());
    let reporter = Arc::new(reporter);

    // Store event receiver in state for WebSocket streaming
    state
        .active_scans
        .lock()
        .unwrap()
        .insert(scan_id, event_receiver);

    // Spawn scan in background using tokio::task::spawn_blocking
    let db_path = state.db_path.clone();
    let root_id = root.root_id();
    let mut scan_copy = scan;

    tokio::task::spawn_blocking(move || {
        let mut db = Database::new(db_path).expect("Failed to open database");
        let root = Root::get_by_id(&db, root_id)
            .expect("Failed to get root")
            .expect("Root not found");

        if let Err(e) = Scanner::do_scan_machine(&mut db, &mut scan_copy, &root, reporter.clone())
        {
            error!("Scan failed: {}", e);
            let _ = reporter.println(&format!("Scan error: {}", e));
        }

        // Emit scan completed event
        let _ = reporter.println("Scan completed");
    });

    Ok(Json(InitiateScanResponse { scan_id }))
}

/// WebSocket endpoint for streaming scan progress
/// GET /ws/scans/{scan_id}
pub async fn scan_progress_ws(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
    Path(scan_id): Path<i64>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_scan_progress(socket, state, scan_id))
}

async fn handle_scan_progress(mut socket: WebSocket, state: AppState, scan_id: i64) {
    // Get the event receiver for this scan
    let receiver = {
        let mut scans = state.active_scans.lock().unwrap();
        scans.remove(&scan_id)
    };

    if let Some(receiver) = receiver {
        // Stream events to the WebSocket
        loop {
            match receiver.try_recv() {
                Ok(event) => {
                    let json = serde_json::to_string(&event).unwrap();
                    if socket.send(Message::Text(json.into())).await.is_err() {
                        break;
                    }
                }
                Err(crossbeam_channel::TryRecvError::Empty) => {
                    // No events available, wait a bit
                    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
                }
                Err(crossbeam_channel::TryRecvError::Disconnected) => {
                    // Scan is complete, close connection
                    break;
                }
            }
        }
    }
    // WebSocket will close automatically when dropped
}
