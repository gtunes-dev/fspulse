use axum::{
    http::StatusCode,
    response::Json,
    Extension,
};
use serde_json::{json, Value};
use std::path::PathBuf;

pub async fn list_alerts(Extension(_db_path): Extension<Option<PathBuf>>) -> Result<Json<Value>, StatusCode> {
    // TODO: Integrate with real alerts system from database
    let alerts = json!({
        "alerts": [],
        "total": 0,
        "unread": 0
    });

    Ok(Json(alerts))
}