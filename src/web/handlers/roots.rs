use axum::{http::StatusCode, Extension, Json};
use log::error;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::database::Database;
use crate::error::FsPulseError;
use crate::roots::Root;

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

/// POST /api/roots
/// Creates a new root after validating the path
pub async fn create_root(
    Extension(db_path): Extension<Option<PathBuf>>,
    Json(req): Json<CreateRootRequest>,
) -> Result<(StatusCode, Json<CreateRootResponse>), (StatusCode, Json<ErrorResponse>)> {
    // Open database connection
    let db = Database::new(db_path).map_err(|e| {
        error!("Failed to open database: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: "Database connection error".to_string(),
            }),
        )
    })?;

    // Attempt to create the root using Root::try_create
    // This handles all validation: empty path, existence, symlink checks, directory checks
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_path_from_error() {
        let msg = "Path '/foo/bar' does not exist";
        assert_eq!(extract_path_from_error(msg), "/foo/bar");

        let msg2 = "Path '/a/b/c' is a symlink and not allowed";
        assert_eq!(extract_path_from_error(msg2), "/a/b/c");

        let msg3 = "Provided path is empty";
        assert_eq!(extract_path_from_error(msg3), "");
    }
}
