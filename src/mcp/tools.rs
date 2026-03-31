use rmcp::{
    ServerHandler,
    model::{CallToolResult, Content, ServerInfo, ServerCapabilities, ToolsCapability, Implementation},
    tool, tool_router, tool_handler,
};
use rmcp::handler::server::wrapper::Parameters;

use crate::db::Database;
use crate::query::{QueryProcessor, QueryResultData};

use super::formatting::{format_table, effective_limit, fmt_ts, fmt_opt_ts, MAX_RESULT_ROWS};
use super::help::{general_help, domain_help};
use super::params::*;

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

    #[tool(description = "Get a high-level overview of the fspulse system: monitored roots with latest scan stats (file/folder counts, total monitored size), unreviewed integrity issue counts, and database path/size.")]
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

    #[tool(description = "Execute a fspulse query using the built-in DSL. Supports five domains: roots, scans, items, versions, hashes. Date display: @short (default, date only), @full (date + time), @timestamp (Unix epoch). Date filters accept all three forms: 2025-01-01, 2025-01-01 14:30:00, or 1735689600 — any output format can be used directly as filter input. Use LIMIT and OFFSET in the query string to paginate (max 200 rows per call). Use query_count to get total row counts. Use query_help for syntax details. Returns results as a markdown table.")]
    async fn query_data(
        &self,
        Parameters(params): Parameters<QueryDataParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let query = params.query;

        let result = tokio::task::spawn_blocking(move || {
            QueryProcessor::execute_query_override(&query, MAX_RESULT_ROWS, 0)
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

    #[tool(description = "Get a report of integrity issues (validation failures, suspect hashes) for a monitored root. Supports pagination via limit/offset parameters. Returns total count in response.")]
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

            let limit = effective_limit(params.limit);
            let offset = params.offset.unwrap_or(0).max(0);
            let count = integrity_api::count_items(&filter).map_err(|e| e.to_string())?;
            let items = integrity_api::query_items(&filter, offset, limit).map_err(|e| e.to_string())?;

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

            if count > items.len() as i64 + offset {
                out.push_str(&format!(
                    "\n(Showing items {}-{} of {}. Use offset to paginate.)\n",
                    offset + 1,
                    offset + items.len() as i64,
                    count
                ));
            }

            Ok(out)
        })
        .await
        .map_err(|e| rmcp::ErrorData::internal_error(e.to_string(), None))?
        .map_err(|e| rmcp::ErrorData::internal_error(e, None))?;

        Ok(CallToolResult::success(vec![Content::text(result)]))
    }

    #[tool(description = "Get scan history for a root, showing how file counts, sizes, and change rates evolved over time. Supports pagination via limit/offset parameters. Returns total count in response.")]
    async fn scan_history(
        &self,
        Parameters(params): Parameters<ScanHistoryParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let result = tokio::task::spawn_blocking(move || -> Result<String, String> {
            let conn = Database::get_connection().map_err(|e| e.to_string())?;
            let limit = effective_limit(params.limit);
            let offset = params.offset.unwrap_or(0).max(0);

            // Get total count
            let total: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM scans WHERE root_id = ? AND state = 4",
                    [params.root_id],
                    |row| row.get(0),
                )
                .map_err(|e| e.to_string())?;

            if total == 0 {
                return Ok("No completed scans found for this root.".to_string());
            }

            let mut stmt = conn
                .prepare(
                    "SELECT scan_id, started_at, ended_at, state,
                            file_count, folder_count, total_size,
                            add_count, modify_count, delete_count,
                            new_hash_suspect_count, new_val_invalid_count
                     FROM scans
                     WHERE root_id = ? AND state = 4
                     ORDER BY started_at DESC
                     LIMIT ? OFFSET ?",
                )
                .map_err(|e| e.to_string())?;

            let mut rows = stmt
                .query(rusqlite::params![params.root_id, limit, offset])
                .map_err(|e| e.to_string())?;

            let mut out = String::new();
            let mut row_count: i64 = 0;

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
                    fmt_ts(started_at),
                    files.unwrap_or(0),
                    folders.unwrap_or(0),
                    size.unwrap_or(0),
                    adds.unwrap_or(0),
                    mods.unwrap_or(0),
                    dels.unwrap_or(0),
                    hash_s.unwrap_or(0),
                    val_i.unwrap_or(0),
                ));
                row_count += 1;
            }

            let mut summary = format!("{} total scan(s). Showing {}-{}.\n\n",
                total, offset + 1, offset + row_count);
            summary.push_str(&out);

            if total > offset + row_count {
                summary.push_str(&format!(
                    "\n(More results available. Use offset: {} to see next page.)\n",
                    offset + row_count
                ));
            }

            Ok(summary)
        })
        .await
        .map_err(|e| rmcp::ErrorData::internal_error(e.to_string(), None))?
        .map_err(|e| rmcp::ErrorData::internal_error(e, None))?;

        Ok(CallToolResult::success(vec![Content::text(result)]))
    }

    #[tool(description = "Browse the monitored filesystem tree at a specific point in time. Shows immediate children of a directory path within a root at a given scan. Supports pagination via limit/offset parameters. Returns total count in response.")]
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

            let limit = effective_limit(params.limit);
            let offset = params.offset.unwrap_or(0).max(0);

            let total = items::count_temporal_immediate_children(params.root_id, &parent_path, scan_id)
                .map_err(|e| e.to_string())?;

            if total == 0 {
                return Ok(format!("No children found under '{}' at scan {}.", parent_path, scan_id));
            }

            let children = items::get_temporal_immediate_children(
                params.root_id, &parent_path, scan_id, Some(limit), Some(offset),
            ).map_err(|e| e.to_string())?;

            let row_count = children.len() as i64;
            let mut out = format!(
                "{} total item(s) under '{}' at scan {}. Showing {}-{}.\n\n",
                total, parent_path, scan_id, offset + 1, offset + row_count
            );
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
                    fmt_opt_ts(child.mod_date),
                    status,
                ));
            }

            if total > offset + row_count {
                out.push_str(&format!(
                    "\n(More results available. Use offset: {} to see next page.)\n",
                    offset + row_count
                ));
            }

            Ok(out)
        })
        .await
        .map_err(|e| rmcp::ErrorData::internal_error(e.to_string(), None))?
        .map_err(|e| rmcp::ErrorData::internal_error(e, None))?;

        Ok(CallToolResult::success(vec![Content::text(result)]))
    }

    #[tool(description = "Search for files and directories by name substring within a monitored root at a specific point in time. Supports pagination via limit/offset parameters. Returns total count in response.")]
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

            let limit = effective_limit(params.limit);
            let offset = params.offset.unwrap_or(0).max(0);

            let total = items::count_temporal_search_items(params.root_id, scan_id, &params.query)
                .map_err(|e| e.to_string())?;

            if total == 0 {
                return Ok(format!("No items matching '{}' found.", params.query));
            }

            let results = items::get_temporal_search_items(
                params.root_id, scan_id, &params.query, Some(limit), Some(offset),
            ).map_err(|e| e.to_string())?;

            let row_count = results.len() as i64;
            let mut out = format!(
                "{} total item(s) matching '{}'. Showing {}-{}.\n\n",
                total, params.query, offset + 1, offset + row_count
            );
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

            if total > offset + row_count {
                out.push_str(&format!(
                    "\n(More results available. Use offset: {} to see next page.)\n",
                    offset + row_count
                ));
            }

            Ok(out)
        })
        .await
        .map_err(|e| rmcp::ErrorData::internal_error(e.to_string(), None))?
        .map_err(|e| rmcp::ErrorData::internal_error(e, None))?;

        Ok(CallToolResult::success(vec![Content::text(result)]))
    }

    #[tool(description = "Get detailed information about a specific item including its version history, size changes, integrity state, and hash observations. Version list supports pagination via limit/offset parameters.")]
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
            let limit = effective_limit(params.limit);
            let offset = params.offset.unwrap_or(0).max(0);
            let versions = items::get_versions(params.item_id, offset, limit, "desc")
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
                        fmt_opt_ts(v.mod_date),
                        v.is_added,
                        v.is_deleted,
                        val,
                    ));
                }

                let shown = versions.len() as i64;
                if version_count > offset + shown {
                    out.push_str(&format!(
                        "\n(Showing versions {}-{} of {}. Use offset: {} to see next page.)\n",
                        offset + 1, offset + shown, version_count, offset + shown
                    ));
                }
            }

            Ok(out)
        })
        .await
        .map_err(|e| rmcp::ErrorData::internal_error(e.to_string(), None))?
        .map_err(|e| rmcp::ErrorData::internal_error(e, None))?;

        Ok(CallToolResult::success(vec![Content::text(result)]))
    }

    #[tool(description = "Show what files were added, modified, or deleted in a specific scan. Supports pagination via limit/offset parameters. Returns total count in response.")]
    async fn scan_changes(
        &self,
        Parameters(params): Parameters<ScanChangesParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let scan_id = params.scan_id;
        let change_type = params.change_type.unwrap_or_else(|| "all".to_string());
        let limit = effective_limit(params.limit);
        let offset = params.offset.unwrap_or(0).max(0);

        // Build the WHERE clause based on change_type
        let where_clause = match change_type.as_str() {
            "added" => format!("first_scan_id:({}), is_added:(T)", scan_id),
            "deleted" => format!("first_scan_id:({}), is_deleted:(T)", scan_id),
            "modified" => format!("first_scan_id:({}), is_added:(F), is_deleted:(F)", scan_id),
            _ => format!("first_scan_id:({})", scan_id),
        };

        let show_clause = match change_type.as_str() {
            "added" | "deleted" | "modified" => "show item_path, item_type, size",
            _ => "show item_path, item_type, is_added, is_deleted, size",
        };

        // Build count and data queries
        let count_query = format!("versions where {} {}", where_clause, show_clause);
        let data_query = format!(
            "versions where {} {} order by item_path limit {} offset {}",
            where_clause, show_clause, limit, offset
        );

        let count_q = count_query.clone();
        let result = tokio::task::spawn_blocking(move || -> Result<(i64, QueryResultData), String> {
            let total = QueryProcessor::execute_query_count(&count_q)
                .map_err(|e| e.to_string())?;
            let data = QueryProcessor::execute_query(&data_query)
                .map_err(|e| e.to_string())?;
            Ok((total, data))
        })
        .await
        .map_err(|e| rmcp::ErrorData::internal_error(e.to_string(), None))?;

        match result {
            Ok((total, (rows, headers, alignments))) => {
                // Get scan summary
                let row_count = rows.len() as i64;
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
                let mut out = format!(
                    "{}{} total matching row(s). Showing {}-{}.\n\n{}",
                    summary, total, offset + 1, offset + row_count, table
                );

                if total > offset + row_count {
                    out.push_str(&format!(
                        "\n(More results available. Use offset: {} to see next page.)\n",
                        offset + row_count
                    ));
                }

                Ok(CallToolResult::success(vec![Content::text(out)]))
            }
            Err(e) => {
                Ok(CallToolResult::error(vec![Content::text(e)]))
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
                 to learn available column names. Do not guess column names. \
                 All tools return at most 200 rows per call. Use limit/offset \
                 parameters (or LIMIT/OFFSET in query strings) to paginate. \
                 Use query_count to get total counts for query_data queries."
            )
    }
}

