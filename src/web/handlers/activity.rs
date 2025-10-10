use axum::{
    http::StatusCode,
    response::Json,
    Extension,
};
use serde_json::{json, Value};
use std::path::PathBuf;

use crate::config::CONFIG;

pub async fn recent_activity(Extension(_db_path): Extension<Option<PathBuf>>) -> Result<Json<Value>, StatusCode> {
    let config = CONFIG.get().ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;

    let activity = if config.web.use_mock_data {
        // Mock activity data for development/testing
        json!({
        "current_scans": [
            {
                "id": 43,
                "root_path": "/home/user/documents",
                "status": "scanning",
                "progress": 67,
                "files_scanned": 1234,
                "files_total": 1845,
                "started": "2025-10-09T10:15:00Z"
            }
        ],
        "recent_scans": [
            {
                "id": 42,
                "root_path": "/home/user/music",
                "status": "completed",
                "files_scanned": 3456,
                "duration_seconds": 127,
                "completed": "2025-10-09T09:30:00Z",
                "alerts_generated": 1
            },
            {
                "id": 41,
                "root_path": "/home/user/documents",
                "status": "completed",
                "files_scanned": 1823,
                "duration_seconds": 89,
                "completed": "2025-10-09T08:45:00Z",
                "alerts_generated": 1
            }
        ],
        "recent_changes": [
            {
                "file_path": "/documents/important.pdf",
                "change_type": "size_modified",
                "old_size": 1024768,
                "new_size": 1156432,
                "timestamp": "2025-10-09T08:30:00Z"
            },
            {
                "file_path": "/downloads/new_file.zip",
                "change_type": "added",
                "size": 2048576,
                "timestamp": "2025-10-09T07:45:00Z"
            }
        ]
    })
    } else {
        // TODO: Integrate with real scans and changes from database
        json!({
            "current_scans": [],
            "recent_scans": [],
            "recent_changes": []
        })
    };

    Ok(Json(activity))
}