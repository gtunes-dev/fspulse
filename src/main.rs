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

use cli::Cli;
use log::{debug, error};


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