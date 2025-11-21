use axum::{extract::Path, http::StatusCode, Json};
use log::{debug, error};
use serde::{Deserialize, Serialize};

use crate::error::FsPulseError;
use crate::query::columns::{
    ColMap, ColSpec, ColType, ALERTS_QUERY_COLS, CHANGES_QUERY_COLS, ITEMS_QUERY_COLS,
    ROOTS_QUERY_COLS, SCANS_QUERY_COLS,
};
use crate::query::{ColAlign, QueryProcessor};

/// Request structure for count/fetch endpoints
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

/// Response structure for count endpoint
#[derive(Debug, Serialize)]
pub struct CountResponse {
    pub count: i64,
}

/// Response structure for fetch endpoint
#[derive(Debug, Serialize)]
pub struct FetchResponse {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<String>>,
}

/// Column metadata from domain column definitions
#[derive(Debug, Serialize)]
pub struct ColumnMetadata {
    pub name: String,
    pub display_name: String,
    pub col_type: String,
    pub alignment: String,
    pub is_default: bool,
    pub filter_info: FilterInfo,
}

#[derive(Debug, Serialize)]
pub struct FilterInfo {
    pub type_name: String,
    pub syntax_hint: String,
}

#[derive(Debug, Serialize)]
pub struct MetadataResponse {
    pub domain: String,
    pub columns: Vec<ColumnMetadata>,
}

/// Request structure for raw query count (for Query tab)
#[derive(Debug, Deserialize)]
pub struct RawCountRequest {
    pub query: String,
}

/// Request structure for raw query execution (for Query tab)
#[derive(Debug, Deserialize)]
pub struct RawQueryRequest {
    pub query: String,
    pub limit_override: i64,
    pub offset_add: i64,
}

/// Response structure for raw query results
#[derive(Debug, Serialize)]
pub struct RawQueryResponse {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<String>>,
    pub alignments: Vec<ColAlign>,
}

/// Request structure for filter validation
#[derive(Debug, Deserialize)]
pub struct ValidateFilterRequest {
    pub domain: String,
    pub column: String,
    pub value: String,
}

/// Response structure for filter validation
#[derive(Debug, Serialize)]
pub struct ValidateFilterResponse {
    pub valid: bool,
    pub error: Option<String>,
}

/// GET /api/query/{domain}/metadata
/// Returns column metadata for the specified domain
pub async fn get_metadata(
    Path(domain): Path<String>,
) -> Result<Json<MetadataResponse>, StatusCode> {
    let col_map = match domain.as_str() {
        "alerts" => &ALERTS_QUERY_COLS,
        "items" => &ITEMS_QUERY_COLS,
        "changes" => &CHANGES_QUERY_COLS,
        "scans" => &SCANS_QUERY_COLS,
        "roots" => &ROOTS_QUERY_COLS,
        _ => return Err(StatusCode::NOT_FOUND),
    };

    let columns = map_col_map_to_metadata(col_map);

    Ok(Json(MetadataResponse {
        domain: domain.clone(),
        columns,
    }))
}

fn map_col_map_to_metadata(col_map: &ColMap) -> Vec<ColumnMetadata> {
    col_map
        .entries()
        .map(|(name, spec)| map_col_spec_to_metadata(name, spec))
        .collect()
}

fn map_col_spec_to_metadata(name: &str, spec: &ColSpec) -> ColumnMetadata {
    let col_type_info = spec.col_type.info();

    ColumnMetadata {
        name: name.to_string(),
        display_name: spec.name_display.to_string(),
        col_type: format!("{:?}", spec.col_type),
        alignment: alignment_to_string(&spec.col_align),
        is_default: spec.is_default,
        filter_info: FilterInfo {
            type_name: col_type_info.type_name.to_string(),
            syntax_hint: col_type_info.tip.to_string(),
        },
    }
}

fn alignment_to_string(align: &ColAlign) -> String {
    match align {
        ColAlign::Left => "Left",
        ColAlign::Center => "Center",
        ColAlign::Right => "Right",
    }
    .to_string()
}

/// POST /api/query/{domain}/count
/// Returns count of matching rows without fetching the actual data
pub async fn count_query(
    Path(domain): Path<String>,
    Json(req): Json<QueryRequest>,
) -> Result<Json<CountResponse>, StatusCode> {
    // Build count query (just domain + filters, no SHOW/ORDER/LIMIT/OFFSET)
    let count_query_str = match build_count_query_string(&domain, &req) {
        Ok(s) => s,
        Err(e) => {
            error!("Failed to build count query: {}", e);
            return Err(StatusCode::BAD_REQUEST);
        }
    };

    debug!("Count query: {}", count_query_str);

    // Execute count query
    let count = match QueryProcessor::execute_query_count(&count_query_str) {
        Ok(count) => count,
        Err(e) => {
            error!("Count query failed: {}", e);
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    };

    Ok(Json(CountResponse { count }))
}

/// POST /api/query/{domain}/fetch
/// Fetches a page of rows without counting
pub async fn fetch_query(
    Path(domain): Path<String>,
    Json(req): Json<QueryRequest>,
) -> Result<Json<FetchResponse>, StatusCode> {
    // Build full query with SHOW/ORDER/LIMIT/OFFSET
    let query_str = match build_query_string(&domain, &req) {
        Ok(s) => s,
        Err(e) => {
            error!("Failed to build query string: {}", e);
            return Err(StatusCode::BAD_REQUEST);
        }
    };

    debug!("Fetch query: {}", query_str);

    // Execute fetch query
    match QueryProcessor::execute_query(&query_str) {
        Ok((rows, column_headers, _alignments)) => Ok(Json(FetchResponse {
            columns: column_headers,
            rows,
        })),
        Err(e) => {
            error!("Fetch query failed: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// POST /api/query/fetch_override
/// Accepts raw FsPulse query text and executes it with limit/offset overrides (for Query tab)
pub async fn fetch_override_query(
    Json(req): Json<RawQueryRequest>,
) -> Result<Json<RawQueryResponse>, (StatusCode, String)> {
    debug!(
        "Executing raw query: {} (limit_override: {}, offset_add: {})",
        req.query, req.limit_override, req.offset_add
    );

    match QueryProcessor::execute_query_override(&req.query, req.limit_override, req.offset_add) {
        Ok((rows, column_headers, alignments)) => Ok(Json(RawQueryResponse {
            columns: column_headers,
            rows,
            alignments,
        })),
        Err(e) => {
            let error_msg = e.to_string();
            error!("Raw query execution failed: {}", error_msg);
            Err((StatusCode::BAD_REQUEST, error_msg))
        }
    }
}

/// POST /api/query/count_raw
/// Returns count for a raw FsPulse query (for Query tab)
pub async fn count_raw_query(
    Json(req): Json<RawCountRequest>,
) -> Result<Json<CountResponse>, (StatusCode, String)> {
    debug!("Counting raw query: {}", req.query);

    match QueryProcessor::execute_query_count(&req.query) {
        Ok(count) => Ok(Json(CountResponse { count })),
        Err(e) => {
            let error_msg = e.to_string();
            error!("Raw count query failed: {}", error_msg);
            Err((StatusCode::BAD_REQUEST, error_msg))
        }
    }
}

/// POST /api/validate-filter
/// Validates a filter value for a given column in a domain
pub async fn validate_filter(
    Json(req): Json<ValidateFilterRequest>,
) -> Result<Json<ValidateFilterResponse>, StatusCode> {
    let col_map = match req.domain.as_str() {
        "alerts" => &ALERTS_QUERY_COLS,
        "items" => &ITEMS_QUERY_COLS,
        "changes" => &CHANGES_QUERY_COLS,
        "scans" => &SCANS_QUERY_COLS,
        "roots" => &ROOTS_QUERY_COLS,
        _ => return Err(StatusCode::NOT_FOUND),
    };

    let col_spec = col_map.get(&req.column);
    if col_spec.is_none() {
        return Err(StatusCode::NOT_FOUND);
    }

    let col_spec = col_spec.unwrap();
    let col_type_info = col_spec.col_type.info();

    match QueryProcessor::validate_filter(col_type_info.rule, &req.value) {
        Some(error_msg) => Ok(Json(ValidateFilterResponse {
            valid: false,
            error: Some(error_msg),
        })),
        None => Ok(Json(ValidateFilterResponse {
            valid: true,
            error: None,
        })),
    }
}

/// Builds a count query with just domain and filters (no SHOW, ORDER BY, LIMIT, OFFSET)
fn build_count_query_string(domain: &str, req: &QueryRequest) -> Result<String, FsPulseError> {
    let mut query = domain.to_lowercase();

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

    Ok(query)
}

/// Get column map for a domain
fn get_col_map(domain: &str) -> Option<&'static ColMap> {
    match domain {
        "alerts" => Some(&ALERTS_QUERY_COLS),
        "items" => Some(&ITEMS_QUERY_COLS),
        "changes" => Some(&CHANGES_QUERY_COLS),
        "scans" => Some(&SCANS_QUERY_COLS),
        "roots" => Some(&ROOTS_QUERY_COLS),
        _ => None,
    }
}

/// Check if a column is a date type
fn is_date_column(col_map: &ColMap, col_name: &str) -> bool {
    col_map
        .get(col_name)
        .map(|spec| matches!(spec.col_type, ColType::Date))
        .unwrap_or(false)
}

/// Builds a full FsPulse query string from structured request data
fn build_query_string(domain: &str, req: &QueryRequest) -> Result<String, FsPulseError> {
    let mut query = domain.to_lowercase();

    // Get column map for checking date columns
    let col_map =
        get_col_map(domain).ok_or_else(|| FsPulseError::Error("Invalid domain".into()))?;

    // Build WHERE clause from filters
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

    // Build SHOW clause from visible columns
    let mut visible_columns: Vec<&ColumnSpec> = req.columns.iter().filter(|c| c.visible).collect();
    visible_columns.sort_by_key(|c| c.position);

    if !visible_columns.is_empty() {
        query.push_str(" show ");
        for (i, col) in visible_columns.iter().enumerate() {
            if i > 0 {
                query.push_str(", ");
            }
            query.push_str(&col.name);

            // Add @timestamp format modifier for date columns
            if is_date_column(col_map, &col.name) {
                query.push_str("@timestamp");
            }
        }
    }

    // Build ORDER BY clause from columns with sort direction
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

    // Build OFFSET clause
    if let Some(offset) = req.offset {
        if offset > 0 {
            query.push_str(" offset ");
            query.push_str(&offset.to_string());
        }
    }

    Ok(query)
}
