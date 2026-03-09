use log::warn;
use rusqlite::{params, Connection};

use crate::{error::FsPulseError, item_version::ItemVersion};

/// Log type discriminator for the scan_undo_log table.
///
/// - ItemVersion (0): ref_id1 = version_id, ref_id2 = 0
/// - HashVersion (1): ref_id1 = item_id, ref_id2 = first_scan_id
/// - ValVersion (2): ref_id1 = item_id, ref_id2 = first_scan_id
#[repr(i64)]
#[derive(Debug, Clone, Copy)]
pub enum UndoLogType {
    ItemVersion = 0,
    HashVersion = 1,
    ValVersion = 2,
}

/// Transient undo log for batched scan rollback.
///
/// Records the prior values of mutable fields before in-place updates.
/// Cleared on scan completion. Consumed on scan stop to restore state.
///
/// The new schema uses a log_type discriminator to handle item_versions,
/// hash_versions, and val_versions in a single table.
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
            "INSERT INTO scan_undo_log (log_type, ref_id1, ref_id2, old_last_scan_id)
             VALUES (?, ?, 0, ?)",
            params![
                UndoLogType::ItemVersion as i64,
                version.version_id(),
                version.last_scan_id(),
            ],
        )?;
        Ok(())
    }

    /// Record the current last_scan_id of a hash_version before extending it.
    ///
    /// Called before HashVersion::extend_last_scan to enable rollback.
    pub fn log_hash_version_extend(
        conn: &Connection,
        item_id: i64,
        first_scan_id: i64,
        old_last_scan_id: i64,
    ) -> Result<(), FsPulseError> {
        conn.execute(
            "INSERT INTO scan_undo_log (log_type, ref_id1, ref_id2, old_last_scan_id)
             VALUES (?, ?, ?, ?)",
            params![
                UndoLogType::HashVersion as i64,
                item_id,
                first_scan_id,
                old_last_scan_id,
            ],
        )?;
        Ok(())
    }

    /// Record the current last_scan_id of a val_version before extending it.
    ///
    /// Called before ValVersion::extend_last_scan to enable rollback.
    pub fn log_val_version_extend(
        conn: &Connection,
        item_id: i64,
        first_scan_id: i64,
        old_last_scan_id: i64,
    ) -> Result<(), FsPulseError> {
        conn.execute(
            "INSERT INTO scan_undo_log (log_type, ref_id1, ref_id2, old_last_scan_id)
             VALUES (?, ?, ?, ?)",
            params![
                UndoLogType::ValVersion as i64,
                item_id,
                first_scan_id,
                old_last_scan_id,
            ],
        )?;
        Ok(())
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
    /// Steps, in order:
    /// 1. Replay item_version undo entries — restore pre-scan `last_scan_id`.
    /// 2. Replay hash_version undo entries — restore pre-scan `last_scan_id`.
    /// 3. Replay val_version undo entries — restore pre-scan `last_scan_id`.
    /// 4. Delete item_versions created in this scan.
    /// 5. Delete hash_versions created in this scan.
    /// 6. Delete val_versions created in this scan.
    /// 7. Delete orphaned identity rows.
    /// 8. Clear undo log.
    ///
    /// Must be called inside a transaction.
    pub fn rollback(conn: &Connection, scan_id: i64) -> Result<(), FsPulseError> {
        // Step 1: Replay item_version undo entries
        conn.execute(
            "UPDATE item_versions SET last_scan_id = u.old_last_scan_id
             FROM scan_undo_log u
             WHERE u.log_type = 0
               AND item_versions.version_id = u.ref_id1",
            [],
        )?;

        // Step 2: Replay hash_version undo entries
        conn.execute(
            "UPDATE hash_versions SET last_scan_id = u.old_last_scan_id
             FROM scan_undo_log u
             WHERE u.log_type = 1
               AND hash_versions.item_id = u.ref_id1
               AND hash_versions.first_scan_id = u.ref_id2",
            [],
        )?;

        // Step 3: Replay val_version undo entries
        conn.execute(
            "UPDATE val_versions SET last_scan_id = u.old_last_scan_id
             FROM scan_undo_log u
             WHERE u.log_type = 2
               AND val_versions.item_id = u.ref_id1
               AND val_versions.first_scan_id = u.ref_id2",
            [],
        )?;

        // Step 4: Delete item_versions created in this scan
        conn.execute(
            "DELETE FROM item_versions WHERE first_scan_id = ?",
            [scan_id],
        )?;

        // Step 5: Delete hash_versions created in this scan
        conn.execute(
            "DELETE FROM hash_versions WHERE first_scan_id = ?",
            [scan_id],
        )?;

        // Step 6: Delete val_versions created in this scan
        conn.execute(
            "DELETE FROM val_versions WHERE first_scan_id = ?",
            [scan_id],
        )?;

        // Step 7: Delete orphaned identity rows
        conn.execute(
            "DELETE FROM items
             WHERE NOT EXISTS (
                 SELECT 1 FROM item_versions iv
                 WHERE iv.item_id = items.item_id
             )",
            [],
        )?;

        // Step 8: Clear undo log
        conn.execute("DELETE FROM scan_undo_log", [])?;

        Ok(())
    }
}
