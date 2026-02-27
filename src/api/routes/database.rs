use axum::{http::StatusCode, Json};
use log::error;

use crate::db::{Database, DbStats};

/// GET /api/database/stats
///
/// Returns database statistics including path, size, and wasted space
pub async fn get_database_stats() -> Result<Json<DbStats>, StatusCode> {
    let stats = Database::get_stats().map_err(|e| {
        error!("Failed to get database stats: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok(Json(stats))
}
