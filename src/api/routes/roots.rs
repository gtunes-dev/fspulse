use axum::{extract::Path, http::StatusCode, Json};
use log::error;
use serde::{Deserialize, Serialize};

use rusqlite::OptionalExtension;

use crate::db::Database;
use crate::error::FsPulseError;
use crate::ignore::IgnoreSet;
use crate::root_status::{self, RootStatus};
use crate::roots::Root;
use crate::task::CheckpointSettings;
use crate::task_manager::TaskManager;
use crate::scans::Scan;
use crate::schedules;

/// Request structure for creating a new root
#[derive(Debug, Deserialize)]
pub struct CreateRootRequest {
    pub path: String,
}

/// Response structure for successful root creation
#[derive(Debug, Serialize)]
pub struct CreateRootResponse {
    pub root_id: i64,
    pub root_path: String,
}

/// Error response structure with user-friendly message
#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    pub error: String,
}

/// Response structure for a root with its last scan information
#[derive(Debug, Serialize)]
pub struct RootWithScan {
    pub root_id: i64,
    pub root_path: String,
    pub last_scan: Option<ScanInfo>,
    pub schedule_count: i64, // Number of active schedules for this root
}

/// Scan information subset for display
#[derive(Debug, Serialize)]
pub struct ScanInfo {
    pub scan_id: i64,
    pub state: String,
    pub started_at: i64, // Raw Unix timestamp for client-side formatting
    pub error: Option<String>,
}

/// POST /api/roots
/// Creates a new root after validating the path
pub async fn create_root(
    Json(req): Json<CreateRootRequest>,
) -> Result<(StatusCode, Json<CreateRootResponse>), (StatusCode, Json<ErrorResponse>)> {
    // Attempt to create the root using Root::try_create
    match Root::try_create(&req.path) {
        Ok(root) => {
            log::info!(
                "Created new root: {} (id: {})",
                root.root_path(),
                root.root_id()
            );
            Ok((
                StatusCode::CREATED,
                Json(CreateRootResponse {
                    root_id: root.root_id(),
                    root_path: root.root_path().to_string(),
                }),
            ))
        }
        Err(e) => {
            // Map FsPulseError to user-friendly error messages
            let (status_code, error_message) = match &e {
                // User input errors (400 Bad Request)
                FsPulseError::Error(msg) => {
                    if msg.contains("empty") {
                        (StatusCode::BAD_REQUEST, "Path cannot be empty".to_string())
                    } else if msg.contains("does not exist") {
                        (
                            StatusCode::BAD_REQUEST,
                            format!("Path does not exist: {}", extract_path_from_error(msg)),
                        )
                    } else if msg.contains("is a symlink") {
                        (
                            StatusCode::BAD_REQUEST,
                            "Symlinks are not allowed as roots".to_string(),
                        )
                    } else if msg.contains("not a directory") {
                        (
                            StatusCode::BAD_REQUEST,
                            "Path must be a directory, not a file".to_string(),
                        )
                    } else {
                        (StatusCode::BAD_REQUEST, msg.clone())
                    }
                }
                // Database errors (500 Internal Server Error or 409 Conflict for duplicates)
                FsPulseError::DatabaseError(db_err) => {
                    // Check if this is a unique constraint violation (duplicate root_path)
                    if let rusqlite::Error::SqliteFailure(sqlite_err, _) = db_err {
                        if sqlite_err.code == rusqlite::ErrorCode::ConstraintViolation {
                            (
                                StatusCode::CONFLICT,
                                "This root path already exists in the database".to_string(),
                            )
                        } else {
                            error!("Database error creating root: {}", db_err);
                            (
                                StatusCode::INTERNAL_SERVER_ERROR,
                                "Database error occurred".to_string(),
                            )
                        }
                    } else {
                        error!("Database error creating root: {}", db_err);
                        (
                            StatusCode::INTERNAL_SERVER_ERROR,
                            "Database error occurred".to_string(),
                        )
                    }
                }
                // IO errors (500 Internal Server Error)
                FsPulseError::IoError(io_err) => {
                    error!("IO error creating root: {}", io_err);
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        format!("File system error: {}", io_err),
                    )
                }
                // Other errors
                _ => {
                    error!("Unexpected error creating root: {}", e);
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        "An unexpected error occurred".to_string(),
                    )
                }
            };

            Err((
                status_code,
                Json(ErrorResponse {
                    error: error_message,
                }),
            ))
        }
    }
}

/// GET /api/roots/with-scans
/// Fetches all roots with their last scan information
pub async fn get_roots_with_scans(
) -> Result<Json<Vec<RootWithScan>>, (StatusCode, Json<ErrorResponse>)> {
    // Get database connection
    let conn = Database::get_connection().map_err(|e| {
        error!("Failed to get database connection: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: "Database connection error".to_string(),
            }),
        )
    })?;

    // Query all roots
    let mut stmt = conn
        .prepare_cached("SELECT root_id, root_path FROM roots ORDER BY root_path COLLATE natural_path")
        .map_err(|e| {
            error!("Failed to prepare query: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "Database query error".to_string(),
                }),
            )
        })?;

    let roots_iter = stmt
        .query_map([], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(|e| {
            error!("Failed to execute query: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "Database query error".to_string(),
                }),
            )
        })?;

    // Build response with scan information
    let mut results = Vec::new();
    for root_result in roots_iter {
        let (root_id, root_path) = root_result.map_err(|e| {
            error!("Failed to read root row: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "Database read error".to_string(),
                }),
            )
        })?;

        // Fetch last scan for this root
        let last_scan = match Scan::get_latest_for_root(root_id) {
            Ok(Some(scan)) => Some(ScanInfo {
                scan_id: scan.scan_id(),
                state: scan.state().to_string(),
                started_at: scan.started_at(),
                error: scan.error().map(|s| s.to_string()),
            }),
            Ok(None) => None,
            Err(e) => {
                error!("Failed to fetch last scan for root {}: {}", root_id, e);
                None // Continue with no scan info rather than failing
            }
        };

        // Count active schedules for this root
        let schedule_count = schedules::count_schedules_for_root(root_id).unwrap_or_else(|e| {
            error!("Failed to count schedules for root {}: {}", root_id, e);
            0 // Continue with 0 count rather than failing
        });

        results.push(RootWithScan {
            root_id,
            root_path,
            last_scan,
            schedule_count,
        });
    }

    Ok(Json(results))
}

/// GET /api/roots/{root_id}/schedule-count
/// Returns the number of active (enabled) schedules for a specific root
pub async fn get_schedule_count(
    Path(root_id): Path<i64>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let count = schedules::count_schedules_for_root(root_id).map_err(|e| {
        error!("Failed to count schedules for root {}: {}", root_id, e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok(Json(serde_json::json!({ "count": count })))
}

/// DELETE /api/roots/{root_id}
/// Deletes a root and all associated data (scans, items, versions)
pub async fn delete_root(
    Path(root_id): Path<i64>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    // Attempt to delete the root
    match Root::delete_root(root_id) {
        Ok(()) => {
            log::info!("Deleted root with id: {}", root_id);
            Ok(StatusCode::NO_CONTENT)
        }
        Err(e) => {
            // Map FsPulseError to user-friendly error messages
            let (status_code, error_message) = match &e {
                FsPulseError::Error(msg) if msg.contains("not found") => (
                    StatusCode::NOT_FOUND,
                    format!("Root with id {} not found", root_id),
                ),
                FsPulseError::DatabaseError(db_err) => {
                    error!("Database error deleting root: {}", db_err);
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        "Database error occurred while deleting root".to_string(),
                    )
                }
                _ => {
                    error!("Unexpected error deleting root: {}", e);
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        "An unexpected error occurred".to_string(),
                    )
                }
            };

            Err((
                status_code,
                Json(ErrorResponse {
                    error: error_message,
                }),
            ))
        }
    }
}

/// Helper function to extract the path from error messages
fn extract_path_from_error(msg: &str) -> &str {
    // Error messages are formatted like "Path '/some/path' does not exist"
    if let Some(start) = msg.find('\'') {
        if let Some(end) = msg[start + 1..].find('\'') {
            return &msg[start + 1..start + 1 + end];
        }
    }
    "" // Return empty string if path not found in message
}

// ============================================================================
// Observation: watching, ignore patterns, and reconcile walks
// ============================================================================

/// How one root is being observed: its settings, when it was last
/// walked, and the watcher's current view of it.
#[derive(Debug, Serialize)]
pub struct RootObservation {
    pub root_id: i64,
    pub root_path: String,
    /// Setting: watch this root for realtime changes.
    pub watch_enabled: bool,
    /// Setting: patterns excluded from watching and walks.
    pub ignore_patterns: Vec<String>,
    pub last_walk_started_at: Option<i64>,
    /// `None` if no walk has completed, or the latest one did not.
    pub last_walk_completed_at: Option<i64>,
    /// The ignore patterns changed and no full check has completed
    /// since. Nothing new is recorded for ignored paths, but items
    /// tracked before the change may not be relabelled yet.
    pub ignore_pending: bool,
    /// Runtime state from the watcher and the last walk.
    #[serde(flatten)]
    pub status: RootStatus,
}

fn internal_error(context: &str, e: impl std::fmt::Display) -> (StatusCode, Json<ErrorResponse>) {
    error!("{context}: {e}");
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(ErrorResponse {
            error: "An unexpected error occurred".to_string(),
        }),
    )
}

fn root_not_found(root_id: i64) -> (StatusCode, Json<ErrorResponse>) {
    (
        StatusCode::NOT_FOUND,
        Json(ErrorResponse {
            error: format!("Root {root_id} not found"),
        }),
    )
}

/// GET /api/roots/observation
/// Every root with its observation settings and current watch state.
pub async fn get_observation(
) -> Result<Json<Vec<RootObservation>>, (StatusCode, Json<ErrorResponse>)> {
    let conn = Database::get_connection().map_err(|e| internal_error("Database connection", e))?;
    let mut stmt = conn
        .prepare_cached(
            "SELECT root_id, root_path, watch_enabled, ignore_patterns,
                    last_checkpoint_started_at, last_checkpoint_completed_at,
                    ignore_changed_at IS NOT NULL
               FROM roots ORDER BY root_path COLLATE natural_path",
        )
        .map_err(|e| internal_error("Reading roots", e))?;

    let roots = stmt
        .query_map([], |row| {
            let root_id: i64 = row.get(0)?;
            let patterns: Option<String> = row.get(3)?;
            Ok(RootObservation {
                root_id,
                root_path: row.get(1)?,
                watch_enabled: row.get(2)?,
                // Shown as stored; a value that cannot be parsed shows as
                // no patterns, which is also how it is applied.
                ignore_patterns: patterns
                    .and_then(|json| serde_json::from_str(&json).ok())
                    .unwrap_or_default(),
                last_walk_started_at: row.get(4)?,
                last_walk_completed_at: row.get(5)?,
                ignore_pending: row.get(6)?,
                status: root_status::get(root_id),
            })
        })
        .and_then(|rows| rows.collect::<Result<Vec<_>, _>>())
        .map_err(|e| internal_error("Reading roots", e))?;

    Ok(Json(roots))
}

/// Request to change a root's observation settings. A field that is
/// left out is not changed.
#[derive(Debug, Deserialize)]
pub struct UpdateObservationRequest {
    pub watch_enabled: Option<bool>,
    pub ignore_patterns: Option<Vec<String>>,
    /// Queue a full check of the root after saving, so that items
    /// already tracked are relabelled now and not at the next scheduled
    /// check.
    #[serde(default)]
    pub run_check: bool,
}

/// PUT /api/roots/{root_id}/observation
/// Changes whether a root is watched and what it ignores.
///
/// New patterns are in force for every write from the moment this
/// transaction commits: `item_ops` reads them under the write lock.
/// Items that were already tracked are relabelled lazily, by the next
/// full check (queued here if `run_check` is set). The watcher is told
/// to reload at once.
pub async fn update_observation(
    Path(root_id): Path<i64>,
    Json(req): Json<UpdateObservationRequest>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    let patterns_json = match &req.ignore_patterns {
        None => None,
        Some(patterns) => Some(
            IgnoreSet::to_stored_json(patterns)
                .map_err(|e| internal_error("Encoding ignore patterns", e))?,
        ),
    };

    let conn = Database::get_connection().map_err(|e| internal_error("Database connection", e))?;
    let updated = Database::immediate_transaction(&conn, |c| {
        let exists = c
            .query_row("SELECT 1 FROM roots WHERE root_id = ?", [root_id], |_| Ok(()))
            .optional()?
            .is_some();
        if !exists {
            return Ok(false);
        }
        if let Some(watch_enabled) = req.watch_enabled {
            c.execute(
                "UPDATE roots SET watch_enabled = ?1 WHERE root_id = ?2",
                rusqlite::params![watch_enabled, root_id],
            )?;
        }
        if let Some(json) = &patterns_json {
            // ignore_changed_at marks "changed, not yet covered by a
            // full check". It only ever goes up, even within one
            // second, so a check can tell whether the patterns moved
            // while it ran by comparing values.
            c.execute(
                "UPDATE roots
                    SET ignore_patterns = ?1,
                        ignore_changed_at = MAX(?3, COALESCE(ignore_changed_at, 0) + 1)
                  WHERE root_id = ?2 AND ignore_patterns IS NOT ?1",
                rusqlite::params![json, root_id, crate::utils::Utils::now_secs()],
            )?;
        }
        Ok(true)
    })
    .map_err(|e| internal_error("Updating root observation settings", e))?;

    if !updated {
        return Err(root_not_found(root_id));
    }
    log::info!("Updated observation settings for root {root_id}");
    crate::watcher::request_refresh();

    if req.run_check {
        // The settings are saved either way. If the check cannot be
        // queued (tasks paused, say), the root stays marked as pending.
        match TaskManager::request_checkpoint(&conn, root_id, &CheckpointSettings::default()) {
            Ok(()) => log::info!("Full check queued for root {root_id} after a settings change"),
            Err(e) => log::warn!("Could not queue a full check for root {root_id}: {e}"),
        }
    }
    Ok(StatusCode::NO_CONTENT)
}

/// Request to walk a root now.
#[derive(Debug, Default, Deserialize)]
pub struct WalkRequest {
    /// Confirms that a large amount of missing content really was
    /// deleted. Needed to get past a walk that refused because the root
    /// was empty or most of its items were missing.
    #[serde(default)]
    pub confirm_deletions: bool,
}

/// POST /api/roots/{root_id}/checkpoint
/// Queues a reconcile walk of a root. Returns 202; the walk runs through
/// the task system.
pub async fn request_checkpoint(
    Path(root_id): Path<i64>,
    body: Option<Json<WalkRequest>>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    let Json(req) = body.unwrap_or_default();
    let settings = CheckpointSettings {
        allow_mass_delete: req.confirm_deletions,
    };

    let conn = Database::get_connection().map_err(|e| internal_error("Database connection", e))?;
    match TaskManager::request_checkpoint(&conn, root_id, &settings) {
        Ok(()) => {
            log::info!(
                "Walk queued for root {root_id} (deletions confirmed: {})",
                req.confirm_deletions
            );
            Ok(StatusCode::ACCEPTED)
        }
        Err(FsPulseError::Error(message)) if message.contains("not found") => {
            Err(root_not_found(root_id))
        }
        Err(FsPulseError::Error(message)) => Err((
            StatusCode::CONFLICT,
            Json(ErrorResponse { error: message }),
        )),
        Err(FsPulseError::ShuttingDown) => Err((
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ErrorResponse {
                error: "The server is shutting down".to_string(),
            }),
        )),
        Err(e) => Err(internal_error("Queueing a walk", e)),
    }
}
