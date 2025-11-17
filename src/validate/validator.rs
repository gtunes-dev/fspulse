use std::sync::atomic::AtomicBool;
use std::{ffi::OsStr, fmt, path::Path, sync::Arc};

use log::warn;

use crate::error::FsPulseError;

use super::{claxon::ClaxonValidator, image::ImageValidator, lopdf::LopdfValidator};

/// Represents the validation state of an item.
/// Stored as integer in the database.
/// Unknown or invalid values in the database default to `Unknown`.
#[repr(i64)]
#[derive(Debug, Clone, PartialEq, Eq, Copy)]
pub enum ValidationState {
    Unknown = 0,
    Valid = 1,
    Invalid = 2,
    NoValidator = 3,
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
    pub fn as_i64(&self) -> i64 {
        *self as i64
    }

    pub fn from_i64(value: i64) -> Self {
        match value {
            0 => ValidationState::Unknown,
            1 => ValidationState::Valid,
            2 => ValidationState::Invalid,
            3 => ValidationState::NoValidator,
            _ => {
                warn!("Invalid ValidationState value in database: {}, defaulting to Unknown", value);
                ValidationState::Unknown
            }
        }
    }

    pub fn short_name(&self) -> &'static str {
        match self {
            ValidationState::Unknown => "U",
            ValidationState::Valid => "V",
            ValidationState::Invalid => "I",
            ValidationState::NoValidator => "N",
        }
    }

    pub fn full_name(&self) -> &'static str {
        match self {
            ValidationState::Unknown => "Unknown",
            ValidationState::Valid => "Valid",
            ValidationState::Invalid => "Invalid",
            ValidationState::NoValidator => "No Validator",
        }
    }

    pub fn from_string(s: &str) -> Option<Self> {
        match s.to_ascii_uppercase().as_str() {
            // Full names
            "UNKNOWN" => Some(ValidationState::Unknown),
            "VALID" => Some(ValidationState::Valid),
            "INVALID" => Some(ValidationState::Invalid),
            "NO VALIDATOR" | "NOVALIDATOR" => Some(ValidationState::NoValidator),
            // Short names
            "U" => Some(ValidationState::Unknown),
            "V" => Some(ValidationState::Valid),
            "I" => Some(ValidationState::Invalid),
            "N" => Some(ValidationState::NoValidator),
            _ => None,
        }
    }
}

/// Implement Display to print the full names
impl fmt::Display for ValidationState {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.full_name())
    }
}

impl crate::query::QueryEnum for ValidationState {
    fn from_token(s: &str) -> Option<i64> {
        Self::from_string(s).map(|state| state.as_i64())
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
        interrupt_token: &Arc<AtomicBool>,
    ) -> Result<(ValidationState, Option<String>), FsPulseError>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn test_validation_state_integer_values() {
        // Verify the integer values match the expected order
        assert_eq!(ValidationState::Unknown.as_i64(), 0);
        assert_eq!(ValidationState::Valid.as_i64(), 1);
        assert_eq!(ValidationState::Invalid.as_i64(), 2);
        assert_eq!(ValidationState::NoValidator.as_i64(), 3);
    }

    #[test]
    fn test_validation_state_from_i64() {
        // Verify round-trip conversion
        assert_eq!(ValidationState::from_i64(0), ValidationState::Unknown);
        assert_eq!(ValidationState::from_i64(1), ValidationState::Valid);
        assert_eq!(ValidationState::from_i64(2), ValidationState::Invalid);
        assert_eq!(ValidationState::from_i64(3), ValidationState::NoValidator);

        // Invalid values should default to Unknown
        assert_eq!(ValidationState::from_i64(999), ValidationState::Unknown);
        assert_eq!(ValidationState::from_i64(-1), ValidationState::Unknown);
    }

    #[test]
    fn test_validation_state_from_string() {
        assert_eq!(ValidationState::from_string("U"), Some(ValidationState::Unknown));
        assert_eq!(ValidationState::from_string("V"), Some(ValidationState::Valid));
        assert_eq!(ValidationState::from_string("I"), Some(ValidationState::Invalid));
        assert_eq!(ValidationState::from_string("N"), Some(ValidationState::NoValidator));
        assert_eq!(ValidationState::from_string("UNKNOWN"), Some(ValidationState::Unknown));
        assert_eq!(ValidationState::from_string("VALID"), Some(ValidationState::Valid));
        assert_eq!(ValidationState::from_string("INVALID"), Some(ValidationState::Invalid));
        assert_eq!(ValidationState::from_string("NO VALIDATOR"), Some(ValidationState::NoValidator));
        assert_eq!(ValidationState::from_string(""), None); // Invalid
        assert_eq!(ValidationState::from_string("X"), None); // Invalid
    }

    #[test]
    fn test_validation_state_display() {
        assert_eq!(format!("{}", ValidationState::Unknown), "Unknown");
        assert_eq!(format!("{}", ValidationState::Valid), "Valid");
        assert_eq!(format!("{}", ValidationState::Invalid), "Invalid");
        assert_eq!(format!("{}", ValidationState::NoValidator), "No Validator");
    }


    #[test]
    fn test_from_extension() {
        assert!(from_extension("flac").is_some());
        assert!(from_extension("jpg").is_some());
        assert!(from_extension("jpeg").is_some());
        assert!(from_extension("png").is_some());
        assert!(from_extension("gif").is_some());
        assert!(from_extension("tiff").is_some());
        assert!(from_extension("bmp").is_some());
        assert!(from_extension("pdf").is_some());
        assert!(from_extension("txt").is_none());
        assert!(from_extension("unknown").is_none());
    }

    #[test]
    fn test_from_extension_case_insensitive() {
        assert!(from_extension("FLAC").is_some());
        assert!(from_extension("JPG").is_some());
        assert!(from_extension("PDF").is_some());
    }

    #[test]
    fn test_from_path() {
        assert!(from_path(Path::new("test.flac")).is_some());
        assert!(from_path(Path::new("image.jpg")).is_some());
        assert!(from_path(Path::new("document.pdf")).is_some());
        assert!(from_path(Path::new("readme.txt")).is_none());
        assert!(from_path(Path::new("no_extension")).is_none());
    }

    #[test]
    fn test_from_path_with_directory() {
        assert!(from_path(Path::new("/path/to/audio.flac")).is_some());
        assert!(from_path(Path::new("./relative/path/photo.png")).is_some());
    }
}
