use clap::{CommandFactory, FromArgMatches, Parser, Subcommand};
use dialoguer::{theme::ColorfulTheme, BasicHistory, Input, Select};
use indicatif::MultiProgress;
use log::info;

use std::sync::Arc;

use crate::config::CONFIG;
use crate::database::Database;
use crate::error::FsPulseError;
use crate::explore::Explorer;
use crate::progress::cli::CliProgressReporter;
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

Once a command type is selected, you'll be prompted to select from relevant existing items
such as roots, scans, items, or changes. The selected command will then be run using your choices."#
    )]
    Interact,

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
            conflicts_with = "no_hash",
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
    Explore,
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
        #[arg(help = "The query string (e.g., \"items where scan:(5)\")")]
        query: String,
    },

    #[command(
        about = "Start the server",
        long_about = "Start the FsPulse server to run as a background service with browser-based access to filesystem scanning and reporting."
    )]
    Serve,
}

#[derive(Subcommand)]
pub enum ReportType {
    #[command(about = "Report on known root paths")]
    Roots {
        #[arg(long, conflicts_with = "root_path", help = "Show details for the root with the specified ID")]
        root_id: Option<u32>,

        #[arg(long, conflicts_with = "root_id", help = "Show details for the root with the specified path")]
        root_path: Option<String>,
    },

    #[command(about = "Report on scans")]
    Scans {
        #[arg(long, conflicts_with = "last", help = "Filter by specific scan ID")]
        scan_id: Option<u32>,

        #[arg(long, default_value_t = 10, conflicts_with = "scan_id", help = "Show last N scans (default: 10)")]
        last: u32,
    },

    #[command(about = "Report on scanned items")]
    Items {
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
            Command::Interact => {
                info!("Running interact");
                let mut db = Database::new()?;

                Cli::handle_interact(&mut db, multi_prog)
            }
            Command::Scan {
                root_id,
                root_path,
                last,
                no_hash,
                hash_new,
                no_validate,
                validate_all,
            } => {
                info!(
                    "Running scan with root_id: {root_id:?}, root_path: {root_path:?}, last: {last}, no_hash: {no_hash}, hash_new: {hash_new}, no_validate: {no_validate}, validate_all: {validate_all}"
                );

                let mut db = Database::new()?;
                let analysis_spec = AnalysisSpec::new(no_hash, hash_new, no_validate, validate_all);

                // Create CliProgressReporter from MultiProgress
                let reporter = Arc::new(CliProgressReporter::new(std::mem::take(multi_prog)));

                Scanner::do_scan_command(
                    &mut db,
                    root_id,
                    root_path,
                    last,
                    &analysis_spec,
                    reporter,
                )
            }
            Command::Explore => {
                info!("Initiating interactive explorer");

                let db = Database::new()?;
                let mut explorer = Explorer::new();
                explorer.explore(&db)

            }
            Command::Report { report_type } => match report_type {
                ReportType::Roots {
                    root_id,
                    root_path,
                } => {
                    info!(
                        "Generating roots report with root_id: {root_id:?}, root_path: {root_path:?}",
                    );
                    let db = Database::new()?;

                    Reports::report_roots(&db, root_id, root_path)
                }
                ReportType::Scans {
                    scan_id,
                    last,
                } => {
                    info!(
                        "Generating scans report with scan_id: {scan_id:?}, last: {last}",
                    );
                    let db = Database::new()?;

                    Reports::report_scans(&db, scan_id, last)
                }
                ReportType::Items {
                    item_id,
                    item_path,
                    root_id,
                    invalid,
                    format,
                } => {
                    info!(
                        "Generating items report with item_id: {item_id:?}, item_path: {item_path:?}, root_id: {root_id:?}, format: {format}"
                    );
                    let db = Database::new()?;
                    let format: ReportFormat = format.parse()?;

                    Reports::report_items(&db, item_id, item_path, root_id, invalid, format)
                }
                ReportType::Changes {
                    change_id,
                    item_id,
                    scan_id,
                } => {
                    info!(
                        "Generating changes report with change_id: {change_id:?}, item_id: {item_id:?}, scan_id: {scan_id:?}",
                    );
                    let db = Database::new()?;

                    Reports::report_changes(&db, change_id, item_id, scan_id)
                }
            },
            Command::Query { query } => {
                info!("Processing query: '{query}'");
                let db = Database::new()?;
                QueryProcessor::execute_query_and_print(&db, &query)
            }
            Command::Serve => {
                let config = CONFIG.get().expect("Config not initialized");

                info!("Starting server on {}:{}", config.server.host, config.server.port);

                // Start the async runtime for the web server
                let rt = tokio::runtime::Runtime::new()
                    .map_err(|e| FsPulseError::Error(format!("Failed to create runtime: {}", e)))?;

                rt.block_on(async {
                    let web_server = crate::web::WebServer::new(config.server.host.clone(), config.server.port);
                    web_server.start().await
                })
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
            CommandChoice::Scan => {
                // Create CliProgressReporter from MultiProgress
                let reporter = Arc::new(CliProgressReporter::new(std::mem::take(multi_prog)));
                Scanner::do_interactive_scan(db, reporter)?
            }
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

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;
    
    #[test]
    fn test_command_choice_copy_clone() {
        let choice = CommandChoice::Scan;
        let choice_copy = choice;
        let choice_clone = choice;
        
        // All should be equal (testing Copy trait)
        assert!(matches!(choice, CommandChoice::Scan));
        assert!(matches!(choice_copy, CommandChoice::Scan));
        assert!(matches!(choice_clone, CommandChoice::Scan));
    }
    
    #[test]
    fn test_report_choice_copy_clone() {
        let choice = ReportChoice::Items;
        let choice_copy = choice;
        let choice_clone = choice;
        
        // All should be equal (testing Copy trait)
        assert!(matches!(choice, ReportChoice::Items));
        assert!(matches!(choice_copy, ReportChoice::Items));
        assert!(matches!(choice_clone, ReportChoice::Items));
    }
    
    #[test]
    fn test_item_report_choice_copy_clone() {
        let choice = ItemReportChoice::InvalidItems;
        let choice_copy = choice;
        let choice_clone = choice;
        
        // All should be equal (testing Copy trait)
        assert!(matches!(choice, ItemReportChoice::InvalidItems));
        assert!(matches!(choice_copy, ItemReportChoice::InvalidItems));
        assert!(matches!(choice_clone, ItemReportChoice::InvalidItems));
    }
    
    #[test]
    fn test_command_choices_completeness() {
        // Verify all enum variants are represented in the static array
        assert_eq!(COMMAND_CHOICES.len(), 5);
        
        // Verify each choice has a label
        let choices: Vec<CommandChoice> = COMMAND_CHOICES.iter().map(|(choice, _)| *choice).collect();
        assert!(choices.iter().any(|c| matches!(c, CommandChoice::Scan)));
        assert!(choices.iter().any(|c| matches!(c, CommandChoice::QuerySimple)));
        assert!(choices.iter().any(|c| matches!(c, CommandChoice::Explore)));
        assert!(choices.iter().any(|c| matches!(c, CommandChoice::Report)));
        assert!(choices.iter().any(|c| matches!(c, CommandChoice::Exit)));
        
        // Verify labels are not empty
        for (_, label) in COMMAND_CHOICES {
            assert!(!label.is_empty(), "Command choice label should not be empty");
        }
    }
    
    #[test]
    fn test_report_choices_completeness() {
        // Verify all enum variants are represented in the static array
        assert_eq!(REPORT_CHOICES.len(), 5);
        
        // Verify each choice has a label
        let choices: Vec<ReportChoice> = REPORT_CHOICES.iter().map(|(choice, _)| *choice).collect();
        assert!(choices.iter().any(|c| matches!(c, ReportChoice::Roots)));
        assert!(choices.iter().any(|c| matches!(c, ReportChoice::Scans)));
        assert!(choices.iter().any(|c| matches!(c, ReportChoice::Items)));
        assert!(choices.iter().any(|c| matches!(c, ReportChoice::Changes)));
        assert!(choices.iter().any(|c| matches!(c, ReportChoice::Exit)));
        
        // Verify labels are not empty
        for (_, label) in REPORT_CHOICES {
            assert!(!label.is_empty(), "Report choice label should not be empty");
        }
    }
    
    #[test]
    fn test_item_report_choices_completeness() {
        // Verify all enum variants are represented in the static array
        assert_eq!(ITEM_REPORT_CHOICES.len(), 2);
        
        // Verify each choice has a label
        let choices: Vec<ItemReportChoice> = ITEM_REPORT_CHOICES.iter().map(|(choice, _)| *choice).collect();
        assert!(choices.iter().any(|c| matches!(c, ItemReportChoice::InvalidItems)));
        assert!(choices.iter().any(|c| matches!(c, ItemReportChoice::Exit)));
        
        // Verify labels are not empty
        for (_, label) in ITEM_REPORT_CHOICES {
            assert!(!label.is_empty(), "Item report choice label should not be empty");
        }
    }
    
    #[test]
    fn test_command_choices_labels() {
        // Test specific label mappings
        let scan_label = COMMAND_CHOICES.iter()
            .find(|(choice, _)| matches!(choice, CommandChoice::Scan))
            .map(|(_, label)| *label);
        assert_eq!(scan_label, Some("Scan"));
        
        let query_label = COMMAND_CHOICES.iter()
            .find(|(choice, _)| matches!(choice, CommandChoice::QuerySimple))
            .map(|(_, label)| *label);
        assert_eq!(query_label, Some("Query"));
        
        let explore_label = COMMAND_CHOICES.iter()
            .find(|(choice, _)| matches!(choice, CommandChoice::Explore))
            .map(|(_, label)| *label);
        assert_eq!(explore_label, Some("Explore"));
        
        let report_label = COMMAND_CHOICES.iter()
            .find(|(choice, _)| matches!(choice, CommandChoice::Report))
            .map(|(_, label)| *label);
        assert_eq!(report_label, Some("Report"));
        
        let exit_label = COMMAND_CHOICES.iter()
            .find(|(choice, _)| matches!(choice, CommandChoice::Exit))
            .map(|(_, label)| *label);
        assert_eq!(exit_label, Some("Exit"));
    }
    
    #[test]
    fn test_cli_scan_parsing_with_root_id() {
        let result = Cli::try_parse_from(["fspulse", "scan", "--root-id", "123"]);
        assert!(result.is_ok());
        
        let cli = result.unwrap();
        if let Command::Scan { root_id, root_path, last, .. } = cli.command {
            assert_eq!(root_id, Some(123));
            assert_eq!(root_path, None);
            assert!(!last);
        } else {
            panic!("Expected Scan command");
        }
    }
    
    #[test]
    fn test_cli_scan_parsing_with_root_path() {
        let result = Cli::try_parse_from(["fspulse", "scan", "--root-path", "/test/path"]);
        assert!(result.is_ok());
        
        let cli = result.unwrap();
        if let Command::Scan { root_id, root_path, last, .. } = cli.command {
            assert_eq!(root_id, None);
            assert_eq!(root_path, Some("/test/path".to_string()));
            assert!(!last);
        } else {
            panic!("Expected Scan command");
        }
    }
    
    #[test]
    fn test_cli_scan_parsing_with_last() {
        let result = Cli::try_parse_from(["fspulse", "scan", "--last"]);
        assert!(result.is_ok());
        
        let cli = result.unwrap();
        if let Command::Scan { root_id, root_path, last, .. } = cli.command {
            assert_eq!(root_id, None);
            assert_eq!(root_path, None);
            assert!(last);
        } else {
            panic!("Expected Scan command");
        }
    }
    
    #[test]
    fn test_cli_scan_parsing_with_hash_options() {
        let result = Cli::try_parse_from(["fspulse", "scan", "--root-id", "1", "--no-hash", "--validate-all"]);
        assert!(result.is_ok());
        
        let cli = result.unwrap();
        if let Command::Scan { no_hash, hash_new, no_validate, validate_all, .. } = cli.command {
            assert!(no_hash);
            assert!(!hash_new);
            assert!(!no_validate);
            assert!(validate_all);
        } else {
            panic!("Expected Scan command");
        }
    }
    
    #[test]
    fn test_cli_interact_parsing() {
        let result = Cli::try_parse_from(["fspulse", "interact"]);
        assert!(result.is_ok());

        let cli = result.unwrap();
        if let Command::Interact = cli.command {
            // Success - just verify the command type
        } else {
            panic!("Expected Interact command");
        }
    }

    #[test]
    fn test_cli_query_parsing() {
        let result = Cli::try_parse_from(["fspulse", "query", "items where scan:(5)"]);
        assert!(result.is_ok());

        let cli = result.unwrap();
        if let Command::Query { query } = cli.command {
            assert_eq!(query, "items where scan:(5)");
        } else {
            panic!("Expected Query command");
        }
    }
    
    #[test]
    fn test_cli_report_roots_parsing() {
        let result = Cli::try_parse_from(["fspulse", "report", "roots", "--root-id", "42"]);
        assert!(result.is_ok());
        
        let cli = result.unwrap();
        if let Command::Report { report_type } = cli.command {
            if let ReportType::Roots { root_id, root_path, .. } = report_type {
                assert_eq!(root_id, Some(42));
                assert_eq!(root_path, None);
            } else {
                panic!("Expected Roots report type");
            }
        } else {
            panic!("Expected Report command");
        }
    }
    
    #[test]
    fn test_cli_report_items_parsing() {
        let result = Cli::try_parse_from(["fspulse", "report", "items", "--root-id", "1", "--invalid", "--format", "tree"]);
        assert!(result.is_ok());
        
        let cli = result.unwrap();
        if let Command::Report { report_type } = cli.command {
            if let ReportType::Items { root_id, invalid, format, .. } = report_type {
                assert_eq!(root_id, Some(1));
                assert!(invalid);
                assert_eq!(format, "tree");
            } else {
                panic!("Expected Items report type");
            }
        } else {
            panic!("Expected Report command");
        }
    }
    
    #[test]
    fn test_cli_parsing_conflicts() {
        // Test that conflicting arguments are properly rejected
        let result = Cli::try_parse_from(["fspulse", "scan", "--root-id", "1", "--root-path", "/test"]);
        assert!(result.is_err(), "Should reject conflicting root-id and root-path");
        
        let result = Cli::try_parse_from(["fspulse", "scan", "--root-id", "1", "--last"]);
        assert!(result.is_err(), "Should reject conflicting root-id and last");
        
        let result = Cli::try_parse_from(["fspulse", "report", "roots", "--root-id", "1", "--root-path", "/test"]);
        assert!(result.is_err(), "Should reject conflicting root-id and root-path in reports");
    }
    
    #[test]
    fn test_cli_parsing_invalid_arguments() {
        // Test various invalid argument combinations
        let result = Cli::try_parse_from(["fspulse", "nonexistent-command"]);
        assert!(result.is_err(), "Should reject unknown commands");
        
        let result = Cli::try_parse_from(["fspulse", "scan", "--invalid-flag"]);
        assert!(result.is_err(), "Should reject unknown flags");
        
        let result = Cli::try_parse_from(["fspulse", "scan", "--root-id", "not-a-number"]);
        assert!(result.is_err(), "Should reject non-numeric root ID");
    }
}
