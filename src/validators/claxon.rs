use std::path::Path;

use indicatif::ProgressBar;

use claxon::{Block, FlacReader};

use crate::error::FsPulseError;
use crate::validators::Validator;

use super::validator::ValidationState;

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
    ) -> Result<(ValidationState, Option<String>), FsPulseError> 
    {
        let mut reader =  match FlacReader::open(path) {
            Ok(reader) => reader,
            Err(e) => {
                return Ok((ValidationState::Invalid, Some(e.to_string())))
            }
        };

        let mut frame_reader = reader.blocks();
        let mut block = Block::empty();

        let mut tick_blocks = 0;

        loop {
            match frame_reader.read_next_or_eof(block.into_buffer()) {
                Ok(Some(next_block)) => block = next_block,
                Ok(None) => break, // EOF.
                Err(e) => {
                    return Ok((ValidationState::Invalid, Some(e.to_string())))
                },
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