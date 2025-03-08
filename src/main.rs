mod database;
mod changes;
mod error;
mod hash;
//mod indent;
mod items;
mod reports;
mod root_paths;
mod scans;
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

    /// Generate reports from scans, items, changes, or paths
    Report {
        #[command(subcommand)]
        report_type: ReportCommand,
    },
}

#[derive(Subcommand)]
enum ReportCommand {
    /// Report details about scans in the database
    Scans {
        /// Specify a scan ID to report on (may be combined with "--changes and "items", conflicts with "count")
        #[arg(long = "id", short = 'i', conflicts_with = "count")]
        id: Option<u64>,

        /// Report on the latest scan (may be combined with "--changes" and "--items", conflicts with "id" and "count")
        #[arg(long = "latest", short = 'l', conflicts_with_all = ["id", "count"], default_value_t = false)]
        latest: bool,
        
        /// Show the latest `N` scans (default: 10) (conflicts with "id" and "latest")
        #[arg(long = "count", short = 'c', conflicts_with = "id")]
        count: Option<u64>,

        /// Include changes in the scan report (conflicts with 'count')
        #[arg(long = "changes", conflicts_with_all = ["count"], default_value_t = false, group = "output")]
        changes: bool,

        /// Include items in the scan report. Only available for most recent scan (requires 'latest'. conflicts with 'id' and 'count')
        #[arg(long = "items", requires = "latest", conflicts_with_all = ["id", "count"], default_value_t = false, group = "output")]
        items: bool,

        /// Output format (only applicable with `--items` or `--changes`, defaults to "table")
        #[arg(
            long = "format",
            value_parser = clap::builder::PossibleValuesParser::new(["tree", "table"]),
            default_value = "table",
            requires = "output"
    )]
    format: String,

        /// Database file directory (default: current directory)
        #[arg(long = "dbpath", short = 'd', default_value = ".")]
        dbpath: String,
    },

    /// Report details about all root paths in the database
    #[command(name = "root-paths")]
    RootPaths {
        /// Specify a Root Path ID to report on (may be combined with "items")
        #[arg(long = "id", short = 'i')]
        id: Option<u64>,

        /// Specify a Root Path ID to report on (may be combined with "items", conflicts with "id")
        #[arg(long = "items", requires = "id")]
        items: bool,

        /// Database file directory (default: current directory)
        #[arg(long = "dbpath", short = 'd', default_value = ".")]
        dbpath: String,
    },

    /// Report details about file and folder items in the database
    Items {
        /// Specify an item by ID (conflicts with `path`)
        #[arg(long = "id", short = 'i', conflicts_with = "path")]
        id: Option<u64>,

        /// Specify an item by textual path (conflicts with `id`)
        #[arg(long = "path", short = 'p', conflicts_with = "id")]
        path: Option<String>,

        /// Show changes for the specified item (only valid with `id`)
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
            ReportCommand::Items { dbpath, .. } => dbpath,
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
                ReportCommand::Scans { id, latest, count, changes, items, format, .. } => {
                    let id = Utils::opt_u64_to_opt_i64(id);
                    let count = Utils::opt_u64_to_opt_i64(count);
                    Reports::report_scans(&mut db, id, latest, count, changes, items, &format)?;
                }
                ReportCommand::RootPaths { id, items, .. } => {
                    let id = Utils::opt_u64_to_opt_i64(id);
                    Reports::report_root_paths(&mut db, id, items)?;
                }
                ReportCommand::Items { id, path: _, changes: _, count: _, dbpath: _ } => {
                    let id = Utils::opt_u64_to_opt_i64(id).unwrap();
                    Reports::report_items(&db, id)?;                    
                }
            }
        }
    }

    Ok(())
}
