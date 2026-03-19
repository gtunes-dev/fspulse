// ============================================================================
// Schema Upgrade: Version 28 → 29 — Integrity Experience Foundation
//
// Adds the schema primitives required for the Integrity page:
//
//   items table:
//     - file_extension TEXT          — lowercase extension (e.g. 'pdf', 'jpg'),
//                                      NULL for folders and extensionless files.
//                                      Indexed for efficient file-type filtering.
//     - do_not_validate INTEGER      — user flag to skip validation for this item
//                                      across all future versions.
//
//   item_versions table:
//     - val_acknowledged_at INTEGER  — when user acknowledged this version's
//                                      validation issue. Never auto-cleared.
//     - hash_acknowledged_at INTEGER — when user acknowledged this version's
//                                      hash integrity issue. Auto-cleared only
//                                      when the first Suspect hash_version is
//                                      created for a version that had none
//                                      (i.e., new integrity evidence, not
//                                      ongoing drift).
//
// The migration populates file_extension for all existing file items using
// the same extension extraction logic as the runtime scanner. It then
// recomputes has_validator from file_extension to ensure consistency.
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

ALTER TABLE item_versions ADD COLUMN val_acknowledged_at INTEGER DEFAULT NULL;
ALTER TABLE item_versions ADD COLUMN hash_acknowledged_at INTEGER DEFAULT NULL;

CREATE INDEX IF NOT EXISTS idx_items_root_ext ON items (root_id, file_extension);

INSERT OR REPLACE INTO meta (key, value) VALUES ('schema_version', '29');
"#;

/// Populate `file_extension` for all existing file items, then recompute
/// `has_validator` from the newly populated extensions for consistency.
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

    Ok(())
}
