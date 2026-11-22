use axum::{
    extract::{Path, Query},
    http::StatusCode,
    Json,
};
use log::error;
use serde::{Deserialize, Serialize};

use crate::db::Database;
use crate::items::{self, HashHistoryEntry};
use crate::timeline::SizePoint;
use crate::timeline::TimeRange;
use crate::utils::Utils;

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
    pub points: Vec<SizePoint>,
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
    /// to the hour (see `crate::timeline`).
    pub at: Option<i64>,
}

/// Query parameters for integrity state
#[derive(Debug, Deserialize)]
pub struct IntegrityStateParams {
    /// See `ChildrenCountsParams::at`.
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
/// Returns the hash and validation state of an item at a time
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

/// GET /api/items/:item_id/size-history
///
/// The item's size over everything recorded about it, oldest first: see
/// `timeline::size_history`. A folder's size is the total of the files
/// beneath it.
pub async fn get_item_size_history(
    Path(item_id): Path<i64>,
) -> Result<Json<SizeHistoryResponse>, (StatusCode, String)> {
    let conn = Database::get_connection()
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed: {}", e)))?;
    match crate::timeline::size_history(&conn, item_id) {
        Ok(points) => Ok(Json(SizeHistoryResponse { points })),
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
/// Returns counts of the files and directories beneath the given directory, at any
/// depth, at a time
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

/// GET /api/items/:item_id/version-at?at=1700000000
/// Returns the version in effect at the given time and whether the item
/// was present then. 404 if nothing had been recorded about it yet.
pub async fn get_version_at(
    Path(item_id): Path<i64>,
    Query(params): Query<VersionAtParams>,
) -> Result<Json<items::VersionAt>, (StatusCode, String)> {
    match items::get_version_at(item_id, as_of(params.at)) {
        Ok(Some(version)) => Ok(Json(version)),
        Ok(None) => Err((StatusCode::NOT_FOUND, "No version found at this time".to_string())),
        Err(e) => {
            error!("Failed to get the version at a time for item {}: {}", item_id, e);
            Err((StatusCode::INTERNAL_SERVER_ERROR, format!("Failed: {}", e)))
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct VersionAtParams {
    /// See `ChildrenCountsParams::at`.
    pub at: Option<i64>,
}

/// GET /api/items/:item_id/version-count
///
/// The number of versions and a stamp that changes whenever anything
/// recorded about the item does: see `items::count_versions`.
pub async fn get_version_count(
    Path(item_id): Path<i64>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    match items::count_versions(item_id) {
        Ok((total, stamp)) => Ok(Json(serde_json::json!({ "total": total, "stamp": stamp }))),
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
