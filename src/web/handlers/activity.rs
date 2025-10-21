use axum::{
    http::StatusCode,
    response::Json,
};
use serde_json::{json, Value};

pub async fn recent_activity() -> Result<Json<Value>, StatusCode> {
    // TODO: Integrate with real scans and changes from database
    let activity = json!({
        "current_scans": [],
        "recent_scans": [],
        "recent_changes": []
    });

    Ok(Json(activity))
}