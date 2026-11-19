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

/// Parse a time given by an MCP caller. Accepted forms:
///
/// - `now`, or nothing: the present (`None`)
/// - `24h`, `7d`, `2w`: that long ago
/// - `2026-11-17`: the start of that day, local time
/// - `2026-11-17 14:30` or `2026-11-17 14:30:00` (a `T` separator is
///   fine): that local time
/// - `1795000000`: Unix seconds
pub(super) fn parse_time(value: Option<&str>, now: i64) -> Result<Option<i64>, String> {
    let Some(text) = value.map(str::trim).filter(|t| !t.is_empty()) else {
        return Ok(None);
    };
    if text.eq_ignore_ascii_case("now") {
        return Ok(None);
    }
    let invalid = || {
        format!(
            "Could not read '{text}' as a time. Use 'now', a span back from now \
             (24h, 7d, 2w), a date (2026-11-17), a date and time \
             (2026-11-17 14:30), or Unix seconds."
        )
    };

    if text.bytes().all(|b| b.is_ascii_digit()) {
        return text.parse().map(Some).map_err(|_| invalid());
    }

    let (number, unit) = text.split_at(text.len() - 1);
    if !number.is_empty() && number.bytes().all(|b| b.is_ascii_digit()) {
        let seconds = match unit {
            "h" | "H" => 3600,
            "d" | "D" => 86_400,
            "w" | "W" => 7 * 86_400,
            _ => return Err(invalid()),
        };
        let count: i64 = number.parse().map_err(|_| invalid())?;
        return Ok(Some(now - count.saturating_mul(seconds)));
    }

    let text = text.replace('T', " ");
    let parsed = match text.len() {
        10 => crate::utils::Utils::date_start_of_day(&text),
        16 => crate::utils::Utils::datetime_to_epoch(&format!("{text}:00")),
        _ => crate::utils::Utils::datetime_to_epoch(&text),
    };
    parsed.map(Some).map_err(|_| invalid())
}

/// The times a tool actually used, in words. Starts, and ends in the
/// past, are on hour boundaries; say so, because the caller may have
/// asked for something else.
pub(super) fn describe_range(range: &crate::timeline::TimeRange) -> String {
    let end = if range.to_is_now {
        format!("now ({})", fmt_ts(range.to))
    } else {
        fmt_ts(range.to)
    };
    match range.from {
        Some(from) => format!("Changes from {} to {}", fmt_ts(from), end),
        None => format!("State as of {end}"),
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

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: i64 = 1_795_000_000;

    #[test]
    fn nothing_and_now_mean_the_present() {
        assert_eq!(parse_time(None, NOW), Ok(None));
        assert_eq!(parse_time(Some("  "), NOW), Ok(None));
        assert_eq!(parse_time(Some("Now"), NOW), Ok(None));
    }

    #[test]
    fn spans_count_back_from_now() {
        assert_eq!(parse_time(Some("24h"), NOW), Ok(Some(NOW - 86_400)));
        assert_eq!(parse_time(Some("7d"), NOW), Ok(Some(NOW - 7 * 86_400)));
        assert_eq!(parse_time(Some("2w"), NOW), Ok(Some(NOW - 14 * 86_400)));
    }

    #[test]
    fn unix_seconds_are_taken_as_given() {
        assert_eq!(parse_time(Some("1794000000"), NOW), Ok(Some(1_794_000_000)));
    }

    #[test]
    fn dates_and_local_times_are_read() {
        let day = parse_time(Some("2026-11-17"), NOW).unwrap().unwrap();
        let afternoon = parse_time(Some("2026-11-17 14:30"), NOW).unwrap().unwrap();
        assert_eq!(afternoon - day, 14 * 3600 + 30 * 60);
        assert_eq!(parse_time(Some("2026-11-17T14:30:00"), NOW), Ok(Some(afternoon)));
    }

    #[test]
    fn anything_else_is_an_error_that_lists_the_forms() {
        let error = parse_time(Some("last tuesday"), NOW).unwrap_err();
        assert!(error.contains("last tuesday") && error.contains("7d"));
        assert!(parse_time(Some("5x"), NOW).is_err());
    }
}
