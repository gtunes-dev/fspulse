use axum::{extract::State, http::StatusCode, response::IntoResponse, Json};
use serde::{Deserialize, Serialize};
use directories::ProjectDirs;

use crate::config::{self, MIN_ANALYSIS_THREADS, MAX_ANALYSIS_THREADS};
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
    pub logging: LoggingSettings,
    pub server: ServerSettings,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct AnalysisSettings {
    pub threads: ConfigSetting<usize>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct LoggingSettings {
    pub fspulse: ConfigSetting<String>,
    pub lopdf: ConfigSetting<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ServerSettings {
    pub host: ConfigSetting<String>,
    pub port: ConfigSetting<u16>,
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
    // Helper to compute effective source
    fn get_effective_source<T>(config_value: &crate::config::ConfigValue<T>) -> &str {
        if config_value.env_value.is_some() {
            "environment"
        } else if config_value.active_file_value.is_some() {
            "config"
        } else if config_value.file_value.is_some() {
            "config"
        } else {
            "default"
        }
    }

    // No locks visible here - all hidden in config module!

    // Analysis Threads
    let threads_value = config::Config::get_analysis_threads_value();
    let threads_setting = ConfigSetting {
        config_value: threads_value.file_value.unwrap_or(threads_value.default_value),
        effective_value: *threads_value.get(),
        source: get_effective_source(&threads_value).to_string(),
        env_var: "FSPULSE_ANALYSIS_THREADS".to_string(),
        editable: threads_value.env_value.is_none(),
    };

    // Logging FsPulse
    let fspulse_value = config::Config::get_logging_fspulse_value();
    let fspulse_setting = ConfigSetting {
        config_value: fspulse_value.file_value.clone().unwrap_or_else(|| fspulse_value.default_value.clone()),
        effective_value: fspulse_value.get().clone(),
        source: get_effective_source(&fspulse_value).to_string(),
        env_var: "FSPULSE_LOGGING_FSPULSE".to_string(),
        editable: fspulse_value.env_value.is_none(),
    };

    // Logging LoPDF
    let lopdf_value = config::Config::get_logging_lopdf_value();
    let lopdf_setting = ConfigSetting {
        config_value: lopdf_value.file_value.clone().unwrap_or_else(|| lopdf_value.default_value.clone()),
        effective_value: lopdf_value.get().clone(),
        source: get_effective_source(&lopdf_value).to_string(),
        env_var: "FSPULSE_LOGGING_LOPDF".to_string(),
        editable: lopdf_value.env_value.is_none(),
    };

    // Server Host
    let host_value = config::Config::get_server_host_value();
    let host_setting = ConfigSetting {
        config_value: host_value.file_value.clone().unwrap_or_else(|| host_value.default_value.clone()),
        effective_value: host_value.get().clone(),
        source: get_effective_source(&host_value).to_string(),
        env_var: "FSPULSE_SERVER_HOST".to_string(),
        editable: host_value.env_value.is_none(),
    };

    // Server Port
    let port_value = config::Config::get_server_port_value();
    let port_setting = ConfigSetting {
        config_value: port_value.file_value.unwrap_or(port_value.default_value),
        effective_value: *port_value.get(),
        source: get_effective_source(&port_value).to_string(),
        env_var: "FSPULSE_SERVER_PORT".to_string(),
        editable: port_value.env_value.is_none(),
    };

    let response = SettingsResponse {
        analysis: AnalysisSettings {
            threads: threads_setting,
        },
        logging: LoggingSettings {
            fspulse: fspulse_setting,
            lopdf: lopdf_setting,
        },
        server: ServerSettings {
            host: host_setting,
            port: port_setting,
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
    // Get project dirs
    let project_dirs = ProjectDirs::from("", "", "fspulse").ok_or_else(|| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Failed to determine config directory".to_string(),
        )
    })?;

    // Update analysis threads if provided
    if let Some(analysis_update) = request.analysis {
        if let Some(threads) = analysis_update.threads {
            // Validate threads value
            if threads < MIN_ANALYSIS_THREADS || threads > MAX_ANALYSIS_THREADS {
                return Err((
                    StatusCode::BAD_REQUEST,
                    format!("Threads must be between {} and {}", MIN_ANALYSIS_THREADS, MAX_ANALYSIS_THREADS),
                ));
            }

            // No locks visible - all hidden in config module!
            let (_took_effect, message) = config::Config::set_analysis_threads(threads, &project_dirs)
                .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;

            return Ok((StatusCode::OK, message));
        }
    }

    Err((
        StatusCode::BAD_REQUEST,
        "No valid settings provided to update".to_string(),
    ))
}
