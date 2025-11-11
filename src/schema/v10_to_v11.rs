pub const UPGRADE_10_TO_11_SQL: &str = r#"
--
-- Schema Upgrade: Version 10 → 11
--
-- This migration adds support for tracking scan timing, restart status,
-- and schedule associations, plus soft delete support for schedules.
--
-- Changes:
-- Scans table:
--   - scan_time → started_at (tracks when scan begins)
--   - Add ended_at (tracks when scan completes, nullable)
--   - Add was_restarted (boolean flag for interrupted/resumed scans)
--   - Add schedule_id (links scan to schedule if scheduled, nullable)
--
-- Scan_schedules table:
--   - Add deleted_at (soft delete timestamp, nullable)
--   - Add index on deleted_at for filtering active schedules
--

PRAGMA foreign_keys = OFF;

BEGIN TRANSACTION;

-- Verify schema version is exactly 10
SELECT 1 / (CASE WHEN (SELECT value FROM meta WHERE key = 'schema_version') = '10' THEN 1 ELSE 0 END);

-- ========================================
-- Update scans table
-- ========================================

-- Rename scan_time to started_at
ALTER TABLE scans RENAME COLUMN scan_time TO started_at;

-- Add ended_at column (nullable - null if scan is in progress or incomplete)
ALTER TABLE scans ADD COLUMN ended_at INTEGER DEFAULT NULL;

-- Add was_restarted flag (tracks if scan was interrupted and resumed)
ALTER TABLE scans ADD COLUMN was_restarted BOOLEAN NOT NULL DEFAULT 0;

-- Add schedule_id to link scans to schedules (nullable - null for manual scans)
ALTER TABLE scans ADD COLUMN schedule_id INTEGER DEFAULT NULL
    REFERENCES scan_schedules(schedule_id);

-- ========================================
-- Update scan_schedules table
-- ========================================

-- Add deleted_at for soft deletes (nullable - null for active schedules)
ALTER TABLE scan_schedules ADD COLUMN deleted_at INTEGER DEFAULT NULL;

-- Index on deleted_at for efficient filtering (WHERE deleted_at IS NULL)
CREATE INDEX idx_scan_schedules_deleted ON scan_schedules(deleted_at);

-- ========================================
-- Finalize migration
-- ========================================

UPDATE meta SET value = '11' WHERE key = 'schema_version';

COMMIT;

PRAGMA foreign_keys = ON;
"#;
