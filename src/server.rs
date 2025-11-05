use axum::{
    body::Body,
    http::{header, StatusCode},
    response::Html,
    routing::{delete, get, post, put},
    Router,
};

// These imports are needed for static file handlers
#[cfg(debug_assertions)]
use axum::{
    http::Uri,
    response::{IntoResponse, Response},
};

#[cfg(not(debug_assertions))]
use axum::{
    http::Uri,
    response::{IntoResponse, Response},
};
use std::net::SocketAddr;
use std::time::Duration;
use tokio::net::TcpListener;

use crate::database::Database;
use crate::error::FsPulseError;
use crate::scan_manager::ScanManager;
use crate::api;

// Embed static files in release builds
#[cfg(not(debug_assertions))]
use rust_embed::RustEmbed;

#[cfg(not(debug_assertions))]
#[derive(RustEmbed)]
#[folder = "frontend/dist/"]
struct Asset;

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

        // Start background queue processor
        tokio::spawn(async {
            log::info!("Starting background queue processor (polling every 20 seconds)");
            let mut interval = tokio::time::interval(Duration::from_secs(20));

            loop {
                interval.tick().await;

                let db = match Database::new() {
                    Ok(db) => db,
                    Err(e) => {
                        log::error!("Queue processor: Failed to open database: {}", e);
                        continue;
                    }
                };

                if let Err(e) = ScanManager::poll_queue(&db) {
                    log::error!("Queue processor error: {}", e);
                }
            }
        });

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

            // Item endpoints
            .route("/api/items/{item_id}/folder-size", get(api::items::get_folder_size))

            // Scan endpoints
            .route("/api/scans/schedule", post(api::scans::schedule_scan))
            .route("/api/scans/current", get(api::scans::get_current_scan))
            .route("/api/scans/{scan_id}/cancel", post(api::scans::cancel_scan))

            // Root endpoints
            .route("/api/roots", post(api::roots::create_root))
            .route("/api/roots/with-scans", get(api::roots::get_roots_with_scans))
            .route("/api/roots/{root_id}", delete(api::roots::delete_root))

            // WebSocket routes
            .route("/ws/scans/progress", get(api::scans::scan_progress_ws))

            // Add state for handlers
            .with_state(app_state);

        // Serve static files differently based on build type
        #[cfg(debug_assertions)]
        {
            // Development: serve from filesystem with SPA fallback
            let app = app.fallback(dev_static_handler);
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

// Handler for filesystem static files (development builds only)
#[cfg(debug_assertions)]
async fn dev_static_handler(uri: Uri) -> impl IntoResponse {
    let path = uri.path().trim_start_matches('/');
    let file_path = if path.is_empty() {
        "frontend/dist/index.html"
    } else {
        &format!("frontend/dist/{}", path)
    };

    // Try to serve the requested file
    if let Ok(content) = std::fs::read(file_path) {
        let mime = mime_guess::from_path(file_path).first_or_octet_stream();

        return Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, mime.as_ref())
            .body(Body::from(content))
            .unwrap();
    }

    // For SPA routing: if file not found, serve index.html
    if let Ok(content) = std::fs::read("frontend/dist/index.html") {
        return Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, "text/html")
            .body(Body::from(content))
            .unwrap();
    }

    // If even index.html is missing, return 404
    Response::builder()
        .status(StatusCode::NOT_FOUND)
        .body(Body::from("404 Not Found"))
        .unwrap()
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
