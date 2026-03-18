use crate::validate::validator::ValidationState;

/// Validation state for a file. Stored as integer in the database.
///
/// Note: There is no "Unknown" variant — NULL val_state on item_versions
/// means the version has not been validated.
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

    #[allow(dead_code)]
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
    /// Only Valid and Invalid map to ValState. NoValidator is tracked
    /// via `items.has_validator` and Unknown means NULL val_state.
    /// Callers should not pass Unknown or NoValidator here.
    pub fn from_validation_state(vs: ValidationState) -> Option<Self> {
        match vs {
            ValidationState::Valid => Some(ValState::Valid),
            ValidationState::Invalid => Some(ValState::Invalid),
            ValidationState::NoValidator | ValidationState::Unknown => None,
        }
    }
}
