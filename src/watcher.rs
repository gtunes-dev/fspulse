use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use log::{error, info, warn};
use notify::RecursiveMode;
use notify_debouncer_mini::{new_debouncer, DebouncedEventKind};

use crate::db::Database;
use crate::error::FsPulseError;

/// Given a list of root paths, return only the top-level parents.
/// If `/a/b` and `/a/b/c` are both roots, only `/a/b` is returned
/// since recursive watching already covers `/a/b/c`.
fn coalesce_roots(roots: &[(i64, String)]) -> Vec<&(i64, String)> {
    // Sort by path length so shorter (parent) paths come first
    let mut sorted: Vec<&(i64, String)> = roots.iter().collect();
    sorted.sort_by_key(|(_, path)| path.len());

    let mut result: Vec<&(i64, String)> = Vec::new();

    for candidate in &sorted {
        let candidate_path = Path::new(&candidate.1);
        let is_nested = result
            .iter()
            .any(|(_, parent)| candidate_path.starts_with(parent));
        if is_nested {
            info!(
                "Watcher: root {} ({}) is nested under another root, skipping",
                candidate.0, candidate.1
            );
        } else {
            result.push(candidate);
        }
    }

    result
}

/// Runs the watcher on all configured roots.
/// Blocks until the interrupt token is set.
pub fn watch_roots(interrupt: Arc<AtomicBool>) -> Result<(), FsPulseError> {
    let conn = Database::get_connection()?;

    // Query all roots
    let mut stmt = conn.prepare("SELECT root_id, root_path FROM roots")?;
    let roots: Vec<(i64, String)> = stmt
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
        .collect::<Result<Vec<_>, _>>()?;

    if roots.is_empty() {
        warn!("Watcher: no roots configured, nothing to watch");
        return Ok(());
    }

    let watch_roots = coalesce_roots(&roots);

    // Channel for debounced events
    let (tx, rx) = std::sync::mpsc::channel();

    // Create debouncer with 2-second debounce window
    let mut debouncer = new_debouncer(Duration::from_secs(2), tx)
        .map_err(|e| FsPulseError::Error(format!("Failed to create file watcher: {e}")))?;

    // Watch each root recursively
    for (root_id, root_path) in &watch_roots {
        let path = Path::new(root_path);
        if !path.exists() {
            warn!("Watcher: root {} ({}) does not exist, skipping", root_id, root_path);
            continue;
        }
        match debouncer.watcher().watch(path, RecursiveMode::Recursive) {
            Ok(()) => info!("Watcher: watching root {} ({})", root_id, root_path),
            Err(e) => error!("Watcher: failed to watch root {} ({}): {}", root_id, root_path, e),
        }
    }

    info!("Watcher: started, listening for filesystem events");

    // Event loop — check for events or interrupt
    loop {
        if interrupt.load(Ordering::Relaxed) {
            info!("Watcher: interrupt received, stopping");
            break;
        }

        match rx.recv_timeout(Duration::from_millis(500)) {
            Ok(Ok(events)) => {
                for event in &events {
                    let kind = match event.kind {
                        DebouncedEventKind::Any => "changed",
                        DebouncedEventKind::AnyContinuous => "changed (ongoing)",
                        _ => "unknown",
                    };
                    info!("Watcher: {} — {}", kind, event.path.display());
                }
            }
            Ok(Err(e)) => {
                error!("Watcher: error: {e}");
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                // No events — loop back to check interrupt
            }
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                warn!("Watcher: channel disconnected, stopping");
                break;
            }
        }
    }

    info!("Watcher: stopped");
    Ok(())
}
