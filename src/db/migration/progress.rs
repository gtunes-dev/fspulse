use std::sync::{Mutex, OnceLock};
use tokio::sync::broadcast;

/// Message types sent to SSE clients during migration.
#[derive(Clone, Debug, serde::Serialize)]
#[serde(tag = "type")]
pub enum MigrationMessage {
    /// A progress log line from the migration.
    Progress { message: String },
    /// An error occurred during migration.
    Error { message: String },
    /// Migrations completed successfully.
    Complete,
    /// Migration failed — the process will exit.
    Failed { message: String },
}

static MIGRATION_TX: OnceLock<broadcast::Sender<MigrationMessage>> = OnceLock::new();
static MIGRATION_HISTORY: OnceLock<Mutex<Vec<MigrationMessage>>> = OnceLock::new();

pub struct MigrationProgress;

impl MigrationProgress {
    /// Initialize the broadcast channel. Call once before migrations start.
    pub fn init() {
        let (tx, _) = broadcast::channel::<MigrationMessage>(256);
        MIGRATION_TX
            .set(tx)
            .expect("MigrationProgress already initialized");
        MIGRATION_HISTORY
            .set(Mutex::new(Vec::new()))
            .expect("MigrationProgress history already initialized");
    }

    /// Subscribe to migration progress. Returns the history of past messages
    /// and a receiver for future messages. Returns None if not initialized.
    pub fn subscribe() -> Option<(Vec<MigrationMessage>, broadcast::Receiver<MigrationMessage>)> {
        let tx = MIGRATION_TX.get()?;
        let history = MIGRATION_HISTORY.get()?;
        // Subscribe first, then read history — this way any message sent between
        // reading history and subscribing is caught by the receiver (at worst
        // duplicated, which the client can tolerate).
        let rx = tx.subscribe();
        let history = history.lock().unwrap().clone();
        Some((history, rx))
    }

    /// Send a progress message. No-op if not initialized.
    pub fn send(msg: &str) {
        let message = MigrationMessage::Progress {
            message: msg.to_string(),
        };
        Self::broadcast(message);
    }

    /// Send an error message. No-op if not initialized.
    pub fn send_error(msg: &str) {
        let message = MigrationMessage::Error {
            message: msg.to_string(),
        };
        Self::broadcast(message);
    }

    /// Send the completion signal.
    pub fn send_complete() {
        Self::broadcast(MigrationMessage::Complete);
    }

    /// Send the failure signal.
    pub fn send_failed(msg: &str) {
        let message = MigrationMessage::Failed {
            message: msg.to_string(),
        };
        Self::broadcast(message);
    }

    fn broadcast(msg: MigrationMessage) {
        if let Some(history) = MIGRATION_HISTORY.get() {
            history.lock().unwrap().push(msg.clone());
        }
        if let Some(tx) = MIGRATION_TX.get() {
            let _ = tx.send(msg);
        }
    }
}
