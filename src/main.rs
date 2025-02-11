mod analytics;mod database;
mod dirscan;
mod error;
mod schema;

use clap::{ Parser, Subcommand };
use dirscan::Scan;
use crate::analytics::Analytics;
use crate::error::DirCheckError;
use crate::database::Database;

#[derive(Parser)]
#[command(name = "dircheck", version = "0.1", about = "File system tree scanner")]
struct Args {
    /// Database file directory (default: current directory)
    #[arg(long, default_value = ".")]
    dbpath: String,

    #[command(subcommand)]
    command: DirCheckCommand,
}

#[derive(Subcommand)]
enum DirCheckCommand {
    /// Scan a directory and record changes
    Scan {
        /// Path to scan
        path: String,
    },

    /// Show changes from a scan
    Changes {
        /// Get changes from the latest scan (default if no scan ID is provided)
        #[arg(short = 'l', long, default_value_t = true, conflicts_with = "scanid")]
        latest: bool,

        /// Get changes from a specific scan ID
        #[arg(long, conflicts_with = "latest")]
        scanid: Option<u64>,

        /// Enable verbose output (list all changes)
        #[arg(short = 'v', long, default_value_t = false)]
        verbose: bool,
    },

    /// List scans previous scans including ids and root paths
    Scans {
        /// Display all scans (conflicts with `count`)
        #[arg(short = 'a', long, default_value_t = false, conflicts_with = "count")]
        all: bool,

        /// Number of scans to display (default: 10)
        #[arg(short = 'c', long, default_value_t = 10, conflicts_with = "all")]
        count: u64,
    }
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
    let mut db = Database::new(&args.dbpath)?;

    match args.command {
        DirCheckCommand::Scan { path } => {
            //DirScan::scan_directory(&mut db, Path::new(&path))?;
            Scan::do_scan(&mut db, path)?;
        }
        DirCheckCommand::Changes { latest: _, scanid, verbose } => {
            Analytics::do_changes(scanid, verbose, &mut db)?;
        }
        DirCheckCommand::Scans { all, count } => {
            Analytics::do_scans(&mut db, all, count)?;
        }
    }

    Ok(())
}
