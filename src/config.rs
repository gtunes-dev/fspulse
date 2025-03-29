use std::fs;

use directories::ProjectDirs;
use figment::{providers::{Format, Serialized, Toml}, Figment};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct LoggingConfig {
    pub fspulse: String,
    pub lopdf: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Config {
    pub logging: LoggingConfig,
}

impl Config {
    /// Loads the configuration from a TOML file located in your app's data directory.
    /// If the file is missing or fails to parse, defaults are used.
    /// Additionally, writes the default config to disk if no file exists.
    pub fn load_config(project_dirs: &ProjectDirs) -> Self {
        let config_path = project_dirs.data_local_dir().join("config.toml");

        // Define default logging levels
        let default_config = Config {
            logging: LoggingConfig {
                fspulse: "info".to_string(),
                lopdf: "error".to_string(),
            },
        };

        // If the config file doesn't exist, write the default configuration to disk.
        if !config_path.exists() {
            if let Ok(toml_string) = toml::to_string_pretty(&default_config) {
                if let Err(e) = fs::write(&config_path, toml_string) {
                    eprintln!(
                        "Failed to write default config to {}: {}",
                        config_path.display(),
                        e
                    );
                }
            } else {
                eprintln!("Failed to serialize default config.");
            }
        }

        // Build a Figment instance that uses the defaults merged with the TOML file (if it exists)
        let figment = Figment::from(Serialized::defaults(default_config.clone()))
            .merge(Toml::file(&config_path));
        
        // Attempt to extract the configuration; on error, log a message and fall back to defaults.
        figment.extract().unwrap_or_else(|err| {
            eprintln!(
                "Could not load config file {}: {}. Using default configuration.",
                config_path.display(),
                err
            );
            default_config
        })
    }
}