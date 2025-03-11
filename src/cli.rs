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
    /// Perform a filesystem scan.
    Scan {
        /// Specifies the directory where the database is stored.
        /// Defaults to the user's home directory (`~/` on Unix, `%USERPROFILE%\` on Windows).
        /// The database file will always be named "fspulse.db".
        #[arg(long)]
        db_path: Option<PathBuf>,

        /// Scan using a stored path ID.
        #[arg(long, conflicts_with_all = ["path", "last"])]
        path_id: Option<u32>,

        /// Scan using a string path.
        #[arg(long, conflicts_with_all = ["path_id", "last"])]
        path: Option<String>,

        /// Scan the last-used path (default if no other path is provided).
        #[arg(long, conflicts_with_all = ["path_id", "path"])]
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
    /// Reports on scanned paths.
    Paths {
        /// Specifies the directory where the database is stored.
        /// Defaults to the user's home directory (`~/` on Unix, `%USERPROFILE%\` on Windows).
        /// The database file will always be named "fspulse.db".
        #[arg(long)]
        db_path: Option<PathBuf>,

        /// Filter by path ID.
        #[arg(long, conflicts_with = "path")]
        path_id: Option<u32>,

        /// Filter by specific path string.
        #[arg(long, conflicts_with = "path_id")]
        path: Option<String>,

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

        /// Filter by item ID.
        #[arg(long, conflicts_with = "path_id")]
        item_id: Option<u32>,

        /// Filter by path ID (shows items from last scan of this path).
        #[arg(long, conflicts_with = "item_id")]
        path_id: Option<u32>,

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
            Command::Scan { db_path, path_id, path, last, deep } => {
                info!(
                    "Running scan with db_path: {:?}, path_id: {:?}, path: {:?}, last: {}, deep: {}",
                    db_path, path_id, path, last, deep
                );
                Self::handle_scan(db_path, path_id, path, last, deep)?;
            }
            Command::Report { report_type } => match report_type {
                ReportType::Paths { db_path, path_id, path, format } => {
                    info!(
                        "Generating paths report with db_path: {:?}, path_id: {:?}, path: {:?}, format: {}",
                        db_path, path_id, path, format
                    );
                    Self::handle_report_paths(db_path, path_id, path, format)?;
                }
                ReportType::Scans { db_path, scan_id, last, format } => {
                    info!(
                        "Generating scans report with db_path: {:?}, scan_id: {:?}, last: {}, format: {}",
                        db_path, scan_id, last, format
                    );
                    Self::handle_report_scans(db_path, scan_id, last, format)?;
                }
                ReportType::Items { db_path, item_id, path_id, format } => {
                    info!(
                        "Generating items report with db_path: {:?}, item_id: {:?}, path_id: {:?}, format: {}",
                        db_path, item_id, path_id, format
                    );
                    Self::handle_report_items(db_path, item_id, path_id, format)?;
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
        path_id: Option<u32>,
        path: Option<String>,
        last: bool,
        deep: bool,
    ) -> Result<(), FsPulseError> {
        let mut db = Database::new(db_path)?;
        Scan::do_scan(&mut db, path_id, path, last, deep)?;

        Ok(())
    }

    /// Handler for `report paths`
    fn handle_report_paths(
        db_path: Option<PathBuf>,
        path_id: Option<u32>,
        path: Option<String>,
        format: String,
    ) -> Result<(), FsPulseError> {
        let db = Database::new(db_path)?;
        let format: ReportFormat = format.parse()?;
        
        Reports::report_root_paths(&db, path_id, path, format)?;
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
        path_id: Option<u32>,
        format: String,
    ) -> Result<(), FsPulseError> {
        let db = Database::new(db_path)?;
        let format: ReportFormat = format.parse()?;

        Reports::report_items(&db, item_id, path_id, format)?;
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