use clap::{Parser, Subcommand};
use log::info;

use std::path::PathBuf;

use crate::database::Database;
use crate::error::FsPulseError; 
use crate::reports::{ReportFormat, Reports}; 
use crate::scans::Scan;
    
/// CLI for fspulse: A filesystem scan and reporting tool.
#[derive(Parser)]
#[command(name = "fspulse", version = "1.0", about = "Filesystem Pulse Scanner and Reporter")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

/// Available commands in fspulse.
#[derive(Subcommand)]
pub enum Command {
    /// Perform a filesystem scan on a specified "root". If the root has been scanned previously,
    /// it can be identified by its root-id. A root can also be identified by path by
    /// specifying a root-path. In the case of root-path, an existing root will be used if
    /// one exists and, if not, a new root will be created
    Scan {
        /// Specifies the directory where the database is stored.
        /// Defaults to the user's home directory (`~/` on Unix, `%USERPROFILE%\` on Windows).
        /// The database file will always be named "fspulse.db".
        #[arg(long)]
        db_path: Option<PathBuf>,

        /// Scan a known root by id
        #[arg(long, conflicts_with_all = ["root_path", "last"])]
        root_id: Option<u32>,

        /// Scan a known or new root by path (must be a directory)
        #[arg(long, conflicts_with_all = ["root_id", "last"])]
        root_path: Option<String>,

        /// Scan the root which was scanned most recently
        #[arg(long, conflicts_with_all = ["root_id", "root_path"])]
        last: bool,

        /// Perform a deep scan. Defaults to shallow if not provided.
        #[arg(long)]
        deep: bool,
    },

    /// Generate reports.
    Report {
        #[command(subcommand)]
        report_type: ReportType,
    },
}

/// Available report types.
#[derive(Subcommand)]
pub enum ReportType {
    /// Reports on "roots" which have been scanned in the past
    Roots {
        /// Specifies the directory where the database is stored.
        /// Defaults to the user's home directory (`~/` on Unix, `%USERPROFILE%\` on Windows).
        /// The database file will always be named "fspulse.db".
        #[arg(long)]
        db_path: Option<PathBuf>,

        /// Show details of the root with the specified id
        #[arg(long, conflicts_with = "root_path")]
        root_id: Option<u32>,

        /// Show details of the root with the specified path
        #[arg(long, conflicts_with = "root_id")]
        root_path: Option<String>,

        /// Report format (csv, table).
        #[arg(long, default_value = "table", value_parser = ["csv", "table"])]
        format: String,
    },

    /// Reports on scans.
    Scans {
        /// Specifies the directory where the database is stored.
        /// Defaults to the user's home directory (`~/` on Unix, `%USERPROFILE%\` on Windows).
        /// The database file will always be named "fspulse.db".
        #[arg(long)]
        db_path: Option<PathBuf>,

        /// Filter by scan ID.
        #[arg(long, conflicts_with = "last")]
        scan_id: Option<u32>,

        /// Show last N scans (default: 10)
        #[arg(long, default_value_t = 10, conflicts_with = "scan_id")]
        last: u32,

        /// Report format (csv, table).
        #[arg(long, default_value = "table", value_parser = ["csv", "table"])]
        format: String,
    },

    /// Reports on items.
    Items {
        /// Specifies the directory where the database is stored.
        /// Defaults to the user's home directory (`~/` on Unix, `%USERPROFILE%\` on Windows).
        /// The database file will always be named "fspulse.db".
        #[arg(long)]
        db_path: Option<PathBuf>,

        /// Show a specific Item
        #[arg(long, conflicts_with = "root_id")]
        item_id: Option<u32>,

        /// Shows the items seen on the most recent scan of the specified root
        #[arg(long, conflicts_with = "item_id")]
        root_id: Option<u32>,

        /// Report format (csv, table, tree).
        #[arg(long, default_value = "table", value_parser = ["csv", "table", "tree"])]
        format: String,
    },

    /// Reports on changes.
    Changes {
        /// Specifies the directory where the database is stored.
        /// Defaults to the user's home directory (`~/` on Unix, `%USERPROFILE%\` on Windows).
        /// The database file will always be named "fspulse.db".
        #[arg(long)]
        db_path: Option<PathBuf>,


        /// Filter by change ID.
        #[arg(long, conflicts_with_all = ["item_id", "scan_id"])]
        change_id: Option<u32>,

        /// Filter by item ID (shows all changes affecting the item).
        #[arg(long, conflicts_with_all = ["change_id", "scan_id"])]
        item_id: Option<u32>,

        /// Filter by scan ID (shows all changes recorded in this scan).
        #[arg(long, conflicts_with_all = ["change_id", "item_id"])]
        scan_id: Option<u32>,

        /// Report format (csv, table, tree - tree only valid with scan-id).
        #[arg(long, default_value = "table", value_parser = ["csv", "table", "tree"])]
        format: String,
    },
}

impl Cli {
    pub fn handle_command_line() -> Result<(), FsPulseError>{
        let args = Cli::parse();
        
        match args.command {
            Command::Scan { db_path, root_id, root_path, last, deep } => {
                info!(
                    "Running scan with db_path: {:?}, root_id: {:?}, root_path: {:?}, last: {}, deep: {}",
                    db_path, root_id, root_path, last, deep
                );
                Self::handle_scan(db_path, root_id, root_path, last, deep)?;
            }
            Command::Report { report_type } => match report_type {
                ReportType::Roots { db_path, root_id, root_path, format } => {
                    info!(
                        "Generating roots report with db_path: {:?}, root_id: {:?}, root_path: {:?}, format: {}",
                        db_path, root_id, root_path, format
                    );
                    Self::handle_report_roots(db_path, root_id, root_path, format)?;
                }
                ReportType::Scans { db_path, scan_id, last, format } => {
                    info!(
                        "Generating scans report with db_path: {:?}, scan_id: {:?}, last: {}, format: {}",
                        db_path, scan_id, last, format
                    );
                    Self::handle_report_scans(db_path, scan_id, last, format)?;
                }
                ReportType::Items { db_path, item_id, root_id, format } => {
                    info!(
                        "Generating items report with db_path: {:?}, item_id: {:?}, root_id: {:?}, format: {}",
                        db_path, item_id, root_id, format
                    );
                    Self::handle_report_items(db_path, item_id, root_id, format)?;
                }
                ReportType::Changes { db_path, change_id, item_id, scan_id, format } => {
                    info!(
                        "Generating changes report with db_path: {:?}, change_id: {:?}, item_id: {:?}, scan_id: {:?}, format: {}",
                        db_path, change_id, item_id, scan_id, format
                    );
                    Self::handle_report_changes(db_path, change_id, item_id, scan_id, format)?;
                }
            },
        }

        Ok(())
    }

    /// Handler for `pulse` command.
    fn handle_scan(
        db_path: Option<PathBuf>,
        root_id: Option<u32>,
        root_path: Option<String>,
        last: bool,
        deep: bool,
    ) -> Result<(), FsPulseError> {
        let mut db = Database::new(db_path)?;
        Scan::do_scan(&mut db, root_id, root_path, last, deep)?;

        Ok(())
    }

    /// Handler for `report paths`
    fn handle_report_roots(
        db_path: Option<PathBuf>,
        root_id: Option<u32>,
        root_path: Option<String>,
        format: String,
    ) -> Result<(), FsPulseError> {
        let db = Database::new(db_path)?;
        let format: ReportFormat = format.parse()?;
        
        Reports::report_roots(&db, root_id, root_path, format)?;
        Ok(())
    }

    /// Handler for `report scans`
    fn handle_report_scans(
        db_path: Option<PathBuf>,
        scan_id: Option<u32>,
        last: u32,
        format: String,
    ) -> Result<(), FsPulseError> {
        let db = Database::new(db_path)?;
        let format: ReportFormat = format.parse()?;

        Reports::report_scans(&db, scan_id, last, format)?;
        Ok(())
    }

    /// Handler for `report items`
    fn handle_report_items(
        db_path: Option<PathBuf>,
        item_id: Option<u32>,
        root_id: Option<u32>,
        format: String,
    ) -> Result<(), FsPulseError> {
        let db = Database::new(db_path)?;
        let format: ReportFormat = format.parse()?;

        Reports::report_items(&db, item_id, root_id, format)?;
        Ok(())
    }

    /// Handler for `report changes`
    fn handle_report_changes(
        db_path: Option<PathBuf>,
        change_id: Option<u32>,
        item_id: Option<u32>,
        scan_id: Option<u32>,
        format: String,
    ) -> Result<(), FsPulseError> {
        let db = Database::new(db_path)?;
        let format: ReportFormat = format.parse()?;

        Reports::report_changes(&db, change_id, item_id, scan_id, format)?;
        Ok(())
    }
}