use phf::ordered_map::{Entries, Values};
use phf_macros::phf_ordered_map;

use super::Rule;

pub type ColMap = phf::OrderedMap<&'static str, ColSpec>;

#[derive(Debug)]
pub enum ColAlign {
    Left,
    Center,
    Right,
}

impl ColAlign {
    pub fn to_tabled(&self) -> tabled::settings::Alignment {
        match self {
            ColAlign::Left => tabled::settings::Alignment::left(),
            ColAlign::Center => tabled::settings::Alignment::center(),
            ColAlign::Right => tabled::settings::Alignment::right(),
        }
    }

    pub fn to_ratatui(&self) -> ratatui::layout::Alignment {
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
    Enum,
    Int,
}

impl ColType {
    pub fn info(&self) -> ColTypeInfo {
        match self {
            ColType::Id => ColTypeInfo::new(Rule::id_filter_values, "Id", "1, 3, 4..6"),
            _ => ColTypeInfo::new(Rule::id_filter_values, "Id", "1, 3, 4..6"),
        }
    }
}

#[derive(Debug)]
pub struct ColSpec {
    pub name_db: &'static str,
    pub is_default: bool,
    pub in_select_list: bool,
    pub col_type: ColType,
    pub col_align: ColAlign,
}

impl ColSpec {
    const fn new(
        name_db: &'static str,
        is_default: bool,
        in_select_list: bool,
        col_type: ColType,
        alignment: ColAlign,
    ) -> Self {
        ColSpec {
            name_db,
            is_default,
            in_select_list,
            col_type,
            col_align: alignment,
        }
    }
}

pub const ROOTS_QUERY_COLS: ColMap = phf_ordered_map! {
    "root_id" => ColSpec::new("root_id", true, true, ColType::Id, ColAlign::Right),
    "root_path" => ColSpec::new( "root_path", true, true, ColType::Path, ColAlign::Right),
};

pub const SCANS_QUERY_COLS: ColMap = phf_ordered_map! {
    "scan_id"  => ColSpec::new("scans.scan_id", true, true, ColType::Id, ColAlign::Right),
    "root_id" => ColSpec::new("root_id", true, true, ColType::Id, ColAlign::Right),
    "state" => ColSpec::new("state", false, true, ColType::Int, ColAlign::Center),
    "is_hash" => ColSpec::new("is_hash", true, true, ColType::Bool, ColAlign::Center),
    "hash_all" => ColSpec::new("hash_all", false, true, ColType::Bool, ColAlign::Center),
    "is_val" => ColSpec::new("is_val", true, true, ColType::Bool, ColAlign::Center),
    "val_all" => ColSpec::new("val_all", false, true, ColType::Bool, ColAlign::Center),
    "scan_time" => ColSpec::new("scan_time", true, true, ColType::Date, ColAlign::Center),
    "file_count" => ColSpec::new("file_count", true, true, ColType::Int, ColAlign::Right),
    "folder_count" => ColSpec::new("folder_count", true, true, ColType::Int, ColAlign::Right),
    "adds" => ColSpec::new("adds", true, false, ColType::Int, ColAlign::Right),
    "modifies" => ColSpec::new("modifies", true, false, ColType::Int, ColAlign::Right),
    "deletes" => ColSpec::new("deletes", true, false, ColType::Int, ColAlign::Right),
};

pub const ITEMS_QUERY_COLS: ColMap = phf_ordered_map! {
    "item_id" => ColSpec::new("item_id", true, true, ColType::Id, ColAlign::Right),
    "root_id" => ColSpec::new("root_id", true, true, ColType::Id, ColAlign::Right),
    "item_path" => ColSpec::new("item_path", true, true, ColType::Path, ColAlign::Left),
    "item_type" => ColSpec::new("item_type", true, true, ColType::Enum, ColAlign::Center),
    "last_scan" => ColSpec::new("last_scan", true, true, ColType::Id, ColAlign::Right),
    "is_ts" => ColSpec::new("is_ts", true, true, ColType::Bool, ColAlign::Center),
    "mod_date" => ColSpec::new("mod_date", true, true, ColType::Date, ColAlign::Center),
    "file_size" => ColSpec::new("file_size", false, true, ColType::Int, ColAlign::Right),
    "last_hash_scan" => ColSpec::new("last_hash_scan", false, true, ColType::Id, ColAlign::Right),
    "file_hash" => ColSpec::new("file_hash", false, true, ColType::String, ColAlign::Left),
    "last_val_scan" => ColSpec::new("last_val_scan", false, true, ColType::Id, ColAlign::Right),
    "val" => ColSpec::new("val", false, true, ColType::Enum, ColAlign::Center),
    "val_error" => ColSpec::new("val_error", false, true, ColType::String, ColAlign::Left),
};

pub const CHANGES_QUERY_COLS: ColMap = phf_ordered_map! {
    "change_id" => ColSpec::new("changes.change_id", true, true, ColType::Id, ColAlign::Right),
    "root_id" => ColSpec::new("items.root_id", true, true, ColType::Id, ColAlign::Right),
    "scan_id"  => ColSpec::new("changes.scan_id", true, true, ColType::Id, ColAlign::Right),
    "item_id" => ColSpec::new("changes.item_id", true, true, ColType::Id, ColAlign::Right),
    "item_path" => ColSpec::new("items.item_path", false, true, ColType::Path, ColAlign::Left),
    "change_type" => ColSpec::new("change_type", true, true, ColType::Enum, ColAlign::Center),
    "meta_change" => ColSpec::new("meta_change", false, true, ColType::Bool, ColAlign::Center),
    "mod_date_old" => ColSpec::new("mod_date_old", false, true, ColType::Date, ColAlign::Center),
    "mod_date_new" => ColSpec::new("mod_date_new", false, true, ColType::Date, ColAlign::Center),
    "hash_change" => ColSpec::new("hash_change", false, true, ColType::Bool, ColAlign::Center),
    "val_change" => ColSpec::new("val_change", false, true, ColType::Bool, ColAlign::Center),
    "val_old" => ColSpec::new("val_old", false, true, ColType::Enum, ColAlign::Center),
    "val_new" => ColSpec::new("val_new", false, true, ColType::Enum, ColAlign::Center),
    "val_error_old" => ColSpec::new("val_error_old", false, true, ColType::String, ColAlign::Left),
    "val_error_new" => ColSpec::new("val_error_new", false, true, ColType::String, ColAlign::Left),
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

    pub fn values(&self) -> Values<&str, ColSpec> {
        self.col_map.values()
    }

    pub fn entries(&self) -> Entries<&'static str, ColSpec> {
        self.col_map.entries()
    }

    pub fn col_name_to_db(&self, column_name: &str) -> Option<&'static str> {
        match self.col_map.get(column_name) {
            Some(col_spec) => Some(col_spec.name_db),
            None => None,
        }
    }
}
