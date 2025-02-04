mod error;

use clap::Parser;
use error::DirCheckError;
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
    if !path.exists() {
        eprintln!("Error: Path '{}' does not exist.", args.path);
        std::process::exit(1);
    }
    if !path.is_dir() {
        eprintln!("Error: Path '{}' is not a directory.", args.path);
        std::process::exit(1);
    }

    println!("Scanning directory: {}", args.path);
    scan_directory(path);
}

fn scan_directory(path: &Path) {
    match fs::read_dir(path) {
        Ok(entries) => {
            for entry in entries {
                match entry {
                    Ok(entry) => {
                        let metadata = entry.metadata().unwrap();
                        let file_type = if metadata.is_dir() {
                            FileType::Directory
                        } else {
                            FileType::File
                        };
                        println!("{:?}: {}", file_type, entry.path().display());

                        if let FileType::Directory = file_type {
                            scan_directory(&entry.path());
                        }
                    }
                    Err(err) => eprintln!("Error reading entry: {}", err),
                }
            }
        }
        Err(err) => eprintln!("Error reading directory '{}': {}", path.display(), err),
    }
}