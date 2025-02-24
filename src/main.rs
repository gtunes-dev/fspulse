mod database;
mod changes;
mod entries;
mod error;
mod hash;
mod indent;
mod scans;
mod reports;
mod root_paths;
mod schema;
mod utils;

use clap::{ Parser, Subcommand };
use scans::Scan;
use utils::Utils;
use crate::reports::Reports;
use crate::error::DirCheckError;
use crate::database::Database;

#[derive(Parser)]
#[command(name = "dircheck", version = "0.1", about = "File system tree scanner")]
struct Args {
    #[command(subcommand)]
    command: DirCheckCommand,
}

#[derive(Subcommand)]
enum DirCheckCommand {
    /// Scan a directory and record changes (default: current directory)
    Scan {
        /// Path to scan
        #[arg(long = "path", short = 'p', default_value = ".")]
        path: String,

        /// Perform a 'deep' scan computing a hash for each file
        #[arg(long = "deep", default_value_t = false)]
        deep: bool,
        
        /// Database file directory (default: current directory)
        #[arg(long = "dbpath", short = 'd', default_value = ".")]
        dbpath: String,
    },

    /// Generate reports from scans, entries, changes, or paths
    Report {
        #[command(subcommand)]
        report_type: ReportCommand,
    },
}

#[derive(Subcommand)]
enum ReportCommand {
    /// Report details about scans in the database
    Scans {
        /// Specify a scan ID to report on (default: most recent scan. conflicts with `latest` and `count`)
        #[arg(long = "id", short = 'i', conflicts_with = "count")]
        id: Option<u64>,

        /// Show the latest `N` scans (default: 10) (conflicts with `id`, 'count', and 'entries')
        #[arg(long = "count", short = 'c', conflicts_with = "id")]
        count: Option<u64>,

        /// Include changes in the scan report (conflicts with 'count' and `entries`)
        #[arg(long = "changes", conflicts_with_all = ["count", "entries"], default_value_t = false)]
        changes: bool,

        /// Include entries in the scan report. Only available for most recent scan (conflicts with 'id', 'count' and 'changes')
        #[arg(long = "entries", conflicts_with_all = ["id", "count", "changes"], default_value_t = false)]
        entries: bool,

        /// Database file directory (default: current directory)
        #[arg(long = "dbpath", short = 'd', default_value = ".")]
        dbpath: String,
    },

    /// Report details about root paths in the database
    #[command(name = "root-paths")]
    RootPaths {
        /// Specify a root path by ID (conflicts with `path`)
        #[arg(long = "id", short = 'i', conflicts_with = "path")]
        id: Option<u64>,

        /// Specify a root path by textual path (conflicts with `id`)
        #[arg(long = "path", short = 'p', conflicts_with = "id")]
        path: Option<String>,

        /// Include scans under this root path (only valid if `id` or `path` is provided)
        #[arg(long = "scans", short = 's', default_value_t = false)]
        scans: bool,

        /// Number of scans to include when using `--scans` (default: 10)
        #[arg(long = "count", short = 'c', requires = "scans")]
        count: Option<u64>,

        /// Database file directory (default: current directory)
        #[arg(long = "dbpath", short = 'd', default_value = ".")]
        dbpath: String,
    },

    /// Report details about file and folder entries in the database
    Entries {
        /// Specify an entry by ID (conflicts with `path`)
        #[arg(long = "id", short = 'i', conflicts_with = "path")]
        id: Option<u64>,

        /// Specify an entry by textual path (conflicts with `id`)
        #[arg(long = "path", short = 'p', conflicts_with = "id")]
        path: Option<String>,

        /// Show changes for the specified entry (only valid with `id`)
        #[arg(long = "changes", short = 'c', requires = "id")]
        changes: bool,

        /// Number of changes to display (default: 10)
        #[arg(long = "count", short = 'n', default_value_t = 10, requires = "changes")]
        count: u64,

        /// Database file directory (default: current directory)
        #[arg(long = "dbpath", short = 'd', default_value = ".")]
        dbpath: String,
    },
}

fn main() {
    // Parse command-line arguments
    
    //let temp_args: Vec<String> = std::env::args().collect();
    //println!("{:?}", temp_args);

    // Hi this is Lex!
    
    let args = Args::parse();

    if let Err(err) = handle_command(args) {
        eprintln!("Error: {}", err);
        std::process::exit(1);           
    }
}

fn handle_command(args: Args) -> Result<(), DirCheckError> {
    // Extract dbpath first from the top-level arguments or subcommands
    let dbpath = match &args.command {
        DirCheckCommand::Scan { dbpath, .. } => dbpath,
        DirCheckCommand::Report { report_type } => match report_type {
            ReportCommand::Scans { dbpath, .. } => dbpath,
            ReportCommand::RootPaths { dbpath, .. } => dbpath,
            ReportCommand::Entries { dbpath, .. } => dbpath,
            // ReportCommand::Changes { dbpath, .. } => dbpath,
        },
    };

    // Initialize the database
    let mut db = Database::new(dbpath)?;   
    
    match args.command {
        DirCheckCommand::Scan { path, deep, .. } => {
            Scan::do_scan(&mut db, path, deep)?;
        }
        DirCheckCommand::Report { report_type } => {
            match report_type {
                ReportCommand::Scans { id, count, changes, entries, .. } => {
                    let id = Utils::opt_u64_to_opt_i64(id);
                    Reports::do_report_scans(&mut db, id, count, changes, entries)?;
                }
                ReportCommand::Entries { id, path: _, changes, count: _, dbpath: _ } => {
                    //let id = Utils::opt_u64_to_opt_i64(id);
                    
                    if changes && id.is_none() {
                        return Err(DirCheckError::Error("Cannot use --changes without specifying an entry ID.".to_string()));
                    }
                }
                ReportCommand::RootPaths { id, path, scans, count, ..} => {
                    let id = Utils::opt_u64_to_opt_i64(id);
                    Reports::report_root_paths(&mut db, id, path, scans, count)?;
                }
            }
        }
    }

    Ok(())
}
