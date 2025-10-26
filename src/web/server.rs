use axum::{
    http::StatusCode,
    response::Html,
    routing::{get, post, put},
    Router,
};
use std::net::SocketAddr;
use tokio::net::TcpListener;

use crate::error::FsPulseError;

use super::handlers;

pub struct WebServer {
    host: String,
    port: u16,
}

impl WebServer {
    pub fn new(host: String, port: u16) -> Self {
        Self { host, port }
    }

    pub async fn start(&self) -> Result<(), FsPulseError> {
        let app = self.create_router()?;

        let addr: SocketAddr = format!("{}:{}", self.host, self.port)
            .parse()
            .map_err(|e| FsPulseError::Error(format!("Invalid address: {}", e)))?;

        println!("🚀 FsPulse server starting on http://{}", addr);

        let listener = TcpListener::bind(addr).await
            .map_err(|e| FsPulseError::Error(format!("Failed to bind to {}: {}", addr, e)))?;

        axum::serve(listener, app).await
            .map_err(|e| FsPulseError::Error(format!("Server error: {}", e)))?;

        Ok(())
    }

    fn create_router(&self) -> Result<Router, FsPulseError> {
        // Create shared application state
        let app_state = handlers::scans::AppState::new();

        let app = Router::new()
            // Static routes
            .route("/", get(handlers::home::dashboard))
            .route("/health", get(health_check))

            // API routes
            .route("/api/status", get(handlers::home::api_status))
            .route("/api/home/last-scan-stats", get(handlers::home::get_last_scan_stats))
            .route("/api/home/scan-stats/{scan_id}", get(handlers::home::get_scan_stats))
            .route("/api/alerts", get(handlers::alerts::list_alerts))
            .route("/api/alerts/{alert_id}/status", put(handlers::alerts::update_alert_status))
            .route("/api/activity", get(handlers::activity::recent_activity))
            .route("/api/metadata/{domain}", get(handlers::metadata::get_metadata))
            .route("/api/query/execute", post(handlers::query::execute_raw_query))
            .route("/api/query/{domain}", post(handlers::query::execute_query))
            .route("/api/validate-filter", post(handlers::query::validate_filter))
            .route("/api/scans/status", get(handlers::scans::get_scans_status))
            .route("/api/scans/start", post(handlers::scans::initiate_scan))
            .route("/api/scans/current", get(handlers::scans::get_current_scan))
            .route("/api/scans/{scan_id}/cancel", post(handlers::scans::cancel_scan))
            .route("/api/roots", post(handlers::roots::create_root))
            .route("/api/roots/with-scans", get(handlers::roots::get_roots_with_scans))

            // WebSocket routes
            .route("/ws/scans/progress", get(handlers::scans::scan_progress_ws))

            // Add state for handlers
            .with_state(app_state);

        Ok(app)
    }
}

async fn health_check() -> Result<(StatusCode, Html<String>), StatusCode> {
    Ok((
        StatusCode::OK,
        Html("<h1>FsPulse Server</h1><p>✅ Server is running</p>".to_string()),
    ))
}