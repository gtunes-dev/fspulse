use axum::{
    extract::{Json, Query, State},
    http::StatusCode,
};
use log::error;
use serde::{Deserialize, Serialize};

use crate::db::Database;
use crate::schedules::{TaskEntry, TaskHistoryRow};
use crate::scans::{HashMode, ValidateMode};
use crate::task::TaskType;
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

/// Query parameters for task history count endpoint
#[derive(Debug, Deserialize)]
pub struct TaskHistoryCountParams {
    pub task_type: Option<i64>,
}

/// Response structure for task history count
#[derive(Debug, Serialize)]
pub struct TaskHistoryCountResponse {
    pub count: i64,
}

/// Query parameters for task history fetch endpoint
#[derive(Debug, Deserialize)]
pub struct TaskHistoryFetchParams {
    pub task_type: Option<i64>,
    pub limit: i64,
    pub offset: i64,
}

/// Response structure for task history fetch
#[derive(Debug, Serialize)]
pub struct TaskHistoryFetchResponse {
    pub tasks: Vec<TaskHistoryRow>,
}

/// GET /api/tasks/history/count?task_type=0
/// Returns count of task history entries (completed, stopped, or error states)
/// Optionally filtered by task_type
pub async fn get_task_history_count(
    Query(params): Query<TaskHistoryCountParams>,
) -> Result<Json<TaskHistoryCountResponse>, StatusCode> {
    let task_type = params.task_type.map(TaskType::from_i64);

    let count = TaskEntry::get_task_history_count(task_type).map_err(|e| {
        error!("Failed to get task history count: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok(Json(TaskHistoryCountResponse { count }))
}

/// GET /api/tasks/history/fetch?task_type=0&limit=25&offset=0
/// Returns paginated task history with root and schedule information
/// Optionally filtered by task_type
pub async fn get_task_history_fetch(
    Query(params): Query<TaskHistoryFetchParams>,
) -> Result<Json<TaskHistoryFetchResponse>, StatusCode> {
    let task_type = params.task_type.map(TaskType::from_i64);

    let tasks = TaskEntry::get_task_history(task_type, params.limit, params.offset)
        .map_err(|e| {
            error!("Failed to get task history: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    Ok(Json(TaskHistoryFetchResponse { tasks }))
}
