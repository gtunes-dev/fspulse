use crate::database::Database;
use crate::task_manager::TaskManager;
use crate::scans::{HashMode, ValidateMode};
use crate::schedules::{
    CreateScheduleParams, IntervalUnit, Schedule, ScheduleType, ScheduleWithRoot,
};
use axum::{extract::Path, http::StatusCode, Json};
use serde::Deserialize;
use serde_json::{json, Value};

/// Request body for creating a schedule
#[derive(Debug, Deserialize)]
pub struct CreateScheduleRequest {
    pub root_id: i64,
    pub schedule_name: String,
    pub schedule_type: ScheduleType,
    pub time_of_day: Option<String>,
    pub days_of_week: Option<String>,
    pub day_of_month: Option<i64>,
    pub interval_value: Option<i64>,
    pub interval_unit: Option<IntervalUnit>,
    pub hash_mode: HashMode,
    pub validate_mode: ValidateMode,
}

/// Request body for updating a schedule
#[derive(Debug, Deserialize)]
pub struct UpdateScheduleRequest {
    pub schedule_name: String,
    pub schedule_type: ScheduleType,
    pub time_of_day: Option<String>,
    pub days_of_week: Option<String>,
    pub day_of_month: Option<i64>,
    pub interval_value: Option<i64>,
    pub interval_unit: Option<IntervalUnit>,
    pub hash_mode: HashMode,
    pub validate_mode: ValidateMode,
}

/// Request body for toggling schedule enabled status
#[derive(Debug, Deserialize)]
pub struct ToggleScheduleRequest {
    pub enabled: bool,
}

/// POST /api/schedules
/// Create a new schedule
pub async fn create_schedule(
    Json(request): Json<CreateScheduleRequest>,
) -> Result<Json<Schedule>, StatusCode> {
    let conn = Database::get_connection().map_err(|e| {
        log::error!("Database error: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let schedule = TaskManager::create_schedule(
        &conn,
        CreateScheduleParams {
            root_id: request.root_id,
            schedule_name: request.schedule_name,
            schedule_type: request.schedule_type,
            time_of_day: request.time_of_day,
            days_of_week: request.days_of_week,
            day_of_month: request.day_of_month,
            interval_value: request.interval_value,
            interval_unit: request.interval_unit,
            hash_mode: request.hash_mode,
            validate_mode: request.validate_mode,
        },
    )
    .map_err(|e| {
        log::error!("Failed to create schedule: {}", e);
        StatusCode::BAD_REQUEST
    })?;

    Ok(Json(schedule))
}

/// PUT /api/schedules/:id
/// Update an existing schedule
pub async fn update_schedule(
    Path(schedule_id): Path<i64>,
    Json(request): Json<UpdateScheduleRequest>,
) -> Result<StatusCode, StatusCode> {
    let conn = Database::get_connection().map_err(|e| {
        log::error!("Database error: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    // Get existing schedule to preserve fields
    let existing = Schedule::get_by_id(&conn, schedule_id)
        .map_err(|e| {
            log::error!("Failed to get schedule: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or_else(|| {
            log::error!("Schedule {} not found", schedule_id);
            StatusCode::NOT_FOUND
        })?;

    // Build updated schedule (preserve id, root_id, enabled, timestamps)
    let now = chrono::Utc::now().timestamp();
    let updated = Schedule {
        schedule_id,
        root_id: existing.root_id, // Can't change root
        enabled: existing.enabled, // Use PATCH for enable/disable
        schedule_name: request.schedule_name,
        schedule_type: request.schedule_type,
        time_of_day: request.time_of_day,
        days_of_week: request.days_of_week,
        day_of_month: request.day_of_month,
        interval_value: request.interval_value,
        interval_unit: request.interval_unit,
        hash_mode: request.hash_mode,
        validate_mode: request.validate_mode,
        created_at: existing.created_at, // Preserve
        updated_at: now,
    };

    TaskManager::update_schedule(&conn, &updated).map_err(|e| {
        log::error!("Failed to update schedule: {}", e);
        StatusCode::BAD_REQUEST
    })?;

    Ok(StatusCode::OK)
}

/// DELETE /api/schedules/:id
/// Delete a schedule
pub async fn delete_schedule(Path(schedule_id): Path<i64>) -> Result<StatusCode, StatusCode> {
    let conn = Database::get_connection().map_err(|e| {
        log::error!("Database error: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    TaskManager::delete_schedule(&conn, schedule_id).map_err(|e| {
        log::error!("Failed to delete schedule: {}", e);
        StatusCode::BAD_REQUEST
    })?;

    Ok(StatusCode::OK)
}

/// PATCH /api/schedules/:id/toggle
/// Toggle schedule enabled status
pub async fn toggle_schedule(
    Path(schedule_id): Path<i64>,
    Json(request): Json<ToggleScheduleRequest>,
) -> Result<StatusCode, StatusCode> {
    // Use set_schedule_enabled which properly handles queue management
    TaskManager::set_schedule_enabled(schedule_id, request.enabled).map_err(|e| {
        log::error!("Failed to toggle schedule: {}", e);
        StatusCode::BAD_REQUEST
    })?;

    Ok(StatusCode::OK)
}

/// GET /api/schedules/upcoming
/// Get upcoming scans for display in Scans page
/// Returns list of upcoming scans (excludes currently running scan unless paused)
pub async fn get_upcoming_scans() -> Result<Json<Value>, StatusCode> {
    use crate::task_manager::TaskManager;

    // Get next 10 upcoming scans via TaskManager (synchronized with pause state)
    let scans = TaskManager::get_upcoming_scans(10).map_err(|e| {
        log::error!("Error fetching upcoming scans: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok(Json(json!({ "upcoming_scans": scans })))
}

/// GET /api/schedules
/// List all schedules with their root information and next scan time
pub async fn list_schedules() -> Result<Json<Vec<ScheduleWithRoot>>, StatusCode> {
    let schedules = crate::schedules::list_schedules().map_err(|e| {
        log::error!("Failed to list schedules: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok(Json(schedules))
}
