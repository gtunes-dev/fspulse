use axum::{extract::State, http::StatusCode};

use super::state::AppState;

/// POST /api/server/shutdown
///
/// Initiates a graceful server shutdown, triggering the same sequence
/// as SIGINT/SIGTERM: background tasks are stopped, the database is
/// checkpointed, and the process exits.
pub async fn shutdown(State(state): State<AppState>) -> StatusCode {
    log::info!("Shutdown requested via API");
    state.shutdown_notify.notify_one();
    StatusCode::ACCEPTED
}
