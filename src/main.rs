mod alerts;
mod api;
mod cli;
mod config;
mod database;
mod error;
mod hash;
mod item_identity;
mod item_version;
mod items;
mod query;
mod roots;
mod task_manager;
mod scanner;
mod scans;
mod schedules;
mod schema;
mod server;
mod sort;
mod task;
mod undo_log;
mod utils;
mod validate;

use std::path::PathBuf;
use std::time::Instant;

use chrono::Local;
use cli::Cli;
use config::Config;
use database::Database;
use directories::ProjectDirs;
use flexi_logger::{Cleanup, Criterion, DeferredNow, FileSpec, Logger, Naming, Record};
use log::{error, info};

/// Custom log format with microsecond timestamps for performance analysis
fn perf_format(
    w: &mut dyn std::io::Write,
    now: &mut DeferredNow,
    record: &Record,
) -> std::io::Result<()> {
    write!(
        w,
        "{} [{}] {}",
        now.format("%Y-%m-%d %H:%M:%S%.6f"),
        record.level(),
        record.args()
    )
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let project_dirs =
        ProjectDirs::from("", "", "fspulse").expect("Could not determine project directories");

    Config::load_config(&project_dirs)?;

    setup_logging(&project_dirs);

    // Initialize database connection pool
    Database::init()?;

    // Mark the start time and log a timestamped message
    let start = Instant::now();
    let now = Local::now();
    info!("fspulse starting at {}", now.format("%Y-%m-%d %H:%M:%S"));

    // Run the command line handler
    let result = Cli::handle_command_line();

    let duration = start.elapsed();

    match result {
        Ok(()) => {
            info!("fspulse completed successfully in {duration:.2?}");
            Ok(())
        }
        Err(err) => {
            error!("fspulse exited with error in {duration:.2?}");
            error!("{err:?}");
            eprint!("{err}");
            Err(err.into())
        }
    }
}

pub fn setup_logging(_project_dirs: &ProjectDirs) {
    let log_levels = format!(
        "fspulse={}, lopdf={}, logging_timer={}, TimerFinished={}, TimerStarting={}, TimerExecuting={}",
        Config::get_logging_fspulse(), Config::get_logging_lopdf(), Config::get_logging_fspulse(), Config::get_logging_fspulse(), Config::get_logging_fspulse(), Config::get_logging_fspulse(),
    );

    // Use data directory from config (already resolved from FSPULSE_DATA_DIR or default)
    let log_dir = PathBuf::from(Config::get_data_dir()).join("logs");

    Logger::try_with_str(log_levels)
        .unwrap()
        .format(perf_format)
        .log_to_file(FileSpec::default().directory(log_dir))
        .rotate(
            Criterion::Size(50_000_000), // Rotate at 50 MB to limit individual log file size
            Naming::TimestampsDirect,    // Directly logs to a timestamped file (no CURRENT)
            Cleanup::KeepLogFiles(20),   // Keep 20 most recent log files (~1 GB max)
        )
        .start()
        .unwrap();
}
