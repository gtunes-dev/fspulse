// ============================================================================
// Schema Upgrade: Version 28 → 29 — Integrity Experience Foundation
//
// Adds the schema primitives required for the Integrity page and removes
// the legacy alerts infrastructure:
//
//   items table:
//     - file_extension TEXT          — lowercase extension (e.g. 'pdf', 'jpg'),
//                                      NULL for folders and extensionless files.
//                                      Indexed for efficient file-type filtering.
//     - do_not_validate INTEGER      — user flag to skip validation for this item
//                                      across all future versions.
//
//   item_versions table:
//     - val_reviewed_at INTEGER      — when user marked this version's validation
//                                      issue as reviewed. Never auto-cleared.
//     - hash_reviewed_at INTEGER     — when user marked this version's hash
//                                      integrity issue as reviewed. Auto-cleared
//                                      only when the first Suspect hash_version
//                                      is created for a version that had none
//                                      (i.e., new integrity evidence, not
//                                      ongoing drift).
//
//   scans table:
//     - alert_count removed          — alerts infrastructure dropped
//     - new_hash_suspect_count added — count of hash_versions with hash_state=2
//                                      first seen in this scan (new suspect hashes)
//     - new_val_invalid_count added  — count of item_versions with val_state=2
//                                      validated in this scan (new val failures)
//
//   alerts table:
//     - Dropped entirely             — replaced by integrity-derived views
//
// The migration populates file_extension for all existing file items using
// the same extension extraction logic as the runtime scanner. It then
// recomputes has_validator from file_extension to ensure consistency.
// It also backfills new_hash_suspect_count and new_val_invalid_count from
// the existing hash_versions and item_versions data.
//
// This is a Transacted migration. The framework runs pre_sql, code_fn, and
// post_sql inside a single IMMEDIATE transaction and bumps the schema version
// on success.
// ============================================================================

use std::path::Path;

use rusqlite::{params, Connection};

use crate::error::FsPulseError;

pub const UPGRADE_28_TO_29_PRE_SQL: &str = r#"
ALTER TABLE items ADD COLUMN do_not_validate INTEGER NOT NULL DEFAULT 0;
ALTER TABLE items ADD COLUMN file_extension TEXT;

ALTER TABLE item_versions ADD COLUMN val_reviewed_at INTEGER DEFAULT NULL;
ALTER TABLE item_versions ADD COLUMN hash_reviewed_at INTEGER DEFAULT NULL;

ALTER TABLE scans DROP COLUMN alert_count;
ALTER TABLE scans ADD COLUMN new_hash_suspect_count INTEGER DEFAULT NULL;
ALTER TABLE scans ADD COLUMN new_val_invalid_count INTEGER DEFAULT NULL;

DROP TABLE IF EXISTS alerts;

CREATE INDEX IF NOT EXISTS idx_items_root_ext ON items (root_id, file_extension);

CREATE INDEX IF NOT EXISTS idx_hash_versions_first_scan ON hash_versions (first_scan_id, hash_state);
CREATE INDEX IF NOT EXISTS idx_versions_val_scan ON item_versions (val_scan_id, val_state);

INSERT OR REPLACE INTO meta (key, value) VALUES ('schema_version', '29');
"#;

/// Populate `file_extension` for all existing file items, recompute
/// `has_validator` for consistency, and backfill the new scan integrity
/// issue counts from existing hash_versions and item_versions data.
pub fn migrate_v28_to_v29(conn: &Connection) -> Result<(), FsPulseError> {
    // Collect all file items (item_type = 0 = File).
    // Non-files (directories, symlinks) keep file_extension = NULL.
    let mut stmt =
        conn.prepare("SELECT item_id, item_name FROM items WHERE item_type = 0")?;

    let items: Vec<(i64, String)> = stmt
        .query_map([], |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?)))?
        .collect::<rusqlite::Result<_>>()?;

    drop(stmt);

    let mut update =
        conn.prepare("UPDATE items SET file_extension = ? WHERE item_id = ?")?;

    for (item_id, item_name) in &items {
        let ext: Option<String> = Path::new(item_name)
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_ascii_lowercase());

        update.execute(params![ext, item_id])?;
    }

    drop(update);

    // Recompute has_validator from file_extension to guarantee consistency.
    // This matches the extension list in validate/validator.rs.
    conn.execute_batch(
        "UPDATE items SET has_validator = CASE
             WHEN item_type = 0 AND file_extension IN (
                 'flac', 'jpg', 'jpeg', 'png', 'gif', 'tiff', 'bmp', 'pdf'
             ) THEN 1
             ELSE 0
         END",
    )?;

    // Backfill new_hash_suspect_count: hash_versions rows with hash_state=2
    // whose first_scan_id matches the scan (newly detected suspect hashes).
    conn.execute_batch(
        "UPDATE scans SET new_hash_suspect_count = (
             SELECT COALESCE(COUNT(*), 0)
             FROM hash_versions
             WHERE hash_versions.first_scan_id = scans.scan_id
               AND hash_versions.hash_state = 2
         )
         WHERE scans.state = 4",
    )?;

    // Backfill new_val_invalid_count: item_versions rows with val_state=2
    // whose val_scan_id matches the scan (newly detected validation failures).
    conn.execute_batch(
        "UPDATE scans SET new_val_invalid_count = (
             SELECT COALESCE(COUNT(*), 0)
             FROM item_versions
             WHERE item_versions.val_scan_id = scans.scan_id
               AND item_versions.val_state = 2
         )
         WHERE scans.state = 4",
    )?;

    Ok(())
}
