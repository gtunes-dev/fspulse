use axum::{extract::Path, http::StatusCode, Extension, Json};
use log::{debug, error};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::database::Database;
use crate::error::FsPulseError;
use crate::query::QueryProcessor;

/// Request structure for query endpoint
/// Mirrors the web UI's column state and filter state
#[derive(Debug, Deserialize)]
pub struct QueryRequest {
    pub columns: Vec<ColumnSpec>,
    pub filters: Vec<FilterSpec>,
    pub limit: u32,
    pub offset: Option<u32>,
}

#[derive(Debug, Deserialize)]
pub struct ColumnSpec {
    pub name: String,
    pub visible: bool,
    pub sort_direction: String, // "none" | "asc" | "desc"
    pub position: usize,
}

#[derive(Debug, Deserialize)]
pub struct FilterSpec {
    pub column: String,
    pub value: String, // The filter expression as typed by user
}

/// Response structure for query results
#[derive(Debug, Serialize)]
pub struct QueryResponse {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<String>>,
    pub total: usize,
}

/// POST /api/query/{domain}
/// Accepts structured query data (columns, filters, limit) and executes an FsPulse query
pub async fn execute_query(
    Path(domain): Path<String>,
    Extension(db_path): Extension<Option<PathBuf>>,
    Json(req): Json<QueryRequest>,
) -> Result<Json<QueryResponse>, StatusCode> {
    // Create database connection
    let db = Database::new(db_path).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    // Build the FsPulse query string from the request
    let query_str = match build_query_string(&domain, &req) {
        Ok(s) => s,
        Err(e) => {
            error!("Failed to build query string: {}", e);
            return Err(StatusCode::BAD_REQUEST);
        }
    };

    debug!("Generated query: {}", query_str);

    // Build count query (just domain + filters, no SHOW/ORDER/LIMIT/OFFSET)
    let count_query_str = match build_count_query_string(&domain, &req) {
        Ok(s) => s,
        Err(e) => {
            error!("Failed to build count query: {}", e);
            return Err(StatusCode::BAD_REQUEST);
        }
    };

    debug!("Count query: {}", count_query_str);

    // Get total count first
    let total_count = match QueryProcessor::execute_query_count(&db, &count_query_str) {
        Ok(count) => count as usize,
        Err(e) => {
            error!("Count query failed: {}", e);
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    };

    // Execute the main query with LIMIT/OFFSET to get page data
    match QueryProcessor::execute_query(&db, &query_str) {
        Ok(rows) => {
            // Extract column names from first row if available
            let columns = if !rows.is_empty() {
                req.columns
                    .iter()
                    .filter(|c| c.visible)
                    .map(|c| c.name.clone())
                    .collect()
            } else {
                Vec::new()
            };

            Ok(Json(QueryResponse {
                total: total_count,
                columns,
                rows,
            }))
        }
        Err(e) => {
            error!("Query execution failed: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// Builds a count query with just domain and filters (no SHOW, ORDER BY, LIMIT, OFFSET)
/// Used to get total count of matching rows for pagination
fn build_count_query_string(domain: &str, req: &QueryRequest) -> Result<String, FsPulseError> {
    let mut query = domain.to_lowercase();

    // Build WHERE clause from filters (same as main query)
    if !req.filters.is_empty() {
        query.push_str(" where ");
        for (i, filter) in req.filters.iter().enumerate() {
            if i > 0 {
                query.push_str(", ");
            }
            query.push_str(&filter.column);
            query.push_str(":(");
            query.push_str(&filter.value);
            query.push(')');
        }
    }

    // No SHOW, ORDER BY, LIMIT, or OFFSET for count query
    Ok(query)
}

/// Builds an FsPulse query string from structured request data
/// Pattern follows TUI's build_query_and_columns() in src/explore/explorer.rs:610-692
fn build_query_string(domain: &str, req: &QueryRequest) -> Result<String, FsPulseError> {
    let mut query = domain.to_lowercase();

    // Build WHERE clause from filters
    if !req.filters.is_empty() {
        query.push_str(" where ");
        for (i, filter) in req.filters.iter().enumerate() {
            if i > 0 {
                query.push_str(", ");
            }
            // Format: column_name:(filter_value)
            query.push_str(&filter.column);
            query.push_str(":(");
            query.push_str(&filter.value);
            query.push(')');
        }
    }

    // Build SHOW clause from visible columns
    // Sort by position to maintain user's column order
    let mut visible_columns: Vec<&ColumnSpec> = req
        .columns
        .iter()
        .filter(|c| c.visible)
        .collect();
    visible_columns.sort_by_key(|c| c.position);

    if !visible_columns.is_empty() {
        query.push_str(" show ");
        for (i, col) in visible_columns.iter().enumerate() {
            if i > 0 {
                query.push_str(", ");
            }
            query.push_str(&col.name);
        }
    }

    // Build ORDER BY clause from columns with sort direction
    // Only include visible columns (SQL requires ORDER BY columns to be in SELECT list)
    let mut sorted_columns: Vec<&ColumnSpec> = req
        .columns
        .iter()
        .filter(|c| c.visible && c.sort_direction != "none")
        .collect();
    sorted_columns.sort_by_key(|c| c.position);

    if !sorted_columns.is_empty() {
        query.push_str(" order by ");
        for (i, col) in sorted_columns.iter().enumerate() {
            if i > 0 {
                query.push_str(", ");
            }
            query.push_str(&col.name);
            query.push(' ');
            query.push_str(&col.sort_direction);
        }
    }

    // Build LIMIT clause
    if req.limit > 0 {
        query.push_str(" limit ");
        query.push_str(&req.limit.to_string());
    }

    // Build OFFSET clause (comes after LIMIT in SQL)
    if let Some(offset) = req.offset {
        if offset > 0 {
            query.push_str(" offset ");
            query.push_str(&offset.to_string());
        }
    }

    Ok(query)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_query_basic() {
        let req = QueryRequest {
            columns: vec![
                ColumnSpec {
                    name: "item_id".to_string(),
                    visible: true,
                    sort_direction: "none".to_string(),
                    position: 0,
                },
                ColumnSpec {
                    name: "item_path".to_string(),
                    visible: true,
                    sort_direction: "asc".to_string(),
                    position: 1,
                },
            ],
            filters: vec![],
            limit: 25,
            offset: None,
        };

        let query = build_query_string("items", &req).unwrap();
        assert_eq!(query, "items show item_id, item_path order by item_path asc limit 25");
    }

    #[test]
    fn test_build_query_with_filters() {
        let req = QueryRequest {
            columns: vec![ColumnSpec {
                name: "item_path".to_string(),
                visible: true,
                sort_direction: "none".to_string(),
                position: 0,
            }],
            filters: vec![
                FilterSpec {
                    column: "item_path".to_string(),
                    value: "'invoice'".to_string(),
                },
                FilterSpec {
                    column: "item_type".to_string(),
                    value: "F".to_string(),
                },
            ],
            limit: 10,
            offset: None,
        };

        let query = build_query_string("items", &req).unwrap();
        assert_eq!(
            query,
            "items where item_path:('invoice'), item_type:(F) show item_path limit 10"
        );
    }

    #[test]
    fn test_build_query_hidden_columns_excluded() {
        let req = QueryRequest {
            columns: vec![
                ColumnSpec {
                    name: "item_id".to_string(),
                    visible: false,
                    sort_direction: "asc".to_string(),
                    position: 0,
                },
                ColumnSpec {
                    name: "item_path".to_string(),
                    visible: true,
                    sort_direction: "desc".to_string(),
                    position: 1,
                },
            ],
            filters: vec![],
            limit: 5,
            offset: None,
        };

        let query = build_query_string("items", &req).unwrap();
        // Hidden column should not appear in SHOW or ORDER BY
        assert_eq!(query, "items show item_path order by item_path desc limit 5");
    }

    #[test]
    fn test_build_query_with_offset() {
        let req = QueryRequest {
            columns: vec![ColumnSpec {
                name: "item_id".to_string(),
                visible: true,
                sort_direction: "asc".to_string(),
                position: 0,
            }],
            filters: vec![],
            limit: 25,
            offset: Some(50),
        };

        let query = build_query_string("items", &req).unwrap();
        assert_eq!(query, "items show item_id order by item_id asc limit 25 offset 50");
    }

    #[test]
    fn test_build_query_with_zero_offset() {
        let req = QueryRequest {
            columns: vec![ColumnSpec {
                name: "item_id".to_string(),
                visible: true,
                sort_direction: "none".to_string(),
                position: 0,
            }],
            filters: vec![],
            limit: 10,
            offset: Some(0),
        };

        let query = build_query_string("items", &req).unwrap();
        // Zero offset should not be included in query
        assert_eq!(query, "items show item_id limit 10");
    }

    #[test]
    fn test_build_count_query_basic() {
        let req = QueryRequest {
            columns: vec![],
            filters: vec![],
            limit: 25,
            offset: None,
        };

        let query = build_count_query_string("items", &req).unwrap();
        assert_eq!(query, "items");
    }

    #[test]
    fn test_build_count_query_with_filters() {
        let req = QueryRequest {
            columns: vec![],
            filters: vec![
                FilterSpec {
                    column: "item_type".to_string(),
                    value: "F".to_string(),
                },
            ],
            limit: 25,
            offset: None,
        };

        let query = build_count_query_string("items", &req).unwrap();
        assert_eq!(query, "items where item_type:(F)");
    }

    #[test]
    fn test_build_count_query_with_multiple_filters() {
        let req = QueryRequest {
            columns: vec![],
            filters: vec![
                FilterSpec {
                    column: "item_type".to_string(),
                    value: "F".to_string(),
                },
                FilterSpec {
                    column: "item_path".to_string(),
                    value: "'Documents'".to_string(),
                },
            ],
            limit: 25,
            offset: None,
        };

        let query = build_count_query_string("items", &req).unwrap();
        assert_eq!(query, "items where item_type:(F), item_path:('Documents')");
    }

    #[test]
    fn test_build_count_query_ignores_columns_and_sort() {
        let req = QueryRequest {
            columns: vec![
                ColumnSpec {
                    name: "item_id".to_string(),
                    visible: true,
                    sort_direction: "asc".to_string(),
                    position: 0,
                },
                ColumnSpec {
                    name: "item_path".to_string(),
                    visible: true,
                    sort_direction: "desc".to_string(),
                    position: 1,
                },
            ],
            filters: vec![],
            limit: 25,
            offset: Some(50),
        };

        let query = build_count_query_string("items", &req).unwrap();
        // Count query should ignore columns, sort, limit, and offset
        assert_eq!(query, "items");
    }
}
