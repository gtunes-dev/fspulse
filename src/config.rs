use std::env;
use std::fs;
use std::path::PathBuf;

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

    fn with_env_overrides(mut self) -> Self {
        if let Ok(val) = env::var("FSPULSE_LOGGING_FSPULSE") {
            self.fspulse = val;
        }
        if let Ok(val) = env::var("FSPULSE_LOGGING_LOPDF") {
            self.lopdf = val;
        }
        self
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

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct AnalysisConfig {
    threads: usize,
}

impl AnalysisConfig {
    pub fn threads(&self) -> usize {
        self.threads
    }

    fn default() -> Self {
        AnalysisConfig {
            threads: 8,
        }
    }

    fn with_env_overrides(mut self) -> Self {
        if let Ok(val) = env::var("FSPULSE_ANALYSIS_THREADS") {
            if let Ok(threads) = val.parse::<usize>() {
                self.threads = threads;
            }
        }
        self
    }

    fn ensure_valid(&mut self) {
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ServerConfig {
    pub port: u16,
    pub host: String,
}

impl ServerConfig {
    fn default() -> Self {
        ServerConfig {
            port: 8080,
            host: "127.0.0.1".to_string(),
        }
    }

    fn with_env_overrides(mut self) -> Self {
        if let Ok(val) = env::var("FSPULSE_SERVER_HOST") {
            self.host = val;
        }
        if let Ok(val) = env::var("FSPULSE_SERVER_PORT") {
            if let Ok(port) = val.parse::<u16>() {
                self.port = port;
            }
        }
        self
    }

    fn ensure_valid(&mut self) {
        // Trim and validate host
        self.host = self.host.trim().to_string();
        if self.host.is_empty() {
            eprintln!("Config error: server host cannot be empty - using default '127.0.0.1'");
            self.host = "127.0.0.1".to_string();
        }

        // Validate port range
        if self.port == 0 {
            eprintln!("Config error: server port cannot be 0 - using default 8080");
            self.port = 8080;
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct DatabaseConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
}

impl DatabaseConfig {
    fn with_env_overrides(mut self) -> Self {
        if let Ok(val) = env::var("FSPULSE_DATABASE_PATH") {
            self.path = Some(val);
        }
        self
    }

    fn ensure_valid(&mut self) {
        // Trim path if provided
        if let Some(ref mut path) = self.path {
            *path = path.trim().to_string();
            if path.is_empty() {
                eprintln!("Config error: database path cannot be empty - ignoring");
                self.path = None;
            }
        }
    }

    pub fn get_path(&self) -> Option<&str> {
        self.path.as_deref()
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

impl Config {
    /// Loads the configuration from a TOML file located in your app's data directory.
    /// If the file is missing or fails to parse, defaults are used.
    /// Additionally, writes the default config to disk if no file exists.
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

        // Define default config
        // Note: database is None by default - users can add [database] section if needed
        let default_config = Config {
            logging: LoggingConfig::default(),
            analysis: AnalysisConfig::default(),
            server: ServerConfig::default(),
            database: None,
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

        // Apply environment variable overrides
        config.logging = config.logging.with_env_overrides();
        config.analysis = config.analysis.with_env_overrides();
        config.server = config.server.with_env_overrides();
        if let Some(database) = config.database {
            config.database = Some(database.with_env_overrides());
        } else {
            // Check if FSPULSE_DATABASE_PATH is set even if database section doesn't exist
            if let Ok(val) = env::var("FSPULSE_DATABASE_PATH") {
                config.database = Some(DatabaseConfig { path: Some(val) });
            }
        }

        config.ensure_valid();

        config
    }

    fn ensure_valid(&mut self) {
        self.logging.ensure_valid();
        self.analysis.ensure_valid();
        self.server.ensure_valid();
        if let Some(ref mut database) = self.database {
            database.ensure_valid();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;
    use std::io::Write;
    use tempfile::NamedTempFile;


    #[test]
    fn test_analysis_config_default() {
        let config = AnalysisConfig::default();
        assert_eq!(config.threads(), 8);
    }

    #[test]
    fn test_logging_config_default() {
        let config = LoggingConfig::default();
        assert_eq!(config.fspulse, "info");
        assert_eq!(config.lopdf, "error");
    }

    #[test]
    fn test_logging_config_ensure_valid() {
        let mut config = LoggingConfig {
            fspulse: "  INFO  ".to_string(),
            lopdf: "WARN".to_string(),
        };
        config.ensure_valid();
        assert_eq!(config.fspulse, "info");
        assert_eq!(config.lopdf, "warn");
    }

    #[test]
    fn test_logging_config_invalid_level() {
        let mut config = LoggingConfig {
            fspulse: "invalid_level".to_string(),
            lopdf: "also_invalid".to_string(),
        };
        config.ensure_valid();
        assert_eq!(config.fspulse, "info"); // Should default to FSPULSE_LEVEL
        assert_eq!(config.lopdf, "error"); // Should default to LOPDF_LEVEL
    }

    #[test]
    fn test_analysis_config_ensure_valid() {
        let mut config = AnalysisConfig {
            threads: 4,
        };
        config.ensure_valid();
    }

    #[test]
    fn test_config_serialization() {
        let config = Config {
            logging: LoggingConfig {
                fspulse: "debug".to_string(),
                lopdf: "warn".to_string(),
            },
            analysis: AnalysisConfig {
                threads: 16,
            },
            server: ServerConfig {
                port: 8080,
                host: "127.0.0.1".to_string(),
            },
            database: None,
        };

        let toml_str = toml::to_string(&config).expect("Failed to serialize config");
        assert!(toml_str.contains("fspulse = \"debug\""));
        assert!(toml_str.contains("lopdf = \"warn\""));
        assert!(toml_str.contains("threads = 16"));
        // database should not be serialized when None
        assert!(!toml_str.contains("[database]"));
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
        assert_eq!(config.logging.fspulse, "trace");
        assert_eq!(config.logging.lopdf, "info");
        assert_eq!(config.analysis.threads, 12);
    }

    #[test]
    fn test_server_config_default() {
        let config = ServerConfig::default();
        assert_eq!(config.port, 8080);
        assert_eq!(config.host, "127.0.0.1");
    }

    #[test]
    fn test_server_config_ensure_valid() {
        let mut config = ServerConfig {
            port: 3000,
            host: "  localhost  ".to_string(),
        };
        config.ensure_valid();
        assert_eq!(config.host, "localhost");
        assert_eq!(config.port, 3000);
    }

    #[test]
    fn test_server_config_invalid_values() {
        let mut config = ServerConfig {
            port: 0,
            host: "".to_string(),
        };
        config.ensure_valid();
        assert_eq!(config.host, "127.0.0.1"); // Should default
        assert_eq!(config.port, 8080); // Should default
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
        assert_eq!(config.logging.fspulse, "debug");
        assert_eq!(config.analysis.threads, 12);
        assert_eq!(config.server.port, 3000);
        assert_eq!(config.server.host, "0.0.0.0");
    }

    #[test]
    #[serial]
    fn test_load_config_with_temp_file() {
        let mut temp_file = NamedTempFile::new().expect("Failed to create temp file");
        let _temp_dir = temp_file.path().parent().unwrap().to_path_buf();

        let toml_content = r#"
[logging]
fspulse = "debug"
lopdf = "warn"

[analysis]
threads = 6

[server]
port = 9000
host = "localhost"
"#;
        temp_file.write_all(toml_content.as_bytes()).expect("Failed to write temp file");

        // Create a mock ProjectDirs pointing to our temp directory
        let project_dirs = directories::ProjectDirs::from("test", "test", "fspulse")
            .expect("Failed to create project dirs");

        // Test default config creation (when file doesn't exist at expected location)
        let config = Config::load_config(&project_dirs);

        // Should get defaults since the temp file isn't at the expected config location
        assert_eq!(config.logging.fspulse, "info");
        assert_eq!(config.logging.lopdf, "error");
        assert_eq!(config.analysis.threads, 8);
        assert_eq!(config.server.port, 8080);
        assert_eq!(config.server.host, "127.0.0.1");
        assert!(config.database.is_none());
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
    #[serial]
    fn test_logging_config_env_overrides() {
        // Set environment variables
        env::set_var("FSPULSE_LOGGING_FSPULSE", "debug");
        env::set_var("FSPULSE_LOGGING_LOPDF", "warn");

        let config = LoggingConfig::default().with_env_overrides();

        assert_eq!(config.fspulse, "debug");
        assert_eq!(config.lopdf, "warn");

        // Clean up
        env::remove_var("FSPULSE_LOGGING_FSPULSE");
        env::remove_var("FSPULSE_LOGGING_LOPDF");
    }

    #[test]
    #[serial]
    fn test_logging_config_no_env_overrides() {
        // Ensure no env vars are set
        env::remove_var("FSPULSE_LOGGING_FSPULSE");
        env::remove_var("FSPULSE_LOGGING_LOPDF");

        let config = LoggingConfig::default().with_env_overrides();

        // Should keep defaults
        assert_eq!(config.fspulse, "info");
        assert_eq!(config.lopdf, "error");
    }

    #[test]
    #[serial]
    fn test_analysis_config_env_overrides() {
        env::set_var("FSPULSE_ANALYSIS_THREADS", "16");

        let config = AnalysisConfig::default().with_env_overrides();

        assert_eq!(config.threads, 16);

        // Clean up
        env::remove_var("FSPULSE_ANALYSIS_THREADS");
    }

    #[test]
    #[serial]
    fn test_analysis_config_invalid_threads_env() {
        env::set_var("FSPULSE_ANALYSIS_THREADS", "not_a_number");

        let config = AnalysisConfig::default().with_env_overrides();

        // Should keep default when parse fails
        assert_eq!(config.threads, 8);

        // Clean up
        env::remove_var("FSPULSE_ANALYSIS_THREADS");
    }

    #[test]
    #[serial]
    fn test_server_config_env_overrides() {
        env::set_var("FSPULSE_SERVER_HOST", "0.0.0.0");
        env::set_var("FSPULSE_SERVER_PORT", "9090");

        let config = ServerConfig::default().with_env_overrides();

        assert_eq!(config.host, "0.0.0.0");
        assert_eq!(config.port, 9090);

        // Clean up
        env::remove_var("FSPULSE_SERVER_HOST");
        env::remove_var("FSPULSE_SERVER_PORT");
    }

    #[test]
    #[serial]
    fn test_server_config_invalid_port_env() {
        env::set_var("FSPULSE_SERVER_PORT", "invalid");

        let config = ServerConfig::default().with_env_overrides();

        // Should keep default when parse fails
        assert_eq!(config.port, 8080);

        // Clean up
        env::remove_var("FSPULSE_SERVER_PORT");
    }

    #[test]
    #[serial]
    fn test_database_config_env_override() {
        env::set_var("FSPULSE_DATABASE_PATH", "/custom/path/db");

        let config = DatabaseConfig { path: None }.with_env_overrides();

        assert_eq!(config.path, Some("/custom/path/db".to_string()));

        // Clean up
        env::remove_var("FSPULSE_DATABASE_PATH");
    }

    #[test]
    #[serial]
    fn test_database_config_env_override_replaces_existing() {
        env::set_var("FSPULSE_DATABASE_PATH", "/override/path");

        let config = DatabaseConfig {
            path: Some("/original/path".to_string()),
        }
        .with_env_overrides();

        assert_eq!(config.path, Some("/override/path".to_string()));

        // Clean up
        env::remove_var("FSPULSE_DATABASE_PATH");
    }

    #[test]
    #[serial]
    fn test_env_overrides_preserve_non_overridden_values() {
        env::set_var("FSPULSE_SERVER_HOST", "192.168.1.1");
        // Don't set PORT - it should keep its original value

        let config = ServerConfig {
            host: "127.0.0.1".to_string(),
            port: 3000,
        }
        .with_env_overrides();

        assert_eq!(config.host, "192.168.1.1"); // Overridden
        assert_eq!(config.port, 3000); // Preserved

        // Clean up
        env::remove_var("FSPULSE_SERVER_HOST");
    }

    #[test]
    #[serial]
    fn test_multiple_env_overrides_together() {
        // Set multiple env vars across different config sections
        env::set_var("FSPULSE_LOGGING_FSPULSE", "trace");
        env::set_var("FSPULSE_ANALYSIS_THREADS", "32");
        env::set_var("FSPULSE_SERVER_HOST", "0.0.0.0");
        env::set_var("FSPULSE_SERVER_PORT", "7070");

        let logging = LoggingConfig::default().with_env_overrides();
        let analysis = AnalysisConfig::default().with_env_overrides();
        let server = ServerConfig::default().with_env_overrides();

        assert_eq!(logging.fspulse, "trace");
        assert_eq!(analysis.threads, 32);
        assert_eq!(server.host, "0.0.0.0");
        assert_eq!(server.port, 7070);

        // Clean up
        env::remove_var("FSPULSE_LOGGING_FSPULSE");
        env::remove_var("FSPULSE_ANALYSIS_THREADS");
        env::remove_var("FSPULSE_SERVER_HOST");
        env::remove_var("FSPULSE_SERVER_PORT");
    }
}
