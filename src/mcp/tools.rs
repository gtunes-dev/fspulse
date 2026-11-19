use rmcp::{
    ServerHandler,
    model::{CallToolResult, Content, ServerInfo, ServerCapabilities, ToolsCapability, Implementation},
    tool, tool_router, tool_handler,
};
use rmcp::handler::server::wrapper::Parameters;

use crate::db::Database;
use crate::query::QueryProcessor;

use crate::timeline::{self, Change, ChangeCounts, TimeRange};
use crate::utils::Utils;

use super::formatting::{
    describe_range, effective_limit, fmt_opt_ts, fmt_ts, format_table, parse_time, MAX_RESULT_ROWS,
};
use super::help::{general_help, domain_help};
use super::params::*;

/// Format a pagination summary like "Showing 1-50 of 1392 (4%)"
fn pagination_summary(offset: i64, row_count: i64, total: i64) -> String {
    let pct = if total > 0 {
        ((row_count as f64 / total as f64) * 100.0).round() as i64
    } else {
        100
    };
    format!("Showing {}-{} of {} ({}%)", offset + 1, offset + row_count, total, pct)
}

fn more_available(total: i64, offset: i64, row_count: i64) -> String {
    if total > offset + row_count {
        format!(
            "\n(More results available. Use offset: {} to see next page.)\n",
            offset + row_count
        )
    } else {
        String::new()
    }
}

fn change_word(change: Change) -> &'static str {
    match change {
        Change::Added => "added",
        Change::Modified => "modified",
        Change::Deleted => "deleted",
        Change::Unchanged => "",
        Change::Transient => "transient",
        Change::Ignored => "ignored",
        Change::Resumed => "resumed",
    }
}

/// "+3 ~1 -2" for the changes beneath a folder, with "ignored 4" and
/// "resumed 1" when there are any; empty if none.
fn counts_word(counts: &ChangeCounts) -> String {
    let mut parts = Vec::new();
    if counts.added > 0 {
        parts.push(format!("+{}", counts.added));
    }
    if counts.modified > 0 {
        parts.push(format!("~{}", counts.modified));
    }
    if counts.deleted > 0 {
        parts.push(format!("-{}", counts.deleted));
    }
    if counts.ignored > 0 {
        parts.push(format!("ignored {}", counts.ignored));
    }
    if counts.resumed > 0 {
        parts.push(format!("resumed {}", counts.resumed));
    }
    parts.join(" ")
}

/// Resolve a tool's `from` and `to`/`at` strings against the clock.
fn resolve_range(from: Option<&str>, to: Option<&str>) -> Result<TimeRange, String> {
    let now = Utils::now_secs();
    let from = parse_time(from, now)?;
    let to = parse_time(to, now)?;
    Ok(TimeRange::resolve(from, to, now))
}

fn root_path(conn: &rusqlite::Connection, root_id: i64) -> Result<String, String> {
    conn.query_row(
        "SELECT root_path FROM roots WHERE root_id = ?",
        [root_id],
        |row| row.get(0),
    )
    .map_err(|_| format!("Root {root_id} not found. Use system_overview to list roots."))
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

    #[tool(description = "Get a high-level overview of the fspulse system. For each monitored root: how it is being tracked (watching in real time, degraded, offline, ...), when it was last checked in full, current file/folder counts and size, and when its last integrity check completed. Also unreviewed integrity issue counts and database path/size. Start here to get root IDs.")]
    async fn system_overview(&self) -> Result<CallToolResult, rmcp::ErrorData> {
        let result = tokio::task::spawn_blocking(|| -> Result<String, String> {
            let conn = Database::get_connection().map_err(|e| e.to_string())?;

            let mut stmt = conn
                .prepare(
                    "SELECT r.root_id, r.root_path, r.watch_enabled,
                            r.last_checkpoint_completed_at,
                            (SELECT s.file_count FROM snapshots s WHERE s.root_id = r.root_id
                              ORDER BY s.taken_at DESC LIMIT 1),
                            (SELECT s.folder_count FROM snapshots s WHERE s.root_id = r.root_id
                              ORDER BY s.taken_at DESC LIMIT 1),
                            (SELECT s.total_size FROM snapshots s WHERE s.root_id = r.root_id
                              ORDER BY s.taken_at DESC LIMIT 1),
                            (SELECT MAX(sc.ended_at) FROM scans sc
                              WHERE sc.root_id = r.root_id AND sc.state = 2)
                     FROM roots r
                     ORDER BY r.root_path COLLATE natural_path",
                )
                .map_err(|e| e.to_string())?;

            let roots: Vec<String> = stmt
                .query_map([], |row| {
                    let root_id: i64 = row.get(0)?;
                    let root_path: String = row.get(1)?;
                    let watch_enabled: bool = row.get(2)?;
                    let checked: Option<i64> = row.get(3)?;
                    let files: Option<i64> = row.get(4)?;
                    let folders: Option<i64> = row.get(5)?;
                    let size: Option<i64> = row.get(6)?;
                    let integrity: Option<i64> = row.get(7)?;

                    let status = crate::root_status::get(root_id);
                    let tracking = match status.watch {
                        Some(crate::root_status::WatchState::Watching) => "watching (changes recorded as they happen)".to_string(),
                        Some(crate::root_status::WatchState::Degraded) => "watching, degraded (some changes may wait for the next full check)".to_string(),
                        Some(crate::root_status::WatchState::Unwatched) => "not watched (changes found by full checks only)".to_string(),
                        Some(crate::root_status::WatchState::Disabled) => "watching turned off (changes found by full checks only)".to_string(),
                        Some(crate::root_status::WatchState::Offline) => "offline (directory not present)".to_string(),
                        None if !watch_enabled => "watching turned off".to_string(),
                        None => "not yet started".to_string(),
                    };
                    let blocked = match status.walk_block {
                        Some(_) => format!(
                            "\n  - Full checks are held: {}",
                            status.walk_block_detail.unwrap_or_else(|| "awaiting confirmation in the web UI".to_string())
                        ),
                        None => String::new(),
                    };
                    let totals = match (files, folders, size) {
                        (Some(files), Some(folders), Some(size)) => {
                            format!("{files} files, {folders} folders, {size} bytes")
                        }
                        _ => "no totals yet".to_string(),
                    };

                    Ok(format!(
                        "- Root {root_id} ({root_path})\n  - Tracking: {tracking}{blocked}\n  - Last checked in full: {}\n  - Now: {totals}\n  - Last integrity check completed: {}",
                        checked.map(fmt_ts).unwrap_or_else(|| "never".to_string()),
                        integrity.map(fmt_ts).unwrap_or_else(|| "never".to_string()),
                    ))
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
            out.push_str(&format!("\nCurrent time: {}\n", fmt_ts(Utils::now_secs())));

            Ok(out)
        })
        .await
        .map_err(|e| rmcp::ErrorData::internal_error(e.to_string(), None))?
        .map_err(|e| rmcp::ErrorData::internal_error(e, None))?;

        Ok(CallToolResult::success(vec![Content::text(result)]))
    }

    #[tool(description = "Execute a fspulse query using the built-in DSL. Supports six domains: roots, items, versions, hashes, snapshots (hourly totals and change counts per root), scans (integrity checks). For \"what changed between two times\" prefer the changes tool; for a folder listing at a time prefer browse_filesystem. Date display: @short (default, date only), @full (date + time), @timestamp (Unix epoch). Date filters accept all three forms: 2025-01-01, 2025-01-01 14:30:00, or 1735689600 — any output format can be used directly as filter input. Use LIMIT and OFFSET in the query string to paginate (max 200 rows per call). Use query_count to get total row counts. Use query_help for syntax details. Returns results as a markdown table.")]
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
                    "\n({}. Use offset to paginate.)\n",
                    pagination_summary(offset, items.len() as i64, count)
                ));
            }

            Ok(out)
        })
        .await
        .map_err(|e| rmcp::ErrorData::internal_error(e.to_string(), None))?
        .map_err(|e| rmcp::ErrorData::internal_error(e, None))?;

        Ok(CallToolResult::success(vec![Content::text(result)]))
    }

    #[tool(description = "List completed integrity checks (hashing and validation runs) for a root, newest first, with how many new suspect hashes and validation failures each found. For file counts, sizes and change rates over time use the activity tool instead. Supports pagination via limit/offset parameters. Returns total count in response.")]
    async fn integrity_check_history(
        &self,
        Parameters(params): Parameters<IntegrityCheckHistoryParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let result = tokio::task::spawn_blocking(move || -> Result<String, String> {
            let conn = Database::get_connection().map_err(|e| e.to_string())?;
            let limit = effective_limit(params.limit);
            let offset = params.offset.unwrap_or(0).max(0);
            let completed = crate::scans::ScanState::Completed.as_i64();

            let total: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM scans WHERE root_id = ? AND state = ?",
                    rusqlite::params![params.root_id, completed],
                    |row| row.get(0),
                )
                .map_err(|e| e.to_string())?;

            if total == 0 {
                return Ok("No completed integrity checks found for this root.".to_string());
            }

            let mut stmt = conn
                .prepare(
                    "SELECT scan_id, started_at, ended_at, is_hash, hash_all, is_val,
                            new_hash_suspect_count, new_val_invalid_count
                     FROM scans
                     WHERE root_id = ? AND state = ?
                     ORDER BY started_at DESC
                     LIMIT ? OFFSET ?",
                )
                .map_err(|e| e.to_string())?;

            let mut rows = stmt
                .query(rusqlite::params![params.root_id, completed, limit, offset])
                .map_err(|e| e.to_string())?;

            let mut out = String::new();
            let mut row_count: i64 = 0;

            out.push_str("| Check | Started | Ended | Hashing | Validation | New Suspect Hashes | New Validation Failures |\n");
            out.push_str("|-------|---------|-------|---------|------------|--------------------|-------------------------|\n");

            while let Some(row) = rows.next().map_err(|e| e.to_string())? {
                let scan_id: i64 = row.get(0).map_err(|e| e.to_string())?;
                let started_at: i64 = row.get(1).map_err(|e| e.to_string())?;
                let ended_at: Option<i64> = row.get(2).map_err(|e| e.to_string())?;
                let is_hash: bool = row.get(3).map_err(|e| e.to_string())?;
                let hash_all: bool = row.get(4).map_err(|e| e.to_string())?;
                let is_val: bool = row.get(5).map_err(|e| e.to_string())?;
                let hash_s: Option<i64> = row.get(6).map_err(|e| e.to_string())?;
                let val_i: Option<i64> = row.get(7).map_err(|e| e.to_string())?;

                let hashing = match (is_hash, hash_all) {
                    (false, _) => "none",
                    (true, false) => "new or changed",
                    (true, true) => "all",
                };

                out.push_str(&format!(
                    "| {} | {} | {} | {} | {} | {} | {} |\n",
                    scan_id,
                    fmt_ts(started_at),
                    fmt_opt_ts(ended_at),
                    hashing,
                    if is_val { "yes" } else { "no" },
                    hash_s.unwrap_or(0),
                    val_i.unwrap_or(0),
                ));
                row_count += 1;
            }

            let mut summary = format!("{} total integrity check(s). {}.\n\n",
                total, pagination_summary(offset, row_count, total));
            summary.push_str(&out);
            summary.push_str(&more_available(total, offset, row_count));

            Ok(summary)
        })
        .await
        .map_err(|e| rmcp::ErrorData::internal_error(e.to_string(), None))?
        .map_err(|e| rmcp::ErrorData::internal_error(e, None))?;

        Ok(CallToolResult::success(vec![Content::text(result)]))
    }

    #[tool(description = "Show how a root changed over time: per day (default) or per hour, how many items were added, modified and deleted, and the root's file count, folder count and total size at the end of each period. Only periods in which something changed are listed, newest first. Counts are items changed, not events: a file changed many times within an hour counts once for that hour. Use this to find when activity happened, then the changes tool to see what changed. Times: '24h', '7d', '2w', a date, a date and time, or Unix seconds. Default period: the last 7 days.")]
    async fn activity(
        &self,
        Parameters(params): Parameters<ActivityParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let result = tokio::task::spawn_blocking(move || -> Result<String, String> {
            use chrono::{Local, TimeZone};

            let conn = Database::get_connection().map_err(|e| e.to_string())?;
            root_path(&conn, params.root_id)?;

            let now = Utils::now_secs();
            let from = parse_time(params.from.as_deref().or(Some("7d")), now)?.unwrap_or(now);
            let to = parse_time(params.to.as_deref(), now)?.unwrap_or(now);
            let by_hour = match params.bucket.as_deref().unwrap_or("day") {
                "hour" => true,
                "day" => false,
                other => return Err(format!("Unknown bucket '{other}'. Use 'hour' or 'day'.")),
            };
            let limit = effective_limit(params.limit);
            let offset = params.offset.unwrap_or(0).max(0);

            let points = timeline::activity(&conn, params.root_id, from, to)
                .map_err(|e| e.to_string())?;

            // (label, added, modified, deleted, files, folders, size), oldest first.
            type Row = (String, i64, i64, i64, Option<i64>, Option<i64>, Option<i64>);
            let mut rows: Vec<Row> = Vec::new();
            for point in &points {
                let local = Local.timestamp_opt(point.time, 0).single();
                let label = match (by_hour, local) {
                    (true, Some(t)) => t.format("%Y-%m-%d %H:00").to_string(),
                    (false, Some(t)) => t.format("%Y-%m-%d").to_string(),
                    (_, None) => point.time.to_string(),
                };
                match rows.last_mut() {
                    Some(last) if last.0 == label => {
                        last.1 += point.added;
                        last.2 += point.modified;
                        last.3 += point.deleted;
                        last.4 = point.files.or(last.4);
                        last.5 = point.folders.or(last.5);
                        last.6 = point.size.or(last.6);
                    }
                    _ => rows.push((
                        label, point.added, point.modified, point.deleted,
                        point.files, point.folders, point.size,
                    )),
                }
            }
            rows.reverse();

            let total = rows.len() as i64;
            let period = format!("{} to {}", fmt_ts(from), fmt_ts(to));
            if total == 0 {
                return Ok(format!("No changes recorded for this root from {period}."));
            }

            let page: Vec<&Row> = rows.iter().skip(offset as usize).take(limit as usize).collect();
            let row_count = page.len() as i64;
            let cell = |v: Option<i64>| v.map(|v| v.to_string()).unwrap_or_else(|| "-".to_string());

            let mut out = format!(
                "Activity from {period}: {total} {}(s) with changes. {}.\n\n",
                if by_hour { "hour" } else { "day" },
                pagination_summary(offset, row_count, total)
            );
            out.push_str(&format!(
                "| {} | Added | Modified | Deleted | Files | Folders | Total Size |\n",
                if by_hour { "Hour" } else { "Day" }
            ));
            out.push_str("|------|-------|----------|---------|-------|---------|------------|\n");
            for (label, added, modified, deleted, files, folders, size) in page {
                out.push_str(&format!(
                    "| {label} | {added} | {modified} | {deleted} | {} | {} | {} |\n",
                    cell(*files), cell(*folders), cell(*size)
                ));
            }
            out.push_str(&more_available(total, offset, row_count));

            Ok(out)
        })
        .await
        .map_err(|e| rmcp::ErrorData::internal_error(e.to_string(), None))?;

        match result {
            Ok(text) => Ok(CallToolResult::success(vec![Content::text(text)])),
            Err(e) => Ok(CallToolResult::error(vec![Content::text(e)])),
        }
    }

    #[tool(description = "Show what was added, modified or deleted in a root (or beneath one folder) between two times. This is the net difference between the two ends of the range: a file changed ten times is listed once as modified; a file created and deleted inside the range is 'transient' and listed only when asked for. Items that stopped being tracked because they came to match one of the root's ignore patterns are 'ignored', and items tracked again after that are 'resumed'; neither is counted as deleted or added. Times: '24h', '7d', '2w', a date, a date and time, or Unix seconds; `to` defaults to now. Range starts, and ends in the past, are moved back to the hour; the response states the times used. Returns totals per change type, then the changed items in folder order with size, when each last changed in the range and how many changes were recorded. Supports pagination via limit/offset.")]
    async fn changes(
        &self,
        Parameters(params): Parameters<ChangesParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let result = tokio::task::spawn_blocking(move || -> Result<String, String> {
            let conn = Database::get_connection().map_err(|e| e.to_string())?;
            root_path(&conn, params.root_id)?;

            let range = resolve_range(Some(params.from.as_str()), params.to.as_deref())?;
            if range.from.is_none() {
                return Err("`from` must be earlier than `to`.".to_string());
            }
            let folder = params.folder_path.as_deref();
            let limit = effective_limit(params.limit);
            let offset = params.offset.unwrap_or(0).max(0);

            let counts = timeline::summary(&conn, params.root_id, folder, range)
                .map_err(|e| e.to_string())?
                .counts;

            let change_type = params.change_type.as_deref().unwrap_or("all");
            let (kinds, total) = match change_type {
                "all" => (
                    Change::net_mask(),
                    counts.added + counts.modified + counts.deleted + counts.ignored + counts.resumed,
                ),
                "ignored" => (Change::Ignored.mask(), counts.ignored),
                "resumed" => (Change::Resumed.mask(), counts.resumed),
                "added" => (Change::Added.mask(), counts.added),
                "modified" => (Change::Modified.mask(), counts.modified),
                "deleted" => (Change::Deleted.mask(), counts.deleted),
                "transient" => (Change::Transient.mask(), counts.transient),
                other => {
                    return Err(format!(
                        "Unknown change_type '{other}'. Use added, modified, deleted, ignored, resumed, transient or all."
                    ))
                }
            };

            let mut out = format!(
                "{}{}.\n\nAdded: {}, modified: {}, deleted: {}, transient (appeared and disappeared): {}.\n",
                describe_range(&range),
                folder.map(|f| format!(" beneath '{f}'")).unwrap_or_default(),
                counts.added, counts.modified, counts.deleted, counts.transient
            );
            if counts.ignored > 0 || counts.resumed > 0 {
                out.push_str(&format!(
                    "Ignore patterns: {} stopped being tracked (ignored), {} tracked again (resumed). These are not deletions or additions.\n",
                    counts.ignored, counts.resumed
                ));
            }
            if total == 0 {
                return Ok(out);
            }

            let page = timeline::changes(&conn, params.root_id, folder, range, kinds, limit, offset)
                .map_err(|e| e.to_string())?;
            let row_count = page.items.len() as i64;

            out.push_str(&format!("\n{}.\n\n", pagination_summary(offset, row_count, total)));
            out.push_str("| Change | Type | Path | Size | Last Changed | Changes Recorded | Item |\n");
            out.push_str("|--------|------|------|------|--------------|------------------|------|\n");
            for item in &page.items {
                out.push_str(&format!(
                    "| {} | {} | {} | {} | {} | {} | {} |\n",
                    change_word(item.change),
                    item.item_type.full_name(),
                    item.item_path,
                    item.size.map(|s| s.to_string()).unwrap_or_else(|| "-".to_string()),
                    fmt_ts(item.last_changed_at),
                    item.activity,
                    item.item_id,
                ));
            }
            out.push_str(&more_available(total, offset, row_count));

            Ok(out)
        })
        .await
        .map_err(|e| rmcp::ErrorData::internal_error(e.to_string(), None))?;

        match result {
            Ok(text) => Ok(CallToolResult::success(vec![Content::text(text)])),
            Err(e) => Ok(CallToolResult::error(vec![Content::text(e)])),
        }
    }

    #[tool(description = "List the immediate children of a folder in a monitored root, as of now or as of a past time (`at`). With `from`, also shows how each child changed between `from` and `at`, the net changes beneath each folder (+added ~modified -deleted), and children deleted in that range. Times: 'now', '24h', '7d', a date, a date and time, or Unix seconds; past times are moved back to the hour and the response states the times used. Supports pagination via limit/offset parameters. Returns total count in response.")]
    async fn browse_filesystem(
        &self,
        Parameters(params): Parameters<BrowseFilesystemParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let result = tokio::task::spawn_blocking(move || -> Result<String, String> {
            let conn = Database::get_connection().map_err(|e| e.to_string())?;
            let root = root_path(&conn, params.root_id)?;
            let parent_path = params.parent_path.unwrap_or(root);
            let range = resolve_range(params.from.as_deref(), params.at.as_deref())?;
            let is_range = range.from.is_some();

            let limit = effective_limit(params.limit);
            let offset = params.offset.unwrap_or(0).max(0);

            let listing = timeline::children(
                &conn,
                params.root_id,
                &parent_path,
                range,
                false,
                params.include_ignored.unwrap_or(false),
            )
                .map_err(|e| e.to_string())?;
            let total = listing.items.len() as i64;

            if total == 0 {
                return Ok(format!("{}. Nothing under '{}'.", describe_range(&range), parent_path));
            }

            let page: Vec<_> = listing.items.iter().skip(offset as usize).take(limit as usize).collect();
            let row_count = page.len() as i64;
            let mut out = format!(
                "{}. {} item(s) under '{}'. {}.\n\n",
                describe_range(&range), total, parent_path, pagination_summary(offset, row_count, total)
            );
            if is_range {
                out.push_str("| Name | Type | Size | Mod Date | Change | Changes Beneath | Item |\n");
                out.push_str("|------|------|------|----------|--------|-----------------|------|\n");
            } else {
                out.push_str("| Name | Type | Size | Mod Date | Item |\n");
                out.push_str("|------|------|------|----------|------|\n");
            }

            for child in page {
                let size = child.size.map(|s| s.to_string()).unwrap_or_else(|| "-".to_string());
                // Not tracked: say so, and since when, next to the name.
                let name = if child.is_ignored {
                    format!("{} (ignored since {})", child.item_name, fmt_ts(child.first_seen_at))
                } else {
                    child.item_name.clone()
                };
                if is_range {
                    out.push_str(&format!(
                        "| {} | {} | {} | {} | {} | {} | {} |\n",
                        name,
                        child.item_type.full_name(),
                        size,
                        fmt_opt_ts(child.mod_date),
                        child.change.map(change_word).unwrap_or(""),
                        child.descendants.as_ref().map(counts_word).unwrap_or_default(),
                        child.item_id,
                    ));
                } else {
                    out.push_str(&format!(
                        "| {} | {} | {} | {} | {} |\n",
                        name,
                        child.item_type.full_name(),
                        size,
                        fmt_opt_ts(child.mod_date),
                        child.item_id,
                    ));
                }
            }
            out.push_str(&more_available(total, offset, row_count));

            Ok(out)
        })
        .await
        .map_err(|e| rmcp::ErrorData::internal_error(e.to_string(), None))?;

        match result {
            Ok(text) => Ok(CallToolResult::success(vec![Content::text(text)])),
            Err(e) => Ok(CallToolResult::error(vec![Content::text(e)])),
        }
    }

    #[tool(description = "Search for files and directories by name substring within a monitored root, as of now or as of a past time (`at`). With `from`, results also show how each item changed between `from` and `at` and include items deleted in that range. Times: 'now', '24h', '7d', a date, a date and time, or Unix seconds. Returns up to `limit` matches in path order (default 50, max 200); if the limit is reached there may be more, so narrow the search term.")]
    async fn search_files(
        &self,
        Parameters(params): Parameters<SearchFilesParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let result = tokio::task::spawn_blocking(move || -> Result<String, String> {
            let conn = Database::get_connection().map_err(|e| e.to_string())?;
            root_path(&conn, params.root_id)?;
            let range = resolve_range(params.from.as_deref(), params.at.as_deref())?;
            let is_range = range.from.is_some();
            let limit = effective_limit(params.limit);

            let listing = timeline::search(
                &conn,
                params.root_id,
                &params.query,
                range,
                limit,
                params.include_ignored.unwrap_or(false),
            )
                .map_err(|e| e.to_string())?;

            if listing.items.is_empty() {
                return Ok(format!("{}. No items matching '{}' found.", describe_range(&range), params.query));
            }

            let row_count = listing.items.len() as i64;
            let mut out = format!(
                "{}. {} item(s) matching '{}'{}.\n\n",
                describe_range(&range),
                row_count,
                params.query,
                if row_count >= limit { " (limit reached; there may be more)" } else { "" }
            );
            out.push_str("| Name | Type | Path | Size | Change | Item |\n");
            out.push_str("|------|------|------|------|--------|------|\n");

            for item in &listing.items {
                let change = match (is_range, item.change) {
                    (true, Some(change)) => change_word(change),
                    _ => "",
                };
                let name = if item.is_ignored {
                    format!("{} (ignored since {})", item.item_name, fmt_ts(item.first_seen_at))
                } else {
                    item.item_name.clone()
                };
                out.push_str(&format!(
                    "| {} | {} | {} | {} | {} | {} |\n",
                    name,
                    item.item_type.full_name(),
                    item.item_path,
                    item.size.map(|s| s.to_string()).unwrap_or_else(|| "-".to_string()),
                    change,
                    item.item_id,
                ));
            }

            Ok(out)
        })
        .await
        .map_err(|e| rmcp::ErrorData::internal_error(e.to_string(), None))?;

        match result {
            Ok(text) => Ok(CallToolResult::success(vec![Content::text(text)])),
            Err(e) => Ok(CallToolResult::error(vec![Content::text(e)])),
        }
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
                out.push_str("| Version | First Seen | Last Changed | Changes | Size | Mod Date | Event | Val State |\n");
                out.push_str("|---------|------------|--------------|---------|------|----------|-------|----------|\n");

                for v in &versions {
                    let val = match v.val_state {
                        Some(1) => "Valid",
                        Some(2) => "Invalid",
                        _ => "",
                    };
                    let event = match (v.is_added, v.is_deleted, v.is_ignored) {
                        (_, true, true) => "tracking stopped (ignored)",
                        (_, true, false) => "deleted",
                        (true, false, true) => "tracking resumed",
                        (true, false, false) => "added",
                        (false, false, _) => "modified",
                    };
                    out.push_str(&format!(
                        "| {} | {} | {} | {} | {} | {} | {} | {} |\n",
                        v.item_version,
                        fmt_ts(v.first_seen_at),
                        fmt_ts(v.last_changed_at),
                        v.change_count,
                        v.size.unwrap_or(0),
                        fmt_opt_ts(v.mod_date),
                        event,
                        val,
                    ));
                }

                let shown = versions.len() as i64;
                if version_count > offset + shown {
                    out.push_str(&format!(
                        "\n({}. Use offset: {} to see next page.)\n",
                        pagination_summary(offset, shown, version_count), offset + shown
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
                "fsPulse tracks monitored directories (roots) continuously: \
                 changes are recorded as they happen, grouped by the hour, \
                 and integrity checks (hashing and validation) run separately. \
                 There are no scan IDs for browsing: tools take times. A time \
                 can be 'now', a span back from now ('24h', '7d', '2w'), a \
                 local date ('2026-11-17'), a local date and time \
                 ('2026-11-17 14:30') or Unix seconds. Range starts, and \
                 ends in the past, are moved back to the hour. Typical flow: \
                 system_overview for root IDs, activity to find when things \
                 changed, changes to see what changed in a range, \
                 browse_filesystem and search_files to look at a folder or \
                 find a file at a time, item_detail for one item's history, \
                 integrity_report for integrity issues. The query tools are \
                 for questions these do not answer. \
                 IMPORTANT: Before constructing any query, call query_help for \
                 the relevant domain and verify your filter syntax against the \
                 column table. Do not guess column names or filter syntax. \
                 All tools return at most 200 rows per call. Use limit/offset \
                 parameters (or LIMIT/OFFSET in query strings) to paginate. \
                 Use query_count to get total counts for query_data queries. \
                 IMPORTANT: When querying versions with is_current:(T), always \
                 also filter is_deleted:(F) to exclude deleted items — otherwise \
                 counts and aggregates will include items that no longer exist. \
                 IMPORTANT: Path filters use substring matching. When filtering \
                 by folder path, always include the trailing slash (e.g. \
                 item_path:('/data/photos/')) to avoid matching sibling folders \
                 with similar prefixes (e.g. '/data/photos-backup/'). \
                 IMPORTANT: Before drawing conclusions from any paginated result, \
                 check the total count and percentage shown. If you have not seen \
                 all rows, either paginate to completion or use aggregation \
                 (GROUP BY with count/sum), or explicitly state that your \
                 conclusion is based on a partial result."
            )
    }
}

