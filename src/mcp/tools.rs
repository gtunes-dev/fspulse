use rmcp::{
    ServerHandler,
    model::{CallToolResult, Content, ServerInfo, ServerCapabilities, ToolsCapability, Implementation},
    tool, tool_router, tool_handler,
};
use rmcp::handler::server::wrapper::Parameters;

use crate::db::Database;
use crate::query::QueryProcessor;

use super::formatting::format_table;

// ─── Parameter structs ──────────────────────────────────────────────

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct QueryDataParams {
    /// The fspulse query string. Domains: roots, scans, items, versions, hashes.
    /// Example: "versions where is_current:(T), root_id:(1) show item_path, size limit 20"
    pub query: String,
    /// Maximum rows to return (default 50, max 500)
    pub max_rows: Option<i64>,
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
    /// Maximum items to return (default 30, max 100)
    pub limit: Option<i64>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct ScanHistoryParams {
    /// Root ID
    pub root_id: i64,
    /// Maximum scans to return (default 20, max 100)
    pub limit: Option<i64>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct BrowseFilesystemParams {
    /// Root ID
    pub root_id: i64,
    /// Parent directory path to list children of. Omit for root directory.
    pub parent_path: Option<String>,
    /// Scan ID for temporal view. Omit for latest scan.
    pub scan_id: Option<i64>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct SearchFilesParams {
    /// Root ID
    pub root_id: i64,
    /// Search term (matches against file/directory name)
    pub query: String,
    /// Scan ID for temporal view. Omit for latest scan.
    pub scan_id: Option<i64>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct ItemDetailParams {
    /// Item ID
    pub item_id: i64,
    /// Maximum versions to show (default 10)
    pub max_versions: Option<i64>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct ScanChangesParams {
    /// Scan ID
    pub scan_id: i64,
    /// Filter by change type: "added", "modified", "deleted", or "all" (default: "all")
    pub change_type: Option<String>,
    /// Maximum items to return (default 50, max 200)
    pub limit: Option<i64>,
}

// ─── Tool router ────────────────────────────────────────────────────

#[derive(Clone)]
pub struct FsPulseMcp {
    tool_router: rmcp::handler::server::router::tool::ToolRouter<Self>,
}

#[tool_router]
impl FsPulseMcp {
    pub fn new() -> Self {
        Self {
            tool_router: Self::tool_router(),
        }
    }

    #[tool(description = "Get a high-level overview of the fspulse system: monitored roots, latest scan status, integrity issue counts, and database size.")]
    async fn system_overview(&self) -> Result<CallToolResult, rmcp::ErrorData> {
        let result = tokio::task::spawn_blocking(|| -> Result<String, String> {
            let conn = Database::get_connection().map_err(|e| e.to_string())?;

            // Get roots with latest scan info
            let mut stmt = conn
                .prepare(
                    "SELECT r.root_id, r.root_path,
                            s.scan_id, s.started_at, s.state,
                            s.file_count, s.folder_count, s.total_size
                     FROM roots r
                     LEFT JOIN scans s ON s.root_id = r.root_id
                        AND s.scan_id = (SELECT MAX(scan_id) FROM scans WHERE root_id = r.root_id)
                     ORDER BY r.root_path COLLATE natural_path",
                )
                .map_err(|e| e.to_string())?;

            let roots: Vec<String> = stmt
                .query_map([], |row| {
                    let root_id: i64 = row.get(0)?;
                    let root_path: String = row.get(1)?;
                    let scan_id: Option<i64> = row.get(2)?;
                    let file_count: Option<i64> = row.get(5)?;
                    let folder_count: Option<i64> = row.get(6)?;
                    let total_size: Option<i64> = row.get(7)?;

                    let scan_info = match scan_id {
                        Some(sid) => format!(
                            "last scan #{}, {} files, {} folders, {} bytes",
                            sid,
                            file_count.unwrap_or(0),
                            folder_count.unwrap_or(0),
                            total_size.unwrap_or(0)
                        ),
                        None => "no scans yet".to_string(),
                    };

                    Ok(format!("- Root {} ({}): {}", root_id, root_path, scan_info))
                })
                .map_err(|e| e.to_string())?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| e.to_string())?;

            // Count unreviewed integrity issues
            let hash_issues: i64 = conn
                .query_row(
                    "SELECT COUNT(DISTINCT iv.item_id) FROM item_versions iv
                     WHERE EXISTS (
                         SELECT 1 FROM hash_versions hv
                         WHERE hv.item_id = iv.item_id AND hv.item_version = iv.item_version
                         AND hv.hash_state = 2
                     ) AND iv.hash_reviewed_at IS NULL",
                    [],
                    |row| row.get(0),
                )
                .unwrap_or(0);

            let val_issues: i64 = conn
                .query_row(
                    "SELECT COUNT(DISTINCT iv.item_id) FROM item_versions iv
                     WHERE iv.val_state = 2 AND iv.val_reviewed_at IS NULL",
                    [],
                    |row| row.get(0),
                )
                .unwrap_or(0);

            // Database stats
            let db_path = Database::get_path().unwrap_or_default();
            let db_size = std::fs::metadata(&db_path)
                .map(|m| m.len())
                .unwrap_or(0);

            let mut out = String::new();
            out.push_str("## Monitored Roots\n\n");
            if roots.is_empty() {
                out.push_str("No roots configured.\n");
            } else {
                for root in &roots {
                    out.push_str(root);
                    out.push('\n');
                }
            }
            out.push_str(&format!(
                "\n## Integrity Issues (Unreviewed)\n\n- Suspect hashes: {}\n- Validation failures: {}\n",
                hash_issues, val_issues
            ));
            out.push_str(&format!(
                "\n## Database\n\n- Path: {}\n- Size: {} bytes\n",
                db_path.display(),
                db_size
            ));

            Ok(out)
        })
        .await
        .map_err(|e| rmcp::ErrorData::internal_error(e.to_string(), None))?
        .map_err(|e| rmcp::ErrorData::internal_error(e, None))?;

        Ok(CallToolResult::success(vec![Content::text(result)]))
    }

    #[tool(description = "Execute a fspulse query using the built-in DSL. Supports five domains: roots, scans, items, versions, hashes. Use query_help for syntax details. Returns results as a markdown table.")]
    async fn query_data(
        &self,
        Parameters(params): Parameters<QueryDataParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let query = params.query;
        let max_rows = params.max_rows.unwrap_or(50).clamp(1, 500);

        let result = tokio::task::spawn_blocking(move || {
            QueryProcessor::execute_query_override(&query, max_rows, 0)
        })
        .await
        .map_err(|e| rmcp::ErrorData::internal_error(e.to_string(), None))?;

        match result {
            Ok((rows, headers, alignments)) => {
                let table = format_table(&headers, &rows, &alignments);
                let summary = format!("{} row(s) returned.\n\n{}", rows.len(), table);
                Ok(CallToolResult::success(vec![Content::text(summary)]))
            }
            Err(e) => {
                let msg = format!(
                    "Query error: {}\n\nUse query_help to see available columns for each domain.",
                    e
                );
                Ok(CallToolResult::error(vec![Content::text(msg)]))
            }
        }
    }

    #[tool(description = "Count how many rows match a fspulse query without returning the data. Useful for understanding data volumes before fetching.")]
    async fn query_count(
        &self,
        Parameters(params): Parameters<QueryCountParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let query = params.query;

        let result = tokio::task::spawn_blocking(move || {
            QueryProcessor::execute_query_count(&query)
        })
        .await
        .map_err(|e| rmcp::ErrorData::internal_error(e.to_string(), None))?;

        match result {
            Ok(count) => Ok(CallToolResult::success(vec![Content::text(
                format!("{}", count),
            )])),
            Err(e) => {
                let msg = format!(
                    "Query error: {}\n\nUse query_help to see available columns for each domain.",
                    e
                );
                Ok(CallToolResult::error(vec![Content::text(msg)]))
            }
        }
    }

    #[tool(description = "Get documentation for the fspulse query DSL. Without a domain parameter, returns general syntax. With a domain, returns available columns and filter types.")]
    async fn query_help(
        &self,
        Parameters(params): Parameters<QueryHelpParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let text = match params.domain.as_deref() {
            None => general_help(),
            Some(domain) => domain_help(domain)?,
        };

        Ok(CallToolResult::success(vec![Content::text(text)]))
    }

    #[tool(description = "Get a report of integrity issues (validation failures, suspect hashes) for a monitored root.")]
    async fn integrity_report(
        &self,
        Parameters(params): Parameters<IntegrityReportParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let result = tokio::task::spawn_blocking(move || -> Result<String, String> {
            use crate::integrity::integrity_api;

            let extensions: Vec<String> = params
                .extensions
                .map(|s| s.split(',').map(|e| e.trim().to_lowercase()).collect())
                .unwrap_or_default();

            let filter = integrity_api::IntegrityFilter {
                root_id: params.root_id,
                issue_type: params.issue_type,
                extensions,
                status: params.status.unwrap_or_else(|| "unreviewed".to_string()),
                path_search: params.path_search,
                show_deleted: false,
            };

            let limit = params.limit.unwrap_or(30).clamp(1, 100);
            let count = integrity_api::count_items(&filter).map_err(|e| e.to_string())?;
            let items = integrity_api::query_items(&filter, 0, limit).map_err(|e| e.to_string())?;

            let mut out = format!("Found {} item(s) with integrity issues.\n\n", count);

            if items.is_empty() {
                return Ok(out);
            }

            out.push_str("| Item | Path | Hash Issues | Val Issues |\n");
            out.push_str("|------|------|-------------|------------|\n");

            for item in &items {
                let hash_total = item.hash_unreviewed + item.hash_reviewed;
                let val_total = item.val_unreviewed + item.val_reviewed;
                out.push_str(&format!(
                    "| {} | {} | {} ({} unreviewed) | {} ({} unreviewed) |\n",
                    item.item_id,
                    item.item_path,
                    hash_total,
                    item.hash_unreviewed,
                    val_total,
                    item.val_unreviewed,
                ));
            }

            if count > limit {
                out.push_str(&format!("\n(Showing {} of {} items)\n", items.len(), count));
            }

            Ok(out)
        })
        .await
        .map_err(|e| rmcp::ErrorData::internal_error(e.to_string(), None))?
        .map_err(|e| rmcp::ErrorData::internal_error(e, None))?;

        Ok(CallToolResult::success(vec![Content::text(result)]))
    }

    #[tool(description = "Get scan history for a root, showing how file counts, sizes, and change rates evolved over time.")]
    async fn scan_history(
        &self,
        Parameters(params): Parameters<ScanHistoryParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let result = tokio::task::spawn_blocking(move || -> Result<String, String> {
            let conn = Database::get_connection().map_err(|e| e.to_string())?;
            let limit = params.limit.unwrap_or(20).clamp(1, 100);

            let mut stmt = conn
                .prepare(
                    "SELECT scan_id, started_at, ended_at, state,
                            file_count, folder_count, total_size,
                            add_count, modify_count, delete_count,
                            new_hash_suspect_count, new_val_invalid_count
                     FROM scans
                     WHERE root_id = ? AND state = 4
                     ORDER BY started_at DESC
                     LIMIT ?",
                )
                .map_err(|e| e.to_string())?;

            let mut rows = stmt
                .query(rusqlite::params![params.root_id, limit])
                .map_err(|e| e.to_string())?;

            let mut out = String::new();
            let mut count = 0;

            out.push_str("| Scan | Started | Files | Folders | Total Size | Adds | Mods | Dels | Hash Suspect | Val Invalid |\n");
            out.push_str("|------|---------|-------|---------|------------|------|------|------|-------------|-------------|\n");

            while let Some(row) = rows.next().map_err(|e| e.to_string())? {
                let scan_id: i64 = row.get(0).map_err(|e| e.to_string())?;
                let started_at: i64 = row.get(1).map_err(|e| e.to_string())?;
                let files: Option<i64> = row.get(4).map_err(|e| e.to_string())?;
                let folders: Option<i64> = row.get(5).map_err(|e| e.to_string())?;
                let size: Option<i64> = row.get(6).map_err(|e| e.to_string())?;
                let adds: Option<i64> = row.get(7).map_err(|e| e.to_string())?;
                let mods: Option<i64> = row.get(8).map_err(|e| e.to_string())?;
                let dels: Option<i64> = row.get(9).map_err(|e| e.to_string())?;
                let hash_s: Option<i64> = row.get(10).map_err(|e| e.to_string())?;
                let val_i: Option<i64> = row.get(11).map_err(|e| e.to_string())?;

                out.push_str(&format!(
                    "| {} | {} | {} | {} | {} | {} | {} | {} | {} | {} |\n",
                    scan_id,
                    started_at,
                    files.unwrap_or(0),
                    folders.unwrap_or(0),
                    size.unwrap_or(0),
                    adds.unwrap_or(0),
                    mods.unwrap_or(0),
                    dels.unwrap_or(0),
                    hash_s.unwrap_or(0),
                    val_i.unwrap_or(0),
                ));
                count += 1;
            }

            if count == 0 {
                return Ok("No completed scans found for this root.".to_string());
            }

            Ok(format!("{} scan(s) returned.\n\n{}", count, out))
        })
        .await
        .map_err(|e| rmcp::ErrorData::internal_error(e.to_string(), None))?
        .map_err(|e| rmcp::ErrorData::internal_error(e, None))?;

        Ok(CallToolResult::success(vec![Content::text(result)]))
    }

    #[tool(description = "Browse the monitored filesystem tree at a specific point in time. Shows immediate children of a directory path within a root at a given scan.")]
    async fn browse_filesystem(
        &self,
        Parameters(params): Parameters<BrowseFilesystemParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let result = tokio::task::spawn_blocking(move || -> Result<String, String> {
            use crate::items;
            use crate::scans::Scan;

            let scan_id = match params.scan_id {
                Some(id) => id,
                None => {
                    Scan::resolve_scan_for_date(params.root_id, None)
                        .map_err(|e| e.to_string())?
                        .map(|(id, _)| id)
                        .ok_or_else(|| "No completed scans found for this root.".to_string())?
                }
            };

            let parent_path = match params.parent_path {
                Some(p) => p,
                None => {
                    let conn = Database::get_connection().map_err(|e| e.to_string())?;
                    conn.query_row(
                        "SELECT root_path FROM roots WHERE root_id = ?",
                        [params.root_id],
                        |row| row.get(0),
                    )
                    .map_err(|e| format!("Root {} not found: {}", params.root_id, e))?
                }
            };

            let children = items::get_temporal_immediate_children(params.root_id, &parent_path, scan_id)
                .map_err(|e| e.to_string())?;

            if children.is_empty() {
                return Ok(format!("No children found under '{}' at scan {}.", parent_path, scan_id));
            }

            let mut out = format!("{} item(s) under '{}' at scan {}.\n\n", children.len(), parent_path, scan_id);
            out.push_str("| Name | Type | Size | Mod Date | Status |\n");
            out.push_str("|------|------|------|----------|--------|\n");

            for child in &children {
                let type_str = match child.item_type {
                    crate::items::ItemType::File => "File",
                    crate::items::ItemType::Directory => "Dir",
                    crate::items::ItemType::Symlink => "Sym",
                    crate::items::ItemType::Unknown => "?",
                };
                let status = if child.is_deleted { "deleted" }
                    else if child.is_added { "added" }
                    else { "" };

                out.push_str(&format!(
                    "| {} | {} | {} | {} | {} |\n",
                    child.item_name,
                    type_str,
                    child.size.unwrap_or(0),
                    child.mod_date.unwrap_or(0),
                    status,
                ));
            }

            Ok(out)
        })
        .await
        .map_err(|e| rmcp::ErrorData::internal_error(e.to_string(), None))?
        .map_err(|e| rmcp::ErrorData::internal_error(e, None))?;

        Ok(CallToolResult::success(vec![Content::text(result)]))
    }

    #[tool(description = "Search for files and directories by name substring within a monitored root at a specific point in time.")]
    async fn search_files(
        &self,
        Parameters(params): Parameters<SearchFilesParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let result = tokio::task::spawn_blocking(move || -> Result<String, String> {
            use crate::items;
            use crate::scans::Scan;

            let scan_id = match params.scan_id {
                Some(id) => id,
                None => {
                    Scan::resolve_scan_for_date(params.root_id, None)
                        .map_err(|e| e.to_string())?
                        .map(|(id, _)| id)
                        .ok_or_else(|| "No completed scans found for this root.".to_string())?
                }
            };

            let results = items::search_temporal_items(params.root_id, scan_id, &params.query)
                .map_err(|e| e.to_string())?;

            if results.is_empty() {
                return Ok(format!("No items matching '{}' found.", params.query));
            }

            let mut out = format!("{} item(s) matching '{}'.\n\n", results.len(), params.query);
            out.push_str("| Name | Type | Path | Size |\n");
            out.push_str("|------|------|------|------|\n");

            for item in &results {
                let type_str = match item.item_type {
                    crate::items::ItemType::File => "File",
                    crate::items::ItemType::Directory => "Dir",
                    crate::items::ItemType::Symlink => "Sym",
                    crate::items::ItemType::Unknown => "?",
                };
                out.push_str(&format!(
                    "| {} | {} | {} | {} |\n",
                    item.item_name,
                    type_str,
                    item.item_path,
                    item.size.unwrap_or(0),
                ));
            }

            Ok(out)
        })
        .await
        .map_err(|e| rmcp::ErrorData::internal_error(e.to_string(), None))?
        .map_err(|e| rmcp::ErrorData::internal_error(e, None))?;

        Ok(CallToolResult::success(vec![Content::text(result)]))
    }

    #[tool(description = "Get detailed information about a specific item including its version history, size changes, and integrity state.")]
    async fn item_detail(
        &self,
        Parameters(params): Parameters<ItemDetailParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let result = tokio::task::spawn_blocking(move || -> Result<String, String> {
            use crate::items;

            let conn = Database::get_connection().map_err(|e| e.to_string())?;

            // Get item identity
            let (item_path, item_type, root_id): (String, i64, i64) = conn
                .query_row(
                    "SELECT item_path, item_type, root_id FROM items WHERE item_id = ?",
                    [params.item_id],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                )
                .map_err(|e| format!("Item {} not found: {}", params.item_id, e))?;

            let type_str = match crate::items::ItemType::from_i64(item_type) {
                crate::items::ItemType::File => "File",
                crate::items::ItemType::Directory => "Directory",
                crate::items::ItemType::Symlink => "Symlink",
                crate::items::ItemType::Unknown => "Unknown",
            };

            let mut out = format!("## Item {}\n\n- Path: {}\n- Type: {}\n- Root: {}\n",
                params.item_id, item_path, type_str, root_id);

            // Version count and history
            let version_count = items::count_versions(params.item_id).map_err(|e| e.to_string())?;
            let max_versions = params.max_versions.unwrap_or(10).clamp(1, 50);
            let versions = items::get_versions(params.item_id, 0, max_versions, "desc")
                .map_err(|e| e.to_string())?;

            out.push_str(&format!("\n## Version History ({} total)\n\n", version_count));

            if !versions.is_empty() {
                out.push_str("| Version | Scans | Size | Mod Date | Added | Deleted | Val State |\n");
                out.push_str("|---------|-------|------|----------|-------|---------|----------|\n");

                for v in &versions {
                    let val = match v.val_state {
                        Some(1) => "Valid",
                        Some(2) => "Invalid",
                        _ => "",
                    };
                    out.push_str(&format!(
                        "| {} | {}..{} | {} | {} | {} | {} | {} |\n",
                        v.item_version,
                        v.first_scan_id,
                        v.last_scan_id,
                        v.size.unwrap_or(0),
                        v.mod_date.unwrap_or(0),
                        v.is_added,
                        v.is_deleted,
                        val,
                    ));
                }

                if version_count > max_versions {
                    out.push_str(&format!("\n(Showing {} of {} versions)\n", versions.len(), version_count));
                }
            }

            Ok(out)
        })
        .await
        .map_err(|e| rmcp::ErrorData::internal_error(e.to_string(), None))?
        .map_err(|e| rmcp::ErrorData::internal_error(e, None))?;

        Ok(CallToolResult::success(vec![Content::text(result)]))
    }

    #[tool(description = "Show what files were added, modified, or deleted in a specific scan.")]
    async fn scan_changes(
        &self,
        Parameters(params): Parameters<ScanChangesParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let scan_id = params.scan_id;
        let change_type = params.change_type.unwrap_or_else(|| "all".to_string());
        let limit = params.limit.unwrap_or(50).clamp(1, 200);

        // Build a query DSL string dynamically based on change_type
        let filter = match change_type.as_str() {
            "added" => format!("versions where first_scan_id:({}), is_added:(T) show item_path, item_type, size order by item_path limit {}", scan_id, limit),
            "deleted" => format!("versions where first_scan_id:({}), is_deleted:(T) show item_path, item_type, size order by item_path limit {}", scan_id, limit),
            "modified" => format!("versions where first_scan_id:({}), is_added:(F), is_deleted:(F) show item_path, item_type, size order by item_path limit {}", scan_id, limit),
            _ => format!("versions where first_scan_id:({}) show item_path, item_type, is_added, is_deleted, size order by item_path limit {}", scan_id, limit),
        };

        let result = tokio::task::spawn_blocking(move || {
            QueryProcessor::execute_query(&filter)
        })
        .await
        .map_err(|e| rmcp::ErrorData::internal_error(e.to_string(), None))?;

        match result {
            Ok((rows, headers, alignments)) => {
                // Get scan summary
                let summary = tokio::task::spawn_blocking(move || -> Result<String, String> {
                    let conn = Database::get_connection().map_err(|e| e.to_string())?;
                    let (adds, mods, dels): (Option<i64>, Option<i64>, Option<i64>) = conn
                        .query_row(
                            "SELECT add_count, modify_count, delete_count FROM scans WHERE scan_id = ?",
                            [scan_id],
                            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                        )
                        .map_err(|e| format!("Scan {} not found: {}", scan_id, e))?;

                    Ok(format!(
                        "Scan {} — {} added, {} modified, {} deleted\n\n",
                        scan_id,
                        adds.unwrap_or(0),
                        mods.unwrap_or(0),
                        dels.unwrap_or(0),
                    ))
                })
                .await
                .map_err(|e| rmcp::ErrorData::internal_error(e.to_string(), None))?
                .map_err(|e| rmcp::ErrorData::internal_error(e, None))?;

                let table = format_table(&headers, &rows, &alignments);
                let out = format!("{}{} row(s) returned.\n\n{}", summary, rows.len(), table);
                Ok(CallToolResult::success(vec![Content::text(out)]))
            }
            Err(e) => {
                Ok(CallToolResult::error(vec![Content::text(e.to_string())]))
            }
        }
    }
}

#[tool_handler]
impl ServerHandler for FsPulseMcp {
    fn get_info(&self) -> ServerInfo {
        let mut server_info = Implementation::from_build_env();
        server_info.name = "fspulse".to_string();

        let mut capabilities = ServerCapabilities::default();
        capabilities.tools = Some(ToolsCapability::default());

        ServerInfo::new(capabilities)
            .with_server_info(server_info)
            .with_instructions(
                "fsPulse filesystem scanner and integrity tracker. \
                 IMPORTANT: Always call query_help before using query_data \
                 to learn available column names. Do not guess column names."
            )
    }
}

// ─── Query help content ─────────────────────────────────────────────

fn general_help() -> String {
    r#"## fspulse Query DSL

### Structure

```
DOMAIN [WHERE ...] [GROUP BY ...] [SHOW ...] [ORDER BY ...] [LIMIT ...] [OFFSET ...]
```

### Domains

- **items** — Item identity (path, name, extension, type)
- **versions** — Item versions over time (size, mod_date, val_state, etc.). Filter with `is_current:(T)` for latest state.
- **hashes** — Hash observations on item versions (file_hash, hash_state)
- **scans** — Scan sessions (timestamps, counts, integrity findings)
- **roots** — Monitored root directories

### WHERE Clause

Filters use the syntax: `column_name:(value1, value2, ...)`

| Type | Examples |
|------|----------|
| Integer | `5`, `1..5`, `> 1024`, `null`, `not null` |
| Date | `2024-01-01`, `2024-01-01..2024-06-30` |
| Boolean | `T`, `F`, `true`, `false` |
| String | `'example'`, `null`, `not null` |
| Path | `'/photos'`, `'report.pdf'` |
| Val State | `V`, `I`, `N`, `U` (Valid, Invalid, No Validator, Unknown) |
| Hash State | `V`, `S`, `U` (Valid, Suspect, Unknown) |
| Item Type | `F`, `D`, `S`, `U` (File, Directory, Symlink, Unknown) |

Multiple values within a filter are OR'd. Multiple filters are AND'd.

### GROUP BY and Aggregates

Group rows by one or more columns and apply aggregate functions. GROUP BY requires a SHOW clause.

Aggregate functions: `count(*)`, `count(col)`, `sum(col)`, `avg(col)`, `min(col)`, `max(col)`
- `sum` and `avg` work on integer columns only
- `min` and `max` work on integer, date, and id columns
- Every non-aggregate column in SHOW must appear in GROUP BY
- Aggregates can be used in ORDER BY

### SHOW Clause

Controls displayed columns. Use `default` for defaults, `all` for everything.
Format modifiers: `item_path@name`, `mod_date@short`, `started_at@timestamp`

### Examples

```
versions where is_current:(T), root_id:(1) show item_path, size limit 20
hashes where hash_state:(S) show item_path, file_hash
scans where root_id:(1) order by started_at desc limit 10
items where file_extension:('pdf') show item_path, item_name
versions where is_current:(T), item_type:(F) group by file_extension show file_extension, count(*), sum(size) order by sum(size) desc
scans group by root_id show root_id, count(*), max(total_size) order by count(*) desc
hashes group by hash_state show hash_state, count(*)
```

Use `query_help` with a domain parameter for column details."#
        .to_string()
}

fn domain_help(domain: &str) -> Result<String, rmcp::ErrorData> {
    use crate::query::columns::*;

    let col_map: &ColMap = match domain {
        "items" => &ITEMS_QUERY_COLS,
        "versions" => &VERSIONS_QUERY_COLS,
        "hashes" => &HASHES_QUERY_COLS,
        "scans" => &SCANS_QUERY_COLS,
        "roots" => &ROOTS_QUERY_COLS,
        _ => {
            return Err(rmcp::ErrorData::invalid_params(
                format!("Unknown domain '{}'. Valid domains: items, versions, hashes, scans, roots", domain),
                None,
            ));
        }
    };

    let mut out = format!("## `{}` Domain Columns\n\n", domain);
    out.push_str("| Column | Type | Filter Syntax |\n");
    out.push_str("|--------|------|---------------|\n");

    for (name, spec) in col_map.entries() {
        let type_info = spec.col_type.info();
        out.push_str(&format!(
            "| `{}` | {} | {} |\n",
            name,
            type_info.type_name,
            type_info.tip.replace('\n', " "),
        ));
    }

    Ok(out)
}
