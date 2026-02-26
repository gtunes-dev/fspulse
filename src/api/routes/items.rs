use axum::{
    extract::{Path, Query},
    http::StatusCode,
    Json,
};
use log::error;
use serde::{Deserialize, Serialize};

use crate::items::{self, SizeHistoryPoint};

/// Query parameters for size history (temporal model)
#[derive(Debug, Deserialize)]
pub struct SizeHistoryParams {
    pub from_date: String,
    pub to_scan_id: i64,
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

/// Query parameters for children counts
#[derive(Debug, Deserialize)]
pub struct ChildrenCountsParams {
    pub scan_id: i64,
}

/// GET /api/items/:item_id/size-history?from_date=YYYY-MM-DD&to_scan_id=42
/// Returns size history for an item from a date up to a specific scan
pub async fn get_item_size_history(
    Path(item_id): Path<i64>,
    Query(params): Query<SizeHistoryParams>,
) -> Result<Json<SizeHistoryResponse>, (StatusCode, String)> {
    match items::get_size_history(item_id, &params.from_date, params.to_scan_id) {
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

/// GET /api/items/:item_id/children-counts?scan_id=42
/// Returns counts of files and directories that are children of the given directory
/// at the specified scan point in time
pub async fn get_children_counts(
    Path(item_id): Path<i64>,
    Query(params): Query<ChildrenCountsParams>,
) -> Result<Json<ChildrenCountsResponse>, (StatusCode, String)> {
    match items::get_children_counts(item_id, params.scan_id) {
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
    pub first_scan_id: i64,
    pub is_added: bool,
    pub is_deleted: bool,
    pub mod_date: Option<i64>,
    pub size: Option<i64>,
    pub add_count: Option<i64>,
    pub modify_count: Option<i64>,
    pub delete_count: Option<i64>,
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
                    first_scan_id: item.first_scan_id,
                    is_added: item.is_added,
                    is_deleted: item.is_deleted,
                    mod_date: item.mod_date,
                    size: item.size,
                    add_count: item.add_count,
                    modify_count: item.modify_count,
                    delete_count: item.delete_count,
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
                    first_scan_id: item.first_scan_id,
                    is_added: item.is_added,
                    is_deleted: item.is_deleted,
                    mod_date: item.mod_date,
                    size: item.size,
                    add_count: item.add_count,
                    modify_count: item.modify_count,
                    delete_count: item.delete_count,
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

/// Query parameters for version history
#[derive(Debug, Deserialize)]
pub struct VersionHistoryParams {
    /// For initial load: the scan to anchor around
    pub scan_id: Option<i64>,
    /// For pagination: load versions older than this scan
    pub before_scan_id: Option<i64>,
    /// Maximum versions to return (default 500)
    pub limit: Option<i64>,
}

/// GET /api/items/:item_id/version-history?scan_id=42&limit=500
/// GET /api/items/:item_id/version-history?before_scan_id=10&limit=100
/// Returns version history for an item, either anchored at a scan or paginated
pub async fn get_version_history(
    Path(item_id): Path<i64>,
    Query(params): Query<VersionHistoryParams>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let limit = params.limit.unwrap_or(500).min(500);

    if let Some(scan_id) = params.scan_id {
        // Initial load: anchored at a specific scan
        match items::get_version_history_init(item_id, scan_id, limit) {
            Ok(response) => Ok(Json(serde_json::to_value(response).unwrap())),
            Err(e) => {
                error!(
                    "Failed to get version history for item {}, scan {}: {}",
                    item_id, scan_id, e
                );
                Err((
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Failed to get version history: {}", e),
                ))
            }
        }
    } else if let Some(before_scan_id) = params.before_scan_id {
        // Pagination: load older versions
        match items::get_version_history_page(item_id, before_scan_id, limit) {
            Ok(response) => Ok(Json(serde_json::to_value(response).unwrap())),
            Err(e) => {
                error!(
                    "Failed to get version history page for item {}: {}",
                    item_id, e
                );
                Err((
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Failed to get version history: {}", e),
                ))
            }
        }
    } else {
        Err((
            StatusCode::BAD_REQUEST,
            "Either scan_id or before_scan_id must be provided".to_string(),
        ))
    }
}
