use axum::{
    extract::Query,
    http::StatusCode,
    response::Json,
    Extension,
};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::config::CONFIG;
use crate::database::{Database, ListQuery};
use crate::roots::Root;

#[derive(Deserialize)]
pub struct ListParams {
    pub page: Option<u32>,
    pub limit: Option<u32>,
    pub sort: Option<String>,
    pub filter: Option<String>,
}

#[derive(Serialize)]
pub struct ListResponse<T> {
    pub items: Vec<T>,
    pub total: u32,
    pub page: u32,
    pub limit: u32,
    pub has_next: bool,
    pub has_prev: bool,
}

pub async fn list_roots(
    Query(params): Query<ListParams>,
    Extension(db_path): Extension<Option<PathBuf>>
) -> Result<Json<ListResponse<Root>>, StatusCode> {
    let config = CONFIG.get().ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;

    if config.web.use_mock_data {
        // Mock data for development/testing (keep for backwards compatibility)
        let all_roots = vec![
            Root::new(1, "/Users/john/Documents".to_string()),
            Root::new(2, "/Users/john/Downloads".to_string()),
            Root::new(3, "/Users/john/Pictures".to_string()),
            Root::new(4, "/var/log".to_string()),
            Root::new(5, "/opt/applications".to_string()),
        ];

        // Apply the old application-tier filtering/sorting for mock data
        let mut filtered_roots = all_roots;
        if let Some(filter) = &params.filter {
            if !filter.is_empty() {
                filtered_roots.retain(|root| {
                    root.root_path().to_lowercase().contains(&filter.to_lowercase())
                });
            }
        }

        if let Some(sort) = &params.sort {
            match sort.as_str() {
                "path" => filtered_roots.sort_by(|a, b| a.root_path().cmp(b.root_path())),
                "path_desc" => filtered_roots.sort_by(|a, b| b.root_path().cmp(a.root_path())),
                "id" => filtered_roots.sort_by_key(|a| a.root_id()),
                "id_desc" => filtered_roots.sort_by_key(|b| std::cmp::Reverse(b.root_id())),
                _ => {}
            }
        }

        let page = params.page.unwrap_or(1).max(1);
        let limit = params.limit.unwrap_or(25).clamp(1, 100);
        let total = filtered_roots.len() as u32;
        let offset = ((page - 1) * limit) as usize;

        let items = filtered_roots
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

        let result = Root::list_paginated(&db, &query).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

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