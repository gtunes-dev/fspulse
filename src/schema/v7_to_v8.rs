pub const UPGRADE_7_TO_8_SQL: &str = r#"
--
-- Schema Upgrade: Version 7 → 8
--
-- This migration adds natural path collation and improves index structure:
-- - roots.root_path: Add COLLATE natural_path to idx_roots_path
-- - items indexes:
--   - Add idx_items_path: (item_path COLLATE natural_path) for path-only queries
--   - Rename idx_items_path → idx_items_root_path: (root_id, item_path, item_type)
--   - Rename idx_items_scan → idx_items_root_scan: (root_id, last_scan, is_ts)
--
-- This enables natural, case-insensitive path sorting that matches macOS Finder
-- behavior, where /proj and its children sort before /proj-A.
--

-- Disable foreign key constraints BEFORE transaction starts
PRAGMA foreign_keys = OFF;

BEGIN TRANSACTION;

-- Verify schema version is exactly 7
SELECT 1 / (CASE WHEN (SELECT value FROM meta WHERE key = 'schema_version') = '7' THEN 1 ELSE 0 END);

-- ========================================
-- Update roots table indexes
-- ========================================

-- Drop existing index
DROP INDEX IF EXISTS idx_roots_path;

-- Recreate with natural_path collation
CREATE INDEX idx_roots_path ON roots (root_path COLLATE natural_path);

-- ========================================
-- Update items table indexes
-- ========================================

-- Drop existing indexes
DROP INDEX IF EXISTS idx_items_path;
DROP INDEX IF EXISTS idx_items_scan;

-- Create new indexes with proper naming and natural_path collation
CREATE INDEX idx_items_path ON items (item_path COLLATE natural_path);
CREATE INDEX idx_items_root_path ON items (root_id, item_path COLLATE natural_path, item_type);
CREATE INDEX idx_items_root_scan ON items (root_id, last_scan, is_ts);

-- ========================================
-- Finalize migration
-- ========================================

-- Update schema version
UPDATE meta SET value = '8' WHERE key = 'schema_version';

COMMIT;

-- Re-enable foreign key constraints
PRAGMA foreign_keys = ON;
"#;
