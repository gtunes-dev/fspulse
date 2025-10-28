use axum::{
    http::StatusCode,
    response::Html,
    routing::{get, post, put},
    Router,
};

// These imports are only needed for production (embedded assets)
#[cfg(not(debug_assertions))]
use axum::{
    body::Body,
    http::{header, Uri},
    response::{IntoResponse, Response},
};
use std::net::SocketAddr;
use tokio::net::TcpListener;

use crate::error::FsPulseError;
use crate::api;

// Embed static files in release builds
#[cfg(not(debug_assertions))]
use rust_embed::RustEmbed;

#[cfg(not(debug_assertions))]
#[derive(RustEmbed)]
#[folder = "frontend/dist/"]
struct Asset;

// Use filesystem serving in debug builds
#[cfg(debug_assertions)]
use tower_http::services::ServeDir;

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

        #[cfg(debug_assertions)]
        println!("   Running in DEVELOPMENT mode - serving assets from frontend/dist/");

        #[cfg(not(debug_assertions))]
        println!("   Running in PRODUCTION mode - serving embedded assets");

        let listener = TcpListener::bind(addr).await
            .map_err(|e| FsPulseError::Error(format!("Failed to bind to {}: {}", addr, e)))?;

        axum::serve(listener, app).await
            .map_err(|e| FsPulseError::Error(format!("Server error: {}", e)))?;

        Ok(())
    }

    fn create_router(&self) -> Result<Router, FsPulseError> {
        // Create shared application state for scan management
        let app_state = api::scans::AppState::new();

        let app = Router::new()
            // Health check
            .route("/health", get(health_check))

            // Home/Dashboard API
            .route("/api/home/last-scan-stats", get(api::scans::get_last_scan_stats))

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

        // Serve static files differently based on build type
        #[cfg(debug_assertions)]
        {
            // Development: serve from filesystem for fast iteration
            let app = app.fallback_service(ServeDir::new("frontend/dist"));
            Ok(app)
        }

        #[cfg(not(debug_assertions))]
        {
            // Production: serve embedded files
            let app = app.fallback(static_handler);
            Ok(app)
        }
    }
}

async fn health_check() -> Result<(StatusCode, Html<String>), StatusCode> {
    Ok((
        StatusCode::OK,
        Html("<h1>FsPulse Server</h1><p>✅ Server is running</p>".to_string()),
    ))
}

// Handler for embedded static files (production builds only)
#[cfg(not(debug_assertions))]
async fn static_handler(uri: Uri) -> impl IntoResponse {
    let path = uri.path().trim_start_matches('/');

    // Try to serve the requested file
    if let Some(content) = Asset::get(path) {
        let mime = mime_guess::from_path(path).first_or_octet_stream();

        return Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, mime.as_ref())
            .body(Body::from(content.data))
            .unwrap();
    }

    // For SPA routing: if file not found, serve index.html
    // This allows React Router to handle the route
    if let Some(content) = Asset::get("index.html") {
        return Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, "text/html")
            .body(Body::from(content.data))
            .unwrap();
    }

    // If even index.html is missing, return 404
    Response::builder()
        .status(StatusCode::NOT_FOUND)
        .body(Body::from("404 Not Found"))
        .unwrap()
}
