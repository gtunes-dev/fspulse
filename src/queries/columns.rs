
use super::query::QueryType;


// Define a macro to simplify construction.
macro_rules! column_spec {
    ($(($db:expr, $display:expr)),+ $(,)?) => {{
        &[ $(
            ColumnSpec {
                db_name: $db,
                display_name: $display,
            },
        )+ ]
    }};
}

#[derive(Debug)]
pub struct ColumnSpec {
    db_name: &'static str,
    display_name: &'static str
}

#[derive(Copy, Clone,  Debug)]
pub struct ColumnSet {
    cols: &'static [ColumnSpec],
}

impl ColumnSet {
    pub fn for_query_type(query_type: QueryType) -> ColumnSet {
        
        let cols: &'static [ColumnSpec] = match query_type {
            QueryType::Roots =>  ROOTS_COL_SET,
            QueryType::Items =>  ITEMS_COL_SET,
            QueryType::Scans =>  SCANS_COL_SET,
            QueryType::Changes =>  CHANGES_COL_SET,
        };

        ColumnSet { cols }
    }

    pub fn as_select(&self) -> String {
        let mut sql = "SELECT ".to_string();

        let mut first = true;
        for column in self.cols {
            match first {
                true => first = false,
                false => sql.push_str(", "),
            }
            sql.push_str(column.db_name);
        };

        sql
    }

    pub fn display_to_db(&self, display: &str) -> Option<&'static str>  {
        let display_lower = display.to_lowercase();

        for column in self.cols {
            if column.display_name == display_lower {
                return Some(column.db_name);
            }
        }

        None
    }
}

pub const ROOTS_COL_SET: &[ColumnSpec] = column_spec![
    ("id", "root_id"),
    ("path", "path")
];

pub const SCANS_COL_SET: &[ColumnSpec] = column_spec![
    ("id",  "scan_id"),
    ("root_id", "root_id"),
    ("state", "state"),
    ("hashing", "hashing"),
    ("validating", "validating"),
    ("time_of_scan", "time_of_scan"),
    ("file_count", "file_count"),
    ("folder_count", "folder_count"),
];


pub const ITEMS_COL_SET: &[ColumnSpec] = column_spec![
    ("id", "item_id"),
    ("root_id", "root"),
    ("path", "path"),
    ("item_type", "item_type"),
    ("last_scan_id", "last_scan"),
    ("is_tombstone", "is_ts"),
    ("mod_date", "mod_date"),
    ("file_size", "size"),
    ("last_hash_scan_id", "last_hash_scan"),
    ("last_validation_scan_id", "last_val_scan"),
    ("validity_state", "val"),
    ("validation_error", "val_error"),
];

pub const CHANGES_COL_SET: &[ColumnSpec] = column_spec![
    ("changes.id", "change_id"),
    ("items.root_id", "root_id"),
    ("scan_id", "scan_id"),
    ("item_id", "item_id"),
    ("change_type", "change_type"),
    ("metadata_changed", "meta_change"),
    ("hash_changed", "hash_change"),
    ("validity_changed", "val_change"),
    ("validity_state_old", "val_old"),
    ("validity_state_new", "val_new"),
    ("items.path", "path"),
];

