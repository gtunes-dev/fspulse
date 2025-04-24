use std::fs;

use directories::ProjectDirs;
use figment::{
    providers::{Format, Serialized, Toml},
    Figment,
};
use once_cell::sync::OnceCell;
use serde::{Deserialize, Serialize};

pub static CONFIG: OnceCell<Config> = OnceCell::new();

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct LoggingConfig {
    pub fspulse: String,
    pub lopdf: String,
}

impl LoggingConfig {
    const LOG_LEVELS: [&str; 5] = ["error", "warn", "info", "debug", "trace"];
    const FSPULSE_LEVEL: &str = "info";
    const LOPDF_LEVEL: &str = "error";

    fn default() -> Self {
        LoggingConfig {
            fspulse: Self::FSPULSE_LEVEL.to_string(),
            lopdf: Self::LOPDF_LEVEL.to_string(),
        }
    }

    fn ensure_valid(&mut self) {
        // Ensure that specified log levels are valid. If this list grows, we'll make a function to call for each
        // For now:
        //      trim and lowercase the string
        //      confirm that it's a valid log level. if not:
        //          - inform the user
        //          - use the default

        let mut str_original = self.fspulse.clone();
        self.fspulse = self.fspulse.trim().to_ascii_lowercase();
        if !Self::LOG_LEVELS.contains(&self.fspulse.as_str()) {
            eprintln!(
                "Config error: fspulse log level of '{}' is invalid - using default of '{}'",
                str_original,
                Self::FSPULSE_LEVEL
            );
            self.fspulse = Self::FSPULSE_LEVEL.to_owned();
        }

        str_original = self.lopdf.clone();
        self.lopdf = self.lopdf.trim().to_ascii_lowercase();
        if !Self::LOG_LEVELS.contains(&self.lopdf.as_str()) {
            eprintln!(
                "Config error: lopdf log level of '{}' is invalid - using default of '{}'",
                str_original,
                Self::LOPDF_LEVEL
            );
            self.lopdf = Self::LOPDF_LEVEL.to_owned();
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum HashFunc {
    MD5,
    SHA2,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct AnalysisConfig {
    threads: usize,
    hash: String,
}

impl AnalysisConfig {
    const HASH_FUNCS: [&str; 2] = ["md5", "sha2"];

    const HASH_MD5: &str = "md5";
    const HASH_SHA2: &str = "sha2";

    pub fn threads(&self) -> usize {
        self.threads
    }

    pub fn hash_func(&self) -> HashFunc {
        // We can't easily pre-cache the enum because
        // the classes here are serialized - this
        // approach is fine - we should always have a valid
        // value in "hash" so we just check for md5 and treat
        // anything else like SHA2 which is the same as saying
        // treat sha2 as sha2 but we really never want to panic
        match self.hash.as_str() {
            Self::HASH_MD5 => HashFunc::MD5,
            _ => HashFunc::SHA2,
        }
    }

    fn default() -> Self {
        AnalysisConfig {
            threads: 8,
            hash: Self::HASH_SHA2.to_owned(),
        }
    }

    fn ensure_valid(&mut self) {
        let str_original = self.hash.clone();
        self.hash = self.hash.trim().to_ascii_lowercase();
        if !Self::HASH_FUNCS.contains(&self.hash.as_str()) {
            eprintln!(
                "Config error: hash of '{}' is invalid - using default of '{}'",
                str_original,
                Self::HASH_SHA2
            );
            self.hash = Self::HASH_SHA2.to_owned();
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Config {
    pub logging: LoggingConfig,
    pub analysis: AnalysisConfig,
}

impl Config {
    /// Loads the configuration from a TOML file located in your app's data directory.
    /// If the file is missing or fails to parse, defaults are used.
    /// Additionally, writes the default config to disk if no file exists.
    pub fn load_config(project_dirs: &ProjectDirs) -> Self {
        let config_path = project_dirs.data_local_dir().join("config.toml");

        // Define default logging levels
        let default_config = Config {
            logging: LoggingConfig::default(),
            analysis: AnalysisConfig::default(),
        };

        // If the config file doesn't exist, write the default configuration to disk.
        if !config_path.exists() {
            if let Some(parent) = config_path.parent() {
                if let Err(e) = fs::create_dir_all(parent) {
                    eprintln!(
                        "Failed to create configuration directory {}: {}",
                        parent.display(),
                        e
                    );
                }
            }
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
        let mut config = figment.extract().unwrap_or_else(|err| {
            eprintln!(
                "Could not load config file {}: {}. Using default configuration.",
                config_path.display(),
                err
            );
            default_config
        });

        config.ensure_valid();

        config
    }

    fn ensure_valid(&mut self) {
        self.logging.ensure_valid();
        self.analysis.ensure_valid();
    }
}
