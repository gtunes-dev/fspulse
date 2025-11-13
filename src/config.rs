use std::env;
use std::fs;
use std::path::PathBuf;

use directories::ProjectDirs;
use figment::{
    providers::{Env, Format, Serialized, Toml},
    Figment,
};
use once_cell::sync::OnceCell;
use serde::{Deserialize, Serialize};

use crate::error::FsPulseError;

pub static CONFIG: OnceCell<Config> = OnceCell::new();

/// Represents where a configuration value came from
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ConfigSource {
    Environment,
    ConfigFile,
    Default,
}

/// A configuration value with provenance tracking
///
/// Serializes/deserializes transparently - only the inner value is written/read,
/// while source is tracked separately at runtime
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ConfigValue<T> {
    pub value: T,
    #[serde(skip, default = "default_config_source")]
    pub source: ConfigSource,
}

fn default_config_source() -> ConfigSource {
    ConfigSource::Default
}


// Helper function to generate source context for error messages
fn source_context(source: ConfigSource, field_path: &str) -> String {
    match source {
        ConfigSource::Environment => {
            let env_var = format!("FSPULSE_{}", field_path.replace(".", "_").to_uppercase());
            format!("environment variable {}", env_var)
        }
        ConfigSource::ConfigFile => {
            format!("config.toml field '{}'", field_path)
        }
        ConfigSource::Default => unreachable!("defaults don't need validation"),
    }
}

// Helper function to enumerate all leaf values in a toml::Value tree
fn enumerate_leaf_values(value: &toml::Value, prefix: Vec<String>) -> Vec<(Vec<String>, toml::Value)> {
    match value {
        toml::Value::Table(map) => {
            map.iter()
                .flat_map(|(k, v)| {
                    let mut new_prefix = prefix.clone();
                    new_prefix.push(k.clone());
                    enumerate_leaf_values(v, new_prefix)
                })
                .collect()
        }
        leaf_value => vec![(prefix, leaf_value.clone())],
    }
}

// Type extraction helpers
fn extract_usize(value: &toml::Value, field_path: &str) -> Result<usize, FsPulseError> {
    match value {
        toml::Value::Integer(n) if *n >= 0 => Ok(*n as usize),
        toml::Value::String(s) => s.trim().parse().map_err(|_| {
            FsPulseError::ConfigError(format!("{} must be a positive number, got '{}'", field_path, s))
        }),
        _ => Err(FsPulseError::ConfigError(format!(
            "{} must be a number",
            field_path
        ))),
    }
}

fn extract_u16(value: &toml::Value, field_path: &str) -> Result<u16, FsPulseError> {
    match value {
        toml::Value::Integer(n) if *n >= 0 && *n <= u16::MAX as i64 => Ok(*n as u16),
        toml::Value::String(s) => s.trim().parse().map_err(|_| {
            FsPulseError::ConfigError(format!("{} must be a number between 0 and 65535, got '{}'", field_path, s))
        }),
        _ => Err(FsPulseError::ConfigError(format!(
            "{} must be a number",
            field_path
        ))),
    }
}

fn extract_string(value: &toml::Value) -> String {
    match value {
        toml::Value::String(s) => s.trim().to_ascii_lowercase(),
        _ => value.to_string().trim().to_ascii_lowercase(),
    }
}

// Public helper functions for working with config files
// Used by settings.rs for updating configuration

/// Gets the path to the config.toml file
///
/// Checks FSPULSE_DATA_DIR environment variable first,
/// then falls back to OS-specific directories.
pub fn get_config_path(project_dirs: &ProjectDirs) -> PathBuf {
    let config_dir = if let Ok(data_dir) = env::var("FSPULSE_DATA_DIR") {
        PathBuf::from(data_dir)
    } else {
        project_dirs.data_local_dir().to_path_buf()
    };
    config_dir.join("config.toml")
}

/// Loads the config.toml file as a toml::Value without any Figment processing
///
/// Returns an empty table if the file doesn't exist or can't be parsed.
pub fn load_toml_only(config_path: &PathBuf) -> Result<toml::Value, String> {
    if config_path.exists() {
        let toml_str = fs::read_to_string(config_path)
            .map_err(|e| format!("Failed to read config file: {}", e))?;

        toml::from_str::<toml::Value>(&toml_str)
            .map_err(|e| format!("Failed to parse config file: {}", e))
    } else {
        Ok(toml::Value::Table(toml::map::Map::new()))
    }
}

/// Writes a toml::Value to the config.toml file
///
/// Creates the config directory if it doesn't exist.
pub fn write_toml(config_path: &PathBuf, value: &toml::Value) -> Result<(), String> {
    let config_toml = toml::to_string_pretty(value)
        .map_err(|e| format!("Failed to serialize config: {}", e))?;

    // Ensure config directory exists
    if let Some(parent) = config_path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create config directory: {}", e))?;
    }

    fs::write(config_path, config_toml)
        .map_err(|e| format!("Failed to write config file: {}", e))?;

    Ok(())
}

// Wrapped config structs (with ConfigValue) - used at runtime
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct LoggingConfig {
    pub fspulse: ConfigValue<String>,
    pub lopdf: ConfigValue<String>,
}

impl Default for LoggingConfig {
    fn default() -> Self {
        LoggingConfig {
            fspulse: ConfigValue {
                value: "info".to_string(),
                source: ConfigSource::Default,
            },
            lopdf: ConfigValue {
                value: "error".to_string(),
                source: ConfigSource::Default,
            },
        }
    }
}

impl LoggingConfig {
    const LOG_LEVELS: [&str; 5] = ["error", "warn", "info", "debug", "trace"];

    fn set_fspulse(&mut self, level: ConfigValue<String>, field_path: &str) -> Result<(), FsPulseError> {
        if !Self::LOG_LEVELS.contains(&level.value.as_str()) {
            return Err(FsPulseError::ConfigError(format!(
                "Invalid {}: must be one of {:?}, got '{}'",
                source_context(level.source.clone(), field_path),
                Self::LOG_LEVELS,
                level.value
            )));
        }
        self.fspulse = level;
        Ok(())
    }

    fn set_lopdf(&mut self, level: ConfigValue<String>, field_path: &str) -> Result<(), FsPulseError> {
        if !Self::LOG_LEVELS.contains(&level.value.as_str()) {
            return Err(FsPulseError::ConfigError(format!(
                "Invalid {}: must be one of {:?}, got '{}'",
                source_context(level.source.clone(), field_path),
                Self::LOG_LEVELS,
                level.value
            )));
        }
        self.lopdf = level;
        Ok(())
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct AnalysisConfig {
    pub threads: ConfigValue<usize>,
}

impl Default for AnalysisConfig {
    fn default() -> Self {
        AnalysisConfig {
            threads: ConfigValue {
                value: 8,
                source: ConfigSource::Default,
            },
        }
    }
}

impl AnalysisConfig {
    fn set_threads(&mut self, threads: ConfigValue<usize>, field_path: &str) -> Result<(), FsPulseError> {
        if threads.value < 1 || threads.value > 24 {
            return Err(FsPulseError::ConfigError(format!(
                "Invalid {}: threads must be between 1 and 24, got {}",
                source_context(threads.source.clone(), field_path),
                threads.value
            )));
        }
        self.threads = threads;
        Ok(())
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ServerConfig {
    pub port: ConfigValue<u16>,
    pub host: ConfigValue<String>,
}

impl Default for ServerConfig {
    fn default() -> Self {
        ServerConfig {
            port: ConfigValue {
                value: 8080,
                source: ConfigSource::Default,
            },
            host: ConfigValue {
                value: "127.0.0.1".to_string(),
                source: ConfigSource::Default,
            },
        }
    }
}

impl ServerConfig {
    fn set_host(&mut self, host: ConfigValue<String>, field_path: &str) -> Result<(), FsPulseError> {
        if host.value.is_empty() {
            return Err(FsPulseError::ConfigError(format!(
                "Invalid {}: host cannot be empty",
                source_context(host.source.clone(), field_path)
            )));
        }
        self.host = host;
        Ok(())
    }

    fn set_port(&mut self, port: ConfigValue<u16>, field_path: &str) -> Result<(), FsPulseError> {
        if port.value == 0 {
            return Err(FsPulseError::ConfigError(format!(
                "Invalid {}: port cannot be 0",
                source_context(port.source.clone(), field_path)
            )));
        }
        self.port = port;
        Ok(())
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct DatabaseConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<ConfigValue<String>>,
}

impl DatabaseConfig {
    fn set_path(&mut self, path: ConfigValue<String>, field_path: &str) -> Result<(), FsPulseError> {
        if path.value.is_empty() {
            return Err(FsPulseError::ConfigError(format!(
                "Invalid {}: path cannot be empty",
                source_context(path.source.clone(), field_path)
            )));
        }
        self.path = Some(path);
        Ok(())
    }

    pub fn get_path(&self) -> Option<&str> {
        self.path.as_ref().map(|p| p.value.as_str())
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Config {
    pub logging: LoggingConfig,
    pub analysis: AnalysisConfig,
    pub server: ServerConfig,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub database: Option<DatabaseConfig>,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            logging: LoggingConfig::default(),
            analysis: AnalysisConfig::default(),
            server: ServerConfig::default(),
            database: None,
        }
    }
}

impl Config {
    /// Loads the configuration from a TOML file located in your app's data directory.
    /// If the file is missing or fails to parse, defaults are used.
    /// Additionally, writes the default config to disk if no file exists.
    ///
    /// Configuration is loaded in the following precedence order (highest to lowest):
    /// 1. Environment variables (FSPULSE_<SECTION>_<FIELD>)
    /// 2. TOML configuration file
    /// 3. Built-in defaults
    ///
    /// Docker Support: Checks FSPULSE_DATA_DIR environment variable first,
    /// then falls back to OS-specific directories.
    pub fn load_config(project_dirs: &ProjectDirs) -> Self {
        // Check for Docker/custom data directory via environment variable
        let config_dir = if let Ok(data_dir) = env::var("FSPULSE_DATA_DIR") {
            PathBuf::from(data_dir)
        } else {
            project_dirs.data_local_dir().to_path_buf()
        };

        let config_path = config_dir.join("config.toml");

        // Get default configuration
        let defaults = Config::default();

        // If the config file doesn't exist, write the default configuration to disk
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
            // Serialize defaults to TOML
            if let Ok(default_toml) = toml::to_string_pretty(&defaults) {
                if let Err(e) = fs::write(&config_path, default_toml) {
                    eprintln!(
                        "Failed to write default config to {}: {}",
                        config_path.display(),
                        e
                    );
                }
            } else {
                eprintln!("Failed to serialize default config");
            }
        }

        // Step 1: Parse TOML file (if exists)
        let toml_figment = Figment::from(Toml::file(&config_path));
        let toml_result = toml_figment.extract::<toml::Value>();

        // If TOML file exists but has errors, fail
        if config_path.exists() {
            if let Err(ref err) = toml_result {
                eprintln!("Fatal error parsing config file {}: {}", config_path.display(), err);
                std::process::exit(1);
            }
        }

        let toml_value = toml_result.unwrap_or_else(|_| toml::Value::Table(toml::map::Map::new()));

        // Step 2: Parse environment variables
        let env_figment = Figment::from(Env::prefixed("FSPULSE_").split("_"));
        let env_result = env_figment.extract::<toml::Value>();
        if let Err(ref err) = env_result {
            eprintln!("Fatal error parsing environment variables: {}", err);
            std::process::exit(1);
        }
        let env_value = env_result.unwrap();

        // Start with defaults
        let mut config = defaults;

        // Phase 1: Merge TOML values
        for (path, value) in enumerate_leaf_values(&toml_value, Vec::new()) {
            let field_path = path.join(".");
            if let Err(e) = config.merge_value(&path, &value, ConfigSource::ConfigFile, &field_path) {
                eprintln!("Fatal error: {}", e);
                std::process::exit(1);
            }
        }

        // Phase 2: Merge environment values (overrides TOML)
        for (path, value) in enumerate_leaf_values(&env_value, Vec::new()) {
            let field_path = path.join(".");
            if let Err(e) = config.merge_value(&path, &value, ConfigSource::Environment, &field_path) {
                eprintln!("Fatal error: {}", e);
                std::process::exit(1);
            }
        }

        config
    }

    fn merge_value(
        &mut self,
        path: &[String],
        value: &toml::Value,
        source: ConfigSource,
        field_path: &str,
    ) -> Result<(), FsPulseError> {
        // Normalize string values (trim and lowercase)
        let normalized_string = if matches!(value, toml::Value::String(_)) {
            Some(extract_string(value))
        } else {
            None
        };

        // Convert Vec<String> to Vec<&str> for pattern matching
        let path_strs: Vec<&str> = path.iter().map(|s| s.as_str()).collect();
        match path_strs.as_slice() {
            ["logging", "fspulse"] => {
                let level = normalized_string.as_ref().ok_or_else(|| {
                    FsPulseError::ConfigError(format!("{} must be a string", field_path))
                })?;
                self.logging.set_fspulse(
                    ConfigValue {
                        value: level.clone(),
                        source,
                    },
                    field_path,
                )?;
            }
            ["logging", "lopdf"] => {
                let level = normalized_string.as_ref().ok_or_else(|| {
                    FsPulseError::ConfigError(format!("{} must be a string", field_path))
                })?;
                self.logging.set_lopdf(
                    ConfigValue {
                        value: level.clone(),
                        source,
                    },
                    field_path,
                )?;
            }
            ["analysis", "threads"] => {
                let threads = extract_usize(value, field_path)?;
                self.analysis.set_threads(
                    ConfigValue {
                        value: threads,
                        source,
                    },
                    field_path,
                )?;
            }
            ["server", "host"] => {
                // For host, we want to preserve case and just trim
                let host = value
                    .as_str()
                    .ok_or_else(|| FsPulseError::ConfigError(format!("{} must be a string", field_path)))?
                    .trim();
                self.server.set_host(
                    ConfigValue {
                        value: host.to_string(),
                        source,
                    },
                    field_path,
                )?;
            }
            ["server", "port"] => {
                let port = extract_u16(value, field_path)?;
                self.server.set_port(
                    ConfigValue {
                        value: port,
                        source,
                    },
                    field_path,
                )?;
            }
            ["database", "path"] => {
                // For path, we just trim but preserve case
                let path_str = value
                    .as_str()
                    .ok_or_else(|| FsPulseError::ConfigError(format!("{} must be a string", field_path)))?
                    .trim();
                if self.database.is_none() {
                    self.database = Some(DatabaseConfig::default());
                }
                self.database.as_mut().unwrap().set_path(
                    ConfigValue {
                        value: path_str.to_string(),
                        source,
                    },
                    field_path,
                )?;
            }
            _ => {
                return Err(FsPulseError::ConfigError(format!(
                    "Unknown {}",
                    source_context(source, field_path)
                )));
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_analysis_config_default() {
        let config = AnalysisConfig::default();
        assert_eq!(config.threads.value, 8);
        assert_eq!(config.threads.source, ConfigSource::Default);
    }

    #[test]
    fn test_analysis_config_validation() {
        let mut config = AnalysisConfig::default();

        // Test thread count too low - should error
        let result = config.set_threads(
            ConfigValue { value: 0, source: ConfigSource::ConfigFile },
            "analysis.threads",
        );
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("must be between 1 and 24"));

        // Test thread count too high - should error
        let result = config.set_threads(
            ConfigValue { value: 100, source: ConfigSource::ConfigFile },
            "analysis.threads",
        );
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("must be between 1 and 24"));

        // Test valid thread counts - should succeed
        assert!(config
            .set_threads(
                ConfigValue { value: 1, source: ConfigSource::ConfigFile },
                "analysis.threads"
            )
            .is_ok());
        assert_eq!(config.threads.value, 1);

        assert!(config
            .set_threads(
                ConfigValue { value: 24, source: ConfigSource::ConfigFile },
                "analysis.threads"
            )
            .is_ok());
        assert_eq!(config.threads.value, 24);

        assert!(config
            .set_threads(
                ConfigValue { value: 12, source: ConfigSource::ConfigFile },
                "analysis.threads"
            )
            .is_ok());
        assert_eq!(config.threads.value, 12);
    }

    #[test]
    fn test_logging_config_default() {
        let config = LoggingConfig::default();
        assert_eq!(config.fspulse.value, "info");
        assert_eq!(config.lopdf.value, "error");
        assert_eq!(config.fspulse.source, ConfigSource::Default);
    }

    #[test]
    fn test_logging_config_normalization() {
        let mut config = LoggingConfig::default();

        // Test normalization: "info" should be accepted
        assert!(config
            .set_fspulse(
                ConfigValue {
                    value: "info".to_string(),
                    source: ConfigSource::ConfigFile
                },
                "logging.fspulse"
            )
            .is_ok());
        assert_eq!(config.fspulse.value, "info");

        // Test normalization: "warn" should be accepted
        assert!(config
            .set_lopdf(
                ConfigValue {
                    value: "warn".to_string(),
                    source: ConfigSource::ConfigFile
                },
                "logging.lopdf"
            )
            .is_ok());
        assert_eq!(config.lopdf.value, "warn");
    }

    #[test]
    fn test_logging_config_invalid_level() {
        let mut config = LoggingConfig::default();

        // Test invalid log level - should error
        let result = config.set_fspulse(
            ConfigValue {
                value: "invalid_level".to_string(),
                source: ConfigSource::ConfigFile
            },
            "logging.fspulse",
        );
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("must be one of"));

        // Test another invalid log level
        let result = config.set_lopdf(
            ConfigValue {
                value: "also_invalid".to_string(),
                source: ConfigSource::ConfigFile
            },
            "logging.lopdf",
        );
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("must be one of"));
    }

    #[test]
    fn test_config_serialization() {
        let config = Config {
            logging: LoggingConfig {
                fspulse: ConfigValue {
                    value: "debug".to_string(),
                    source: ConfigSource::ConfigFile,
                },
                lopdf: ConfigValue {
                    value: "warn".to_string(),
                    source: ConfigSource::ConfigFile,
                },
            },
            analysis: AnalysisConfig {
                threads: ConfigValue {
                    value: 16,
                    source: ConfigSource::ConfigFile,
                },
            },
            server: ServerConfig {
                port: ConfigValue {
                    value: 8080,
                    source: ConfigSource::Default,
                },
                host: ConfigValue {
                    value: "127.0.0.1".to_string(),
                    source: ConfigSource::Default,
                },
            },
            database: None,
        };

        let toml_str = toml::to_string(&config).expect("Failed to serialize config");
        assert!(toml_str.contains("fspulse = \"debug\""));
        assert!(toml_str.contains("lopdf = \"warn\""));
        assert!(toml_str.contains("threads = 16"));
        // database should not be serialized when None
        assert!(!toml_str.contains("[database]"));
        // source should not be serialized (it's marked with #[serde(skip)])
        assert!(!toml_str.contains("source"));
    }

    #[test]
    fn test_config_deserialization() {
        let toml_str = r#"
[logging]
fspulse = "trace"
lopdf = "info"

[analysis]
threads = 12

[server]
port = 8080
host = "127.0.0.1"
"#;

        let config: Config = toml::from_str(toml_str).expect("Failed to deserialize config");
        assert_eq!(config.logging.fspulse.value, "trace");
        assert_eq!(config.logging.lopdf.value, "info");
        assert_eq!(config.analysis.threads.value, 12);
        // Source should be Default when deserializing (due to #[serde(skip)])
        assert_eq!(config.logging.fspulse.source, ConfigSource::Default);
    }

    #[test]
    fn test_server_config_default() {
        let config = ServerConfig::default();
        assert_eq!(config.port.value, 8080);
        assert_eq!(config.host.value, "127.0.0.1");
        assert_eq!(config.port.source, ConfigSource::Default);
    }

    #[test]
    fn test_server_config_validation() {
        let mut config = ServerConfig::default();

        // Test valid host and port
        assert!(config
            .set_host(
                ConfigValue {
                    value: "localhost".to_string(),
                    source: ConfigSource::ConfigFile
                },
                "server.host"
            )
            .is_ok());
        assert_eq!(config.host.value, "localhost");

        assert!(config
            .set_port(
                ConfigValue { value: 3000, source: ConfigSource::ConfigFile },
                "server.port"
            )
            .is_ok());
        assert_eq!(config.port.value, 3000);
    }

    #[test]
    fn test_server_config_invalid_values() {
        let mut config = ServerConfig::default();

        // Test invalid port (0) - should error
        let result = config.set_port(
            ConfigValue { value: 0, source: ConfigSource::ConfigFile },
            "server.port",
        );
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("port cannot be 0"));

        // Test invalid host (empty) - should error
        let result = config.set_host(
            ConfigValue {
                value: "".to_string(),
                source: ConfigSource::ConfigFile
            },
            "server.host",
        );
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("host cannot be empty"));
    }

    #[test]
    fn test_config_with_server_section() {
        let toml_str = r#"
[logging]
fspulse = "debug"
lopdf = "warn"

[analysis]
threads = 12

[server]
port = 3000
host = "0.0.0.0"
"#;

        let config: Config = toml::from_str(toml_str).expect("Failed to deserialize config");
        assert_eq!(config.logging.fspulse.value, "debug");
        assert_eq!(config.analysis.threads.value, 12);
        assert_eq!(config.server.port.value, 3000);
        assert_eq!(config.server.host.value, "0.0.0.0");
    }

    #[test]
    fn test_database_config() {
        let toml_str = r#"
[logging]
fspulse = "info"
lopdf = "error"

[analysis]
threads = 8

[server]
port = 8080
host = "127.0.0.1"

[database]
path = "/custom/db/path"
"#;

        let config: Config = toml::from_str(toml_str).expect("Failed to deserialize config");
        assert!(config.database.is_some());
        assert_eq!(config.database.unwrap().get_path(), Some("/custom/db/path"));
    }

    #[test]
    fn test_env_override_analysis_threads() {
        figment::Jail::expect_with(|jail| {
            jail.set_env("FSPULSE_ANALYSIS_THREADS", "16");

            let figment = Figment::from(Serialized::defaults(Config::default()))
                .merge(Env::prefixed("FSPULSE_").split("_"));

            let config: Config = figment.extract()?;
            assert_eq!(config.analysis.threads.value, 16);

            Ok(())
        });
    }

    #[test]
    fn test_env_override_logging() {
        figment::Jail::expect_with(|jail| {
            jail.set_env("FSPULSE_LOGGING_FSPULSE", "debug");
            jail.set_env("FSPULSE_LOGGING_LOPDF", "warn");

            let figment = Figment::from(Serialized::defaults(Config::default()))
                .merge(Env::prefixed("FSPULSE_").split("_"));

            let config: Config = figment.extract()?;
            assert_eq!(config.logging.fspulse.value, "debug");
            assert_eq!(config.logging.lopdf.value, "warn");

            Ok(())
        });
    }

    #[test]
    fn test_env_override_server() {
        figment::Jail::expect_with(|jail| {
            jail.set_env("FSPULSE_SERVER_HOST", "0.0.0.0");
            jail.set_env("FSPULSE_SERVER_PORT", "9090");

            let figment = Figment::from(Serialized::defaults(Config::default()))
                .merge(Env::prefixed("FSPULSE_").split("_"));

            let config: Config = figment.extract()?;
            assert_eq!(config.server.host.value, "0.0.0.0");
            assert_eq!(config.server.port.value, 9090);

            Ok(())
        });
    }

    #[test]
    fn test_env_override_database() {
        figment::Jail::expect_with(|jail| {
            jail.set_env("FSPULSE_DATABASE_PATH", "/custom/path");

            let figment = Figment::from(Serialized::defaults(Config::default()))
                .merge(Env::prefixed("FSPULSE_").split("_"));

            let config: Config = figment.extract()?;
            assert_eq!(config.database.as_ref().unwrap().get_path(), Some("/custom/path"));

            Ok(())
        });
    }

    #[test]
    fn test_precedence_env_overrides_toml() {
        figment::Jail::expect_with(|jail| {
            // Create a TOML config file with certain values
            jail.create_file("config.toml", r#"
[logging]
fspulse = "info"
lopdf = "error"

[analysis]
threads = 8

[server]
port = 8080
host = "127.0.0.1"
"#)?;

            // Set environment variables that should override TOML values
            jail.set_env("FSPULSE_ANALYSIS_THREADS", "16");
            jail.set_env("FSPULSE_LOGGING_FSPULSE", "debug");
            jail.set_env("FSPULSE_SERVER_PORT", "9090");

            let figment = Figment::from(Serialized::defaults(Config::default()))
                .merge(Toml::file("config.toml"))
                .merge(Env::prefixed("FSPULSE_").split("_"));

            let config: Config = figment.extract()?;

            // Verify env vars override TOML
            assert_eq!(config.analysis.threads.value, 16); // env overrides toml (8)
            assert_eq!(config.logging.fspulse.value, "debug"); // env overrides toml (info)
            assert_eq!(config.server.port.value, 9090); // env overrides toml (8080)

            // Verify TOML values still used when no env override
            assert_eq!(config.logging.lopdf.value, "error"); // from toml
            assert_eq!(config.server.host.value, "127.0.0.1"); // from toml

            Ok(())
        });
    }

    #[test]
    fn test_precedence_toml_overrides_defaults() {
        figment::Jail::expect_with(|jail| {
            // Create a TOML config file
            jail.create_file("config.toml", r#"
[analysis]
threads = 12

[logging]
fspulse = "trace"

[server]
port = 3000
"#)?;

            let figment = Figment::from(Serialized::defaults(Config::default()))
                .merge(Toml::file("config.toml"));

            let config: Config = figment.extract()?;

            // Verify TOML overrides defaults
            assert_eq!(config.analysis.threads.value, 12); // toml overrides default (8)
            assert_eq!(config.logging.fspulse.value, "trace"); // toml overrides default (info)
            assert_eq!(config.server.port.value, 3000); // toml overrides default (8080)

            // Verify defaults still used when not in TOML
            assert_eq!(config.logging.lopdf.value, "error"); // default
            assert_eq!(config.server.host.value, "127.0.0.1"); // default

            Ok(())
        });
    }

    #[test]
    fn test_precedence_full_chain() {
        figment::Jail::expect_with(|jail| {
            // Create a TOML config with some overrides
            jail.create_file("config.toml", r#"
[analysis]
threads = 12

[logging]
fspulse = "warn"
lopdf = "info"
"#)?;

            // Set one env var to override TOML
            jail.set_env("FSPULSE_ANALYSIS_THREADS", "20");

            let figment = Figment::from(Serialized::defaults(Config::default()))
                .merge(Toml::file("config.toml"))
                .merge(Env::prefixed("FSPULSE_").split("_"));

            let config: Config = figment.extract()?;

            // Full precedence chain test:
            assert_eq!(config.analysis.threads.value, 20); // env > toml (12) > default (8)
            assert_eq!(config.logging.fspulse.value, "warn"); // toml > default (info)
            assert_eq!(config.logging.lopdf.value, "info"); // toml > default (error)
            assert_eq!(config.server.port.value, 8080); // default (no toml, no env)
            assert_eq!(config.server.host.value, "127.0.0.1"); // default (no toml, no env)

            Ok(())
        });
    }

    // Note: Tests for invalid type handling (Figment's all-or-nothing extraction behavior)
    // have been removed as we no longer use Figment's typed extraction in production.
    // The production code uses build_config_with_provenance which validates values
    // and provides fail-fast behavior on configuration errors.

    #[test]
    fn test_invalid_env_var_with_fallback() {
        figment::Jail::expect_with(|jail| {
            // Create a TOML config with valid value
            jail.create_file("config.toml", r#"
[analysis]
threads = 12
"#)?;

            // Set env var with INVALID type
            jail.set_env("FSPULSE_ANALYSIS_THREADS", "not_a_number");

            let figment = Figment::from(Serialized::defaults(Config::default()))
                .merge(Toml::file("config.toml"))
                .merge(Env::prefixed("FSPULSE_").split("_"));

            // Use unwrap_or_else like our actual code does
            let config = figment.extract().unwrap_or_else(|err| {
                println!("Extraction failed, using defaults. Error: {:?}", err);
                Config::default()
            });

            // What value do we actually end up with?
            println!("Final threads value: {}", config.analysis.threads.value);

            // Check if we got: toml value (12), default (8), or something else
            if config.analysis.threads.value == 12 {
                println!("SUCCESS: Got TOML value despite invalid env var!");
            } else if config.analysis.threads.value == 8 {
                println!("Got DEFAULT - lost TOML value due to invalid env var");
            } else {
                println!("Got unexpected value: {}", config.analysis.threads.value);
            }

            Ok(())
        });
    }
}
