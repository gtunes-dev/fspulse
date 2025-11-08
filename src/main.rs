mod alerts;
mod api;
mod changes;
mod cli;
mod config;
mod database;
mod error;
mod explore;
mod hash;
mod items;
mod progress;
mod query;
mod reports;
mod roots;
mod scan_manager;
mod scanner;
mod scans;
mod schedules;
mod schema;
mod server;
mod sort;
mod utils;
mod validate;

use std::env;
use std::path::PathBuf;
use std::time::Instant;

use chrono::Local;
use cli::Cli;
use config::{Config, CONFIG};
use directories::ProjectDirs;
use flexi_logger::{Cleanup, Criterion, FileSpec, Logger, Naming};
use log::{error, info};

fn main() {
    let project_dirs =
        ProjectDirs::from("", "", "fspulse").expect("Could not determine project directories");

    let config = Config::load_config(&project_dirs);
    CONFIG.set(config).expect("Config already set!");

    setup_logging(&project_dirs);

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
        }
        Err(err) => {
            error!("fspulse exited with error in {duration:.2?}");
            error!("{err:?}");
            eprint!("{err}");
            std::process::exit(1);
        }
    }
}

pub fn setup_logging(project_dirs: &ProjectDirs) {
    let config = CONFIG.get().expect("Config not initialized");
    let log_levels = format!(
        "fspulse={}, lopdf={}",
        config.logging.fspulse, config.logging.lopdf
    );

    // Check for Docker/custom data directory via environment variable
    let log_dir = if let Ok(data_dir) = env::var("FSPULSE_DATA_DIR") {
        PathBuf::from(data_dir).join("logs")
    } else {
        project_dirs.data_local_dir().join("logs")
    };

    Logger::try_with_str(log_levels)
        .unwrap()
        .log_to_file(FileSpec::default().directory(log_dir))
        .rotate(
            Criterion::Size(u64::MAX),  // Effectively disables size-based rotation
            Naming::TimestampsDirect,   // 💡 Directly logs to a timestamped file (no CURRENT)
            Cleanup::KeepLogFiles(100), // Keep 100 most recent log files
        )
        .start()
        .unwrap();
}
