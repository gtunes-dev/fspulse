use axum::{
    extract::Path,
    http::StatusCode,
    response::{Html, Json},
};
use serde_json::{json, Value};

use crate::database::Database;
use crate::scans::ScanStats;

pub async fn dashboard() -> Result<Html<String>, StatusCode> {
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

pub async fn api_status() -> Result<Json<Value>, StatusCode> {
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

pub async fn get_last_scan_stats() -> Result<Json<Value>, StatusCode> {
    let db = Database::new().map_err(|e| {
        eprintln!("Database error: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    match ScanStats::get_latest(&db) {
        Ok(Some(stats)) => Ok(Json(json!({
            "state": "last_scan",
            "scan_id": stats.scan_id,
            "root_id": stats.root_id,
            "root_path": stats.root_path,
            "scan_state": format!("{:?}", stats.state),
            "scan_time": stats.scan_time,
            "total_files": stats.total_files,
            "total_folders": stats.total_folders,
            "files_added": stats.files_added,
            "files_modified": stats.files_modified,
            "files_deleted": stats.files_deleted,
            "folders_added": stats.folders_added,
            "folders_modified": stats.folders_modified,
            "folders_deleted": stats.folders_deleted,
            "items_hashed": stats.items_hashed,
            "items_validated": stats.items_validated,
            "alerts_generated": stats.alerts_generated,
            "hash_enabled": stats.hash_enabled,
            "validation_enabled": stats.validation_enabled,
            "error": stats.error,
        }))),
        Ok(None) => Ok(Json(json!({
            "state": "no_scans"
        }))),
        Err(e) => {
            eprintln!("Error getting scan stats: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub async fn get_scan_stats(Path(scan_id): Path<i64>) -> Result<Json<Value>, StatusCode> {
    let db = Database::new().map_err(|e| {
        eprintln!("Database error: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    match ScanStats::get_for_scan(&db, scan_id) {
        Ok(Some(stats)) => Ok(Json(json!({
            "state": "scan_found",
            "scan_id": stats.scan_id,
            "root_id": stats.root_id,
            "root_path": stats.root_path,
            "scan_state": format!("{:?}", stats.state),
            "scan_time": stats.scan_time,
            "total_files": stats.total_files,
            "total_folders": stats.total_folders,
            "files_added": stats.files_added,
            "files_modified": stats.files_modified,
            "files_deleted": stats.files_deleted,
            "folders_added": stats.folders_added,
            "folders_modified": stats.folders_modified,
            "folders_deleted": stats.folders_deleted,
            "items_hashed": stats.items_hashed,
            "items_validated": stats.items_validated,
            "alerts_generated": stats.alerts_generated,
            "hash_enabled": stats.hash_enabled,
            "validation_enabled": stats.validation_enabled,
            "error": stats.error,
        }))),
        Ok(None) => Ok(Json(json!({
            "state": "not_found"
        }))),
        Err(e) => {
            eprintln!("Error getting scan stats for scan {}: {}", scan_id, e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}