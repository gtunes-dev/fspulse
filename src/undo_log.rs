use log::warn;
use rusqlite::{params, Connection};

use crate::{error::FsPulseError, item_version::ItemVersion};

/// Transient undo log for batched scan rollback.
///
/// Records the prior values of mutable fields before in-place updates to pre-existing
/// versions (where `first_scan_id < current_scan`). Cleared on scan completion.
/// Consumed on scan stop to restore versions to their pre-scan state.
pub struct UndoLog;

impl UndoLog {
    /// Record the current state of a version before an in-place update.
    ///
    /// Must be called BEFORE the UPDATE is applied. Only needed for pre-existing
    /// versions — versions created in the current scan (`first_scan_id = current_scan`)
    /// are simply deleted on rollback and don't need undo entries.
    pub fn log_update(
        conn: &Connection,
        version: &ItemVersion,
    ) -> Result<(), FsPulseError> {
        conn.execute(
            "INSERT INTO scan_undo_log (version_id, old_last_scan_id, old_last_hash_scan, old_last_val_scan)
             VALUES (?, ?, ?, ?)",
            params![
                version.version_id(),
                version.last_scan_id(),
                version.last_hash_scan(),
                version.last_val_scan(),
            ],
        )?;
        Ok(())
    }

    /// Read the pre-scan `last_scan_id` for a version from the undo log.
    ///
    /// Used by the analysis phase when it needs to "properly close" a pre-existing version
    /// before inserting a new version. The walk phase already logged the undo entry via
    /// `handle_item_no_change`; this reads the original value back so the analysis phase
    /// can restore it, ensuring only the new version has `last_scan_id = current_scan`.
    pub fn get_old_last_scan_id(
        conn: &Connection,
        version_id: i64,
    ) -> Result<i64, FsPulseError> {
        let old_last_scan_id: i64 = conn.query_row(
            "SELECT old_last_scan_id FROM scan_undo_log WHERE version_id = ?",
            params![version_id],
            |row| row.get(0),
        )?;
        Ok(old_last_scan_id)
    }

    /// Clear the entire undo log. Called on scan completion.
    ///
    /// SQLite's truncate optimization makes DELETE without WHERE effectively O(1).
    pub fn clear(conn: &Connection) -> Result<(), FsPulseError> {
        conn.execute("DELETE FROM scan_undo_log", [])?;
        Ok(())
    }

    /// Guard: warn and clear if the undo log is non-empty at scan start.
    ///
    /// A non-empty undo log at scan start means a previous scan completed or errored
    /// without properly cleaning up — likely a crash. The stale entries are harmless
    /// but would corrupt rollback if left in place, so we clear them.
    pub fn warn_and_clear_if_not_empty(conn: &Connection) -> Result<(), FsPulseError> {
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM scan_undo_log",
            [],
            |row| row.get(0),
        )?;

        if count > 0 {
            warn!(
                "Undo log contains {} stale entries at scan start — clearing (likely prior crash)",
                count
            );
            Self::clear(conn)?;
        }

        Ok(())
    }

    /// Roll back the new temporal model for a stopped scan.
    ///
    /// Four steps, in order:
    /// 1. Replay undo log — restore pre-scan `last_scan_id`, `last_hash_scan`, `last_val_scan`
    ///    on pre-existing versions that were modified in-place during the scan.
    /// 2. Delete versions created in this scan (`first_scan_id = scan_id`) — covers
    ///    new items, rehydrations, modifications, deletions, and analysis-phase inserts.
    ///    Must come before item deletion to satisfy FK constraint on `item_versions.item_id`.
    /// 3. Delete orphaned identity rows — items with no remaining versions after step 2.
    /// 4. Clear undo log.
    ///
    /// Must be called inside a transaction.
    pub fn rollback(conn: &Connection, scan_id: i64) -> Result<(), FsPulseError> {
        // Step 1: Replay undo log — restore pre-scan bookkeeping values
        conn.execute(
            "UPDATE item_versions SET
                last_scan_id = u.old_last_scan_id,
                last_hash_scan = u.old_last_hash_scan,
                last_val_scan = u.old_last_val_scan
             FROM scan_undo_log u
             WHERE item_versions.version_id = u.version_id",
            [],
        )?;

        // Step 2: Delete versions created in this scan.
        // Must come before item deletion because item_versions.item_id
        // references items(item_id) — FK enforcement blocks deletion of
        // items that still have child versions.
        conn.execute(
            "DELETE FROM item_versions WHERE first_scan_id = ?",
            [scan_id],
        )?;

        // Step 3: Delete orphaned identity rows (items with no remaining versions).
        // After step 2 removed all versions from this scan, orphaned items are
        // simply those with no versions left at all.
        conn.execute(
            "DELETE FROM items
             WHERE NOT EXISTS (
                 SELECT 1 FROM item_versions iv
                 WHERE iv.item_id = items.item_id
             )",
            [],
        )?;

        // Step 4: Clear undo log
        conn.execute("DELETE FROM scan_undo_log", [])?;

        Ok(())
    }
}
