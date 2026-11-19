use crate::db::Database;
use crate::error::FsPulseError;
use crate::roots::Root;
use crate::utils::Utils;

use chrono::{Local, NaiveDate, TimeZone, Utc};
use log::info;
use rusqlite::{params, Connection, OptionalExtension, Result};
use serde::Serialize;

use std::fmt;

const SQL_SCAN_ID_OR_LATEST: &str =
    "SELECT scan_id, root_id, schedule_id, started_at, ended_at, was_restarted, state, is_hash, hash_all, is_val, new_hash_suspect_count, new_val_invalid_count, error
        FROM scans
        WHERE scan_id = IFNULL(?1, (SELECT MAX(scan_id) FROM scans))";

const SQL_LATEST_FOR_ROOT: &str =
    "SELECT scan_id, root_id, schedule_id, started_at, ended_at, was_restarted, state, is_hash, hash_all, is_val, new_hash_suspect_count, new_val_invalid_count, error
        FROM scans
        WHERE root_id = ?
        ORDER BY scan_id DESC LIMIT 1";

#[derive(Copy, Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[repr(i32)]
pub enum HashMode {
    None = 0,
    New = 1,
    All = 2,
}

impl HashMode {
    pub fn from_i32(value: i32) -> Option<Self> {
        match value {
            0 => Some(Self::None),
            1 => Some(Self::New),
            2 => Some(Self::All),
            _ => None,
        }
    }

    pub fn as_i32(self) -> i32 {
        self as i32
    }
}

#[derive(Copy, Clone, Debug)]
pub struct AnalysisSpec {
    hash_mode: HashMode,
    is_val: bool,
}

impl AnalysisSpec {
    pub fn new(hash_mode: HashMode, is_val: bool) -> Self {
        AnalysisSpec {
            hash_mode,
            is_val,
        }
    }

    pub fn is_hash(&self) -> bool {
        self.hash_mode != HashMode::None
    }

    pub fn hash_all(&self) -> bool {
        self.hash_mode == HashMode::All
    }

    pub fn is_val(&self) -> bool {
        self.is_val
    }
}

#[derive(Clone, Debug)]
pub struct Scan {
    // Schema fields
    scan_id: i64,
    root_id: i64,
    #[allow(dead_code)]
    schedule_id: Option<i64>,
    #[allow(dead_code)]
    started_at: i64,
    #[allow(dead_code)]
    ended_at: Option<i64>,
    state: ScanState,
    analysis_spec: AnalysisSpec,
    new_hash_suspect_count: Option<i64>,
    new_val_invalid_count: Option<i64>,
    error: Option<String>,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
#[repr(i64)]
pub enum ScanState {
    Scanning = 1,
    Completed = 2,
    Stopped = 3,
    Error = 4,
}

impl ScanState {
    pub fn from_i64(value: i64) -> Self {
        match value {
            1 => ScanState::Scanning,
            2 => ScanState::Completed,
            3 => ScanState::Stopped,
            4 => ScanState::Error,
            _ => panic!("Invalid ScanState value: {}", value),
        }
    }

    pub fn as_i64(&self) -> i64 {
        *self as i64
    }

    pub fn short_name(&self) -> &'static str {
        match self {
            ScanState::Scanning => "S",
            ScanState::Completed => "C",
            ScanState::Stopped => "P",
            ScanState::Error => "E",
        }
    }

    pub fn full_name(&self) -> &'static str {
        match self {
            ScanState::Scanning => "Scanning",
            ScanState::Completed => "Completed",
            ScanState::Stopped => "Stopped",
            ScanState::Error => "Error",
        }
    }

    pub fn from_string(s: &str) -> Option<Self> {
        match s.to_ascii_uppercase().as_str() {
            "SCANNING" => Some(ScanState::Scanning),
            "COMPLETED" => Some(ScanState::Completed),
            "STOPPED" => Some(ScanState::Stopped),
            "ERROR" => Some(ScanState::Error),
            "S" => Some(ScanState::Scanning),
            "C" => Some(ScanState::Completed),
            "P" => Some(ScanState::Stopped),
            "E" => Some(ScanState::Error),
            _ => None,
        }
    }
}

impl fmt::Display for ScanState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.full_name())
    }
}

impl crate::query::QueryEnum for ScanState {
    fn from_token(s: &str) -> Option<i64> {
        Self::from_string(s).map(|state| state.as_i64())
    }
}

impl Scan {
    // Create a Scan that will be used during a directory scan
    // In this case, the scan_id is not yet known
    fn new_for_scan(
        scan_id: i64,
        root_id: i64,
        schedule_id: Option<i64>,
        started_at: i64,
        state: i64,
        analysis_spec: AnalysisSpec,
    ) -> Self {
        Scan {
            scan_id,
            root_id,
            schedule_id,
            started_at,
            ended_at: None,
            state: ScanState::from_i64(state),
            analysis_spec,
            new_hash_suspect_count: None,
            new_val_invalid_count: None,
            error: None,
        }
    }

    pub fn create(
        conn: &rusqlite::Connection,
        root: &Root,
        schedule_id: Option<i64>,
        analysis_spec: &AnalysisSpec,
    ) -> Result<Self, FsPulseError> {
        let (scan_id, started_at): (i64, i64) = conn.query_row(
            "INSERT INTO scans (root_id, schedule_id, state, is_hash, hash_all, is_val, started_at)
             VALUES (?, ?, ?, ?, ?, ?, strftime('%s', 'now', 'utc'))
             RETURNING scan_id, started_at",
            params![
                root.root_id(),
                schedule_id,
                ScanState::Scanning.as_i64(),
                analysis_spec.is_hash() as i64,
                analysis_spec.hash_all() as i64,
                analysis_spec.is_val() as i64,
            ],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;

        let scan = Scan::new_for_scan(
            scan_id,
            root.root_id(),
            schedule_id,
            started_at,
            ScanState::Scanning.as_i64(),
            *analysis_spec,
        );
        Ok(scan)
    }

    pub fn get_latest_for_root(root_id: i64) -> Result<Option<Self>, FsPulseError> {
        let conn = Database::get_connection()?;
        Self::get_by_id_or_latest(&conn, None, Some(root_id))
    }

    pub fn get_by_id_or_latest(
        conn: &rusqlite::Connection,
        scan_id: Option<i64>,
        root_id: Option<i64>,
    ) -> Result<Option<Self>, FsPulseError> {
        let (query, query_param) = match (scan_id, root_id) {
            (Some(_), _) => (SQL_SCAN_ID_OR_LATEST, scan_id),
            (_, Some(_)) => (SQL_LATEST_FOR_ROOT, root_id),
            _ => (SQL_SCAN_ID_OR_LATEST, None),
        };

        // If the scan id wasn't explicitly specified, load the most recent otherwise,
        // load the specified scan
        let scan_row: Option<Scan> = conn
            .query_row(query, params![query_param], |row| {
                let is_hash = row.get(7)?;
                let hash_all = row.get(8)?;
                let hash_mode = match (is_hash, hash_all) {
                    (false, _) => HashMode::None,
                    (_, true) => HashMode::All,
                    _ => HashMode::New,
                };

                let is_val: bool = row.get(9)?;

                Ok(Scan {
                    scan_id: row.get(0)?,
                    root_id: row.get(1)?,
                    schedule_id: row.get(2)?,
                    started_at: row.get(3)?,
                    ended_at: row.get(4)?,
                    state: ScanState::from_i64(row.get(6)?),
                    analysis_spec: AnalysisSpec {
                        hash_mode,
                        is_val,
                    },
                    new_hash_suspect_count: row.get(10)?,
                    new_val_invalid_count: row.get(11)?,
                    error: row.get(12)?,
                })
            })
            .optional()?;

        Ok(scan_row)
    }

    pub fn scan_id(&self) -> i64 {
        self.scan_id
    }

    pub fn root_id(&self) -> i64 {
        self.root_id
    }

    pub fn state(&self) -> ScanState {
        self.state
    }

    pub fn analysis_spec(&self) -> &AnalysisSpec {
        &self.analysis_spec
    }

    pub fn started_at(&self) -> i64 {
        self.started_at
    }

    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    /// Resolve a date to the most recent completed scan for a root at or before that date.
    /// If `date_str` is None, returns the most recent completed scan for the root.
    /// Returns (scan_id, started_at) or None if no matching scan exists.
    pub fn resolve_scan_for_date(
        root_id: i64,
        date_str: Option<&str>,
    ) -> Result<Option<(i64, i64)>, FsPulseError> {
        let conn = Database::get_connection()?;

        match date_str {
            Some(date) => {
                // Get end-of-day timestamp for the given date
                let (_start_ts, end_ts) = crate::utils::Utils::single_date_bounds(date)?;

                let result: Option<(i64, i64)> = conn
                    .query_row(
                        "SELECT scan_id, started_at FROM scans
                         WHERE root_id = ? AND state = 2 AND started_at <= ?
                         ORDER BY started_at DESC LIMIT 1",
                        params![root_id, end_ts],
                        |row| Ok((row.get(0)?, row.get(1)?)),
                    )
                    .optional()?;

                Ok(result)
            }
            None => {
                let result: Option<(i64, i64)> = conn
                    .query_row(
                        "SELECT scan_id, started_at FROM scans
                         WHERE root_id = ? AND state = 2
                         ORDER BY scan_id DESC LIMIT 1",
                        params![root_id],
                        |row| Ok((row.get(0)?, row.get(1)?)),
                    )
                    .optional()?;

                Ok(result)
            }
        }
    }

    pub fn set_state_completed(&mut self, conn: &Connection) -> Result<(), FsPulseError> {
        match self.state() {
            ScanState::Scanning => {
                // Counted before taking the write lock: it reads every
                // file of the root, and the applier must not wait on it.
                let posture = crate::snapshots::count_integrity_posture(conn, self.root_id)?;

                let result =
                    Database::immediate_transaction(conn, |c| {
                        // New hash suspects: hash_versions with hash_state=2
                        // first seen during this scan (first_seen_at >= scan.started_at).
                        let new_hash_suspect_count: i64 = c
                            .query_row(
                                "SELECT COUNT(*) FROM hash_versions hv
                                 JOIN items i ON i.item_id = hv.item_id
                                 WHERE hv.first_seen_at >= (
                                     SELECT started_at FROM scans WHERE scan_id = ?2
                                 ) AND hv.hash_state = 2
                                   AND i.root_id = ?1",
                                params![self.root_id, self.scan_id],
                                |row| row.get(0),
                            )
                            .unwrap_or(0);

                        // New validation failures: val_state=2 validated
                        // during this scan (val_at >= scan.started_at).
                        let new_val_invalid_count: i64 = c
                            .query_row(
                                "SELECT COUNT(*) FROM item_versions
                                 WHERE root_id = ?
                                   AND val_state = 2
                                   AND val_at >= (
                                       SELECT started_at FROM scans WHERE scan_id = ?
                                   )",
                                params![self.root_id, self.scan_id],
                                |row| row.get(0),
                            )
                            .unwrap_or(0);

                        c.execute(
                            "UPDATE scans SET
                                new_hash_suspect_count = ?,
                                new_val_invalid_count = ?,
                                state = ?,
                                ended_at = strftime('%s', 'now', 'utc')
                            WHERE scan_id = ?",
                            params![
                                new_hash_suspect_count, new_val_invalid_count,
                                ScanState::Completed.as_i64(),
                                self.scan_id,
                            ],
                        )?;

                        // The root's integrity posture goes into the
                        // current hour's snapshot, where Trends reads it.
                        crate::snapshots::record_integrity_locked(
                            c,
                            self.root_id,
                            &posture,
                            new_val_invalid_count,
                            new_hash_suspect_count,
                            Utils::now_secs(),
                        )?;

                        Ok((new_hash_suspect_count, new_val_invalid_count))
                    })?;

                let (new_hash_suspect_count, new_val_invalid_count) = result;

                self.state = ScanState::Completed;
                self.new_hash_suspect_count = Some(new_hash_suspect_count);
                self.new_val_invalid_count = Some(new_val_invalid_count);

                Ok(())
            }
            _ => Err(FsPulseError::Error(format!(
                "Can't set Scan Id {} to state completed from state {}",
                self.scan_id(),
                self.state().as_i64()
            ))),
        }
    }

    pub fn set_state_stopped(&mut self, conn: &Connection) -> Result<(), FsPulseError> {
        match self.state() {
            ScanState::Scanning => {
                Scan::stop_scan(conn, self, None)?;
                self.state = ScanState::Stopped;
                Ok(())
            }
            _ => Err(FsPulseError::Error(format!(
                "Can't stop scan - invalid state {}",
                self.state().as_i64()
            ))),
        }
    }

    pub fn stop_scan(
        conn: &Connection,
        scan: &Scan,
        error_message: Option<&str>,
    ) -> Result<(), FsPulseError> {
        let action = if error_message.is_some() { "error" } else { "stop" };
        info!("Stopping scan {} (reason: {})...", scan.scan_id(), action);

        Database::immediate_transaction(conn, |c| {
            let final_state = if error_message.is_some() {
                ScanState::Error
            } else {
                ScanState::Stopped
            };

            c.execute(
                "UPDATE scans SET state = ?, error = ? WHERE scan_id = ?",
                params![final_state.as_i64(), error_message, scan.scan_id()],
            )?;

            Ok(())
        })?;

        Ok(())
    }
}

/// Compact scan summary for the ScanPicker component
#[derive(Debug, Clone, Serialize)]
pub struct ScanSummary {
    pub scan_id: i64,
    pub started_at: i64,
}

/// Get the distinct local-date strings (YYYY-MM-DD) that have completed scans
/// for a given root within a calendar month.
pub fn get_scan_dates_for_month(
    conn: &Connection,
    root_id: i64,
    year: i32,
    month: u32,
) -> Result<Vec<String>, FsPulseError> {
    use chrono::Datelike;

    let first_day = NaiveDate::from_ymd_opt(year, month, 1).ok_or_else(|| {
        FsPulseError::Error(format!("Invalid year/month: {year}-{month}"))
    })?;
    let last_day = if month == 12 {
        NaiveDate::from_ymd_opt(year + 1, 1, 1)
    } else {
        NaiveDate::from_ymd_opt(year, month + 1, 1)
    }
    .unwrap()
    .pred_opt()
    .unwrap();

    let first_str = first_day.format("%Y-%m-%d").to_string();
    let last_str = last_day.format("%Y-%m-%d").to_string();
    let (start_ts, end_ts) = crate::utils::Utils::range_date_bounds(&first_str, &last_str)?;

    let mut stmt = conn.prepare_cached(
        "SELECT started_at FROM scans
         WHERE root_id = ? AND state = 2 AND started_at >= ? AND started_at <= ?
         ORDER BY started_at",
    )?;

    let timestamps: Vec<i64> = stmt
        .query_map(params![root_id, start_ts, end_ts], |row| row.get(0))?
        .filter_map(|r| r.ok())
        .collect();

    // Convert UTC timestamps to local dates and deduplicate
    let mut dates: Vec<String> = Vec::new();
    for ts in timestamps {
        let dt = Utc
            .timestamp_opt(ts, 0)
            .single()
            .ok_or_else(|| FsPulseError::Error(format!("Invalid timestamp: {ts}")))?;
        let local = dt.with_timezone(&Local);
        let date_str = format!(
            "{:04}-{:02}-{:02}",
            local.year(),
            local.month(),
            local.day()
        );
        if dates.last() != Some(&date_str) {
            dates.push(date_str);
        }
    }

    Ok(dates)
}

/// Get all completed scans for a root on a specific local date, most recent first.
pub fn get_scans_for_date(
    conn: &Connection,
    root_id: i64,
    date_str: &str,
) -> Result<Vec<ScanSummary>, FsPulseError> {
    let (start_ts, end_ts) = crate::utils::Utils::single_date_bounds(date_str)?;

    let mut stmt = conn.prepare_cached(
        "SELECT scan_id, started_at
         FROM scans
         WHERE root_id = ? AND state = 2 AND started_at >= ? AND started_at <= ?
         ORDER BY started_at DESC",
    )?;

    let rows = stmt.query_map(params![root_id, start_ts, end_ts], |row| {
        Ok(ScanSummary {
            scan_id: row.get(0)?,
            started_at: row.get(1)?,
        })
    })?;

    let mut result = Vec::new();
    for row in rows {
        result.push(row?);
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scan_state_as_i64() {
        assert_eq!(ScanState::Scanning.as_i64(), 1);
        assert_eq!(ScanState::Completed.as_i64(), 2);
        assert_eq!(ScanState::Stopped.as_i64(), 3);
        assert_eq!(ScanState::Error.as_i64(), 4);
    }

    #[test]
    fn test_scan_state_from_i64() {
        assert_eq!(ScanState::from_i64(1), ScanState::Scanning);
        assert_eq!(ScanState::from_i64(2), ScanState::Completed);
        assert_eq!(ScanState::from_i64(3), ScanState::Stopped);
        assert_eq!(ScanState::from_i64(4), ScanState::Error);
    }

    #[test]
    fn test_scan_state_round_trip() {
        let states = [
            ScanState::Scanning,
            ScanState::Completed,
            ScanState::Stopped,
            ScanState::Error,
        ];

        for state in states {
            let i64_val = state.as_i64();
            let converted_back = ScanState::from_i64(i64_val);
            assert_eq!(state, converted_back, "Round trip failed for {state:?}");
        }
    }

    #[test]
    fn test_scan_state_display() {
        assert_eq!(ScanState::Scanning.to_string(), "Scanning");
        assert_eq!(ScanState::Completed.to_string(), "Completed");
        assert_eq!(ScanState::Stopped.to_string(), "Stopped");
        assert_eq!(ScanState::Error.to_string(), "Error");
    }

    #[test]
    fn test_hash_mode_enum() {
        // Test that HashMode enum has expected variants
        let _none = HashMode::None;
        let _new = HashMode::New;
        let _all = HashMode::All;

        // Test PartialEq
        assert_eq!(HashMode::None, HashMode::None);
        assert_ne!(HashMode::None, HashMode::New);
        assert_ne!(HashMode::New, HashMode::All);
    }

    #[test]
    fn test_analysis_spec_is_val() {
        let spec_val = AnalysisSpec::new(HashMode::None, true);
        assert!(spec_val.is_val());
        assert!(!spec_val.is_hash());

        let spec_no_val = AnalysisSpec::new(HashMode::New, false);
        assert!(!spec_no_val.is_val());
        assert!(spec_no_val.is_hash());
    }
}
