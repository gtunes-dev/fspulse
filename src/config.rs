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
pub struct WebConfig {
    pub use_mock_data: bool,
    pub port: u16,
    pub host: String,
}

impl WebConfig {
    fn default() -> Self {
        WebConfig {
            use_mock_data: false,
            port: 8080,
            host: "127.0.0.1".to_string(),
        }
    }

    fn ensure_valid(&mut self) {
        // Trim and validate host
        self.host = self.host.trim().to_string();
        if self.host.is_empty() {
            eprintln!("Config error: web host cannot be empty - using default '127.0.0.1'");
            self.host = "127.0.0.1".to_string();
        }

        // Validate port range
        if self.port == 0 {
            eprintln!("Config error: web port cannot be 0 - using default 8080");
            self.port = 8080;
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Config {
    pub logging: LoggingConfig,
    pub analysis: AnalysisConfig,
    pub web: WebConfig,
}

impl Config {
    /// Loads the configuration from a TOML file located in your app's data directory.
    /// If the file is missing or fails to parse, defaults are used.
    /// Additionally, writes the default config to disk if no file exists.
    pub fn load_config(project_dirs: &ProjectDirs) -> Self {
        let config_path = project_dirs.data_local_dir().join("config.toml");

        // Define default config
        let default_config = Config {
            logging: LoggingConfig::default(),
            analysis: AnalysisConfig::default(),
            web: WebConfig::default(),
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
        self.web.ensure_valid();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_hash_func_enum() {
        let config = AnalysisConfig {
            threads: 4,
            hash: "md5".to_string(),
        };
        assert!(matches!(config.hash_func(), HashFunc::MD5));

        let config = AnalysisConfig {
            threads: 4,
            hash: "sha2".to_string(),
        };
        assert!(matches!(config.hash_func(), HashFunc::SHA2));

        // Default to SHA2 for unknown values
        let config = AnalysisConfig {
            threads: 4,
            hash: "unknown".to_string(),
        };
        assert!(matches!(config.hash_func(), HashFunc::SHA2));
    }

    #[test]
    fn test_analysis_config_default() {
        let config = AnalysisConfig::default();
        assert_eq!(config.threads(), 8);
        assert!(matches!(config.hash_func(), HashFunc::SHA2));
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
            hash: "  SHA2  ".to_string(),
        };
        config.ensure_valid();
        assert_eq!(config.hash, "sha2");
    }

    #[test]
    fn test_analysis_config_invalid_hash() {
        let mut config = AnalysisConfig {
            threads: 4,
            hash: "invalid_hash".to_string(),
        };
        config.ensure_valid();
        assert_eq!(config.hash, "sha2"); // Should default to HASH_SHA2
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
                hash: "md5".to_string(),
            },
            web: WebConfig {
                use_mock_data: false,
                port: 8080,
                host: "127.0.0.1".to_string(),
            },
        };

        let toml_str = toml::to_string(&config).expect("Failed to serialize config");
        assert!(toml_str.contains("fspulse = \"debug\""));
        assert!(toml_str.contains("lopdf = \"warn\""));
        assert!(toml_str.contains("threads = 16"));
        assert!(toml_str.contains("hash = \"md5\""));
    }

    #[test]
    fn test_config_deserialization() {
        let toml_str = r#"
[logging]
fspulse = "trace"
lopdf = "info"

[analysis]
threads = 12
hash = "sha2"

[web]
use_mock_data = false
port = 8080
host = "127.0.0.1"
"#;

        let config: Config = toml::from_str(toml_str).expect("Failed to deserialize config");
        assert_eq!(config.logging.fspulse, "trace");
        assert_eq!(config.logging.lopdf, "info");
        assert_eq!(config.analysis.threads, 12);
        assert_eq!(config.analysis.hash, "sha2");
    }

    #[test]
    fn test_web_config_default() {
        let config = WebConfig::default();
        assert_eq!(config.use_mock_data, false);
        assert_eq!(config.port, 8080);
        assert_eq!(config.host, "127.0.0.1");
    }

    #[test]
    fn test_web_config_ensure_valid() {
        let mut config = WebConfig {
            use_mock_data: true,
            port: 3000,
            host: "  localhost  ".to_string(),
        };
        config.ensure_valid();
        assert_eq!(config.host, "localhost");
        assert_eq!(config.port, 3000);
        assert_eq!(config.use_mock_data, true);
    }

    #[test]
    fn test_web_config_invalid_values() {
        let mut config = WebConfig {
            use_mock_data: false,
            port: 0,
            host: "".to_string(),
        };
        config.ensure_valid();
        assert_eq!(config.host, "127.0.0.1"); // Should default
        assert_eq!(config.port, 8080); // Should default
    }

    #[test]
    fn test_config_with_web_section() {
        let toml_str = r#"
[logging]
fspulse = "debug"
lopdf = "warn"

[analysis]
threads = 12
hash = "sha2"

[web]
use_mock_data = true
port = 3000
host = "0.0.0.0"
"#;

        let config: Config = toml::from_str(toml_str).expect("Failed to deserialize config");
        assert_eq!(config.logging.fspulse, "debug");
        assert_eq!(config.analysis.threads, 12);
        assert_eq!(config.web.use_mock_data, true);
        assert_eq!(config.web.port, 3000);
        assert_eq!(config.web.host, "0.0.0.0");
    }

    #[test]
    fn test_load_config_with_temp_file() {
        let mut temp_file = NamedTempFile::new().expect("Failed to create temp file");
        let _temp_dir = temp_file.path().parent().unwrap().to_path_buf();

        let toml_content = r#"
[logging]
fspulse = "debug"
lopdf = "warn"

[analysis]
threads = 6
hash = "md5"

[web]
use_mock_data = true
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
        assert!(matches!(config.analysis.hash_func(), HashFunc::SHA2));
        assert_eq!(config.web.use_mock_data, false);
        assert_eq!(config.web.port, 8080);
        assert_eq!(config.web.host, "127.0.0.1");
    }
}
