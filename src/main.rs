mod database;
mod error;

use clap::Parser;
use crate::error::DirCheckError;
use std::collections::VecDeque;
use std::fs;
use std::path::Path;
use std::path::PathBuf;
use crate::database::Database;

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
    /// Path to scan
    #[arg(short, long)]
    path: String,

    /// Directory where the SQLite database file will be stored (default: current directory)
    #[arg(long, default_value = ".")]
    dbpath: String,
}

fn main() {
    // Parse command-line arguments
    let args = Args::parse();

    let db = Database::connect(&args.dbpath).unwrap_or_else(|err| {
            eprintln!("Error: {}", err);
            std::process::exit(1);
    });

    // Validate the path
    let path = Path::new(&args.path);
    if let Err(err) = scan_directory(path) {
        eprintln!("Error: {}", err);
        std::process::exit(1);
    }
}

fn scan_directory(path: &Path) -> Result<(), DirCheckError> {

    let initial_metadata = fs::metadata(path)?;

    let mut q = VecDeque::new();

    q.push_back(QueueEntry {
        path: path.to_path_buf(),
        metadata: initial_metadata,
    });

    while let Some(q_entry) = q.pop_front() {
        println!("Directory: {}", q_entry.path.display());

        let entries = fs::read_dir(&q_entry.path)?;

        for entry in entries {
            let entry = entry?;

            let metadata = entry.metadata()?;

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