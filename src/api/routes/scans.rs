use crate::db::Database;
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
use serde::Deserialize;

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
