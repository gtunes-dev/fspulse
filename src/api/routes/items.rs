use axum::{
    extract::{Path, Query},
    http::StatusCode,
    Json,
};
use log::error;
use serde::{Deserialize, Serialize};

use crate::database::Database;
use crate::items::{Item, SizeHistoryPoint};

/// Query parameters for date range filtering
#[derive(Debug, Deserialize)]
pub struct DateRangeParams {
    pub from_date: String,  // Date string in format "yyyy-MM-dd"
    pub to_date: String,    // Date string in format "yyyy-MM-dd"
}

/// Response structure for size history
#[derive(Debug, Serialize)]
pub struct SizeHistoryResponse {
    pub history: Vec<SizeHistoryPoint>,
}

/// Response structure for children counts
#[derive(Debug, Serialize)]
pub struct ChildrenCountsResponse {
    pub file_count: i64,
    pub directory_count: i64,
}

/// GET /api/items/:item_id/size-history?from_date=YYYY-MM-DD&to_date=YYYY-MM-DD
/// Returns size history for an item within a date range based on scan times
pub async fn get_item_size_history(
    Path(item_id): Path<i64>,
    Query(params): Query<DateRangeParams>,
) -> Result<Json<SizeHistoryResponse>, (StatusCode, String)> {
    let db = Database::new().map_err(|e| {
        error!("Failed to open database: {}", e);
        (StatusCode::INTERNAL_SERVER_ERROR, format!("Database error: {}", e))
    })?;

    match Item::get_size_history(&db, item_id, &params.from_date, &params.to_date) {
        Ok(history) => Ok(Json(SizeHistoryResponse { history })),
        Err(e) => {
            error!("Failed to get size history for item {}: {}", item_id, e);
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to get size history: {}", e),
            ))
        }
    }
}

/// GET /api/items/:item_id/children-counts
/// Returns counts of files and directories that are children of the given directory
pub async fn get_children_counts(
    Path(item_id): Path<i64>,
) -> Result<Json<ChildrenCountsResponse>, (StatusCode, String)> {
    let db = Database::new().map_err(|e| {
        error!("Failed to open database: {}", e);
        (StatusCode::INTERNAL_SERVER_ERROR, format!("Database error: {}", e))
    })?;

    match Item::get_children_counts(&db, item_id) {
        Ok(counts) => Ok(Json(ChildrenCountsResponse {
            file_count: counts.file_count,
            directory_count: counts.directory_count,
        })),
        Err(e) => {
            error!("Failed to get children counts for item {}: {}", item_id, e);
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to get children counts: {}", e),
            ))
        }
    }
}
