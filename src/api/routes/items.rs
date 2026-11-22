use axum::{
    extract::{Path, Query},
    http::StatusCode,
    Json,
};
use log::error;
use serde::{Deserialize, Serialize};

use crate::db::Database;
use crate::items::{self, HashHistoryEntry, SizeHistoryPoint};
use crate::timeline::TimeRange;
use crate::utils::Utils;

/// Query parameters for size history (temporal model)
#[derive(Debug, Deserialize)]
pub struct SizeHistoryParams {
    /// Unix seconds. Leave out for all history.
    pub from: Option<i64>,
    /// Unix seconds. Leave out for now.
    pub to: Option<i64>,
}

/// The last second that counts as "at" the requested time, for the
/// `items` functions, which take an inclusive time.
fn as_of(at: Option<i64>) -> i64 {
    TimeRange::resolve(None, at, Utils::now_secs()).last_second()
}

#[derive(Debug, Deserialize)]
pub struct InRangeParams {
    pub from: Option<i64>,
    pub to: Option<i64>,
}

/// GET /api/items/:item_id/in-range?from=..&to=..
///
/// What happened to the item inside a range: see `timeline::ItemInRange`.
/// The range is resolved as the timeline endpoints resolve it and is
/// returned, so the caller can tell which versions fall inside it.
pub async fn get_item_in_range(
    Path(item_id): Path<i64>,
    Query(params): Query<InRangeParams>,
) -> Result<Json<crate::timeline::ItemInRange>, (StatusCode, String)> {
    let range = TimeRange::resolve(params.from, params.to, Utils::now_secs());
    let conn = Database::get_connection()
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed: {}", e)))?;
    crate::timeline::item_in_range(&conn, item_id, range)
        .map(Json)
        .map_err(|e| {
            error!("Failed to read item {} in a range: {}", item_id, e);
            (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed: {}", e))
        })
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
    /// Unix seconds. Leave out for now. A time in the past is moved back
    /// to the hour (see `crate::timeline`). `scan_id` is accepted as an
    /// old name for the same value.
    #[serde(alias = "scan_id")]
    pub at: Option<i64>,
}

/// Query parameters for integrity state
#[derive(Debug, Deserialize)]
pub struct IntegrityStateParams {
    /// See `ChildrenCountsParams::at`.
    #[serde(alias = "scan_id")]
    pub at: Option<i64>,
}

/// Response structure for integrity state
#[derive(Debug, Serialize)]
pub struct IntegrityStateResponse {
    pub has_validator: bool,
    pub do_not_validate: bool,
    pub hash_state: Option<i64>,
    pub file_hash: Option<String>,
    pub val_state: Option<i64>,
    pub val_error: Option<String>,
}

/// GET /api/items/:item_id/integrity-state?at=1700000000
/// Returns the current hash and validation state for an item at a specific scan point
pub async fn get_integrity_state(
    Path(item_id): Path<i64>,
    Query(params): Query<IntegrityStateParams>,
) -> Result<Json<IntegrityStateResponse>, (StatusCode, String)> {
    match items::get_integrity_state(item_id, as_of(params.at)) {
        Ok(state) => Ok(Json(IntegrityStateResponse {
            has_validator: state.has_validator,
            do_not_validate: state.do_not_validate,
            hash_state: state.hash_state,
            file_hash: state.file_hash.map(hex::encode),
            val_state: state.val_state,
            val_error: state.val_error,
        })),
        Err(e) => {
            error!("Failed to get integrity state for item {}: {}", item_id, e);
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to get integrity state: {}", e),
            ))
        }
    }
}

/// GET /api/items/:item_id/size-history?from=1700000000&to=1700090000
/// Returns size history for an item from a date up to a specific scan
pub async fn get_item_size_history(
    Path(item_id): Path<i64>,
    Query(params): Query<SizeHistoryParams>,
) -> Result<Json<SizeHistoryResponse>, (StatusCode, String)> {
    match items::get_size_history(item_id, params.from.unwrap_or(0), as_of(params.to)) {
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

/// GET /api/items/:item_id/children-counts?at=1700000000
/// Returns counts of files and directories that are children of the given directory
/// at the specified scan point in time
pub async fn get_children_counts(
    Path(item_id): Path<i64>,
    Query(params): Query<ChildrenCountsParams>,
) -> Result<Json<ChildrenCountsResponse>, (StatusCode, String)> {
    match items::get_children_counts(item_id, as_of(params.at)) {
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

/// Response structure for hash history
#[derive(Debug, Serialize)]
pub struct HashHistoryResponse {
    pub hashes: Vec<HashHistoryEntry>,
}

/// GET /api/items/:item_id/versions/:item_version/hashes
/// Returns all hash versions for a given item_id and item_version, ordered chronologically
pub async fn get_hash_history(
    Path((item_id, item_version)): Path<(i64, i64)>,
) -> Result<Json<HashHistoryResponse>, (StatusCode, String)> {
    match items::get_hash_history(item_id, item_version) {
        Ok(hashes) => Ok(Json(HashHistoryResponse { hashes })),
        Err(e) => {
            error!(
                "Failed to get hash history for item {} version {}: {}",
                item_id, item_version, e
            );
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to get hash history: {}", e),
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
    pub first_seen_at: i64,
    pub is_added: bool,
    pub is_deleted: bool,
    pub mod_date: Option<i64>,
    pub size: Option<i64>,
    pub add_count: Option<i64>,
    pub modify_count: Option<i64>,
    pub delete_count: Option<i64>,
    pub unchanged_count: Option<i64>,
    pub val_state: Option<i64>,
    pub hash_state: Option<i64>,
}

/// GET /api/items/immediate-children?root_id=X&parent_path=/path&scan_id=Y
/// Returns immediate children at a point in time using the item_versions table.
/// Always includes deleted items - filtering should be done client-side.
pub async fn get_immediate_children(
    Query(params): Query<ImmediateChildrenParams>,
) -> Result<Json<Vec<ItemResponse>>, (StatusCode, String)> {
    match items::get_temporal_immediate_children(params.root_id, &params.parent_path, params.scan_id, None, None)
    {
        Ok(items) => {
            let response: Vec<ItemResponse> = items
                .iter()
                .map(|item| ItemResponse {
                    item_id: item.item_id,
                    item_path: item.item_path.clone(),
                    item_name: item.item_name.clone(),
                    item_type: item.item_type.short_name().to_string(),
                    first_seen_at: item.first_seen_at,
                    is_added: item.is_added,
                    is_deleted: item.is_deleted,
                    mod_date: item.mod_date,
                    size: item.size,
                    add_count: item.add_count,
                    modify_count: item.modify_count,
                    delete_count: item.delete_count,
                    unchanged_count: item.unchanged_count,
                    val_state: item.val_state,
                    hash_state: item.hash_state,
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
    match items::get_temporal_search_items(params.root_id, params.scan_id, &params.query, None, None) {
        Ok(items) => {
            let response: Vec<ItemResponse> = items
                .iter()
                .map(|item| ItemResponse {
                    item_id: item.item_id,
                    item_path: item.item_path.clone(),
                    item_name: item.item_name.clone(),
                    item_type: item.item_type.short_name().to_string(),
                    first_seen_at: item.first_seen_at,
                    is_added: item.is_added,
                    is_deleted: item.is_deleted,
                    mod_date: item.mod_date,
                    size: item.size,
                    add_count: item.add_count,
                    modify_count: item.modify_count,
                    delete_count: item.delete_count,
                    unchanged_count: item.unchanged_count,
                    val_state: item.val_state,
                    hash_state: item.hash_state,
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

/// GET /api/items/:item_id/version-at?at=1700000000
/// Returns the item_version in effect at the given time. Also served at
/// the old path `version-at-scan`.
pub async fn get_version_at_scan(
    Path(item_id): Path<i64>,
    Query(params): Query<VersionAtScanParams>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    match items::get_version_at_scan(item_id, as_of(params.at)) {
        Ok(Some(version)) => Ok(Json(serde_json::json!({ "item_version": version }))),
        Ok(None) => Err((StatusCode::NOT_FOUND, "No version found at this time".to_string())),
        Err(e) => {
            error!("Failed to get the version at a time for item {}: {}", item_id, e);
            Err((StatusCode::INTERNAL_SERVER_ERROR, format!("Failed: {}", e)))
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct VersionAtScanParams {
    /// See `ChildrenCountsParams::at`.
    #[serde(alias = "scan_id")]
    pub at: Option<i64>,
}

/// GET /api/items/:item_id/version-count
pub async fn get_version_count(
    Path(item_id): Path<i64>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    match items::count_versions(item_id) {
        Ok(total) => Ok(Json(serde_json::json!({ "total": total }))),
        Err(e) => {
            error!("Failed to count versions for item {}: {}", item_id, e);
            Err((StatusCode::INTERNAL_SERVER_ERROR, format!("Failed: {}", e)))
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct VersionsParams {
    pub offset: Option<i64>,
    pub limit: Option<i64>,
    /// "asc" or "desc" (by item_version). Default: "desc" (newest first).
    pub order: Option<String>,
}

/// GET /api/items/:item_id/versions?offset=0&limit=10&order=desc
pub async fn get_versions(
    Path(item_id): Path<i64>,
    Query(params): Query<VersionsParams>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let offset = params.offset.unwrap_or(0).max(0);
    let limit = params.limit.unwrap_or(10).clamp(1, 200);
    let order = params.order.as_deref().unwrap_or("desc");

    match items::get_versions(item_id, offset, limit, order) {
        Ok(versions) => Ok(Json(serde_json::json!({ "versions": versions }))),
        Err(e) => {
            error!("Failed to get versions for item {}: {}", item_id, e);
            Err((StatusCode::INTERNAL_SERVER_ERROR, format!("Failed: {}", e)))
        }
    }
}
