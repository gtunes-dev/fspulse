use axum::Json;
use serde::Serialize;

use crate::database::Database;

/// Response structure for app information
#[derive(Debug, Serialize)]
pub struct AppInfo {
    pub name: String,
    pub version: String,
    pub schema_version: String,
    pub git_commit: String,
    pub git_commit_short: String,
    pub git_branch: String,
    pub build_timestamp: String,
}

/// GET /api/app-info
///
/// Returns application version and build information
pub async fn get_app_info() -> Json<AppInfo> {
    let schema_version = Database::get_schema_version().unwrap_or_else(|_| "unknown".to_string());

    Json(AppInfo {
        name: env!("CARGO_PKG_NAME").to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        schema_version,
        git_commit: env!("GIT_COMMIT").to_string(),
        git_commit_short: env!("GIT_COMMIT_SHORT").to_string(),
        git_branch: env!("GIT_BRANCH").to_string(),
        build_timestamp: env!("BUILD_TIMESTAMP").to_string(),
    })
}
