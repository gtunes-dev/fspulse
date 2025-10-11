use axum::{
    http::StatusCode,
    response::Html,
    routing::{get, post},
    Router,
};
use std::net::SocketAddr;
use std::path::PathBuf;
use tokio::net::TcpListener;
use tower::ServiceBuilder;

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

    pub async fn start(&self, db_path: Option<PathBuf>) -> Result<(), FsPulseError> {
        let app = self.create_router(db_path)?;

        let addr: SocketAddr = format!("{}:{}", self.host, self.port)
            .parse()
            .map_err(|e| FsPulseError::Error(format!("Invalid address: {}", e)))?;

        println!("🚀 FsPulse web server starting on http://{}", addr);

        let listener = TcpListener::bind(addr).await
            .map_err(|e| FsPulseError::Error(format!("Failed to bind to {}: {}", addr, e)))?;

        axum::serve(listener, app).await
            .map_err(|e| FsPulseError::Error(format!("Server error: {}", e)))?;

        Ok(())
    }

    fn create_router(&self, db_path: Option<PathBuf>) -> Result<Router, FsPulseError> {
        // Store database path for handlers to create connections as needed
        let app = Router::new()
            // Static routes
            .route("/", get(handlers::overview::dashboard))
            .route("/health", get(health_check))

            // API routes
            .route("/api/status", get(handlers::overview::api_status))
            .route("/api/alerts", get(handlers::alerts::list_alerts))
            .route("/api/activity", get(handlers::activity::recent_activity))
            .route("/api/metadata/{domain}", get(handlers::metadata::get_metadata))
            .route("/api/query/{domain}", post(handlers::query::execute_query))

            // Store database path in extension for handlers
            .layer(
                ServiceBuilder::new()
                    .layer(axum::Extension(db_path))
            );

        Ok(app)
    }
}

async fn health_check() -> Result<(StatusCode, Html<String>), StatusCode> {
    Ok((
        StatusCode::OK,
        Html("<h1>FsPulse Web Server</h1><p>✅ Server is running</p>".to_string()),
    ))
}