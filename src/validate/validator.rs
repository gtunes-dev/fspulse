use std::{ffi::OsStr, fmt, path::Path};

use indicatif::ProgressBar;

use crate::error::FsPulseError;

use super::{claxon::ClaxonValidator, image::ImageValidator, lopdf::LopdfValidator};

/// Represents the validation state of an item.
/// Stored as a single-character code in the database for compactness.
/// Unknown or invalid values in the database default to `Unknown`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum ValidationState {
    #[default]
    Unknown,
    Valid,
    Invalid,
    NoValidator,
}

// macro to simplify code in validators which generates Ok(invalid) results
#[macro_export]
macro_rules! try_invalid {
    ($expr:expr) => {
        match $expr {
            Ok(val) => val,
            Err(e) => return Ok((ValidationState::Invalid, Some(e.to_string()))),
        }
    };
}

impl ValidationState {
    /// Returns the short code representing the validation state.
    pub fn as_str(&self) -> &'static str {
        match self {
            ValidationState::Unknown => "U",
            ValidationState::Valid => "V",
            ValidationState::Invalid => "I",
            ValidationState::NoValidator => "N",
        }
    }

    pub fn short_str_to_full(s: &str) -> Result<&str, FsPulseError> {
        match s {
            "U" => Ok("Unknown"),
            "V" => Ok("Valid"),
            "I" => Ok("Invalid"),
            "N" => Ok("No Validator"),
            _ => Err(FsPulseError::Error(format!(
                "Invalid validation state: '{s}'"
            ))),
        }
    }

    /// Converts from a string representation from the database,
    /// defaulting to `Unknown` for invalid or empty values.
    pub fn from_string(value: &str) -> Self {
        value
            .chars()
            .next()
            .map(ValidationState::from_char)
            .unwrap_or_default()
    }

    /// Convert a single-character string from the database into a State
    pub fn from_char(value: char) -> Self {
        match value {
            'U' => ValidationState::Unknown,
            'V' => ValidationState::Valid,
            'I' => ValidationState::Invalid,
            'N' => ValidationState::NoValidator,
            _ => ValidationState::Unknown,
        }
    }
}

/// Implement Display to print the short codes directly
impl fmt::Display for ValidationState {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

pub fn from_extension<S>(ext: S) -> Option<Box<dyn Validator>>
where
    S: AsRef<OsStr>,
{
    let ext = ext.as_ref().to_str()?.to_ascii_lowercase();

    match ext.as_str() {
        "flac" => Some(Box::new(ClaxonValidator::new())),
        "jpg" | "jpeg" | "png" | "gif" | "tiff" | "bmp" => Some(Box::new(ImageValidator::new())),
        "pdf" => Some(Box::new(LopdfValidator::new())),
        _ => None,
    }
}

pub fn from_path<P>(path: P) -> Option<Box<dyn Validator>>
where
    P: AsRef<Path>,
{
    path.as_ref().extension().and_then(from_extension)
}

/// Defines the behavior of a validator.
pub trait Validator {
    fn validate(
        &self,
        path: &Path,
        validation_pb: &ProgressBar,
    ) -> Result<(ValidationState, Option<String>), FsPulseError>;

    fn wants_steady_tick(&self) -> bool;
}
