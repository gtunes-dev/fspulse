mod database;
mod dirscan;
mod error;
mod schema;

use clap::Parser;
use crate::error::DirCheckError;
use std::path::Path;
use crate::database::Database;
use crate::dirscan::DirScan;

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
    let mut db = Database::connect(&args.dbpath)?;

    // Validate the path
    let path = Path::new(&args.path);
    //scan_directory(&mut db, &path)?;

    DirScan::scan_directory(&mut db, &path)?;

    Ok(())
}