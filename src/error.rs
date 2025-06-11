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
}
