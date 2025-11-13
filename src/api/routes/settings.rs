use axum::{extract::State, http::StatusCode, response::IntoResponse, Json};
use serde::{Deserialize, Serialize};
use std::env;
use directories::ProjectDirs;

use crate::config::{self, CONFIG};
use crate::api::scans::AppState;

/// Represents a single configuration setting with its source information
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ConfigSetting<T> {
    /// Value from config.toml file
    pub config_value: T,
    /// Actual effective value (after environment variable overrides)
    pub effective_value: T,
    /// Source of the effective value: "config", "environment", or "default"
    pub source: String,
    /// Environment variable name that can override this setting
    pub env_var: String,
    /// Whether this setting can be edited in the UI (false if env var is set)
    pub editable: bool,
}

/// Response structure for GET /api/settings
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SettingsResponse {
    pub analysis: AnalysisSettings,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct AnalysisSettings {
    pub threads: ConfigSetting<usize>,
}

/// Request structure for PUT /api/settings
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SettingsUpdateRequest {
    pub analysis: Option<AnalysisUpdateRequest>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct AnalysisUpdateRequest {
    pub threads: Option<usize>,
}

/// GET /api/settings
/// Returns current configuration settings with source information
pub async fn get_settings() -> Result<Json<SettingsResponse>, (StatusCode, String)> {
    let config = CONFIG.get().ok_or_else(|| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Configuration not initialized".to_string(),
        )
    })?;

    // The runtime config has provenance information built-in
    let threads_env_var = "FSPULSE_ANALYSIS_THREADS";
    let is_from_env = config.analysis.threads.source == crate::config::ConfigSource::Environment;

    // If value is from environment, load the file to get the config_value
    // Otherwise, the effective value IS the config value
    let config_value = if is_from_env {
        // Load TOML to get the config file value (not the env override)
        let project_dirs = ProjectDirs::from("", "", "fspulse").ok_or_else(|| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to determine config directory".to_string(),
            )
        })?;
        let config_path = config::get_config_path(&project_dirs);
        let toml_value = config::load_toml_only(&config_path).map_err(|e| {
            (StatusCode::INTERNAL_SERVER_ERROR, e)
        })?;

        // Extract threads from TOML, or use default if not present
        toml_value
            .get("analysis")
            .and_then(|a| a.get("threads"))
            .and_then(|t| t.as_integer())
            .map(|t| t as usize)
            .unwrap_or_else(|| config::AnalysisConfig::default().threads.value)
    } else {
        config.analysis.threads.value
    };

    let source_str = match config.analysis.threads.source {
        crate::config::ConfigSource::Environment => "environment",
        crate::config::ConfigSource::ConfigFile => "config",
        crate::config::ConfigSource::Default => "default",
    };

    let response = SettingsResponse {
        analysis: AnalysisSettings {
            threads: ConfigSetting {
                config_value,
                effective_value: config.analysis.threads.value,
                source: source_str.to_string(),
                env_var: threads_env_var.to_string(),
                editable: !is_from_env,
            },
        },
    };

    Ok(Json(response))
}

/// PUT /api/settings
/// Updates configuration settings in config.toml
pub async fn update_settings(
    State(_state): State<AppState>,
    Json(request): Json<SettingsUpdateRequest>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    // Get the config file path using helper from config.rs
    let project_dirs = ProjectDirs::from("", "", "fspulse").ok_or_else(|| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Failed to determine config directory".to_string(),
        )
    })?;

    let config_path = config::get_config_path(&project_dirs);

    // Load existing TOML file using helper from config.rs
    let mut file_toml = config::load_toml_only(&config_path).map_err(|e| {
        (StatusCode::INTERNAL_SERVER_ERROR, e)
    })?;

    // Apply updates
    let mut updated = false;

    if let Some(analysis_update) = request.analysis {
        if let Some(threads) = analysis_update.threads {
            // Validate threads value
            if threads < 1 || threads > 24 {
                return Err((
                    StatusCode::BAD_REQUEST,
                    "Threads must be between 1 and 24".to_string(),
                ));
            }

            // Check if this setting is overridden by environment variable
            if env::var("FSPULSE_ANALYSIS_THREADS").is_ok() {
                return Err((
                    StatusCode::CONFLICT,
                    "Cannot update analysis.threads: overridden by environment variable FSPULSE_ANALYSIS_THREADS".to_string(),
                ));
            }

            // Update the TOML value
            if let toml::Value::Table(ref mut table) = file_toml {
                let analysis_table = table
                    .entry("analysis".to_string())
                    .or_insert_with(|| toml::Value::Table(toml::map::Map::new()));

                if let toml::Value::Table(ref mut analysis) = analysis_table {
                    analysis.insert("threads".to_string(), toml::Value::Integer(threads as i64));
                }
            }
            updated = true;
        }
    }

    if !updated {
        return Err((
            StatusCode::BAD_REQUEST,
            "No valid settings provided to update".to_string(),
        ));
    }

    // Write updated config back to file using helper from config.rs
    config::write_toml(&config_path, &file_toml).map_err(|e| {
        (StatusCode::INTERNAL_SERVER_ERROR, e)
    })?;

    Ok((
        StatusCode::OK,
        "Configuration updated successfully. Restart required for changes to take effect.",
    ))
}
