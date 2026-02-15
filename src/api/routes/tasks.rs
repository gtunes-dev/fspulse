use axum::{
    extract::{Json, State},
    http::StatusCode,
};
use log::error;
use serde::Deserialize;

use crate::database::Database;
use crate::scans::{HashMode, ValidateMode};
use crate::task_manager::TaskManager;

use super::scans::AppState;

/// Request structure for scheduling a scan
#[derive(Debug, Deserialize)]
pub struct ScheduleScanRequest {
    pub root_id: i64,
    pub hash_mode: String,     // "None", "New", "All"
    pub validate_mode: String, // "None", "New", "All"
}

/// POST /api/tasks/scan
///
/// Schedules a new manual scan task.
/// Returns 200 OK if scan was scheduled.
pub async fn schedule_scan(
    State(_state): State<AppState>,
    Json(req): Json<ScheduleScanRequest>,
) -> Result<StatusCode, StatusCode> {
    let conn = Database::get_connection().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

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

/// POST /api/tasks/compact-database
///
/// Schedules a database compaction task.
/// Returns 202 Accepted — compaction runs asynchronously via the task system.
pub async fn schedule_compact_database() -> Result<StatusCode, (StatusCode, String)> {
    let conn = Database::get_connection().map_err(|e| {
        error!("Failed to get database connection: {}", e);
        (StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
    })?;

    TaskManager::schedule_compact_database(&conn).map_err(|e| {
        error!("Failed to schedule database compaction: {}", e);
        (StatusCode::CONFLICT, e.to_string())
    })?;

    Ok(StatusCode::ACCEPTED)
}
