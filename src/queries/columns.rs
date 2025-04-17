use phf_macros::phf_ordered_map;
use tabled::settings::Alignment;

pub type ColMap = phf::OrderedMap<&'static str, ColSpec>;

#[derive(Debug)]
pub struct ColSpec {
    pub name_db: &'static str,
    pub align: Alignment,
}

impl ColSpec {
    const fn new(name_db: &'static str, align: Alignment) -> Self {
        ColSpec { name_db, align }
    }
}

pub const ROOTS_QUERY_COLS: ColMap = phf_ordered_map! {
    "root_id" => ColSpec::new("root_id", Alignment::right()),
    "root_path" => ColSpec::new( "root_path", Alignment::right()),
};

pub const SCANS_QUERY_COLS: ColMap = phf_ordered_map! {
    "scan_id"  => ColSpec::new("scan_id",Alignment::right()),
    "root_id" => ColSpec::new("root_id",Alignment::right()),
    "state" => ColSpec::new("state", Alignment::center()),
    "hashing" => ColSpec::new("hashing", Alignment::center()),
    "validating" => ColSpec::new("validating", Alignment::center()),
    "scan_time" => ColSpec::new("scan_time", Alignment::center()),
    "file_count" => ColSpec::new("file_count", Alignment::right()),
    "folder_count" => ColSpec::new("folder_count", Alignment::right()),
};

pub const ITEMS_QUERY_COLS: ColMap = phf_ordered_map! {
    "item_id" => ColSpec::new("item_id", Alignment::right()),
    "root_id" => ColSpec::new("root_id", Alignment::right()),
    "item_path" => ColSpec::new("item_path", Alignment::left()),
    "item_type" => ColSpec::new("item_type", Alignment::center()),
    "last_scan" => ColSpec::new("last_scan", Alignment::right()),
    "is_ts" => ColSpec::new("is_ts", Alignment::center()),
    "mod_date" => ColSpec::new("mod_date", Alignment::center()),
    "file_size" => ColSpec::new("file_size",Alignment::right()),
    "last_hash_scan" => ColSpec::new("last_hash_scan", Alignment::right()),
    "last_val_scan" => ColSpec::new("last_val_scan", Alignment::right()),
    "val" => ColSpec::new("val", Alignment::center()),
    "val_error" => ColSpec::new("val_error", Alignment::left()),
};

pub const CHANGES_QUERY_COLS: ColMap = phf_ordered_map! {
    "change_id" => ColSpec::new("changes.change_id", Alignment::right()),
    "root_id" => ColSpec::new("items.root_id", Alignment::right()),
    "scan_id"  => ColSpec::new("changes.scan_id", Alignment::right()),
    "item_id" => ColSpec::new("changes.item_id", Alignment::right()),
    "change_type" => ColSpec::new("change_type", Alignment::center()),
    "meta_change" => ColSpec::new("meta_change", Alignment::center()),
    "mod_date_old" => ColSpec::new("mod_date_old", Alignment::center()),
    "mod_date_new" => ColSpec::new("mod_date_new", Alignment::center()),
    "hash_change" => ColSpec::new("hash_change", Alignment::center()),
    "val_change" => ColSpec::new("val_change", Alignment::center()),
    "val_old" => ColSpec::new("val_old", Alignment::center()),
    "val_new" => ColSpec::new("val_new", Alignment::center()),
    "item_path" => ColSpec::new("items.item_path", Alignment::left()),
};

#[derive(Debug, Copy, Clone)]
pub struct ColSet {
    col_map: &'static ColMap,
}

impl ColSet {
    /*
    pub fn for_query_type(query_type: QueryType) -> Self {
        let col_map = match query_type {
            QueryType::Roots => &ROOTS_QUERY_COLS,
            QueryType::Items => &ITEMS_QUERY_COLS,
            QueryType::Scans => &SCANS_QUERY_COLS,
            QueryType::Changes => &CHANGES_QUERY_COLS,
        };

        ColSet { col_map }
    }
    */

    pub fn new(col_map: &'static ColMap) -> Self {
        ColSet { col_map }
    }

    pub fn col_set(&self) -> &ColMap {
        self.col_map
    }

    pub fn get_name_db(&self, column_name: &str) -> Option<&'static str> {
        match self.col_map.get(column_name) {
            Some(col_spec) => Some(col_spec.name_db),
            None => None,
        }
    }

    pub fn as_select(&self) -> String {
        let mut sql = "SELECT ".to_string();

        let mut first = true;

        for col_spec in self.col_map.values() {
            match first {
                true => first = false,
                false => sql.push_str(", "),
            }
            sql.push_str(col_spec.name_db);
        }

        sql
    }

    /*
    pub fn get_column_db(&self, column_name: &str) -> Option<&'static str> {
        self.str_map.get(column_name)
    }

    pub fn get_column(&self, column_name: &str) -> Option<&'static str> {
        self.str_map.get_key(column_name)
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
    */
}
