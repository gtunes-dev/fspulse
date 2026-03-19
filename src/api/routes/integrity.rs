use axum::{extract::Query, http::StatusCode, Json};
use log::error;
use serde::{Deserialize, Serialize};

use crate::integrity::integrity_api::{
    self, IntegrityQuery,
};

/// Query parameters for GET /api/integrity
#[derive(Debug, Deserialize)]
pub struct IntegrityQueryParams {
    pub root_id: i64,
    /// "val", "hash", or omit for all
    pub issue_type: Option<String>,
    /// Comma-separated lowercase extensions, e.g. "pdf,jpg"
    pub extensions: Option<String>,
    /// "unreviewed" (default), "reviewed", "all"
    pub status: Option<String>,
    /// Substring match on item_path
    pub path_search: Option<String>,
    pub offset: Option<i64>,
    pub limit: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct IntegrityItemResponse {
    pub item_id: i64,
    pub item_path: String,
    pub item_name: String,
    pub file_extension: Option<String>,
    pub do_not_validate: bool,
    pub item_version: i64,
    pub val_state: Option<i64>,
    pub val_reviewed_at: Option<i64>,
    pub hash_state: Option<i64>,
    pub hash_reviewed_at: Option<i64>,
    pub first_scan_id: i64,
    pub first_detected_at: i64,
}

#[derive(Debug, Serialize)]
pub struct IntegrityListResponse {
    pub items: Vec<IntegrityItemResponse>,
    pub total: i64,
    pub offset: i64,
    pub limit: i64,
}

/// GET /api/integrity
/// Returns integrity issues for a root, with filtering and pagination.
pub async fn get_integrity(
    Query(params): Query<IntegrityQueryParams>,
) -> Result<Json<IntegrityListResponse>, (StatusCode, String)> {
    let extensions = params
        .extensions
        .as_deref()
        .unwrap_or("")
        .split(',')
        .filter(|s| !s.is_empty())
        .map(|s| s.trim().to_ascii_lowercase())
        .collect();

    let limit = params.limit.unwrap_or(50).clamp(1, 200);
    let offset = params.offset.unwrap_or(0).max(0);
    let status = params.status.unwrap_or_else(|| "unreviewed".to_string());

    let query = IntegrityQuery {
        root_id: params.root_id,
        issue_type: params.issue_type,
        extensions,
        status,
        path_search: params.path_search,
        offset,
        limit,
    };

    match integrity_api::query_integrity(&query) {
        Ok(result) => {
            let items = result
                .items
                .into_iter()
                .map(|item| IntegrityItemResponse {
                    item_id: item.item_id,
                    item_path: item.item_path,
                    item_name: item.item_name,
                    file_extension: item.file_extension,
                    do_not_validate: item.do_not_validate,
                    item_version: item.item_version,
                    val_state: item.val_state,
                    val_reviewed_at: item.val_reviewed_at,
                    hash_state: item.hash_state,
                    hash_reviewed_at: item.hash_reviewed_at,
                    first_scan_id: item.first_scan_id,
                    first_detected_at: item.first_detected_at,
                })
                .collect();

            Ok(Json(IntegrityListResponse {
                items,
                total: result.total,
                offset: result.offset,
                limit: result.limit,
            }))
        }
        Err(e) => {
            error!("Failed to query integrity for root {}: {}", params.root_id, e);
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to query integrity: {}", e),
            ))
        }
    }
}

/// Request body for POST /api/integrity/review
#[derive(Debug, Deserialize)]
pub struct ReviewRequest {
    pub item_id: i64,
    pub item_version: i64,
    /// Mark a validation issue on this item_version as reviewed
    pub review_val: Option<bool>,
    /// Mark a hash integrity issue on this item_version as reviewed
    pub review_hash: Option<bool>,
}

#[derive(Debug, Serialize)]
pub struct ReviewResponse {
    pub success: bool,
}

/// POST /api/integrity/review
/// Sets val_reviewed_at and/or hash_reviewed_at on the specified item_version.
pub async fn review(
    Json(req): Json<ReviewRequest>,
) -> Result<Json<ReviewResponse>, (StatusCode, String)> {
    let review_val = req.review_val.unwrap_or(false);
    let review_hash = req.review_hash.unwrap_or(false);

    if !review_val && !review_hash {
        return Err((
            StatusCode::BAD_REQUEST,
            "At least one of review_val or review_hash must be true".to_string(),
        ));
    }

    match integrity_api::review_integrity(
        req.item_id,
        req.item_version,
        review_val,
        review_hash,
    ) {
        Ok(()) => Ok(Json(ReviewResponse { success: true })),
        Err(e) => {
            error!(
                "Failed to review integrity for item {}, version {}: {}",
                req.item_id, req.item_version, e
            );
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to review: {}", e),
            ))
        }
    }
}

/// Request body for POST /api/integrity/do-not-validate
#[derive(Debug, Deserialize)]
pub struct DoNotValidateRequest {
    pub item_id: i64,
    pub do_not_validate: bool,
}

#[derive(Debug, Serialize)]
pub struct DoNotValidateResponse {
    pub success: bool,
}

/// POST /api/integrity/do-not-validate
/// Toggles the do_not_validate flag on an item.
pub async fn set_do_not_validate(
    Json(req): Json<DoNotValidateRequest>,
) -> Result<Json<DoNotValidateResponse>, (StatusCode, String)> {
    match integrity_api::set_do_not_validate(req.item_id, req.do_not_validate) {
        Ok(()) => Ok(Json(DoNotValidateResponse { success: true })),
        Err(e) => {
            error!(
                "Failed to set do_not_validate for item {}: {}",
                req.item_id, e
            );
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to set do_not_validate: {}", e),
            ))
        }
    }
}
