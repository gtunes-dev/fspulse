use clap::{CommandFactory, FromArgMatches, Parser, Subcommand};
use dialoguer::{theme::ColorfulTheme, BasicHistory, Input, Select};
use indicatif::MultiProgress;
use log::info;

use std::path::PathBuf;

use crate::database::Database;
use crate::error::FsPulseError;
use crate::explore::Explorer;
use crate::query::QueryProcessor;
use crate::reports::{ReportFormat, Reports};
use crate::roots::Root;
use crate::scanner::Scanner;
use crate::scans::AnalysisSpec;

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
    #[command(
        about = "Interactively select and run a command",
        long_about = r#"Launches interactive mode where you can choose from one of the main command types:
Scan, Report, or Query.

Once a command type is selected, you’ll be prompted to select from relevant existing items
such as roots, scans, items, or changes. The selected command will then be run using your choices."#
    )]
    Interact {
        #[arg(
            long,
            help = r#"Specifies the directory where the database is stored.
Defaults to the user's home directory (~/ on Unix, %USERPROFILE%\ on Windows).
The database file will always be named 'fspulse.db'."#
        )]
        db_path: Option<PathBuf>,
    },

    #[command(
        about = "Scan the filesystem and track changes",
        long_about = r#"Performs a scan on a specified root path or root-id.

If the root has been scanned before, it can be referenced by its root-id.
Alternatively, use a root path. If the specified path matches an existing
root, it will be reused; otherwise, a new root will be created.

By default, scans will compute hashes for all files and will 
'validate' files that have not been previously validated or have changed
since the last validation. . Hashes are computed using SHA2. Validation
is performed with type-specific logic using open source packages.
Files are considered changed if their file size or modification
date has changed since the last scan. Hashing behavior can be modified 
with --no-hash or --hash-new. Validation behavior can be modified 
with --no_validate or --validate-all."#
    )]
    Scan {
        #[arg(
            long,
            help = r#"Specifies the directory where the database is stored.
Defaults to the user's home directory (~/ on Unix, %USERPROFILE%\ on Windows).
The database file will always be named 'fspulse.db'."#
        )]
        db_path: Option<PathBuf>,

        #[arg(long, conflicts_with_all = ["root_path", "last"], help = "Scan a previously recorded root by numeric ID")]
        root_id: Option<u32>,

        #[arg(long, conflicts_with_all = ["root_id", "last"], help = "Scan a known or new root by path (must be a directory)")]
        root_path: Option<String>,

        #[arg(long, conflicts_with_all = ["root_id", "root_path"], help = "Scan the most recently scanned root")]
        last: bool,

        #[arg(
            long,
            help = r#"Disable hashing for all files"#
        )]
        no_hash: bool,

        #[arg(
            long,
            conflicts_with = "hash",
            help = "Hash only files whose file size or modification date has changed"
        )]
        hash_new: bool,

        #[arg(
            long,
            help = r#"Disable validation for all files"#
        )]
        no_validate: bool,

        #[arg(
            long,
            conflicts_with = "no_validate",
            help = "Validate all files, even if unchanged since last scan"
        )]
        validate_all: bool,
    },
    #[command(
        about = "Interactive data explorer",
        long_about = "Interactive, terminal-ui based data exploration of Roots, Scans, Items, and Changes"
    )]
    Explore {
        #[arg(
            long,
            help = r#"Specifies the directory where the database is stored.
Defaults to the user's home directory (~/ on Unix, %USERPROFILE%\ on Windows).
The database file will always be named 'fspulse.db'."#
        )]
        db_path: Option<PathBuf>,
    },
    #[command(
        about = "Generate reports",
        long_about = "Generate detailed reports about roots, scans, items, or changes stored in the FsPulse database."
    )]
    Report {
        #[command(subcommand)]
        report_type: ReportType,
    },

    #[command(
        about = "Execute a query and view results",
        long_about = r#"Execute a query using the FsPulse query language and return results in tabular form.

Example query: items where scan:(5) order by path limit 10"#
    )]
    Query {
        #[arg(
            long,
            help = r#"Specifies the directory where the database is stored.
Defaults to the user's home directory (~/ on Unix, %USERPROFILE%\ on Windows).
The database file will always be named 'fspulse.db'."#
        )]
        db_path: Option<PathBuf>,

        #[arg(help = "The query string (e.g., \"items where scan:(5)\")")]
        query: String,
    },
}

#[derive(Subcommand)]
pub enum ReportType {
    #[command(about = "Report on known root paths")]
    Roots {
        #[arg(long, help = r#"Specifies the directory where the database is stored.
Defaults to the user's home directory (~/ on Unix, %USERPROFILE%\ on Windows).
The database file will always be named 'fspulse.db'."#)]
        db_path: Option<PathBuf>,

        #[arg(long, conflicts_with = "root_path", help = "Show details for the root with the specified ID")]
        root_id: Option<u32>,

        #[arg(long, conflicts_with = "root_id", help = "Show details for the root with the specified path")]
        root_path: Option<String>,
    },

    #[command(about = "Report on scans")]
    Scans {
        #[arg(long, help = r#"Specifies the directory where the database is stored.
Defaults to the user's home directory (~/ on Unix, %USERPROFILE%\ on Windows).
The database file will always be named 'fspulse.db'."#)]
        db_path: Option<PathBuf>,

        #[arg(long, conflicts_with = "last", help = "Filter by specific scan ID")]
        scan_id: Option<u32>,

        #[arg(long, default_value_t = 10, conflicts_with = "scan_id", help = "Show last N scans (default: 10)")]
        last: u32,
    },

    #[command(about = "Report on scanned items")]
    Items {
        #[arg(long, help = r#"Specifies the directory where the database is stored.
Defaults to the user's home directory (~/ on Unix, %USERPROFILE%\ on Windows).
The database file will always be named 'fspulse.db'."#)]
        db_path: Option<PathBuf>,

        #[arg(long, conflicts_with_all = ["item_path", "root_id"], help = "Show a specific item by ID")]
        item_id: Option<u32>,

        #[arg(long, conflicts_with_all = ["item_id", "root_id"], help = "Show items by file path (may appear in multiple roots)")]
        item_path: Option<String>,

        #[arg(long, conflicts_with_all = ["item_id", "item_path"], help = "Show all items under the specific root")]
        root_id: Option<u32>,

        #[arg(long, requires = "root_id", help = "Show all invalid items under the specified root")]
        invalid: bool,

        #[arg(long, default_value = "table", value_parser = ["table", "tree"], help = "Report format (table or tree)")]
        format: String,
    },

    #[command(about = "Report on changes between scans")]
    Changes {
        #[arg(long, help = r#"Specifies the directory where the database is stored.
Defaults to the user's home directory (~/ on Unix, %USERPROFILE%\ on Windows).
The database file will always be named 'fspulse.db'."#)]
        db_path: Option<PathBuf>,

        #[arg(long, conflicts_with_all = ["item_id", "scan_id"], help = "Filter by change ID")]
        change_id: Option<u32>,

        #[arg(long, conflicts_with_all = ["change_id", "scan_id"], help = "Filter by item ID (shows all changes affecting the item)")]
        item_id: Option<u32>,

        #[arg(long, conflicts_with_all = ["change_id", "item_id"], help = "Filter by scan ID (shows all changes from this scan)")]
        scan_id: Option<u32>,
    },
}

#[derive(Copy, Clone)]
enum CommandChoice {
    Scan,
    QuerySimple,
    Explore,
    Report,
    Exit,
}

static COMMAND_CHOICES: &[(CommandChoice, &str)] = &[
    (CommandChoice::Scan, "Scan"),
    (CommandChoice::QuerySimple, "Query"),
    (CommandChoice::Explore, "Explore"),
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
        //let args = Cli::parse();
        let matches = Cli::command().term_width(0).get_matches();

        let args = Cli::from_arg_matches(&matches).unwrap();

        match args.command {
            Command::Interact { db_path } => {
                info!("Running interact with db_path: {db_path:?}");
                let mut db = Database::new(db_path)?;

                Cli::handle_interact(&mut db, multi_prog)
            }
            Command::Scan {
                db_path,
                root_id,
                root_path,
                last,
                no_hash,
                hash_new,
                no_validate,
                validate_all,
            } => {
                info!(
                    "Running scan with db_path: {db_path:?}, root_id: {root_id:?}, root_path: {root_path:?}, last: {last}, no_hash: {no_hash}, hash_new: {hash_new}, no_validate: {no_validate}, validate_all: {validate_all}"
                );

                let mut db = Database::new(db_path)?;
                let analysis_spec = AnalysisSpec::new(no_hash, hash_new, no_validate, validate_all);
                Scanner::do_scan_command(
                    &mut db,
                    root_id,
                    root_path,
                    last,
                    &analysis_spec,
                    multi_prog,
                )
            }
            Command::Explore { db_path } => {
                info!(
                    "Initiating interative explorer",
                );

                let db = Database::new(db_path)?;
                let mut explorer = Explorer::new();
                explorer.explore(&db)

            }
            Command::Report { report_type } => match report_type {
                ReportType::Roots {
                    db_path,
                    root_id,
                    root_path,
                } => {
                    info!(
                        "Generating roots report with db_path: {db_path:?}, root_id: {root_id:?}, root_path: {root_path:?}",
                    );
                    let db = Database::new(db_path)?;

                    Reports::report_roots(&db, root_id, root_path)
                }
                ReportType::Scans {
                    db_path,
                    scan_id,
                    last,
                } => {
                    info!(
                        "Generating scans report with db_path: {db_path:?}, scan_id: {scan_id:?}, last: {last}",
                    );
                    let db = Database::new(db_path)?;

                    Reports::report_scans(&db, scan_id, last)
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
                        "Generating items report with db_path: {db_path:?}, item_id: {item_id:?}, item_path: {item_path:?}, root_id: {root_id:?}, format: {format}"
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
                } => {
                    info!(
                        "Generating changes report with db_path: {db_path:?}, change_id: {change_id:?}, item_id: {item_id:?}, scan_id: {scan_id:?}",
                    );
                    let db = Database::new(db_path)?;

                    Reports::report_changes(&db, change_id, item_id, scan_id)
                }
            },
            Command::Query { db_path, query } => {
                info!("Processing query with db_path: {db_path:?}, '{query}'");
                let db = Database::new(db_path)?;
                QueryProcessor::execute_query_and_print(&db, &query)
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
            CommandChoice::QuerySimple => Cli::do_interactive_query(db, command)?,
            CommandChoice::Explore => Cli::do_interactive_explore(db)?,
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
                    let query: String = {
                        // Limit scope of dialoguer-related objects
                        Input::with_theme(&ColorfulTheme::default())
                            .history_with(&mut history)
                            .with_post_completion_text("Query: ")
                            .with_prompt("Enter query (or 'q' or 'exit'):")
                            .interact_text()
                            .unwrap()
                    };

                    let query_lower = query.to_lowercase();
                    if query_lower == "exit" || query_lower == "q" {
                        break;
                    }

                    // All interactive objects are dropped at this point
                    QueryProcessor::execute_query_and_print(db, &query)?;
                }
            }
            _ => unreachable!(),
        }

        Ok(())
    }

    fn do_interactive_explore(db: &Database) -> Result<(), FsPulseError> {
        let mut explorer = Explorer::new();
        explorer.explore(db)
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
                    let root_id_32: u32 = root.root_id().try_into().unwrap();
                    Reports::report_items(db, None, None, Some(root_id_32), true, ReportFormat::Table)
                } else {
                    Ok(())
                }
            }
            _ => Ok(()),
        }
    }
}
