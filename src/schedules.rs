use serde::{Deserialize, Serialize};
use crate::database::Database;
use crate::error::FsPulseError;
use crate::scans::{AnalysisSpec, HashMode, Scan, ValidateMode};
use crate::roots::Root;
use rusqlite::OptionalExtension;

/// Schedule type: Daily, Weekly, Interval, or Monthly
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(i32)]
pub enum ScheduleType {
    Daily = 0,
    Weekly = 1,
    Interval = 2,
    Monthly = 3,
}

impl ScheduleType {
    pub fn from_i32(value: i32) -> Option<Self> {
        match value {
            0 => Some(Self::Daily),
            1 => Some(Self::Weekly),
            2 => Some(Self::Interval),
            3 => Some(Self::Monthly),
            _ => None,
        }
    }

    pub fn as_i32(self) -> i32 {
        self as i32
    }
}

/// Interval unit for interval-based schedules
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(i32)]
pub enum IntervalUnit {
    Minutes = 0,
    Hours = 1,
    Days = 2,
    Weeks = 3,
}

impl IntervalUnit {
    pub fn from_i32(value: i32) -> Option<Self> {
        match value {
            0 => Some(Self::Minutes),
            1 => Some(Self::Hours),
            2 => Some(Self::Days),
            3 => Some(Self::Weeks),
            _ => None,
        }
    }

    pub fn as_i32(self) -> i32 {
        self as i32
    }

    /// Convert interval to seconds
    pub fn to_seconds(self, value: i64) -> i64 {
        match self {
            Self::Minutes => value * 60,
            Self::Hours => value * 3600,
            Self::Days => value * 86400,
            Self::Weeks => value * 604800,
        }
    }
}

/// Source of a scan queue entry
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(i32)]
pub enum SourceType {
    Manual = 0,
    Scheduled = 1,
}

impl SourceType {
    pub fn from_i32(value: i32) -> Option<Self> {
        match value {
            0 => Some(Self::Manual),
            1 => Some(Self::Scheduled),
            _ => None,
        }
    }

    pub fn as_i32(self) -> i32 {
        self as i32
    }
}

/// A scan schedule configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Schedule {
    pub schedule_id: i64,
    pub root_id: i64,
    pub enabled: bool,
    pub schedule_name: String,
    pub schedule_type: ScheduleType,

    // For daily/weekly/monthly schedules: 'HH:MM' format (24-hour)
    pub time_of_day: Option<String>,

    // For weekly schedules: JSON array of day names
    // Example: '["Mon","Wed","Fri"]'
    pub days_of_week: Option<String>,

    // For monthly schedules: day of month (1-31)
    pub day_of_month: Option<i64>,

    // For interval schedules: repeat every N minutes/hours/days/weeks
    pub interval_value: Option<i64>,
    pub interval_unit: Option<IntervalUnit>,

    // Scan options
    pub hash_mode: HashMode,
    pub validate_mode: ValidateMode,

    // Metadata
    pub created_at: i64,  // Unix timestamp (UTC)
    pub updated_at: i64,  // Unix timestamp (UTC)
}

impl Schedule {
    /// Validate that schedule fields are consistent with schedule_type
    pub fn validate(&self) -> Result<(), String> {
        match self.schedule_type {
            ScheduleType::Daily => {
                // Daily: time_of_day required, others NULL
                if self.time_of_day.is_none() {
                    return Err("Daily schedule requires time_of_day".to_string());
                }
                if self.days_of_week.is_some() {
                    return Err("Daily schedule should not have days_of_week".to_string());
                }
                if self.day_of_month.is_some() {
                    return Err("Daily schedule should not have day_of_month".to_string());
                }
                if self.interval_value.is_some() || self.interval_unit.is_some() {
                    return Err("Daily schedule should not have interval fields".to_string());
                }
            }
            ScheduleType::Weekly => {
                // Weekly: time_of_day + days_of_week required, others NULL
                if self.time_of_day.is_none() {
                    return Err("Weekly schedule requires time_of_day".to_string());
                }
                if self.days_of_week.is_none() {
                    return Err("Weekly schedule requires days_of_week".to_string());
                }
                if self.day_of_month.is_some() {
                    return Err("Weekly schedule should not have day_of_month".to_string());
                }
                if self.interval_value.is_some() || self.interval_unit.is_some() {
                    return Err("Weekly schedule should not have interval fields".to_string());
                }
            }
            ScheduleType::Interval => {
                // Interval: interval_value + interval_unit required, others NULL
                if self.interval_value.is_none() {
                    return Err("Interval schedule requires interval_value".to_string());
                }
                if self.interval_unit.is_none() {
                    return Err("Interval schedule requires interval_unit".to_string());
                }
                if let Some(value) = self.interval_value {
                    if value <= 0 {
                        return Err("Interval value must be positive".to_string());
                    }
                }
                if self.time_of_day.is_some() {
                    return Err("Interval schedule should not have time_of_day".to_string());
                }
                if self.days_of_week.is_some() {
                    return Err("Interval schedule should not have days_of_week".to_string());
                }
                if self.day_of_month.is_some() {
                    return Err("Interval schedule should not have day_of_month".to_string());
                }
            }
            ScheduleType::Monthly => {
                // Monthly: time_of_day + day_of_month required, others NULL
                if self.time_of_day.is_none() {
                    return Err("Monthly schedule requires time_of_day".to_string());
                }
                if self.day_of_month.is_none() {
                    return Err("Monthly schedule requires day_of_month".to_string());
                }
                if let Some(day) = self.day_of_month {
                    if day < 1 || day > 31 {
                        return Err(format!("day_of_month must be 1-31, got: {}", day));
                    }
                }
                if self.days_of_week.is_some() {
                    return Err("Monthly schedule should not have days_of_week".to_string());
                }
                if self.interval_value.is_some() || self.interval_unit.is_some() {
                    return Err("Monthly schedule should not have interval fields".to_string());
                }
            }
        }

        // Validate time_of_day format if present
        if let Some(ref time) = self.time_of_day {
            self.validate_time_of_day(time)?;
        }

        // Validate days_of_week format if present
        if let Some(ref days) = self.days_of_week {
            self.validate_days_of_week(days)?;
        }

        // day_of_month validation is done in Monthly match arm above

        Ok(())
    }

    /// Validate time_of_day is in 'HH:MM' format
    fn validate_time_of_day(&self, time: &str) -> Result<(), String> {
        let parts: Vec<&str> = time.split(':').collect();
        if parts.len() != 2 {
            return Err(format!("time_of_day must be in HH:MM format, got: {}", time));
        }

        let hours: u32 = parts[0]
            .parse()
            .map_err(|_| format!("Invalid hours in time_of_day: {}", parts[0]))?;
        let minutes: u32 = parts[1]
            .parse()
            .map_err(|_| format!("Invalid minutes in time_of_day: {}", parts[1]))?;

        if hours >= 24 {
            return Err(format!("Hours must be 0-23, got: {}", hours));
        }
        if minutes >= 60 {
            return Err(format!("Minutes must be 0-59, got: {}", minutes));
        }

        Ok(())
    }

    /// Validate days_of_week is valid JSON array of day names
    fn validate_days_of_week(&self, days: &str) -> Result<(), String> {
        let parsed: Vec<String> = serde_json::from_str(days)
            .map_err(|e| format!("Invalid JSON in days_of_week: {}", e))?;

        if parsed.is_empty() {
            return Err("days_of_week cannot be empty".to_string());
        }

        let valid_days = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
        for day in &parsed {
            if !valid_days.contains(&day.as_str()) {
                return Err(format!("Invalid day name: {}", day));
            }
        }

        Ok(())
    }

    // ========================================
    // Next scan time calculation
    // ========================================

    /// Calculate the next run time for this schedule starting from a reference time
    /// Returns Unix timestamp (UTC)
    pub fn calculate_next_scan_time(&self, from_time: i64) -> Result<i64, String> {
        match self.schedule_type {
            ScheduleType::Interval => {
                let unit = self.interval_unit
                    .ok_or("Interval schedule missing interval_unit")?;
                let value = self.interval_value
                    .ok_or("Interval schedule missing interval_value")?;
                let seconds = unit.to_seconds(value);
                Ok(from_time + seconds)
            }

            ScheduleType::Daily => {
                let time_str = self.time_of_day.as_ref()
                    .ok_or("Daily schedule missing time_of_day")?;
                self.calculate_next_daily(from_time, time_str)
            }

            ScheduleType::Weekly => {
                let time_str = self.time_of_day.as_ref()
                    .ok_or("Weekly schedule missing time_of_day")?;
                let days_str = self.days_of_week.as_ref()
                    .ok_or("Weekly schedule missing days_of_week")?;
                self.calculate_next_weekly(from_time, time_str, days_str)
            }

            ScheduleType::Monthly => {
                let time_str = self.time_of_day.as_ref()
                    .ok_or("Monthly schedule missing time_of_day")?;
                let day = self.day_of_month
                    .ok_or("Monthly schedule missing day_of_month")?;
                self.calculate_next_monthly(from_time, time_str, day)
            }
        }
    }

    /// Calculate next daily occurrence
    fn calculate_next_daily(&self, from_time: i64, time_of_day: &str) -> Result<i64, String> {
        use chrono::{Local, TimeZone, Timelike};

        // Parse HH:MM
        let parts: Vec<&str> = time_of_day.split(':').collect();
        let hours: u32 = parts[0].parse()
            .map_err(|_| format!("Invalid hours: {}", parts[0]))?;
        let minutes: u32 = parts[1].parse()
            .map_err(|_| format!("Invalid minutes: {}", parts[1]))?;

        // Get current time in local timezone
        let from_local = Local.timestamp_opt(from_time, 0).single()
            .ok_or("Invalid timestamp")?;

        // Try today at the scheduled time
        let today_at_time = from_local
            .with_hour(hours).ok_or("Invalid hour")?
            .with_minute(minutes).ok_or("Invalid minute")?
            .with_second(0).ok_or("Failed to set seconds")?
            .with_nanosecond(0).ok_or("Failed to set nanoseconds")?;

        // If that time has already passed today, use tomorrow
        let next_occurrence = if today_at_time.timestamp() > from_time {
            today_at_time
        } else {
            today_at_time + chrono::Duration::days(1)
        };

        Ok(next_occurrence.timestamp())
    }

    /// Calculate next weekly occurrence
    fn calculate_next_weekly(&self, from_time: i64, time_of_day: &str, days_of_week: &str) -> Result<i64, String> {
        use chrono::{Datelike, Local, TimeZone, Timelike, Weekday};

        // Parse HH:MM
        let parts: Vec<&str> = time_of_day.split(':').collect();
        let hours: u32 = parts[0].parse()
            .map_err(|_| format!("Invalid hours: {}", parts[0]))?;
        let minutes: u32 = parts[1].parse()
            .map_err(|_| format!("Invalid minutes: {}", parts[1]))?;

        // Parse days_of_week JSON
        let day_names: Vec<String> = serde_json::from_str(days_of_week)
            .map_err(|e| format!("Invalid JSON in days_of_week: {}", e))?;

        // Map day names to Weekday enum
        let target_weekdays: Vec<Weekday> = day_names.iter()
            .map(|name| match name.as_str() {
                "Mon" => Ok(Weekday::Mon),
                "Tue" => Ok(Weekday::Tue),
                "Wed" => Ok(Weekday::Wed),
                "Thu" => Ok(Weekday::Thu),
                "Fri" => Ok(Weekday::Fri),
                "Sat" => Ok(Weekday::Sat),
                "Sun" => Ok(Weekday::Sun),
                _ => Err(format!("Invalid day name: {}", name)),
            })
            .collect::<Result<Vec<_>, _>>()?;

        if target_weekdays.is_empty() {
            return Err("No days specified for weekly schedule".to_string());
        }

        // Get current time in local timezone
        let from_local = Local.timestamp_opt(from_time, 0).single()
            .ok_or("Invalid timestamp")?;

        // Check today first
        let today_at_time = from_local
            .with_hour(hours).ok_or("Invalid hour")?
            .with_minute(minutes).ok_or("Invalid minute")?
            .with_second(0).ok_or("Failed to set seconds")?
            .with_nanosecond(0).ok_or("Failed to set nanoseconds")?;

        if target_weekdays.contains(&from_local.weekday()) && today_at_time.timestamp() > from_time {
            return Ok(today_at_time.timestamp());
        }

        // Search next 7 days for matching weekday
        for days_ahead in 1..=7 {
            let candidate = from_local + chrono::Duration::days(days_ahead);
            if target_weekdays.contains(&candidate.weekday()) {
                let next_occurrence = candidate
                    .with_hour(hours).ok_or("Invalid hour")?
                    .with_minute(minutes).ok_or("Invalid minute")?
                    .with_second(0).ok_or("Failed to set seconds")?
                    .with_nanosecond(0).ok_or("Failed to set nanoseconds")?;
                return Ok(next_occurrence.timestamp());
            }
        }

        Err("Failed to find next weekly occurrence".to_string())
    }

    /// Calculate next monthly occurrence
    fn calculate_next_monthly(&self, from_time: i64, time_of_day: &str, day_of_month: i64) -> Result<i64, String> {
        use chrono::{Datelike, Local, TimeZone, Timelike};

        if day_of_month < 1 || day_of_month > 31 {
            return Err(format!("Invalid day_of_month: {}", day_of_month));
        }

        // Parse HH:MM
        let parts: Vec<&str> = time_of_day.split(':').collect();
        let hours: u32 = parts[0].parse()
            .map_err(|_| format!("Invalid hours: {}", parts[0]))?;
        let minutes: u32 = parts[1].parse()
            .map_err(|_| format!("Invalid minutes: {}", parts[1]))?;

        // Get current time in local timezone
        let from_local = Local.timestamp_opt(from_time, 0).single()
            .ok_or("Invalid timestamp")?;

        // Try to find next occurrence within the next 12 months
        for month_offset in 0..12 {
            let candidate_month = from_local + chrono::Duration::days(30 * month_offset);

            // Get the last day of this month to check if our target day exists
            let year = candidate_month.year();
            let month = candidate_month.month();

            // Check if this month has the target day
            // Try to create a date with the target day
            let candidate_date = Local.with_ymd_and_hms(
                year,
                month,
                day_of_month as u32,
                hours,
                minutes,
                0
            ).single();

            if let Some(candidate) = candidate_date {
                // Check if this is in the future
                if candidate.timestamp() > from_time {
                    return Ok(candidate.timestamp());
                }
            }
            // If day doesn't exist in this month (e.g., Feb 31), skip to next month
        }

        Err(format!("Failed to find next monthly occurrence for day {}", day_of_month))
    }

    // ========================================
    // Database operations
    // ========================================

    /// Create a new schedule and queue entry atomically
    /// This is the primary way to create schedules from API/UI
    /// Creates schedule as enabled by default
    pub fn create_and_queue(
        db: &Database,
        root_id: i64,
        schedule_name: String,
        schedule_type: ScheduleType,
        time_of_day: Option<String>,
        days_of_week: Option<String>,
        day_of_month: Option<i64>,
        interval_value: Option<i64>,
        interval_unit: Option<IntervalUnit>,
        hash_mode: HashMode,
        validate_mode: ValidateMode,
    ) -> Result<Self, FsPulseError> {
        let now = chrono::Utc::now().timestamp();

        // Build schedule
        let schedule = Schedule {
            schedule_id: 0, // Will be set by database
            root_id,
            enabled: true,  // Always create as enabled
            schedule_name,
            schedule_type,
            time_of_day,
            days_of_week,
            day_of_month,
            interval_value,
            interval_unit,
            hash_mode,
            validate_mode,
            created_at: now,
            updated_at: now,
        };

        // Validate schedule fields
        schedule.validate()
            .map_err(|e| FsPulseError::Error(format!("Invalid schedule: {}", e)))?;

        // Calculate next_scan_time BEFORE transaction
        // This ensures we fail fast if calculation fails, avoiding transaction rollback
        let next_scan_time = schedule.calculate_next_scan_time(now)
            .map_err(|e| FsPulseError::Error(format!("Failed to calculate next scan time: {}", e)))?;

        // Now perform database operations in transaction
        db.immediate_transaction(|conn| {
            // Insert schedule
            let schedule_id: i64 = conn.query_row(
                "INSERT INTO scan_schedules (
                    root_id, enabled, schedule_name, schedule_type,
                    time_of_day, days_of_week, day_of_month,
                    interval_value, interval_unit,
                    hash_mode, validate_mode,
                    created_at, updated_at
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                RETURNING schedule_id",
                rusqlite::params![
                    root_id,
                    schedule.enabled,
                    schedule.schedule_name,
                    schedule.schedule_type.as_i32(),
                    schedule.time_of_day,
                    schedule.days_of_week,
                    schedule.day_of_month,
                    schedule.interval_value,
                    schedule.interval_unit.map(|u| u.as_i32()),
                    schedule.hash_mode.as_i32(),
                    schedule.validate_mode.as_i32(),
                    schedule.created_at,
                    schedule.updated_at,
                ],
                |row| row.get(0),
            )
            .map_err(FsPulseError::DatabaseError)?;

            // Insert queue entry (schedule is enabled by default)
            conn.execute(
                "INSERT INTO scan_queue (
                    root_id, schedule_id, scan_id, next_scan_time,
                    hash_mode, validate_mode, source, created_at
                ) VALUES (?, ?, NULL, ?, ?, ?, ?, ?)",
                rusqlite::params![
                    root_id,
                    schedule_id,
                    next_scan_time,
                    schedule.hash_mode.as_i32(),
                    schedule.validate_mode.as_i32(),
                    SourceType::Scheduled.as_i32(),
                    now,
                ],
            )
            .map_err(FsPulseError::DatabaseError)?;

            Ok(Schedule {
                schedule_id,
                ..schedule
            })
        })
    }

    /// Get a schedule by ID
    pub fn get_by_id(conn: &rusqlite::Connection, schedule_id: i64) -> Result<Option<Self>, FsPulseError> {
        conn.query_row(
            "SELECT
                schedule_id, root_id, enabled, schedule_name, schedule_type,
                time_of_day, days_of_week, day_of_month,
                interval_value, interval_unit,
                hash_mode, validate_mode,
                created_at, updated_at
            FROM scan_schedules
            WHERE schedule_id = ?",
            [schedule_id],
            |row| {
                Ok(Schedule {
                    schedule_id: row.get(0)?,
                    root_id: row.get(1)?,
                    enabled: row.get(2)?,
                    schedule_name: row.get(3)?,
                    schedule_type: ScheduleType::from_i32(row.get(4)?)
                        .ok_or_else(|| rusqlite::Error::InvalidColumnType(4, "schedule_type".to_string(), rusqlite::types::Type::Integer))?,
                    time_of_day: row.get(5)?,
                    days_of_week: row.get(6)?,
                    day_of_month: row.get(7)?,
                    interval_value: row.get(8)?,
                    interval_unit: row.get::<_, Option<i32>>(9)?
                        .map(|v| IntervalUnit::from_i32(v).ok_or_else(||
                            rusqlite::Error::InvalidColumnType(9, "interval_unit".to_string(), rusqlite::types::Type::Integer)
                        ))
                        .transpose()?,
                    hash_mode: HashMode::from_i32(row.get(10)?)
                        .ok_or_else(|| rusqlite::Error::InvalidColumnType(10, "hash_mode".to_string(), rusqlite::types::Type::Integer))?,
                    validate_mode: ValidateMode::from_i32(row.get(11)?)
                        .ok_or_else(|| rusqlite::Error::InvalidColumnType(11, "validate_mode".to_string(), rusqlite::types::Type::Integer))?,
                    created_at: row.get(12)?,
                    updated_at: row.get(13)?,
                })
            },
        )
        .optional()
        .map_err(FsPulseError::DatabaseError)
    }

    /// Get all schedules for a specific root
    pub fn get_by_root(db: &Database, root_id: i64) -> Result<Vec<Self>, FsPulseError> {
        let conn = db.conn();
        let mut stmt = conn.prepare(
            "SELECT
                schedule_id, root_id, enabled, schedule_name, schedule_type,
                time_of_day, days_of_week, day_of_month,
                interval_value, interval_unit,
                hash_mode, validate_mode,
                created_at, updated_at
            FROM scan_schedules
            WHERE root_id = ?
            ORDER BY schedule_id ASC",
        )?;

        let rows = stmt.query_map([root_id], |row| {
            Ok(Schedule {
                schedule_id: row.get(0)?,
                root_id: row.get(1)?,
                enabled: row.get(2)?,
                schedule_name: row.get(3)?,
                schedule_type: ScheduleType::from_i32(row.get(4)?)
                    .ok_or_else(|| rusqlite::Error::InvalidColumnType(4, "schedule_type".to_string(), rusqlite::types::Type::Integer))?,
                time_of_day: row.get(5)?,
                days_of_week: row.get(6)?,
                day_of_month: row.get(7)?,
                interval_value: row.get(8)?,
                interval_unit: row.get::<_, Option<i32>>(9)?
                    .map(|v| IntervalUnit::from_i32(v).ok_or_else(||
                        rusqlite::Error::InvalidColumnType(9, "interval_unit".to_string(), rusqlite::types::Type::Integer)
                    ))
                    .transpose()?,
                hash_mode: HashMode::from_i32(row.get(10)?)
                    .ok_or_else(|| rusqlite::Error::InvalidColumnType(10, "hash_mode".to_string(), rusqlite::types::Type::Integer))?,
                validate_mode: ValidateMode::from_i32(row.get(11)?)
                    .ok_or_else(|| rusqlite::Error::InvalidColumnType(11, "validate_mode".to_string(), rusqlite::types::Type::Integer))?,
                created_at: row.get(12)?,
                updated_at: row.get(13)?,
            })
        })?;

        let mut schedules = Vec::new();
        for row in rows {
            schedules.push(row?);
        }

        Ok(schedules)
    }

    /// Get all enabled schedules (used for queue sync)
    pub fn get_all_enabled(db: &Database) -> Result<Vec<Self>, FsPulseError> {
        let conn = db.conn();
        let mut stmt = conn.prepare(
            "SELECT
                schedule_id, root_id, enabled, schedule_name, schedule_type,
                time_of_day, days_of_week, day_of_month,
                interval_value, interval_unit,
                hash_mode, validate_mode,
                created_at, updated_at
            FROM scan_schedules
            WHERE enabled = 1
            ORDER BY schedule_id ASC",
        )?;

        let rows = stmt.query_map([], |row| {
            Ok(Schedule {
                schedule_id: row.get(0)?,
                root_id: row.get(1)?,
                enabled: row.get(2)?,
                schedule_name: row.get(3)?,
                schedule_type: ScheduleType::from_i32(row.get(4)?)
                    .ok_or_else(|| rusqlite::Error::InvalidColumnType(4, "schedule_type".to_string(), rusqlite::types::Type::Integer))?,
                time_of_day: row.get(5)?,
                days_of_week: row.get(6)?,
                day_of_month: row.get(7)?,
                interval_value: row.get(8)?,
                interval_unit: row.get::<_, Option<i32>>(9)?
                    .map(|v| IntervalUnit::from_i32(v).ok_or_else(||
                        rusqlite::Error::InvalidColumnType(9, "interval_unit".to_string(), rusqlite::types::Type::Integer)
                    ))
                    .transpose()?,
                hash_mode: HashMode::from_i32(row.get(10)?)
                    .ok_or_else(|| rusqlite::Error::InvalidColumnType(10, "hash_mode".to_string(), rusqlite::types::Type::Integer))?,
                validate_mode: ValidateMode::from_i32(row.get(11)?)
                    .ok_or_else(|| rusqlite::Error::InvalidColumnType(11, "validate_mode".to_string(), rusqlite::types::Type::Integer))?,
                created_at: row.get(12)?,
                updated_at: row.get(13)?,
            })
        })?;

        let mut schedules = Vec::new();
        for row in rows {
            schedules.push(row?);
        }

        Ok(schedules)
    }

    /// Update an existing schedule and recalculate next_scan_time
    /// This atomically updates both the schedule and its queue entry
    pub fn update(&self, db: &Database) -> Result<(), FsPulseError> {
        // Validate before updating
        self.validate()
            .map_err(|e| FsPulseError::Error(format!("Invalid schedule: {}", e)))?;

        let now = chrono::Utc::now().timestamp();

        // Calculate next_scan_time BEFORE transaction
        let next_scan_time = self.calculate_next_scan_time(now)
            .map_err(|e| FsPulseError::Error(format!("Failed to calculate next scan time: {}", e)))?;

        db.immediate_transaction(|conn| {
            // Update schedule
            let rows_affected = conn.execute(
                "UPDATE scan_schedules SET
                    schedule_name = ?,
                    schedule_type = ?,
                    time_of_day = ?,
                    days_of_week = ?,
                    day_of_month = ?,
                    interval_value = ?,
                    interval_unit = ?,
                    hash_mode = ?,
                    validate_mode = ?,
                    updated_at = ?
                WHERE schedule_id = ?",
                rusqlite::params![
                    self.schedule_name,
                    self.schedule_type.as_i32(),
                    self.time_of_day,
                    self.days_of_week,
                    self.day_of_month,
                    self.interval_value,
                    self.interval_unit.map(|u| u.as_i32()),
                    self.hash_mode.as_i32(),
                    self.validate_mode.as_i32(),
                    now,
                    self.schedule_id,
                ],
            )
            .map_err(FsPulseError::DatabaseError)?;

            if rows_affected == 0 {
                return Err(FsPulseError::Error(format!(
                    "Schedule with id {} not found",
                    self.schedule_id
                )));
            }

            // Update next_scan_time in queue (if queue entry exists)
            conn.execute(
                "UPDATE scan_queue SET next_scan_time = ? WHERE schedule_id = ?",
                rusqlite::params![next_scan_time, self.schedule_id],
            )
            .map_err(FsPulseError::DatabaseError)?;

            Ok(())
        })
    }

    /// Enable or disable a schedule
    /// When disabling: sets next_scan_time to NULL in queue (running scan completes normally)
    /// When re-enabling: recalculates and sets next_scan_time
    pub fn set_enabled(db: &Database, schedule_id: i64, enabled: bool) -> Result<(), FsPulseError> {
        let now = chrono::Utc::now().timestamp();

        // If enabling, we need to recalculate next_scan_time
        // Get the schedule to calculate next_scan_time BEFORE transaction
        let next_scan_time = if enabled {
            let schedule = Self::get_by_id(db.conn(), schedule_id)?
                .ok_or_else(|| FsPulseError::Error(format!("Schedule {} not found", schedule_id)))?;

            Some(schedule.calculate_next_scan_time(now)
                .map_err(|e| FsPulseError::Error(format!("Failed to calculate next scan time: {}", e)))?)
        } else {
            None
        };

        db.immediate_transaction(|conn| {
            // Update the schedule
            let rows_affected = conn.execute(
                "UPDATE scan_schedules SET enabled = ?, updated_at = ? WHERE schedule_id = ?",
                rusqlite::params![enabled, now, schedule_id],
            )
            .map_err(FsPulseError::DatabaseError)?;

            if rows_affected == 0 {
                return Err(FsPulseError::Error(format!(
                    "Schedule with id {} not found",
                    schedule_id
                )));
            }

            // Update queue entry based on enabled state
            if enabled {
                // Re-enabling: create queue entry if it doesn't exist, or update next_scan_time
                let queue_exists: bool = conn.query_row(
                    "SELECT COUNT(*) FROM scan_queue WHERE schedule_id = ?",
                    [schedule_id],
                    |row| row.get::<_, i64>(0),
                )
                .map(|count| count > 0)
                .map_err(FsPulseError::DatabaseError)?;

                if queue_exists {
                    // Update existing queue entry
                    conn.execute(
                        "UPDATE scan_queue SET next_scan_time = ? WHERE schedule_id = ?",
                        rusqlite::params![next_scan_time.unwrap(), schedule_id],
                    )
                    .map_err(FsPulseError::DatabaseError)?;
                } else {
                    // Create new queue entry (shouldn't normally happen, but handle it)
                    // Need to get schedule details
                    let schedule = Self::get_by_id(conn, schedule_id)?
                        .ok_or_else(|| FsPulseError::Error(format!("Schedule {} not found", schedule_id)))?;

                    conn.execute(
                        "INSERT INTO scan_queue (
                            root_id, schedule_id, scan_id, next_scan_time,
                            hash_mode, validate_mode, source, created_at
                        ) VALUES (?, ?, NULL, ?, ?, ?, ?, ?)",
                        rusqlite::params![
                            schedule.root_id,
                            schedule_id,
                            next_scan_time.unwrap(),
                            schedule.hash_mode.as_i32(),
                            schedule.validate_mode.as_i32(),
                            SourceType::Scheduled.as_i32(),
                            now,
                        ],
                    )
                    .map_err(FsPulseError::DatabaseError)?;
                }
            } else {
                // Disabling: set next_scan_time to NULL
                conn.execute(
                    "UPDATE scan_queue SET next_scan_time = NULL WHERE schedule_id = ?",
                    [schedule_id]
                )
                .map_err(FsPulseError::DatabaseError)?;
            }

            Ok(())
        })
    }

    /// Delete a schedule and its associated queue entry
    /// Fails if a scan is currently running for this schedule
    pub fn delete(db: &Database, schedule_id: i64) -> Result<(), FsPulseError> {
        db.immediate_transaction(|conn| {
            // Check if scan is currently running for this schedule
            let scan_id: Option<Option<i64>> = conn.query_row(
                "SELECT scan_id FROM scan_queue WHERE schedule_id = ?",
                [schedule_id],
                |row| row.get(0)
            )
            .optional()
            .map_err(FsPulseError::DatabaseError)?;

            // scan_id is Some(Some(id)) if queue entry exists with a running scan
            // scan_id is Some(None) if queue entry exists but no scan running
            // scan_id is None if no queue entry exists
            if let Some(Some(_)) = scan_id {
                return Err(FsPulseError::Error(
                    "Cannot delete schedule while scan is in progress. Stop the scan first.".to_string()
                ));
            }

            // Delete queue entry first (references schedule_id)
            conn.execute(
                "DELETE FROM scan_queue WHERE schedule_id = ?",
                [schedule_id]
            )
            .map_err(FsPulseError::DatabaseError)?;

            // Delete the schedule itself
            let rows_affected = conn.execute(
                "DELETE FROM scan_schedules WHERE schedule_id = ?",
                [schedule_id]
            )
            .map_err(FsPulseError::DatabaseError)?;

            if rows_affected == 0 {
                return Err(FsPulseError::Error(format!(
                    "Schedule with id {} not found",
                    schedule_id
                )));
            }

            Ok(())
        })
    }

}

/// A scan queue entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueueEntry {
    pub queue_id: i64,
    pub root_id: i64,
    pub schedule_id: Option<i64>,  // NULL for manual scans
    pub scan_id: Option<i64>,      // NULL until scan starts, set by ScanManager

    // When this work should run (Unix timestamp, UTC)
    // NULL when schedule is disabled
    pub next_scan_time: Option<i64>,

    // Scan configuration
    pub hash_mode: HashMode,
    pub validate_mode: ValidateMode,

    // Source: Manual or Scheduled
    pub source: SourceType,

    // Metadata
    pub created_at: i64,  // Unix timestamp (UTC)
}

impl QueueEntry {
    // ========================================
    // Database operations
    // ========================================

    /// Create a new manual queue entry
    pub fn create_manual(
        db: &Database,
        root_id: i64,
        hash_mode: HashMode,
        validate_mode: ValidateMode,
    ) -> Result<Self, FsPulseError> {
        let now = chrono::Utc::now().timestamp();
        let conn = db.conn();

        let queue_id: i64 = conn.query_row(
            "INSERT INTO scan_queue (
                root_id, schedule_id, scan_id, next_scan_time,
                hash_mode, validate_mode, source, created_at
            ) VALUES (?, NULL, NULL, ?, ?, ?, ?, ?)
            RETURNING queue_id",
            rusqlite::params![
                root_id,
                now,  // Manual scans run immediately
                hash_mode.as_i32(),
                validate_mode.as_i32(),
                SourceType::Manual.as_i32(),
                now,
            ],
            |row| row.get(0),
        )
        .map_err(FsPulseError::DatabaseError)?;

        Ok(QueueEntry {
            queue_id,
            root_id,
            schedule_id: None,
            scan_id: None,
            next_scan_time: Some(now),
            hash_mode,
            validate_mode,
            source: SourceType::Manual,
            created_at: now,
        })
    }

    /// Get a queue entry by ID
    pub fn get_by_id(db: &Database, queue_id: i64) -> Result<Option<Self>, FsPulseError> {
        let conn = db.conn();

        conn.query_row(
            "SELECT
                queue_id, root_id, schedule_id, scan_id, next_scan_time,
                hash_mode, validate_mode, source, created_at
            FROM scan_queue
            WHERE queue_id = ?",
            [queue_id],
            |row| {
                Ok(QueueEntry {
                    queue_id: row.get(0)?,
                    root_id: row.get(1)?,
                    schedule_id: row.get(2)?,
                    scan_id: row.get(3)?,
                    next_scan_time: row.get(4)?,
                    hash_mode: HashMode::from_i32(row.get(5)?)
                        .ok_or_else(|| rusqlite::Error::InvalidColumnType(5, "hash_mode".to_string(), rusqlite::types::Type::Integer))?,
                    validate_mode: ValidateMode::from_i32(row.get(6)?)
                        .ok_or_else(|| rusqlite::Error::InvalidColumnType(6, "validate_mode".to_string(), rusqlite::types::Type::Integer))?,
                    source: SourceType::from_i32(row.get(7)?)
                        .ok_or_else(|| rusqlite::Error::InvalidColumnType(7, "source".to_string(), rusqlite::types::Type::Integer))?,
                    created_at: row.get(8)?,
                })
            },
        )
        .optional()
        .map_err(FsPulseError::DatabaseError)
    }

    /// Get queue entry for a specific schedule (if it exists)
    pub fn get_by_schedule(db: &Database, schedule_id: i64) -> Result<Option<Self>, FsPulseError> {
        let conn = db.conn();

        conn.query_row(
            "SELECT
                queue_id, root_id, schedule_id, scan_id, next_scan_time,
                hash_mode, validate_mode, source, created_at
            FROM scan_queue
            WHERE schedule_id = ?",
            [schedule_id],
            |row| {
                Ok(QueueEntry {
                    queue_id: row.get(0)?,
                    root_id: row.get(1)?,
                    schedule_id: row.get(2)?,
                    scan_id: row.get(3)?,
                    next_scan_time: row.get(4)?,
                    hash_mode: HashMode::from_i32(row.get(5)?)
                        .ok_or_else(|| rusqlite::Error::InvalidColumnType(5, "hash_mode".to_string(), rusqlite::types::Type::Integer))?,
                    validate_mode: ValidateMode::from_i32(row.get(6)?)
                        .ok_or_else(|| rusqlite::Error::InvalidColumnType(6, "validate_mode".to_string(), rusqlite::types::Type::Integer))?,
                    source: SourceType::from_i32(row.get(7)?)
                        .ok_or_else(|| rusqlite::Error::InvalidColumnType(7, "source".to_string(), rusqlite::types::Type::Integer))?,
                    created_at: row.get(8)?,
                })
            },
        )
        .optional()
        .map_err(FsPulseError::DatabaseError)
    }

    /// Get all queue entries for a root
    pub fn get_by_root(db: &Database, root_id: i64) -> Result<Vec<Self>, FsPulseError> {
        let conn = db.conn();
        let mut stmt = conn.prepare(
            "SELECT
                queue_id, root_id, schedule_id, scan_id, next_scan_time,
                hash_mode, validate_mode, source, created_at
            FROM scan_queue
            WHERE root_id = ?
            ORDER BY next_scan_time ASC",
        )?;

        let rows = stmt.query_map([root_id], |row| {
            Ok(QueueEntry {
                queue_id: row.get(0)?,
                root_id: row.get(1)?,
                schedule_id: row.get(2)?,
                scan_id: row.get(3)?,
                next_scan_time: row.get(4)?,
                hash_mode: HashMode::from_i32(row.get(5)?)
                    .ok_or_else(|| rusqlite::Error::InvalidColumnType(5, "hash_mode".to_string(), rusqlite::types::Type::Integer))?,
                validate_mode: ValidateMode::from_i32(row.get(6)?)
                    .ok_or_else(|| rusqlite::Error::InvalidColumnType(6, "validate_mode".to_string(), rusqlite::types::Type::Integer))?,
                source: SourceType::from_i32(row.get(7)?)
                    .ok_or_else(|| rusqlite::Error::InvalidColumnType(7, "source".to_string(), rusqlite::types::Type::Integer))?,
                created_at: row.get(8)?,
            })
        })?;

        let mut entries = Vec::new();
        for row in rows {
            entries.push(row?);
        }

        Ok(entries)
    }

    /// Get the next scan to run, creating it atomically if needed
    ///
    /// This function handles the complete scan initiation workflow:
    /// 1. Checks for incomplete scans (resume case) - returns existing scan
    /// 2. Finds highest priority work (manual first, then scheduled due)
    /// 3. Creates scan record for the work
    /// 4. Updates queue entry with scan_id
    /// 5. For scheduled scans: calculates next_scan_time (fail-fast before crashes)
    ///
    /// Returns the Scan object ready to be executed, or None if no work available.
    pub fn get_next_work(db: &Database) -> Result<Option<Scan>, FsPulseError> {
        let now = chrono::Utc::now().timestamp();

        db.immediate_transaction(|conn| {
            // Step 1: Check if ANY scan is currently running (resume case)
            let running_scan_id: Option<i64> = conn.query_row(
                "SELECT scan_id FROM scan_queue WHERE scan_id IS NOT NULL LIMIT 1",
                [],
                |row| row.get(0)
            )
            .optional()
            .map_err(FsPulseError::DatabaseError)?;

            if let Some(scan_id) = running_scan_id {
                // Resume case: return existing incomplete scan
                return Scan::get_by_id_or_latest(conn, Some(scan_id), None);
            }

            // Step 2: Find highest priority work (manual first, then scheduled due)
            // Single query with priority: manual scans (source=0) always first,
            // then scheduled scans (source=1) only if due
            let work = conn.query_row(
                "SELECT queue_id, root_id, schedule_id, hash_mode, validate_mode
                 FROM scan_queue
                 WHERE scan_id IS NULL AND (source = ? OR next_scan_time <= ?)
                 ORDER BY source ASC, next_scan_time ASC, queue_id ASC
                 LIMIT 1",
                rusqlite::params![SourceType::Manual.as_i32(), now],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,  // queue_id
                        row.get::<_, i64>(1)?,  // root_id
                        row.get::<_, Option<i64>>(2)?,  // schedule_id
                        HashMode::from_i32(row.get(3)?)
                            .ok_or_else(|| rusqlite::Error::InvalidColumnType(3, "hash_mode".to_string(), rusqlite::types::Type::Integer))?,
                        ValidateMode::from_i32(row.get(4)?)
                            .ok_or_else(|| rusqlite::Error::InvalidColumnType(4, "validate_mode".to_string(), rusqlite::types::Type::Integer))?,
                    ))
                }
            )
            .optional()
            .map_err(FsPulseError::DatabaseError)?;

            let work = match work {
                Some(w) => w,
                None => return Ok(None), // No work available
            };

            let (queue_id, root_id, schedule_id, hash_mode, validate_mode) = work;

            // Step 3: Get root for scan creation
            let root = Root::get_by_id(conn, root_id)?
                .ok_or_else(|| FsPulseError::Error(format!("Root {} not found", root_id)))?;

            // Step 4: Create analysis spec and scan
            let analysis_spec = AnalysisSpec::from_modes(hash_mode, validate_mode);
            let scan = Scan::create(conn, &root, &analysis_spec)?;

            // Step 5: Update queue entry with scan_id
            conn.execute(
                "UPDATE scan_queue SET scan_id = ? WHERE queue_id = ?",
                rusqlite::params![scan.scan_id(), queue_id]
            )
            .map_err(FsPulseError::DatabaseError)?;

            // Step 6: For scheduled scans, calculate and set next_scan_time NOW
            if let Some(schedule_id) = schedule_id {
                let schedule = Schedule::get_by_id(conn, schedule_id)?
                    .ok_or_else(|| FsPulseError::Error(format!("Schedule {} not found", schedule_id)))?;

                let next_time = schedule.calculate_next_scan_time(now)
                    .map_err(|e| FsPulseError::Error(format!("Failed to calculate next scan time: {}", e)))?;

                conn.execute(
                    "UPDATE scan_queue SET next_scan_time = ? WHERE queue_id = ?",
                    rusqlite::params![next_time, queue_id]
                )
                .map_err(FsPulseError::DatabaseError)?;
            }

            Ok(Some(scan))
        })
    }

    /// Set scan_id when ScanManager starts a scan
    pub fn set_scan_id(
        db: &Database,
        queue_id: i64,
        scan_id: i64,
    ) -> Result<(), FsPulseError> {
        let conn = db.conn();

        let rows_affected = conn.execute(
            "UPDATE scan_queue SET scan_id = ? WHERE queue_id = ?",
            rusqlite::params![scan_id, queue_id],
        )
        .map_err(FsPulseError::DatabaseError)?;

        if rows_affected == 0 {
            return Err(FsPulseError::Error(format!(
                "Queue entry with id {} not found",
                queue_id
            )));
        }

        Ok(())
    }

    /// Clear scan_id when scan completes (for scheduled scans that will run again)
    pub fn clear_scan_id(
        db: &Database,
        queue_id: i64,
    ) -> Result<(), FsPulseError> {
        let conn = db.conn();

        let rows_affected = conn.execute(
            "UPDATE scan_queue SET scan_id = NULL WHERE queue_id = ?",
            [queue_id],
        )
        .map_err(FsPulseError::DatabaseError)?;

        if rows_affected == 0 {
            return Err(FsPulseError::Error(format!(
                "Queue entry with id {} not found",
                queue_id
            )));
        }

        Ok(())
    }

    /// Update the next_scan_time for a queue entry (used after scheduled scans complete)
    pub fn update_next_scan_time(
        db: &Database,
        queue_id: i64,
        next_scan_time: i64,
    ) -> Result<(), FsPulseError> {
        let conn = db.conn();

        let rows_affected = conn.execute(
            "UPDATE scan_queue SET next_scan_time = ? WHERE queue_id = ?",
            rusqlite::params![next_scan_time, queue_id],
        )
        .map_err(FsPulseError::DatabaseError)?;

        if rows_affected == 0 {
            return Err(FsPulseError::Error(format!(
                "Queue entry with id {} not found",
                queue_id
            )));
        }

        Ok(())
    }

    /// Delete a queue entry (used after manual scans complete, or when schedule is deleted)
    pub fn delete(db: &Database, queue_id: i64) -> Result<(), FsPulseError> {
        let conn = db.conn();

        let rows_affected = conn.execute(
            "DELETE FROM scan_queue WHERE queue_id = ?",
            [queue_id]
        )
        .map_err(FsPulseError::DatabaseError)?;

        if rows_affected == 0 {
            return Err(FsPulseError::Error(format!(
                "Queue entry with id {} not found",
                queue_id
            )));
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Local};

    // Base time for all tests: Wednesday, January 15, 2025, 10:30:00 local time
    fn base_time() -> i64 {
        Local.with_ymd_and_hms(2025, 1, 15, 10, 30, 0)
            .single()
            .unwrap()
            .timestamp()
    }

    #[test]
    fn test_schedule_type_conversion() {
        assert_eq!(ScheduleType::from_i32(0), Some(ScheduleType::Daily));
        assert_eq!(ScheduleType::from_i32(3), Some(ScheduleType::Monthly));
        assert_eq!(ScheduleType::from_i32(99), None);
        assert_eq!(ScheduleType::Daily.as_i32(), 0);
    }

    #[test]
    fn test_interval_unit_to_seconds() {
        assert_eq!(IntervalUnit::Minutes.to_seconds(5), 300);
        assert_eq!(IntervalUnit::Hours.to_seconds(2), 7200);
        assert_eq!(IntervalUnit::Days.to_seconds(1), 86400);
        assert_eq!(IntervalUnit::Weeks.to_seconds(1), 604800);
    }

    #[test]
    fn test_validate_daily_schedule() {
        let schedule = Schedule {
            schedule_id: 1,
            root_id: 1,
            enabled: true,
            schedule_name: "Daily scan".to_string(),
            schedule_type: ScheduleType::Daily,
            time_of_day: Some("02:00".to_string()),
            days_of_week: None,
            day_of_month: None,
            interval_value: None,
            interval_unit: None,
            hash_mode: HashMode::New,
            validate_mode: ValidateMode::None,
            created_at: 0,
            updated_at: 0,
        };

        assert!(schedule.validate().is_ok());
    }

    #[test]
    fn test_validate_daily_schedule_missing_time() {
        let schedule = Schedule {
            schedule_id: 1,
            root_id: 1,
            enabled: true,
            schedule_name: "Daily scan".to_string(),
            schedule_type: ScheduleType::Daily,
            time_of_day: None,  // Missing!
            days_of_week: None,
            day_of_month: None,
            interval_value: None,
            interval_unit: None,
            hash_mode: HashMode::New,
            validate_mode: ValidateMode::None,
            created_at: 0,
            updated_at: 0,
        };

        assert!(schedule.validate().is_err());
    }

    #[test]
    fn test_validate_interval_schedule() {
        let schedule = Schedule {
            schedule_id: 1,
            root_id: 1,
            enabled: true,
            schedule_name: "Every 5 minutes".to_string(),
            schedule_type: ScheduleType::Interval,
            time_of_day: None,
            days_of_week: None,
            day_of_month: None,
            interval_value: Some(5),
            interval_unit: Some(IntervalUnit::Minutes),
            hash_mode: HashMode::None,
            validate_mode: ValidateMode::None,
            created_at: 0,
            updated_at: 0,
        };

        assert!(schedule.validate().is_ok());
    }

    #[test]
    fn test_validate_time_of_day_format() {
        let schedule = Schedule {
            schedule_id: 1,
            root_id: 1,
            enabled: true,
            schedule_name: "Daily scan".to_string(),
            schedule_type: ScheduleType::Daily,
            time_of_day: Some("25:00".to_string()),  // Invalid hour
            days_of_week: None,
            day_of_month: None,
            interval_value: None,
            interval_unit: None,
            hash_mode: HashMode::New,
            validate_mode: ValidateMode::None,
            created_at: 0,
            updated_at: 0,
        };

        assert!(schedule.validate().is_err());
    }

    #[test]
    fn test_validate_days_of_week() {
        let schedule = Schedule {
            schedule_id: 1,
            root_id: 1,
            enabled: true,
            schedule_name: "Weekly scan".to_string(),
            schedule_type: ScheduleType::Weekly,
            time_of_day: Some("14:00".to_string()),
            days_of_week: Some(r#"["Mon","Wed","Fri"]"#.to_string()),
            day_of_month: None,
            interval_value: None,
            interval_unit: None,
            hash_mode: HashMode::All,
            validate_mode: ValidateMode::All,
            created_at: 0,
            updated_at: 0,
        };

        assert!(schedule.validate().is_ok());
    }

    #[test]
    fn test_validate_monthly_schedule() {
        let schedule = Schedule {
            schedule_id: 1,
            root_id: 1,
            enabled: true,
            schedule_name: "Monthly scan".to_string(),
            schedule_type: ScheduleType::Monthly,
            time_of_day: Some("02:00".to_string()),
            days_of_week: None,
            day_of_month: Some(15),
            interval_value: None,
            interval_unit: None,
            hash_mode: HashMode::New,
            validate_mode: ValidateMode::New,
            created_at: 0,
            updated_at: 0,
        };

        assert!(schedule.validate().is_ok());
    }

    #[test]
    fn test_validate_monthly_schedule_invalid_day() {
        let schedule = Schedule {
            schedule_id: 1,
            root_id: 1,
            enabled: true,
            schedule_name: "Monthly scan".to_string(),
            schedule_type: ScheduleType::Monthly,
            time_of_day: Some("02:00".to_string()),
            days_of_week: None,
            day_of_month: Some(32),  // Invalid day
            interval_value: None,
            interval_unit: None,
            hash_mode: HashMode::New,
            validate_mode: ValidateMode::New,
            created_at: 0,
            updated_at: 0,
        };

        assert!(schedule.validate().is_err());
    }

    // ========================================
    // Tests for calculate_next_scan_time
    // ========================================

    // Base time: Wednesday, January 15, 2025, 10:30:00 local time

    #[test]
    fn test_calculate_interval_minutes() {
        let base = base_time();
        let schedule = Schedule {
            schedule_id: 1,
            root_id: 1,
            enabled: true,
            schedule_name: "Every 5 minutes".to_string(),
            schedule_type: ScheduleType::Interval,
            time_of_day: None,
            days_of_week: None,
            day_of_month: None,
            interval_value: Some(5),
            interval_unit: Some(IntervalUnit::Minutes),
            hash_mode: HashMode::None,
            validate_mode: ValidateMode::None,
            created_at: 0,
            updated_at: 0,
        };

        let next = schedule.calculate_next_scan_time(base).unwrap();
        assert_eq!(next, base + 300); // 5 minutes = 300 seconds
    }

    #[test]
    fn test_calculate_interval_hours() {
        let base = base_time();
        let schedule = Schedule {
            schedule_id: 1,
            root_id: 1,
            enabled: true,
            schedule_name: "Every 2 hours".to_string(),
            schedule_type: ScheduleType::Interval,
            time_of_day: None,
            days_of_week: None,
            day_of_month: None,
            interval_value: Some(2),
            interval_unit: Some(IntervalUnit::Hours),
            hash_mode: HashMode::None,
            validate_mode: ValidateMode::None,
            created_at: 0,
            updated_at: 0,
        };

        let next = schedule.calculate_next_scan_time(base).unwrap();
        assert_eq!(next, base + 7200); // 2 hours = 7200 seconds
    }

    #[test]
    fn test_calculate_interval_days() {
        let base = base_time();
        let schedule = Schedule {
            schedule_id: 1,
            root_id: 1,
            enabled: true,
            schedule_name: "Every day".to_string(),
            schedule_type: ScheduleType::Interval,
            time_of_day: None,
            days_of_week: None,
            day_of_month: None,
            interval_value: Some(1),
            interval_unit: Some(IntervalUnit::Days),
            hash_mode: HashMode::None,
            validate_mode: ValidateMode::None,
            created_at: 0,
            updated_at: 0,
        };

        let next = schedule.calculate_next_scan_time(base).unwrap();
        assert_eq!(next, base + 86400); // 1 day = 86400 seconds
    }

    #[test]
    fn test_calculate_daily_time_not_passed() {
        // Base: Wed Jan 15, 2025 10:30
        // Schedule: Daily at 14:00 (time hasn't passed today)
        let base = base_time();
        let schedule = Schedule {
            schedule_id: 1,
            root_id: 1,
            enabled: true,
            schedule_name: "Daily at 2pm".to_string(),
            schedule_type: ScheduleType::Daily,
            time_of_day: Some("14:00".to_string()),
            days_of_week: None,
            day_of_month: None,
            interval_value: None,
            interval_unit: None,
            hash_mode: HashMode::None,
            validate_mode: ValidateMode::None,
            created_at: 0,
            updated_at: 0,
        };

        let next = schedule.calculate_next_scan_time(base).unwrap();

        // Should be today at 14:00
        let expected = Local.with_ymd_and_hms(2025, 1, 15, 14, 0, 0)
            .single()
            .unwrap()
            .timestamp();
        assert_eq!(next, expected);
    }

    #[test]
    fn test_calculate_daily_time_passed() {
        // Base: Wed Jan 15, 2025 10:30
        // Schedule: Daily at 09:00 (time has passed today)
        let base = base_time();
        let schedule = Schedule {
            schedule_id: 1,
            root_id: 1,
            enabled: true,
            schedule_name: "Daily at 9am".to_string(),
            schedule_type: ScheduleType::Daily,
            time_of_day: Some("09:00".to_string()),
            days_of_week: None,
            day_of_month: None,
            interval_value: None,
            interval_unit: None,
            hash_mode: HashMode::None,
            validate_mode: ValidateMode::None,
            created_at: 0,
            updated_at: 0,
        };

        let next = schedule.calculate_next_scan_time(base).unwrap();

        // Should be tomorrow at 09:00
        let expected = Local.with_ymd_and_hms(2025, 1, 16, 9, 0, 0)
            .single()
            .unwrap()
            .timestamp();
        assert_eq!(next, expected);
    }

    #[test]
    fn test_calculate_weekly_today_time_not_passed() {
        // Base: Wed Jan 15, 2025 10:30
        // Schedule: Weekly on Wednesday at 14:00
        let base = base_time();
        let schedule = Schedule {
            schedule_id: 1,
            root_id: 1,
            enabled: true,
            schedule_name: "Weekly Wed".to_string(),
            schedule_type: ScheduleType::Weekly,
            time_of_day: Some("14:00".to_string()),
            days_of_week: Some(r#"["Wed"]"#.to_string()),
            day_of_month: None,
            interval_value: None,
            interval_unit: None,
            hash_mode: HashMode::None,
            validate_mode: ValidateMode::None,
            created_at: 0,
            updated_at: 0,
        };

        let next = schedule.calculate_next_scan_time(base).unwrap();

        // Should be today (Wednesday) at 14:00
        let expected = Local.with_ymd_and_hms(2025, 1, 15, 14, 0, 0)
            .single()
            .unwrap()
            .timestamp();
        assert_eq!(next, expected);
    }

    #[test]
    fn test_calculate_weekly_today_time_passed() {
        // Base: Wed Jan 15, 2025 10:30
        // Schedule: Weekly on Wednesday at 09:00 (time passed)
        let base = base_time();
        let schedule = Schedule {
            schedule_id: 1,
            root_id: 1,
            enabled: true,
            schedule_name: "Weekly Wed".to_string(),
            schedule_type: ScheduleType::Weekly,
            time_of_day: Some("09:00".to_string()),
            days_of_week: Some(r#"["Wed"]"#.to_string()),
            day_of_month: None,
            interval_value: None,
            interval_unit: None,
            hash_mode: HashMode::None,
            validate_mode: ValidateMode::None,
            created_at: 0,
            updated_at: 0,
        };

        let next = schedule.calculate_next_scan_time(base).unwrap();

        // Should be next Wednesday (Jan 22) at 09:00
        let expected = Local.with_ymd_and_hms(2025, 1, 22, 9, 0, 0)
            .single()
            .unwrap()
            .timestamp();
        assert_eq!(next, expected);
    }

    #[test]
    fn test_calculate_weekly_next_day() {
        // Base: Wed Jan 15, 2025 10:30
        // Schedule: Weekly on Monday at 14:00
        let base = base_time();
        let schedule = Schedule {
            schedule_id: 1,
            root_id: 1,
            enabled: true,
            schedule_name: "Weekly Mon".to_string(),
            schedule_type: ScheduleType::Weekly,
            time_of_day: Some("14:00".to_string()),
            days_of_week: Some(r#"["Mon"]"#.to_string()),
            day_of_month: None,
            interval_value: None,
            interval_unit: None,
            hash_mode: HashMode::None,
            validate_mode: ValidateMode::None,
            created_at: 0,
            updated_at: 0,
        };

        let next = schedule.calculate_next_scan_time(base).unwrap();

        // Should be next Monday (Jan 20) at 14:00
        let expected = Local.with_ymd_and_hms(2025, 1, 20, 14, 0, 0)
            .single()
            .unwrap()
            .timestamp();
        assert_eq!(next, expected);
    }

    #[test]
    fn test_calculate_weekly_multiple_days() {
        // Base: Wed Jan 15, 2025 10:30
        // Schedule: Weekly on Mon,Wed,Fri at 14:00
        let base = base_time();
        let schedule = Schedule {
            schedule_id: 1,
            root_id: 1,
            enabled: true,
            schedule_name: "Weekly MWF".to_string(),
            schedule_type: ScheduleType::Weekly,
            time_of_day: Some("14:00".to_string()),
            days_of_week: Some(r#"["Mon","Wed","Fri"]"#.to_string()),
            day_of_month: None,
            interval_value: None,
            interval_unit: None,
            hash_mode: HashMode::None,
            validate_mode: ValidateMode::None,
            created_at: 0,
            updated_at: 0,
        };

        let next = schedule.calculate_next_scan_time(base).unwrap();

        // Should be today (Wednesday) at 14:00 since time hasn't passed
        let expected = Local.with_ymd_and_hms(2025, 1, 15, 14, 0, 0)
            .single()
            .unwrap()
            .timestamp();
        assert_eq!(next, expected);
    }

    #[test]
    fn test_calculate_weekly_multiple_days_all_passed() {
        // Base: Wed Jan 15, 2025 10:30
        // Schedule: Weekly on Mon,Tue at 14:00 (both in the past)
        let base = base_time();
        let schedule = Schedule {
            schedule_id: 1,
            root_id: 1,
            enabled: true,
            schedule_name: "Weekly Mon,Tue".to_string(),
            schedule_type: ScheduleType::Weekly,
            time_of_day: Some("14:00".to_string()),
            days_of_week: Some(r#"["Mon","Tue"]"#.to_string()),
            day_of_month: None,
            interval_value: None,
            interval_unit: None,
            hash_mode: HashMode::None,
            validate_mode: ValidateMode::None,
            created_at: 0,
            updated_at: 0,
        };

        let next = schedule.calculate_next_scan_time(base).unwrap();

        // Should be next Monday (Jan 20) at 14:00
        let expected = Local.with_ymd_and_hms(2025, 1, 20, 14, 0, 0)
            .single()
            .unwrap()
            .timestamp();
        assert_eq!(next, expected);
    }

    #[test]
    fn test_calculate_monthly_current_month_not_passed() {
        // Base: Wed Jan 15, 2025 10:30
        // Schedule: Monthly on day 20 at 14:00
        let base = base_time();
        let schedule = Schedule {
            schedule_id: 1,
            root_id: 1,
            enabled: true,
            schedule_name: "Monthly 20th".to_string(),
            schedule_type: ScheduleType::Monthly,
            time_of_day: Some("14:00".to_string()),
            days_of_week: None,
            day_of_month: Some(20),
            interval_value: None,
            interval_unit: None,
            hash_mode: HashMode::None,
            validate_mode: ValidateMode::None,
            created_at: 0,
            updated_at: 0,
        };

        let next = schedule.calculate_next_scan_time(base).unwrap();

        // Should be January 20, 2025 at 14:00
        let expected = Local.with_ymd_and_hms(2025, 1, 20, 14, 0, 0)
            .single()
            .unwrap()
            .timestamp();
        assert_eq!(next, expected);
    }

    #[test]
    fn test_calculate_monthly_current_month_passed() {
        // Base: Wed Jan 15, 2025 10:30
        // Schedule: Monthly on day 10 at 14:00 (already passed in Jan)
        let base = base_time();
        let schedule = Schedule {
            schedule_id: 1,
            root_id: 1,
            enabled: true,
            schedule_name: "Monthly 10th".to_string(),
            schedule_type: ScheduleType::Monthly,
            time_of_day: Some("14:00".to_string()),
            days_of_week: None,
            day_of_month: Some(10),
            interval_value: None,
            interval_unit: None,
            hash_mode: HashMode::None,
            validate_mode: ValidateMode::None,
            created_at: 0,
            updated_at: 0,
        };

        let next = schedule.calculate_next_scan_time(base).unwrap();

        // Should be February 10, 2025 at 14:00
        let expected = Local.with_ymd_and_hms(2025, 2, 10, 14, 0, 0)
            .single()
            .unwrap()
            .timestamp();
        assert_eq!(next, expected);
    }

    #[test]
    fn test_calculate_monthly_day_31_current_month() {
        // Base: Wed Jan 15, 2025 10:30
        // Schedule: Monthly on day 31 at 14:00
        let base = base_time();
        let schedule = Schedule {
            schedule_id: 1,
            root_id: 1,
            enabled: true,
            schedule_name: "Monthly 31st".to_string(),
            schedule_type: ScheduleType::Monthly,
            time_of_day: Some("14:00".to_string()),
            days_of_week: None,
            day_of_month: Some(31),
            interval_value: None,
            interval_unit: None,
            hash_mode: HashMode::None,
            validate_mode: ValidateMode::None,
            created_at: 0,
            updated_at: 0,
        };

        let next = schedule.calculate_next_scan_time(base).unwrap();

        // Should be January 31, 2025 at 14:00 (Jan has 31 days)
        let expected = Local.with_ymd_and_hms(2025, 1, 31, 14, 0, 0)
            .single()
            .unwrap()
            .timestamp();
        assert_eq!(next, expected);
    }

    #[test]
    fn test_calculate_monthly_day_31_skip_february() {
        // Base: Wed Feb 12, 2025 10:30 (mid-February)
        // Schedule: Monthly on day 31 at 14:00
        // Should skip February (only 28 days) and return March 31
        let base = Local.with_ymd_and_hms(2025, 2, 12, 10, 30, 0)
            .single()
            .unwrap()
            .timestamp();

        let schedule = Schedule {
            schedule_id: 1,
            root_id: 1,
            enabled: true,
            schedule_name: "Monthly 31st".to_string(),
            schedule_type: ScheduleType::Monthly,
            time_of_day: Some("14:00".to_string()),
            days_of_week: None,
            day_of_month: Some(31),
            interval_value: None,
            interval_unit: None,
            hash_mode: HashMode::None,
            validate_mode: ValidateMode::None,
            created_at: 0,
            updated_at: 0,
        };

        let next = schedule.calculate_next_scan_time(base).unwrap();

        // Should skip Feb and be March 31, 2025 at 14:00
        let expected = Local.with_ymd_and_hms(2025, 3, 31, 14, 0, 0)
            .single()
            .unwrap()
            .timestamp();
        assert_eq!(next, expected);
    }

    #[test]
    fn test_calculate_monthly_day_30_skip_february() {
        // Base: Wed Feb 12, 2025 10:30
        // Schedule: Monthly on day 30 at 14:00
        // Should skip February and return March 30
        let base = Local.with_ymd_and_hms(2025, 2, 12, 10, 30, 0)
            .single()
            .unwrap()
            .timestamp();

        let schedule = Schedule {
            schedule_id: 1,
            root_id: 1,
            enabled: true,
            schedule_name: "Monthly 30th".to_string(),
            schedule_type: ScheduleType::Monthly,
            time_of_day: Some("14:00".to_string()),
            days_of_week: None,
            day_of_month: Some(30),
            interval_value: None,
            interval_unit: None,
            hash_mode: HashMode::None,
            validate_mode: ValidateMode::None,
            created_at: 0,
            updated_at: 0,
        };

        let next = schedule.calculate_next_scan_time(base).unwrap();

        // Should skip Feb and be March 30, 2025 at 14:00
        let expected = Local.with_ymd_and_hms(2025, 3, 30, 14, 0, 0)
            .single()
            .unwrap()
            .timestamp();
        assert_eq!(next, expected);
    }
}
