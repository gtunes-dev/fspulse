use phf::ordered_map::{Entries, Values};
use phf_macros::phf_ordered_map;
use tabled::settings::Alignment;

pub type ColMap = phf::OrderedMap<&'static str, ColSpec>;

#[derive(Debug)]
pub struct ColSpec {
    pub name_db: &'static str,
    pub is_default: bool,
    pub in_select_list: bool,
    pub align: Alignment,
}

impl ColSpec {
    const fn new(
        name_db: &'static str,
        is_default: bool,
        in_select_list: bool,
        align: Alignment,
    ) -> Self {
        ColSpec {
            name_db,
            is_default,
            in_select_list,
            align,
        }
    }
}

pub const ROOTS_QUERY_COLS: ColMap = phf_ordered_map! {
    "root_id" => ColSpec::new("root_id", true, true, Alignment::right()),
    "root_path" => ColSpec::new( "root_path", true, true, Alignment::right()),
};

pub const SCANS_QUERY_COLS: ColMap = phf_ordered_map! {
    "scan_id"  => ColSpec::new("scans.scan_id", true, true, Alignment::right()),
    "root_id" => ColSpec::new("root_id", true, true, Alignment::right()),
    "state" => ColSpec::new("state", false, true, Alignment::center()),
    "is_hash" => ColSpec::new("is_hash", true, true, Alignment::center()),
    "hash_all" => ColSpec::new("hash_all", false, true, Alignment::center()),
    "is_val" => ColSpec::new("is_val", true, true, Alignment::center()),
    "val_all" => ColSpec::new("val_all", false, true, Alignment::center()),
    "scan_time" => ColSpec::new("scan_time", true, true, Alignment::center()),
    "file_count" => ColSpec::new("file_count", true, true, Alignment::right()),
    "folder_count" => ColSpec::new("folder_count", true, true, Alignment::right()),
    "adds" => ColSpec::new("adds", true, false, Alignment::right()),
    "modifies" => ColSpec::new("modifies", true, false, Alignment::right()),
    "deletes" => ColSpec::new("deletes", true, false, Alignment::right()),
};

pub const ITEMS_QUERY_COLS: ColMap = phf_ordered_map! {
    "item_id" => ColSpec::new("item_id", true, true, Alignment::right()),
    "root_id" => ColSpec::new("root_id", true, true, Alignment::right()),
    "item_path" => ColSpec::new("item_path", true, true, Alignment::left()),
    "item_type" => ColSpec::new("item_type", true, true, Alignment::center()),
    "last_scan" => ColSpec::new("last_scan", true, true, Alignment::right()),
    "is_ts" => ColSpec::new("is_ts", true, true, Alignment::center()),
    "mod_date" => ColSpec::new("mod_date", true, true, Alignment::center()),
    "file_size" => ColSpec::new("file_size", false, true, Alignment::right()),
    "last_hash_scan" => ColSpec::new("last_hash_scan", false, true, Alignment::right()),
    "file_hash" => ColSpec::new("file_hash", false, true, Alignment::left()),
    "last_val_scan" => ColSpec::new("last_val_scan", false, true, Alignment::right()),
    "val" => ColSpec::new("val", false, true, Alignment::center()),
    "val_error" => ColSpec::new("val_error", false, true, Alignment::left()),
};

pub const CHANGES_QUERY_COLS: ColMap = phf_ordered_map! {
    "change_id" => ColSpec::new("changes.change_id", true, true, Alignment::right()),
    "root_id" => ColSpec::new("items.root_id", true, true, Alignment::right()),
    "scan_id"  => ColSpec::new("changes.scan_id", true, true, Alignment::right()),
    "item_id" => ColSpec::new("changes.item_id", true, true, Alignment::right()),
    "item_path" => ColSpec::new("items.item_path", false, true, Alignment::left()),
    "change_type" => ColSpec::new("change_type", true, true, Alignment::center()),
    "meta_change" => ColSpec::new("meta_change", false, true, Alignment::center()),
    "mod_date_old" => ColSpec::new("mod_date_old", false, true, Alignment::center()),
    "mod_date_new" => ColSpec::new("mod_date_new", false, true, Alignment::center()),
    "hash_change" => ColSpec::new("hash_change", false, true, Alignment::center()),
    "val_change" => ColSpec::new("val_change", false, true, Alignment::center()),
    "val_old" => ColSpec::new("val_old", false, true, Alignment::center()),
    "val_new" => ColSpec::new("val_new", false, true, Alignment::center()),
    "val_error_old" => ColSpec::new("val_error_old", false, true, Alignment::left()),
    "val_error_new" => ColSpec::new("val_error_new", false, true, Alignment::left()),
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
