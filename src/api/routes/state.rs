use std::sync::Arc;
use tokio::sync::Notify;

/// Shared application state passed to all Axum handlers via `.with_state()`.
#[derive(Clone)]
pub struct AppState {
    pub shutdown_notify: Arc<Notify>,
}

impl AppState {
    pub fn new(shutdown_notify: Arc<Notify>) -> Self {
        Self { shutdown_notify }
    }
}
