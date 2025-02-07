mod database;
mod error;
mod schema;

use clap::Arg;
use clap::Parser;
use crate::error::DirCheckError;
use std::collections::VecDeque;
use std::env;
use std::fs;
use std::fs::File;
use std::fs::FileTimes;
use std::fs::FileType;
use std::iter;
use std::path::Path;
use std::path::PathBuf;
use crate::database::{ Database, ItemType };

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

    // Check the user-specified directory
    if let Err(err) = do_check_directory(args) {
        eprintln!("Error: {}", err);
        std::process::exit(1);        
    }
}

fn do_check_directory(args: Args) -> Result<(), DirCheckError> {
    let db = Database::connect(&args.dbpath)?;

    // Validate the path
    let path = Path::new(&args.path);
    scan_directory(&db, &path)?;

    Ok(())
}

fn validate_and_resolve_path(user_path: &Path) -> Result<PathBuf, DirCheckError> {
    if user_path.as_os_str().is_empty() {
        return Err(DirCheckError::Error("Provided path is empty".to_string()));
    }

    let absolute_path = if user_path.is_absolute() {
        user_path.to_owned()
    }  else {
        env::current_dir()?.join(user_path)
    };
    
    if !absolute_path.exists() {
        return Err(DirCheckError::Error(format!("Path '{}' does not exist", absolute_path.display())));
    }

    let metadata = fs::symlink_metadata(&absolute_path)?;
    if metadata.file_type().is_symlink() {
        return Err(DirCheckError::Error(format!("Path '{}' is a symlink and not allowed", absolute_path.display())));
    }
    
    if !metadata.is_dir() {
        return Err(DirCheckError::Error(format!("Path '{}' is not a directory", absolute_path.display())));
    }

    Ok(absolute_path)
} 

fn scan_directory(db: &Database, path: &Path) -> Result<(), DirCheckError> {
    let absolute_path = validate_and_resolve_path(&path)?;

    let metadata = fs::symlink_metadata(&absolute_path)?;

    let mut q = VecDeque::new();

    q.push_back(QueueEntry {
        path: absolute_path.to_path_buf(),
        metadata: metadata,
    });

    while let Some(q_entry) = q.pop_front() {
        println!("Directory: {}", q_entry.path.display());

        // Update the database
        Database::handle_item(&db, ItemType::Directory, q_entry.path.as_path(), &q_entry.metadata)?;

        let entries = fs::read_dir(&q_entry.path)?;

        for entry in entries {
            let entry = entry?;
            let metadata = fs::symlink_metadata(entry.path())?; // Use symlink_metadata to check for symlinks

            if metadata.is_dir() {
                q.push_back(QueueEntry {
                    path: entry.path(),
                    metadata,
                });
            } else {
                let item_type = if metadata.is_file() {
                    ItemType::File
                } else if metadata.is_symlink() {
                    ItemType::Symlink
                } else {
                    ItemType::Other
                };

                println!("{:?}: {}", item_type, entry.path().display());
                
                // Update the database
                Database::handle_item(&db, item_type, &entry.path(), &metadata)?;
            }
        }
    }
    Ok(())
}