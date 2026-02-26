use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use log::info;
use rusqlite::{params, Connection};

use crate::database::{migration_info, Database};
use crate::error::FsPulseError;
use crate::scanner::Scanner;

// ============================================================================
// Schema Upgrade: Version 18 → 19 (Standalone)
//
// Backfills folder descendant change counts for all historical completed scans.
// This is a Standalone migration — it manages its own transactions because
// the recursive walk + batched writes require multiple independent transactions.
//
// Crash recovery: If the process dies mid-backfill, the schema version is still
// 18 (this function bumps to 19 only in its final transaction). On restart,
// the migration loop re-runs this function. The HWM in the meta table lets it
// skip already-processed scans efficiently. The underlying write logic is also
// fully idempotent (Case A: UPDATE, Case B: INSERT with carry-forward).
// ============================================================================

/// Meta table key for tracking backfill progress (high-water mark).
const BACKFILL_META_KEY: &str = "v19_backfill_hwm";

/// Standalone migration v18→v19: backfill folder descendant change counts.
///
/// Iterates through all completed scans (ordered by scan_id ascending),
/// running the same recursive walk + write logic used by the scan analysis phase.
/// Tracks progress via a high-water mark stored in the meta table.
pub fn run_backfill_folder_counts(conn: &Connection) -> Result<(), FsPulseError> {
    // Read or create HWM. On first run, the key doesn't exist — start from 0.
    // On resume after crash, the key holds the last successfully processed scan_id.
    let hwm: i64 = match Database::get_meta_value_locked(conn, BACKFILL_META_KEY)? {
        Some(val) => val.parse().unwrap_or(0),
        None => {
            // First run — insert the HWM key
            Database::immediate_transaction(conn, |c| {
                Database::set_meta_value_locked(c, BACKFILL_META_KEY, "0")
            })?;
            0
        }
    };

    // Query all completed scans after the HWM
    let mut stmt = conn.prepare(
        "SELECT s.scan_id, s.root_id, r.root_path
         FROM scans s
         JOIN roots r ON r.root_id = s.root_id
         WHERE s.state = 4 AND s.scan_id > ?
         ORDER BY s.scan_id ASC",
    )?;

    let scans: Vec<(i64, i64, String)> = stmt
        .query_map(params![hwm], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?))
        })?
        .collect::<Result<Vec<_>, _>>()?;

    let total = scans.len();

    if total == 0 {
        // Nothing to backfill — clean up HWM key and bump version atomically
        Database::immediate_transaction(conn, |c| {
            Database::delete_meta_locked(c, BACKFILL_META_KEY)?;
            Database::set_meta_value_locked(c, "schema_version", "19")
        })?;
        info!("Migration 18→19: No completed scans to backfill.");
        return Ok(());
    }

    migration_info(&format!("Backfilling folder counts for {} completed scans...", total));

    // Dummy interrupt token — backfill at startup is not interruptible
    let dummy_token = Arc::new(AtomicBool::new(false));

    for (completed, (scan_id, root_id, root_path)) in scans.iter().enumerate() {
        info!(
            "Migration 18→19: Processing scan {} ({}/{})",
            scan_id,
            completed + 1,
            total
        );

        Scanner::scan_analysis_worker(*root_id, *scan_id, root_path, &dummy_token)?;

        // Periodic console progress every 25 scans
        let done = completed + 1;
        if done % 25 == 0 {
            migration_info(&format!("  Backfill progress: {}/{} scans processed", done, total));
        }

        // Persist HWM after each scan completes
        Database::immediate_transaction(conn, |c| {
            Database::set_meta_value_locked(c, BACKFILL_META_KEY, &scan_id.to_string())
        })?;
    }

    // All scans processed — delete HWM key and bump version atomically
    Database::immediate_transaction(conn, |c| {
        Database::delete_meta_locked(c, BACKFILL_META_KEY)?;
        Database::set_meta_value_locked(c, "schema_version", "19")
    })?;

    migration_info(&format!("Backfilled folder counts for {} scans.", total));

    Ok(())
}
