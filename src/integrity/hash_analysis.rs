use std::path::Path;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use rusqlite::Connection;

use crate::error::FsPulseError;
use crate::hash::Hash;
use super::analysis::AnalysisItem;
use crate::scans::Scan;
use crate::undo_log::UndoLog;

use super::hash_version::{HashState, HashVersion};

/// Compute a SHA-256 hash for the file at the given path.
pub fn compute_hash(
    path: &Path,
    interrupt_token: &Arc<AtomicBool>,
) -> Result<String, FsPulseError> {
    Hash::compute_sha2_256_hash(path, interrupt_token)
}

/// Persist hash results to `hash_versions`.
///
/// If the hash changed (or is new), inserts a new row. If unchanged, extends
/// `last_scan_id` on the existing row.
pub fn persist_hash(
    conn: &Connection,
    scan: &Scan,
    analysis_item: &AnalysisItem,
    new_hash: Option<&str>,
    hash_changed: bool,
) -> Result<(), FsPulseError> {
    let hash_state;

    if hash_changed {
        if analysis_item.hash_first_scan_id().is_none() {
            // First hash for this item_version — Baseline
            hash_state = HashState::Baseline;
        } else {
            // Hash changed within same item_version — always Suspect.
            // Metadata changes (mod_date, size) always produce a new item_version,
            // so any hash change on the same version is content-only and suspicious.
            hash_state = HashState::Suspect;
        }

        // Insert new hash_version row
        HashVersion::insert(
            conn,
            analysis_item.item_id(),
            analysis_item.item_version(),
            scan.scan_id(),
            new_hash.unwrap(),
            hash_state,
        )?;
    } else if let Some(first_scan_id) = analysis_item.hash_first_scan_id() {
        // Hash unchanged — extend the existing hash_version's last_scan_id
        let current_hv = HashVersion::get_current_for_version(conn, analysis_item.item_id(), analysis_item.item_version())?;
        if let Some(hv) = current_hv {
            UndoLog::log_hash_version_extend(
                conn, analysis_item.item_id(), analysis_item.item_version(), first_scan_id, hv.last_scan_id(),
            )?;
        }
        HashVersion::extend_last_scan(
            conn, analysis_item.item_id(), analysis_item.item_version(), first_scan_id, scan.scan_id(),
        )?;
    }

    Ok(())
}

