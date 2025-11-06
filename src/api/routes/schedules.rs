use axum::{
    extract::Path,
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use crate::database::Database;
use crate::scan_manager::ScanManager;
use crate::schedules::{CreateScheduleParams, QueueEntry, Schedule, ScheduleType, IntervalUnit};
use crate::scans::{HashMode, ValidateMode};

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
    let db = Database::new()
        .map_err(|e| {
            log::error!("Database error: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    let schedule = ScanManager::create_schedule(
        &db,
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
    let db = Database::new()
        .map_err(|e| {
            log::error!("Database error: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    // Get existing schedule to preserve fields
    let existing = Schedule::get_by_id(db.conn(), schedule_id)
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
        root_id: existing.root_id,  // Can't change root
        enabled: existing.enabled,   // Use PATCH for enable/disable
        schedule_name: request.schedule_name,
        schedule_type: request.schedule_type,
        time_of_day: request.time_of_day,
        days_of_week: request.days_of_week,
        day_of_month: request.day_of_month,
        interval_value: request.interval_value,
        interval_unit: request.interval_unit,
        hash_mode: request.hash_mode,
        validate_mode: request.validate_mode,
        created_at: existing.created_at,  // Preserve
        updated_at: now,
    };

    ScanManager::update_schedule(&db, &updated)
        .map_err(|e| {
            log::error!("Failed to update schedule: {}", e);
            StatusCode::BAD_REQUEST
        })?;

    Ok(StatusCode::OK)
}

/// DELETE /api/schedules/:id
/// Delete a schedule
pub async fn delete_schedule(
    Path(schedule_id): Path<i64>,
) -> Result<StatusCode, StatusCode> {
    let db = Database::new()
        .map_err(|e| {
            log::error!("Database error: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    ScanManager::delete_schedule(&db, schedule_id)
        .map_err(|e| {
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
    let db = Database::new()
        .map_err(|e| {
            log::error!("Database error: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    // Use set_schedule_enabled which properly handles queue management
    ScanManager::set_schedule_enabled(&db, schedule_id, request.enabled)
        .map_err(|e| {
            log::error!("Failed to toggle schedule: {}", e);
            StatusCode::BAD_REQUEST
        })?;

    Ok(StatusCode::OK)
}

/// GET /api/schedules/upcoming
/// Get upcoming scans for display in Activity page
/// Returns list of upcoming scans (excludes currently running scan)
pub async fn get_upcoming_scans() -> Result<Json<Value>, StatusCode> {
    let db = Database::new()
        .map_err(|e| {
            log::error!("Database error: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    // Get next 10 upcoming scans
    let scans = QueueEntry::get_upcoming_scans(&db, 10)
        .map_err(|e| {
            log::error!("Error fetching upcoming scans: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    Ok(Json(json!({ "upcoming_scans": scans })))
}

/// Response body for schedule with root information
#[derive(Debug, Serialize)]
pub struct ScheduleWithRoot {
    #[serde(flatten)]
    pub schedule: Schedule,
    pub root_path: String,
    pub next_scan_time: Option<i64>,
}

/// GET /api/schedules
/// List all schedules with their root information and next scan time
pub async fn list_schedules() -> Result<Json<Vec<ScheduleWithRoot>>, StatusCode> {
    let db = Database::new()
        .map_err(|e| {
            log::error!("Database error: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    let conn = db.conn();

    // Query all schedules with root information and next_scan_time from queue
    let mut stmt = conn.prepare(
        "SELECT
            s.schedule_id, s.root_id, s.enabled, s.schedule_name, s.schedule_type,
            s.time_of_day, s.days_of_week, s.day_of_month,
            s.interval_value, s.interval_unit,
            s.hash_mode, s.validate_mode,
            s.created_at, s.updated_at,
            r.root_path,
            q.next_scan_time
        FROM scan_schedules s
        INNER JOIN roots r ON s.root_id = r.root_id
        LEFT JOIN scan_queue q ON s.schedule_id = q.schedule_id
        ORDER BY s.schedule_name COLLATE NOCASE ASC"
    ).map_err(|e| {
        log::error!("Failed to prepare query: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let schedules = stmt.query_map([], |row| {
        Ok(ScheduleWithRoot {
            schedule: Schedule {
                schedule_id: row.get(0)?,
                root_id: row.get(1)?,
                enabled: row.get(2)?,
                schedule_name: row.get(3)?,
                schedule_type: ScheduleType::from_i32(row.get(4)?)
                    .ok_or_else(|| rusqlite::Error::InvalidColumnType(4, "schedule_type".to_string(), rusqlite::types::Type::Integer))?,
                time_of_day: row.get(5)?,
                days_of_week: row.get(6)?,
                day_of_month: row.get(7)?,
                interval_value: row.get(8)?,
                interval_unit: row.get::<_, Option<i32>>(9)?
                    .map(|v| IntervalUnit::from_i32(v).ok_or_else(||
                        rusqlite::Error::InvalidColumnType(9, "interval_unit".to_string(), rusqlite::types::Type::Integer)
                    ))
                    .transpose()?,
                hash_mode: HashMode::from_i32(row.get(10)?)
                    .ok_or_else(|| rusqlite::Error::InvalidColumnType(10, "hash_mode".to_string(), rusqlite::types::Type::Integer))?,
                validate_mode: ValidateMode::from_i32(row.get(11)?)
                    .ok_or_else(|| rusqlite::Error::InvalidColumnType(11, "validate_mode".to_string(), rusqlite::types::Type::Integer))?,
                created_at: row.get(12)?,
                updated_at: row.get(13)?,
            },
            root_path: row.get(14)?,
            next_scan_time: row.get(15)?,
        })
    }).map_err(|e| {
        log::error!("Failed to execute query: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let results: Result<Vec<_>, _> = schedules.collect();
    let schedules_vec = results.map_err(|e| {
        log::error!("Failed to collect schedules: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok(Json(schedules_vec))
}
