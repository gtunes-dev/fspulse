// ============================================================================
// Schema Upgrade: Version 33 → 34 — Ignored as a recorded state
//
// Until now an item that came to match one of its root's ignore patterns
// was marked deleted. Ignoring is not deleting: this version records it
// as its own kind of history entry.
//
// - item_versions.is_ignored marks a version written because of the
//   ignore patterns. With is_deleted = 1 it means "tracking stopped";
//   with is_added = 1 it means "tracking resumed". Existing versions are
//   all 0: items ignored before this version stay recorded as deleted.
// - The two range indexes are rebuilt with is_ignored appended, so the
//   range queries can tell ignored from deleted without touching the
//   table.
// - roots.ignore_changed_at records when a root's patterns last changed.
// - snapshots.ignored_count and resumed_count count those transitions,
//   which are in neither add_count nor delete_count.
//
// Transacted: the DDL and the version bump are one transaction.
// ============================================================================

pub const UPGRADE_33_TO_34_SQL: &str = "
    ALTER TABLE item_versions ADD COLUMN is_ignored BOOLEAN NOT NULL DEFAULT 0;

    DROP INDEX IF EXISTS idx_versions_root_time_hid;
    DROP INDEX IF EXISTS idx_versions_root_hid_time;
    CREATE INDEX idx_versions_root_time_hid ON item_versions
        (root_id, first_seen_at, hierarchy_id, is_added, is_deleted, is_ignored);
    CREATE INDEX idx_versions_root_hid_time ON item_versions
        (root_id, hierarchy_id, first_seen_at, is_added, is_deleted, is_ignored);

    ALTER TABLE roots ADD COLUMN ignore_changed_at INTEGER;

    ALTER TABLE snapshots ADD COLUMN ignored_count INTEGER;
    ALTER TABLE snapshots ADD COLUMN resumed_count INTEGER;

    INSERT OR REPLACE INTO meta (key, value) VALUES ('schema_version', '34');
";
