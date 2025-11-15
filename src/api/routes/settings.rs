use axum::{extract::State, http::StatusCode, response::IntoResponse, Json};
use serde::{Deserialize, Serialize};
use directories::ProjectDirs;

use crate::config::{self, MIN_ANALYSIS_THREADS, MAX_ANALYSIS_THREADS};
use crate::api::scans::AppState;

/// Represents a single configuration setting with complete ConfigValue information
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ConfigSetting<T> {
    pub env_value: Option<T>,
    pub file_value: Option<T>,
    pub file_value_original: Option<T>,
    pub default_value: T,
    pub env_var: String,
    pub requires_restart: bool,
    pub editable: bool,
}

/// Response structure for GET /api/settings
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SettingsResponse {
    pub analysis_threads: ConfigSetting<usize>,
    pub logging_fspulse: ConfigSetting<String>,
    pub logging_lopdf: ConfigSetting<String>,
    pub server_host: ConfigSetting<String>,
    pub server_port: ConfigSetting<u16>,
    pub database_dir: ConfigSetting<String>,
}

/// Request structure for PUT /api/settings
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SettingsUpdateRequest {
    pub analysis_threads: Option<usize>,
    pub server_host: Option<String>,
    pub server_port: Option<u16>,
    pub logging_fspulse: Option<String>,
    pub logging_lopdf: Option<String>,
    pub database_dir: Option<String>,
}

/// Request structure for DELETE /api/settings
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SettingsDeleteRequest {
    pub setting_key: String,
}

/// GET /api/settings
/// Returns current configuration settings with complete ConfigValue information
pub async fn get_settings() -> Result<Json<SettingsResponse>, (StatusCode, String)> {
    // Analysis Threads
    let threads_value = config::Config::get_analysis_threads_value();
    let threads_setting = ConfigSetting {
        env_value: threads_value.env_value,
        file_value: threads_value.file_value,
        file_value_original: threads_value.file_value_original,
        default_value: threads_value.default_value,
        env_var: "FSPULSE_ANALYSIS_THREADS".to_string(),
        requires_restart: threads_value.requires_restart,
        editable: threads_value.env_value.is_none(),
    };

    // Logging FsPulse
    let fspulse_value = config::Config::get_logging_fspulse_value();
    let fspulse_setting = ConfigSetting {
        env_value: fspulse_value.env_value.clone(),
        file_value: fspulse_value.file_value.clone(),
        file_value_original: fspulse_value.file_value_original.clone(),
        default_value: fspulse_value.default_value.clone(),
        env_var: "FSPULSE_LOGGING_FSPULSE".to_string(),
        requires_restart: fspulse_value.requires_restart,
        editable: fspulse_value.env_value.is_none(),
    };

    // Logging LoPDF
    let lopdf_value = config::Config::get_logging_lopdf_value();
    let lopdf_setting = ConfigSetting {
        env_value: lopdf_value.env_value.clone(),
        file_value: lopdf_value.file_value.clone(),
        file_value_original: lopdf_value.file_value_original.clone(),
        default_value: lopdf_value.default_value.clone(),
        env_var: "FSPULSE_LOGGING_LOPDF".to_string(),
        requires_restart: lopdf_value.requires_restart,
        editable: lopdf_value.env_value.is_none(),
    };

    // Server Host
    let host_value = config::Config::get_server_host_value();
    let host_setting = ConfigSetting {
        env_value: host_value.env_value.clone(),
        file_value: host_value.file_value.clone(),
        file_value_original: host_value.file_value_original.clone(),
        default_value: host_value.default_value.clone(),
        env_var: "FSPULSE_SERVER_HOST".to_string(),
        requires_restart: host_value.requires_restart,
        editable: host_value.env_value.is_none(),
    };

    // Server Port
    let port_value = config::Config::get_server_port_value();
    let port_setting = ConfigSetting {
        env_value: port_value.env_value,
        file_value: port_value.file_value,
        file_value_original: port_value.file_value_original,
        default_value: port_value.default_value,
        env_var: "FSPULSE_SERVER_PORT".to_string(),
        requires_restart: port_value.requires_restart,
        editable: port_value.env_value.is_none(),
    };

    // Database Dir
    let dir_value = config::Config::get_database_dir_value();
    let dir_setting = ConfigSetting {
        env_value: dir_value.env_value.clone(),
        file_value: dir_value.file_value.clone(),
        file_value_original: dir_value.file_value_original.clone(),
        default_value: dir_value.default_value.clone(),
        env_var: "FSPULSE_DATABASE_DIR".to_string(),
        requires_restart: dir_value.requires_restart,
        editable: dir_value.env_value.is_none(),
    };

    let response = SettingsResponse {
        analysis_threads: threads_setting,
        logging_fspulse: fspulse_setting,
        logging_lopdf: lopdf_setting,
        server_host: host_setting,
        server_port: port_setting,
        database_dir: dir_setting,
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

    let mut updated = false;

    // Update server host if provided
    if let Some(host) = request.server_host {
        config::Config::set_server_host(host, &project_dirs)
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        updated = true;
    }

    // Update server port if provided
    if let Some(port) = request.server_port {
        config::Config::set_server_port(port, &project_dirs)
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        updated = true;
    }

    // Update analysis threads if provided
    if let Some(threads) = request.analysis_threads {
        // Validate threads value
        if !(MIN_ANALYSIS_THREADS..=MAX_ANALYSIS_THREADS).contains(&threads) {
            return Err((
                StatusCode::BAD_REQUEST,
                format!("Threads must be between {} and {}", MIN_ANALYSIS_THREADS, MAX_ANALYSIS_THREADS),
            ));
        }

        config::Config::set_analysis_threads(threads, &project_dirs)
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        updated = true;
    }

    // Update logging fspulse if provided
    if let Some(level) = request.logging_fspulse {
        config::Config::set_logging_fspulse(level, &project_dirs)
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        updated = true;
    }

    // Update logging lopdf if provided
    if let Some(level) = request.logging_lopdf {
        config::Config::set_logging_lopdf(level, &project_dirs)
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        updated = true;
    }

    // Update database dir if provided
    if let Some(dir) = request.database_dir {
        config::Config::set_database_dir(dir, &project_dirs)
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        updated = true;
    }

    if updated {
        Ok((StatusCode::OK, "Configuration updated successfully"))
    } else {
        Err((
            StatusCode::BAD_REQUEST,
            "No valid settings provided to update".to_string(),
        ))
    }
}

/// DELETE /api/settings
/// Deletes a configuration setting from config.toml
pub async fn delete_settings(
    State(_state): State<AppState>,
    Json(request): Json<SettingsDeleteRequest>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    // Get project dirs
    let project_dirs = ProjectDirs::from("", "", "fspulse").ok_or_else(|| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Failed to determine config directory".to_string(),
        )
    })?;

    // Match on setting_key and call appropriate delete function
    match request.setting_key.as_str() {
        "server_host" => {
            config::Config::delete_server_host(&project_dirs)
                .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        }
        "server_port" => {
            config::Config::delete_server_port(&project_dirs)
                .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        }
        "analysis_threads" => {
            config::Config::delete_analysis_threads(&project_dirs)
                .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        }
        "logging_fspulse" => {
            config::Config::delete_logging_fspulse(&project_dirs)
                .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        }
        "logging_lopdf" => {
            config::Config::delete_logging_lopdf(&project_dirs)
                .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        }
        "database_dir" => {
            config::Config::delete_database_dir(&project_dirs)
                .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        }
        _ => {
            return Err((
                StatusCode::BAD_REQUEST,
                format!("Unknown setting key: {}", request.setting_key),
            ));
        }
    };

    Ok((StatusCode::OK, "Setting deleted from config file"))
}
