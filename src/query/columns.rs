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

impl ColAlign {
    pub fn to_tabled(self) -> tabled::settings::Alignment {
        match self {
            ColAlign::Left => tabled::settings::Alignment::left(),
            ColAlign::Center => tabled::settings::Alignment::center(),
            ColAlign::Right => tabled::settings::Alignment::right(),
        }
    }

    pub fn to_ratatui(self) -> ratatui::layout::Alignment {
        match self {
            ColAlign::Left => ratatui::layout::Alignment::Left,
            ColAlign::Center => ratatui::layout::Alignment::Center,
            ColAlign::Right => ratatui::layout::Alignment::Right,
        }
    }
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
    ChangeType,
    AlertType,
    AlertStatus,
    ScanState,
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
                "F (file), D (directory), S (symlink)\nComma-separated values",
            ),
            ColType::ChangeType => ColTypeInfo::new(
                Rule::change_type_filter_EOI,
                "Change Type",
                "Change types: A (add), M (modify), D (delete)\nComma-separated values",
            ),
            ColType::AlertType => ColTypeInfo::new(
                Rule::alert_type_filter_EOI,
                "Alert Type",
                "Alert types: H (suspicious hash), I (invalid item)\nComma-separated values",
            ),
            ColType::AlertStatus => ColTypeInfo::new(
                Rule::alert_status_filter_EOI,
                "Alert Status",
                "Alert status types: D (dismissed), F (flagged), O (open)\nComma-separated values",
            ),
            ColType::ScanState => ColTypeInfo::new(
                Rule::scan_state_filter_EOI,
                "Scan State",
                "Scan states: S (Scanning), W (Sweeping), A (Analyzing), C (Completed), P (Stopped), E (Error)\nComma-separated values",
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
    "scan_state" => ColSpec::new("state", "State", false, ColType::ScanState, ColAlign::Center),
    "is_hash" => ColSpec::new("is_hash", "Is Hash", true, ColType::Bool, ColAlign::Center),
    "hash_all" => ColSpec::new("hash_all", "Hash All", false, ColType::Bool, ColAlign::Center),
    "is_val" => ColSpec::new("is_val", "Is Val", true, ColType::Bool, ColAlign::Center),
    "val_all" => ColSpec::new("val_all", "Val All", false, ColType::Bool, ColAlign::Center),
    "scan_time" => ColSpec::new("scan_time", "Scan Time", true, ColType::Date, ColAlign::Center),
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
    "item_id" => ColSpec::new("item_id", "Item Id", true, ColType::Id, ColAlign::Right),
    "root_id" => ColSpec::new("root_id", "Root Id", true, ColType::Id, ColAlign::Right),
    "item_path" => ColSpec::new("item_path", "Item Path", true, ColType::Path, ColAlign::Left),
    "item_type" => ColSpec::new("item_type", "Type", true, ColType::ItemType, ColAlign::Center),
    "last_scan" => ColSpec::new("last_scan", "Last Scan", true, ColType::Id, ColAlign::Right),
    "is_ts" => ColSpec::new("is_ts", "Is TS", true, ColType::Bool, ColAlign::Center),
    "mod_date" => ColSpec::new("mod_date", "Mod Date", true, ColType::Date, ColAlign::Center),
    "size" => ColSpec::new("size", "Size", false, ColType::Int, ColAlign::Right),
    "last_hash_scan" => ColSpec::new("last_hash_scan", "Last Hash Scan", false, ColType::Id, ColAlign::Right),
    "file_hash" => ColSpec::new("file_hash", "File Hash", false, ColType::String, ColAlign::Left),
    "last_val_scan" => ColSpec::new("last_val_scan", "Last Val Scan", false, ColType::Id, ColAlign::Right),
    "val" => ColSpec::new("val", "Val", false, ColType::Val, ColAlign::Center),
    "val_error" => ColSpec::new("val_error", "Val Error", false, ColType::String, ColAlign::Left),
};

pub const CHANGES_QUERY_COLS: ColMap = phf_ordered_map! {
    "change_id" => ColSpec::new("changes.change_id", "Change Id", true, ColType::Id, ColAlign::Right),
    "root_id" => ColSpec::new("items.root_id", "Root Id", true, ColType::Id, ColAlign::Right),
    "scan_id"  => ColSpec::new("changes.scan_id", "Scan Id", true, ColType::Id, ColAlign::Right),
    "item_id" => ColSpec::new("changes.item_id", "Item Id", true, ColType::Id, ColAlign::Right),
    "item_path" => ColSpec::new("items.item_path", "Item Path", false, ColType::Path, ColAlign::Left),
    "change_type" => ColSpec::new("change_type", "Type", true, ColType::ChangeType, ColAlign::Center),
    "is_undelete" => ColSpec::new("is_undelete", "Is Undelete", false, ColType::Bool, ColAlign::Center),
    "meta_change" => ColSpec::new("meta_change", "Meta Change", false, ColType::Bool, ColAlign::Center),
    "mod_date_old" => ColSpec::new("mod_date_old", "Mod Date Old", false, ColType::Date, ColAlign::Center),
    "mod_date_new" => ColSpec::new("mod_date_new", "Mod Date New", false, ColType::Date, ColAlign::Center),
    "size_old" => ColSpec::new("size_old", "Size Old", false, ColType::Int, ColAlign::Right),
    "size_new" => ColSpec::new("size_new", "Size New", false, ColType::Int, ColAlign::Right),
    "hash_change" => ColSpec::new("hash_change", "Hash Change", false, ColType::Bool, ColAlign::Center),
    "last_hash_scan_old" => ColSpec::new("last_hash_scan_old", "Last Hash Scan Old", false, ColType::Id, ColAlign::Right),
    "hash_old" => ColSpec::new("hash_old", "Hash Old", false, ColType::String, ColAlign::Left),
    "hash_new" => ColSpec::new("hash_new", "Hash New", false, ColType::String, ColAlign::Left),
    "val_change" => ColSpec::new("val_change", "Val Change", false, ColType::Bool, ColAlign::Center),
    "last_val_scan_old" => ColSpec::new("last_val_scan_old", "Last Val Scan Old", false, ColType::Id, ColAlign::Right),
    "val_old" => ColSpec::new("val_old", "Val Old", false, ColType::Val, ColAlign::Center),
    "val_new" => ColSpec::new("val_new", "Val New", false, ColType::Val, ColAlign::Center),
    "val_error_old" => ColSpec::new("val_error_old", "Val Error Old", false, ColType::String, ColAlign::Left),
    "val_error_new" => ColSpec::new("val_error_new", "Val Error New", false, ColType::String, ColAlign::Left),
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
