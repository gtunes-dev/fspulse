use thiserror::Error;
use std::io;

/// Categorization of different kinds of directory scanning errors.
#[derive(Debug)]
pub enum ErrorKind {
    ReadDir,
    Metadata,
}

/// Errors that can occur while scanning directories.
#[derive(Error, Debug)]
#[error("{kind:?} error: {source}")]
pub struct DirCheckError {
    pub kind: ErrorKind,
    #[source]  
    pub source: io::Error,
}