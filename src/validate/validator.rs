use std::{ffi::OsStr, fmt, path::Path, sync::Arc};

use crate::error::FsPulseError;
use crate::progress::{ProgressId, ProgressReporter};

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
        prog_id: ProgressId,
        reporter: &Arc<dyn ProgressReporter>,
    ) -> Result<(ValidationState, Option<String>), FsPulseError>;

    fn wants_steady_tick(&self) -> bool;
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn test_validation_state_as_str() {
        assert_eq!(ValidationState::Unknown.as_str(), "U");
        assert_eq!(ValidationState::Valid.as_str(), "V");
        assert_eq!(ValidationState::Invalid.as_str(), "I");
        assert_eq!(ValidationState::NoValidator.as_str(), "N");
    }

    #[test]
    fn test_validation_state_from_char() {
        assert_eq!(ValidationState::from_char('U'), ValidationState::Unknown);
        assert_eq!(ValidationState::from_char('V'), ValidationState::Valid);
        assert_eq!(ValidationState::from_char('I'), ValidationState::Invalid);
        assert_eq!(ValidationState::from_char('N'), ValidationState::NoValidator);
        assert_eq!(ValidationState::from_char('X'), ValidationState::Unknown); // Default for invalid
    }

    #[test]
    fn test_validation_state_from_string() {
        assert_eq!(ValidationState::from_string("U"), ValidationState::Unknown);
        assert_eq!(ValidationState::from_string("V"), ValidationState::Valid);
        assert_eq!(ValidationState::from_string("I"), ValidationState::Invalid);
        assert_eq!(ValidationState::from_string("N"), ValidationState::NoValidator);
        assert_eq!(ValidationState::from_string(""), ValidationState::Unknown); // Default for empty
        assert_eq!(ValidationState::from_string("Invalid"), ValidationState::Invalid); // First char
    }

    #[test]
    fn test_validation_state_display() {
        assert_eq!(format!("{}", ValidationState::Unknown), "U");
        assert_eq!(format!("{}", ValidationState::Valid), "V");
        assert_eq!(format!("{}", ValidationState::Invalid), "I");
        assert_eq!(format!("{}", ValidationState::NoValidator), "N");
    }

    #[test]
    fn test_validation_state_short_str_to_full() {
        assert_eq!(ValidationState::short_str_to_full("U").unwrap(), "Unknown");
        assert_eq!(ValidationState::short_str_to_full("V").unwrap(), "Valid");
        assert_eq!(ValidationState::short_str_to_full("I").unwrap(), "Invalid");
        assert_eq!(ValidationState::short_str_to_full("N").unwrap(), "No Validator");
        assert!(ValidationState::short_str_to_full("X").is_err());
    }

    #[test]
    fn test_validation_state_default() {
        assert_eq!(ValidationState::default(), ValidationState::Unknown);
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
