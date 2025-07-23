use std::path::Path;

use indicatif::ProgressBar;

use claxon::{Block, FlacReader};

use crate::error::FsPulseError;

use super::validator::{ValidationState, Validator};


/// Validator implementation for FLAC audio files using the Claxon crate.
pub struct ClaxonValidator;

impl ClaxonValidator {
    const BLOCKS_PER_TICK: i32 = 500;

    /// Constructs a new ClaxonValidator instance.
    pub fn new() -> Self {
        ClaxonValidator
    }
}

impl Validator for ClaxonValidator {
    fn validate(
        &self,
        path: &Path,
        validation_pb: &ProgressBar,
    ) -> Result<(ValidationState, Option<String>), FsPulseError> {
        let mut reader = match FlacReader::open(path) {
            Ok(reader) => reader,
            Err(e) => return Ok((ValidationState::Invalid, Some(e.to_string()))),
        };

        let mut frame_reader = reader.blocks();
        let mut block = Block::empty();

        let mut tick_blocks = 0;

        loop {
            match frame_reader.read_next_or_eof(block.into_buffer()) {
                Ok(Some(next_block)) => block = next_block,
                Ok(None) => break, // EOF.
                Err(e) => return Ok((ValidationState::Invalid, Some(e.to_string()))),
            }
            tick_blocks += 1;
            if tick_blocks == Self::BLOCKS_PER_TICK {
                validation_pb.tick();
                tick_blocks = 0;
            }
        }

        Ok((ValidationState::Valid, None))
    }

    fn wants_steady_tick(&self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use indicatif::ProgressBar;
    use std::path::Path;

    #[test]
    fn test_claxon_validator_new() {
        let validator = ClaxonValidator::new();
        assert!(!validator.wants_steady_tick());
    }

    #[test]
    fn test_claxon_validator_wants_steady_tick() {
        let validator = ClaxonValidator::new();
        assert!(!validator.wants_steady_tick());
    }

    #[test]
    fn test_claxon_validator_blocks_per_tick_constant() {
        assert_eq!(ClaxonValidator::BLOCKS_PER_TICK, 500);
    }

    #[test]
    fn test_claxon_validator_nonexistent_file() {
        let validator = ClaxonValidator::new();
        let progress_bar = ProgressBar::hidden();
        let nonexistent_path = Path::new("/this/path/does/not/exist.flac");
        
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
    fn test_claxon_validator_invalid_file() {
        let validator = ClaxonValidator::new();
        let progress_bar = ProgressBar::hidden();
        
        // Create a temporary file with invalid FLAC content
        use std::io::Write;
        use tempfile::NamedTempFile;
        
        let mut temp_file = NamedTempFile::new().expect("Failed to create temp file");
        temp_file.write_all(b"not a flac file").expect("Failed to write temp file");
        
        let result = validator.validate(temp_file.path(), &progress_bar);
        assert!(result.is_ok());
        
        let (state, error_msg) = result.unwrap();
        assert_eq!(state, ValidationState::Invalid);
        assert!(error_msg.is_some());
    }
}
