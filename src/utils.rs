use chrono::{offset::LocalResult, DateTime, Local, NaiveDate, NaiveDateTime, TimeZone, Utc};
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use std::{
    path::{Path, MAIN_SEPARATOR_STR},
    time::Duration,
};
use tico::tico;

use crate::error::FsPulseError;

const NO_DIR_SEPARATOR: &str = "";

pub struct Utils {}

impl Utils {
    pub fn display_opt_bool(opt_bool: &Option<bool>) -> String {
        match opt_bool {
            Some(true) => "T".into(),
            Some(false) => "F".into(),
            None => "-".into(),
        }
    }

    pub fn display_bool(v: &bool) -> String {
        Utils::display_opt_bool(&Some(*v))
    }

    pub fn display_opt_str<T: AsRef<str>>(opt: &Option<T>) -> String {
        match opt {
            Some(s) => s.as_ref().to_string(),
            None => "-".to_owned(),
        }
    }

    pub fn display_short_path(path: &str) -> String {
        tico(path, None)
    }

    pub fn display_db_time(db_time: &i64) -> String {
        let datetime_utc = DateTime::<Utc>::from_timestamp(*db_time, 0)
            .unwrap_or_else(|| DateTime::<Utc>::from_timestamp(0, 0).unwrap());

        let datetime_local: DateTime<Local> = datetime_utc.with_timezone(&Local);

        datetime_local.format("%Y-%m-%d %H:%M").to_string()
    }

    pub fn display_opt_db_time(opt: &Option<i64>) -> String {
        match opt {
            Some(db_time) => Utils::display_db_time(db_time),
            None => "-".to_owned(),
        }
    }

    /*
    pub fn display_short_path<T: AsRef<str>>(opt: &Option<T>) -> String {
        match opt {
            Some(s) => tico(s.as_ref(), None),
            None => "-".to_owned()
        }
    }
    */

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

    /// Parses a single date string (yyyy-mm-dd) and returns the NaiveDateTime values for:
    /// - start of day (00:00:00)
    /// - end of day (23:59:59)
    fn parse_date_bounds(date_str: &str) -> Result<(NaiveDateTime, NaiveDateTime), FsPulseError> {
        let date = NaiveDate::parse_from_str(date_str, "%Y-%m-%d").map_err(|_| {
            FsPulseError::Error(format!("Failed to parse '{}' as a valid date", date_str))
        })?;

        let start_dt = date.and_hms_opt(0, 0, 0).ok_or_else(|| {
            FsPulseError::Error(format!("Failed to create start time for '{}'", date_str))
        })?;
        let end_dt = date.and_hms_opt(23, 59, 59).ok_or_else(|| {
            FsPulseError::Error(format!("Failed to create end time for '{}'", date_str))
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
                return Err(FsPulseError::Error(format!(
                    "Local start time '{}' is invalid (e.g., during DST gap)",
                    date_str
                )));
            }
        };

        // For end time, use the latest valid time.
        let local_end = match Local.from_local_datetime(&naive_end) {
            LocalResult::Single(dt) => dt,
            LocalResult::Ambiguous(_earliest, latest) => latest,
            LocalResult::None => {
                return Err(FsPulseError::Error(format!(
                    "Local end time '{}' is invalid (e.g., during DST gap)",
                    date_str
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
                return Err(FsPulseError::Error(format!(
                    "Local start time '{}' is invalid (e.g., during DST gap)",
                    start_date_str
                )));
            }
        };

        let local_end = match Local.from_local_datetime(&naive_end) {
            LocalResult::Single(dt) => dt,
            LocalResult::Ambiguous(_, latest) => latest,
            LocalResult::None => {
                return Err(FsPulseError::Error(format!(
                    "Local end time '{}' is invalid (e.g., during DST gap)",
                    end_date_str
                )));
            }
        };

        if local_start > local_end {
            return Err(FsPulseError::Error(format!(
                "Start date '{}' is after end date '{}'",
                start_date_str, end_date_str
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

    pub fn add_section_bar(
        multi_prog: &mut MultiProgress,
        stage_index: i32,
        msg: impl Into<String>,
    ) -> ProgressBar {
        Utils::add_spinner_bar(multi_prog, format!("[{}/3]", stage_index), msg, true)
    }

    pub fn finish_section_bar(section_bar: &ProgressBar, msg: impl Into<String>) {
        section_bar.finish_with_message(msg.into());
    }

    pub fn add_spinner_bar(
        multi_prog: &mut MultiProgress,
        prefix: impl Into<String>,
        msg: impl Into<String>,
        tick: bool,
    ) -> ProgressBar {
        let spinner = multi_prog.add(
            ProgressBar::new_spinner()
                .with_style(
                    ProgressStyle::default_spinner()
                        .template("{prefix}{spinner} {msg}")
                        .unwrap(),
                )
                .with_prefix(prefix.into())
                .with_message(msg.into()),
        );
        if tick {
            spinner.enable_steady_tick(Duration::from_millis(250));
        }

        spinner
    }
}
