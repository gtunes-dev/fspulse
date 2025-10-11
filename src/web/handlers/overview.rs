use axum::{
    http::StatusCode,
    response::{Html, Json},
    Extension,
};
use serde_json::{json, Value};
use std::path::PathBuf;

pub async fn dashboard(Extension(_db_path): Extension<Option<PathBuf>>) -> Result<Html<String>, StatusCode> {
    // Check if we're running from source directory (development mode)
    let html = if std::path::Path::new("src/web/templates/dashboard.html").exists() {
        // Development: read from file system for instant updates
        std::fs::read_to_string("src/web/templates/dashboard.html")
            .unwrap_or_else(|_| include_str!("../templates/dashboard.html").to_string())
    } else {
        // Production: use embedded template
        include_str!("../templates/dashboard.html").to_string()
    };

    Ok(Html(html))
}

pub async fn api_status(Extension(_db_path): Extension<Option<PathBuf>>) -> Result<Json<Value>, StatusCode> {
    // TODO: Integrate with real database status
    let status = json!({
        "server": "running",
        "database": "connected",
        "active_scans": 0,
        "total_alerts": 0,
        "last_scan": null
    });

    Ok(Json(status))
}