use std::path::MAIN_SEPARATOR_STR;

use chrono::{DateTime, Local, Utc};

const NO_DIR_SEPARATOR: &str = "";

pub struct Utils {
}

impl Utils {
    
    /*
    pub fn opt_u64_to_opt_i64(opt_u64: Option<u64>) -> Option<i64> {
        opt_u64.map(|v| v as i64)
    }
    */

    /*
    pub fn opt_u32_to_opt_i64(opt_u32: Option<u32>) -> Option<i64> {
        opt_u32.map(|v| v as i64 )
    }
    */

    /*

    // TODO: Dead code?
    pub fn string_value_or_none(s: &Option<String>) -> &str {
        s.as_deref().unwrap_or("None")
    }

    pub fn str_value_or_none<'a>(s: &'a Option<&'a str>) -> &'a str {
        s.unwrap_or("None")
    }
    */

    pub fn opt_i64_or_none_as_str(opt_i64: Option<i64>) -> String {
        match opt_i64 {
            Some(i) => i.to_string(),
            None => "-".to_string(),
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

    pub fn format_db_time_short(db_time: i64) -> String {
        let datetime_utc = DateTime::<Utc>::from_timestamp(db_time, 0)
            .unwrap_or_else(|| DateTime::<Utc>::from_timestamp(0, 0).unwrap());

        let datetime_local: DateTime<Local> = datetime_utc.with_timezone(&Local);

        datetime_local.format("%Y-%b-%d %H:%M").to_string()

    }

    pub fn format_db_time_short_or_none(db_time: Option<i64>) -> String {
        db_time.map_or("-".to_string(), Self::format_db_time_short)
    }

    pub fn opt_bool_or_none_as_str(opt_bool: Option<bool>) -> &'static str {
        match opt_bool {
            Some(true) => "T",
            Some(false) => "F",
            None => "-",
        }
    }

    /*
    pub fn opt_string_or_none(str: &Option<String>) -> &str {
        match str {
            Some(s) => s.as_str(),
            None => "-",
        }
    }
    */

}