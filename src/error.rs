use thiserror::Error;
use std::io;

/// Errors that can occur while scanning directories.
#[derive(Error, Debug)]
pub enum DirCheckError {
    #[error("Failed to read directory: {0}")]
    ReadDirError(#[from] io::Error),

    #[error("Failed to retrieve metadata: {0}")]
    MetadataError(#[from] io::Error),
}