use axum::{
    http::StatusCode,
    response::Json,
};
use serde_json::{json, Value};

pub async fn list_alerts() -> Result<Json<Value>, StatusCode> {
    // TODO: Integrate with real alerts system from database
    let alerts = json!({
        "alerts": [],
        "total": 0,
        "unread": 0
    });

    Ok(Json(alerts))
}