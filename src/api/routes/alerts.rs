use axum::{
    extract::Path,
    http::StatusCode,
    response::Json,
};
use log::error;
use serde::{Deserialize, Serialize};

use crate::alerts::{AlertStatus, Alerts};
use crate::database::Database;

/// Request structure for updating alert status
#[derive(Debug, Deserialize)]
pub struct UpdateAlertStatusRequest {
    pub status: String,
}

/// Response structure for successful alert status update
#[derive(Debug, Serialize)]
pub struct UpdateAlertStatusResponse {
    pub success: bool,
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
    Alerts::set_alert_status(&db, alert_id, new_status).map_err(|e| {
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
