use axum::{
    extract::Query,
    http::StatusCode,
    response::Json,
    Extension,
};
use serde::Serialize;
use std::path::PathBuf;

use crate::config::CONFIG;
use crate::database::{Database, ListQuery};
use crate::scans::ScanRecord;

use super::common::{ListParams, ListResponse};

#[derive(Serialize)]
pub struct ScanItem {
    pub id: i64,
    pub root_id: i64,
    pub root_path: Option<String>,
    pub state: String,
    pub scan_time: String,
    pub file_count: Option<i64>,
    pub folder_count: Option<i64>,
    pub duration_seconds: Option<i64>,
}

pub async fn list_scans(
    Query(params): Query<ListParams>,
    Extension(db_path): Extension<Option<PathBuf>>
) -> Result<Json<ListResponse<ScanItem>>, StatusCode> {
    let config = CONFIG.get().ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;

    if config.web.use_mock_data {
        // Mock scan data for development/testing (keep for backwards compatibility)
        let all_scans = vec![
            ScanItem {
                id: 1,
                root_id: 1,
                root_path: Some("/Users/john/Documents".to_string()),
                state: "completed".to_string(),
                scan_time: "2025-10-09T10:30:00Z".to_string(),
                file_count: Some(1234),
                folder_count: Some(45),
                duration_seconds: Some(87),
            },
            ScanItem {
                id: 2,
                root_id: 2,
                root_path: Some("/Users/john/Downloads".to_string()),
                state: "completed".to_string(),
                scan_time: "2025-10-09T08:15:00Z".to_string(),
                file_count: Some(856),
                folder_count: Some(23),
                duration_seconds: Some(42),
            },
            ScanItem {
                id: 3,
                root_id: 1,
                root_path: Some("/Users/john/Documents".to_string()),
                state: "scanning".to_string(),
                scan_time: "2025-10-09T11:00:00Z".to_string(),
                file_count: Some(678),
                folder_count: Some(12),
                duration_seconds: None,
            },
            ScanItem {
                id: 4,
                root_id: 3,
                root_path: Some("/Users/john/Pictures".to_string()),
                state: "completed".to_string(),
                scan_time: "2025-10-08T16:45:00Z".to_string(),
                file_count: Some(2341),
                folder_count: Some(78),
                duration_seconds: Some(156),
            },
            ScanItem {
                id: 5,
                root_id: 2,
                root_path: Some("/Users/john/Downloads".to_string()),
                state: "stopped".to_string(),
                scan_time: "2025-10-08T14:22:00Z".to_string(),
                file_count: Some(234),
                folder_count: Some(8),
                duration_seconds: Some(15),
            },
        ];

        // Apply the old application-tier filtering/sorting for mock data
        let mut filtered_scans = all_scans;
        if let Some(filter) = &params.filter {
            if !filter.is_empty() {
                filtered_scans.retain(|scan| {
                    let root_path_matches = scan.root_path.as_ref()
                        .map(|path| path.to_lowercase().contains(&filter.to_lowercase()))
                        .unwrap_or(false);
                    let state_matches = scan.state.to_lowercase().contains(&filter.to_lowercase());
                    root_path_matches || state_matches
                });
            }
        }

        if let Some(sort) = &params.sort {
            match sort.as_str() {
                "id" => filtered_scans.sort_by(|a, b| a.id.cmp(&b.id)),
                "id_desc" => filtered_scans.sort_by(|a, b| b.id.cmp(&a.id)),
                "scan_time" => filtered_scans.sort_by(|a, b| a.scan_time.cmp(&b.scan_time)),
                "scan_time_desc" => filtered_scans.sort_by(|a, b| b.scan_time.cmp(&a.scan_time)),
                "state" => filtered_scans.sort_by(|a, b| a.state.cmp(&b.state)),
                "state_desc" => filtered_scans.sort_by(|a, b| b.state.cmp(&a.state)),
                "root_path" => filtered_scans.sort_by(|a, b| a.root_path.cmp(&b.root_path)),
                "root_path_desc" => filtered_scans.sort_by(|a, b| b.root_path.cmp(&a.root_path)),
                "file_count" => filtered_scans.sort_by(|a, b| a.file_count.cmp(&b.file_count)),
                "file_count_desc" => filtered_scans.sort_by(|a, b| b.file_count.cmp(&a.file_count)),
                "folder_count" => filtered_scans.sort_by(|a, b| a.folder_count.cmp(&b.folder_count)),
                "folder_count_desc" => filtered_scans.sort_by(|a, b| b.folder_count.cmp(&a.folder_count)),
                _ => {}
            }
        }

        let page = params.page.unwrap_or(1).max(1);
        let limit = params.limit.unwrap_or(25).clamp(1, 100);
        let total = filtered_scans.len() as u32;
        let offset = ((page - 1) * limit) as usize;

        let items = filtered_scans
            .into_iter()
            .skip(offset)
            .take(limit as usize)
            .collect::<Vec<_>>();

        let response = ListResponse {
            items,
            total,
            page,
            limit,
            has_next: offset + (limit as usize) < total as usize,
            has_prev: page > 1,
        };

        Ok(Json(response))
    } else {
        // Use database-tier sorting and pagination (PROPER IMPLEMENTATION)
        let db = Database::new(db_path).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

        let query = ListQuery {
            filter: params.filter,
            sort: params.sort,
            page: params.page.unwrap_or(1).max(1),
            limit: params.limit.unwrap_or(25).clamp(1, 100),
        };

        let result = ScanRecord::list_paginated(&db, &query).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

        // Convert database records to API response format
        let items: Vec<ScanItem> = result.items.into_iter().map(|record| {
            ScanItem {
                id: record.id,
                root_id: record.root_id,
                root_path: record.root_path,
                state: record.state,
                scan_time: record.scan_time,
                file_count: record.file_count,
                folder_count: record.folder_count,
                duration_seconds: None, // TODO: Add duration calculation from scan metadata
            }
        }).collect();

        let response = ListResponse {
            items,
            total: result.total,
            page: result.page,
            limit: result.limit,
            has_next: result.has_next,
            has_prev: result.has_prev,
        };

        Ok(Json(response))
    }
}