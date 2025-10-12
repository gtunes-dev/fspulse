use rusqlite::Error as RusqliteError;
use std::io;
use thiserror::Error;

use crate::query::Rule;

#[derive(Error, Debug)]
pub enum FsPulseError {
    #[error("Error: {0}")]
    Error(String),

    #[error("I/O error: {0}")]
    IoError(#[from] io::Error), 

    #[error("Database error: {0}")]
    DatabaseError(#[from] RusqliteError),

    #[error("Query parsing error: {0}")]
    ParsingError(#[from] Box<pest::error::Error<Rule>>),

    #[error("Invalid value error: {0}")]
    InvalidValueError(#[from] strum::ParseError),

    #[error("Query parsing error: {0}")]
    CustomParsingError(String),

    #[error("Scan cancelled")]
    ScanCancelled,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io;

    #[test]
    fn test_error_display() {
        let error = FsPulseError::Error("test error".to_string());
        assert_eq!(error.to_string(), "Error: test error");
    }

    #[test]
    fn test_custom_parsing_error_display() {
        let error = FsPulseError::CustomParsingError("invalid syntax".to_string());
        assert_eq!(error.to_string(), "Query parsing error: invalid syntax");
    }

    #[test]
    fn test_io_error_conversion() {
        let io_error = io::Error::new(io::ErrorKind::NotFound, "file not found");
        let fs_error: FsPulseError = io_error.into();
        assert!(matches!(fs_error, FsPulseError::IoError(_)));
    }

    #[test]
    fn test_rusqlite_error_conversion() {
        let db_error = rusqlite::Error::SqliteFailure(
            rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_BUSY),
            Some("database is locked".to_string())
        );
        let fs_error: FsPulseError = db_error.into();
        assert!(matches!(fs_error, FsPulseError::DatabaseError(_)));
    }

    #[test]
    fn test_strum_parse_error_conversion() {
        // Create a parse error by trying to parse invalid enum value
        use strum::ParseError;
        let parse_error = ParseError::VariantNotFound;
        let fs_error: FsPulseError = parse_error.into();
        assert!(matches!(fs_error, FsPulseError::InvalidValueError(_)));
    }
}
