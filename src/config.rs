use std::env;
use std::fs;
use std::path::PathBuf;
use std::sync::RwLock;

use directories::ProjectDirs;
use figment::{
    providers::{Env, Format, Toml},
    Figment,
};
use log::error;
use once_cell::sync::OnceCell;
use serde::Serialize;

use crate::error::FsPulseError;

// =============================================================================
// Constants
// =============================================================================

/// Minimum number of analysis threads
pub const MIN_ANALYSIS_THREADS: usize = 1;

/// Maximum number of analysis threads
pub const MAX_ANALYSIS_THREADS: usize = 24;

// =============================================================================
// Global Configuration State
// =============================================================================

pub static CONFIG: OnceCell<RwLock<Config>> = OnceCell::new();

// =============================================================================
// Core Types
// =============================================================================

/// Represents where a configuration value came from
#[derive(Clone, Debug, PartialEq)]
pub enum ConfigSource {
    Environment,
    ConfigFile,
}

/// A configuration value with full provenance tracking
///
/// Tracks all sources (env, file at startup, file current, default)
/// and provides methods for loading, updating, and accessing values.
#[derive(Clone, Debug)]
pub struct ConfigValue<T> {
    pub env_value: Option<T>,
    pub file_value: Option<T>,
    pub file_value_original: Option<T>,
    pub default_value: T,
    pub requires_restart: bool,

    path: (&'static str, &'static str),
    validator: fn(&toml::Value, ConfigSource) -> Result<T, FsPulseError>,
}

impl<T: Clone> ConfigValue<T> {
    pub fn new(
        default_value: T,
        path: (&'static str, &'static str),
        requires_restart: bool,
        validator: fn(&toml::Value, ConfigSource) -> Result<T, FsPulseError>,
    ) -> Self {
        Self {
            env_value: None,
            file_value: None,
            file_value_original: None,
            default_value,
            path,
            requires_restart,
            validator,
        }
    }

    /// Get the effective value based on precedence: env > active_file > file > default
    pub fn get(&self) -> &T {
        self.env_value
            .as_ref()
            .or(self.file_value_original.as_ref())
            .or(self.file_value.as_ref())
            .unwrap_or(&self.default_value)
    }

    /// Take values from TOML and environment maps during config load
    /// Removes values from maps if found (for later unknown key detection)
    pub fn take(
        &mut self,
        toml_map: &mut toml::Table,
        env_map: &mut toml::Table,
    ) -> Result<(), FsPulseError> {
        let (section, field) = self.path;

        // Try to take from environment first
        if let Some(section_table) = env_map.get_mut(section) {
            if let Some(fields) = section_table.as_table_mut() {
                if let Some(value) = fields.remove(field) {
                    self.env_value = Some((self.validator)(&value, ConfigSource::Environment)?);
                }
            }
        }

        // Try to take from config file
        if let Some(section_table) = toml_map.get_mut(section) {
            if let Some(fields) = section_table.as_table_mut() {
                if let Some(value) = fields.remove(field) {
                    self.file_value = Some((self.validator)(&value, ConfigSource::ConfigFile)?);
                    self.file_value_original = self.file_value.clone()
                }
            }
        }

        Ok(())
    }

    /// Set a new file value at runtime (called from UI)
    /// Validates, writes to disk, and preserves active value if restart required
    pub fn set_file_value(
        &mut self,
        new_value: T,
        config_path: &PathBuf,
    ) -> Result<(), FsPulseError>
    where
        T: PartialEq + Serialize,
    {
        // If value hasn't changed, nothing to do
        if self.file_value.as_ref() == Some(&new_value) {
            return Ok(());
        }

        // Write to config file
        self.write_to_toml(config_path, &new_value).map_err(|e| {
            error!("Failed to write configuration to file: {}", e);
            FsPulseError::ConfigError("Failed to update configuration".to_string())
        })?;

        // Update file value
        self.file_value = Some(new_value);
        // if this property doesn't require restart, the file value and original values
        // are kept in sync. The UI sees this as a value whose state of change does not
        // matter, which is accurate
        if !self.requires_restart {
            self.file_value_original = self.file_value.clone()
        }

        Ok(())
    }

    /// Delete the file value from config.toml (called from UI)
    /// Removes from disk and preserves active value if restart required
    pub fn delete_file_value(&mut self, config_path: &PathBuf) -> Result<(), FsPulseError> {
        // If no file value exists, nothing to do
        if self.file_value.is_none() {
            return Ok(());
        }

        // Remove from config file
        self.delete_from_toml(config_path).map_err(|e| {
            error!("Failed to delete configuration from file: {}", e);
            FsPulseError::ConfigError("Failed to delete configuration".to_string())
        })?;

        // Clear file value
        self.file_value = None;
        // if this property doesn't require restart, the file value and original values
        // are kept in sync. The UI sees this as a value whose state of change does not
        // matter, which is accurate
        if !self.requires_restart {
            self.file_value_original = None
        }

        Ok(())
    }

    /// Write this value to the config.toml file
    fn write_to_toml(&self, config_path: &PathBuf, value: &T) -> Result<(), FsPulseError>
    where
        T: Serialize,
    {
        // Load existing config or create new table
        let mut config_table = if config_path.exists() {
            load_toml_only(config_path)
                .map_err(FsPulseError::ConfigError)?
                .as_table()
                .cloned()
                .unwrap_or_default()
        } else {
            toml::map::Map::new()
        };

        let (section, field) = self.path;

        // Get or create section table
        let section_table = config_table
            .entry(section)
            .or_insert_with(|| toml::Value::Table(toml::map::Map::new()))
            .as_table_mut()
            .ok_or_else(|| {
                FsPulseError::ConfigError(format!("Config section '{}' is not a table", section))
            })?;

        // Set the field value
        let toml_value = toml::Value::try_from(value).map_err(|e| {
            FsPulseError::ConfigError(format!("Failed to convert value to TOML: {}", e))
        })?;
        section_table.insert(field.to_string(), toml_value);

        // Write back to file
        write_toml(config_path, &toml::Value::Table(config_table))
            .map_err(FsPulseError::ConfigError)
    }

    /// Delete this value from the config.toml file
    fn delete_from_toml(&self, config_path: &PathBuf) -> Result<(), FsPulseError> {
        // Load existing config
        if !config_path.exists() {
            return Ok(()); // Nothing to delete
        }

        let mut config_table = load_toml_only(config_path)
            .map_err(FsPulseError::ConfigError)?
            .as_table()
            .cloned()
            .unwrap_or_default();

        let (section, field) = self.path;

        // Get section table
        if let Some(section_value) = config_table.get_mut(section) {
            if let Some(section_table) = section_value.as_table_mut() {
                // Remove the field
                section_table.remove(field);
            }
        }

        // Write back to file
        write_toml(config_path, &toml::Value::Table(config_table))
            .map_err(FsPulseError::ConfigError)
    }
}

/// Main configuration structure (flat)
#[derive(Clone, Debug)]
pub struct Config {
    /// Special field: data directory (from FSPULSE_DATA_DIR or ProjectDirs)
    /// Not a ConfigValue - computed once at startup, read-only after that
    pub data_dir: String,

    pub server_host: ConfigValue<String>,
    pub server_port: ConfigValue<u16>,
    pub analysis_threads: ConfigValue<usize>,
    pub logging_fspulse: ConfigValue<String>,
    pub logging_lopdf: ConfigValue<String>,
    pub database_dir: ConfigValue<String>,
}

// =============================================================================
// Validation Functions
// =============================================================================

fn extract_usize(value: &toml::Value) -> Result<usize, String> {
    match value {
        toml::Value::Integer(n) if *n >= 0 => Ok(*n as usize),
        toml::Value::String(s) => s
            .trim()
            .parse()
            .map_err(|_| format!("must be a positive number, got '{}'", s)),
        _ => Err("must be a number".to_string()),
    }
}

fn extract_u16(value: &toml::Value) -> Result<u16, String> {
    match value {
        toml::Value::Integer(n) if *n >= 0 && *n <= u16::MAX as i64 => Ok(*n as u16),
        toml::Value::String(s) => s
            .trim()
            .parse()
            .map_err(|_| format!("must be a number between 0 and 65535, got '{}'", s)),
        _ => Err("must be a number".to_string()),
    }
}

fn extract_string(value: &toml::Value) -> String {
    match value {
        toml::Value::String(s) => s.trim().to_string(),
        _ => value.to_string().trim().to_string(),
    }
}

fn is_valid_log_level(level: &str) -> bool {
    matches!(
        level.to_lowercase().as_str(),
        "error" | "warn" | "info" | "debug" | "trace"
    )
}

fn validate_log_level(value: &toml::Value, source: ConfigSource) -> Result<String, FsPulseError> {
    let level = extract_string(value).to_lowercase();
    if !is_valid_log_level(&level) {
        return Err(FsPulseError::ConfigError(format!(
            "Invalid log level '{}' from {:?}. Must be one of: error, warn, info, debug, trace",
            level, source
        )));
    }
    Ok(level)
}

fn validate_threads(value: &toml::Value, source: ConfigSource) -> Result<usize, FsPulseError> {
    let threads = extract_usize(value).map_err(|e| {
        FsPulseError::ConfigError(format!("analysis.threads {}, from {:?}", e, source))
    })?;

    if !(MIN_ANALYSIS_THREADS..=MAX_ANALYSIS_THREADS).contains(&threads) {
        return Err(FsPulseError::ConfigError(format!(
            "analysis.threads must be between {} and {}, got {} from {:?}",
            MIN_ANALYSIS_THREADS, MAX_ANALYSIS_THREADS, threads, source
        )));
    }
    Ok(threads)
}

fn validate_port(value: &toml::Value, source: ConfigSource) -> Result<u16, FsPulseError> {
    let port = extract_u16(value)
        .map_err(|e| FsPulseError::ConfigError(format!("server.port {}, from {:?}", e, source)))?;

    if port == 0 {
        return Err(FsPulseError::ConfigError(format!(
            "server.port cannot be 0, from {:?}",
            source
        )));
    }
    Ok(port)
}

fn validate_host(value: &toml::Value, source: ConfigSource) -> Result<String, FsPulseError> {
    let host = extract_string(value);
    if host.trim().is_empty() {
        return Err(FsPulseError::ConfigError(format!(
            "server.host cannot be empty, from {:?}",
            source
        )));
    }
    Ok(host)
}

fn validate_database_dir(
    value: &toml::Value,
    _source: ConfigSource,
) -> Result<String, FsPulseError> {
    let dir = extract_string(value);
    // Empty string is valid - it means "use data_dir"
    Ok(dir)
}

// =============================================================================
// Public Configuration File Helpers
// =============================================================================

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
    let config_toml =
        toml::to_string_pretty(value).map_err(|e| format!("Failed to serialize config: {}", e))?;

    // Ensure config directory exists
    if let Some(parent) = config_path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create config directory: {}", e))?;
    }

    fs::write(config_path, config_toml)
        .map_err(|e| format!("Failed to write config file: {}", e))?;

    Ok(())
}

// =============================================================================
// Private Helper Functions
// =============================================================================

/// Helper function to create default config file as a commented template
fn create_default_config_file(config_path: &PathBuf) -> Result<(), FsPulseError> {
    // Create parent directory if needed
    if let Some(parent) = config_path.parent() {
        fs::create_dir_all(parent).map_err(|e| {
            FsPulseError::ConfigError(format!(
                "Failed to create config directory {}: {}",
                parent.display(),
                e
            ))
        })?;
    }

    // Write commented template file
    let template = r#"# FsPulse Configuration
#
# Precedence: Environment Variables > config.toml > Built-in Defaults
#
# To override a setting, uncomment and edit the line below.
# For environment variable names and full documentation, see:
# https://github.com/gregcampbellcohen/fspulse
#
# Examples:
#
# [server]
# host = "0.0.0.0"    # Default: "127.0.0.1"
# port = 8080          # Default: 8080
#
# [analysis]
# threads = 8          # Default: 8 (range: 1-24)
#
# [logging]
# fspulse = "info"     # Default: "info" (error, warn, info, debug, trace)
# lopdf = "error"      # Default: "error"
#
# [database]
# dir = ""             # Default: "" (empty = use data directory)
#                      # Set to a custom path to override
"#;

    fs::write(config_path, template)
        .map_err(|e| FsPulseError::ConfigError(format!("Failed to write config file: {}", e)))
}

/// Check for unknown keys in TOML and environment maps
fn check_for_unknown_keys(
    toml_map: &toml::Table,
    env_map: &toml::Table,
) -> Result<(), FsPulseError> {
    // List of deprecated configuration keys that should warn but not fail
    // Format: (section, field)
    const DEPRECATED_KEYS: &[(&str, &str)] = &[
        ("analysis", "hash"), // Was FSPULSE_ANALYSIS_HASH, deprecated
    ];

    // Helper to check if a key is deprecated
    let is_deprecated = |section: &str, field: &str| -> bool {
        DEPRECATED_KEYS.iter().any(|(s, f)| *s == section && *f == field)
    };

    // Check TOML map for unknown sections or keys
    for (section, value) in toml_map {
        if let Some(table) = value.as_table() {
            if !table.is_empty() {
                // Filter out deprecated keys and warn about them
                let mut unknown_keys = Vec::new();
                for key in table.keys() {
                    if is_deprecated(section, key) {
                        eprintln!("Warning: Configuration key '{}.{}' in config.toml is deprecated and will be ignored", section, key);
                    } else {
                        unknown_keys.push(key.clone());
                    }
                }

                // Error only if there are truly unknown keys
                if !unknown_keys.is_empty() {
                    return Err(FsPulseError::ConfigError(format!(
                        "Unknown configuration keys in section '{}': {}",
                        section,
                        unknown_keys.join(", ")
                    )));
                }
            }
        } else {
            // Unknown section entirely
            return Err(FsPulseError::ConfigError(format!(
                "Unknown configuration section: {}",
                section
            )));
        }
    }

    // Check environment map for unknown keys
    for (section, value) in env_map {
        if let Some(table) = value.as_table() {
            if !table.is_empty() {
                // Filter out deprecated keys and warn about them
                let mut unknown_keys = Vec::new();
                for key in table.keys() {
                    if is_deprecated(section, key) {
                        eprintln!("Warning: Environment variable 'FSPULSE_{}_{}'  is deprecated and will be ignored",
                            section.to_uppercase(),
                            key.to_uppercase());
                    } else {
                        unknown_keys.push(key.clone());
                    }
                }

                // Error only if there are truly unknown keys
                if !unknown_keys.is_empty() {
                    return Err(FsPulseError::ConfigError(format!(
                        "Unknown environment variables in section '{}': FSPULSE_{}_{}",
                        section,
                        section.to_uppercase(),
                        unknown_keys.join(", ")
                    )));
                }
            }
        }
    }

    Ok(())
}

// =============================================================================
// Config Implementation
// =============================================================================

impl Default for Config {
    fn default() -> Self {
        Self {
            data_dir: String::new(), // Set during load_config(), not here
            server_host: ConfigValue::new(
                "127.0.0.1".to_string(),
                ("server", "host"),
                true,
                validate_host,
            ),
            server_port: ConfigValue::new(8080, ("server", "port"), true, validate_port),
            analysis_threads: ConfigValue::new(8, ("analysis", "threads"), false, validate_threads),
            logging_fspulse: ConfigValue::new(
                "info".to_string(),
                ("logging", "fspulse"),
                true,
                validate_log_level,
            ),
            logging_lopdf: ConfigValue::new(
                "error".to_string(),
                ("logging", "lopdf"),
                true,
                validate_log_level,
            ),
            database_dir: ConfigValue::new(
                String::new(), // Empty string = use data_dir
                ("database", "dir"),
                true,
                validate_database_dir,
            ),
        }
    }
}

impl Config {
    /// Loads the configuration from a TOML file located in your app's data directory.
    ///
    /// Configuration is loaded in the following precedence order (highest to lowest):
    /// 1. Environment variables (FSPULSE_<SECTION>_<FIELD>)
    /// 2. TOML configuration file
    /// 3. Built-in defaults
    ///
    /// This function initializes the global CONFIG and fails fast on any error.
    ///
    /// Docker Support: Checks FSPULSE_DATA_DIR environment variable first,
    /// then falls back to OS-specific directories.
    pub fn load_config(project_dirs: &ProjectDirs) -> Result<(), FsPulseError> {
        // Step 1: Load ENV map from Figment
        let env_figment = Figment::from(Env::prefixed("FSPULSE_").split("_"));
        let mut env_map = match env_figment.extract::<toml::Value>() {
            Ok(toml::Value::Table(table)) => table,
            Ok(_) => toml::map::Map::new(), // Shouldn't happen but handle gracefully
            Err(e) => {
                return Err(FsPulseError::ConfigError(format!(
                    "Failed to parse environment variables: {}",
                    e
                )));
            }
        };

        // Step 2: "Take" FSPULSE_DATA_DIR from env_map (becomes ["data"]["dir"])
        //         This prevents unknown key warning later
        let data_dir = if let Some(data_section) = env_map.get_mut("data") {
            if let Some(fields) = data_section.as_table_mut() {
                fields
                    .remove("dir")
                    .map(|v| extract_string(&v))
                    .unwrap_or_else(|| project_dirs.data_local_dir().to_string_lossy().to_string())
            } else {
                project_dirs.data_local_dir().to_string_lossy().to_string()
            }
        } else {
            project_dirs.data_local_dir().to_string_lossy().to_string()
        };

        // Step 3: NOW we can construct config path and load TOML
        let config_path = PathBuf::from(&data_dir).join("config.toml");

        if !config_path.exists() {
            create_default_config_file(&config_path)?;
        }

        let toml_figment = Figment::from(Toml::file(&config_path));
        let mut toml_map = match toml_figment.extract::<toml::Value>() {
            Ok(toml::Value::Table(table)) => table,
            Ok(_) => {
                return Err(FsPulseError::ConfigError(
                    "Config file is not a valid TOML table".to_string(),
                ));
            }
            Err(e) => {
                return Err(FsPulseError::ConfigError(format!(
                    "Failed to parse config file {}: {}",
                    config_path.display(),
                    e
                )));
            }
        };

        // Step 4: Initialize config and store data_dir
        let mut config = Config {
            data_dir,
            ..Config::default()
        };

        // Step 5: Tell each property to take its values
        config.server_host.take(&mut toml_map, &mut env_map)?;
        config.server_port.take(&mut toml_map, &mut env_map)?;
        config.analysis_threads.take(&mut toml_map, &mut env_map)?;
        config.logging_fspulse.take(&mut toml_map, &mut env_map)?;
        config.logging_lopdf.take(&mut toml_map, &mut env_map)?;
        config.database_dir.take(&mut toml_map, &mut env_map)?;

        // Step 6: Check for unknown keys
        check_for_unknown_keys(&toml_map, &env_map)?;

        // Step 7: Initialize the global CONFIG
        CONFIG
            .set(RwLock::new(config))
            .map_err(|_| FsPulseError::ConfigError("Config already initialized".to_string()))?;

        Ok(())
    }

    // =========================================================================
    // Lock Acquisition Helpers
    // =========================================================================

    /// Execute a closure with read access to the config
    fn with_config_read<F, R>(f: F) -> R
    where
        F: FnOnce(&Config) -> R,
    {
        let lock = CONFIG.get().expect("Config not initialized");
        let config = lock.read().expect("Failed to acquire config read lock");
        f(&config)
    }

    /// Execute a closure with write access to the config
    fn with_config_write<F, R>(f: F) -> R
    where
        F: FnOnce(&mut Config) -> R,
    {
        let lock = CONFIG.get().expect("Config not initialized");
        let mut config = lock.write().expect("Failed to acquire config write lock");
        f(&mut config)
    }

    // =========================================================================
    // Public Accessor and Mutator Functions
    // =========================================================================

    // Server Host

    pub fn get_server_host() -> String {
        Self::with_config_read(|config| config.server_host.get().clone())
    }

    pub fn get_server_host_value() -> ConfigValue<String> {
        Self::with_config_read(|config| config.server_host.clone())
    }

    pub fn set_server_host(host: String, project_dirs: &ProjectDirs) -> Result<(), FsPulseError> {
        let config_path = get_config_path(project_dirs);
        Self::with_config_write(|config| config.server_host.set_file_value(host, &config_path))
    }

    pub fn delete_server_host(project_dirs: &ProjectDirs) -> Result<(), FsPulseError> {
        let config_path = get_config_path(project_dirs);
        Self::with_config_write(|config| config.server_host.delete_file_value(&config_path))
    }

    // Server Port

    pub fn get_server_port() -> u16 {
        Self::with_config_read(|config| *config.server_port.get())
    }

    pub fn get_server_port_value() -> ConfigValue<u16> {
        Self::with_config_read(|config| config.server_port.clone())
    }

    pub fn set_server_port(port: u16, project_dirs: &ProjectDirs) -> Result<(), FsPulseError> {
        let config_path = get_config_path(project_dirs);
        Self::with_config_write(|config| config.server_port.set_file_value(port, &config_path))
    }

    pub fn delete_server_port(project_dirs: &ProjectDirs) -> Result<(), FsPulseError> {
        let config_path = get_config_path(project_dirs);
        Self::with_config_write(|config| config.server_port.delete_file_value(&config_path))
    }

    // Analysis Threads

    pub fn get_analysis_threads() -> usize {
        Self::with_config_read(|config| *config.analysis_threads.get())
    }

    pub fn get_analysis_threads_value() -> ConfigValue<usize> {
        Self::with_config_read(|config| config.analysis_threads.clone())
    }

    pub fn set_analysis_threads(
        threads: usize,
        project_dirs: &ProjectDirs,
    ) -> Result<(), FsPulseError> {
        let config_path = get_config_path(project_dirs);
        Self::with_config_write(|config| {
            config
                .analysis_threads
                .set_file_value(threads, &config_path)
        })
    }

    pub fn delete_analysis_threads(project_dirs: &ProjectDirs) -> Result<(), FsPulseError> {
        let config_path = get_config_path(project_dirs);
        Self::with_config_write(|config| config.analysis_threads.delete_file_value(&config_path))
    }

    // Logging FsPulse

    pub fn get_logging_fspulse() -> String {
        Self::with_config_read(|config| config.logging_fspulse.get().clone())
    }

    pub fn get_logging_fspulse_value() -> ConfigValue<String> {
        Self::with_config_read(|config| config.logging_fspulse.clone())
    }

    pub fn set_logging_fspulse(
        level: String,
        project_dirs: &ProjectDirs,
    ) -> Result<(), FsPulseError> {
        let config_path = get_config_path(project_dirs);
        Self::with_config_write(|config| config.logging_fspulse.set_file_value(level, &config_path))
    }

    pub fn delete_logging_fspulse(project_dirs: &ProjectDirs) -> Result<(), FsPulseError> {
        let config_path = get_config_path(project_dirs);
        Self::with_config_write(|config| config.logging_fspulse.delete_file_value(&config_path))
    }

    // Logging LoPDF

    pub fn get_logging_lopdf() -> String {
        Self::with_config_read(|config| config.logging_lopdf.get().clone())
    }

    pub fn get_logging_lopdf_value() -> ConfigValue<String> {
        Self::with_config_read(|config| config.logging_lopdf.clone())
    }

    pub fn set_logging_lopdf(
        level: String,
        project_dirs: &ProjectDirs,
    ) -> Result<(), FsPulseError> {
        let config_path = get_config_path(project_dirs);
        Self::with_config_write(|config| config.logging_lopdf.set_file_value(level, &config_path))
    }

    pub fn delete_logging_lopdf(project_dirs: &ProjectDirs) -> Result<(), FsPulseError> {
        let config_path = get_config_path(project_dirs);
        Self::with_config_write(|config| config.logging_lopdf.delete_file_value(&config_path))
    }

    // Database Directory

    pub fn get_database_dir() -> String {
        Self::with_config_read(|config| config.database_dir.get().clone())
    }

    pub fn get_database_dir_value() -> ConfigValue<String> {
        Self::with_config_read(|config| config.database_dir.clone())
    }

    pub fn set_database_dir(dir: String, project_dirs: &ProjectDirs) -> Result<(), FsPulseError> {
        let config_path = get_config_path(project_dirs);
        Self::with_config_write(|config| config.database_dir.set_file_value(dir, &config_path))
    }

    pub fn delete_database_dir(project_dirs: &ProjectDirs) -> Result<(), FsPulseError> {
        let config_path = get_config_path(project_dirs);
        Self::with_config_write(|config| config.database_dir.delete_file_value(&config_path))
    }

    // Data Directory (special, not a ConfigValue - read-only)

    pub fn get_data_dir() -> String {
        Self::with_config_read(|config| config.data_dir.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use figment::Jail;
    use serial_test::serial;
    use std::fs;

    #[test]
    #[serial]
    fn test_config_value_precedence() {
        let mut cv = ConfigValue::new(42, ("test", "value"), true, |v, _| {
            extract_usize(v).map_err(FsPulseError::ConfigError)
        });

        // Default only
        assert_eq!(*cv.get(), 42);

        // File value overrides default
        cv.file_value = Some(10);
        assert_eq!(*cv.get(), 10);

        // File original overrides file
        cv.file_value_original = Some(20);
        assert_eq!(*cv.get(), 20);

        // Env overrides everything
        cv.env_value = Some(30);
        assert_eq!(*cv.get(), 30);
    }

    #[test]
    #[serial]
    fn test_config_value_synchronization_requires_restart() {
        Jail::expect_with(|jail| {
            let config_path = jail.directory().join("config.toml");

            // Create a ConfigValue that requires restart
            let mut cv = ConfigValue::new(
                "default".to_string(),
                ("test", "value"),
                true, // requires_restart = true
                validate_log_level,
            );

            // Set initial file value
            cv.set_file_value("info".to_string(), &config_path).unwrap();
            assert_eq!(cv.file_value, Some("info".to_string()));
            assert_eq!(cv.file_value_original, None); // Should NOT sync when requires_restart=true

            // Verify TOML was written
            let toml_content = fs::read_to_string(&config_path).unwrap();
            assert!(toml_content.contains("info"));

            Ok(())
        });
    }

    #[test]
    #[serial]
    fn test_config_value_synchronization_no_restart() {
        Jail::expect_with(|jail| {
            let config_path = jail.directory().join("config.toml");

            // Create a ConfigValue that does NOT require restart
            let mut cv = ConfigValue::new(
                8,
                ("analysis", "threads"),
                false, // requires_restart = false
                validate_threads,
            );

            // Set file value
            cv.set_file_value(12, &config_path).unwrap();
            assert_eq!(cv.file_value, Some(12));
            assert_eq!(cv.file_value_original, Some(12)); // SHOULD sync when requires_restart=false

            // Delete file value
            cv.delete_file_value(&config_path).unwrap();
            assert_eq!(cv.file_value, None);
            assert_eq!(cv.file_value_original, None); // SHOULD sync on delete too

            Ok(())
        });
    }

    /// Comprehensive test that initializes CONFIG and tests all basic operations
    /// This must run FIRST as CONFIG can only be initialized once per test run
    #[test]
    #[serial]
    fn test_config_integration() {
        Jail::expect_with(|jail| {
            // Skip if CONFIG is already initialized (from other tests)
            if CONFIG.get().is_some() {
                return Ok(());
            }

            let project_dirs = ProjectDirs::from("", "", "fspulse-test").unwrap();
            let dir = jail.directory().to_str().unwrap().to_string();
            jail.set_env("FSPULSE_DATA_DIR", &dir);

            // Test 1: Load config with defaults only
            Config::load_config(&project_dirs).unwrap();

            assert_eq!(Config::get_server_host(), "127.0.0.1");
            assert_eq!(Config::get_server_port(), 8080);
            assert_eq!(Config::get_analysis_threads(), 8);
            assert_eq!(Config::get_logging_fspulse(), "info");
            assert_eq!(Config::get_logging_lopdf(), "error");
            assert_eq!(Config::get_database_dir(), "");

            // Test 2: Set and delete a value
            Config::set_analysis_threads(16, &project_dirs).unwrap();
            assert_eq!(Config::get_analysis_threads(), 16);

            // Verify it was written to TOML
            let config_path = jail.directory().join("config.toml");
            let toml_content = fs::read_to_string(&config_path).unwrap();
            assert!(toml_content.contains("threads = 16"));

            // Delete the value
            Config::delete_analysis_threads(&project_dirs).unwrap();
            assert_eq!(Config::get_analysis_threads(), 8); // Back to default

            // Test 3: Verify file_value_original tracking for no-restart properties
            Config::set_analysis_threads(12, &project_dirs).unwrap();
            let threads_value = Config::get_analysis_threads_value();
            assert_eq!(threads_value.file_value, Some(12));
            assert_eq!(threads_value.file_value_original, Some(12)); // Synced!
            assert_eq!(Config::get_analysis_threads(), 12);

            Ok(())
        });
    }

    #[test]
    #[serial]
    fn test_load_config_unknown_key_in_toml() {
        Jail::expect_with(|jail| {
            // Tests run serially to avoid CONFIG conflicts

            jail.create_file("config.toml", r#"
[server]
host = "0.0.0.0"
unknown_field = "value"
"#)?;

            let project_dirs = ProjectDirs::from("", "", "fspulse-test").unwrap();
            let dir = jail.directory().to_str().unwrap().to_string(); jail.set_env("FSPULSE_DATA_DIR", &dir);

            let result = Config::load_config(&project_dirs);
            assert!(result.is_err());
            let err_msg = format!("{}", result.unwrap_err());
            assert!(err_msg.contains("Unknown configuration keys"));
            assert!(err_msg.contains("unknown_field"));

            Ok(())
        });
    }

    #[test]
    #[serial]
    fn test_load_config_unknown_env_var() {
        Jail::expect_with(|jail| {
            // Tests run serially to avoid CONFIG conflicts

            let dir = jail.directory().to_str().unwrap().to_string(); jail.set_env("FSPULSE_DATA_DIR", &dir);
            jail.set_env("FSPULSE_SERVER_UNKNOWN", "value");

            let project_dirs = ProjectDirs::from("", "", "fspulse-test").unwrap();
            let result = Config::load_config(&project_dirs);

            assert!(result.is_err());
            let err_msg = format!("{}", result.unwrap_err());
            assert!(err_msg.contains("Unknown environment variables"));

            Ok(())
        });
    }

    #[test]
    #[serial]
    fn test_validation_threads_range() {
        Jail::expect_with(|jail| {
            // Tests run serially to avoid CONFIG conflicts

            // Too low
            jail.create_file("config.toml", r#"
[analysis]
threads = 0
"#)?;

            let project_dirs = ProjectDirs::from("", "", "fspulse-test").unwrap();
            let dir = jail.directory().to_str().unwrap().to_string(); jail.set_env("FSPULSE_DATA_DIR", &dir);

            let result = Config::load_config(&project_dirs);
            assert!(result.is_err());
            assert!(format!("{}", result.unwrap_err()).contains("between 1 and 24"));

            // Tests run serially to avoid CONFIG conflicts

            // Too high
            jail.create_file("config.toml", r#"
[analysis]
threads = 100
"#)?;

            let result = Config::load_config(&project_dirs);
            assert!(result.is_err());
            assert!(format!("{}", result.unwrap_err()).contains("between 1 and 24"));

            Ok(())
        });
    }

    #[test]
    #[serial]
    fn test_validation_log_level() {
        Jail::expect_with(|jail| {
            // Tests run serially to avoid CONFIG conflicts

            jail.create_file("config.toml", r#"
[logging]
fspulse = "invalid_level"
"#)?;

            let project_dirs = ProjectDirs::from("", "", "fspulse-test").unwrap();
            let dir = jail.directory().to_str().unwrap().to_string(); jail.set_env("FSPULSE_DATA_DIR", &dir);

            let result = Config::load_config(&project_dirs);
            assert!(result.is_err());
            let err_msg = format!("{}", result.unwrap_err());
            assert!(err_msg.contains("Invalid log level"));
            assert!(err_msg.contains("invalid_level"));

            Ok(())
        });
    }

    #[test]
    #[serial]
    fn test_validation_port_zero() {
        Jail::expect_with(|jail| {
            // Tests run serially to avoid CONFIG conflicts

            jail.create_file("config.toml", r#"
[server]
port = 0
"#)?;

            let project_dirs = ProjectDirs::from("", "", "fspulse-test").unwrap();
            let dir = jail.directory().to_str().unwrap().to_string(); jail.set_env("FSPULSE_DATA_DIR", &dir);

            let result = Config::load_config(&project_dirs);
            assert!(result.is_err());
            assert!(format!("{}", result.unwrap_err()).contains("cannot be 0"));

            Ok(())
        });
    }

    #[test]
    #[serial]
    fn test_validation_empty_host() {
        Jail::expect_with(|jail| {
            // Tests run serially to avoid CONFIG conflicts

            jail.create_file("config.toml", r#"
[server]
host = ""
"#)?;

            let project_dirs = ProjectDirs::from("", "", "fspulse-test").unwrap();
            let dir = jail.directory().to_str().unwrap().to_string(); jail.set_env("FSPULSE_DATA_DIR", &dir);

            let result = Config::load_config(&project_dirs);
            assert!(result.is_err());
            assert!(format!("{}", result.unwrap_err()).contains("cannot be empty"));

            Ok(())
        });
    }
}
