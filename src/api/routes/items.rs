use axum::{
    extract::{Path, Query},
    http::StatusCode,
    Json,
};
use log::error;
use serde::{Deserialize, Serialize};

use crate::items::{self, Item, SizeHistoryPoint};

/// Query parameters for date range filtering
#[derive(Debug, Deserialize)]
pub struct DateRangeParams {
    pub from_date: String, // Date string in format "yyyy-MM-dd"
    pub to_date: String,   // Date string in format "yyyy-MM-dd"
}

/// Response structure for size history
#[derive(Debug, Serialize)]
pub struct SizeHistoryResponse {
    pub history: Vec<SizeHistoryPoint>,
}

/// Response structure for children counts (old model)
#[derive(Debug, Serialize)]
pub struct OldChildrenCountsResponse {
    pub file_count: i64,
    pub directory_count: i64,
}

/// Query parameters for getting immediate children (old model)
#[derive(Debug, Deserialize)]
pub struct OldImmediateChildrenParams {
    pub root_id: i64,
    pub parent_path: String,
}

/// Item data for API response (old model)
#[derive(Debug, Serialize)]
pub struct OldItemResponse {
    pub item_id: i64,
    pub item_path: String,
    pub item_type: String,
    pub is_ts: bool,
}

/// GET /api/items/:item_id/size-history?from_date=YYYY-MM-DD&to_date=YYYY-MM-DD
/// Returns size history for an item within a date range based on scan times
pub async fn get_item_size_history(
    Path(item_id): Path<i64>,
    Query(params): Query<DateRangeParams>,
) -> Result<Json<SizeHistoryResponse>, (StatusCode, String)> {
    match Item::get_size_history(item_id, &params.from_date, &params.to_date) {
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

/// GET /api/old_items/:item_id/children-counts
/// Returns counts of files and directories that are children of the given directory
pub async fn old_get_children_counts(
    Path(item_id): Path<i64>,
) -> Result<Json<OldChildrenCountsResponse>, (StatusCode, String)> {
    match Item::old_get_children_counts(item_id) {
        Ok(counts) => Ok(Json(OldChildrenCountsResponse {
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

/// GET /api/old_items/immediate-children?root_id=X&parent_path=/path
/// Returns immediate children (one level deep) of the specified directory path
/// Always includes tombstones - filtering should be done client-side
pub async fn old_get_immediate_children(
    Query(params): Query<OldImmediateChildrenParams>,
) -> Result<Json<Vec<OldItemResponse>>, (StatusCode, String)> {
    match Item::old_get_immediate_children(params.root_id, &params.parent_path) {
        Ok(items) => {
            let response: Vec<OldItemResponse> = items
                .iter()
                .map(|item| OldItemResponse {
                    item_id: item.item_id(),
                    item_path: item.item_path().to_string(),
                    item_type: item.item_type().short_name().to_string(),
                    is_ts: item.is_ts(),
                })
                .collect();
            Ok(Json(response))
        }
        Err(e) => {
            error!(
                "Failed to get immediate children for root_id={}, parent_path={}: {}",
                params.root_id, params.parent_path, e
            );
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to get immediate children: {}", e),
            ))
        }
    }
}

// ---- New temporal model endpoints ----

/// Query parameters for temporal immediate children
#[derive(Debug, Deserialize)]
pub struct ImmediateChildrenParams {
    pub root_id: i64,
    pub parent_path: String,
    pub scan_id: i64,
}

/// Item data for temporal API response
#[derive(Debug, Serialize)]
pub struct ItemResponse {
    pub item_id: i64,
    pub item_path: String,
    pub item_name: String,
    pub item_type: String,
    pub is_deleted: bool,
}

/// GET /api/items/immediate-children?root_id=X&parent_path=/path&scan_id=Y
/// Returns immediate children at a point in time using the item_versions table.
/// Always includes deleted items - filtering should be done client-side.
pub async fn get_immediate_children(
    Query(params): Query<ImmediateChildrenParams>,
) -> Result<Json<Vec<ItemResponse>>, (StatusCode, String)> {
    match items::get_temporal_immediate_children(params.root_id, &params.parent_path, params.scan_id)
    {
        Ok(items) => {
            let response: Vec<ItemResponse> = items
                .iter()
                .map(|item| ItemResponse {
                    item_id: item.item_id,
                    item_path: item.item_path.clone(),
                    item_name: item.item_name.clone(),
                    item_type: item.item_type.short_name().to_string(),
                    is_deleted: item.is_deleted,
                })
                .collect();
            Ok(Json(response))
        }
        Err(e) => {
            error!(
                "Failed to get temporal immediate children for root_id={}, parent_path={}, scan_id={}: {}",
                params.root_id, params.parent_path, params.scan_id, e
            );
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to get immediate children: {}", e),
            ))
        }
    }
}

/// Query parameters for temporal item search
#[derive(Debug, Deserialize)]
pub struct SearchParams {
    pub root_id: i64,
    pub scan_id: i64,
    pub query: String,
}

/// GET /api/items/search?root_id=X&scan_id=Y&query=term
/// Searches for items by name (last path segment) at a point in time.
/// Always includes deleted items - filtering should be done client-side.
pub async fn search_items(
    Query(params): Query<SearchParams>,
) -> Result<Json<Vec<ItemResponse>>, (StatusCode, String)> {
    match items::search_temporal_items(params.root_id, params.scan_id, &params.query) {
        Ok(items) => {
            let response: Vec<ItemResponse> = items
                .iter()
                .map(|item| ItemResponse {
                    item_id: item.item_id,
                    item_path: item.item_path.clone(),
                    item_name: item.item_name.clone(),
                    item_type: item.item_type.short_name().to_string(),
                    is_deleted: item.is_deleted,
                })
                .collect();
            Ok(Json(response))
        }
        Err(e) => {
            error!(
                "Failed to search items for root_id={}, scan_id={}, query='{}': {}",
                params.root_id, params.scan_id, params.query, e
            );
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to search items: {}", e),
            ))
        }
    }
}
