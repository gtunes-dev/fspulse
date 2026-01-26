pub const UPGRADE_11_TO_12_SQL: &str = r#"
--
-- Schema Upgrade: Version 11 → 12
--
-- This migration adds support for tracking file access state.
-- When filesystem permission errors occur during scanning or analysis,
-- the item's access state is updated rather than aborting the scan.
--
-- Changes:
-- Items table:
--   - Add access (integer enum: 0=Ok, 1=MetaError, 2=ReadError)
--
-- Changes table:
--   - Add access_old (previous access state when changed)
--   - Add access_new (new access state when changed)
--
-- Scan queue table:
--   - Add analysis_hwm (high water mark for analysis restart resilience)
--

-- Verify schema version is exactly 11
SELECT 1 / (CASE WHEN (SELECT value FROM meta WHERE key = 'schema_version') = '11' THEN 1 ELSE 0 END);

-- ========================================
-- Update items table
-- ========================================

-- Add access state enum (0=Ok, 1=MetaError, 2=ReadError)
-- Default is 0 (Ok) - no known access issues
ALTER TABLE items ADD COLUMN access INTEGER NOT NULL DEFAULT 0;

-- ========================================
-- Update changes table
-- ========================================

-- Add access state tracking for change records
ALTER TABLE changes ADD COLUMN access_old INTEGER DEFAULT NULL;
ALTER TABLE changes ADD COLUMN access_new INTEGER DEFAULT NULL;

-- ========================================
-- Update scan_queue table
-- ========================================

-- Add high water mark for analysis phase restart resilience
-- Stores the last item_id processed during analysis so we can resume
-- after app restart without reprocessing already-processed items
ALTER TABLE scan_queue ADD COLUMN analysis_hwm INTEGER DEFAULT NULL;

-- ========================================
-- Finalize migration
-- ========================================

UPDATE meta SET value = '12' WHERE key = 'schema_version';
"#;
