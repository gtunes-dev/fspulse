use clap::{Parser, Subcommand};
use dialoguer::{theme::ColorfulTheme, BasicHistory, Editor, Input, Select};
use indicatif::MultiProgress;
use log::info;

use std::path::PathBuf;

use crate::database::Database;
use crate::error::FsPulseError;
use crate::queries::Query;
use crate::reports::{ReportFormat, Reports};
use crate::roots::Root;
use crate::scanner::Scanner;

/// CLI for fspulse: A filesystem scan and reporting tool.
#[derive(Parser)]
#[command(
    name = "fspulse",
    version = "1.0",
    about = "Filesystem Pulse Scanner and Reporter"
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

/// Available commands in fspulse.
#[derive(Subcommand)]
pub enum Command {
    /// Interactively choose the command type (Scan or Report) and then choose from
    /// among existing items (roots, scans, items, changes) to initiate the
    /// command
    Interact {
        /// Specifies the directory where the database is stored.
        /// Defaults to the user's home directory (`~/` on Unix, `%USERPROFILE%\` on Windows).
        /// The database file will always be named "fspulse.db".
        #[arg(long)]
        db_path: Option<PathBuf>,
    },
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

        /// Hash files using md5 and compare to previous known hashes
        #[arg(long)]
        hash: bool,

        /// Validate file contents for known file types (tbd)
        #[arg(long)]
        validate: bool,
    },

    /// Generate reports.
    Report {
        #[command(subcommand)]
        report_type: ReportType,
    },
    Query {
        /// Specifies the directory where the database is stored.
        /// Defaults to the user's home directory (`~/` on Unix, `%USERPROFILE%\` on Windows).
        /// The database file will always be named "fspulse.db".
        #[arg(long)]
        db_path: Option<PathBuf>,

        /// The query string (e.g., "items where scan:(5)")
        query: String,
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
        #[arg(long, conflicts_with_all = ["item_path", "root_id"])]
        item_id: Option<u32>,

        /// Show all items with a specific path (an item may appear in multiple roots
        /// (in the case where one root is a subdirectory of another)
        #[arg(long, conflicts_with_all = ["item_id", "root_id"])]
        item_path: Option<String>,

        /// Shows the items seen on the most recent scan of the specified root
        #[arg(long, conflicts_with_all = ["item_id", "item_path"])]
        root_id: Option<u32>,

        /// Shows all invalid items under a specific root
        #[arg(long, requires = "root_id")]
        invalid: bool,

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

#[derive(Copy, Clone)]
enum CommandChoice {
    Scan,
    QuerySimple,
    QueryEditor,
    Report,
    Exit,
}

static COMMAND_CHOICES: &[(CommandChoice, &str)] = &[
    (CommandChoice::Scan, "Scan"),
    (CommandChoice::QuerySimple, "Query (Simple)"),
    (CommandChoice::QueryEditor, "Query (Editor)"),
    (CommandChoice::Report, "Report"),
    (CommandChoice::Exit, "Exit"),
];

#[derive(Copy, Clone)]
enum ReportChoice {
    Roots,
    Scans,
    Items,
    Changes,
    Exit,
}

static REPORT_CHOICES: &[(ReportChoice, &str)] = &[
    (ReportChoice::Roots, "Roots"),
    (ReportChoice::Scans, "Scans"),
    (ReportChoice::Items, "Items"),
    (ReportChoice::Changes, "Changes"),
    (ReportChoice::Exit, "Exit"),
];

#[derive(Copy, Clone)]
enum ItemReportChoice {
    InvalidItems,
    Exit,
}

static ITEM_REPORT_CHOICES: &[(ItemReportChoice, &str)] = &[
    (ItemReportChoice::InvalidItems, "Invalid Items"),
    (ItemReportChoice::Exit, "Exit"),
];

impl Cli {
    pub fn handle_command_line(multi_prog: &mut MultiProgress) -> Result<(), FsPulseError> {
        let args = Cli::parse();

        match args.command {
            Command::Interact { db_path } => {
                info!("Running interact with db_path: {:?}", db_path);
                let mut db = Database::new(db_path)?;

                Cli::handle_interact(&mut db, multi_prog)
            }
            Command::Scan {
                db_path,
                root_id,
                root_path,
                last,
                hash,
                validate,
            } => {
                info!(
                    "Running scan with db_path: {:?}, root_id: {:?}, root_path: {:?}, last: {}, hash: {}, validate: {}",
                    db_path, root_id, root_path, last, hash, validate
                );

                let mut db = Database::new(db_path)?;
                Scanner::do_scan_command(
                    &mut db, root_id, root_path, last, hash, validate, multi_prog,
                )
            }
            Command::Report { report_type } => match report_type {
                ReportType::Roots {
                    db_path,
                    root_id,
                    root_path,
                    format,
                } => {
                    info!(
                        "Generating roots report with db_path: {:?}, root_id: {:?}, root_path: {:?}, format: {}",
                        db_path, root_id, root_path, format
                    );
                    let db = Database::new(db_path)?;
                    let format: ReportFormat = format.parse()?;

                    Reports::report_roots(&db, root_id, root_path, format)
                }
                ReportType::Scans {
                    db_path,
                    scan_id,
                    last,
                    format,
                } => {
                    info!(
                        "Generating scans report with db_path: {:?}, scan_id: {:?}, last: {}, format: {}",
                        db_path, scan_id, last, format
                    );
                    let db = Database::new(db_path)?;
                    let format: ReportFormat = format.parse()?;

                    Reports::report_scans(&db, scan_id, last, format)
                }
                ReportType::Items {
                    db_path,
                    item_id,
                    item_path,
                    root_id,
                    invalid,
                    format,
                } => {
                    info!(
                        "Generating items report with db_path: {:?}, item_id: {:?}, item_path: {:?}, root_id: {:?}, format: {}",
                        db_path, item_id, item_path, root_id, format
                    );
                    let db = Database::new(db_path)?;
                    let format: ReportFormat = format.parse()?;

                    Reports::report_items(&db, item_id, item_path, root_id, invalid, format)
                }
                ReportType::Changes {
                    db_path,
                    change_id,
                    item_id,
                    scan_id,
                    format,
                } => {
                    info!(
                        "Generating changes report with db_path: {:?}, change_id: {:?}, item_id: {:?}, scan_id: {:?}, format: {}",
                        db_path, change_id, item_id, scan_id, format
                    );
                    let db = Database::new(db_path)?;
                    let format: ReportFormat = format.parse()?;

                    Reports::report_changes(&db, change_id, item_id, scan_id, format)
                }
            },
            Command::Query { db_path, query } => {
                info!("Processing query with db_path: {:?}, '{}'", db_path, query);
                let db = Database::new(db_path)?;
                Query::process_query(&db, &query)
            }
        }
    }

    fn handle_interact(
        db: &mut Database,
        multi_prog: &mut MultiProgress,
    ) -> Result<(), FsPulseError> {
        // Get the user's command choice.
        let command = Cli::choose_command();

        // Process the command.
        match command {
            CommandChoice::Scan => Scanner::do_interactive_scan(db, multi_prog)?,
            CommandChoice::QuerySimple | CommandChoice::QueryEditor => {
                Cli::do_interactive_query(db, command)?
            }
            CommandChoice::Report => Cli::do_interactive_report(db)?,
            CommandChoice::Exit => {}
        }
        Ok(())
    }

    fn choose_command() -> CommandChoice {
        // Build a vector of labels for display.
        let labels: Vec<&str> = COMMAND_CHOICES.iter().map(|&(_, label)| label).collect();

        let selection = Select::with_theme(&ColorfulTheme::default())
            .with_prompt("Choose Command")
            .default(0)
            .items(&labels)
            .interact()
            .unwrap();

        // Directly select the enum variant.
        COMMAND_CHOICES[selection].0
    }

    fn do_interactive_query(db: &Database, choice: CommandChoice) -> Result<(), FsPulseError> {
        match choice {
            CommandChoice::QuerySimple => {
                let mut history = BasicHistory::new().max_entries(8).no_duplicates(true);

                loop {
                    let query: String = Input::with_theme(&ColorfulTheme::default())
                        .history_with(&mut history)
                        .with_post_completion_text("Query: ")
                        .with_prompt("Enter query (or 'q' or 'exit'):")
                        .interact_text()
                        .unwrap();
                    let query_lower = query.to_lowercase();
                    if query_lower == "exit" || query_lower == "q" {
                        break;
                    }
                    Query::process_query(db, &query)?;
                }
            }
            CommandChoice::QueryEditor => {
                if let Some(query) = Editor::new().edit("Enter a query").unwrap() {
                    Query::process_query(db, &query)?;
                }
            }
            _ => unreachable!(),
        }

        Ok(())
    }

    fn do_interactive_report(db: &Database) -> Result<(), FsPulseError> {
        let labels: Vec<&str> = REPORT_CHOICES.iter().map(|&(_, label)| label).collect();

        let selection = Select::with_theme(&ColorfulTheme::default())
            .with_prompt("Choose Report Type")
            .default(0)
            .items(&labels)
            .interact()
            .unwrap();

        // Directly select the enum variant.
        match REPORT_CHOICES[selection].0 {
            ReportChoice::Roots => Ok(()),
            ReportChoice::Changes => Ok(()),
            ReportChoice::Items => Cli::do_interactive_report_items(db),
            _ => Ok(()),
        }
    }

    fn do_interactive_report_items(db: &Database) -> Result<(), FsPulseError> {
        let labels: Vec<&str> = ITEM_REPORT_CHOICES
            .iter()
            .map(|&(_, label)| label)
            .collect();

        let selection = Select::with_theme(&ColorfulTheme::default())
            .with_prompt("Choose Item Report Type")
            .default(0)
            .items(&labels)
            .interact()
            .unwrap();

        match ITEM_REPORT_CHOICES[selection].0 {
            ItemReportChoice::InvalidItems => {
                let root = Root::interact_choose_root(db, "Invalid items for which root?")?;
                if let Some(root) = root {
                    Reports::print_invalid_items_as_table(db, &root)
                } else {
                    Ok(())
                }
            }
            _ => Ok(()),
        }
    }
}
