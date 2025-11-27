use clap::{Parser, Subcommand};
use log::info;

use crate::config::Config;
use crate::error::FsPulseError;

#[derive(Parser)]
#[command(
    name = "fspulse",
    version,
    about = "FsPulse: Filesystem scanning and monitoring service"
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Subcommand)]
pub enum Command {
    /// Start the server (default if no command specified)
    Serve,
}

impl Cli {
    pub fn handle_command_line() -> Result<(), FsPulseError> {
        let args = Cli::parse();

        // Default to Serve if no command specified
        match args.command.unwrap_or(Command::Serve) {
            Command::Serve => Self::start_server(),
        }
    }

    fn start_server() -> Result<(), FsPulseError> {
        let host = Config::get_server_host();
        let port = Config::get_server_port();

        info!("Starting server on {}:{}", host, port);

        let rt = tokio::runtime::Runtime::new()
            .map_err(|e| FsPulseError::Error(format!("Failed to create runtime: {}", e)))?;

        rt.block_on(async {
            let web_server = crate::server::WebServer::new(host, port);
            web_server.start().await
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[test]
    fn test_cli_parsing_no_command_defaults_to_serve() {
        let result = Cli::try_parse_from(["fspulse"]);
        assert!(result.is_ok(), "Should accept no command");

        let cli = result.unwrap();
        assert!(cli.command.is_none());
        // Verify default behavior
        assert!(matches!(cli.command.unwrap_or(Command::Serve), Command::Serve));
    }

    #[test]
    fn test_cli_parsing_explicit_serve_command() {
        let result = Cli::try_parse_from(["fspulse", "serve"]);
        assert!(result.is_ok(), "Should accept explicit serve command");

        let cli = result.unwrap();
        assert!(matches!(cli.command, Some(Command::Serve)));
    }

    #[test]
    fn test_cli_parsing_invalid_arguments() {
        let result = Cli::try_parse_from(["fspulse", "nonexistent-command"]);
        assert!(result.is_err(), "Should reject unknown commands");

        let result = Cli::try_parse_from(["fspulse", "serve", "--invalid-flag"]);
        assert!(result.is_err(), "Should reject unknown flags on serve");
    }
}
