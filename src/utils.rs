use chrono::{offset::LocalResult, Local, NaiveDate, NaiveDateTime, TimeZone, Utc};
use std::ffi::OsStr;
use std::path::Path;
use std::time::Duration;
use tico::tico;

use crate::error::FsPulseError;

pub struct Utils {}

impl Utils {
    pub fn display_short_path(path: &str) -> String {
        tico(path, None)
    }

    pub fn display_path_name(path: &str) -> String {
        Path::new(path)
            .file_name()
            .and_then(OsStr::to_str)
            .map(str::to_owned)
            .unwrap_or_else(|| path.to_owned())
    }

    // ── Date parsing primitives ──────────────────────────────────────────

    fn parse_date(date_str: &str) -> Result<NaiveDate, FsPulseError> {
        NaiveDate::parse_from_str(date_str, "%Y-%m-%d")
            .map_err(|_| FsPulseError::CustomParsingError(format!("Invalid date: '{date_str}'")))
    }

    fn parse_datetime(datetime_str: &str) -> Result<NaiveDateTime, FsPulseError> {
        NaiveDateTime::parse_from_str(datetime_str, "%Y-%m-%d %H:%M:%S")
            .map_err(|_| FsPulseError::CustomParsingError(format!("Invalid datetime: '{datetime_str}'")))
    }

    pub fn parse_timestamp(ts_str: &str) -> Result<i64, FsPulseError> {
        ts_str.trim().parse().map_err(|_| {
            FsPulseError::CustomParsingError(format!("Invalid timestamp: '{ts_str}'"))
        })
    }

    // ── Local-to-UTC conversion ──────────────────────────────────────────

    /// Converts a local NaiveDateTime to UTC epoch, choosing the earliest
    /// valid time when DST makes the local time ambiguous.
    fn local_to_utc_earliest(naive: &NaiveDateTime, label: &str) -> Result<i64, FsPulseError> {
        match Local.from_local_datetime(naive) {
            LocalResult::Single(dt) => Ok(dt.with_timezone(&Utc).timestamp()),
            LocalResult::Ambiguous(earliest, _) => Ok(earliest.with_timezone(&Utc).timestamp()),
            LocalResult::None => Err(FsPulseError::CustomParsingError(format!(
                "Invalid local time '{label}'"
            ))),
        }
    }

    /// Converts a local NaiveDateTime to UTC epoch, choosing the latest
    /// valid time when DST makes the local time ambiguous.
    fn local_to_utc_latest(naive: &NaiveDateTime, label: &str) -> Result<i64, FsPulseError> {
        match Local.from_local_datetime(naive) {
            LocalResult::Single(dt) => Ok(dt.with_timezone(&Utc).timestamp()),
            LocalResult::Ambiguous(_, latest) => Ok(latest.with_timezone(&Utc).timestamp()),
            LocalResult::None => Err(FsPulseError::CustomParsingError(format!(
                "Invalid local time '{label}'"
            ))),
        }
    }

    // ── Start/end resolution ─────────────────────────────────────────────
    //
    // These are the core building blocks for date filters. Each date form
    // resolves differently depending on whether it appears as the start or
    // end of a range:
    //
    //   date_short ("2025-01-15")     → start of day / end of day
    //   date_long  ("2025-01-15 08:00:00") → exact second
    //   timestamp  ("1737936000")     → exact value

    /// Resolve a date-only string to UTC epoch for use as a range start (00:00:00 local).
    pub fn date_start_of_day(date_str: &str) -> Result<i64, FsPulseError> {
        let date = Self::parse_date(date_str)?;
        let dt = date.and_hms_opt(0, 0, 0).expect("00:00:00 is always valid");
        Self::local_to_utc_earliest(&dt, date_str)
    }

    /// Resolve a date-only string to UTC epoch for use as a range end (23:59:59 local).
    pub fn date_end_of_day(date_str: &str) -> Result<i64, FsPulseError> {
        let date = Self::parse_date(date_str)?;
        let dt = date.and_hms_opt(23, 59, 59).expect("23:59:59 is always valid");
        Self::local_to_utc_latest(&dt, date_str)
    }

    /// Resolve a datetime string to a UTC epoch (exact second).
    pub fn datetime_to_epoch(datetime_str: &str) -> Result<i64, FsPulseError> {
        let dt = Self::parse_datetime(datetime_str)?;
        Self::local_to_utc_earliest(&dt, datetime_str)
    }

    // ── Range validation ─────────────────────────────────────────────────

    /// Validates that start <= end.
    pub fn validate_range(start: i64, end: i64) -> Result<(i64, i64), FsPulseError> {
        if start > end {
            return Err(FsPulseError::CustomParsingError(format!(
                "Range start ({}) is after range end ({})",
                start, end
            )));
        }
        Ok((start, end))
    }

    // ── Convenience methods for callers outside the query engine ──────────

    /// For a single date-only string: returns full-day bounds as UTC epochs.
    pub fn single_date_bounds(date_str: &str) -> Result<(i64, i64), FsPulseError> {
        let start = Self::date_start_of_day(date_str)?;
        let end = Self::date_end_of_day(date_str)?;
        Ok((start, end))
    }

    /// For a range of date-only strings: returns (start-of-first-day, end-of-last-day) as UTC epochs.
    pub fn range_date_bounds(
        start_date_str: &str,
        end_date_str: &str,
    ) -> Result<(i64, i64), FsPulseError> {
        let start = Self::date_start_of_day(start_date_str)?;
        let end = Self::date_end_of_day(end_date_str)?;
        Self::validate_range(start, end)
    }

    /// Current time as a Unix epoch in seconds. Returns 0 if the
    /// system clock is before the epoch (which shouldn't happen on
    /// any sane system).
    pub fn now_secs() -> i64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0)
    }

    /// Format a duration as a human-readable elapsed time string.
    /// Examples: "0s", "5s", "1m 30s", "61m 1s"
    pub fn format_elapsed(duration: Duration) -> String {
        let total_secs = duration.as_secs();
        if total_secs < 60 {
            format!("{}s", total_secs)
        } else {
            let mins = total_secs / 60;
            let secs = total_secs % 60;
            format!("{}m {}s", mins, secs)
        }
    }

    pub fn _format_path_for_display(path: &Path, max_len: usize) -> String {
        let path_str = path.to_string_lossy();
        if path_str.len() <= max_len {
            path_str.to_string()
        } else {
            let prefix = "...";
            let available_len = max_len - prefix.len();
            format!("{}{}", prefix, &path_str[path_str.len() - available_len..])
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── date_start_of_day / date_end_of_day ──────────────────────────────

    #[test]
    fn test_date_start_of_day_valid() {
        let result = Utils::date_start_of_day("2023-12-25");
        assert!(result.is_ok());
    }

    #[test]
    fn test_date_end_of_day_valid() {
        let result = Utils::date_end_of_day("2023-12-25");
        assert!(result.is_ok());
    }

    #[test]
    fn test_date_day_bounds_span_full_day() {
        let start = Utils::date_start_of_day("2023-12-25").unwrap();
        let end = Utils::date_end_of_day("2023-12-25").unwrap();
        assert!(start < end);
        assert!(end - start <= 86400);
    }

    #[test]
    fn test_date_start_of_day_invalid() {
        assert!(Utils::date_start_of_day("invalid-date").is_err());
        assert!(Utils::date_start_of_day("2023-13-25").is_err());
        assert!(Utils::date_start_of_day("").is_err());
    }

    #[test]
    fn test_date_end_of_day_invalid() {
        assert!(Utils::date_end_of_day("invalid-date").is_err());
        assert!(Utils::date_end_of_day("2023-13-25").is_err());
        assert!(Utils::date_end_of_day("").is_err());
    }

    // ── datetime_to_epoch ────────────────────────────────────────────────

    #[test]
    fn test_datetime_to_epoch_valid() {
        let result = Utils::datetime_to_epoch("2023-12-25 14:30:00");
        assert!(result.is_ok());
    }

    #[test]
    fn test_datetime_to_epoch_invalid() {
        assert!(Utils::datetime_to_epoch("2023-12-25").is_err());
        assert!(Utils::datetime_to_epoch("not-a-datetime").is_err());
    }

    // ── parse_timestamp ──────────────────────────────────────────────────

    #[test]
    fn test_parse_timestamp_valid() {
        let result = Utils::parse_timestamp("1703500200");
        assert_eq!(result.unwrap(), 1703500200);
    }

    #[test]
    fn test_parse_timestamp_invalid() {
        assert!(Utils::parse_timestamp("not_a_number").is_err());
    }

    // ── validate_range ───────────────────────────────────────────────────

    #[test]
    fn test_validate_range_valid() {
        let result = Utils::validate_range(100, 400);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), (100, 400));
    }

    #[test]
    fn test_validate_range_equal() {
        let result = Utils::validate_range(100, 100);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_range_reversed() {
        assert!(Utils::validate_range(400, 100).is_err());
    }

    #[test]
    fn test_datetime_to_epoch_within_day_bounds() {
        // A datetime at midday should fall between start-of-day and end-of-day
        let start = Utils::date_start_of_day("2023-12-25").unwrap();
        let end = Utils::date_end_of_day("2023-12-25").unwrap();
        let mid = Utils::datetime_to_epoch("2023-12-25 12:00:00").unwrap();
        assert!(mid > start, "midday ({mid}) should be after start-of-day ({start})");
        assert!(mid < end, "midday ({mid}) should be before end-of-day ({end})");
    }

    // ── Convenience methods (single_date_bounds, range_date_bounds) ──────

    #[test]
    fn test_single_date_bounds() {
        let (start, end) = Utils::single_date_bounds("2023-12-25").unwrap();
        assert!(start <= end);
        assert!(end - start <= 86400);
    }

    #[test]
    fn test_single_date_bounds_invalid() {
        assert!(Utils::single_date_bounds("invalid-date").is_err());
        assert!(Utils::single_date_bounds("2023-13-25").is_err());
        assert!(Utils::single_date_bounds("").is_err());
    }

    #[test]
    fn test_range_date_bounds_valid() {
        let (start, end) = Utils::range_date_bounds("2023-12-24", "2023-12-25").unwrap();
        assert!(start <= end);
    }

    #[test]
    fn test_range_date_bounds_reversed() {
        assert!(Utils::range_date_bounds("2023-12-25", "2023-12-24").is_err());
    }

    #[test]
    fn test_display_short_path() {
        let short_path = Utils::display_short_path("/very/long/path/to/file.txt");
        assert!(!short_path.is_empty());

        let normal_path = Utils::display_short_path("short.txt");
        assert_eq!(normal_path, "short.txt");
    }

    #[test]
    fn test_format_elapsed() {
        assert_eq!(Utils::format_elapsed(Duration::from_secs(0)), "0s");
        assert_eq!(Utils::format_elapsed(Duration::from_secs(5)), "5s");
        assert_eq!(Utils::format_elapsed(Duration::from_secs(59)), "59s");
        assert_eq!(Utils::format_elapsed(Duration::from_secs(60)), "1m 0s");
        assert_eq!(Utils::format_elapsed(Duration::from_secs(90)), "1m 30s");
        assert_eq!(Utils::format_elapsed(Duration::from_secs(3661)), "61m 1s");
    }

    #[test]
    fn test_display_path_name() {
        // Test absolute path with file
        assert_eq!(Utils::display_path_name("/path/to/file.txt"), "file.txt");

        // Test absolute path with directory
        assert_eq!(Utils::display_path_name("/path/to/directory"), "directory");

        // Test relative path
        assert_eq!(Utils::display_path_name("path/to/file.txt"), "file.txt");

        // Test just a filename
        assert_eq!(Utils::display_path_name("file.txt"), "file.txt");

        // Test root path - should return the original path
        assert_eq!(Utils::display_path_name("/"), "/");

        // Test empty string - should return empty string
        assert_eq!(Utils::display_path_name(""), "");

        // Test path with trailing separator
        assert_eq!(Utils::display_path_name("/path/to/dir/"), "dir");
    }
}
