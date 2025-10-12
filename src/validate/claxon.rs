use std::{path::Path, sync::Arc};

use claxon::{Block, FlacReader};

use crate::error::FsPulseError;
use crate::progress::{ProgressId, ProgressReporter};

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
        prog_id: ProgressId,
        reporter: &Arc<dyn ProgressReporter>,
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

                    // Update progress every BLOCKS_PER_TICK blocks
                    if block_count % Self::BLOCKS_PER_TICK == 0 {
                        reporter.inc(prog_id, Self::BLOCKS_PER_TICK as u64);
                    }
                }
                Ok(None) => break, // EOF.
                Err(e) => return Ok((ValidationState::Invalid, Some(e.to_string()))),
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
        fn set_message(&self, _id: ProgressId, _message: String) {}
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
        let reporter: Arc<dyn ProgressReporter> = Arc::new(MockReporter);
        let prog_id = ProgressId::new();
        let nonexistent_path = Path::new("/this/path/does/not/exist.flac");

        let result = validator.validate(nonexistent_path, prog_id, &reporter);
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
        let reporter: Arc<dyn ProgressReporter> = Arc::new(MockReporter);
        let prog_id = ProgressId::new();

        // Create a temporary file with invalid FLAC content
        use std::io::Write;
        use tempfile::NamedTempFile;

        let mut temp_file = NamedTempFile::new().expect("Failed to create temp file");
        temp_file
            .write_all(b"not a flac file")
            .expect("Failed to write temp file");

        let result = validator.validate(temp_file.path(), prog_id, &reporter);
        assert!(result.is_ok());

        let (state, error_msg) = result.unwrap();
        assert_eq!(state, ValidationState::Invalid);
        assert!(error_msg.is_some());
    }
}
