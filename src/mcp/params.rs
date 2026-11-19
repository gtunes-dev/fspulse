// ─── Parameter structs ──────────────────────────────────────────────
//
// Times are strings: "now", a span back from now ("24h", "7d", "2w"),
// a local date ("2026-11-17"), a local date and time
// ("2026-11-17 14:30"), or Unix seconds. See `formatting::parse_time`.

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct QueryDataParams {
    /// The fspulse query string. Domains: roots, items, versions, hashes, snapshots, scans.
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
    /// Optional domain: "roots", "items", "versions", "hashes", "snapshots", or "scans"
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
pub struct IntegrityCheckHistoryParams {
    /// Root ID
    pub root_id: i64,
    /// Maximum integrity checks to return (default 50, max 200)
    pub limit: Option<i64>,
    /// Number of integrity checks to skip for pagination (default 0)
    pub offset: Option<i64>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct ActivityParams {
    /// Root ID
    pub root_id: i64,
    /// Start of the period: "24h", "7d", "2w", a date, a date and time, or Unix seconds. Default: 7d.
    pub from: Option<String>,
    /// End of the period, same forms. Default: now.
    pub to: Option<String>,
    /// "hour" lists each hour that had changes; "day" adds them up per local day. Default: "day".
    pub bucket: Option<String>,
    /// Maximum rows to return, most recent first (default 50, max 200)
    pub limit: Option<i64>,
    /// Number of rows to skip for pagination (default 0)
    pub offset: Option<i64>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct ChangesParams {
    /// Root ID
    pub root_id: i64,
    /// Start of the range: "24h", "7d", "2w", a date, a date and time, or Unix seconds. Required. Moved back to the hour.
    pub from: String,
    /// End of the range, same forms. Default: now. An end in the past is moved back to the hour.
    pub to: Option<String>,
    /// Only changes beneath this folder (absolute path). Omit for the whole root.
    pub folder_path: Option<String>,
    /// Filter by change type: "added", "modified", "deleted", "ignored" (stopped being tracked because of an ignore pattern), "resumed" (tracked again after being ignored), "transient" (appeared and disappeared inside the range), or "all" (everything but transient; the default)
    pub change_type: Option<String>,
    /// Maximum items to return (default 50, max 200)
    pub limit: Option<i64>,
    /// Number of items to skip for pagination (default 0)
    pub offset: Option<i64>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct BrowseFilesystemParams {
    /// Root ID
    pub root_id: i64,
    /// Parent directory path to list children of. Omit for root directory.
    pub parent_path: Option<String>,
    /// The time to show the folder as of: "now", "24h", a date, a date and time, or Unix seconds. Default: now. A time in the past is moved back to the hour.
    pub at: Option<String>,
    /// Optional start of a comparison range, same forms. When given, each child shows how it changed between `from` and `at`, folders show the changes beneath them, and children deleted in the range are listed.
    pub from: Option<String>,
    /// Also list items that are no longer tracked because they match one of the root's ignore patterns (default false). They are marked "ignored" with the time tracking stopped; nothing is known about them after that.
    pub include_ignored: Option<bool>,
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
    /// The time to search as of: "now", "24h", a date, a date and time, or Unix seconds. Default: now.
    pub at: Option<String>,
    /// Optional start of a comparison range, same forms. When given, results show how each item changed and include items deleted in the range.
    pub from: Option<String>,
    /// Also find items that are no longer tracked because they match one of the root's ignore patterns (default false).
    pub include_ignored: Option<bool>,
    /// Maximum items to return (default 50, max 200)
    pub limit: Option<i64>,
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
