use rusqlite::Error as RusqliteError;
use std::io;
use thiserror::Error;

use crate::queries::Rule;

#[derive(Error, Debug)]
pub enum FsPulseError {
    #[error("I/O error: {0}")]
    IoError(#[from] io::Error), // Converts io::Error into FsPulseError automatically

    #[error("Database error: {0}")]
    DatabaseError(#[from] RusqliteError), // Converts rusqlite::Error automatically

    #[error("Error: {0}")]
    Error(String), // Allows custom application errors

    #[error("Query parsing error: {0}")]
    ParsingError(#[from] Box<pest::error::Error<Rule>>),
}
