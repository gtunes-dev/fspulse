use chrono::{DateTime, Local, Utc};

use crate::query::columns::ColAlign;

/// Hard cap on rows returned by any single MCP tool call.
pub(super) const MAX_RESULT_ROWS: i64 = 200;
/// Default number of rows when the caller omits a limit.
const DEFAULT_RESULT_ROWS: i64 = 50;

/// Apply the standard default and cap to a caller-supplied limit.
pub(super) fn effective_limit(limit: Option<i64>) -> i64 {
    limit.unwrap_or(DEFAULT_RESULT_ROWS).clamp(1, MAX_RESULT_ROWS)
}

/// Format a Unix epoch timestamp as a local datetime string.
pub(super) fn fmt_ts(epoch: i64) -> String {
    DateTime::<Utc>::from_timestamp(epoch, 0)
        .map(|dt| {
            let local: DateTime<Local> = dt.with_timezone(&Local);
            local.format("%Y-%m-%d %H:%M:%S").to_string()
        })
        .unwrap_or_else(|| epoch.to_string())
}

/// Format an optional Unix epoch timestamp.
pub(super) fn fmt_opt_ts(epoch: Option<i64>) -> String {
    match epoch {
        Some(ts) => fmt_ts(ts),
        None => "-".to_string(),
    }
}

/// Format query result data as a markdown table.
/// Returns the table string, or a message if no rows were returned.
pub fn format_table(
    headers: &[String],
    rows: &[Vec<String>],
    alignments: &[ColAlign],
) -> String {
    if headers.is_empty() {
        return "No columns to display.".to_string();
    }

    if rows.is_empty() {
        return "No results found.".to_string();
    }

    // Calculate column widths (min: header length)
    let mut widths: Vec<usize> = headers.iter().map(|h| h.len()).collect();
    for row in rows {
        for (i, cell) in row.iter().enumerate() {
            if i < widths.len() {
                widths[i] = widths[i].max(cell.len());
            }
        }
    }

    let mut out = String::new();

    // Header row
    out.push('|');
    for (i, header) in headers.iter().enumerate() {
        out.push_str(&format!(" {:width$} |", header, width = widths[i]));
    }
    out.push('\n');

    // Separator row with alignment
    out.push('|');
    for (i, width) in widths.iter().enumerate() {
        let align = alignments.get(i).copied().unwrap_or(ColAlign::Left);
        let sep = match align {
            ColAlign::Left => format!(" {:-<width$} |", "", width = *width),
            ColAlign::Right => format!(" {:-<width$}:|", "", width = width.saturating_sub(1).max(1)),
            ColAlign::Center => format!(":{:-<width$}:|", "", width = width.saturating_sub(1).max(1)),
        };
        out.push_str(&sep);
    }
    out.push('\n');

    // Data rows
    for row in rows {
        out.push('|');
        for (i, cell) in row.iter().enumerate() {
            if i < widths.len() {
                out.push_str(&format!(" {:width$} |", cell, width = widths[i]));
            }
        }
        out.push('\n');
    }

    out
}
