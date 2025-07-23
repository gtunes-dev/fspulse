use std::path::Path;

use indicatif::ProgressBar;

use image::ImageReader;

use crate::error::FsPulseError;

use super::validator::{ValidationState, Validator};

/// Validator implementation for FLAC audio files using the Claxon crate.
pub struct ImageValidator;

impl ImageValidator {
    /// Constructs a new ImageValidator instance.
    pub fn new() -> Self {
        ImageValidator
    }
}

impl Validator for ImageValidator {
    fn validate(
        &self,
        path: &Path,
        _validation_pb: &ProgressBar,
    ) -> Result<(ValidationState, Option<String>), FsPulseError> {
        let open_result = ImageReader::open(path);
        let reader = match open_result {
            Ok(reader) => reader,
            Err(e) => {
                let e_str = e.to_string();
                return Ok((ValidationState::Invalid, Some(e_str)));
            }
        };

        match reader.decode() {
            Ok(_) => Ok((ValidationState::Valid, None)),
            Err(e) => {
                let e_str = e.to_string();
                Ok((ValidationState::Invalid, Some(e_str)))
            }
        }
    }

    fn wants_steady_tick(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use indicatif::ProgressBar;
    use std::path::Path;

    #[test]
    fn test_image_validator_new() {
        let validator = ImageValidator::new();
        assert!(validator.wants_steady_tick());
    }

    #[test]
    fn test_image_validator_wants_steady_tick() {
        let validator = ImageValidator::new();
        assert!(validator.wants_steady_tick());
    }

    #[test]
    fn test_image_validator_nonexistent_file() {
        let validator = ImageValidator::new();
        let progress_bar = ProgressBar::hidden();
        let nonexistent_path = Path::new("/this/path/does/not/exist.jpg");
        
        let result = validator.validate(nonexistent_path, &progress_bar);
        assert!(result.is_ok());
        
        let (state, error_msg) = result.unwrap();
        assert_eq!(state, ValidationState::Invalid);
        assert!(error_msg.is_some());
        let msg = error_msg.unwrap();
        assert!(msg.contains("No such file or directory") || 
                msg.contains("cannot find the file") ||
                msg.contains("system cannot find the file"));
    }

    #[test]
    fn test_image_validator_invalid_file() {
        let validator = ImageValidator::new();
        let progress_bar = ProgressBar::hidden();
        
        // Create a temporary file with invalid image content
        use std::io::Write;
        use tempfile::NamedTempFile;
        
        let mut temp_file = NamedTempFile::new().expect("Failed to create temp file");
        temp_file.write_all(b"not an image file").expect("Failed to write temp file");
        
        let result = validator.validate(temp_file.path(), &progress_bar);
        assert!(result.is_ok());
        
        let (state, error_msg) = result.unwrap();
        assert_eq!(state, ValidationState::Invalid);
        assert!(error_msg.is_some());
    }

    #[test]
    fn test_image_validator_empty_file() {
        let validator = ImageValidator::new();
        let progress_bar = ProgressBar::hidden();
        
        // Create a temporary empty file
        use tempfile::NamedTempFile;
        
        let temp_file = NamedTempFile::new().expect("Failed to create temp file");
        
        let result = validator.validate(temp_file.path(), &progress_bar);
        assert!(result.is_ok());
        
        let (state, error_msg) = result.unwrap();
        assert_eq!(state, ValidationState::Invalid);
        assert!(error_msg.is_some());
    }
}
