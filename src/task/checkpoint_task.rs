//! Checkpoint task — fast filesystem reconciliation pass for one root.
//!
//! A checkpoint walks a root's filesystem and applies what it observes
//! to the items / item_versions tables via
//! `item_ops::apply_observed_events`. It does NOT hash files, run
//! validators, or write to the `scans` table — those are the integrity
//! scanner's job.
//!
//! **Walk.** Recursive `read_dir` + `symlink_metadata`. The walk holds
//! no lock and allocates no timestamps. ObservedItems accumulate in a
//! Vec; when the Vec reaches `CHECKPOINT_BATCH_SIZE`, it's flushed via
//! `apply_observed_events`, which acquires the write lock and allocates
//! a single `now` for the whole batch.
//!
//! **Deletions are not detected in this interim state.** The old sweep
//! relied on a per-version `last_seen_at` that the v32 schema no longer
//! has. Deletion detection returns when the walk becomes a
//! per-directory diff against the database (design doc, phase 3).
//!
//! Checkpoint coexists with the watcher via the SQLite write lock and
//! the lock-allocated-timestamp invariant — the watcher is NOT paused
//! at any point.
//!
//! Checkpoints are not resumable. An interrupted checkpoint is dropped
//! and the next run starts from scratch — partial progress is fine
//! because every version already written is a valid observation.

use std::fs;
use std::io::ErrorKind;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use rusqlite::params;
use serde::{Deserialize, Serialize};

use crate::db::Database;
use crate::error::FsPulseError;
use crate::item_identity::Access;
use crate::item_ops::{self, ObservedEvent, ObservedItem};
use crate::utils::Utils;

use super::progress::TaskProgress;
use super::task_type::TaskType;
use super::traits::Task;

/// Number of observed items to accumulate before flushing a batch
/// through `apply_observed_events`. Matches the prior scanner's batch
/// size so the lock-hold-time profile is comparable.
const CHECKPOINT_BATCH_SIZE: usize = 2000;

// ============================================================================
// CheckpointSettings - Checkpoint-specific task settings
// ============================================================================

/// Settings for a checkpoint task.
///
/// This task requires no configuration today, but each task type has
/// its own settings struct for protocol consistency. Serializes to `{}`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CheckpointSettings {}

impl CheckpointSettings {
    /// Serialize to JSON string for storage in database
    #[allow(dead_code)]
    pub fn to_json(&self) -> Result<String, FsPulseError> {
        serde_json::to_string(self).map_err(|e| {
            FsPulseError::Error(format!("Failed to serialize CheckpointSettings: {e}"))
        })
    }

    /// Deserialize from JSON string retrieved from database
    #[allow(dead_code)]
    pub fn from_json(json: &str) -> Result<Self, FsPulseError> {
        serde_json::from_str(json).map_err(|e| {
            FsPulseError::Error(format!("Failed to deserialize CheckpointSettings: {e}"))
        })
    }
}

// ============================================================================
// CheckpointTask - Task trait implementation for checkpoint operations
// ============================================================================

/// A checkpoint task — performs a fast filesystem reconciliation pass
/// over a single root: walks the tree and applies observed events to
/// the items / item_versions tables via `item_ops`.
///
/// Checkpoints are not resumable. An interrupted checkpoint is dropped
/// and the next scheduled or manual run starts from scratch — partial
/// progress is fine because every version already written is a valid
/// observation. The task therefore stores no `task_state` and
/// ignores any state JSON it might be reconstructed with.
pub struct CheckpointTask {
    task_id: i64,
    root_id: i64,
    root_path: String,
}

impl CheckpointTask {
    /// Pure constructor — no I/O. Mirrors `ScanTask::new`'s shape so
    /// the schedules.rs dispatch can construct it the same way.
    #[allow(dead_code)] // wired into schedules.rs dispatch but no UI flow creates rows yet
    pub fn new(task_id: i64, root_id: i64, root_path: String) -> Self {
        Self {
            task_id,
            root_id,
            root_path,
        }
    }

    // ========================================================================
    // Phase 0: begin
    // ========================================================================

    /// Allocate `started_at` under the write lock and stamp it on the
    /// root row. Also clears `last_checkpoint_completed_at`, so during
    /// the run the row reflects "checkpoint in progress, no completion
    /// yet." Returns the `started_at` value.
    fn begin(root_id: i64) -> Result<i64, FsPulseError> {
        let conn = Database::get_connection()?;
        Database::immediate_transaction(&conn, |c| {
            let started_at = Utils::now_secs();
            let updated = c.execute(
                "UPDATE roots
                    SET last_checkpoint_started_at = ?1,
                        last_checkpoint_completed_at = NULL
                  WHERE root_id = ?2",
                params![started_at, root_id],
            )?;
            if updated == 0 {
                return Err(FsPulseError::Error(format!(
                    "Checkpoint: root {root_id} not found"
                )));
            }
            Ok(started_at)
        })
    }

    // ========================================================================
    // Phase 1: walk
    // ========================================================================

    fn walk_root(state: &mut WalkState, root_path: &Path) -> Result<(), FsPulseError> {
        Self::check_interrupt(state.interrupt_token)?;

        // The root directory itself is NOT stored as an item — only
        // its descendants. Open the root and recurse into its entries.
        let entries = match fs::read_dir(root_path) {
            Ok(it) => it,
            Err(e) if e.kind() == ErrorKind::PermissionDenied => {
                return Err(FsPulseError::Error(format!(
                    "Checkpoint: root '{}' is unreadable (permission denied)",
                    root_path.display()
                )));
            }
            Err(e) if e.kind() == ErrorKind::NotFound => {
                return Err(FsPulseError::Error(format!(
                    "Checkpoint: root '{}' not found",
                    root_path.display()
                )));
            }
            Err(e) => return Err(FsPulseError::from(e)),
        };

        state.dirs_walked += 1;
        state.update_progress();
        Self::walk_entries(state, entries)
    }

    /// Process the contents of one directory.
    ///
    /// For each entry:
    ///   - stat it (`symlink_metadata`)
    ///   - if it's a directory, attempt `read_dir` *up front* so the
    ///     `Access` value emitted on the directory's ObservedItem
    ///     reflects whether we'll be able to recurse into it
    ///   - emit the ObservedItem (parent before children, so item_ops
    ///     never has to fault in)
    ///   - if it's a readable directory, recurse with the open
    ///     ReadDir iterator
    fn walk_entries(state: &mut WalkState, entries: fs::ReadDir) -> Result<(), FsPulseError> {
        Self::check_interrupt(state.interrupt_token)?;

        for entry in entries {
            let entry = match entry {
                Ok(e) => e,
                Err(_) => {
                    // Per-entry iteration error: skip and keep going.
                    // Matches prior scanner behavior.
                    continue;
                }
            };
            let entry_path = entry.path();

            // Stat the entry.
            let metadata = match fs::symlink_metadata(&entry_path) {
                Ok(m) => Some(m),
                Err(e) if e.kind() == ErrorKind::NotFound => continue,
                Err(e) if e.kind() == ErrorKind::PermissionDenied => None,
                Err(e) => return Err(FsPulseError::from(e)),
            };

            // For directories, try to open read_dir up front so we can
            // emit the right Access on the parent ObservedItem before
            // recursing. The iterator is held open and used by the
            // recursive call below.
            let dir_iter: Option<Result<fs::ReadDir, ()>> = match metadata.as_ref() {
                Some(m) if m.is_dir() => match fs::read_dir(&entry_path) {
                    Ok(it) => Some(Ok(it)),
                    Err(e) if e.kind() == ErrorKind::PermissionDenied => Some(Err(())),
                    Err(e) if e.kind() == ErrorKind::NotFound => continue,
                    Err(e) => return Err(FsPulseError::from(e)),
                },
                _ => None,
            };

            let access = if metadata.is_none() {
                Access::MetaError
            } else if matches!(dir_iter, Some(Err(()))) {
                Access::ReadError
            } else {
                Access::Ok
            };

            let observed =
                ObservedItem::from_checkpoint(&entry_path, metadata.as_ref(), access)?;
            state.batch.push(ObservedEvent::Upsert {
                root_id: state.root_id,
                item: observed,
            });
            state.files_walked += 1;
            state.update_progress();

            if state.batch.len() >= CHECKPOINT_BATCH_SIZE {
                Self::flush_batch(state)?;
            }

            // Recurse into readable directories. The directory's own
            // ObservedItem has already been emitted just above, so
            // children are processed in parent-before-children order.
            if let Some(Ok(child_entries)) = dir_iter {
                state.dirs_walked += 1;
                state.update_progress();
                Self::walk_entries(state, child_entries)?;
            }
        }

        Ok(())
    }

    /// Flush the accumulated batch to the database. The interrupt
    /// token is checked immediately before and after the
    /// `apply_observed_events` call — `apply_observed_events` itself
    /// is treated as atomic and is NOT given the interrupt token.
    fn flush_batch(state: &mut WalkState) -> Result<(), FsPulseError> {
        if state.batch.is_empty() {
            return Ok(());
        }
        Self::check_interrupt(state.interrupt_token)?;
        let stats = item_ops::apply_observed_events(&state.batch)?;
        Self::check_interrupt(state.interrupt_token)?;
        state.applied += stats.applied;
        state.skipped += stats.skipped;
        state.batch.clear();
        Ok(())
    }

    // ========================================================================
    // Finalize
    // ========================================================================

    /// Write `last_checkpoint_completed_at = now` under the lock.
    /// `last_checkpoint_started_at` is intentionally left in place so
    /// the UI can show "last checkpoint ran from X to Y."
    ///
    /// Only the successful path calls this. Interrupted or errored
    /// runs leave `completed_at` NULL — combined with `started_at`
    /// from `begin`, that's the signal the next run uses to know the
    /// previous attempt didn't finish.
    fn finalize(root_id: i64) -> Result<(), FsPulseError> {
        let conn = Database::get_connection()?;
        Database::immediate_transaction(&conn, |c| {
            let now = Utils::now_secs();
            c.execute(
                "UPDATE roots
                    SET last_checkpoint_completed_at = ?1
                  WHERE root_id = ?2",
                params![now, root_id],
            )?;
            Ok(())
        })
    }

    // ========================================================================
    // Small utilities
    // ========================================================================

    fn check_interrupt(interrupt_token: &AtomicBool) -> Result<(), FsPulseError> {
        if interrupt_token.load(Ordering::Acquire) {
            Err(FsPulseError::TaskInterrupted)
        } else {
            Ok(())
        }
    }
}

impl Task for CheckpointTask {
    fn run(
        &mut self,
        progress: Arc<TaskProgress>,
        interrupt_token: Arc<AtomicBool>,
    ) -> Result<(), FsPulseError> {
        // Phase 0 — set started_at, clear completed_at, all under the lock.
        Self::begin(self.root_id)?;

        // Walk
        progress.set_phase("Walking");
        let mut state = WalkState::new(self.root_id, &progress, &interrupt_token);
        Self::check_interrupt(&interrupt_token)?;
        Self::walk_root(&mut state, Path::new(&self.root_path))?;
        Self::flush_batch(&mut state)?;
        Self::check_interrupt(&interrupt_token)?;
        progress.add_breadcrumb(&format!(
            "Walked {} files in {} directories ({} applied, {} skipped)",
            state.files_walked, state.dirs_walked, state.applied, state.skipped,
        ));

        // Finalize — write completed_at. started_at is left in place
        // so the UI can show "last checkpoint ran from X to Y."
        Self::finalize(self.root_id)?;

        Ok(())
    }

    fn task_type(&self) -> TaskType {
        TaskType::Checkpoint
    }

    fn task_id(&self) -> i64 {
        self.task_id
    }

    fn active_root_id(&self) -> Option<i64> {
        Some(self.root_id)
    }

    fn action(&self) -> &str {
        "Checkpointing"
    }

    fn display_target(&self) -> String {
        self.root_path.clone()
    }

    fn on_stopped(&mut self) -> Result<(), FsPulseError> {
        // Checkpoints are not resumable. The previous run's writes
        // stay in the database — they are still valid observations,
        // just incomplete coverage. The next checkpoint re-walks the
        // root and sweeps anything not seen.
        //
        // We deliberately do NOT touch `roots.last_checkpoint_started_at`
        // and we do NOT set `last_checkpoint_completed_at`. The next
        // checkpoint overwrites `started_at` on entry, and the absence
        // of a fresh `completed_at` is the signal that the previous
        // run did not finish cleanly.
        Ok(())
    }

    fn on_error(&mut self, _error_msg: &str) -> Result<(), FsPulseError> {
        // Same reasoning as on_stopped: nothing to roll back.
        Ok(())
    }

    fn is_exclusive(&self) -> bool {
        // Checkpoint is serialized with other tasks by TaskManager's
        // one-task-at-a-time rule, but it does not need to block
        // pause/unpause or other scheduling decisions while running.
        false
    }

    fn is_stoppable(&self) -> bool {
        true
    }

    fn is_pausable(&self) -> bool {
        true
    }
}

// ============================================================================
// WalkState — accumulator carried through the recursive walk
// ============================================================================

struct WalkState<'a> {
    root_id: i64,
    progress: &'a TaskProgress,
    interrupt_token: &'a AtomicBool,
    batch: Vec<ObservedEvent>,
    files_walked: u64,
    dirs_walked: u64,
    applied: usize,
    skipped: usize,
}

impl<'a> WalkState<'a> {
    fn new(
        root_id: i64,
        progress: &'a TaskProgress,
        interrupt_token: &'a AtomicBool,
    ) -> Self {
        Self {
            root_id,
            progress,
            interrupt_token,
            batch: Vec::with_capacity(CHECKPOINT_BATCH_SIZE),
            files_walked: 0,
            dirs_walked: 0,
            applied: 0,
            skipped: 0,
        }
    }

    fn update_progress(&self) {
        self.progress.set_indeterminate_progress(&format!(
            "{} files in {} directories",
            self.files_walked, self.dirs_walked
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_checkpoint_settings_round_trip() {
        let settings = CheckpointSettings {};
        let json = settings.to_json().unwrap();
        assert_eq!(json, "{}");
        let restored = CheckpointSettings::from_json(&json).unwrap();
        assert_eq!(settings, restored);
    }

    #[test]
    fn test_checkpoint_task_metadata() {
        let task = CheckpointTask::new(7, 42, "/some/root".to_string());
        assert_eq!(task.task_type(), TaskType::Checkpoint);
        assert_eq!(task.task_id(), 7);
        assert_eq!(task.active_root_id(), Some(42));
        assert_eq!(task.action(), "Checkpointing");
        assert_eq!(task.display_target(), "/some/root");
        assert!(!task.is_exclusive());
        assert!(task.is_stoppable());
        assert!(task.is_pausable());
    }
}
