use serde::{Deserialize, Serialize};

/// Common query parameters for listing paginated resources
#[derive(Deserialize)]
pub struct ListParams {
    pub page: Option<u32>,
    pub limit: Option<u32>,
    pub sort: Option<String>,
    pub filter: Option<String>,
}

/// Common response structure for paginated lists
#[derive(Serialize)]
pub struct ListResponse<T> {
    pub items: Vec<T>,
    pub total: u32,
    pub page: u32,
    pub limit: u32,
    pub has_next: bool,
    pub has_prev: bool,
}
