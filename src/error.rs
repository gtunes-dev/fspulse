use rusqlite::Error as RusqliteError;
use thiserror::Error;
use std::io;

#[derive(Error, Debug)]
pub enum DirCheckError {
    #[error("I/O error: {0}")]
    Io(#[from] io::Error),  // Converts io::Error into DirCheckError automatically

    #[error("Database error: {0}")]
    Database(#[from] RusqliteError), // Converts rusqlite::Error automatically
}