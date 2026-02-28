use crate::db::Database;
use crate::scans::{Scan, ScanHistoryRow, ScanStats};
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

use super::state::AppState;

/// WebSocket endpoint for streaming task progress
/// GET /ws/tasks/progress
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

/// POST /api/tasks/{task_id}/stop
/// Request stop of a running task by its task_id
pub async fn stop_task(
    State(_state): State<AppState>,
    Path(task_id): Path<i64>,
) -> Result<StatusCode, StatusCode> {
    use crate::task_manager::TaskManager;

    TaskManager::request_stop(task_id).map_err(|e| {
        error!("Failed to stop task (task_id {}): {}", task_id, e);
        StatusCode::NOT_FOUND
    })?;

    log::info!("Stop requested for task (task_id {})", task_id);
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

/// Query parameters for scan dates within a month
#[derive(Debug, Deserialize)]
pub struct ScanDatesParams {
    pub root_id: i64,
    pub year: i32,
    pub month: u32,
}

/// Response structure for scan dates
#[derive(Debug, Serialize)]
pub struct ScanDatesResponse {
    pub dates: Vec<String>,
}

/// GET /api/scans/scan_dates?root_id=X&year=YYYY&month=MM
/// Returns distinct local dates within a month that have completed scans for the root.
pub async fn get_scan_dates(
    Query(params): Query<ScanDatesParams>,
) -> Result<Json<ScanDatesResponse>, (StatusCode, String)> {
    let conn = Database::get_connection().map_err(|e| {
        error!("Database error: {}", e);
        (StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
    })?;

    let dates =
        crate::scans::get_scan_dates_for_month(&conn, params.root_id, params.year, params.month)
            .map_err(|e| {
                error!(
                    "Failed to get scan dates for root_id={}, {}-{}: {}",
                    params.root_id, params.year, params.month, e
                );
                (StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
            })?;

    Ok(Json(ScanDatesResponse { dates }))
}

/// Query parameters for scans on a specific date
#[derive(Debug, Deserialize)]
pub struct ScansByDateParams {
    pub root_id: i64,
    pub date: String,
}

/// Response structure for scans by date
#[derive(Debug, Serialize)]
pub struct ScansByDateResponse {
    pub scans: Vec<crate::scans::ScanSummary>,
}

/// GET /api/scans/by_date?root_id=X&date=YYYY-MM-DD
/// Returns all completed scans for a root on a specific date, most recent first.
pub async fn get_scans_by_date(
    Query(params): Query<ScansByDateParams>,
) -> Result<Json<ScansByDateResponse>, (StatusCode, String)> {
    let conn = Database::get_connection().map_err(|e| {
        error!("Database error: {}", e);
        (StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
    })?;

    let scans = crate::scans::get_scans_for_date(&conn, params.root_id, &params.date).map_err(
        |e| {
            error!(
                "Failed to get scans for root_id={}, date={}: {}",
                params.root_id, params.date, e
            );
            (StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
        },
    )?;

    Ok(Json(ScansByDateResponse { scans }))
}

/// Query parameters for scan resolution
#[derive(Debug, Deserialize)]
pub struct ResolveScanParams {
    pub root_id: i64,
    pub date: Option<String>, // "YYYY-MM-DD", omit for latest
}

/// Response structure for resolved scan
#[derive(Debug, Serialize)]
pub struct ResolvedScanResponse {
    pub scan_id: i64,
    pub started_at: i64,
}

/// GET /api/scans/resolve?root_id=X&date=YYYY-MM-DD
/// Resolves a date to the most recent completed scan for a root at or before that date.
/// If date is omitted, returns the latest completed scan.
pub async fn resolve_scan(
    Query(params): Query<ResolveScanParams>,
) -> Result<Json<ResolvedScanResponse>, (StatusCode, String)> {
    match Scan::resolve_scan_for_date(params.root_id, params.date.as_deref()) {
        Ok(Some((scan_id, started_at))) => Ok(Json(ResolvedScanResponse {
            scan_id,
            started_at,
        })),
        Ok(None) => Err((
            StatusCode::NOT_FOUND,
            "No completed scan found for the specified root and date".to_string(),
        )),
        Err(e) => {
            error!(
                "Failed to resolve scan for root_id={}, date={:?}: {}",
                params.root_id, params.date, e
            );
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to resolve scan: {}", e),
            ))
        }
    }
}
