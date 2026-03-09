use rusqlite::{params, Connection, OptionalExtension};

use crate::error::FsPulseError;
use crate::validate::validator::ValidationState;

/// Validation state for a file. Stored as integer in the database.
///
/// Note: There is no "Unknown" variant — absence of a val_versions row
/// for an item means it has never been validated.
#[repr(i64)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValState {
    Valid = 1,
    Invalid = 2,
}

impl ValState {
    pub fn as_i64(self) -> i64 {
        self as i64
    }

    pub fn from_i64(value: i64) -> Self {
        match value {
            1 => ValState::Valid,
            2 => ValState::Invalid,
            _ => {
                log::warn!("Invalid ValState value in database: {}, defaulting to Valid", value);
                ValState::Valid
            }
        }
    }

    /// Convert from the validator's ValidationState to ValState.
    ///
    /// Only Valid and Invalid map to ValState. NoValidator is now tracked
    /// via `items.has_validator` and Unknown means no row in val_versions.
    /// Callers should not pass Unknown or NoValidator here.
    pub fn from_validation_state(vs: ValidationState) -> Option<Self> {
        match vs {
            ValidationState::Valid => Some(ValState::Valid),
            ValidationState::Invalid => Some(ValState::Invalid),
            ValidationState::NoValidator | ValidationState::Unknown => None,
        }
    }
}

/// A single validation observation for a file. Maps to the `val_versions` table.
///
/// Each row represents a period where a particular validation result was observed.
/// `first_scan_id` is when this result was first computed; `last_scan_id` is
/// extended each time the same result is re-confirmed.
///
/// Absence of a row for an item means it has never been validated.
#[allow(dead_code)]
pub struct ValVersion {
    item_id: i64,
    first_scan_id: i64,
    last_scan_id: i64,
    val_state: ValState,
    val_error: Option<String>,
}

#[allow(dead_code)]
impl ValVersion {
    pub fn item_id(&self) -> i64 {
        self.item_id
    }

    pub fn first_scan_id(&self) -> i64 {
        self.first_scan_id
    }

    pub fn last_scan_id(&self) -> i64 {
        self.last_scan_id
    }

    pub fn val_state(&self) -> ValState {
        self.val_state
    }

    pub fn val_error(&self) -> Option<&str> {
        self.val_error.as_deref()
    }

    /// Get the most recent val_version for an item (if any).
    pub fn get_current(
        conn: &Connection,
        item_id: i64,
    ) -> Result<Option<Self>, FsPulseError> {
        conn.query_row(
            "SELECT item_id, first_scan_id, last_scan_id, val_state, val_error
             FROM val_versions
             WHERE item_id = ?
             ORDER BY first_scan_id DESC
             LIMIT 1",
            params![item_id],
            Self::from_row,
        )
        .optional()
        .map_err(FsPulseError::DatabaseError)
    }

    /// Insert a new validation observation.
    pub fn insert(
        conn: &Connection,
        item_id: i64,
        scan_id: i64,
        val_state: ValState,
        val_error: Option<&str>,
    ) -> Result<(), FsPulseError> {
        conn.execute(
            "INSERT INTO val_versions (item_id, first_scan_id, last_scan_id, val_state, val_error)
             VALUES (?, ?, ?, ?, ?)",
            params![item_id, scan_id, scan_id, val_state.as_i64(), val_error],
        )?;
        Ok(())
    }

    /// Extend the last_scan_id on an existing val_version (result re-confirmed).
    pub fn extend_last_scan(
        conn: &Connection,
        item_id: i64,
        first_scan_id: i64,
        new_last_scan_id: i64,
    ) -> Result<(), FsPulseError> {
        conn.execute(
            "UPDATE val_versions SET last_scan_id = ?
             WHERE item_id = ? AND first_scan_id = ?",
            params![new_last_scan_id, item_id, first_scan_id],
        )?;
        Ok(())
    }

    fn from_row(row: &rusqlite::Row) -> rusqlite::Result<Self> {
        Ok(ValVersion {
            item_id: row.get(0)?,
            first_scan_id: row.get(1)?,
            last_scan_id: row.get(2)?,
            val_state: ValState::from_i64(row.get(3)?),
            val_error: row.get(4)?,
        })
    }
}
