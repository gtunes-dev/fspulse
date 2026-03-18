use axum::{
    extract::{Json, Query, State},
    http::StatusCode,
};
use log::error;
use serde::{Deserialize, Serialize};

use crate::db::Database;
use crate::schedules::{SourceType, TaskEntry};
use crate::scans::HashMode;
use crate::task::{TaskStatus, TaskType};
use crate::task_manager::TaskManager;

use super::state::AppState;

/// Request structure for scheduling a scan
#[derive(Debug, Deserialize)]
pub struct ScheduleScanRequest {
    pub root_id: i64,
    pub hash_mode: String, // "None", "New", "All"
    pub is_val: bool,
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

    TaskManager::schedule_manual_scan(&conn, req.root_id, hash_mode, req.is_val).map_err(
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
    pub root_id: Option<i64>,
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
    pub root_id: Option<i64>,
    pub limit: i64,
    pub offset: i64,
}

/// A task history row with scan-specific fields for the API response.
#[derive(Debug, Serialize)]
pub struct TaskHistoryResponseRow {
    pub task_id: i64,
    pub task_type: TaskType,
    pub root_id: Option<i64>,
    pub root_path: Option<String>,
    pub schedule_name: Option<String>,
    pub source: SourceType,
    pub status: TaskStatus,
    pub started_at: Option<i64>,
    pub completed_at: Option<i64>,
    pub scan_id: Option<i64>,
    pub add_count: Option<i64>,
    pub modify_count: Option<i64>,
    pub delete_count: Option<i64>,
    pub was_restarted: Option<bool>,
}

/// Response structure for task history fetch
#[derive(Debug, Serialize)]
pub struct TaskHistoryFetchResponse {
    pub tasks: Vec<TaskHistoryResponseRow>,
}

/// GET /api/tasks/history/count?task_type=0&root_id=1
/// Returns count of task history entries (completed, stopped, or error states)
/// Optionally filtered by task_type and/or root_id
pub async fn get_task_history_count(
    Query(params): Query<TaskHistoryCountParams>,
) -> Result<Json<TaskHistoryCountResponse>, StatusCode> {
    let task_type = params.task_type.map(TaskType::from_i64);

    let count = TaskEntry::get_task_history_count(task_type, params.root_id).map_err(|e| {
        error!("Failed to get task history count: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok(Json(TaskHistoryCountResponse { count }))
}

/// GET /api/tasks/history/fetch?task_type=0&root_id=1&limit=25&offset=0
/// Returns paginated task history with root, schedule, and scan-specific information
/// Optionally filtered by task_type and/or root_id
pub async fn get_task_history_fetch(
    Query(params): Query<TaskHistoryFetchParams>,
) -> Result<Json<TaskHistoryFetchResponse>, StatusCode> {
    let task_type = params.task_type.map(TaskType::from_i64);

    let rows = TaskEntry::get_task_history(task_type, params.root_id, params.limit, params.offset)
        .map_err(|e| {
            error!("Failed to get task history: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    let tasks = rows
        .into_iter()
        .map(|row| TaskHistoryResponseRow {
            task_id: row.task_id,
            task_type: row.task_type,
            root_id: row.root_id,
            root_path: row.root_path,
            schedule_name: row.schedule_name,
            source: row.source,
            status: row.status,
            started_at: row.started_at,
            completed_at: row.completed_at,
            scan_id: row.scan_id,
            add_count: row.add_count,
            modify_count: row.modify_count,
            delete_count: row.delete_count,
            was_restarted: row.was_restarted,
        })
        .collect();

    Ok(Json(TaskHistoryFetchResponse { tasks }))
}
