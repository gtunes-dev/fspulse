mod error;

use clap::Parser;
use crate::error::{DirCheckError, ErrorKind};
use std::fs::{self, DirEntry};
use std::path::Path;
use std::collections::VecDeque;
use std::path::PathBuf;

#[derive(Debug)]
enum FileType {
    File,
    Directory
}

#[derive(Clone, Debug)]
struct QueueEntry {
    path: PathBuf,
    metadata: fs::Metadata,
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

fn scan_directory(path: &Path) -> Result<(), DirCheckError> {
    let initial_metadata = fs::metadata(path)
        .map_err(|e| DirCheckError{ kind: ErrorKind::Metadata, source: e})?;

    let mut q = VecDeque::new();

    q.push_back(QueueEntry {
        path: path.to_path_buf(),
        metadata: initial_metadata,
    });

    while let Some(q_entry) = q.pop_front() {
        println!("Directory: {}", q_entry.path.display());

        let entries = fs::read_dir(&q_entry.path)
            .map_err(|e| DirCheckError{ kind: ErrorKind::ReadDir, source: e})?;

        for entry in entries {
            let entry = entry
                .map_err(|e| DirCheckError {kind: ErrorKind::ReadDir, source: e})?;

            let metadata = entry.metadata()
                .map_err(|e| DirCheckError { kind: ErrorKind::Metadata, source: e })?;

            if metadata.is_dir() {
                q.push_back(QueueEntry{
                    path: entry.path(),
                    metadata,
                });
            } else {
                println!("File: {}", entry.path().display());
            }
        }
    }
    Ok(())
}