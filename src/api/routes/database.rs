use axum::{http::StatusCode, Json};
use log::error;

use crate::database::{Database, DbStats};
use crate::scan_manager::ScanManager;

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

/// POST /api/database/compact
///
/// Compacts the database using VACUUM
/// Returns error if a scan is currently running
/// May take several minutes for large databases
pub async fn compact_database() -> Result<StatusCode, (StatusCode, String)> {
    // Call ScanManager to coordinate compaction
    ScanManager::compact_db().map_err(|e| {
        error!("Database compaction failed: {}", e);
        (StatusCode::CONFLICT, e)
    })?;

    Ok(StatusCode::OK)
}
