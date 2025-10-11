use axum::{
    extract::Query,
    http::StatusCode,
    response::Json,
    Extension,
};
use std::path::PathBuf;

use crate::config::CONFIG;
use crate::database::{Database, ListQuery};
use crate::items::Item;

use super::common::{ListParams, ListResponse};

pub async fn list_items(
    Query(params): Query<ListParams>,
    Extension(db_path): Extension<Option<PathBuf>>
) -> Result<Json<ListResponse<Item>>, StatusCode> {
    let config = CONFIG.get().ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;

    if config.web.use_mock_data {
        // Mock data for development/testing (keep for backwards compatibility)
        let all_items = vec![
            create_mock_item(1, 1, "/Users/john/Documents/report.pdf", "F", 156789, 245632),
            create_mock_item(2, 1, "/Users/john/Documents/notes.txt", "F", 156789, 1024),
            create_mock_item(3, 1, "/Users/john/Documents/Projects", "D", 156789, 0),
            create_mock_item(4, 1, "/Users/john/Documents/Projects/fspulse", "D", 156790, 0),
            create_mock_item(5, 1, "/Users/john/Documents/Projects/fspulse/README.md", "F", 156790, 4821),
            create_mock_item(6, 1, "/Users/john/Documents/Projects/fspulse/Cargo.toml", "F", 156790, 892),
            create_mock_item(7, 2, "/Users/john/Downloads/installer.dmg", "F", 156788, 98765432),
            create_mock_item(8, 2, "/Users/john/Downloads/image.png", "F", 156788, 2458963),
            create_mock_item(9, 2, "/Users/john/Downloads/archive.zip", "F", 156787, 15632478),
            create_mock_item(10, 3, "/Users/john/Pictures/vacation", "D", 156785, 0),
            create_mock_item(11, 3, "/Users/john/Pictures/vacation/beach.jpg", "F", 156785, 3845672),
            create_mock_item(12, 3, "/Users/john/Pictures/vacation/sunset.jpg", "F", 156785, 4123890),
            create_mock_item(13, 3, "/Users/john/Pictures/family", "D", 156784, 0),
            create_mock_item(14, 3, "/Users/john/Pictures/family/reunion.jpg", "F", 156784, 5632147),
            create_mock_item(15, 1, "/Users/john/Documents/spreadsheet.xlsx", "F", 156789, 87456),
        ];

        // Apply the old application-tier filtering/sorting for mock data
        let mut filtered_items = all_items;
        if let Some(filter) = &params.filter {
            if !filter.is_empty() {
                filtered_items.retain(|item| {
                    item.item_path().to_lowercase().contains(&filter.to_lowercase())
                });
            }
        }

        if let Some(sort) = &params.sort {
            match sort.as_str() {
                "path" => filtered_items.sort_by(|a, b| a.item_path().cmp(b.item_path())),
                "path_desc" => filtered_items.sort_by(|a, b| b.item_path().cmp(a.item_path())),
                "type" => filtered_items.sort_by(|a, b| a.item_type().cmp(b.item_type())),
                "type_desc" => filtered_items.sort_by(|a, b| b.item_type().cmp(a.item_type())),
                "size" => filtered_items.sort_by_key(|a| a.file_size()),
                "size_desc" => filtered_items.sort_by_key(|b| std::cmp::Reverse(b.file_size())),
                "mod_date" => filtered_items.sort_by_key(|a| a.mod_date()),
                "mod_date_desc" => filtered_items.sort_by_key(|b| std::cmp::Reverse(b.mod_date())),
                "id" => filtered_items.sort_by_key(|a| a.item_id()),
                "id_desc" => filtered_items.sort_by_key(|b| std::cmp::Reverse(b.item_id())),
                _ => {}
            }
        }

        let page = params.page.unwrap_or(1).max(1);
        let limit = params.limit.unwrap_or(25).clamp(1, 100);
        let total = filtered_items.len() as u32;
        let offset = ((page - 1) * limit) as usize;

        let items = filtered_items
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

        let result = Item::list_paginated(&db, &query).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

        let response = ListResponse {
            items: result.items,
            total: result.total,
            page: result.page,
            limit: result.limit,
            has_next: result.has_next,
            has_prev: result.has_prev,
        };

        Ok(Json(response))
    }
}

// Helper function to create mock items for testing
fn create_mock_item(item_id: i64, root_id: i64, path: &str, item_type: &str, mod_date: i64, file_size: i64) -> Item {
    // We need to use serde_json to create Item instances since fields are private
    // This is a bit of a workaround for the mock data
    let json_str = format!(
        r#"{{
            "id": {},
            "root_id": {},
            "path": "{}",
            "type": "{}",
            "last_scan": 1,
            "is_ts": false,
            "mod_date": {},
            "file_size": {},
            "last_hash_scan": null,
            "file_hash": null,
            "last_val_scan": null,
            "val": "U",
            "val_error": null
        }}"#,
        item_id, root_id, path, item_type,
        if item_type == "D" { "null" } else { &mod_date.to_string() },
        if item_type == "D" { "null" } else { &file_size.to_string() }
    );
    serde_json::from_str(&json_str).unwrap()
}
