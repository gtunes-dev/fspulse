// ============================================================================
// Schema Upgrade: Version 30 → 31 — Remove false-positive suspect hashes
//
// Prior to the guards added in recent versions, the app could detect false
// positive suspicious hashes: if a file changed between the initial filesystem
// walk and the completion of its hashing, the hash would be flagged as suspect
// even though the file legitimately changed.
//
// This migration:
//   1. Deletes all suspect hash_versions (hash_state = 2).
//   2. Clears hash_reviewed_at on all item_versions (no suspect hashes remain
//      to review).
//   3. Adjusts scan counts: shifts hash_suspect_count into hash_baseline_count
//      and zeros out hash_suspect_count and new_hash_suspect_count.
//
// No schema DDL changes — this is a pure data-repair migration.
//
// This is a Transacted migration. All work runs inside a single IMMEDIATE
// transaction and the schema version is bumped atomically.
// ============================================================================

use rusqlite::Connection;

use crate::db::migration_info;
use crate::error::FsPulseError;

pub const UPGRADE_30_TO_31_PRE_SQL: &str =
    "INSERT OR REPLACE INTO meta (key, value) VALUES ('schema_version', '31');";

/// Remove all suspect hash_versions and adjust related counts and review flags.
pub fn migrate_v30_to_v31(conn: &Connection) -> Result<(), FsPulseError> {
    // Phase 1: Delete all suspect hash_versions
    let deleted = conn.execute("DELETE FROM hash_versions WHERE hash_state = 2", [])?;
    migration_info(&format!(
        "  Deleted {} suspect hash_version rows",
        deleted
    ));

    // Phase 2: Clear hash_reviewed_at on all item_versions
    let cleared = conn.execute(
        "UPDATE item_versions SET hash_reviewed_at = NULL WHERE hash_reviewed_at IS NOT NULL",
        [],
    )?;
    migration_info(&format!(
        "  Cleared hash_reviewed_at on {} item_versions",
        cleared
    ));

    // Phase 3: Adjust scan counts
    //   - Shift hash_suspect_count into hash_baseline_count
    //   - Zero out hash_suspect_count and new_hash_suspect_count
    let adjusted = conn.execute(
        "UPDATE scans SET
            hash_baseline_count = hash_baseline_count + hash_suspect_count,
            hash_suspect_count = 0,
            new_hash_suspect_count = 0
         WHERE hash_suspect_count > 0",
        [],
    )?;
    migration_info(&format!(
        "  Adjusted hash counts on {} scans",
        adjusted
    ));

    // Zero out new_hash_suspect_count on scans that had new suspects but no
    // cumulative suspects (edge case: scan created new suspects that were
    // already shifted out of hash_suspect_count by a later scan's recount).
    let extra = conn.execute(
        "UPDATE scans SET new_hash_suspect_count = 0
         WHERE new_hash_suspect_count > 0",
        [],
    )?;
    if extra > 0 {
        migration_info(&format!(
            "  Zeroed new_hash_suspect_count on {} additional scans",
            extra
        ));
    }

    migration_info("  Migration v30→v31 complete");
    Ok(())
}
