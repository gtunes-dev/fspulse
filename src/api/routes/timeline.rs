//! Time-based browsing endpoints. See `crate::timeline` for what a
//! range means and how times are adjusted.

use axum::{extract::Query, http::StatusCode, Json};
use log::error;
use serde::{Deserialize, Serialize};

use crate::db::Database;
use crate::error::FsPulseError;
use crate::timeline::{self, ActivityPoint, Change, ChangesPage, Listing, Summary, TimeRange};
use crate::utils::Utils;

#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    pub error: String,
}

type ApiError = (StatusCode, Json<ErrorResponse>);

fn api_error(context: &str, e: FsPulseError) -> ApiError {
    match e {
        // "Root 7 not found", "Folder not found: /path"
        FsPulseError::Error(message) if message.contains("not found") => (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse { error: message }),
        ),
        other => {
            error!("{context}: {other}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "An unexpected error occurred".to_string(),
                }),
            )
        }
    }
}

/// Times are Unix seconds.
#[derive(Debug, Deserialize)]
pub struct ChildrenParams {
    pub root_id: i64,
    /// Absolute path of the folder to list. The root's own path lists
    /// the top level.
    pub parent_path: String,
    /// Start of the range. Leave out for a point in time.
    pub from: Option<i64>,
    /// The time to show. Leave out for now.
    pub to: Option<i64>,
    /// Also list items that appeared and disappeared inside the range.
    #[serde(default)]
    pub include_transient: bool,
}

/// GET /api/timeline/children?root_id=1&parent_path=/data&from=..&to=..
///
/// A folder's children as of `to`, in name order. With `from`, each
/// child also says how it differs between `from` and `to`, each folder
/// carries the net changes beneath it, and children deleted inside the
/// range are listed. The response's `range` holds the times actually
/// used: a start, and an end in the past, are moved back to the hour.
pub async fn get_children(
    Query(params): Query<ChildrenParams>,
) -> Result<Json<Listing>, ApiError> {
    let range = TimeRange::resolve(params.from, params.to, Utils::now_secs());
    let conn = Database::get_connection().map_err(|e| api_error("Database connection", e))?;
    timeline::children(
        &conn,
        params.root_id,
        &params.parent_path,
        range,
        params.include_transient,
    )
    .map(Json)
    .map_err(|e| api_error("Listing folder", e))
}

#[derive(Debug, Deserialize)]
pub struct ActivityParams {
    pub root_id: i64,
    /// Leave out for all recorded history.
    pub from: Option<i64>,
    /// Leave out for now.
    pub to: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct ActivityResponse {
    pub from: i64,
    pub to: i64,
    pub points: Vec<ActivityPoint>,
}

/// GET /api/timeline/activity?root_id=1&from=..&to=..
///
/// The hours in which a root changed, oldest first, with how many items
/// were added, modified and deleted in each and the root's totals at
/// the end of it. Hours without changes are absent. Grouping into days
/// is left to the client, which knows the viewer's time zone.
pub async fn get_activity(
    Query(params): Query<ActivityParams>,
) -> Result<Json<ActivityResponse>, ApiError> {
    let now = Utils::now_secs();
    let from = params.from.unwrap_or(0);
    let to = params.to.unwrap_or(now).min(now);
    let conn = Database::get_connection().map_err(|e| api_error("Database connection", e))?;
    let points = timeline::activity(&conn, params.root_id, from, to)
        .map_err(|e| api_error("Reading activity", e))?;
    Ok(Json(ActivityResponse { from, to, points }))
}

#[derive(Debug, Deserialize)]
pub struct SummaryParams {
    pub root_id: i64,
    /// Limit to this folder and everything beneath it. Leave out for
    /// the whole root.
    pub path: Option<String>,
    pub from: i64,
    pub to: Option<i64>,
}

/// GET /api/timeline/summary?root_id=1&from=..&to=..&path=/data/docs
///
/// How many items were added, modified and deleted between `from` and
/// `to`, comparing the two ends, plus how many came and went in
/// between (`transient`).
pub async fn get_summary(Query(params): Query<SummaryParams>) -> Result<Json<Summary>, ApiError> {
    let range = TimeRange::resolve(Some(params.from), params.to, Utils::now_secs());
    let conn = Database::get_connection().map_err(|e| api_error("Database connection", e))?;
    timeline::summary(&conn, params.root_id, params.path.as_deref(), range)
        .map(Json)
        .map_err(|e| api_error("Summarizing changes", e))
}

fn default_page_size() -> i64 {
    200
}

/// Most rows one request may ask for.
const MAX_PAGE_SIZE: i64 = 5000;

#[derive(Debug, Deserialize)]
pub struct ChangesParams {
    pub root_id: i64,
    pub path: Option<String>,
    pub from: i64,
    pub to: Option<i64>,
    /// Comma-separated kinds to include: `added`, `modified`, `deleted`,
    /// `transient`. Leave out for added, modified and deleted.
    pub kinds: Option<String>,
    #[serde(default = "default_page_size")]
    pub limit: i64,
    #[serde(default)]
    pub offset: i64,
}

fn parse_kinds(kinds: Option<&str>) -> Result<i64, ApiError> {
    let Some(kinds) = kinds else {
        return Ok(Change::net_mask());
    };
    let mut mask = 0;
    for kind in kinds.split(',').map(str::trim).filter(|k| !k.is_empty()) {
        mask |= match kind {
            "added" => Change::Added.mask(),
            "modified" => Change::Modified.mask(),
            "deleted" => Change::Deleted.mask(),
            "transient" => Change::Transient.mask(),
            other => {
                return Err((
                    StatusCode::BAD_REQUEST,
                    Json(ErrorResponse {
                        error: format!(
                            "Unknown kind '{other}'. Use added, modified, deleted or transient."
                        ),
                    }),
                ))
            }
        };
    }
    Ok(mask)
}

/// GET /api/timeline/changes?root_id=1&from=..&to=..&kinds=added,deleted&limit=200&offset=0
///
/// One page of the items that differ between `from` and `to`, in
/// folder order (a folder, then its contents). Each has its path, how
/// it changed, its size at `to`, and how many changes to it were
/// recorded in the range.
pub async fn get_changes(
    Query(params): Query<ChangesParams>,
) -> Result<Json<ChangesPage>, ApiError> {
    let kinds = parse_kinds(params.kinds.as_deref())?;
    let range = TimeRange::resolve(Some(params.from), params.to, Utils::now_secs());
    let conn = Database::get_connection().map_err(|e| api_error("Database connection", e))?;
    timeline::changes(
        &conn,
        params.root_id,
        params.path.as_deref(),
        range,
        kinds,
        params.limit.clamp(1, MAX_PAGE_SIZE),
        params.offset.max(0),
    )
    .map(Json)
    .map_err(|e| api_error("Listing changes", e))
}

#[derive(Debug, Deserialize)]
pub struct SearchParams {
    pub root_id: i64,
    pub query: String,
    pub from: Option<i64>,
    pub to: Option<i64>,
    #[serde(default = "default_page_size")]
    pub limit: i64,
}

/// GET /api/timeline/search?root_id=1&query=report&from=..&to=..
///
/// Items whose name contains `query`, as of `to`, in path order. With
/// `from`, items deleted inside the range are included and each result
/// says how it changed.
pub async fn search(Query(params): Query<SearchParams>) -> Result<Json<Listing>, ApiError> {
    let range = TimeRange::resolve(params.from, params.to, Utils::now_secs());
    let conn = Database::get_connection().map_err(|e| api_error("Database connection", e))?;
    timeline::search(
        &conn,
        params.root_id,
        &params.query,
        range,
        params.limit.clamp(1, MAX_PAGE_SIZE),
    )
    .map(Json)
    .map_err(|e| api_error("Searching", e))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kinds_default_to_net_changes_and_reject_unknown_names() {
        assert_eq!(parse_kinds(None).unwrap(), Change::net_mask());
        assert_eq!(
            parse_kinds(Some("added, deleted")).unwrap(),
            Change::Added.mask() | Change::Deleted.mask()
        );
        assert_eq!(parse_kinds(Some("transient")).unwrap(), Change::Transient.mask());
        assert!(parse_kinds(Some("added,renamed")).is_err());
    }
}
