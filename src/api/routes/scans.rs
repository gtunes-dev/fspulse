use crate::database::Database;
use crate::scans::{ScanHistoryRow, ScanStats};
use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        Path, Query, State,
    },
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use log::error;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

/// Shared application state
/// TaskManager is now a global singleton, so AppState is empty
#[derive(Clone)]
pub struct AppState {}

impl AppState {
    pub fn new() -> Self {
        Self {}
    }
}

/// Request structure for scheduling a scan
#[derive(Debug, Deserialize)]
pub struct ScheduleScanRequest {
    pub root_id: i64,
    pub hash_mode: String,     // "None", "New", "All"
    pub validate_mode: String, // "None", "New", "All"
}

/// POST /api/scans/schedule
/// Schedules a new manual scan through the queue
/// Returns 200 OK if scan was scheduled
/// UI should call GET /api/scans/current to check if scan started
pub async fn schedule_scan(
    State(_state): State<AppState>,
    Json(req): Json<ScheduleScanRequest>,
) -> Result<StatusCode, StatusCode> {
    use crate::task_manager::TaskManager;
    use crate::scans::{HashMode, ValidateMode};

    let conn = Database::get_connection().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // Parse scan options
    let hash_mode = match req.hash_mode.as_str() {
        "None" => HashMode::None,
        "New" => HashMode::New,
        "All" => HashMode::All,
        _ => {
            error!("Invalid hash_mode: {}", req.hash_mode);
            return Err(StatusCode::BAD_REQUEST);
        }
    };

    let validate_mode = match req.validate_mode.as_str() {
        "None" => ValidateMode::None,
        "New" => ValidateMode::New,
        "All" => ValidateMode::All,
        _ => {
            error!("Invalid validate_mode: {}", req.validate_mode);
            return Err(StatusCode::BAD_REQUEST);
        }
    };

    // Schedule manual scan (creates queue entry and tries to start it)
    TaskManager::schedule_manual_scan(&conn, req.root_id, hash_mode, validate_mode).map_err(
        |e| {
            error!("Failed to schedule manual scan: {}", e);
            if e.to_string().contains("Root not found") {
                StatusCode::NOT_FOUND
            } else {
                StatusCode::INTERNAL_SERVER_ERROR
            }
        },
    )?;

    log::info!("Manual scan scheduled for root {}", req.root_id);

    Ok(StatusCode::OK)
}

/// WebSocket endpoint for streaming scan progress
/// GET /ws/scans/progress
pub async fn scan_progress_ws(
    ws: WebSocketUpgrade,
    State(_state): State<AppState>,
) -> impl IntoResponse {
    ws.on_upgrade(handle_scan_progress)
}

async fn handle_scan_progress(mut socket: WebSocket) {
    use crate::task_manager::TaskManager;

    log::info!("[WS] New WebSocket connection established");

    // Subscribe to scan state broadcasts from TaskManager
    let mut receiver = TaskManager::subscribe();

    // Send initial state to client immediately upon connection
    // This is the handshake that ensures clients always know the current state
    log::info!("[WS] Broadcasting current state to new client");

    // Immediately broadcast status. Terminal status is only sent from
    TaskManager::broadcast_current_state(false);

    // Stream broadcast messages (ActiveScan or NoActiveScan)
    loop {
        tokio::select! {
            result = receiver.recv() => {
                match result {
                    Ok(broadcast_message) => {
                        if let Ok(json) = serde_json::to_string(&broadcast_message) {
                            if socket.send(Message::Text(json.into())).await.is_err() {
                                log::info!("[WS] Client disconnected (send failed)");
                                break; // Client disconnected
                            }
                        }
                    }
                    Err(e) => {
                        // Channel closed or lagged
                        log::info!("[WS] Broadcast channel error: {:?} - closing connection", e);
                        break;
                    }
                }
            }
            msg = socket.recv() => {
                // Handle incoming messages from client (keepalive pings)
                match msg {
                    Some(Ok(Message::Text(_))) => {
                        // Client sent a text ping - ignore it, the act of receiving keeps connection alive
                    }
                    Some(Ok(Message::Ping(_))) => {
                        // Client sent a ping - Axum automatically sends pong
                    }
                    Some(Ok(Message::Close(_))) | None => {
                        // Client closed connection
                        log::info!("[WS] Client initiated close");
                        break;
                    }
                    Some(Err(e)) => {
                        // Error receiving message
                        log::info!("[WS] Error receiving from client: {:?}", e);
                        break;
                    }
                    _ => {}
                }
            }
            _ = tokio::time::sleep(tokio::time::Duration::from_secs(30)) => {
                // Send ping to keep connection alive
                if socket.send(Message::Ping(vec![].into())).await.is_err() {
                    log::info!("[WS] Keepalive ping failed - client disconnected");
                    break;
                }
            }
        }
    }
    log::info!("[WS] WebSocket handler exiting");
    // WebSocket will close automatically when dropped
}

/// POST /api/scans/{scan_id}/stop
/// Request stop of a running scan
pub async fn stop_scan(
    State(_state): State<AppState>,
    Path(scan_id): Path<i64>,
) -> Result<StatusCode, StatusCode> {
    use crate::task_manager::TaskManager;

    TaskManager::request_stop(scan_id).map_err(|e| {
        error!("Failed to stop scan {}: {}", scan_id, e);
        StatusCode::NOT_FOUND
    })?;

    log::info!("Stop requested for scan {}", scan_id);
    Ok(StatusCode::ACCEPTED) // 202 Accepted - stop requested, will complete async
}

/// Request structure for setting pause
#[derive(Debug, Deserialize)]
pub struct PauseRequest {
    pub duration_seconds: i64, // -1 for indefinite
}

/// POST /api/pause
/// Set pause with duration - stops current scan and prevents new scans
pub async fn set_pause(
    State(_state): State<AppState>,
    Json(req): Json<PauseRequest>,
) -> Result<StatusCode, StatusCode> {
    use crate::task_manager::TaskManager;

    let conn = Database::get_connection().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    TaskManager::set_pause(&conn, req.duration_seconds).map_err(|e| {
        error!("Failed to set pause: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    log::info!("Pause set for {} seconds", req.duration_seconds);
    Ok(StatusCode::OK)
}

/// DELETE /api/pause
/// Clear pause - allows scanning to resume
pub async fn clear_pause(State(_state): State<AppState>) -> Result<StatusCode, StatusCode> {
    use crate::task_manager::TaskManager;

    let conn = Database::get_connection().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    TaskManager::clear_pause(&conn).map_err(|e| {
        error!("Failed to clear pause: {}", e);
        StatusCode::BAD_REQUEST // 400 - likely tried to unpause while scan unwinding
    })?;

    log::info!("Pause cleared");
    Ok(StatusCode::OK)
}

/// GET /api/scans/current
/// Get information about the currently running scan, if any
pub async fn get_current_scan(
    State(_state): State<AppState>,
) -> Result<Json<Option<crate::task_manager::CurrentScanInfo>>, StatusCode> {
    use crate::task_manager::TaskManager;

    let current = TaskManager::get_current_scan_info();
    Ok(Json(current))
}

/// GET /api/home/last-scan-stats
/// Get statistics for the most recent scan (used by Home page dashboard)
pub async fn get_last_scan_stats() -> Result<Json<Value>, StatusCode> {
    let conn = Database::get_connection().map_err(|e| {
        eprintln!("Database error: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    match ScanStats::get_latest(&conn) {
        Ok(Some(stats)) => Ok(Json(json!({
            "state": "last_scan",
            "scan_id": stats.scan_id,
            "root_id": stats.root_id,
            "root_path": stats.root_path,
            "scan_state": format!("{:?}", stats.state),
            "started_at": stats.started_at,
            "total_files": stats.total_files,
            "total_folders": stats.total_folders,
            "total_size": stats.total_size,
            "total_adds": stats.total_adds,
            "total_modifies": stats.total_modifies,
            "total_deletes": stats.total_deletes,
            "files_added": stats.files_added,
            "files_modified": stats.files_modified,
            "files_deleted": stats.files_deleted,
            "folders_added": stats.folders_added,
            "folders_modified": stats.folders_modified,
            "folders_deleted": stats.folders_deleted,
            "items_hashed": stats.items_hashed,
            "items_validated": stats.items_validated,
            "alerts_generated": stats.alerts_generated,
            "hash_enabled": stats.hash_enabled,
            "validation_enabled": stats.validation_enabled,
            "error": stats.error,
        }))),
        Ok(None) => Ok(Json(json!({
            "state": "no_scans"
        }))),
        Err(e) => {
            eprintln!("Error getting scan stats: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// Query parameters for scan history count endpoint
#[derive(Debug, Deserialize)]
pub struct ScanHistoryCountParams {
    pub root_id: Option<i64>,
}

/// Response structure for scan history count
#[derive(Debug, Serialize)]
pub struct ScanHistoryCountResponse {
    pub count: i64,
}

/// Query parameters for scan history fetch endpoint
#[derive(Debug, Deserialize)]
pub struct ScanHistoryFetchParams {
    pub root_id: Option<i64>,
    pub limit: i64,
    pub offset: i64,
}

/// Response structure for scan history fetch
#[derive(Debug, Serialize)]
pub struct ScanHistoryFetchResponse {
    pub scans: Vec<ScanHistoryRow>,
}

/// GET /api/scans/scan_history/count?root_id=X
/// Returns count of scan history entries (completed, stopped, or error states)
/// Optionally filtered by root_id
pub async fn get_scan_history_count(
    Query(params): Query<ScanHistoryCountParams>,
) -> Result<Json<ScanHistoryCountResponse>, StatusCode> {
    let conn = Database::get_connection().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let count = crate::scans::get_scan_history_count(&conn, params.root_id).map_err(|e| {
        error!("Failed to get scan history count: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok(Json(ScanHistoryCountResponse { count }))
}

/// GET /api/scans/scan_history/fetch?root_id=X&limit=25&offset=0
/// Returns paginated scan history with schedule information
/// Optionally filtered by root_id
pub async fn get_scan_history_fetch(
    Query(params): Query<ScanHistoryFetchParams>,
) -> Result<Json<ScanHistoryFetchResponse>, StatusCode> {
    let conn = Database::get_connection().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let scans = crate::scans::get_scan_history(&conn, params.root_id, params.limit, params.offset)
        .map_err(|e| {
            error!("Failed to get scan history: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    Ok(Json(ScanHistoryFetchResponse { scans }))
}
