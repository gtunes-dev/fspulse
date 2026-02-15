use crate::database::Database;
use crate::error::FsPulseError;
use crate::scans::{HashMode, ValidateMode};
use crate::task::{CompactDatabaseSettings, CompactDatabaseTask, ScanSettings, ScanTask, Task, TaskStatus, TaskType};
use rusqlite::{Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

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

/// Parameters for creating a new schedule
pub struct CreateScheduleParams {
    pub root_id: i64,
    pub schedule_name: String,
    pub schedule_type: ScheduleType,
    pub time_of_day: Option<String>,
    pub days_of_week: Option<String>,
    pub day_of_month: Option<i64>,
    pub interval_value: Option<i64>,
    pub interval_unit: Option<IntervalUnit>,
    pub hash_mode: HashMode,
    pub validate_mode: ValidateMode,
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
    pub created_at: i64, // Unix timestamp (UTC)
    pub updated_at: i64, // Unix timestamp (UTC)
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
                    if !(1..=31).contains(&day) {
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
            return Err(format!(
                "time_of_day must be in HH:MM format, got: {}",
                time
            ));
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
                let unit = self
                    .interval_unit
                    .ok_or("Interval schedule missing interval_unit")?;
                let value = self
                    .interval_value
                    .ok_or("Interval schedule missing interval_value")?;
                let seconds = unit.to_seconds(value);
                Ok(from_time + seconds)
            }

            ScheduleType::Daily => {
                let time_str = self
                    .time_of_day
                    .as_ref()
                    .ok_or("Daily schedule missing time_of_day")?;
                self.calculate_next_daily(from_time, time_str)
            }

            ScheduleType::Weekly => {
                let time_str = self
                    .time_of_day
                    .as_ref()
                    .ok_or("Weekly schedule missing time_of_day")?;
                let days_str = self
                    .days_of_week
                    .as_ref()
                    .ok_or("Weekly schedule missing days_of_week")?;
                self.calculate_next_weekly(from_time, time_str, days_str)
            }

            ScheduleType::Monthly => {
                let time_str = self
                    .time_of_day
                    .as_ref()
                    .ok_or("Monthly schedule missing time_of_day")?;
                let day = self
                    .day_of_month
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
        let hours: u32 = parts[0]
            .parse()
            .map_err(|_| format!("Invalid hours: {}", parts[0]))?;
        let minutes: u32 = parts[1]
            .parse()
            .map_err(|_| format!("Invalid minutes: {}", parts[1]))?;

        // Get current time in local timezone
        let from_local = Local
            .timestamp_opt(from_time, 0)
            .single()
            .ok_or("Invalid timestamp")?;

        // Try today at the scheduled time
        let today_at_time = from_local
            .with_hour(hours)
            .ok_or("Invalid hour")?
            .with_minute(minutes)
            .ok_or("Invalid minute")?
            .with_second(0)
            .ok_or("Failed to set seconds")?
            .with_nanosecond(0)
            .ok_or("Failed to set nanoseconds")?;

        // If that time has already passed today, use tomorrow
        let next_occurrence = if today_at_time.timestamp() > from_time {
            today_at_time
        } else {
            today_at_time + chrono::Duration::days(1)
        };

        Ok(next_occurrence.timestamp())
    }

    /// Calculate next weekly occurrence
    fn calculate_next_weekly(
        &self,
        from_time: i64,
        time_of_day: &str,
        days_of_week: &str,
    ) -> Result<i64, String> {
        use chrono::{Datelike, Local, TimeZone, Timelike, Weekday};

        // Parse HH:MM
        let parts: Vec<&str> = time_of_day.split(':').collect();
        let hours: u32 = parts[0]
            .parse()
            .map_err(|_| format!("Invalid hours: {}", parts[0]))?;
        let minutes: u32 = parts[1]
            .parse()
            .map_err(|_| format!("Invalid minutes: {}", parts[1]))?;

        // Parse days_of_week JSON
        let day_names: Vec<String> = serde_json::from_str(days_of_week)
            .map_err(|e| format!("Invalid JSON in days_of_week: {}", e))?;

        // Map day names to Weekday enum
        let target_weekdays: Vec<Weekday> = day_names
            .iter()
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
        let from_local = Local
            .timestamp_opt(from_time, 0)
            .single()
            .ok_or("Invalid timestamp")?;

        // Check today first
        let today_at_time = from_local
            .with_hour(hours)
            .ok_or("Invalid hour")?
            .with_minute(minutes)
            .ok_or("Invalid minute")?
            .with_second(0)
            .ok_or("Failed to set seconds")?
            .with_nanosecond(0)
            .ok_or("Failed to set nanoseconds")?;

        if target_weekdays.contains(&from_local.weekday()) && today_at_time.timestamp() > from_time
        {
            return Ok(today_at_time.timestamp());
        }

        // Search next 7 days for matching weekday
        for days_ahead in 1..=7 {
            let candidate = from_local + chrono::Duration::days(days_ahead);
            if target_weekdays.contains(&candidate.weekday()) {
                let next_occurrence = candidate
                    .with_hour(hours)
                    .ok_or("Invalid hour")?
                    .with_minute(minutes)
                    .ok_or("Invalid minute")?
                    .with_second(0)
                    .ok_or("Failed to set seconds")?
                    .with_nanosecond(0)
                    .ok_or("Failed to set nanoseconds")?;
                return Ok(next_occurrence.timestamp());
            }
        }

        Err("Failed to find next weekly occurrence".to_string())
    }

    /// Calculate next monthly occurrence
    fn calculate_next_monthly(
        &self,
        from_time: i64,
        time_of_day: &str,
        day_of_month: i64,
    ) -> Result<i64, String> {
        use chrono::{Datelike, Local, TimeZone};

        if !(1..=31).contains(&day_of_month) {
            return Err(format!("Invalid day_of_month: {}", day_of_month));
        }

        // Parse HH:MM
        let parts: Vec<&str> = time_of_day.split(':').collect();
        let hours: u32 = parts[0]
            .parse()
            .map_err(|_| format!("Invalid hours: {}", parts[0]))?;
        let minutes: u32 = parts[1]
            .parse()
            .map_err(|_| format!("Invalid minutes: {}", parts[1]))?;

        // Get current time in local timezone
        let from_local = Local
            .timestamp_opt(from_time, 0)
            .single()
            .ok_or("Invalid timestamp")?;

        // Try to find next occurrence within the next 12 months
        for month_offset in 0..12 {
            // Calculate target year and month using proper month arithmetic
            let current_year = from_local.year();
            let current_month = from_local.month();

            let total_months = current_month as i32 + month_offset;
            let target_year = current_year + (total_months - 1) / 12;
            let target_month = ((total_months - 1) % 12) + 1;

            // Try to create a date with the target day
            let candidate_date = Local
                .with_ymd_and_hms(
                    target_year,
                    target_month as u32,
                    day_of_month as u32,
                    hours,
                    minutes,
                    0,
                )
                .single();

            if let Some(candidate) = candidate_date {
                // Check if this is in the future
                if candidate.timestamp() > from_time {
                    return Ok(candidate.timestamp());
                }
            }
            // If day doesn't exist in this month (e.g., Feb 31), skip to next month
        }

        Err(format!(
            "Failed to find next monthly occurrence for day {}",
            day_of_month
        ))
    }

    // ========================================
    // Database operations
    // ========================================

    /// Create a new schedule and queue entry atomically
    /// This is the primary way to create schedules from API/UI
    /// Creates schedule as enabled by default
    ///
    /// IMPORTANT: Caller must hold an immediate transaction
    pub fn create_and_queue(
        conn: &rusqlite::Connection,
        params: CreateScheduleParams,
    ) -> Result<Self, FsPulseError> {
        let now = chrono::Utc::now().timestamp();

        // Build schedule
        let schedule = Schedule {
            schedule_id: 0, // Will be set by database
            root_id: params.root_id,
            enabled: true, // Always create as enabled
            schedule_name: params.schedule_name,
            schedule_type: params.schedule_type,
            time_of_day: params.time_of_day,
            days_of_week: params.days_of_week,
            day_of_month: params.day_of_month,
            interval_value: params.interval_value,
            interval_unit: params.interval_unit,
            hash_mode: params.hash_mode,
            validate_mode: params.validate_mode,
            created_at: now,
            updated_at: now,
        };

        // Validate schedule fields
        schedule
            .validate()
            .map_err(|e| FsPulseError::Error(format!("Invalid schedule: {}", e)))?;

        // Calculate next_scan_time BEFORE database operations
        // This ensures we fail fast if calculation fails
        let next_scan_time = schedule.calculate_next_scan_time(now).map_err(|e| {
            FsPulseError::Error(format!("Failed to calculate next scan time: {}", e))
        })?;

        // Perform database operations (caller holds transaction)
        // Insert schedule
        let schedule_id: i64 = conn
            .query_row(
                "INSERT INTO scan_schedules (
                root_id, enabled, schedule_name, schedule_type,
                time_of_day, days_of_week, day_of_month,
                interval_value, interval_unit,
                hash_mode, validate_mode,
                created_at, updated_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            RETURNING schedule_id",
                rusqlite::params![
                    schedule.root_id,
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

        // Build task_settings using typed struct
        let task_settings = ScanSettings::new(schedule.hash_mode, schedule.validate_mode).to_json()?;

        // Insert task entry (schedule is enabled by default)
        conn.execute(
            "INSERT INTO tasks (
                task_type, status, root_id, schedule_id, run_at,
                source, task_settings, created_at
            ) VALUES (?, 0, ?, ?, ?, ?, ?, ?)",
            rusqlite::params![
                TaskType::Scan.as_i64(),
                schedule.root_id,
                schedule_id,
                next_scan_time,
                SourceType::Scheduled.as_i32(),
                task_settings,
                now,
            ],
        )
        .map_err(FsPulseError::DatabaseError)?;

        Ok(Schedule {
            schedule_id,
            ..schedule
        })
    }

    /// Get a schedule by ID
    pub fn get_by_id(
        conn: &rusqlite::Connection,
        schedule_id: i64,
    ) -> Result<Option<Self>, FsPulseError> {
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
                    schedule_type: ScheduleType::from_i32(row.get(4)?).ok_or_else(|| {
                        rusqlite::Error::InvalidColumnType(
                            4,
                            "schedule_type".to_string(),
                            rusqlite::types::Type::Integer,
                        )
                    })?,
                    time_of_day: row.get(5)?,
                    days_of_week: row.get(6)?,
                    day_of_month: row.get(7)?,
                    interval_value: row.get(8)?,
                    interval_unit: row
                        .get::<_, Option<i32>>(9)?
                        .map(|v| {
                            IntervalUnit::from_i32(v).ok_or_else(|| {
                                rusqlite::Error::InvalidColumnType(
                                    9,
                                    "interval_unit".to_string(),
                                    rusqlite::types::Type::Integer,
                                )
                            })
                        })
                        .transpose()?,
                    hash_mode: HashMode::from_i32(row.get(10)?).ok_or_else(|| {
                        rusqlite::Error::InvalidColumnType(
                            10,
                            "hash_mode".to_string(),
                            rusqlite::types::Type::Integer,
                        )
                    })?,
                    validate_mode: ValidateMode::from_i32(row.get(11)?).ok_or_else(|| {
                        rusqlite::Error::InvalidColumnType(
                            11,
                            "validate_mode".to_string(),
                            rusqlite::types::Type::Integer,
                        )
                    })?,
                    created_at: row.get(12)?,
                    updated_at: row.get(13)?,
                })
            },
        )
        .optional()
        .map_err(FsPulseError::DatabaseError)
    }

    /// Update an existing schedule and recalculate run_at for its Pending task
    /// This atomically updates both the schedule and its Pending task entry
    ///
    /// IMPORTANT: Caller must hold an immediate transaction
    pub fn update(&self, conn: &rusqlite::Connection) -> Result<(), FsPulseError> {
        // Validate before updating
        self.validate()
            .map_err(|e| FsPulseError::Error(format!("Invalid schedule: {}", e)))?;

        let now = chrono::Utc::now().timestamp();

        // Calculate next_scan_time BEFORE database operations
        let next_scan_time = self.calculate_next_scan_time(now).map_err(|e| {
            FsPulseError::Error(format!("Failed to calculate next scan time: {}", e))
        })?;
        // Update schedule
        let rows_affected = conn
            .execute(
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
            WHERE schedule_id = ? AND deleted_at IS NULL",
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

        // Update run_at on the Pending task (if one exists)
        conn.execute(
            "UPDATE tasks SET run_at = ? WHERE schedule_id = ? AND status = 0",
            rusqlite::params![next_scan_time, self.schedule_id],
        )
        .map_err(FsPulseError::DatabaseError)?;

        Ok(())
    }

    /// Enable or disable a schedule
    /// When disabling: deletes the Pending task (running scan completes normally)
    /// When re-enabling: creates a new Pending task with recalculated run_at
    pub fn set_enabled(schedule_id: i64, enabled: bool) -> Result<(), FsPulseError> {
        let conn = Database::get_connection()?;
        let now = chrono::Utc::now().timestamp();

        // If enabling, we need to recalculate next_scan_time
        // Get the schedule to calculate next_scan_time BEFORE transaction
        let next_scan_time = if enabled {
            let schedule = Self::get_by_id(&conn, schedule_id)?.ok_or_else(|| {
                FsPulseError::Error(format!("Schedule {} not found", schedule_id))
            })?;

            Some(schedule.calculate_next_scan_time(now).map_err(|e| {
                FsPulseError::Error(format!("Failed to calculate next scan time: {}", e))
            })?)
        } else {
            None
        };

        Database::immediate_transaction(&conn, |c| {
            // Update the schedule
            let rows_affected = c.execute(
                "UPDATE scan_schedules SET enabled = ?, updated_at = ? WHERE schedule_id = ? AND deleted_at IS NULL",
                rusqlite::params![enabled, now, schedule_id],
            )
            .map_err(FsPulseError::DatabaseError)?;

            if rows_affected == 0 {
                return Err(FsPulseError::Error(format!(
                    "Schedule with id {} not found",
                    schedule_id
                )));
            }

            // Update task entries based on enabled state
            if enabled {
                // Re-enabling: check if a Pending task already exists
                let pending_exists: bool = c
                    .query_row(
                        "SELECT COUNT(*) FROM tasks WHERE schedule_id = ? AND status = 0",
                        [schedule_id],
                        |row| row.get::<_, i64>(0),
                    )
                    .map(|count| count > 0)
                    .map_err(FsPulseError::DatabaseError)?;

                if pending_exists {
                    // Update existing Pending task's run_at
                    c.execute(
                        "UPDATE tasks SET run_at = ? WHERE schedule_id = ? AND status = 0",
                        rusqlite::params![next_scan_time.unwrap(), schedule_id],
                    )
                    .map_err(FsPulseError::DatabaseError)?;
                } else {
                    // Create new Pending task
                    let schedule = Self::get_by_id(c, schedule_id)?.ok_or_else(|| {
                        FsPulseError::Error(format!("Schedule {} not found", schedule_id))
                    })?;

                    let task_settings =
                        ScanSettings::new(schedule.hash_mode, schedule.validate_mode).to_json()?;

                    c.execute(
                        "INSERT INTO tasks (
                            task_type, status, root_id, schedule_id, run_at,
                            source, task_settings, created_at
                        ) VALUES (?, 0, ?, ?, ?, ?, ?, ?)",
                        rusqlite::params![
                            TaskType::Scan.as_i64(),
                            schedule.root_id,
                            schedule_id,
                            next_scan_time.unwrap(),
                            SourceType::Scheduled.as_i32(),
                            task_settings,
                            now,
                        ],
                    )
                    .map_err(FsPulseError::DatabaseError)?;
                }
            } else {
                // Disabling: delete the Pending task row
                c.execute(
                    "DELETE FROM tasks WHERE schedule_id = ? AND status = 0",
                    [schedule_id],
                )
                .map_err(FsPulseError::DatabaseError)?;
            }

            Ok(())
        })
    }

    /// Delete a schedule.
    /// Soft deletes the schedule by setting deleted_at timestamp.
    /// Deletes any Pending task row. Running tasks are left alone (they'll finish
    /// independently, and the ON DELETE SET NULL FK handles the schedule_id).
    ///
    /// IMPORTANT: Caller must hold an immediate transaction
    pub fn delete_immediate(
        conn: &rusqlite::Connection,
        schedule_id: i64,
    ) -> Result<(), FsPulseError> {
        // Delete Pending task row for this schedule (if any)
        conn.execute(
            "DELETE FROM tasks WHERE schedule_id = ? AND status = 0",
            [schedule_id],
        )
        .map_err(FsPulseError::DatabaseError)?;

        // Soft delete the schedule by setting deleted_at timestamp
        let rows_affected = conn
            .execute(
                "UPDATE scan_schedules SET deleted_at = strftime('%s', 'now', 'utc')
             WHERE schedule_id = ? AND deleted_at IS NULL",
                [schedule_id],
            )
            .map_err(FsPulseError::DatabaseError)?;

        if rows_affected == 0 {
            return Err(FsPulseError::Error(format!(
                "Schedule with id {} not found or already deleted",
                schedule_id
            )));
        }

        Ok(())
    }
}

/// A row from the tasks table, used internally to dispatch to task-type-specific factories
struct TaskRow {
    task_id: i64,
    task_type: TaskType,
    status: TaskStatus,
    root_id: Option<i64>,
    root_path: Option<String>,
    schedule_id: Option<i64>,
    task_settings: String,
    task_state: Option<String>,
}

/// Task operations
///
/// This struct provides associated functions for working with the tasks table.
/// It is not instantiated directly - use the associated functions instead.
///
/// The tasks table stores work items (scans and other tasks) with:
/// - `task_id`: Primary key
/// - `task_type`: TaskType enum (0=Scan, etc.)
/// - `status`: TaskStatus enum (0=Pending, 1=Running, 2=Completed, 3=Stopped, 4=Error)
/// - `root_id`: Optional root reference
/// - `schedule_id`: Optional schedule reference
/// - `run_at`: When to run (0 for immediate)
/// - `source`: Manual (0) or Scheduled (1)
/// - `task_settings`: JSON config (e.g., ScanSettings)
/// - `task_state`: Execution state JSON (e.g., ScanTaskState with scan_id and HWM)
/// - `created_at`, `started_at`, `completed_at`: Lifecycle timestamps
pub struct TaskEntry;

impl TaskEntry {
    // ========================================
    // Database operations
    // ========================================

    /// Create a new manual task entry (Pending status, run_at = 0 for immediate execution)
    /// Must be called within a transaction for atomicity
    pub fn create_manual(
        conn: &rusqlite::Connection,
        root_id: i64,
        hash_mode: HashMode,
        validate_mode: ValidateMode,
    ) -> Result<(), FsPulseError> {
        let now = chrono::Utc::now().timestamp();

        // Verify root exists (within same transaction)
        let root_exists = conn
            .query_row("SELECT 1 FROM roots WHERE root_id = ?", [root_id], |_| {
                Ok(())
            })
            .optional()
            .map_err(FsPulseError::DatabaseError)?;

        if root_exists.is_none() {
            return Err(FsPulseError::Error(format!("Root {} not found", root_id)));
        }

        // Build task_settings using typed struct
        let task_settings = ScanSettings::new(hash_mode, validate_mode).to_json()?;

        // Create task entry with Pending status
        conn.execute(
            "INSERT INTO tasks (
                task_type, status, root_id, run_at,
                source, task_settings, created_at
            ) VALUES (?, 0, ?, 0, ?, ?, ?)",
            rusqlite::params![
                TaskType::Scan.as_i64(),
                root_id,
                SourceType::Manual.as_i32(),
                task_settings,
                now,
            ],
        )
        .map_err(FsPulseError::DatabaseError)?;

        Ok(())
    }

    /// Create a new compact database task entry, or no-op if one already exists.
    /// This is the singleton check — prevents duplicate pending compact tasks.
    /// Must be called within a transaction for atomicity.
    pub fn create_compact_database(conn: &rusqlite::Connection) -> Result<(), FsPulseError> {
        // Check for existing Pending compact task
        let exists: bool = conn
            .query_row(
                "SELECT COUNT(*) FROM tasks WHERE task_type = ? AND status = 0",
                [TaskType::CompactDatabase.as_i64()],
                |row| row.get::<_, i64>(0),
            )
            .map(|count| count > 0)
            .map_err(FsPulseError::DatabaseError)?;

        if exists {
            return Ok(()); // Already queued, no-op
        }

        let now = chrono::Utc::now().timestamp();
        let task_settings = CompactDatabaseSettings {}.to_json()?;

        conn.execute(
            "INSERT INTO tasks (
                task_type, status, run_at,
                source, task_settings, created_at
            ) VALUES (?, 0, 0, ?, ?, ?)",
            rusqlite::params![
                TaskType::CompactDatabase.as_i64(),
                SourceType::Manual.as_i32(),
                task_settings,
                now,
            ],
        )
        .map_err(FsPulseError::DatabaseError)?;

        Ok(())
    }

    /// Find the next task to process (generic across all task types)
    ///
    /// Priority order:
    /// 1. Running tasks (resume interrupted task from crash/restart)
    /// 2. Pending manual tasks (FIFO by task_id)
    /// 3. Pending scheduled tasks that are due (by run_at, then task_id)
    fn find_next_pending_task(conn: &Connection, now: i64) -> Result<Option<TaskRow>, FsPulseError> {
        // Step 1: Check for Running task (resume case — process died while executing)
        let active = conn
            .query_row(
                "SELECT t.task_id, t.task_type, t.status, t.root_id, r.root_path,
                        t.schedule_id, t.task_settings, t.task_state
                 FROM tasks t
                 LEFT JOIN roots r ON t.root_id = r.root_id
                 WHERE t.status = 1 LIMIT 1",
                [],
                |row| {
                    Ok(TaskRow {
                        task_id: row.get(0)?,
                        task_type: TaskType::from_i64(row.get(1)?),
                        status: TaskStatus::from_i64(row.get(2)?),
                        root_id: row.get(3)?,
                        root_path: row.get(4)?,
                        schedule_id: row.get(5)?,
                        task_settings: row.get(6)?,
                        task_state: row.get(7)?,
                    })
                },
            )
            .optional()
            .map_err(FsPulseError::DatabaseError)?;

        if let Some(row) = active {
            return Ok(Some(row));
        }

        // Step 2: Find highest priority Pending work (manual first, then scheduled due)
        let work = conn
            .query_row(
                "SELECT t.task_id, t.task_type, t.status, t.root_id, r.root_path,
                        t.schedule_id, t.task_settings, t.task_state
                 FROM tasks t
                 LEFT JOIN roots r ON t.root_id = r.root_id
                 WHERE t.status = 0 AND (t.source = ? OR t.run_at <= ?)
                 ORDER BY t.source ASC, t.run_at ASC, t.task_id ASC
                 LIMIT 1",
                rusqlite::params![SourceType::Manual.as_i32(), now],
                |row| {
                    Ok(TaskRow {
                        task_id: row.get(0)?,
                        task_type: TaskType::from_i64(row.get(1)?),
                        status: TaskStatus::from_i64(row.get(2)?),
                        root_id: row.get(3)?,
                        root_path: row.get(4)?,
                        schedule_id: row.get(5)?,
                        task_settings: row.get(6)?,
                        task_state: row.get(7)?,
                    })
                },
            )
            .optional()
            .map_err(FsPulseError::DatabaseError)?;

        Ok(work)
    }

    /// Atomically activate a Pending task: mark it Running and, if scheduled,
    /// create the next occurrence. No-op for Running tasks (resume case).
    /// Must be called within an immediate transaction.
    fn activate_task(conn: &Connection, row: &TaskRow, now: i64) -> Result<(), FsPulseError> {
        if row.status == TaskStatus::Running {
            // Resume case — task was already activated before crash
            return Ok(());
        }

        // Mark task as Running
        let rows_updated = conn
            .execute(
                "UPDATE tasks SET status = 1, started_at = ? WHERE task_id = ? AND status = 0",
                rusqlite::params![now, row.task_id],
            )
            .map_err(FsPulseError::DatabaseError)?;

        if rows_updated == 0 {
            return Err(FsPulseError::Error(format!(
                "Task {} was not in Pending status during activation",
                row.task_id
            )));
        }

        // For scheduled tasks, create the next occurrence
        if let Some(schedule_id) = row.schedule_id {
            let schedule = Schedule::get_by_id(conn, schedule_id)?.ok_or_else(|| {
                FsPulseError::Error(format!("Schedule {} not found", schedule_id))
            })?;

            let next_time = schedule.calculate_next_scan_time(now).map_err(|e| {
                FsPulseError::Error(format!("Failed to calculate next scan time: {}", e))
            })?;

            // Verify no other Pending row exists for this schedule
            let pending_exists: bool = conn
                .query_row(
                    "SELECT COUNT(*) FROM tasks WHERE schedule_id = ? AND status = 0 AND task_id != ?",
                    rusqlite::params![schedule_id, row.task_id],
                    |row| row.get::<_, i64>(0),
                )
                .map(|count| count > 0)
                .map_err(FsPulseError::DatabaseError)?;

            if !pending_exists {
                conn.execute(
                    "INSERT INTO tasks (
                        task_type, status, root_id, schedule_id, run_at,
                        source, task_settings, created_at
                    ) VALUES (?, 0, ?, ?, ?, ?, ?, ?)",
                    rusqlite::params![
                        row.task_type.as_i64(),
                        row.root_id,
                        schedule_id,
                        next_time,
                        SourceType::Scheduled.as_i32(),
                        row.task_settings,
                        now,
                    ],
                )
                .map_err(FsPulseError::DatabaseError)?;
            }
        }

        Ok(())
    }

    /// Get the next task to execute from the queue
    ///
    /// Atomically finds the next task, activates it (marks Running, schedules next
    /// occurrence), and constructs the concrete Task object via pure factory.
    ///
    /// Returns a boxed Task trait object ready for execution, or None if no work available.
    pub fn get_next_task(conn: &Connection) -> Result<Option<Box<dyn Task>>, FsPulseError> {
        let now = chrono::Utc::now().timestamp();

        Database::immediate_transaction(conn, |c| {
            let row = match Self::find_next_pending_task(c, now)? {
                Some(r) => r,
                None => return Ok(None),
            };

            // Generic activation: mark Running + schedule next occurrence
            Self::activate_task(c, &row, now)?;

            // Dispatch to task-type-specific constructor (pure, no I/O)
            let task: Box<dyn Task> = match row.task_type {
                TaskType::Scan => {
                    let root_id = row.root_id.ok_or_else(|| {
                        FsPulseError::Error(format!(
                            "Scan task {} has NULL root_id",
                            row.task_id
                        ))
                    })?;
                    let root_path = row.root_path.ok_or_else(|| {
                        FsPulseError::Error(format!(
                            "Scan task {} has NULL root_path",
                            row.task_id
                        ))
                    })?;
                    Box::new(ScanTask::new(
                        row.task_id,
                        root_id,
                        root_path,
                        row.schedule_id,
                        &row.task_settings,
                        row.task_state.as_deref(),
                    )?)
                }
                TaskType::CompactDatabase => {
                    Box::new(CompactDatabaseTask::new(row.task_id))
                }
            };

            Ok(Some(task))
        })
    }

    /// Complete a task by setting its terminal status and completion timestamp.
    /// Tasks are never deleted — they become historical records.
    pub fn complete_task(conn: &Connection, task_id: i64, terminal_status: TaskStatus) -> Result<(), FsPulseError> {
        let now = chrono::Utc::now().timestamp();

        Database::immediate_transaction(conn, |c| {
            c.execute(
                "UPDATE tasks SET status = ?, completed_at = ? WHERE task_id = ?",
                rusqlite::params![terminal_status.as_i64(), now, task_id],
            )
            .map_err(FsPulseError::DatabaseError)?;

            log::debug!(
                "Task {} completed with status {:?}",
                task_id,
                terminal_status
            );

            Ok(())
        })
    }

    /// Update the task_state JSON for an active (Running) task.
    /// Uses an immediate transaction and guards against updating non-active tasks.
    /// Returns an error if the task is not in Running status.
    pub fn set_task_state(task_id: i64, task_state: &str) -> Result<(), FsPulseError> {
        let conn = Database::get_connection()?;
        Database::immediate_transaction(&conn, |c| {
            let rows = c
                .execute(
                    "UPDATE tasks SET task_state = ? WHERE task_id = ? AND status = 1",
                    rusqlite::params![task_state, task_id],
                )
                .map_err(FsPulseError::DatabaseError)?;

            if rows == 0 {
                return Err(FsPulseError::Error(format!(
                    "Failed to update task_state: task {} is not active",
                    task_id
                )));
            }
            Ok(())
        })
    }

    /// Get upcoming tasks for display in UI
    /// Returns task entries with root_path and schedule_name joined
    /// Limited to next 10 upcoming tasks, ordered by priority (Running first, then manual, then by time)
    /// If tasks_are_paused is true, includes the Running task as first row
    pub fn get_upcoming_tasks(
        limit: i64,
        tasks_are_paused: bool,
    ) -> Result<Vec<UpcomingTask>, FsPulseError> {
        let now = chrono::Utc::now().timestamp();
        let conn = Database::get_connection()?;

        let pending = TaskStatus::Pending.as_i64();
        let running = TaskStatus::Running.as_i64();

        // Build WHERE clause based on pause state
        let where_clause = if tasks_are_paused {
            // Include Running and Pending tasks
            format!("WHERE q.status IN ({}, {})", pending, running)
        } else {
            // Only Pending tasks
            format!("WHERE q.status = {}", pending)
        };

        let query = format!(
            "SELECT
                q.task_id,
                q.task_type,
                q.root_id,
                r.root_path,
                q.schedule_id,
                s.schedule_name,
                q.run_at,
                q.source,
                q.status
             FROM tasks q
             LEFT JOIN roots r ON q.root_id = r.root_id
             LEFT JOIN scan_schedules s ON q.schedule_id = s.schedule_id
             {}
             ORDER BY q.status DESC, q.source ASC, q.run_at ASC, q.task_id ASC
             LIMIT ?",
            where_clause
        );

        let mut stmt = conn.prepare(&query).map_err(FsPulseError::DatabaseError)?;

        let tasks = stmt
            .query_map([limit], |row| {
                let run_at: i64 = row.get(6)?;
                let source: i32 = row.get(7)?;

                Ok(UpcomingTask {
                    task_id: row.get(0)?,
                    task_type: TaskType::from_i64(row.get(1)?),
                    root_id: row.get(2)?,
                    root_path: row.get(3)?,
                    schedule_id: row.get(4)?,
                    schedule_name: row.get(5)?,
                    run_at,
                    source: SourceType::from_i32(source).ok_or_else(|| {
                        rusqlite::Error::InvalidColumnType(
                            7,
                            "source".to_string(),
                            rusqlite::types::Type::Integer,
                        )
                    })?,
                    is_ready: run_at <= now,
                    status: row.get(8)?,
                })
            })
            .map_err(FsPulseError::DatabaseError)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(FsPulseError::DatabaseError)?;

        Ok(tasks)
    }

    /// Get task history count for pagination
    /// Returns count of tasks in terminal states (Completed, Stopped, Error)
    /// Optionally filtered by task_type
    pub fn get_task_history_count(
        task_type: Option<TaskType>,
    ) -> Result<i64, FsPulseError> {
        let conn = Database::get_connection()?;

        let completed = TaskStatus::Completed.as_i64();
        let stopped = TaskStatus::Stopped.as_i64();
        let error = TaskStatus::Error.as_i64();
        let terminal_statuses = format!("{}, {}, {}", completed, stopped, error);

        let (query, params): (String, Vec<Box<dyn rusqlite::types::ToSql>>) = match task_type {
            Some(tt) => (
                format!("SELECT COUNT(*) FROM tasks WHERE status IN ({}) AND task_type = ?", terminal_statuses),
                vec![Box::new(tt.as_i64())],
            ),
            None => (
                format!("SELECT COUNT(*) FROM tasks WHERE status IN ({})", terminal_statuses),
                vec![],
            ),
        };

        let count: i64 = conn
            .query_row(&query, rusqlite::params_from_iter(params.iter()), |row| row.get(0))
            .map_err(FsPulseError::DatabaseError)?;

        Ok(count)
    }

    /// Get task history for display in UI
    /// Returns completed/stopped/error tasks with root_path and schedule_name
    /// Optionally filtered by task_type, paginated
    pub fn get_task_history(
        task_type: Option<TaskType>,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<TaskHistoryRow>, FsPulseError> {
        let conn = Database::get_connection()?;

        let completed = TaskStatus::Completed.as_i64();
        let stopped = TaskStatus::Stopped.as_i64();
        let error = TaskStatus::Error.as_i64();

        let type_filter = match task_type {
            Some(tt) => format!("AND t.task_type = {}", tt.as_i64()),
            None => String::new(),
        };

        let query = format!(
            "SELECT
                t.task_id,
                t.task_type,
                t.root_id,
                r.root_path,
                s.schedule_name,
                t.source,
                t.status,
                t.started_at,
                t.completed_at
             FROM tasks t
             LEFT JOIN roots r ON t.root_id = r.root_id
             LEFT JOIN scan_schedules s ON t.schedule_id = s.schedule_id
             WHERE t.status IN ({}, {}, {}) {}
             ORDER BY t.completed_at DESC, t.task_id DESC
             LIMIT ? OFFSET ?",
            completed, stopped, error, type_filter
        );

        let mut stmt = conn.prepare(&query).map_err(FsPulseError::DatabaseError)?;

        let tasks = stmt
            .query_map(rusqlite::params![limit, offset], |row| {
                let source: i32 = row.get(5)?;
                let status: i64 = row.get(6)?;

                Ok(TaskHistoryRow {
                    task_id: row.get(0)?,
                    task_type: TaskType::from_i64(row.get(1)?),
                    root_id: row.get(2)?,
                    root_path: row.get(3)?,
                    schedule_name: row.get(4)?,
                    source: SourceType::from_i32(source).ok_or_else(|| {
                        rusqlite::Error::InvalidColumnType(
                            5,
                            "source".to_string(),
                            rusqlite::types::Type::Integer,
                        )
                    })?,
                    status: TaskStatus::from_i64(status),
                    started_at: row.get(7)?,
                    completed_at: row.get(8)?,
                })
            })
            .map_err(FsPulseError::DatabaseError)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(FsPulseError::DatabaseError)?;

        Ok(tasks)
    }
}

/// Count active schedules for a specific root
/// Returns the number of enabled schedules for the given root
pub fn count_schedules_for_root(root_id: i64) -> Result<i64, FsPulseError> {
    let conn = Database::get_connection()?;

    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM scan_schedules WHERE root_id = ? AND enabled = 1 AND deleted_at IS NULL",
        [root_id],
        |row| row.get(0)
    )
    .map_err(FsPulseError::DatabaseError)?;

    Ok(count)
}

/// Check if a root has an active scan in progress.
///
/// A root has an active scan if there is a row in the tasks table
/// with the specified root_id and status = 1 (Running).
///
/// IMPORTANT: The `_immediate` suffix indicates this function MUST be called
/// within an immediate transaction. The caller is responsible for managing
/// the transaction.
pub fn root_has_active_scan_immediate(
    conn: &rusqlite::Connection,
    root_id: i64,
) -> Result<bool, FsPulseError> {
    let has_active_scan: bool = conn
        .query_row(
            "SELECT COUNT(*) FROM tasks WHERE root_id = ? AND status = 1",
            [root_id],
            |row| row.get::<_, i64>(0),
        )
        .map(|count| count > 0)
        .map_err(FsPulseError::DatabaseError)?;

    Ok(has_active_scan)
}

/// Delete all schedules and Pending task entries for a specific root.
///
/// Only deletes Pending tasks (status = 0). Running tasks are left alone
/// (they'll finish independently). Historical completed/stopped/error tasks
/// are preserved.
///
/// IMPORTANT: The `_immediate` suffix indicates this function MUST be called
/// within an immediate transaction. The caller is responsible for managing
/// the transaction.
pub fn delete_schedules_for_root_immediate(
    conn: &rusqlite::Connection,
    root_id: i64,
) -> Result<(), FsPulseError> {
    // Delete only Pending task entries
    conn.execute(
        "DELETE FROM tasks WHERE root_id = ? AND status = 0",
        [root_id],
    )
    .map_err(FsPulseError::DatabaseError)?;

    // Soft delete schedules by setting deleted_at timestamp
    conn.execute(
        "UPDATE scan_schedules SET deleted_at = strftime('%s', 'now', 'utc')
         WHERE root_id = ? AND deleted_at IS NULL",
        [root_id],
    )
    .map_err(FsPulseError::DatabaseError)?;

    Ok(())
}

/// Information about an upcoming task for UI display
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpcomingTask {
    pub task_id: i64,
    pub task_type: TaskType,
    pub root_id: Option<i64>,
    pub root_path: Option<String>,
    pub schedule_id: Option<i64>,
    pub schedule_name: Option<String>, // NULL for manual scans
    pub run_at: i64,                   // Unix timestamp (0 = immediately)
    pub source: SourceType,
    pub is_ready: bool,  // true if run_at <= now (eligible to start)
    pub status: i64,     // TaskStatus as integer (0=Pending, 1=Running)
}

/// A completed task for history display
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskHistoryRow {
    pub task_id: i64,
    pub task_type: TaskType,
    pub root_id: Option<i64>,
    pub root_path: Option<String>,
    pub schedule_name: Option<String>,
    pub source: SourceType,
    pub status: TaskStatus,
    pub started_at: Option<i64>,
    pub completed_at: Option<i64>,
}

/// Schedule with root path and next scan time
#[derive(Debug, Serialize)]
pub struct ScheduleWithRoot {
    #[serde(flatten)]
    pub schedule: Schedule,
    pub root_path: String,
    pub next_scan_time: Option<i64>,
}

/// List all schedules with root information and next scan time
/// Only returns non-deleted schedules, ordered by schedule name
pub fn list_schedules() -> Result<Vec<ScheduleWithRoot>, FsPulseError> {
    let conn = Database::get_connection()?;

    let mut stmt = conn.prepare(
        "SELECT
            s.schedule_id, s.root_id, s.enabled, s.schedule_name, s.schedule_type,
            s.time_of_day, s.days_of_week, s.day_of_month,
            s.interval_value, s.interval_unit,
            s.hash_mode, s.validate_mode,
            s.created_at, s.updated_at,
            r.root_path,
            q.run_at
        FROM scan_schedules s
        INNER JOIN roots r ON s.root_id = r.root_id
        LEFT JOIN tasks q ON s.schedule_id = q.schedule_id AND q.status = 0
        WHERE s.deleted_at IS NULL
        ORDER BY s.schedule_name COLLATE NOCASE ASC",
    )?;

    let schedules = stmt.query_map([], |row| {
        Ok(ScheduleWithRoot {
            schedule: Schedule {
                schedule_id: row.get(0)?,
                root_id: row.get(1)?,
                enabled: row.get(2)?,
                schedule_name: row.get(3)?,
                schedule_type: ScheduleType::from_i32(row.get(4)?).ok_or_else(|| {
                    rusqlite::Error::InvalidColumnType(
                        4,
                        "schedule_type".to_string(),
                        rusqlite::types::Type::Integer,
                    )
                })?,
                time_of_day: row.get(5)?,
                days_of_week: row.get(6)?,
                day_of_month: row.get(7)?,
                interval_value: row.get(8)?,
                interval_unit: row
                    .get::<_, Option<i32>>(9)?
                    .map(|v| {
                        IntervalUnit::from_i32(v).ok_or_else(|| {
                            rusqlite::Error::InvalidColumnType(
                                9,
                                "interval_unit".to_string(),
                                rusqlite::types::Type::Integer,
                            )
                        })
                    })
                    .transpose()?,
                hash_mode: HashMode::from_i32(row.get(10)?).ok_or_else(|| {
                    rusqlite::Error::InvalidColumnType(
                        10,
                        "hash_mode".to_string(),
                        rusqlite::types::Type::Integer,
                    )
                })?,
                validate_mode: ValidateMode::from_i32(row.get(11)?).ok_or_else(|| {
                    rusqlite::Error::InvalidColumnType(
                        11,
                        "validate_mode".to_string(),
                        rusqlite::types::Type::Integer,
                    )
                })?,
                created_at: row.get(12)?,
                updated_at: row.get(13)?,
            },
            root_path: row.get(14)?,
            next_scan_time: row.get(15)?,
        })
    })?;

    let results: Result<Vec<_>, _> = schedules.collect();
    results.map_err(FsPulseError::DatabaseError)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Local, TimeZone};

    // Base time for all tests: Wednesday, January 15, 2025, 10:30:00 local time
    fn base_time() -> i64 {
        Local
            .with_ymd_and_hms(2025, 1, 15, 10, 30, 0)
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
            time_of_day: None, // Missing!
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
            time_of_day: Some("25:00".to_string()), // Invalid hour
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
            day_of_month: Some(32), // Invalid day
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
        let expected = Local
            .with_ymd_and_hms(2025, 1, 15, 14, 0, 0)
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
        let expected = Local
            .with_ymd_and_hms(2025, 1, 16, 9, 0, 0)
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
        let expected = Local
            .with_ymd_and_hms(2025, 1, 15, 14, 0, 0)
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
        let expected = Local
            .with_ymd_and_hms(2025, 1, 22, 9, 0, 0)
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
        let expected = Local
            .with_ymd_and_hms(2025, 1, 20, 14, 0, 0)
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
        let expected = Local
            .with_ymd_and_hms(2025, 1, 15, 14, 0, 0)
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
        let expected = Local
            .with_ymd_and_hms(2025, 1, 20, 14, 0, 0)
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
        let expected = Local
            .with_ymd_and_hms(2025, 1, 20, 14, 0, 0)
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
        let expected = Local
            .with_ymd_and_hms(2025, 2, 10, 14, 0, 0)
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
        let expected = Local
            .with_ymd_and_hms(2025, 1, 31, 14, 0, 0)
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
        let base = Local
            .with_ymd_and_hms(2025, 2, 12, 10, 30, 0)
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
        let expected = Local
            .with_ymd_and_hms(2025, 3, 31, 14, 0, 0)
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
        let base = Local
            .with_ymd_and_hms(2025, 2, 12, 10, 30, 0)
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
        let expected = Local
            .with_ymd_and_hms(2025, 3, 30, 14, 0, 0)
            .single()
            .unwrap()
            .timestamp();
        assert_eq!(next, expected);
    }
}
