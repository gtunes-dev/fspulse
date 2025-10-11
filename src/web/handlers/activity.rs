use axum::{
    http::StatusCode,
    response::Json,
    Extension,
};
use serde_json::{json, Value};
use std::path::PathBuf;

pub async fn recent_activity(Extension(_db_path): Extension<Option<PathBuf>>) -> Result<Json<Value>, StatusCode> {
    // TODO: Integrate with real scans and changes from database
    let activity = json!({
        "current_scans": [],
        "recent_scans": [],
        "recent_changes": []
    });

    Ok(Json(activity))
}