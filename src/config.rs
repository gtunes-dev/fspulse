use std::env;
use std::fs;
use std::path::PathBuf;
use std::sync::RwLock;

use directories::ProjectDirs;
use figment::{
    providers::{Env, Format, Toml},
    Figment,
};
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
    pub active_file_value: Option<T>,
    pub default_value: T,

    path: (&'static str, &'static str),
    requires_restart: bool,
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
            active_file_value: None,
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
            .or(self.active_file_value.as_ref())
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

        // If restart required and file value is currently effective, preserve it
        if self.requires_restart && self.env_value.is_none() {
            self.active_file_value = self.file_value.clone();
        }

        // Write to config file
        self.write_to_toml(config_path, &new_value)?;

        // Update file value
        self.file_value = Some(new_value);

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
                .map_err(|e| FsPulseError::ConfigError(e))?
                .as_table()
                .cloned()
                .unwrap_or_else(|| toml::map::Map::new())
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
        let toml_value = toml::Value::try_from(value)
            .map_err(|e| FsPulseError::ConfigError(format!("Failed to convert value to TOML: {}", e)))?;
        section_table.insert(field.to_string(), toml_value);

        // Write back to file
        write_toml(config_path, &toml::Value::Table(config_table))
            .map_err(|e| FsPulseError::ConfigError(e))
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

fn validate_log_level(
    value: &toml::Value,
    source: ConfigSource,
) -> Result<String, FsPulseError> {
    let level = extract_string(value).to_lowercase();
    if !is_valid_log_level(&level) {
        return Err(FsPulseError::ConfigError(format!(
            "Invalid log level '{}' from {:?}. Must be one of: error, warn, info, debug, trace",
            level, source
        )));
    }
    Ok(level)
}

fn validate_threads(
    value: &toml::Value,
    source: ConfigSource,
) -> Result<usize, FsPulseError> {
    let threads = extract_usize(value).map_err(|e| {
        FsPulseError::ConfigError(format!("analysis.threads {}, from {:?}", e, source))
    })?;

    if threads < MIN_ANALYSIS_THREADS || threads > MAX_ANALYSIS_THREADS {
        return Err(FsPulseError::ConfigError(format!(
            "analysis.threads must be between {} and {}, got {} from {:?}",
            MIN_ANALYSIS_THREADS, MAX_ANALYSIS_THREADS, threads, source
        )));
    }
    Ok(threads)
}

fn validate_port(value: &toml::Value, source: ConfigSource) -> Result<u16, FsPulseError> {
    let port = extract_u16(value).map_err(|e| {
        FsPulseError::ConfigError(format!("server.port {}, from {:?}", e, source))
    })?;

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

    fs::write(config_path, template).map_err(|e| {
        FsPulseError::ConfigError(format!("Failed to write config file: {}", e))
    })
}

/// Check for unknown keys in TOML and environment maps
fn check_for_unknown_keys(
    toml_map: &toml::Table,
    env_map: &toml::Table,
) -> Result<(), FsPulseError> {
    // Check TOML map for unknown sections or keys
    for (section, value) in toml_map {
        if let Some(table) = value.as_table() {
            if !table.is_empty() {
                // There are leftover keys in this section
                let keys: Vec<String> = table.keys().cloned().collect();
                return Err(FsPulseError::ConfigError(format!(
                    "Unknown configuration keys in section '{}': {}",
                    section,
                    keys.join(", ")
                )));
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
                let keys: Vec<String> = table.keys().cloned().collect();
                return Err(FsPulseError::ConfigError(format!(
                    "Unknown environment variables in section '{}': FSPULSE_{}_{}",
                    section,
                    section.to_uppercase(),
                    keys.join(", ")
                )));
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
            data_dir: String::new(),  // Set during load_config(), not here
            server_host: ConfigValue::new(
                "127.0.0.1".to_string(),
                ("server", "host"),
                true,
                validate_host,
            ),
            server_port: ConfigValue::new(
                8080,
                ("server", "port"),
                true,
                validate_port,
            ),
            analysis_threads: ConfigValue::new(
                8,
                ("analysis", "threads"),
                false,
                validate_threads,
            ),
            logging_fspulse: ConfigValue::new(
                "info".to_string(),
                ("logging", "fspulse"),
                false,
                validate_log_level,
            ),
            logging_lopdf: ConfigValue::new(
                "error".to_string(),
                ("logging", "lopdf"),
                false,
                validate_log_level,
            ),
            database_dir: ConfigValue::new(
                String::new(),  // Empty string = use data_dir
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
                    "Failed to parse environment variables: {}", e
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
                    .unwrap_or_else(|| {
                        project_dirs
                            .data_local_dir()
                            .to_string_lossy()
                            .to_string()
                    })
            } else {
                project_dirs
                    .data_local_dir()
                    .to_string_lossy()
                    .to_string()
            }
        } else {
            project_dirs
                .data_local_dir()
                .to_string_lossy()
                .to_string()
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
                    "Config file is not a valid TOML table".to_string()
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
        let mut config = Config::default();
        config.data_dir = data_dir;

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
        CONFIG.set(RwLock::new(config))
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
    // Public Accessor Functions
    // =========================================================================

    // Server Configuration

    pub fn get_server_host() -> String {
        Self::with_config_read(|config| config.server_host.get().clone())
    }

    pub fn get_server_host_value() -> ConfigValue<String> {
        Self::with_config_read(|config| config.server_host.clone())
    }

    pub fn get_server_port() -> u16 {
        Self::with_config_read(|config| *config.server_port.get())
    }

    pub fn get_server_port_value() -> ConfigValue<u16> {
        Self::with_config_read(|config| config.server_port.clone())
    }

    // Analysis Configuration

    pub fn get_analysis_threads() -> usize {
        Self::with_config_read(|config| *config.analysis_threads.get())
    }

    pub fn get_analysis_threads_value() -> ConfigValue<usize> {
        Self::with_config_read(|config| config.analysis_threads.clone())
    }

    // Logging Configuration

    pub fn get_logging_fspulse() -> String {
        Self::with_config_read(|config| config.logging_fspulse.get().clone())
    }

    pub fn get_logging_fspulse_value() -> ConfigValue<String> {
        Self::with_config_read(|config| config.logging_fspulse.clone())
    }

    pub fn get_logging_lopdf() -> String {
        Self::with_config_read(|config| config.logging_lopdf.get().clone())
    }

    pub fn get_logging_lopdf_value() -> ConfigValue<String> {
        Self::with_config_read(|config| config.logging_lopdf.clone())
    }

    // Database Configuration

    pub fn get_database_dir() -> String {
        Self::with_config_read(|config| config.database_dir.get().clone())
    }

    pub fn get_database_dir_value() -> ConfigValue<String> {
        Self::with_config_read(|config| config.database_dir.clone())
    }

    // Data Directory (special, not a ConfigValue)

    pub fn get_data_dir() -> String {
        Self::with_config_read(|config| config.data_dir.clone())
    }

    // =========================================================================
    // Setter Functions (for runtime updates)
    // =========================================================================

    /// Update analysis threads at runtime
    /// Returns (took_effect, message)
    pub fn set_analysis_threads(
        threads: usize,
        project_dirs: &ProjectDirs,
    ) -> Result<(bool, String), String> {
        let config_path = get_config_path(project_dirs);

        Self::with_config_write(|config| {
            config
                .analysis_threads
                .set_file_value(threads, &config_path)
                .map_err(|e| format!("Failed to update configuration: {}", e))?;

            let took_effect = config.analysis_threads.env_value.is_none();

            let message = if took_effect {
                "Configuration updated successfully and is now in effect.".to_string()
            } else {
                "Configuration updated in config.toml but will not take effect until the FSPULSE_ANALYSIS_THREADS environment variable is removed and the application is restarted.".to_string()
            };

            Ok((took_effect, message))
        })
    }
}
