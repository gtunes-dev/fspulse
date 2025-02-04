mod error;

use clap::Parser;
use crate::error::{DirCheckError, ErrorKind};
use std::fs;
use std::path::Path;

#[derive(Debug)]
enum FileType {
    File,
    Directory
}

// Command-line arguments
#[derive(Parser)]
#[command(name = "dircheck", version = "0.1", about = "File system tree scanner")]
struct Args {
    // Path to scan
    #[arg(short, long)]
    path: String,
}

fn main() {
    // Parse command-line arguments
    let args = Args::parse();

    // Validate the path
    let path = Path::new(&args.path);

    if let Err(err) = scan_directory(path) {
        eprintln!("Error: {}", err);
        std::process::exit(1);
    }
}

/// Scans a directory and prints its structure.
///
/// # Arguments
/// * `path` - A reference to the path of the directory to scan.
///
/// # Errors
/// Returns an error if the directory cannot be read or metadata cannot be retrieved.
fn scan_directory(path: &Path) -> Result<(), DirCheckError> {
    let entries = fs::read_dir(path)
        .map_err(|e| DirCheckError{ kind: ErrorKind::ReadDir, source: e})?;

    for entry in entries {
        let entry = entry
            .map_err(|e| DirCheckError {kind: ErrorKind::ReadDir, source: e})?;

        let metadata = entry.metadata()
            .map_err(|e| DirCheckError { kind: ErrorKind::Metadata, source: e })?;

        // Use the FileType enum instead of raw strings
        let file_type = if metadata.is_dir() {
            FileType::Directory
        } else {
            FileType::File
        };

        println!("{:?}: {}", file_type, entry.path().display());

        if metadata.is_dir() {
            scan_directory(&entry.path())?;
        }
    }

    Ok(())
}