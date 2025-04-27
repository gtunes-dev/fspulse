use std::path::Path;

use indicatif::ProgressBar;

use image::ImageReader;

use crate::error::FsPulseError;
use crate::validators::Validator;

use super::validator::ValidationState;

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
