//! Checkpoint task — reconciles one root's database state with the
//! filesystem.
//!
//! The task walks the root with `reconcile::find_differences`, which
//! compares each directory on disk with the same directory in the
//! database and reports only the paths that differ. Those paths go to
//! the applier, which stats each one and writes the change through
//! `item_ops`. A root that matches the database produces no writes.
//!
//! It does NOT hash files, run validators, or write to the `scans`
//! table — those are the integrity scanner's job.
//!
//! Checkpoint coexists with the watcher: both only submit hints, and the
//! single applier thread orders the writes. The watcher is NOT paused
//! at any point.
//!
//! Checkpoints are not resumable. An interrupted checkpoint is dropped
//! and the next run starts from scratch — partial progress is fine
//! because every version already written is a valid observation.

use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use rusqlite::params;
use serde::{Deserialize, Serialize};

use crate::applier::{Applier, Hint};
use crate::db::Database;
use crate::error::FsPulseError;
use crate::item_identity::VersionSource;
use crate::reconcile::{self, Difference};
use crate::root_status::{self, WalkBlock};
use crate::snapshots;
use crate::utils::Utils;

use super::progress::TaskProgress;
use super::task_type::TaskType;
use super::traits::Task;

// ============================================================================
// CheckpointSettings - Checkpoint-specific task settings
// ============================================================================

/// Settings for a checkpoint task.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct CheckpointSettings {
    /// The user has confirmed that a large amount of missing content
    /// really was deleted. Turns off the empty-root and mass-delete
    /// refusals for this one walk.
    #[serde(default)]
    pub allow_mass_delete: bool,
}

/// A walk never refuses over fewer missing items than this, so everyday
/// deletions cost no extra query.
const MASS_DELETE_MIN_ITEMS: u64 = 1000;

/// Items alive in a root. A full pass over the root's items; run only
/// when a walk has found at least `MASS_DELETE_MIN_ITEMS` missing.
const ALIVE_ITEMS_SQL: &str = "
    SELECT COUNT(*)
    FROM items i
    JOIN item_versions iv ON iv.item_id = i.item_id
       AND iv.item_version = (
           SELECT MAX(iv2.item_version) FROM item_versions iv2
           WHERE iv2.item_id = i.item_id
       )
    WHERE i.root_id = ?1 AND iv.is_deleted = 0";

/// True when a walk should refuse to record its deletions: at least
/// `MASS_DELETE_MIN_ITEMS` items are missing and they are more than
/// half of everything alive in the root.
fn is_mass_delete(missing_items: u64, alive_items: u64) -> bool {
    missing_items >= MASS_DELETE_MIN_ITEMS && missing_items * 2 > alive_items
}

impl CheckpointSettings {
    /// Serialize to JSON string for storage in database
    pub fn to_json(&self) -> Result<String, FsPulseError> {
        serde_json::to_string(self).map_err(|e| {
            FsPulseError::Error(format!("Failed to serialize CheckpointSettings: {e}"))
        })
    }

    /// Deserialize from JSON string retrieved from database
    pub fn from_json(json: &str) -> Result<Self, FsPulseError> {
        serde_json::from_str(json).map_err(|e| {
            FsPulseError::Error(format!("Failed to deserialize CheckpointSettings: {e}"))
        })
    }
}

// ============================================================================
// CheckpointTask - Task trait implementation for checkpoint operations
// ============================================================================

/// A checkpoint task — reconciles a single root: walks the tree, finds
/// the paths that differ from the database, and has the applier write
/// them.
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
    settings: CheckpointSettings,
}

impl CheckpointTask {
    /// Pure constructor, no I/O.
    pub fn new(
        task_id: i64,
        root_id: i64,
        root_path: String,
        task_settings: &str,
    ) -> Result<Self, FsPulseError> {
        Ok(Self {
            task_id,
            root_id,
            root_path,
            settings: CheckpointSettings::from_json(task_settings)?,
        })
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
}

impl Task for CheckpointTask {
    fn run(
        &mut self,
        progress: Arc<TaskProgress>,
        interrupt_token: Arc<AtomicBool>,
    ) -> Result<(), FsPulseError> {
        // Set started_at, clear completed_at, all under the lock.
        Self::begin(self.root_id)?;

        progress.set_phase("Comparing with what is recorded");
        let root_id = self.root_id;
        let confirmed = self.settings.allow_mass_delete;
        let conn = Database::get_connection()?;

        // Clear the applier's counters so the totals below describe this
        // walk (exact while the walk is the only producer of hints).
        Applier::barrier()?;

        // Paths that are new or changed go to the applier as they are
        // found. Paths that are missing are held back until the walk is
        // over, so the walk can refuse to record them if too much of
        // the root has disappeared.
        let mut missing: Vec<PathBuf> = Vec::new();
        let walked = reconcile::find_differences(
            &conn,
            root_id,
            Path::new(&self.root_path),
            confirmed,
            &interrupt_token,
            &mut |path, difference| match difference {
                Difference::Changed => Applier::submit(Hint {
                    root_id,
                    path,
                    source: VersionSource::Reconcile,
                    confirmed,
                }),
                Difference::Missing => {
                    missing.push(path);
                    Ok(())
                }
            },
            &mut |stats| {
                progress.set_indeterminate_progress(&format!(
                    "{} items in {} folders, {} differences",
                    stats.entries, stats.dirs, stats.hints
                ));
            },
        );

        let walk = match walked {
            Ok(walk) => walk,
            Err(FsPulseError::RootEmpty(detail)) => {
                root_status::set_walk_block(root_id, WalkBlock::EmptyRoot, detail.clone());
                return Err(FsPulseError::RootEmpty(detail));
            }
            Err(e) => return Err(e),
        };

        if !confirmed && walk.missing_items >= MASS_DELETE_MIN_ITEMS {
            let alive_items: i64 =
                conn.query_row(ALIVE_ITEMS_SQL, params![root_id], |row| row.get(0))?;
            if is_mass_delete(walk.missing_items, alive_items as u64) {
                // What was new or changed is still recorded.
                Applier::barrier()?;
                let detail = format!(
                    "{} of {} items in '{}' are missing. Nothing was marked deleted. \
                     If they really were deleted, run the walk again with confirmation.",
                    walk.missing_items, alive_items, self.root_path
                );
                root_status::set_walk_block(root_id, WalkBlock::MassDelete, detail.clone());
                return Err(FsPulseError::MassDelete(detail));
            }
        }

        for path in missing {
            Applier::submit(Hint {
                root_id,
                path,
                source: VersionSource::Reconcile,
                confirmed,
            })?;
        }

        // Wait for every reported path to be written before declaring
        // the root confirmed.
        progress.set_indeterminate_progress("Applying changes…");
        let applied = Applier::barrier()?;
        root_status::clear_walk_block(root_id);

        // The walk has just confirmed the whole root, so this is the
        // moment to check the snapshot's running totals against a count.
        if snapshots::correct_totals(root_id)? {
            log::info!("Checkpoint: corrected snapshot totals for root {root_id}");
        }

        progress.add_breadcrumb(&format!(
            "Compared {} items in {} folders",
            walk.entries, walk.dirs
        ));
        if walk.incomplete_dirs > 0 {
            progress.add_breadcrumb(&format!(
                "{} folders could not be fully read; deletions in them were not checked",
                walk.incomplete_dirs
            ));
        }
        progress.add_breadcrumb(&format!(
            "{} added, {} modified, {} deleted",
            applied.added,
            applied.modified + applied.coalesced,
            applied.deleted
        ));

        // Write completed_at. started_at is left in place so the UI can
        // show "last checkpoint ran from X to Y."
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
        "Checking for changes"
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_checkpoint_settings_round_trip() {
        let settings = CheckpointSettings {
            allow_mass_delete: true,
        };
        let json = settings.to_json().unwrap();
        assert_eq!(json, r#"{"allow_mass_delete":true}"#);
        assert_eq!(CheckpointSettings::from_json(&json).unwrap(), settings);
        // Rows written before the setting existed.
        assert_eq!(
            CheckpointSettings::from_json("{}").unwrap(),
            CheckpointSettings::default()
        );
    }

    #[test]
    fn mass_delete_needs_both_a_large_count_and_most_of_the_root() {
        // Small deletions never count, whatever the share.
        assert!(!is_mass_delete(999, 999));
        // Large, but not most of the root.
        assert!(!is_mass_delete(1000, 2000));
        assert!(!is_mass_delete(50_000, 3_400_000));
        // Large and more than half.
        assert!(is_mass_delete(1000, 1999));
        assert!(is_mass_delete(3_400_000, 3_400_000));
    }

    #[test]
    fn test_checkpoint_task_metadata() {
        let task = CheckpointTask::new(7, 42, "/some/root".to_string(), "{}").unwrap();
        assert_eq!(task.task_type(), TaskType::Checkpoint);
        assert_eq!(task.task_id(), 7);
        assert_eq!(task.active_root_id(), Some(42));
        assert_eq!(task.action(), "Checking for changes");
        assert_eq!(task.display_target(), "/some/root");
        assert!(!task.is_exclusive());
        assert!(task.is_stoppable());
        assert!(task.is_pausable());
    }
}
