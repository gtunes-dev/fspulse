use axum::{
    http::StatusCode,
    response::Html,
    routing::{get, post, put},
    Router,
};
use std::net::SocketAddr;
use tokio::net::TcpListener;

use crate::error::FsPulseError;
use crate::api;

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
        // Create shared application state (used by both old and new scan handlers)
        let app_state = api::scans::AppState::new();

        let app = Router::new()
            // Static routes
            .route("/", get(handlers::home::dashboard))
            .route("/health", get(health_check))

            // API routes - OLD handlers (for monolith UI)
            .route("/api/status", get(handlers::home::api_status))
            .route("/api/home/last-scan-stats", get(handlers::home::get_last_scan_stats))
            .route("/api/home/scan-stats/{scan_id}", get(handlers::home::get_scan_stats))
            .route("/api/alerts", get(handlers::alerts::list_alerts))
            .route("/api/activity", get(handlers::activity::recent_activity))
            .route("/api/scans/status", get(handlers::scans::get_scans_status))

            // API routes - NEW handlers (for React UI)
            // Query endpoints
            .route("/api/query/{domain}/metadata", get(api::query::get_metadata))
            .route("/api/query/{domain}/count", post(api::query::count_query))
            .route("/api/query/{domain}/fetch", post(api::query::fetch_query))
            .route("/api/query/execute", post(api::query::execute_raw_query))
            .route("/api/validate-filter", post(api::query::validate_filter))

            // Alert endpoints
            .route("/api/alerts/{alert_id}/status", put(api::alerts::update_alert_status))

            // Scan endpoints
            .route("/api/scans/start", post(api::scans::initiate_scan))
            .route("/api/scans/current", get(api::scans::get_current_scan))
            .route("/api/scans/{scan_id}/cancel", post(api::scans::cancel_scan))

            // Root endpoints
            .route("/api/roots", post(api::roots::create_root))
            .route("/api/roots/with-scans", get(api::roots::get_roots_with_scans))

            // WebSocket routes
            .route("/ws/scans/progress", get(api::scans::scan_progress_ws))

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