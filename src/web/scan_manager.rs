use crate::progress::state::{ScanProgressState, ScanStatus};
use crate::progress::web::WebProgressReporter;
use serde::{Deserialize, Serialize};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use tokio::sync::broadcast;
use tokio::task::JoinHandle;

/// Manages the currently active scan with singleton semantics
pub struct ScanManager {
    current_scan: Option<ActiveScanInfo>,
    broadcaster: Option<broadcast::Sender<ScanProgressState>>,
}

/// Information about the currently running scan
struct ActiveScanInfo {
    scan_id: i64,
    root_id: i64,
    root_path: String,
    cancel_token: Arc<AtomicBool>,
    reporter: Arc<WebProgressReporter>,
    #[allow(dead_code)] // Will be used for cancellation in Phase 2
    task_handle: Option<JoinHandle<()>>,
}

/// Information about current scan for status queries
#[derive(Debug, Serialize, Deserialize)]
pub struct CurrentScanInfo {
    pub scan_id: i64,
    pub root_id: i64,
    pub root_path: String,
}

/// Error returned when trying to start a scan while one is already running
#[derive(Debug)]
pub struct ScanInProgressError {
    pub current_scan_id: i64,
}

impl std::fmt::Display for ScanInProgressError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "A scan is already in progress (scan_id: {})",
            self.current_scan_id
        )
    }
}

impl std::error::Error for ScanInProgressError {}

impl ScanManager {
    pub fn new() -> Self {
        Self {
            current_scan: None,
            broadcaster: None,
        }
    }

    /// Attempt to start a new scan. Returns error if scan already in progress.
    /// Returns the broadcast sender and cancel token on success.
    pub fn try_start_scan(
        &mut self,
        scan_id: i64,
        root_id: i64,
        root_path: String,
        broadcaster: broadcast::Sender<ScanProgressState>,
        reporter: Arc<WebProgressReporter>,
    ) -> Result<Arc<AtomicBool>, ScanInProgressError> {
        // Check if scan already running
        if let Some(active) = &self.current_scan {
            return Err(ScanInProgressError {
                current_scan_id: active.scan_id,
            });
        }

        // Create cancel token
        let cancel_token = Arc::new(AtomicBool::new(false));

        // Store active scan info
        self.current_scan = Some(ActiveScanInfo {
            scan_id,
            root_id,
            root_path,
            cancel_token: Arc::clone(&cancel_token),
            reporter,
            task_handle: None,
        });

        self.broadcaster = Some(broadcaster);

        Ok(cancel_token)
    }

    /// Request cancellation of the current scan
    ///
    /// This method ensures atomic state transition: checks that the scan is Running,
    /// then sets both the cancel token and status to Cancelling together.
    pub fn request_cancellation(&mut self, scan_id: i64) -> Result<(), String> {
        match &self.current_scan {
            Some(active) if active.scan_id == scan_id => {
                // Check current status - must be Running to transition to Cancelling
                let current_status = active.reporter.get_status();

                match current_status {
                    ScanStatus::Running => {
                        // Atomically: set status first, then release cancel token
                        // This ordering ensures worker threads that acquire the token
                        // will see the status update via happens-before relationship
                        active.reporter.mark_cancelling();
                        active
                            .cancel_token
                            .store(true, std::sync::atomic::Ordering::Release);
                        Ok(())
                    }
                    ScanStatus::Cancelling => {
                        Err("Scan is already cancelling".to_string())
                    }
                    ScanStatus::Stopped => {
                        Err("Scan has already been stopped".to_string())
                    }
                    ScanStatus::Completed => {
                        Err("Scan has already completed".to_string())
                    }
                    ScanStatus::Error { .. } => {
                        Err("Scan has already errored".to_string())
                    }
                }
            }
            Some(active) => Err(format!(
                "Scan {} is not the current scan (current: {})",
                scan_id, active.scan_id
            )),
            None => Err("No scan is currently running".to_string()),
        }
    }

    /// Subscribe to progress state updates. Returns receiver.
    pub fn subscribe(&self) -> Result<broadcast::Receiver<ScanProgressState>, String> {
        match &self.broadcaster {
            Some(tx) => {
                let rx = tx.subscribe();
                Ok(rx)
            }
            None => Err("No scan is currently running".to_string()),
        }
    }

    /// Get information about the current scan, if any
    pub fn get_current_scan_info(&self) -> Option<CurrentScanInfo> {
        self.current_scan.as_ref().map(|active| CurrentScanInfo {
            scan_id: active.scan_id,
            root_id: active.root_id,
            root_path: active.root_path.clone(),
        })
    }

    /// Mark the current scan as complete and clean up
    pub fn mark_complete(&mut self, scan_id: i64) {
        if let Some(active) = &self.current_scan {
            if active.scan_id == scan_id {
                self.current_scan = None;
                self.broadcaster = None;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::progress::state::ScanProgressState;

    fn create_test_reporter(scan_id: i64, root_path: String) -> Arc<WebProgressReporter> {
        let (reporter, _broadcaster) = WebProgressReporter::new(scan_id, root_path);
        Arc::new(reporter)
    }

    #[test]
    fn test_scan_manager_new() {
        let manager = ScanManager::new();
        assert!(manager.current_scan.is_none());
        assert!(manager.broadcaster.is_none());
    }

    #[tokio::test]
    async fn test_try_start_scan_success() {
        let mut manager = ScanManager::new();
        let (tx, _rx) = broadcast::channel::<ScanProgressState>(100);
        let reporter = create_test_reporter(1, "/test/path".to_string());
        let result = manager.try_start_scan(1, 100, "/test/path".to_string(), tx, reporter);
        assert!(result.is_ok());
        assert!(manager.current_scan.is_some());
        assert!(manager.broadcaster.is_some());
    }

    #[tokio::test]
    async fn test_try_start_scan_fails_when_running() {
        let mut manager = ScanManager::new();
        let (tx1, _rx1) = broadcast::channel::<ScanProgressState>(100);
        let (tx2, _rx2) = broadcast::channel::<ScanProgressState>(100);
        let reporter1 = create_test_reporter(1, "/test/path".to_string());
        let reporter2 = create_test_reporter(2, "/test/path2".to_string());
        manager
            .try_start_scan(1, 100, "/test/path".to_string(), tx1, reporter1)
            .unwrap();
        let result = manager.try_start_scan(2, 101, "/test/path2".to_string(), tx2, reporter2);
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_get_current_scan_info() {
        let mut manager = ScanManager::new();
        assert!(manager.get_current_scan_info().is_none());

        let (tx, _rx) = broadcast::channel::<ScanProgressState>(100);
        let reporter = create_test_reporter(1, "/test/path".to_string());
        manager
            .try_start_scan(1, 100, "/test/path".to_string(), tx, reporter)
            .unwrap();
        let info = manager.get_current_scan_info();
        assert!(info.is_some());
        let info = info.unwrap();
        assert_eq!(info.scan_id, 1);
        assert_eq!(info.root_id, 100);
        assert_eq!(info.root_path, "/test/path");
    }

    #[tokio::test]
    async fn test_mark_complete() {
        let mut manager = ScanManager::new();
        let (tx, _rx) = broadcast::channel::<ScanProgressState>(100);
        let reporter = create_test_reporter(1, "/test/path".to_string());
        manager
            .try_start_scan(1, 100, "/test/path".to_string(), tx, reporter)
            .unwrap();
        assert!(manager.current_scan.is_some());

        manager.mark_complete(1);
        assert!(manager.current_scan.is_none());
        assert!(manager.broadcaster.is_none());
    }

    #[tokio::test]
    async fn test_request_cancellation() {
        let mut manager = ScanManager::new();
        let (tx, _rx) = broadcast::channel::<ScanProgressState>(100);
        let reporter = create_test_reporter(1, "/test/path".to_string());
        let cancel_token = manager
            .try_start_scan(1, 100, "/test/path".to_string(), tx, reporter)
            .unwrap();

        assert!(!cancel_token.load(std::sync::atomic::Ordering::Acquire));
        manager.request_cancellation(1).unwrap();
        assert!(cancel_token.load(std::sync::atomic::Ordering::Acquire));
    }

    #[test]
    fn test_subscribe_with_no_scan() {
        let manager = ScanManager::new();
        let result = manager.subscribe();
        assert!(result.is_err());
    }
}
