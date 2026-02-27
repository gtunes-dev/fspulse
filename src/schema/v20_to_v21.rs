use log::info;
use rusqlite::Connection;

use crate::error::FsPulseError;

/// Schema Upgrade: Version 20 → 21
///
/// Makes `val` nullable (was `NOT NULL DEFAULT 3`) and cleans up folder versions:
///
/// Phase 1 (pre-SQL):
///   - Recreates `item_versions` with `val INTEGER` (nullable).
///   - NULLs out file-specific fields (file_hash, val, val_error, last_hash_scan,
///     last_val_scan) on all folder versions.
///
/// Phase 2 (Rust code):
///   - Collapses consecutive identical folder versions that were only separated
///     by now-removed validation state differences.
///
/// Phase 3 (post-SQL):
///   - Updates schema version to 21.
pub const UPGRADE_20_TO_21_PRE_SQL: &str = r#"
-- Verify schema version is exactly 20
SELECT 1 / (CASE WHEN (SELECT value FROM meta WHERE key = 'schema_version') = '20' THEN 1 ELSE 0 END);

-- Recreate item_versions with val nullable (was NOT NULL DEFAULT 3)
CREATE TABLE item_versions_new (
    version_id      INTEGER PRIMARY KEY AUTOINCREMENT,
    item_id         INTEGER NOT NULL,
    first_scan_id   INTEGER NOT NULL,
    last_scan_id    INTEGER NOT NULL,

    -- Shared fields (all item types)
    is_added        BOOLEAN NOT NULL DEFAULT 0,
    is_deleted      BOOLEAN NOT NULL DEFAULT 0,
    access          INTEGER NOT NULL DEFAULT 0,
    mod_date        INTEGER,
    size            INTEGER,

    -- File-specific fields (NULL for folders)
    file_hash       TEXT,
    val             INTEGER,
    val_error       TEXT,
    last_hash_scan  INTEGER,
    last_val_scan   INTEGER,

    -- Folder-specific fields (NULL for files)
    add_count       INTEGER,
    modify_count    INTEGER,
    delete_count    INTEGER,

    FOREIGN KEY (item_id) REFERENCES items(item_id),
    FOREIGN KEY (first_scan_id) REFERENCES scans(scan_id),
    FOREIGN KEY (last_scan_id) REFERENCES scans(scan_id)
);

-- Copy data, NULLing out file-specific fields for folders (item_type = 1)
INSERT INTO item_versions_new (
    version_id, item_id, first_scan_id, last_scan_id,
    is_added, is_deleted, access, mod_date, size,
    file_hash, val, val_error, last_hash_scan, last_val_scan,
    add_count, modify_count, delete_count
)
SELECT
    iv.version_id, iv.item_id, iv.first_scan_id, iv.last_scan_id,
    iv.is_added, iv.is_deleted, iv.access, iv.mod_date, iv.size,
    CASE WHEN i.item_type = 1 THEN NULL ELSE iv.file_hash END,
    CASE WHEN i.item_type = 1 THEN NULL ELSE iv.val END,
    CASE WHEN i.item_type = 1 THEN NULL ELSE iv.val_error END,
    CASE WHEN i.item_type = 1 THEN NULL ELSE iv.last_hash_scan END,
    CASE WHEN i.item_type = 1 THEN NULL ELSE iv.last_val_scan END,
    iv.add_count, iv.modify_count, iv.delete_count
FROM item_versions iv
JOIN items i ON i.item_id = iv.item_id;

-- Swap tables
DROP TABLE item_versions;
ALTER TABLE item_versions_new RENAME TO item_versions;

-- Fix autoincrement sequence
UPDATE sqlite_sequence SET seq = (SELECT MAX(version_id) FROM item_versions) WHERE name = 'item_versions';

-- Recreate indexes
CREATE INDEX idx_versions_item_scan ON item_versions (item_id, first_scan_id DESC);
CREATE INDEX idx_versions_first_scan ON item_versions (first_scan_id);
"#;

/// Collapse consecutive identical folder versions.
///
/// After NULLing out file-specific fields in pre-SQL, some folder versions
/// that previously differed only in validation state are now identical.
/// This function merges them by extending the earlier version's last_scan_id
/// and deleting the later version.
pub fn collapse_folder_versions(conn: &Connection) -> Result<(), FsPulseError> {
    // Get all folder item_ids
    let mut folder_stmt = conn
        .prepare("SELECT item_id FROM items WHERE item_type = 1")
        .map_err(FsPulseError::DatabaseError)?;

    let folder_ids: Vec<i64> = folder_stmt
        .query_map([], |row| row.get(0))
        .map_err(FsPulseError::DatabaseError)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(FsPulseError::DatabaseError)?;

    drop(folder_stmt);

    info!(
        "Migration 20→21: Checking {} folders for collapsible versions",
        folder_ids.len()
    );

    let mut total_collapsed = 0u64;

    // Prepare statements for reuse
    let mut version_stmt = conn
        .prepare(
            "SELECT version_id, first_scan_id, last_scan_id,
                    is_added, is_deleted, access, mod_date, size,
                    add_count, modify_count, delete_count
             FROM item_versions
             WHERE item_id = ?
             ORDER BY first_scan_id ASC",
        )
        .map_err(FsPulseError::DatabaseError)?;

    let mut update_stmt = conn
        .prepare("UPDATE item_versions SET last_scan_id = ? WHERE version_id = ?")
        .map_err(FsPulseError::DatabaseError)?;

    let mut delete_stmt = conn
        .prepare("DELETE FROM item_versions WHERE version_id = ?")
        .map_err(FsPulseError::DatabaseError)?;

    for item_id in &folder_ids {
        // Fetch all versions for this folder
        let versions: Vec<FolderVersion> = version_stmt
            .query_map([item_id], |row| {
                Ok(FolderVersion {
                    version_id: row.get(0)?,
                    _first_scan_id: row.get(1)?,
                    last_scan_id: row.get(2)?,
                    is_added: row.get(3)?,
                    is_deleted: row.get(4)?,
                    access: row.get(5)?,
                    mod_date: row.get(6)?,
                    size: row.get(7)?,
                    add_count: row.get(8)?,
                    modify_count: row.get(9)?,
                    delete_count: row.get(10)?,
                })
            })
            .map_err(FsPulseError::DatabaseError)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(FsPulseError::DatabaseError)?;

        if versions.len() < 2 {
            continue;
        }

        // Walk through comparing consecutive pairs
        // We track the "keeper" (the version we're extending) as we go
        let mut keeper_idx = 0;

        for i in 1..versions.len() {
            if versions[keeper_idx].is_identical_to(&versions[i]) {
                // Extend keeper's last_scan_id and delete the duplicate
                update_stmt
                    .execute(rusqlite::params![
                        versions[i].last_scan_id,
                        versions[keeper_idx].version_id
                    ])
                    .map_err(FsPulseError::DatabaseError)?;

                delete_stmt
                    .execute(rusqlite::params![versions[i].version_id])
                    .map_err(FsPulseError::DatabaseError)?;

                total_collapsed += 1;
            } else {
                // Different state — this version becomes the new keeper
                keeper_idx = i;
            }
        }
    }

    info!(
        "Migration 20→21: Collapsed {} redundant folder versions",
        total_collapsed
    );

    Ok(())
}

pub const UPGRADE_20_TO_21_POST_SQL: &str = r#"
UPDATE meta SET value = '21' WHERE key = 'schema_version';
"#;

/// Represents a folder version's comparable state.
struct FolderVersion {
    version_id: i64,
    _first_scan_id: i64,
    last_scan_id: i64,
    is_added: bool,
    is_deleted: bool,
    access: i64,
    mod_date: Option<i64>,
    size: Option<i64>,
    add_count: Option<i64>,
    modify_count: Option<i64>,
    delete_count: Option<i64>,
}

impl FolderVersion {
    /// Check if two folder versions have identical state (ignoring scan boundaries).
    fn is_identical_to(&self, other: &FolderVersion) -> bool {
        self.is_added == other.is_added
            && self.is_deleted == other.is_deleted
            && self.access == other.access
            && self.mod_date == other.mod_date
            && self.size == other.size
            && self.add_count == other.add_count
            && self.modify_count == other.modify_count
            && self.delete_count == other.delete_count
    }
}
