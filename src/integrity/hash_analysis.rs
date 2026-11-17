use std::path::Path;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use rusqlite::Connection;

use crate::error::FsPulseError;
use crate::hash::Hash;
use crate::utils::Utils;
use super::analysis::AnalysisItem;

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
/// If the hash changed (or is new), inserts a new row. If unchanged,
/// extends `last_seen_at` on the existing row.
pub fn persist_hash(
    conn: &Connection,
    analysis_item: &AnalysisItem,
    new_hash: Option<&str>,
    hash_changed: bool,
) -> Result<(), FsPulseError> {
    let now = Utils::now_secs();

    if hash_changed {
        let hash_state = if analysis_item.hash_first_seen_at().is_none() {
            // First hash for this item_version — Baseline
            HashState::Baseline
        } else {
            // Hash changed within same item_version — always Suspect.
            // Metadata changes (mod_date, size) always produce a new
            // item_version, so any hash change on the same version is
            // content-only and suspicious.
            HashState::Suspect
        };

        HashVersion::insert(
            conn,
            analysis_item.item_id(),
            analysis_item.item_version(),
            now,
            new_hash.unwrap(),
            hash_state,
        )?;
    } else if let Some(first_seen_at) = analysis_item.hash_first_seen_at() {
        // Hash unchanged — extend the existing hash_version's last_seen_at
        HashVersion::extend_last_seen_at(
            conn,
            analysis_item.item_id(),
            analysis_item.item_version(),
            first_seen_at,
            now,
        )?;
    }

    Ok(())
}
