pub mod web;
pub mod state;

use std::sync::Arc;
use std::time::Duration;

/// Unique identifier for a progress indicator (bar, spinner, section)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ProgressId(u64);

impl ProgressId {
    pub fn new() -> Self {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        Self(COUNTER.fetch_add(1, Ordering::SeqCst))
    }
}

impl Default for ProgressId {
    fn default() -> Self {
        Self::new()
    }
}

/// Style for progress indicators
#[derive(Debug, Clone)]
pub enum ProgressStyle {
    Spinner,
    Bar { total: u64 },
}

/// Semantic work updates for progress indicators
#[derive(Debug, Clone)]
pub enum WorkUpdate {
    /// Scanning a directory
    Directory {
        #[allow(dead_code)]
        path: String,
    },
    /// Scanning a file/item
    File {
        #[allow(dead_code)]
        path: String,
    },
    /// Hashing a file
    Hashing { file: String },
    /// Validating a file
    Validating { file: String },
    /// Thread is idle/waiting
    Idle,
}

/// Configuration for creating a progress indicator
#[derive(Debug, Clone)]
pub struct ProgressConfig {
    pub style: ProgressStyle,
    pub prefix: String,
    pub message: String,
}

/// Thread-safe progress reporting abstraction
///
/// This trait provides a unified interface for reporting scan progress
/// that works in both CLI (terminal UI) and Web (event streaming) contexts.
///
/// # Thread Safety
/// All implementations must be Send + Sync to support multi-threaded scanning.
pub trait ProgressReporter: Send + Sync {
    /// Start a new scan phase section (Scanning, Sweeping, Analyzing)
    fn section_start(&self, stage_index: u32, message: &str) -> ProgressId;

    /// Finish a section with final message
    fn section_finish(&self, id: ProgressId, message: &str);

    /// Create a new spinner or progress bar
    fn create(&self, config: ProgressConfig) -> ProgressId;

    /// Update what work is being performed (semantic work updates)
    fn update_work(&self, id: ProgressId, work: WorkUpdate);

    /// Update progress bar position
    fn set_position(&self, id: ProgressId, position: u64);

    /// Update progress bar length/total
    fn set_length(&self, id: ProgressId, length: u64);

    /// Increment progress bar by delta
    fn inc(&self, id: ProgressId, delta: u64);

    /// Enable steady tick animation for spinners
    fn enable_steady_tick(&self, id: ProgressId, interval: Duration);

    /// Disable steady tick animation
    fn disable_steady_tick(&self, id: ProgressId);

    /// Finish and clear a progress indicator
    fn finish_and_clear(&self, id: ProgressId);

    /// Print a line (for messages that should appear above progress bars)
    fn println(&self, message: &str) -> Result<(), Box<dyn std::error::Error>>;

    /// Clone this reporter as Arc<dyn ProgressReporter>
    /// Enables passing the reporter to worker threads
    fn clone_reporter(&self) -> Arc<dyn ProgressReporter>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_progress_id_unique() {
        let id1 = ProgressId::new();
        let id2 = ProgressId::new();
        let id3 = ProgressId::new();

        assert_ne!(id1, id2);
        assert_ne!(id2, id3);
        assert_ne!(id1, id3);
    }

    #[test]
    fn test_progress_id_default() {
        let id1 = ProgressId::default();
        let id2 = ProgressId::default();

        assert_ne!(id1, id2);
    }

    #[test]
    fn test_progress_id_clone_copy() {
        let id1 = ProgressId::new();
        let id2 = id1;
        let id3 = id1;

        assert_eq!(id1, id2);
        assert_eq!(id2, id3);
    }

    #[test]
    fn test_progress_style_variants() {
        let spinner = ProgressStyle::Spinner;
        let bar = ProgressStyle::Bar { total: 100 };

        match spinner {
            ProgressStyle::Spinner => (),
            _ => panic!("Expected Spinner variant"),
        }

        match bar {
            ProgressStyle::Bar { total } => assert_eq!(total, 100),
            _ => panic!("Expected Bar variant"),
        }
    }

    #[test]
    fn test_progress_config_creation() {
        let config = ProgressConfig {
            style: ProgressStyle::Spinner,
            prefix: "   ".to_string(),
            message: "Processing...".to_string(),
        };

        assert_eq!(config.prefix, "   ");
        assert_eq!(config.message, "Processing...");
    }
}
