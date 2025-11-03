use axum::{
    extract::Path,
    http::StatusCode,
    response::Json,
};
use log::error;
use serde::Serialize;
use std::path::MAIN_SEPARATOR;

use crate::database::Database;
use crate::items::Item;

/// Response structure for folder size calculation
#[derive(Debug, Serialize)]
pub struct FolderSizeResponse {
    pub size: i64,
}

/// Error response structure
#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    pub error: String,
}

/// GET /api/items/{item_id}/folder-size
/// Calculate the total size of all files within a directory item
pub async fn get_folder_size(
    Path(item_id): Path<i64>,
) -> Result<(StatusCode, Json<FolderSizeResponse>), (StatusCode, Json<ErrorResponse>)> {
    // Open database connection
    let db = Database::new().map_err(|e| {
        error!("Failed to open database: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: "Database connection error".to_string(),
            }),
        )
    })?;

    // Get the item to retrieve root_id and path
    let item = Item::get_by_id(&db, item_id)
        .map_err(|e| {
            error!("Failed to query item {}: {}", item_id, e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "Failed to query item".to_string(),
                }),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    error: format!("Item {} not found", item_id),
                }),
            )
        })?;

    // Ensure the path ends with the platform separator
    let folder_path = if item.item_path().ends_with(MAIN_SEPARATOR) {
        item.item_path().to_string()
    } else {
        format!("{}{}", item.item_path(), MAIN_SEPARATOR)
    };

    // Calculate folder size
    let size = Item::calculate_folder_size(&db, item.root_id(), &folder_path)
        .map_err(|e| {
            error!("Failed to calculate folder size for item {}: {}", item_id, e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "Failed to calculate folder size".to_string(),
                }),
            )
        })?;

    Ok((StatusCode::OK, Json(FolderSizeResponse { size })))
}
