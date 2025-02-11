mod analytics;mod database;
mod scan;
mod error;
mod schema;
mod utils;

use clap::{ Parser, Subcommand };
use scan::Scan;
use crate::analytics::Analytics;
use crate::error::DirCheckError;
use crate::database::Database;
use crate::utils::Utils;

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
        /// Get changes from a specific scan ID (default: most recent scan)
        #[arg(long)]
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
        DirCheckCommand::Changes { scanid, verbose } => {
            // scanid is a u64 to prevent callers from passing negative numbers
            // Convert it to an i64 when passing here
            Analytics::do_changes(& mut db, Utils::opt_u64_to_opt_i64(scanid), verbose)?;
        }
        DirCheckCommand::Scans { all, count } => {
            Analytics::do_scans(&mut db, all, count)?;
        }
    }

    Ok(())
}
