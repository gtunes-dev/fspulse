use axum::{extract::Path, http::StatusCode, Json};
use serde::Serialize;

use crate::query::columns::{
    ColAlign, ColMap, ColSpec, ALERTS_QUERY_COLS, CHANGES_QUERY_COLS, ITEMS_QUERY_COLS,
    ROOTS_QUERY_COLS, SCANS_QUERY_COLS,
};

#[derive(Serialize)]
pub struct MetadataResponse {
    pub domain: String,
    pub columns: Vec<ColumnMetadata>,
}

#[derive(Serialize)]
pub struct ColumnMetadata {
    pub name: String,
    pub display_name: String,
    pub col_type: String,
    pub alignment: String,
    pub is_default: bool,
    pub filter_info: FilterInfo,
}

#[derive(Serialize)]
pub struct FilterInfo {
    pub type_name: String,
    pub syntax_hint: String,
}

pub async fn get_metadata(Path(domain): Path<String>) -> Result<Json<MetadataResponse>, StatusCode> {
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
