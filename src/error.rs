use rusqlite::Error as RusqliteError;
use thiserror::Error;
use std::io;

#[derive(Error, Debug)]
pub enum FsPulseError {
    #[error("I/O error: {0}")]
    Io(#[from] io::Error),  // Converts io::Error into FsPulseError automatically

    #[error("Database error: {0}")]
    Database(#[from] RusqliteError), // Converts rusqlite::Error automatically

    #[error("Application error: {0}")]
    Error(String), // Allows custom application errors
}