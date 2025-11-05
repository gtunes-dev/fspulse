use axum::{
    http::StatusCode,
    Json,
};
use serde_json::{json, Value};
use crate::database::Database;
use crate::schedules::QueueEntry;

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
