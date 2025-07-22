use std::path::Path;

use indicatif::ProgressBar;
use log::warn;
use lopdf::{Document, Object};

use crate::error::FsPulseError;
use crate::try_invalid;
use crate::validate::validator::Validator;

use super::validator::ValidationState;


/// Validator implementation for pdf audio files using the lopdf crate.
pub struct LopdfValidator;

impl LopdfValidator {
    /// Constructs a new LopdfValidator instance.
    pub fn new() -> Self {
        LopdfValidator
    }
}

impl Validator for LopdfValidator {
    fn validate(
        &self,
        path: &Path,
        _validation_pb: &ProgressBar,
    ) -> Result<(ValidationState, Option<String>), FsPulseError> {
        let doc = try_invalid!(Document::load(path));
        // Traverse and validate all objects in the document.
        for object in doc.objects.values() {
            if let Err(e) = Self::validate_object(object) {
                return Ok((ValidationState::Invalid, Some(e.to_string())));
            }
        }
        Ok((ValidationState::Valid, None))
    }

    fn wants_steady_tick(&self) -> bool {
        true
    }
}

impl LopdfValidator {
    /// Recursively validates an individual PDF object.
    /// For stream objects, it attempts to decompress the content.
    /// For arrays and dictionaries, it recursively validates each nested object.
    /// Other object types are considered valid if they have been parsed.
    fn validate_object(object: &Object) -> Result<(), lopdf::Error> {
        match object {
            Object::Stream(stream) => {
                // Validate the stream by attempting to decompress its content.
                if stream.is_compressed() {
                    match stream.decompressed_content() {
                        Ok(_) => {}
                        Err(lopdf::Error::Unimplemented(reason)) => {
                            warn!("Lopdf unimplemented feature: {reason}");
                        }
                        Err(err) => {
                            return Err(err);
                        }
                    }
                }
            }
            Object::Array(arr) => {
                // Recursively validate all elements in the array.
                for item in arr {
                    Self::validate_object(item)?;
                }
            }
            Object::Dictionary(dict) => {
                // Recursively validate all values in the dictionary.
                for (_key, value) in dict.iter() {
                    Self::validate_object(value)?;
                }
            }
            // For primitive types (Null, Boolean, Number, String, Reference),
            // we assume they are valid if they've been parsed correctly.
            _ => {}
        }
        Ok(())
    }
}
