use std::sync::atomic::{AtomicBool, Ordering};
use std::{path::Path, sync::Arc};

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
        cancel_token: &Arc<AtomicBool>,
    ) -> Result<(ValidationState, Option<String>), FsPulseError> {
        let doc = try_invalid!(Document::load(path));

        // Traverse and validate all objects in the document.
        let mut object_count = 0;

        for object in doc.objects.values() {
            object_count += 1;
            if object_count % 256 == 0 && cancel_token.load(Ordering::Acquire) {
                return Err(FsPulseError::ScanCancelled);
            }

            if let Err(e) = Self::validate_object(object) {
                return Ok((ValidationState::Invalid, Some(e.to_string())));
            }
        }
        Ok((ValidationState::Valid, None))
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use std::sync::Arc;

    #[test]
    fn test_lopdf_validator_nonexistent_file() {
        let validator = LopdfValidator::new();
        let nonexistent_path = Path::new("/this/path/does/not/exist.pdf");
        let cancel_token = Arc::new(AtomicBool::new(false));

        let result = validator.validate(nonexistent_path, &cancel_token);
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
    fn test_lopdf_validator_invalid_file() {
        let validator = LopdfValidator::new();

        // Create a temporary file with invalid PDF content
        use std::io::Write;
        use tempfile::NamedTempFile;

        let mut temp_file = NamedTempFile::new().expect("Failed to create temp file");
        temp_file.write_all(b"not a pdf file").expect("Failed to write temp file");

        let cancel_token = Arc::new(AtomicBool::new(false));
        let result = validator.validate(temp_file.path(), &cancel_token);
        assert!(result.is_ok());
        
        let (state, error_msg) = result.unwrap();
        assert_eq!(state, ValidationState::Invalid);
        assert!(error_msg.is_some());
    }

    #[test]
    fn test_lopdf_validator_empty_file() {
        let validator = LopdfValidator::new();

        // Create a temporary empty file
        use tempfile::NamedTempFile;

        let temp_file = NamedTempFile::new().expect("Failed to create temp file");

        let cancel_token = Arc::new(AtomicBool::new(false));
        let result = validator.validate(temp_file.path(), &cancel_token);
        assert!(result.is_ok());
        
        let (state, error_msg) = result.unwrap();
        assert_eq!(state, ValidationState::Invalid);
        assert!(error_msg.is_some());
    }

    #[test]
    fn test_validate_object_primitive_types() {
        use lopdf::Object;
        
        // Test primitive object types that should always be valid
        assert!(LopdfValidator::validate_object(&Object::Null).is_ok());
        assert!(LopdfValidator::validate_object(&Object::Boolean(true)).is_ok());
        assert!(LopdfValidator::validate_object(&Object::Boolean(false)).is_ok());
        assert!(LopdfValidator::validate_object(&Object::Integer(42)).is_ok());
        assert!(LopdfValidator::validate_object(&Object::Real(42.5)).is_ok());
        assert!(LopdfValidator::validate_object(&Object::String(b"test".to_vec(), lopdf::StringFormat::Literal)).is_ok());
    }

    #[test]
    fn test_validate_object_array() {
        use lopdf::Object;
        
        // Test valid array
        let valid_array = Object::Array(vec![
            Object::Integer(1),
            Object::String(b"test".to_vec(), lopdf::StringFormat::Literal),
            Object::Boolean(true),
        ]);
        assert!(LopdfValidator::validate_object(&valid_array).is_ok());
        
        // Test empty array
        let empty_array = Object::Array(vec![]);
        assert!(LopdfValidator::validate_object(&empty_array).is_ok());
    }

    #[test]
    fn test_validate_object_dictionary() {
        use lopdf::{Object, Dictionary};
        
        // Test valid dictionary
        let mut dict = Dictionary::new();
        dict.set("key1", Object::Integer(42));
        dict.set("key2", Object::String(b"value".to_vec(), lopdf::StringFormat::Literal));
        
        let dict_object = Object::Dictionary(dict);
        assert!(LopdfValidator::validate_object(&dict_object).is_ok());
        
        // Test empty dictionary
        let empty_dict = Object::Dictionary(Dictionary::new());
        assert!(LopdfValidator::validate_object(&empty_dict).is_ok());
    }
}
