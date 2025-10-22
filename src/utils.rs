use chrono::{offset::LocalResult, DateTime, Local, NaiveDate, NaiveDateTime, TimeZone, Utc};
use std::path::{Path, MAIN_SEPARATOR_STR};
use tico::tico;

use crate::error::FsPulseError;

const NO_DIR_SEPARATOR: &str = "";

pub struct Utils {}

impl Utils {
    pub fn display_short_path(path: &str) -> String {
        tico(path, None)
    }

    pub fn display_opt_i64(opt_i64: &Option<i64>) -> String {
        match opt_i64 {
            Some(i) => i.to_string(),
            None => "-".into(),
        }
    }

    pub fn dir_sep_or_empty(is_dir: bool) -> &'static str {
        if is_dir {
            MAIN_SEPARATOR_STR
        } else {
            NO_DIR_SEPARATOR
        }
    }

    pub fn _format_db_time(db_time: i64) -> String {
        let datetime_utc = DateTime::<Utc>::from_timestamp(db_time, 0)
            .unwrap_or_else(|| DateTime::<Utc>::from_timestamp(0, 0).unwrap());

        let datetime_local: DateTime<Local> = datetime_utc.with_timezone(&Local);

        datetime_local.format("%Y-%m-%d %H:%M:%S").to_string()
    }

    /// Format a UTC timestamp for web display
    /// Returns relative time ("2h ago") for recent dates, absolute date for older dates
    /// This is the single source of truth for date formatting in web handlers
    pub fn format_date_display(timestamp: i64) -> String {
        let datetime_utc = DateTime::<Utc>::from_timestamp(timestamp, 0)
            .unwrap_or_else(|| DateTime::<Utc>::from_timestamp(0, 0).unwrap());

        let datetime_local: DateTime<Local> = datetime_utc.with_timezone(&Local);
        let now_local = Local::now();

        let duration = now_local.signed_duration_since(datetime_local);
        let seconds = duration.num_seconds();

        // Return relative time for recent dates
        if seconds < 0 {
            // Future date - shouldn't happen, but handle gracefully
            return datetime_local.format("%Y-%m-%d").to_string();
        } else if seconds < 60 {
            return "just now".to_string();
        } else if seconds < 3600 {
            let minutes = seconds / 60;
            return format!("{}m ago", minutes);
        } else if seconds < 86400 {
            let hours = seconds / 3600;
            return format!("{}h ago", hours);
        } else if seconds < 604800 {
            let days = seconds / 86400;
            return format!("{}d ago", days);
        }

        // For dates older than 7 days, return formatted date
        datetime_local.format("%m/%d/%Y").to_string()
    }

    /*
    /// Take a UTC timestamp and create a display string in local time
    pub fn format_db_time_short(db_time: i64) -> String {
        let datetime_utc = DateTime::<Utc>::from_timestamp(db_time, 0)
            .unwrap_or_else(|| DateTime::<Utc>::from_timestamp(0, 0).unwrap());

        let datetime_local: DateTime<Local> = datetime_utc.with_timezone(&Local);

        datetime_local.format("%Y-%b-%d %H:%M").to_string()
    }

    pub fn format_db_time_short_or_none(db_time: Option<i64>) -> String {
        db_time.map_or("-".to_string(), Self::format_db_time_short)
    }
    */

    /// Parses a single date string (yyyy-mm-dd) and returns the NaiveDateTime values for:
    /// - start of day (00:00:00)
    /// - end of day (23:59:59)
    fn parse_date_bounds(date_str: &str) -> Result<(NaiveDateTime, NaiveDateTime), FsPulseError> {
        let date = NaiveDate::parse_from_str(date_str, "%Y-%m-%d").map_err(|_| {
            FsPulseError::CustomParsingError(format!("Invalid date: '{date_str}'"))
        })?;

        let start_dt = date.and_hms_opt(0, 0, 0).ok_or_else(|| {
            FsPulseError::CustomParsingError(format!(
                "Unable to create start time for '{date_str}'"
            ))
        })?;

        let end_dt = date.and_hms_opt(23, 59, 59).ok_or_else(|| {
            FsPulseError::CustomParsingError(format!(
                "Unable to create end time for '{date_str}'"
            ))
        })?;
        Ok((start_dt, end_dt))
    }

    /// For a single date input (assumed to be in local time), returns (start_timestamp, end_timestamp)
    /// as UTC timestamps, choosing the earliest possible time for the start (expanding the lower bound)
    /// and the latest possible time for the end.
    pub fn single_date_bounds(date_str: &str) -> Result<(i64, i64), FsPulseError> {
        let (naive_start, naive_end) = Self::parse_date_bounds(date_str)?;

        // For start time, try to use the earliest valid time.
        let local_start = match Local.from_local_datetime(&naive_start) {
            LocalResult::Single(dt) => dt,
            LocalResult::Ambiguous(earliest, _latest) => earliest,
            LocalResult::None => {
                // For missing times, you might decide to move forward a minute until a valid time is found.
                // Here we simply return an error, but you could adjust to your needs.
                return Err(FsPulseError::CustomParsingError(format!(
                    "Invalid time '{date_str}')"
                )));
            }
        };

        // For end time, use the latest valid time.
        let local_end = match Local.from_local_datetime(&naive_end) {
            LocalResult::Single(dt) => dt,
            LocalResult::Ambiguous(_earliest, latest) => latest,
            LocalResult::None => {
                return Err(FsPulseError::CustomParsingError(format!(
                    "Invalid time '{date_str}')"
                )));
            }
        };

        let start_ts = local_start.with_timezone(&Utc).timestamp();
        let end_ts = local_end.with_timezone(&Utc).timestamp();
        Ok((start_ts, end_ts))
    }

    /// Similar modifications would be applied for a date range.
    pub fn range_date_bounds(
        start_date_str: &str,
        end_date_str: &str,
    ) -> Result<(i64, i64), FsPulseError> {
        let (naive_start, _) = Self::parse_date_bounds(start_date_str)?;
        let (_, naive_end) = Self::parse_date_bounds(end_date_str)?;

        let local_start = match Local.from_local_datetime(&naive_start) {
            LocalResult::Single(dt) => dt,
            LocalResult::Ambiguous(earliest, _) => earliest,
            LocalResult::None => {
                return Err(FsPulseError::CustomParsingError(format!(
                    "Invalid start time '{start_date_str}')"
                )));
            }
        };

        let local_end = match Local.from_local_datetime(&naive_end) {
            LocalResult::Single(dt) => dt,
            LocalResult::Ambiguous(_, latest) => latest,
            LocalResult::None => {
                return Err(FsPulseError::CustomParsingError(format!(
                    "Invalid end time '{end_date_str}')"
                )));
            }
        };

        if local_start > local_end {
            return Err(FsPulseError::CustomParsingError(format!(
                "Start date '{start_date_str}' is after end date '{end_date_str}'"
            )));
        }

        let start_ts = local_start.with_timezone(&Utc).timestamp();
        let end_ts = local_end.with_timezone(&Utc).timestamp();
        Ok((start_ts, end_ts))
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

    #[test]
    fn test_display_opt_i64_some() {
        assert_eq!(Utils::display_opt_i64(&Some(42)), "42");
        assert_eq!(Utils::display_opt_i64(&Some(-1)), "-1");
        assert_eq!(Utils::display_opt_i64(&Some(0)), "0");
    }

    #[test]
    fn test_display_opt_i64_none() {
        assert_eq!(Utils::display_opt_i64(&None), "-");
    }

    #[test]
    fn test_dir_sep_or_empty() {
        assert_eq!(Utils::dir_sep_or_empty(true), std::path::MAIN_SEPARATOR_STR);
        assert_eq!(Utils::dir_sep_or_empty(false), "");
    }

    #[test]
    fn test_single_date_bounds_valid() {
        let result = Utils::single_date_bounds("2023-12-25");
        assert!(result.is_ok());
        let (start, end) = result.unwrap();
        assert!(start <= end);
        assert!(end - start <= 86400); // Should be within 24 hours
    }

    #[test]
    fn test_single_date_bounds_invalid() {
        assert!(Utils::single_date_bounds("invalid-date").is_err());
        assert!(Utils::single_date_bounds("2023-13-25").is_err());
        assert!(Utils::single_date_bounds("").is_err());
    }

    #[test]
    fn test_range_date_bounds_valid() {
        let result = Utils::range_date_bounds("2023-12-24", "2023-12-25");
        assert!(result.is_ok());
        let (start, end) = result.unwrap();
        assert!(start <= end);
    }

    #[test]
    fn test_range_date_bounds_reversed() {
        let result = Utils::range_date_bounds("2023-12-25", "2023-12-24");
        assert!(result.is_err());
    }

    #[test]
    fn test_display_short_path() {
        let short_path = Utils::display_short_path("/very/long/path/to/file.txt");
        assert!(!short_path.is_empty());

        let normal_path = Utils::display_short_path("short.txt");
        assert_eq!(normal_path, "short.txt");
    }

    #[test]
    fn test_format_date_display_just_now() {
        let now = Local::now().timestamp();
        let result = Utils::format_date_display(now);
        assert_eq!(result, "just now");
    }

    #[test]
    fn test_format_date_display_minutes() {
        let now = Local::now();
        let five_min_ago = now - chrono::Duration::minutes(5);
        let result = Utils::format_date_display(five_min_ago.timestamp());
        assert_eq!(result, "5m ago");
    }

    #[test]
    fn test_format_date_display_hours() {
        let now = Local::now();
        let two_hours_ago = now - chrono::Duration::hours(2);
        let result = Utils::format_date_display(two_hours_ago.timestamp());
        assert_eq!(result, "2h ago");
    }

    #[test]
    fn test_format_date_display_days() {
        let now = Local::now();
        let three_days_ago = now - chrono::Duration::days(3);
        let result = Utils::format_date_display(three_days_ago.timestamp());
        assert_eq!(result, "3d ago");
    }

    #[test]
    fn test_format_date_display_old_date() {
        let now = Local::now();
        let eight_days_ago = now - chrono::Duration::days(8);
        let result = Utils::format_date_display(eight_days_ago.timestamp());
        // Should return formatted date like "01/15/2025"
        assert!(result.contains("/"));
        assert!(result.len() >= 8); // MM/DD/YYYY format
    }

    #[test]
    fn test_format_date_display_future() {
        let now = Local::now();
        let future = now + chrono::Duration::hours(1);
        let result = Utils::format_date_display(future.timestamp());
        // Should handle gracefully, returning a formatted date
        assert!(!result.is_empty());
    }
}
