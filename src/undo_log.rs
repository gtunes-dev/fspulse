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

    /// Clear the entire undo log. Called on scan completion.
    ///
    /// SQLite's truncate optimization makes DELETE without WHERE effectively O(1).
    #[allow(dead_code)]
    pub fn clear(conn: &Connection) -> Result<(), FsPulseError> {
        conn.execute("DELETE FROM scan_undo_log", [])?;
        Ok(())
    }

    /// Roll back the new temporal model for a stopped scan.
    ///
    /// Four steps, in order:
    /// 1. Replay undo log — restore pre-scan `last_scan_id`, `last_hash_scan`, `last_val_scan`
    ///    on pre-existing versions that were modified in-place during the scan.
    /// 2. Delete orphaned identity rows — items that have versions in this scan but none
    ///    from prior scans. Done before version deletion so we can identify them cheaply
    ///    via EXISTS/NOT EXISTS on indexed columns, avoiding a full table scan.
    /// 3. Delete versions created in this scan (`first_scan_id = scan_id`) — covers
    ///    new items, rehydrations, modifications, deletions, and analysis-phase inserts.
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

        // Step 2: Delete orphaned identity rows (items whose only versions are from this scan)
        conn.execute(
            "DELETE FROM items
             WHERE EXISTS (
                 SELECT 1 FROM item_versions iv
                 WHERE iv.item_id = items.item_id AND iv.first_scan_id = ?1
             )
             AND NOT EXISTS (
                 SELECT 1 FROM item_versions iv
                 WHERE iv.item_id = items.item_id AND iv.first_scan_id != ?1
             )",
            [scan_id],
        )?;

        // Step 3: Delete versions created in this scan
        conn.execute(
            "DELETE FROM item_versions WHERE first_scan_id = ?",
            [scan_id],
        )?;

        // Step 4: Clear undo log
        conn.execute("DELETE FROM scan_undo_log", [])?;

        Ok(())
    }
}
