mod database;
mod changes;
mod cli;
mod error;
mod analysis;
mod items;
mod reports;
mod roots;
mod scans;
mod scan_machine;
mod schema;
mod utils;

use cli::Cli;
use indicatif::MultiProgress;
use indicatif_log_bridge::LogWrapper;
use log::error;

fn main() {
    // Must set an environment variable to use.
    // Set RUST_LOG to one of:
    // ERROR → WARN → INFO → DEBUG → TRACE
    /*
    let logger = 
        env_logger::Builder::from_env(Env::default())
            .filter_level(LevelFilter::Warn)
            .filter_module("symphonia_core::probe", LevelFilter::Warn) // Suppresses error! logs
            .init();    debug!("Command-line args: {:?}", std::env::args_os().collect::<Vec<_>>());
    */

    let logger = 
        env_logger::Builder::from_env(env_logger::Env::default().
            default_filter_or("error"))
            .build();
    let level = logger.filter();
    
    let mut multi_prog = MultiProgress::new();
    LogWrapper::new(multi_prog.clone(), logger)
        .try_init()
        .unwrap();
    log::set_max_level(level);

    if let Err(err) = Cli::handle_command_line(&mut multi_prog) {
        error!("{:?}", err);
        eprint!("{}", err);
        std::process::exit(1);
    }
}