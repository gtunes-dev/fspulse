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
    /// "unacknowledged" (default), "acknowledged", "all"
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
    pub val_acknowledged_at: Option<i64>,
    pub hash_state: Option<i64>,
    pub hash_acknowledged_at: Option<i64>,
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

    let limit = params.limit.unwrap_or(50).min(200).max(1);
    let offset = params.offset.unwrap_or(0).max(0);
    let status = params.status.unwrap_or_else(|| "unacknowledged".to_string());

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
                    val_acknowledged_at: item.val_acknowledged_at,
                    hash_state: item.hash_state,
                    hash_acknowledged_at: item.hash_acknowledged_at,
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

/// Request body for POST /api/integrity/acknowledge
#[derive(Debug, Deserialize)]
pub struct AcknowledgeRequest {
    pub item_id: i64,
    pub item_version: i64,
    /// Acknowledge a validation issue on this item_version
    pub acknowledge_val: Option<bool>,
    /// Acknowledge a hash integrity issue on this item_version
    pub acknowledge_hash: Option<bool>,
}

#[derive(Debug, Serialize)]
pub struct AcknowledgeResponse {
    pub success: bool,
}

/// POST /api/integrity/acknowledge
/// Sets val_acknowledged_at and/or hash_acknowledged_at on the specified item_version.
pub async fn acknowledge(
    Json(req): Json<AcknowledgeRequest>,
) -> Result<Json<AcknowledgeResponse>, (StatusCode, String)> {
    let acknowledge_val = req.acknowledge_val.unwrap_or(false);
    let acknowledge_hash = req.acknowledge_hash.unwrap_or(false);

    if !acknowledge_val && !acknowledge_hash {
        return Err((
            StatusCode::BAD_REQUEST,
            "At least one of acknowledge_val or acknowledge_hash must be true".to_string(),
        ));
    }

    match integrity_api::acknowledge_integrity(
        req.item_id,
        req.item_version,
        acknowledge_val,
        acknowledge_hash,
    ) {
        Ok(()) => Ok(Json(AcknowledgeResponse { success: true })),
        Err(e) => {
            error!(
                "Failed to acknowledge integrity for item {}, version {}: {}",
                req.item_id, req.item_version, e
            );
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to acknowledge: {}", e),
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
