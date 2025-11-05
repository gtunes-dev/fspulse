use axum::{extract::Path, http::StatusCode, Json};
use log::error;
use serde::{Deserialize, Serialize};

use crate::database::Database;
use crate::error::FsPulseError;
use crate::roots::Root;
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
    pub schedule_count: i64,  // Number of active schedules for this root
}

/// Scan information subset for display
#[derive(Debug, Serialize)]
pub struct ScanInfo {
    pub scan_id: i64,
    pub state: String,
    pub scan_time: i64,  // Raw Unix timestamp for client-side formatting
    pub file_count: Option<i64>,
    pub folder_count: Option<i64>,
    pub error: Option<String>,
}

/// POST /api/roots
/// Creates a new root after validating the path
pub async fn create_root(
    Json(req): Json<CreateRootRequest>,
) -> Result<(StatusCode, Json<CreateRootResponse>), (StatusCode, Json<ErrorResponse>)> {
    // Open database connection
    let db = Database::new().map_err(|e| {
        error!("Failed to open database: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: "Database connection error".to_string(),
            }),
        )
    })?;

    // Attempt to create the root using Root::try_create
    match Root::try_create(&db, &req.path) {
        Ok(root) => {
            log::info!("Created new root: {} (id: {})", root.root_path(), root.root_id());
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

            Err((status_code, Json(ErrorResponse { error: error_message })))
        }
    }
}

/// GET /api/roots/with-scans
/// Fetches all roots with their last scan information
pub async fn get_roots_with_scans() -> Result<Json<Vec<RootWithScan>>, (StatusCode, Json<ErrorResponse>)> {
    // Open database connection
    let db = Database::new().map_err(|e| {
        error!("Failed to open database: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: "Database connection error".to_string(),
            }),
        )
    })?;

    // Query all roots
    let conn = db.conn();
    let mut stmt = conn
        .prepare("SELECT root_id, root_path FROM roots ORDER BY root_path COLLATE natural_path")
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
        let last_scan = match Scan::get_latest_for_root(&db, root_id) {
            Ok(Some(scan)) => Some(ScanInfo {
                scan_id: scan.scan_id(),
                state: scan.state().to_string(),
                scan_time: scan.scan_time(),  // Return raw Unix timestamp
                file_count: scan.file_count(),
                folder_count: scan.folder_count(),
                error: scan.error().map(|s| s.to_string()),
            }),
            Ok(None) => None,
            Err(e) => {
                error!("Failed to fetch last scan for root {}: {}", root_id, e);
                None // Continue with no scan info rather than failing
            }
        };

        // Count active schedules for this root
        let schedule_count = schedules::count_schedules_for_root(&db, root_id)
            .unwrap_or_else(|e| {
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

/// DELETE /api/roots/{root_id}
/// Deletes a root and all associated data (scans, items, changes, alerts)
pub async fn delete_root(
    Path(root_id): Path<i64>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    // Open database connection
    let db = Database::new().map_err(|e| {
        error!("Failed to open database: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: "Database connection error".to_string(),
            }),
        )
    })?;

    // Attempt to delete the root
    match Root::delete_root(&db, root_id) {
        Ok(()) => {
            log::info!("Deleted root with id: {}", root_id);
            Ok(StatusCode::NO_CONTENT)
        }
        Err(e) => {
            // Map FsPulseError to user-friendly error messages
            let (status_code, error_message) = match &e {
                FsPulseError::Error(msg) if msg.contains("not found") => {
                    (StatusCode::NOT_FOUND, format!("Root with id {} not found", root_id))
                }
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

            Err((status_code, Json(ErrorResponse { error: error_message })))
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
