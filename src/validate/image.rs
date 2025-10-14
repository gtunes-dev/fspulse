use std::sync::atomic::{AtomicBool, Ordering};
use std::{path::Path, sync::Arc};

use image::ImageReader;

use crate::error::FsPulseError;
use crate::progress::{ProgressId, ProgressReporter};

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
        _prog_id: ProgressId,
        _reporter: &Arc<dyn ProgressReporter>,
        cancel_token: &Arc<AtomicBool>,
    ) -> Result<(ValidationState, Option<String>), FsPulseError> {

        // Check for cancellation before starting. Because of how this validator works,
        // this is the only test we can do
        if cancel_token.load(Ordering::Relaxed) {
            return Err(FsPulseError::ScanCancelled);
        }

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
    use crate::progress::{ProgressConfig, ProgressReporter, WorkUpdate};
    use std::path::Path;
    use std::sync::Arc;

    /// Simple mock reporter for testing
    struct MockReporter;

    impl ProgressReporter for MockReporter {
        fn section_start(&self, _stage_index: u32, _message: &str) -> ProgressId {
            ProgressId::new()
        }
        fn section_finish(&self, _id: ProgressId, _message: &str) {}
        fn create(&self, _config: ProgressConfig) -> ProgressId {
            ProgressId::new()
        }
        fn update_work(&self, _id: ProgressId, _work: WorkUpdate) {}
        fn set_position(&self, _id: ProgressId, _position: u64) {}
        fn set_length(&self, _id: ProgressId, _length: u64) {}
        fn inc(&self, _id: ProgressId, _delta: u64) {}
        fn enable_steady_tick(&self, _id: ProgressId, _interval: std::time::Duration) {}
        fn disable_steady_tick(&self, _id: ProgressId) {}
        fn finish_and_clear(&self, _id: ProgressId) {}
        fn println(&self, _message: &str) -> Result<(), Box<dyn std::error::Error>> {
            Ok(())
        }
        fn clone_reporter(&self) -> Arc<dyn ProgressReporter> {
            Arc::new(MockReporter)
        }
    }

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
        let reporter: Arc<dyn ProgressReporter> = Arc::new(MockReporter);
        let prog_id = ProgressId::new();
        let nonexistent_path = Path::new("/this/path/does/not/exist.jpg");
        let cancel_token = Arc::new(AtomicBool::new(false));

        let result = validator.validate(nonexistent_path, prog_id, &reporter, &cancel_token);
        assert!(result.is_ok());
        
        let (state, error_msg) = result.unwrap();
        assert_eq!(state, ValidationState::Invalid);
        assert!(error_msg.is_some());
        let msg = error_msg.unwrap();
        assert!(
            msg.contains("No such file or directory") || 
            msg.contains("cannot find the file") ||
            msg.contains("system cannot find the file") ||
            msg.contains("The system cannot find the file") ||
            msg.contains("The system cannot find the path specified") ||
            msg.contains("Access is denied") ||
            msg.to_lowercase().contains("not found") ||
            msg.to_lowercase().contains("no such file"),
            "Unexpected error message for nonexistent file: {msg}"
        );
    }

    #[test]
    fn test_image_validator_invalid_file() {
        let validator = ImageValidator::new();
        let reporter: Arc<dyn ProgressReporter> = Arc::new(MockReporter);
        let prog_id = ProgressId::new();

        // Create a temporary file with invalid image content
        use std::io::Write;
        use tempfile::NamedTempFile;

        let mut temp_file = NamedTempFile::new().expect("Failed to create temp file");
        temp_file.write_all(b"not an image file").expect("Failed to write temp file");

        let cancel_token = Arc::new(AtomicBool::new(false));
        let result = validator.validate(temp_file.path(), prog_id, &reporter, &cancel_token);
        assert!(result.is_ok());
        
        let (state, error_msg) = result.unwrap();
        assert_eq!(state, ValidationState::Invalid);
        assert!(error_msg.is_some());
    }

    #[test]
    fn test_image_validator_empty_file() {
        let validator = ImageValidator::new();
        let reporter: Arc<dyn ProgressReporter> = Arc::new(MockReporter);
        let prog_id = ProgressId::new();

        // Create a temporary empty file
        use tempfile::NamedTempFile;

        let temp_file = NamedTempFile::new().expect("Failed to create temp file");

        let cancel_token = Arc::new(AtomicBool::new(false));
        let result = validator.validate(temp_file.path(), prog_id, &reporter, &cancel_token);
        assert!(result.is_ok());
        
        let (state, error_msg) = result.unwrap();
        assert_eq!(state, ValidationState::Invalid);
        assert!(error_msg.is_some());
    }
}
