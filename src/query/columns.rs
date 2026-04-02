use phf::ordered_map::{Entries, Values};
use phf_macros::phf_ordered_map;
use serde::Serialize;

use super::Rule;

pub type ColMap = phf::OrderedMap<&'static str, ColSpec>;

#[derive(Debug, Clone, Copy, Serialize)]
pub enum ColAlign {
    Left,
    Center,
    Right,
}

#[derive(Clone, Copy, Debug)]
pub struct ColTypeInfo {
    pub rule: Rule,
    pub type_name: &'static str,
    pub tip: &'static str,
}

impl ColTypeInfo {
    fn new(rule: Rule, type_name: &'static str, tip: &'static str) -> Self {
        ColTypeInfo {
            rule,
            type_name,
            tip,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub enum ColType {
    Id,
    Date,
    Bool,
    String,
    Hash,
    Path,
    ValState,
    ItemType,
    ScanState,
    Access,
    HashState,
    Int,
}

impl ColType {
    pub fn info(&self) -> ColTypeInfo {
        match self {
            ColType::Id => ColTypeInfo::new(
                Rule::id_filter_EOI,
                "Id",
                "Comma-separated ids or ranges e.g. 3, 5..10\n(null and not null also ok)",
            ),
            ColType::Int => ColTypeInfo::new(
                Rule::int_filter_EOI,
                "Int",
                "Single comparator e.g. > 1024  or  < 10",
            ),
            ColType::Date => ColTypeInfo::new(
                Rule::date_filter_EOI,
                "Date",
                "Dates, datetimes, or epochs e.g. 2025-01-01, 2025-01-01 14:30:00, 1735689600\nRanges e.g. 2025-01-01..2025-01-31, forms can be mixed\n(null and not null also ok)",
            ),
            ColType::Bool => ColTypeInfo::new(
                Rule::bool_filter_EOI,
                "Boolean",
                "true or false (null and not null also ok)",
            ),
            ColType::String => ColTypeInfo::new(
                Rule::string_filter_EOI,
                "String",
                "Single-quoted substring(s) e.g. 'disk', 'error'\nComma-separate values (null and not null also ok)",
            ),
            ColType::Hash => ColTypeInfo::new(
                Rule::hash_filter_EOI,
                "Hash",
                "Single-quoted hex substring(s) e.g. 'a1b2c3'\nComma-separate values (null and not null also ok)",
            ),
            ColType::Path => ColTypeInfo::new(
                Rule::path_filter_EOI,
                "Path",
                "Single-quoted substring(s) e.g. '/var/log', 'docs/report.pdf'\nUse trailing slash for folder prefix: '/photos/' not '/photos' (avoids matching '/photos-backup/')",
            ),
            ColType::ValState => ColTypeInfo::new(
                Rule::val_state_filter_EOI,
                "Val State",
                "Validity codes: V (valid), I (invalid), N (no validator), U (unknown)\nComma-separate codes (null and not_null also ok)",
            ),
            ColType::ItemType => ColTypeInfo::new(
                Rule::item_type_filter_EOI,
                "Item Type",
                "F (file), D (directory), S (symlink), U (unknown)\nComma-separated values",
            ),
            ColType::ScanState => ColTypeInfo::new(
                Rule::scan_state_filter_EOI,
                "Scan State",
                "Scan states: S (Scanning), W (Sweeping), AF (Analyzing Files), AS (Analyzing Scan), C (Completed), P (Stopped), E (Error)\nA is shorthand for AF. Comma-separated values",
            ),
            ColType::Access => ColTypeInfo::new(
                Rule::access_filter_EOI,
                "Access",
                "Access states: N (No Error), M (Meta Error), R (Read Error)\nComma-separated values (null and not null also ok)",
            ),
            ColType::HashState => ColTypeInfo::new(
                Rule::hash_state_filter_EOI,
                "Hash State",
                "Hash states: U (Unknown), V (Valid), S (Suspect)\nComma-separated values (null and not null also ok)",
            ),
        }
    }

    pub fn collation(&self) -> Option<&'static str> {
        match self {
            ColType::Path => Some("natural_path"),
            _ => None,
        }
    }
}

#[derive(Debug)]
pub struct ColSpec {
    pub name_db: &'static str,
    pub name_display: &'static str,
    pub is_default: bool,
    pub col_type: ColType,
    pub col_align: ColAlign,
    pub description: &'static str,
}

impl ColSpec {
    const fn new(
        name_db: &'static str,
        name_display: &'static str,
        is_default: bool,
        col_type: ColType,
        alignment: ColAlign,
        description: &'static str,
    ) -> Self {
        ColSpec {
            name_db,
            name_display,
            is_default,
            col_type,
            col_align: alignment,
            description,
        }
    }
}

pub const ROOTS_QUERY_COLS: ColMap = phf_ordered_map! {
    "root_id" => ColSpec::new("root_id", "Root Id", true, ColType::Id, ColAlign::Right, "Unique identifier for this monitored root"),
    "root_path" => ColSpec::new("root_path", "Root Path", true, ColType::Path, ColAlign::Left, "Filesystem path of the monitored directory"),
};

pub const SCANS_QUERY_COLS: ColMap = phf_ordered_map! {
    "scan_id"  => ColSpec::new("scans.scan_id", "Scan Id", true, ColType::Id, ColAlign::Right, "Unique identifier for this scan"),
    "root_id" => ColSpec::new("root_id", "Root Id", true, ColType::Id, ColAlign::Right, "Root that was scanned"),
    "schedule_id" => ColSpec::new("schedule_id", "Schedule Id", true, ColType::Id, ColAlign::Right, "Schedule that triggered this scan (NULL for manual scans)"),
    "started_at" => ColSpec::new("started_at", "Started At", true, ColType::Date, ColAlign::Center, "When the scan started"),
    "ended_at" => ColSpec::new("ended_at", "Ended At", true, ColType::Date, ColAlign::Center, "When the scan completed (NULL if still running)"),
    "was_restarted" => ColSpec::new("was_restarted", "Was Restarted", true, ColType::Bool, ColAlign::Center, "True if scan was interrupted and resumed"),
    "scan_state" => ColSpec::new("state", "State", true, ColType::ScanState, ColAlign::Center, "Current scan state (Scanning, Completed, Error, etc.)"),
    "is_hash" => ColSpec::new("is_hash", "Is Hash", true, ColType::Bool, ColAlign::Center, "Whether this scan hashed new/changed files"),
    "hash_all" => ColSpec::new("hash_all", "Hash All", false, ColType::Bool, ColAlign::Center, "Whether this scan hashed all files including unchanged"),
    "is_val" => ColSpec::new("is_val", "Is Val", true, ColType::Bool, ColAlign::Center, "Whether this scan validated file contents"),
    "file_count" => ColSpec::new("file_count", "Files", true, ColType::Int, ColAlign::Right, "Total files found in this scan"),
    "folder_count" => ColSpec::new("folder_count", "Folders", true, ColType::Int, ColAlign::Right, "Total folders found in this scan"),
    "total_size" => ColSpec::new("total_size", "Total Size", true, ColType::Int, ColAlign::Right, "Total size in bytes of all items in this scan"),
    "new_hash_suspect_count" => ColSpec::new("new_hash_suspect_count", "New Hash Suspect", false, ColType::Int, ColAlign::Right, "Suspect hash observations first seen in this scan"),
    "new_val_invalid_count" => ColSpec::new("new_val_invalid_count", "New Val Invalid", false, ColType::Int, ColAlign::Right, "Validation failures first seen in this scan"),
    "add_count" => ColSpec::new("add_count", "Adds", true, ColType::Int, ColAlign::Right, "Items added in this scan"),
    "modify_count" => ColSpec::new("modify_count", "Modifies", true, ColType::Int, ColAlign::Right, "Items modified in this scan"),
    "delete_count" => ColSpec::new("delete_count", "Deletes", true, ColType::Int, ColAlign::Right, "Items deleted in this scan"),
    "val_unknown_count" => ColSpec::new("val_unknown_count", "Val Unknown", false, ColType::Int, ColAlign::Right, "Files with unknown validation state at scan completion"),
    "val_valid_count" => ColSpec::new("val_valid_count", "Val Valid", false, ColType::Int, ColAlign::Right, "Files with valid validation state at scan completion"),
    "val_invalid_count" => ColSpec::new("val_invalid_count", "Val Invalid", false, ColType::Int, ColAlign::Right, "Files with invalid validation state at scan completion"),
    "val_no_validator_count" => ColSpec::new("val_no_validator_count", "Val No Validator", false, ColType::Int, ColAlign::Right, "Files with no available validator at scan completion"),
    "hash_unknown_count" => ColSpec::new("hash_unknown_count", "Hash Unknown", false, ColType::Int, ColAlign::Right, "Files with unknown hash state at scan completion"),
    "hash_baseline_count" => ColSpec::new("hash_baseline_count", "Hash Baseline", false, ColType::Int, ColAlign::Right, "Files with baseline (unchanged) hash at scan completion"),
    "hash_suspect_count" => ColSpec::new("hash_suspect_count", "Hash Suspect", false, ColType::Int, ColAlign::Right, "Files with suspect (changed) hash at scan completion"),
    "error" => ColSpec::new("error", "Error", false, ColType::String, ColAlign::Left, "Error message if scan failed"),
};

pub const ITEMS_QUERY_COLS: ColMap = phf_ordered_map! {
    "item_id" => ColSpec::new("i.item_id", "Item Id", true, ColType::Id, ColAlign::Right, "Unique identifier for this item"),
    "root_id" => ColSpec::new("i.root_id", "Root Id", true, ColType::Id, ColAlign::Right, "Root this item belongs to"),
    "item_path" => ColSpec::new("i.item_path", "Item Path", true, ColType::Path, ColAlign::Left, "Full filesystem path of the item"),
    "item_name" => ColSpec::new("i.item_name", "Item Name", true, ColType::Path, ColAlign::Left, "File or folder name only"),
    "file_extension" => ColSpec::new("i.file_extension", "Extension", true, ColType::String, ColAlign::Left, "Lowercase file extension without dot (NULL for folders)"),
    "item_type" => ColSpec::new("i.item_type", "Type", true, ColType::ItemType, ColAlign::Center, "File (F), Directory (D), or Symlink (S)"),
    "has_validator" => ColSpec::new("i.has_validator", "Has Validator", false, ColType::Bool, ColAlign::Center, "Whether a structural validator exists for this file type"),
    "do_not_validate" => ColSpec::new("i.do_not_validate", "Do Not Validate", false, ColType::Bool, ColAlign::Center, "Whether user has opted this item out of validation"),
};

pub const VERSIONS_QUERY_COLS: ColMap = phf_ordered_map! {
    "item_version" => ColSpec::new("iv.item_version", "Version", true, ColType::Id, ColAlign::Right, "Per-item sequence number (1, 2, 3, ...) assigned chronologically"),
    "item_id" => ColSpec::new("iv.item_id", "Item Id", true, ColType::Id, ColAlign::Right, "Item this version belongs to"),
    "root_id" => ColSpec::new("i.root_id", "Root Id", true, ColType::Id, ColAlign::Right, "Root this item belongs to"),
    "item_path" => ColSpec::new("i.item_path", "Item Path", true, ColType::Path, ColAlign::Left, "Full filesystem path of the item"),
    "item_name" => ColSpec::new("i.item_name", "Item Name", false, ColType::Path, ColAlign::Left, "File or folder name only"),
    "file_extension" => ColSpec::new("i.file_extension", "Extension", false, ColType::String, ColAlign::Left, "Lowercase file extension without dot (NULL for folders)"),
    "item_type" => ColSpec::new("i.item_type", "Type", true, ColType::ItemType, ColAlign::Center, "File (F), Directory (D), or Symlink (S)"),
    "first_scan_id" => ColSpec::new("iv.first_scan_id", "First Scan", true, ColType::Id, ColAlign::Right, "Scan that first observed this version"),
    "last_scan_id" => ColSpec::new("iv.last_scan_id", "Last Scan", true, ColType::Id, ColAlign::Right, "Last scan where this version was still current"),
    "is_added" => ColSpec::new("iv.is_added", "Added", false, ColType::Bool, ColAlign::Center, "True if this version represents an add (new item or restoration of a deleted item)"),
    "is_deleted" => ColSpec::new("iv.is_deleted", "Deleted", true, ColType::Bool, ColAlign::Center, "True if this version represents a deletion"),
    "is_current" => ColSpec::new("(iv.first_scan_id = (SELECT MAX(first_scan_id) FROM item_versions WHERE item_id = iv.item_id))", "Current", false, ColType::Bool, ColAlign::Center, "True for the latest version of each item (includes deleted items — combine with is_deleted:(F) for live items)"),
    "access" => ColSpec::new("iv.access", "Access", false, ColType::Access, ColAlign::Center, "Filesystem access state: No Error, Meta Error, or Read Error"),
    "mod_date" => ColSpec::new("iv.mod_date", "Mod Date", true, ColType::Date, ColAlign::Center, "Filesystem modification timestamp"),
    "size" => ColSpec::new("iv.size", "Size", true, ColType::Int, ColAlign::Right, "Size in bytes"),
    "add_count" => ColSpec::new("iv.add_count", "Adds", false, ColType::Int, ColAlign::Right, "Descendant items added (folders only; NULL for files)"),
    "modify_count" => ColSpec::new("iv.modify_count", "Modifies", false, ColType::Int, ColAlign::Right, "Descendant items modified (folders only; NULL for files)"),
    "delete_count" => ColSpec::new("iv.delete_count", "Deletes", false, ColType::Int, ColAlign::Right, "Descendant items deleted (folders only; NULL for files)"),
    "unchanged_count" => ColSpec::new("iv.unchanged_count", "Unchanged", false, ColType::Int, ColAlign::Right, "Descendant items unchanged (folders only; NULL for files)"),
    "val_scan_id" => ColSpec::new("iv.val_scan_id", "Val Scan", false, ColType::Id, ColAlign::Right, "Scan in which this version was validated (NULL if not validated; may be later than first_scan_id)"),
    "val_state" => ColSpec::new("iv.val_state", "Val State", false, ColType::ValState, ColAlign::Center, "Validation result: Valid, Invalid, No Validator, or Unknown"),
    "val_error" => ColSpec::new("iv.val_error", "Val Error", false, ColType::String, ColAlign::Left, "Validation error details (NULL unless val_state is Invalid)"),
    "val_reviewed_at" => ColSpec::new("iv.val_reviewed_at", "Val Reviewed", false, ColType::Date, ColAlign::Center, "When user marked this validation issue as reviewed (NULL until reviewed)"),
    "hash_reviewed_at" => ColSpec::new("iv.hash_reviewed_at", "Hash Reviewed", false, ColType::Date, ColAlign::Center, "When user marked this hash integrity issue as reviewed (NULL until reviewed)"),
};

pub const HASHES_QUERY_COLS: ColMap = phf_ordered_map! {
    "item_id" => ColSpec::new("hv.item_id", "Item Id", true, ColType::Id, ColAlign::Right, "Item this hash observation belongs to"),
    "item_version" => ColSpec::new("hv.item_version", "Version", true, ColType::Id, ColAlign::Right, "Item version this hash was computed for"),
    "item_path" => ColSpec::new("i.item_path", "Item Path", true, ColType::Path, ColAlign::Left, "Full filesystem path of the item"),
    "item_name" => ColSpec::new("i.item_name", "Item Name", false, ColType::Path, ColAlign::Left, "File name only"),
    "first_scan_id" => ColSpec::new("hv.first_scan_id", "First Scan", true, ColType::Id, ColAlign::Right, "Scan that first computed this hash"),
    "last_scan_id" => ColSpec::new("hv.last_scan_id", "Last Scan", true, ColType::Id, ColAlign::Right, "Last scan where this hash was still current"),
    "file_hash" => ColSpec::new("hv.file_hash", "File Hash", true, ColType::Hash, ColAlign::Left, "SHA-256 hash of file contents"),
    "hash_state" => ColSpec::new("hv.hash_state", "Hash State", true, ColType::HashState, ColAlign::Center, "Baseline (first/expected hash) or Suspect (hash changed without metadata change)"),
};

#[derive(Debug, Copy, Clone)]
pub struct ColSet {
    col_map: &'static ColMap,
}

impl ColSet {
    pub fn new(col_map: &'static ColMap) -> Self {
        ColSet { col_map }
    }

    pub fn col_set(&self) -> &ColMap {
        self.col_map
    }

    pub fn values(&self) -> Values<'_, &str, ColSpec> {
        self.col_map.values()
    }

    pub fn entries(&self) -> Entries<'_, &'static str, ColSpec> {
        self.col_map.entries()
    }

    pub fn col_name_to_db(&self, column_name: &str) -> Option<&'static str> {
        match self.col_map.get(column_name) {
            Some(col_spec) => Some(col_spec.name_db),
            None => None,
        }
    }
}
