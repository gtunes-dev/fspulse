use axum::{extract::Path, http::StatusCode, response::Json};
use log::error;
use serde::{Deserialize, Serialize};

use crate::alerts::{AlertStatus, Alerts};
use crate::db::Database;

/// Request structure for updating alert status
#[derive(Debug, Deserialize)]
pub struct UpdateAlertStatusRequest {
    pub status: String,
}

/// Request structure for bulk updating alert status by IDs
#[derive(Debug, Deserialize)]
pub struct BulkUpdateAlertStatusRequest {
    pub alert_ids: Vec<i64>,
    pub status: String,
}

/// Request structure for bulk updating alert status by filter criteria
#[derive(Debug, Deserialize)]
pub struct BulkUpdateAlertStatusByFilterRequest {
    pub status: String,
    pub status_filter: Option<String>,
    pub type_filter: Option<String>,
    pub root_id: Option<i64>,
    pub item_path: Option<String>,
}

/// Response structure for successful alert status update
#[derive(Debug, Serialize)]
pub struct UpdateAlertStatusResponse {
    pub success: bool,
}

/// Response structure for bulk update operations
#[derive(Debug, Serialize)]
pub struct BulkUpdateAlertStatusResponse {
    pub success: bool,
    pub updated_count: usize,
}

/// Error response structure
#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    pub error: String,
}

/// PUT /api/alerts/{alert_id}/status
/// Updates the status of an alert
pub async fn update_alert_status(
    Path(alert_id): Path<i64>,
    Json(req): Json<UpdateAlertStatusRequest>,
) -> Result<(StatusCode, Json<UpdateAlertStatusResponse>), (StatusCode, Json<ErrorResponse>)> {
    // Get database connection
    let conn = Database::get_connection().map_err(|e| {
        error!("Failed to get database connection: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: "Database connection error".to_string(),
            }),
        )
    })?;

    // Parse status string to AlertStatus enum
    let new_status = match req.status.as_str() {
        "O" => AlertStatus::Open,
        "F" => AlertStatus::Flagged,
        "D" => AlertStatus::Dismissed,
        _ => {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    error: format!("Invalid alert status: '{}'", req.status),
                }),
            ));
        }
    };

    // Update the alert status
    Alerts::set_alert_status(&conn, alert_id, new_status).map_err(|e| {
        error!("Failed to update alert status: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: "Failed to update alert status".to_string(),
            }),
        )
    })?;

    Ok((
        StatusCode::OK,
        Json(UpdateAlertStatusResponse { success: true }),
    ))
}

/// Helper to parse a status string to AlertStatus enum
fn parse_alert_status(status: &str) -> Result<AlertStatus, (StatusCode, Json<ErrorResponse>)> {
    match status {
        "O" => Ok(AlertStatus::Open),
        "F" => Ok(AlertStatus::Flagged),
        "D" => Ok(AlertStatus::Dismissed),
        _ => Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: format!("Invalid alert status: '{}'", status),
            }),
        )),
    }
}

/// PUT /api/alerts/bulk-status
/// Updates the status of multiple alerts by their IDs
pub async fn bulk_update_alert_status(
    Json(req): Json<BulkUpdateAlertStatusRequest>,
) -> Result<(StatusCode, Json<BulkUpdateAlertStatusResponse>), (StatusCode, Json<ErrorResponse>)> {
    if req.alert_ids.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "No alert IDs provided".to_string(),
            }),
        ));
    }

    let conn = Database::get_connection().map_err(|e| {
        error!("Failed to get database connection: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: "Database connection error".to_string(),
            }),
        )
    })?;

    let new_status = parse_alert_status(&req.status)?;

    let updated_count =
        Alerts::set_bulk_alert_status(&conn, &req.alert_ids, new_status).map_err(|e| {
            error!("Failed to bulk update alert status: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "Failed to bulk update alert status".to_string(),
                }),
            )
        })?;

    Ok((
        StatusCode::OK,
        Json(BulkUpdateAlertStatusResponse {
            success: true,
            updated_count,
        }),
    ))
}

/// PUT /api/alerts/bulk-status-by-filter
/// Updates the status of all alerts matching the given filter criteria
pub async fn bulk_update_alert_status_by_filter(
    Json(req): Json<BulkUpdateAlertStatusByFilterRequest>,
) -> Result<(StatusCode, Json<BulkUpdateAlertStatusResponse>), (StatusCode, Json<ErrorResponse>)> {
    let conn = Database::get_connection().map_err(|e| {
        error!("Failed to get database connection: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: "Database connection error".to_string(),
            }),
        )
    })?;

    let new_status = parse_alert_status(&req.status)?;

    let updated_count = Alerts::set_filtered_alert_status(
        &conn,
        new_status,
        req.status_filter.as_deref(),
        req.type_filter.as_deref(),
        req.root_id,
        req.item_path.as_deref(),
    )
    .map_err(|e| {
        error!("Failed to bulk update alerts by filter: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: "Failed to bulk update alerts".to_string(),
            }),
        )
    })?;

    Ok((
        StatusCode::OK,
        Json(BulkUpdateAlertStatusResponse {
            success: true,
            updated_count,
        }),
    ))
}
