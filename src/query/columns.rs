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
    Path,
    Val,
    ItemType,
    AlertType,
    AlertStatus,
    ScanState,
    Access,
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
                "ISO dates or ranges e.g. 2025-01-01, 2025-02-01..2025-02-14\n(null and not null also ok)",
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
            ColType::Path => ColTypeInfo::new(
                Rule::path_filter_EOI,
                "Path",
                "Single-quoted substring(s) e.g. '/var/log', 'docs/report.pdf'",
            ),
            ColType::Val => ColTypeInfo::new(
                Rule::val_filter_EOI,
                "Val",
                "Validity codes: V (valid), I (invalid), N (no validator), U (unknown)\nComma-separate codes (null and not_null also ok)",
            ),
            ColType::ItemType => ColTypeInfo::new(
                Rule::item_type_filter_EOI,
                "Item Type",
                "F (file), D (directory), S (symlink), U (unknown)\nComma-separated values",
            ),
            ColType::AlertType => ColTypeInfo::new(
                Rule::alert_type_filter_EOI,
                "Alert Type",
                "Alert types: H (suspicious hash), I (invalid item), A (access denied)\nComma-separated values",
            ),
            ColType::AlertStatus => ColTypeInfo::new(
                Rule::alert_status_filter_EOI,
                "Alert Status",
                "Alert status types: D (dismissed), F (flagged), O (open)\nComma-separated values",
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
}

impl ColSpec {
    const fn new(
        name_db: &'static str,
        name_display: &'static str,
        is_default: bool,
        col_type: ColType,
        alignment: ColAlign,
    ) -> Self {
        ColSpec {
            name_db,
            name_display,
            is_default,
            col_type,
            col_align: alignment,
        }
    }
}

pub const ROOTS_QUERY_COLS: ColMap = phf_ordered_map! {
    "root_id" => ColSpec::new("root_id", "Root Id", true, ColType::Id, ColAlign::Right),
    "root_path" => ColSpec::new( "root_path", "Root Path", true, ColType::Path, ColAlign::Left),
};

pub const SCANS_QUERY_COLS: ColMap = phf_ordered_map! {
    "scan_id"  => ColSpec::new("scans.scan_id", "Scan Id", true, ColType::Id, ColAlign::Right),
    "root_id" => ColSpec::new("root_id", "Root Id", true, ColType::Id, ColAlign::Right),
    "schedule_id" => ColSpec::new("schedule_id", "Schedule Id", true, ColType::Id, ColAlign::Right),
    "started_at" => ColSpec::new("started_at", "Started At", true, ColType::Date, ColAlign::Center),
    "ended_at" => ColSpec::new("ended_at", "Ended At", true, ColType::Date, ColAlign::Center),
    "was_restarted" => ColSpec::new("was_restarted", "Was Restarted", true, ColType::Bool, ColAlign::Center),
    "scan_state" => ColSpec::new("state", "State", false, ColType::ScanState, ColAlign::Center),
    "is_hash" => ColSpec::new("is_hash", "Is Hash", true, ColType::Bool, ColAlign::Center),
    "hash_all" => ColSpec::new("hash_all", "Hash All", false, ColType::Bool, ColAlign::Center),
    "is_val" => ColSpec::new("is_val", "Is Val", true, ColType::Bool, ColAlign::Center),
    "val_all" => ColSpec::new("val_all", "Val All", false, ColType::Bool, ColAlign::Center),
    "file_count" => ColSpec::new("file_count", "Files", true, ColType::Int, ColAlign::Right),
    "folder_count" => ColSpec::new("folder_count", "Folders", true, ColType::Int, ColAlign::Right),
    "total_size" => ColSpec::new("total_size", "Total Size", true, ColType::Int, ColAlign::Right),
    "alert_count" => ColSpec::new("alert_count", "Alerts", true, ColType::Int, ColAlign::Right),
    "add_count" => ColSpec::new("add_count", "Adds", true, ColType::Int, ColAlign::Right),
    "modify_count" => ColSpec::new("modify_count", "Modifies", true, ColType::Int, ColAlign::Right),
    "delete_count" => ColSpec::new("delete_count", "Deletes", true, ColType::Int, ColAlign::Right),
    "error" => ColSpec::new("error", "Error", false, ColType::String, ColAlign::Left),
};

pub const ITEMS_QUERY_COLS: ColMap = phf_ordered_map! {
    "item_id" => ColSpec::new("i.item_id", "Item Id", true, ColType::Id, ColAlign::Right),
    "root_id" => ColSpec::new("i.root_id", "Root Id", true, ColType::Id, ColAlign::Right),
    "item_path" => ColSpec::new("i.item_path", "Item Path", true, ColType::Path, ColAlign::Left),
    "item_name" => ColSpec::new("i.item_name", "Item Name", false, ColType::Path, ColAlign::Left),
    "item_type" => ColSpec::new("i.item_type", "Type", true, ColType::ItemType, ColAlign::Center),
    "version_id" => ColSpec::new("iv.version_id", "Version Id", false, ColType::Id, ColAlign::Right),
    "first_scan_id" => ColSpec::new("iv.first_scan_id", "First Scan", false, ColType::Id, ColAlign::Right),
    "last_scan_id" => ColSpec::new("iv.last_scan_id", "Last Scan", true, ColType::Id, ColAlign::Right),
    "is_deleted" => ColSpec::new("iv.is_deleted", "Deleted", true, ColType::Bool, ColAlign::Center),
    "access" => ColSpec::new("iv.access", "Access", false, ColType::Access, ColAlign::Center),
    "mod_date" => ColSpec::new("iv.mod_date", "Mod Date", true, ColType::Date, ColAlign::Center),
    "size" => ColSpec::new("iv.size", "Size", false, ColType::Int, ColAlign::Right),
    "last_hash_scan" => ColSpec::new("iv.last_hash_scan", "Last Hash Scan", false, ColType::Id, ColAlign::Right),
    "file_hash" => ColSpec::new("iv.file_hash", "File Hash", false, ColType::String, ColAlign::Left),
    "last_val_scan" => ColSpec::new("iv.last_val_scan", "Last Val Scan", false, ColType::Id, ColAlign::Right),
    "val" => ColSpec::new("iv.val", "Val", false, ColType::Val, ColAlign::Center),
    "val_error" => ColSpec::new("iv.val_error", "Val Error", false, ColType::String, ColAlign::Left),
};

pub const VERSIONS_QUERY_COLS: ColMap = phf_ordered_map! {
    "version_id" => ColSpec::new("iv.version_id", "Version Id", true, ColType::Id, ColAlign::Right),
    "item_id" => ColSpec::new("iv.item_id", "Item Id", true, ColType::Id, ColAlign::Right),
    "root_id" => ColSpec::new("i.root_id", "Root Id", true, ColType::Id, ColAlign::Right),
    "item_path" => ColSpec::new("i.item_path", "Item Path", false, ColType::Path, ColAlign::Left),
    "item_name" => ColSpec::new("i.item_name", "Item Name", false, ColType::Path, ColAlign::Left),
    "item_type" => ColSpec::new("i.item_type", "Type", true, ColType::ItemType, ColAlign::Center),
    "first_scan_id" => ColSpec::new("iv.first_scan_id", "First Scan", true, ColType::Id, ColAlign::Right),
    "last_scan_id" => ColSpec::new("iv.last_scan_id", "Last Scan", true, ColType::Id, ColAlign::Right),
    "is_deleted" => ColSpec::new("iv.is_deleted", "Deleted", true, ColType::Bool, ColAlign::Center),
    "access" => ColSpec::new("iv.access", "Access", false, ColType::Access, ColAlign::Center),
    "mod_date" => ColSpec::new("iv.mod_date", "Mod Date", false, ColType::Date, ColAlign::Center),
    "size" => ColSpec::new("iv.size", "Size", false, ColType::Int, ColAlign::Right),
    "last_hash_scan" => ColSpec::new("iv.last_hash_scan", "Last Hash Scan", false, ColType::Id, ColAlign::Right),
    "file_hash" => ColSpec::new("iv.file_hash", "File Hash", false, ColType::String, ColAlign::Left),
    "last_val_scan" => ColSpec::new("iv.last_val_scan", "Last Val Scan", false, ColType::Id, ColAlign::Right),
    "val" => ColSpec::new("iv.val", "Val", false, ColType::Val, ColAlign::Center),
    "val_error" => ColSpec::new("iv.val_error", "Val Error", false, ColType::String, ColAlign::Left),
};

pub const ALERTS_QUERY_COLS: ColMap = phf_ordered_map! {
    "alert_id" => ColSpec::new("alert_id", "Alert Id", false, ColType::Id, ColAlign::Right),
    "alert_type" => ColSpec::new("alert_type", "Type", true, ColType::AlertType, ColAlign::Center),
    "alert_status" => ColSpec::new("alert_status", "Status", true, ColType::AlertStatus, ColAlign::Center),
    "root_id" => ColSpec::new("scans.root_id", "Root Id", false, ColType::Id, ColAlign::Right),
    "scan_id" => ColSpec::new("alerts.scan_id", "Scan Id", false, ColType::Id, ColAlign::Right),
    "item_id" => ColSpec::new("alerts.item_id", "Item Id", false, ColType::Id, ColAlign::Right),
    "item_path" => ColSpec::new("items.item_path", "Item Path", true, ColType::Path, ColAlign::Left),
    "created_at" => ColSpec::new("created_at", "Created", true, ColType::Date, ColAlign::Center),
    "updated_at" => ColSpec::new("updated_at", "Updated", false, ColType::Date, ColAlign::Center),
    "prev_hash_scan" => ColSpec::new("prev_hash_scan", "Prev Hash Scan", false, ColType::Id, ColAlign::Right),
    "hash_old" => ColSpec::new("hash_old", "Hash Old", false, ColType::String, ColAlign::Left),
    "hash_new" => ColSpec::new("hash_new", "Hash New", false, ColType::String, ColAlign::Left),
    "val_error" => ColSpec::new("alerts.val_error", "Val Error", true, ColType::String, ColAlign::Left),
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
