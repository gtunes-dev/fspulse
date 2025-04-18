use phf::ordered_map::{Entries, Values};
use phf_macros::phf_ordered_map;
use tabled::settings::Alignment;

pub type ColMap = phf::OrderedMap<&'static str, ColSpec>;

#[derive(Debug)]
pub struct ColSpec {
    pub name_db: &'static str,
    pub is_default: bool,
    pub align: Alignment,
}

impl ColSpec {
    const fn new(name_db: &'static str, is_default: bool, align: Alignment) -> Self {
        ColSpec {
            name_db,
            is_default,
            align,
        }
    }
}

pub const ROOTS_QUERY_COLS: ColMap = phf_ordered_map! {
    "root_id" => ColSpec::new("root_id", true, Alignment::right()),
    "root_path" => ColSpec::new( "root_path", true, Alignment::right()),
};

pub const SCANS_QUERY_COLS: ColMap = phf_ordered_map! {
    "scan_id"  => ColSpec::new("scan_id", true, Alignment::right()),
    "root_id" => ColSpec::new("root_id", true, Alignment::right()),
    "state" => ColSpec::new("state", false, Alignment::center()),
    "hashing" => ColSpec::new("hashing", true, Alignment::center()),
    "validating" => ColSpec::new("validating", true, Alignment::center()),
    "scan_time" => ColSpec::new("scan_time", true, Alignment::center()),
    "file_count" => ColSpec::new("file_count", true, Alignment::right()),
    "folder_count" => ColSpec::new("folder_count", true, Alignment::right()),
};

pub const ITEMS_QUERY_COLS: ColMap = phf_ordered_map! {
    "item_id" => ColSpec::new("item_id", true, Alignment::right()),
    "root_id" => ColSpec::new("root_id", true, Alignment::right()),
    "item_path" => ColSpec::new("item_path", true, Alignment::left()),
    "item_type" => ColSpec::new("item_type", true, Alignment::center()),
    "last_scan" => ColSpec::new("last_scan", true, Alignment::right()),
    "is_ts" => ColSpec::new("is_ts", true, Alignment::center()),
    "mod_date" => ColSpec::new("mod_date", true, Alignment::center()),
    "file_size" => ColSpec::new("file_size", false, Alignment::right()),
    "last_hash_scan" => ColSpec::new("last_hash_scan", false, Alignment::right()),
    "file_hash" => ColSpec::new("file_hash", false, Alignment::left()),
    "last_val_scan" => ColSpec::new("last_val_scan", false, Alignment::right()),
    "val" => ColSpec::new("val", false, Alignment::center()),
    "val_error" => ColSpec::new("val_error", false, Alignment::left()),
};

pub const CHANGES_QUERY_COLS: ColMap = phf_ordered_map! {
    "change_id" => ColSpec::new("changes.change_id", true, Alignment::right()),
    "root_id" => ColSpec::new("items.root_id", true, Alignment::right()),
    "scan_id"  => ColSpec::new("changes.scan_id", true, Alignment::right()),
    "item_id" => ColSpec::new("changes.item_id", true, Alignment::right()),
    "item_path" => ColSpec::new("items.item_path", false, Alignment::left()),
    "change_type" => ColSpec::new("change_type", true, Alignment::center()),
    "meta_change" => ColSpec::new("meta_change", false, Alignment::center()),
    "mod_date_old" => ColSpec::new("mod_date_old", false, Alignment::center()),
    "mod_date_new" => ColSpec::new("mod_date_new", false, Alignment::center()),
    "hash_change" => ColSpec::new("hash_change", false, Alignment::center()),
    "val_change" => ColSpec::new("val_change", false, Alignment::center()),
    "val_old" => ColSpec::new("val_old", false, Alignment::center()),
    "val_new" => ColSpec::new("val_new", false, Alignment::center()),
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
