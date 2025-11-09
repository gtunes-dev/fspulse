use axum::Json;
use serde::Serialize;

/// Response structure for app information
#[derive(Debug, Serialize)]
pub struct AppInfo {
    pub name: String,
    pub version: String,
    pub git_commit: String,
    pub git_commit_short: String,
    pub git_branch: String,
    pub build_timestamp: String,
}

/// GET /api/app-info
///
/// Returns application version and build information
pub async fn get_app_info() -> Json<AppInfo> {
    Json(AppInfo {
        name: env!("CARGO_PKG_NAME").to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        git_commit: env!("GIT_COMMIT").to_string(),
        git_commit_short: env!("GIT_COMMIT_SHORT").to_string(),
        git_branch: env!("GIT_BRANCH").to_string(),
        build_timestamp: env!("BUILD_TIMESTAMP").to_string(),
    })
}
