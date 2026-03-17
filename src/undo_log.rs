use log::warn;
use rusqlite::{params, Connection};

use crate::{error::FsPulseError, item_version::ItemVersion};

/// Log type discriminator for the scan_undo_log table.
///
/// - ItemVersion (0): ref_id1 = version_id, ref_id2 = 0
/// - HashVersion (1): ref_id1 = item_version_id, ref_id2 = first_scan_id
#[repr(i64)]
#[derive(Debug, Clone, Copy)]
#[allow(clippy::enum_variant_names)]
pub enum UndoLogType {
    ItemVersion = 0,
    HashVersion = 1,
}

/// Transient undo log for batched scan rollback.
///
/// Records the prior values of mutable fields before in-place updates.
/// Cleared on scan completion. Consumed on scan stop to restore state.
///
/// The schema uses a log_type discriminator to handle item_versions and
/// hash_versions in a single table. Val state lives on item_versions and
/// is handled as part of item_version undo.
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
        item_version_id: i64,
        first_scan_id: i64,
        old_last_scan_id: i64,
    ) -> Result<(), FsPulseError> {
        conn.execute(
            "INSERT INTO scan_undo_log (log_type, ref_id1, ref_id2, old_last_scan_id)
             VALUES (?, ?, ?, ?)",
            params![
                UndoLogType::HashVersion as i64,
                item_version_id,
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

    /// Roll back the temporal model for a stopped scan.
    ///
    /// Steps, in order:
    /// 1. Replay item_version undo entries — restore pre-scan `last_scan_id`.
    /// 2. Replay hash_version undo entries — restore pre-scan `last_scan_id`.
    /// 3. Delete hash_versions created in this scan (before item_versions to
    ///    satisfy FK constraint without full-table scan).
    /// 4. Delete item_versions created in this scan (val state goes with them).
    /// 5. NULL out val columns on item_versions whose last_scan_id was reverted
    ///    and whose val_scan_id now exceeds last_scan_id.
    /// 6. Delete orphaned identity rows (items with no remaining versions).
    /// 7. Clear undo log.
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
        // Join to item_versions to get item_id for PK-efficient UPDATE
        conn.execute(
            "UPDATE hash_versions SET last_scan_id = u.old_last_scan_id
             FROM scan_undo_log u
             JOIN item_versions iv ON iv.version_id = u.ref_id1
             WHERE u.log_type = 1
               AND hash_versions.item_id = iv.item_id
               AND hash_versions.item_version_id = u.ref_id1
               AND hash_versions.first_scan_id = u.ref_id2",
            [],
        )?;

        // Step 3: Delete hash_versions created in this scan
        // Must come before item_versions deletion — hash_versions has FK to
        // item_versions, and without this order SQLite does a full table scan
        // of hash_versions to verify FK constraints on each version delete.
        conn.execute(
            "DELETE FROM hash_versions WHERE first_scan_id = ?",
            [scan_id],
        )?;

        // Step 4: Delete item_versions created in this scan
        conn.execute(
            "DELETE FROM item_versions WHERE first_scan_id = ?",
            [scan_id],
        )?;

        // Step 5: NULL out val columns on reverted versions where val_scan_id
        // now exceeds the restored last_scan_id
        conn.execute(
            "UPDATE item_versions
             SET val_scan_id = NULL, val_state = NULL, val_error = NULL
             WHERE val_scan_id IS NOT NULL AND val_scan_id > last_scan_id",
            [],
        )?;

        // Step 6: Delete orphaned identity rows — items whose only version was
        // created this scan and deleted in step 4. Uses LEFT JOIN for efficient
        // index-driven orphan detection.
        conn.execute(
            "DELETE FROM items WHERE item_id IN (
                 SELECT i.item_id
                 FROM items i
                 LEFT JOIN item_versions iv ON iv.item_id = i.item_id
                 WHERE iv.version_id IS NULL
             )",
            [],
        )?;

        // Step 7: Clear undo log
        conn.execute("DELETE FROM scan_undo_log", [])?;

        Ok(())
    }
}
