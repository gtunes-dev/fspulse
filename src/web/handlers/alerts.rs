use axum::{
    http::StatusCode,
    response::Json,
    Extension,
};
use serde_json::{json, Value};
use std::path::PathBuf;

use crate::config::CONFIG;

pub async fn list_alerts(Extension(_db_path): Extension<Option<PathBuf>>) -> Result<Json<Value>, StatusCode> {
    let config = CONFIG.get().ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;

    let alerts = if config.web.use_mock_data {
        // Mock alerts data for development/testing
        json!({
        "alerts": [
            {
                "id": 1,
                "type": "validation_failure",
                "severity": "high",
                "message": "FLAC file corruption detected in /music/album1/track3.flac",
                "path": "/music/album1/track3.flac",
                "timestamp": "2025-10-09T09:15:00Z",
                "scan_id": 42
            },
            {
                "id": 2,
                "type": "size_change",
                "severity": "medium",
                "message": "File size changed significantly: /documents/important.pdf",
                "path": "/documents/important.pdf",
                "timestamp": "2025-10-09T08:30:00Z",
                "scan_id": 41
            },
            {
                "id": 3,
                "type": "new_files",
                "severity": "low",
                "message": "127 new files detected in /downloads",
                "path": "/downloads",
                "timestamp": "2025-10-09T07:45:00Z",
                "scan_id": 40
            }
        ],
        "total": 3,
        "unread": 2
    })
    } else {
        // TODO: Integrate with real alerts system from database
        json!({
            "alerts": [],
            "total": 0,
            "unread": 0
        })
    };

    Ok(Json(alerts))
}