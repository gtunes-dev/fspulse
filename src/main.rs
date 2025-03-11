mod database;
mod changes;
mod cli;
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
use cli::Cli;
use log::{debug, error, info};
use reports::ReportFormat;
use scans::Scan;
use utils::Utils;
use crate::reports::Reports;
use crate::error::FsPulseError;
use crate::database::Database;

#[derive(Parser)]
#[command(name = "fspulse", version = "0.1", about = "File system tree scanner")]
struct Args {
    #[command(subcommand)]
    command: FsPulseCommand,
}

#[derive(Subcommand)]
enum FsPulseCommand {
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
    // Must set an environment variable to use.
    // Set RUST_LOG to one of:
    // ERROR → WARN → INFO → DEBUG → TRACE
    env_logger::init();
    debug!("Command-line args: {:?}", std::env::args_os().collect::<Vec<_>>());

    if let Err(err) = Cli::handle_command_line() {
        error!("{:?}", err);
        eprint!("{}", err);
        std::process::exit(1);
    }
}