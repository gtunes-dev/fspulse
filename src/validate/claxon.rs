use std::sync::atomic::{AtomicBool, Ordering};
use std::{path::Path, sync::Arc};

use claxon::{Block, FlacReader};

use crate::error::FsPulseError;

use super::validator::{ValidationState, Validator};

/// Validator implementation for FLAC audio files using the Claxon crate.
pub struct ClaxonValidator;

impl ClaxonValidator {
    /// Constructs a new ClaxonValidator instance.
    pub fn new() -> Self {
        ClaxonValidator
    }
}

impl Validator for ClaxonValidator {
    fn validate(
        &self,
        path: &Path,
        interrupt_token: &Arc<AtomicBool>,
    ) -> Result<(ValidationState, Option<String>), FsPulseError> {
        let mut reader = match FlacReader::open(path) {
            Ok(reader) => reader,
            Err(e) => return Ok((ValidationState::Invalid, Some(e.to_string()))),
        };

        let mut frame_reader = reader.blocks();
        let mut block = Block::empty();
        let mut block_count = 0i32;

        loop {
            match frame_reader.read_next_or_eof(block.into_buffer()) {
                Ok(Some(next_block)) => {
                    block = next_block;
                    block_count += 1;

                    // Check for interrupt every 256 blocks
                    if block_count % 256 == 0 && interrupt_token.load(Ordering::Acquire) {
                        return Err(FsPulseError::ScanInterrupted);
                    }
                }
                Ok(None) => break, // EOF.
                Err(e) => return Ok((ValidationState::Invalid, Some(e.to_string()))),
            }
        }

        Ok((ValidationState::Valid, None))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use std::sync::Arc;

    #[test]
    fn test_claxon_validator_nonexistent_file() {
        let validator = ClaxonValidator::new();
        let nonexistent_path = Path::new("/this/path/does/not/exist.flac");
        let interrupt_token = Arc::new(AtomicBool::new(false));

        let result = validator.validate(nonexistent_path, &interrupt_token);
        assert!(result.is_ok());

        let (state, error_msg) = result.unwrap();
        assert_eq!(state, ValidationState::Invalid);
        assert!(error_msg.is_some());
        let msg = error_msg.unwrap();
        assert!(
            msg.contains("No such file or directory")
                || msg.contains("cannot find the file")
                || msg.contains("system cannot find the file")
                || msg.contains("The system cannot find the file")
                || msg.contains("The system cannot find the path specified")
                || msg.contains("Access is denied")
                || msg.to_lowercase().contains("not found")
                || msg.to_lowercase().contains("no such file"),
            "Unexpected error message for nonexistent file: {msg}"
        );
    }

    #[test]
    fn test_claxon_validator_invalid_file() {
        let validator = ClaxonValidator::new();

        // Create a temporary file with invalid FLAC content
        use std::io::Write;
        use tempfile::NamedTempFile;

        let mut temp_file = NamedTempFile::new().expect("Failed to create temp file");
        temp_file
            .write_all(b"not a flac file")
            .expect("Failed to write temp file");

        let interrupt_token = Arc::new(AtomicBool::new(false));
        let result = validator.validate(temp_file.path(), &interrupt_token);
        assert!(result.is_ok());

        let (state, error_msg) = result.unwrap();
        assert_eq!(state, ValidationState::Invalid);
        assert!(error_msg.is_some());
    }
}
