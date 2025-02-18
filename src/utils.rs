use chrono::{DateTime, Local, Utc};

pub struct Utils {
}

impl Utils {
    pub fn opt_u64_to_opt_i64(opt_u64: Option<u64>) -> Option<i64> {
        opt_u64.map(|id| id as i64)
    }

    // TODO: Dead code?
    pub fn string_value_or_none(s: &Option<String>) -> &str {
        s.as_deref().unwrap_or("None")
    }

    pub fn str_value_or_none<'a>(s: &'a Option<&'a str>) -> &'a str {
        s.unwrap_or("None")
    }

    pub fn opt_i64_or_none_as_str(opt_i64: Option<i64>) -> String {
        match opt_i64 {
            Some(i) => i.to_string(),
            None => "None".to_string(),
        }
    }

    pub fn formatted_db_time(db_time: i64) -> String {
        let datetime_utc = DateTime::<Utc>::from_timestamp(db_time, 0)
            .unwrap_or_else(|| DateTime::<Utc>::from_timestamp(0, 0).unwrap());

        let datetime_local: DateTime<Local> = datetime_utc.with_timezone(&Local);

        datetime_local.format("%Y-%m-%d %H:%M:%S").to_string()
    }

}