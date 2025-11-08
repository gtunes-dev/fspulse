use chrono::{offset::LocalResult, DateTime, Local, NaiveDate, NaiveDateTime, TimeZone, Utc};
use std::ffi::OsStr;
use std::path::{Path, MAIN_SEPARATOR_STR};
use tico::tico;

use crate::error::FsPulseError;

const NO_DIR_SEPARATOR: &str = "";

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
