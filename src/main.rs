mod database;
mod changes;
mod cli;
mod error;
mod hash;
mod items;
mod reports;
mod roots;
mod scans;
mod scanner;
mod schema;
mod utils;
mod validators;

use std::{path::PathBuf, time::Instant};

use chrono::Local;
use cli::Cli;
use directories::ProjectDirs;
use flexi_logger::{Cleanup, Criterion, FileSpec, Logger, Naming};
use indicatif::MultiProgress;
use log::{error, info};

fn main() {
    setup_logging();

    let mut multi_prog = MultiProgress::new();

    // Mark the start time and log a timestamped message
    let start = Instant::now();
    let now = Local::now();
    info!("fspulse starting at {}", now.format("%Y-%m-%d %H:%M:%S"));
    
    // Run the command line handler
    let result = Cli::handle_command_line(&mut multi_prog);

    let duration = start.elapsed();

    match result {
        Ok(()) => {
            info!("fspulse completed successfully in {:.2?}", duration);
        }
        Err(err) => {
            error!("fspulse exited with error in {:.2?}", duration);
            error!("{:?}", err);
            eprint!("{}", err);
            std::process::exit(1);
        }
    }
}


pub fn setup_logging() {
    let log_dir: PathBuf = ProjectDirs::from("", "", "fspulse")
        .expect("Could not determine project directories")
        .data_local_dir()
        .join("logs");

    Logger::try_with_str("fspulse=info,lopdf=error")
        .unwrap()
        .log_to_file(FileSpec::default().directory(log_dir))
        .rotate(
            Criterion::Size(u64::MAX),       // Effectively disables size-based rotation
            Naming::TimestampsDirect,        // 💡 Directly logs to a timestamped file (no CURRENT)
            Cleanup::KeepLogFiles(100),      // Keep 100 most recent log files
        )
        .start()
        .unwrap();
}