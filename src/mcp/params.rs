// ─── Parameter structs ──────────────────────────────────────────────

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct QueryDataParams {
    /// The fspulse query string. Domains: roots, scans, items, versions, hashes.
    /// Use LIMIT and OFFSET in the query for pagination (e.g. "items where root_id:(1) limit 50 offset 100").
    /// Results are capped at 200 rows. Use query_count to get total row counts before paginating.
    pub query: String,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct QueryCountParams {
    /// The fspulse query string
    pub query: String,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct QueryHelpParams {
    /// Optional domain: "roots", "scans", "items", "versions", or "hashes"
    pub domain: Option<String>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct IntegrityReportParams {
    /// Root ID to check
    pub root_id: i64,
    /// Filter by issue type: "val", "hash", or "all" (default: "all")
    pub issue_type: Option<String>,
    /// Filter by review status: "unreviewed", "reviewed", or "all" (default: "unreviewed")
    pub status: Option<String>,
    /// Filter by file extensions, comma-separated (e.g., "pdf,jpg")
    pub extensions: Option<String>,
    /// Search substring in file paths
    pub path_search: Option<String>,
    /// Maximum items to return (default 50, max 200)
    pub limit: Option<i64>,
    /// Number of items to skip for pagination (default 0)
    pub offset: Option<i64>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct ScanHistoryParams {
    /// Root ID
    pub root_id: i64,
    /// Maximum scans to return (default 50, max 200)
    pub limit: Option<i64>,
    /// Number of scans to skip for pagination (default 0)
    pub offset: Option<i64>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct BrowseFilesystemParams {
    /// Root ID
    pub root_id: i64,
    /// Parent directory path to list children of. Omit for root directory.
    pub parent_path: Option<String>,
    /// Scan ID for temporal view. Omit for latest scan.
    pub scan_id: Option<i64>,
    /// Maximum items to return (default 50, max 200)
    pub limit: Option<i64>,
    /// Number of items to skip for pagination (default 0)
    pub offset: Option<i64>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct SearchFilesParams {
    /// Root ID
    pub root_id: i64,
    /// Search term (matches against file/directory name)
    pub query: String,
    /// Scan ID for temporal view. Omit for latest scan.
    pub scan_id: Option<i64>,
    /// Maximum items to return (default 50, max 200)
    pub limit: Option<i64>,
    /// Number of items to skip for pagination (default 0)
    pub offset: Option<i64>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct ItemDetailParams {
    /// Item ID
    pub item_id: i64,
    /// Maximum versions to show (default 50, max 200)
    pub limit: Option<i64>,
    /// Number of versions to skip for pagination (default 0)
    pub offset: Option<i64>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct ScanChangesParams {
    /// Scan ID
    pub scan_id: i64,
    /// Filter by change type: "added", "modified", "deleted", or "all" (default: "all")
    pub change_type: Option<String>,
    /// Maximum items to return (default 50, max 200)
    pub limit: Option<i64>,
    /// Number of items to skip for pagination (default 0)
    pub offset: Option<i64>,
}
