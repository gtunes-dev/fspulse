use phf_macros::phf_ordered_map;

use super::query::QueryType;

#[derive(Debug, Copy, Clone)]
pub struct StringMap {
    str_map: &'static phf::OrderedMap<&'static str, &'static str>,
}

impl StringMap {
    pub fn new(str_map: &'static phf::OrderedMap<&'static str, &'static str>) -> Self {
        StringMap { str_map }
    }

    /* 
    pub fn contains_key(&self, key: &str) -> bool {
        self.str_map.contains_key(key)
    }
    */

    pub fn get(&self, key: &str) -> Option<&'static str> {
        self.str_map.get(key).copied()
    }

    #[allow(dead_code)]
    pub fn keys_iter(&self) -> impl Iterator<Item = &'static str> {
        self.str_map.entries.iter().map(|&(key, _)| key)
    }

    pub fn entries_iter(&self) -> impl Iterator<Item = (&'static str, &'static str)> {
        self.str_map.entries.iter().copied()
    }
}

pub const ROOTS_QUERY_COLS: phf::OrderedMap<&'static str, &'static str> = phf_ordered_map! {
    "root_id" => "root_id",
    "root_path" => "root_path",
};

pub const SCANS_QUERY_COLS: phf::OrderedMap<&'static str, &'static str> = phf_ordered_map! {
    "scan_id"  => "scan_id",
    "root_id" => "root_id",
    "state" => "state",
    "hashing" => "hashing",
    "validating" => "validating",
    "scan_time" => "scan_time",
    "file_count" => "file_count",
    "folder_count" => "folder_count",
};

pub const ITEMS_QUERY_COLS: phf::OrderedMap<&'static str, &'static str> = phf_ordered_map! {
    "item_id" => "item_id",
    "root_id" => "root_id",
    "item_path" => "item_path",
    "item_type" => "item_type",
    "last_scan" => "last_scan",
    "is_ts" => "is_ts",
    "mod_date" => "mod_date",
    "file_size" => "file_size",
    "last_hash_scan" => "last_hash_scan",
    "last_val_scan" => "last_val_scan",
    "val" => "val",
    "val_error" => "val_error",
};

pub const CHANGES_QUERY_COLS: phf::OrderedMap<&'static str, &'static str> = phf_ordered_map! {
    "change_id" => "changes.change_id",
    "root_id" => "items.root_id",
    "scan_id"  =>"scan_id",
    "item_id" => "changes.item_id",
    "change_type" => "change_type",
    "meta_change" => "meta_change",
    "mod_date_old" => "mod_date_old",
    "mod_date_new" => "mod_date_new",
    "hash_change" => "hash_change",
    "val_change" => "val_change",
    "val_old" => "val_old",
    "val_new" => "val_new",
    "item_path" => "items.item_path",
};

#[derive(Debug, Copy, Clone)]
pub struct ColumnSet {
    str_map: StringMap,
}

impl ColumnSet {
    pub fn for_query_type(query_type: QueryType) -> ColumnSet {
        let query_cols = match query_type {
            QueryType::Roots => &ROOTS_QUERY_COLS,
            QueryType::Items => &ITEMS_QUERY_COLS,
            QueryType::Scans => &SCANS_QUERY_COLS,
            QueryType::Changes => &CHANGES_QUERY_COLS,
        };

        ColumnSet {
            str_map: StringMap::new(query_cols),
        }
    }
    /* 
    pub fn contains_column(&self, column_name: &str) -> bool {
        self.str_map.contains_key(column_name)
    }
    */

    pub fn get_column_db(&self, column_name: &str) -> Option<&'static str> {
        self.str_map.get(column_name)
    }

    pub fn as_select(&self) -> String {
        let mut sql = "SELECT ".to_string();

        let mut first = true;

        for (_, col_name) in self.str_map.entries_iter() {
            match first {
                true => first = false,
                false => sql.push_str(", "),
            }
            sql.push_str(col_name);
        }

        sql
    }

    pub fn display_to_db(&self, display_col: &str) -> Option<&'static str> {
        let display_lower = display_col.to_ascii_lowercase();
        self.str_map.get(&display_lower)
    }
}
