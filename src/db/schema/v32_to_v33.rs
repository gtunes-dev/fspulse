// ============================================================================
// Schema Upgrade: Version 32 → 33 — New-problem counts on snapshots
//
// Snapshots are what Trends reads. They carried the integrity totals
// (how many files are invalid or suspect) but not how many problems each
// integrity check newly found; that stayed on the scans table.
//
// This migration adds new_val_invalid_count and new_hash_suspect_count to
// snapshots and fills them, for snapshots that were migrated from scans,
// from the scan they came from (same root, taken_at = the scan's
// started_at). Snapshots written since v32 by the observation pipeline
// have no scan behind them and stay NULL.
//
// This is a separate version, not an amendment of v32, because databases
// at v32 exist outside the test copies.
//
// Transacted: the DDL, the backfill and the version bump are one
// transaction.
// ============================================================================

pub const UPGRADE_32_TO_33_SQL: &str = "
    ALTER TABLE snapshots ADD COLUMN new_val_invalid_count INTEGER;
    ALTER TABLE snapshots ADD COLUMN new_hash_suspect_count INTEGER;

    UPDATE snapshots SET
        new_val_invalid_count = (
            SELECT s.new_val_invalid_count FROM scans s
            WHERE s.root_id = snapshots.root_id AND s.started_at = snapshots.taken_at
              AND s.state = 2
        ),
        new_hash_suspect_count = (
            SELECT s.new_hash_suspect_count FROM scans s
            WHERE s.root_id = snapshots.root_id AND s.started_at = snapshots.taken_at
              AND s.state = 2
        );

    INSERT OR REPLACE INTO meta (key, value) VALUES ('schema_version', '33');
";
