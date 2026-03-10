// ============================================================================
// Schema Upgrade: Version 27 → 28 — Project Integrity
//
// Separates hash and validation state from item_versions into dedicated
// hash_versions and val_versions tables.
//
// Steps:
//   1. (code_fn) Roll back any in-progress scans using the OLD undo log schema,
//      since the undo log structure and item_versions columns are about to change.
//   2. (post_sql) Stage hash/val data from item_versions into temp tables
//      (must happen before item_versions is rebuilt).
//   3. (post_sql) Rebuild items table with has_validator column.
//   4. (post_sql) Create hash_versions and val_versions tables AFTER items rebuild
//      (so FKs reference the final items table — SQLite rewrites FK references
//      when a table is renamed, which would otherwise leave them pointing at items_old).
//   5. (post_sql) Populate hash/val tables from staging and drop staging tables.
//   6. (post_sql) Rebuild item_versions without hash/val columns (13 columns removed).
//   7. (post_sql) Rebuild scan_undo_log with new schema (log_type discriminator).
//   8. (post_sql) Bump schema version to 28.
//
// The code_fn runs before post_sql to ensure in-progress scans are rolled back
// while the old schema is still intact.
// ============================================================================

use log::info;
use rusqlite::Connection;

use crate::error::FsPulseError;

/// Roll back any in-progress scans before schema changes.
///
/// Must run while the OLD undo log schema and item_versions columns are still
/// intact, because the rollback logic depends on them.
pub fn rollback_in_progress_scans(conn: &Connection) -> Result<(), FsPulseError> {
    // Find all in-progress scans (Scanning=1, Sweeping=2, AnalyzingFiles=3, AnalyzingScan=7)
    let mut stmt = conn.prepare(
        "SELECT scan_id FROM scans WHERE state IN (1, 2, 3, 7)"
    )?;

    let scan_ids: Vec<i64> = stmt
        .query_map([], |row| row.get(0))?
        .collect::<Result<Vec<_>, _>>()?;

    if scan_ids.is_empty() {
        return Ok(());
    }

    info!(
        "Migration v27→v28: rolling back {} in-progress scan(s) before schema change",
        scan_ids.len()
    );

    for scan_id in &scan_ids {
        info!("  Rolling back scan {}", scan_id);

        // Step 1: Replay undo log — restore pre-scan bookkeeping values
        // (uses OLD undo log schema with old_last_hash_scan and old_last_val_scan)
        conn.execute(
            "UPDATE item_versions SET
                last_scan_id = u.old_last_scan_id,
                last_hash_scan = u.old_last_hash_scan,
                last_val_scan = u.old_last_val_scan
             FROM scan_undo_log u
             WHERE item_versions.version_id = u.version_id",
            [],
        )?;

        // Step 2: Delete versions created in this scan
        conn.execute(
            "DELETE FROM item_versions WHERE first_scan_id = ?",
            [scan_id],
        )?;

        // Step 3: Delete orphaned identity rows
        conn.execute(
            "DELETE FROM items
             WHERE NOT EXISTS (
                 SELECT 1 FROM item_versions iv
                 WHERE iv.item_id = items.item_id
             )",
            [],
        )?;

        // Step 4: Clear undo log
        conn.execute("DELETE FROM scan_undo_log", [])?;

        // Step 5: Delete alerts created during the scan
        conn.execute(
            "DELETE FROM alerts WHERE scan_id = ?",
            [scan_id],
        )?;

        // Step 6: Mark the scan as Stopped (state=5)
        conn.execute(
            "UPDATE scans SET state = 5, total_size = NULL, ended_at = strftime('%s', 'now')
             WHERE scan_id = ?",
            [scan_id],
        )?;
    }

    info!("  All in-progress scans rolled back");

    Ok(())
}

pub const UPGRADE_27_TO_28_POST_SQL: &str = r#"
-- ============================================================
-- Migrate hash/val data into temporary staging tables.
-- Must happen before item_versions is rebuilt (needs old hash/val columns).
-- ============================================================
CREATE TEMP TABLE hash_staging AS
SELECT
    iv.item_id,
    iv.last_hash_scan AS first_scan_id,
    iv.last_scan_id,
    iv.file_hash,
    COALESCE(iv.hash_state, 1) AS hash_state
FROM item_versions iv
WHERE iv.file_hash IS NOT NULL
  AND iv.last_hash_scan IS NOT NULL
  AND iv.last_scan_id = (
      SELECT MAX(iv2.last_scan_id)
      FROM item_versions iv2
      WHERE iv2.item_id = iv.item_id
  );

CREATE TEMP TABLE val_staging AS
SELECT
    iv.item_id,
    iv.last_val_scan AS first_scan_id,
    iv.last_scan_id,
    iv.val_state,
    iv.val_error
FROM item_versions iv
WHERE iv.val_state IS NOT NULL
  AND iv.val_state != 0
  AND iv.val_state != 3
  AND iv.last_val_scan IS NOT NULL
  AND iv.last_scan_id = (
      SELECT MAX(iv2.last_scan_id)
      FROM item_versions iv2
      WHERE iv2.item_id = iv.item_id
  );

-- ============================================================
-- Add has_validator column to items table and populate it.
-- Must happen before hash/val table creation so FKs point to
-- the final items table (SQLite rewrites FK references when
-- a table is renamed, which would leave them pointing at items_old).
-- ============================================================
ALTER TABLE items RENAME TO items_old;

CREATE TABLE items (
    item_id        INTEGER PRIMARY KEY AUTOINCREMENT,
    root_id        INTEGER NOT NULL,
    item_path      TEXT NOT NULL,
    item_name      TEXT NOT NULL,
    item_type      INTEGER NOT NULL,
    has_validator   INTEGER NOT NULL DEFAULT 0,
    FOREIGN KEY (root_id) REFERENCES roots(root_id),
    UNIQUE (root_id, item_path, item_type)
);

INSERT INTO items (item_id, root_id, item_path, item_name, item_type, has_validator)
SELECT
    item_id, root_id, item_path, item_name, item_type,
    CASE
        WHEN item_type = 0 AND (
            LOWER(item_path) LIKE '%.flac'
            OR LOWER(item_path) LIKE '%.jpg'
            OR LOWER(item_path) LIKE '%.jpeg'
            OR LOWER(item_path) LIKE '%.png'
            OR LOWER(item_path) LIKE '%.gif'
            OR LOWER(item_path) LIKE '%.tiff'
            OR LOWER(item_path) LIKE '%.bmp'
            OR LOWER(item_path) LIKE '%.pdf'
        ) THEN 1
        ELSE 0
    END
FROM items_old;

DROP TABLE items_old;

CREATE INDEX idx_items_root_path ON items (root_id, item_path COLLATE natural_path, item_type);
CREATE INDEX idx_items_root_name ON items (root_id, item_name COLLATE natural_path);

-- ============================================================
-- Create hash_versions and val_versions tables.
-- Created AFTER the items rebuild so FKs correctly reference
-- the final items table.
-- ============================================================
CREATE TABLE hash_versions (
    item_id         INTEGER NOT NULL,
    first_scan_id   INTEGER NOT NULL,
    last_scan_id    INTEGER NOT NULL,
    file_hash       BLOB NOT NULL,
    hash_state      INTEGER NOT NULL,
    PRIMARY KEY (item_id, first_scan_id),
    FOREIGN KEY (item_id) REFERENCES items(item_id),
    FOREIGN KEY (first_scan_id) REFERENCES scans(scan_id),
    FOREIGN KEY (last_scan_id) REFERENCES scans(scan_id)
) WITHOUT ROWID;

CREATE TABLE val_versions (
    item_id         INTEGER NOT NULL,
    first_scan_id   INTEGER NOT NULL,
    last_scan_id    INTEGER NOT NULL,
    val_state       INTEGER NOT NULL,
    val_error       TEXT,
    PRIMARY KEY (item_id, first_scan_id),
    FOREIGN KEY (item_id) REFERENCES items(item_id),
    FOREIGN KEY (first_scan_id) REFERENCES scans(scan_id),
    FOREIGN KEY (last_scan_id) REFERENCES scans(scan_id)
) WITHOUT ROWID;

-- Populate from staging
INSERT INTO hash_versions (item_id, first_scan_id, last_scan_id, file_hash, hash_state)
SELECT item_id, first_scan_id, last_scan_id, file_hash, hash_state
FROM hash_staging;

INSERT INTO val_versions (item_id, first_scan_id, last_scan_id, val_state, val_error)
SELECT item_id, first_scan_id, last_scan_id, val_state, val_error
FROM val_staging;

DROP TABLE hash_staging;
DROP TABLE val_staging;

-- ============================================================
-- Rebuild item_versions without hash/val and folder state count columns
-- (13 columns removed)
-- ============================================================
ALTER TABLE item_versions RENAME TO item_versions_old;

CREATE TABLE item_versions (
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

    -- Folder-specific descendant change counts (NULL for files)
    add_count       INTEGER,
    modify_count    INTEGER,
    delete_count    INTEGER,
    unchanged_count INTEGER,

    FOREIGN KEY (item_id) REFERENCES items(item_id),
    FOREIGN KEY (first_scan_id) REFERENCES scans(scan_id),
    FOREIGN KEY (last_scan_id) REFERENCES scans(scan_id)
);

INSERT INTO item_versions (
    version_id, item_id, first_scan_id, last_scan_id,
    is_added, is_deleted, access, mod_date, size,
    add_count, modify_count, delete_count, unchanged_count
)
SELECT
    version_id, item_id, first_scan_id, last_scan_id,
    is_added, is_deleted, access, mod_date, size,
    add_count, modify_count, delete_count, unchanged_count
FROM item_versions_old;

DROP TABLE item_versions_old;

CREATE INDEX idx_versions_item_scan ON item_versions (item_id, first_scan_id DESC);
CREATE INDEX idx_versions_first_scan ON item_versions (first_scan_id);

-- ============================================================
-- Rebuild scan_undo_log with new schema
-- (log_type discriminator, composite key, WITHOUT ROWID)
-- ============================================================
DROP TABLE scan_undo_log;

CREATE TABLE scan_undo_log (
    log_type            INTEGER NOT NULL,
    ref_id1             INTEGER NOT NULL,
    ref_id2             INTEGER NOT NULL DEFAULT 0,
    old_last_scan_id    INTEGER NOT NULL,
    PRIMARY KEY (log_type, ref_id1, ref_id2)
) WITHOUT ROWID;

-- ============================================================
-- Bump schema version
-- ============================================================
UPDATE meta SET value = '28' WHERE key = 'schema_version';
"#;
