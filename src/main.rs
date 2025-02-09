mod analytics;mod database;
mod dirscan;
mod error;
mod schema;

use clap::{ Parser, Subcommand };
use crate::analytics::Analytics;
use crate::error::DirCheckError;
use std::path::Path;
use crate::database::Database;
use crate::dirscan::DirScan;


#[derive(Parser)]
#[command(name = "dircheck", version = "0.1", about = "File system tree scanner")]
struct Args {
    #[command(subcommand)]
    command: DirCheckCommand,
}

#[derive(Subcommand)]
enum DirCheckCommand {
    /// Scan a directory and record changes
    Scan {
        /// Path to scan
        path: String,

        /// Database file directory (default: current directory)
        #[arg(long, default_value = ".")]
        dbpath: String,
    },

    /// Show changes from a scan
    Changes {
        /// Get changes from the latest scan (default if no scan ID is provided)
        #[arg(long, default_value = "true", conflicts_with = "scanid")]
        latest: bool,

        /// Get changes from a specific scan ID
        #[arg(long)]
        scanid: Option<u64>,

        /// Database file directory (default: current directory)
        #[arg(long, default_value = ".")]
        dbpath: String,
    },
}

fn main() {
    // Parse command-line arguments
    let args = Args::parse();

    if let Err(err) = handle_command(args) {
        eprintln!("Error: {}", err);
        std::process::exit(1);           
    }
}

fn handle_command(args: Args) -> Result<(), DirCheckError> {
    match args.command {
        DirCheckCommand::Scan { path, dbpath } => {
            scan_command(&path, &dbpath)?;
        }
        DirCheckCommand::Changes { latest, scanid, dbpath } => {
            if latest {
                changes_command(latest, scanid, &dbpath)?;
            } else if let Some(id) = scanid {
                println!("Showing changes for scan ID: {}", id);
            }
        }
    }

    Ok(())
}

fn scan_command(path: &str, dbpath: &str) -> Result<(), DirCheckError> {
    let mut db = Database::new(&dbpath)?;

    // Validate the path
    let path = Path::new(&path);

    DirScan::scan_directory(&mut db, &path)?;

    Ok(())
}

fn changes_command(latest: bool, scanid: Option<u64>, dbpath: &str) -> Result<(), DirCheckError> {
    let mut db = Database::new(&dbpath)?;
    if (latest) {
        Analytics::do_latest_changes(&mut db)?;
    }

    Ok(())
}