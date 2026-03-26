use axum::{extract::Query, extract::Path, http::StatusCode, Json};
use log::error;
use serde::{Deserialize, Serialize};

use crate::integrity::integrity_api::{self, IntegrityFilter};

// ---------------------------------------------------------------------------
// Shared helper to build IntegrityFilter from query params
// ---------------------------------------------------------------------------

fn parse_filter(
    root_id: i64,
    issue_type: Option<String>,
    extensions: Option<String>,
    status: Option<String>,
    path_search: Option<String>,
    show_deleted: Option<bool>,
) -> IntegrityFilter {
    let exts = extensions
        .as_deref()
        .unwrap_or("")
        .split(',')
        .filter(|s| !s.is_empty())
        .map(|s| s.trim().to_ascii_lowercase())
        .collect();

    IntegrityFilter {
        root_id,
        issue_type,
        extensions: exts,
        status: status.unwrap_or_else(|| "unreviewed".to_string()),
        path_search,
        show_deleted: show_deleted.unwrap_or(false),
    }
}

// ---------------------------------------------------------------------------
// GET /api/integrity/count
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct CountParams {
    pub root_id: i64,
    pub issue_type: Option<String>,
    pub extensions: Option<String>,
    pub status: Option<String>,
    pub path_search: Option<String>,
    pub show_deleted: Option<bool>,
}

#[derive(Debug, Serialize)]
pub struct CountResponse {
    pub total: i64,
}

pub async fn count(
    Query(p): Query<CountParams>,
) -> Result<Json<CountResponse>, (StatusCode, String)> {
    let filter = parse_filter(p.root_id, p.issue_type, p.extensions, p.status, p.path_search, p.show_deleted);
    match integrity_api::count_items(&filter) {
        Ok(total) => Ok(Json(CountResponse { total })),
        Err(e) => {
            error!("integrity count failed: {}", e);
            Err((StatusCode::INTERNAL_SERVER_ERROR, format!("Failed: {}", e)))
        }
    }
}

// ---------------------------------------------------------------------------
// GET /api/integrity/items
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct ItemsParams {
    pub root_id: i64,
    pub issue_type: Option<String>,
    pub extensions: Option<String>,
    pub status: Option<String>,
    pub path_search: Option<String>,
    pub show_deleted: Option<bool>,
    pub offset: Option<i64>,
    pub limit: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct ItemSummaryResponse {
    pub item_id: i64,
    pub item_path: String,
    pub item_name: String,
    pub file_extension: Option<String>,
    pub do_not_validate: bool,
    pub hash_unreviewed: i64,
    pub hash_reviewed: i64,
    pub val_unreviewed: i64,
    pub val_reviewed: i64,
    pub latest_scan_id: i64,
}

pub async fn get_items(
    Query(p): Query<ItemsParams>,
) -> Result<Json<Vec<ItemSummaryResponse>>, (StatusCode, String)> {
    let filter = parse_filter(p.root_id, p.issue_type, p.extensions, p.status, p.path_search, p.show_deleted);
    let offset = p.offset.unwrap_or(0).max(0);
    let limit = p.limit.unwrap_or(50).clamp(1, 200);

    match integrity_api::query_items(&filter, offset, limit) {
        Ok(items) => {
            let rows: Vec<ItemSummaryResponse> = items
                .into_iter()
                .map(|i| ItemSummaryResponse {
                    item_id: i.item_id,
                    item_path: i.item_path,
                    item_name: i.item_name,
                    file_extension: i.file_extension,
                    do_not_validate: i.do_not_validate,
                    hash_unreviewed: i.hash_unreviewed,
                    hash_reviewed: i.hash_reviewed,
                    val_unreviewed: i.val_unreviewed,
                    val_reviewed: i.val_reviewed,
                    latest_scan_id: i.latest_scan_id,
                })
                .collect();
            Ok(Json(rows))
        }
        Err(e) => {
            error!("integrity items query failed: {}", e);
            Err((StatusCode::INTERNAL_SERVER_ERROR, format!("Failed: {}", e)))
        }
    }
}

// ---------------------------------------------------------------------------
// GET /api/integrity/items/:item_id/versions
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct VersionsParams {
    pub root_id: i64,
    pub issue_type: Option<String>,
    pub extensions: Option<String>,
    pub status: Option<String>,
    pub path_search: Option<String>,
    pub show_deleted: Option<bool>,
    pub limit: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct VersionResponse {
    pub item_version: i64,
    pub scan_id: i64,
    pub scan_started_at: i64,
    pub hash_version_count: i64,
    pub hash_suspicious_count: i64,
    pub val_state: Option<i64>,
    pub val_error: Option<String>,
    pub val_reviewed_at: Option<i64>,
    pub hash_reviewed_at: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct VersionsListResponse {
    pub versions: Vec<VersionResponse>,
    pub total: i64,
}

pub async fn get_versions(
    Path(item_id): Path<i64>,
    Query(p): Query<VersionsParams>,
) -> Result<Json<VersionsListResponse>, (StatusCode, String)> {
    let filter = parse_filter(p.root_id, p.issue_type, p.extensions, p.status, p.path_search, p.show_deleted);
    let limit = p.limit.unwrap_or(5).clamp(1, 100);

    match integrity_api::query_versions(&filter, item_id, limit) {
        Ok(result) => {
            let versions: Vec<VersionResponse> = result
                .versions
                .into_iter()
                .map(|v| VersionResponse {
                    item_version: v.item_version,
                    scan_id: v.scan_id,
                    scan_started_at: v.scan_started_at,
                    hash_version_count: v.hash_version_count,
                    hash_suspicious_count: v.hash_suspicious_count,
                    val_state: v.val_state,
                    val_error: v.val_error,
                    val_reviewed_at: v.val_reviewed_at,
                    hash_reviewed_at: v.hash_reviewed_at,
                })
                .collect();
            Ok(Json(VersionsListResponse {
                versions,
                total: result.total,
            }))
        }
        Err(e) => {
            error!("integrity versions query for item {} failed: {}", item_id, e);
            Err((StatusCode::INTERNAL_SERVER_ERROR, format!("Failed: {}", e)))
        }
    }
}

// ---------------------------------------------------------------------------
// POST /api/integrity/review
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct ReviewRequest {
    pub item_id: i64,
    /// None = all versions of this item; Some(v) = specific version
    pub item_version: Option<i64>,
    pub set_val: Option<bool>,
    pub set_hash: Option<bool>,
}

pub async fn review(
    Json(req): Json<ReviewRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    if req.set_val.is_none() && req.set_hash.is_none() {
        return Err((
            StatusCode::BAD_REQUEST,
            "At least one of set_val or set_hash must be provided".to_string(),
        ));
    }

    match integrity_api::set_reviewed(req.item_id, req.item_version, req.set_val, req.set_hash) {
        Ok(()) => Ok(Json(serde_json::json!({ "success": true }))),
        Err(e) => {
            error!("review failed for item {}: {}", req.item_id, e);
            Err((StatusCode::INTERNAL_SERVER_ERROR, format!("Failed: {}", e)))
        }
    }
}

// ---------------------------------------------------------------------------
// POST /api/integrity/do-not-validate
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct DoNotValidateRequest {
    pub item_id: i64,
    pub do_not_validate: bool,
}

pub async fn set_do_not_validate(
    Json(req): Json<DoNotValidateRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    match integrity_api::set_do_not_validate(req.item_id, req.do_not_validate) {
        Ok(()) => Ok(Json(serde_json::json!({ "success": true }))),
        Err(e) => {
            error!("do_not_validate failed for item {}: {}", req.item_id, e);
            Err((StatusCode::INTERNAL_SERVER_ERROR, format!("Failed: {}", e)))
        }
    }
}
