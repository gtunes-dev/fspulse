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
    display_name: &'static str,
}

#[derive(Copy, Clone, Debug)]
pub struct ColumnSet {
    cols: &'static [ColumnSpec],
}

impl ColumnSet {
    pub fn for_query_type(query_type: QueryType) -> ColumnSet {
        let cols: &'static [ColumnSpec] = match query_type {
            QueryType::Roots => ROOTS_COL_SET,
            QueryType::Items => ITEMS_COL_SET,
            QueryType::Scans => SCANS_COL_SET,
            QueryType::Changes => CHANGES_COL_SET,
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
        }

        sql
    }

    pub fn display_to_db(&self, display: &str) -> Option<&'static str> {
        let display_lower = display.to_lowercase();

        for column in self.cols {
            if column.display_name == display_lower {
                return Some(column.db_name);
            }
        }

        None
    }
}

pub const ROOTS_COL_SET: &[ColumnSpec] = column_spec![("root_id", "root_id"), ("root_path", "root_path")];

pub const SCANS_COL_SET: &[ColumnSpec] = column_spec![
    ("scan_id", "scan_id"),
    ("root_id", "root_id"),
    ("state", "state"),
    ("hashing", "hashing"),
    ("validating", "validating"),
    ("scan_time", "scan_time"),
    ("file_count", "file_count"),
    ("folder_count", "folder_count"),
];

pub const ITEMS_COL_SET: &[ColumnSpec] = column_spec![
    ("item_id", "item_id"),
    ("root_id", "root"),
    ("item_path", "item_path"),
    ("item_type", "item_type"),
    ("last_scan", "last_scan"),
    ("is_ts", "is_ts"),
    ("mod_date", "mod_date"),
    ("file_size", "size"),
    ("last_hash_scan", "last_hash_scan"),
    ("last_val_scan", "last_val_scan"),
    ("val", "val"),
    ("val_error", "val_error"),
];

pub const CHANGES_COL_SET: &[ColumnSpec] = column_spec![
    ("changes.change_id", "change_id"),
    ("items.root_id", "root_id"),
    ("scan_id", "scan_id"),
    ("changes.item_id", "item_id"),
    ("change_type", "change_type"),
    ("meta_change", "meta_change"),
    ("mod_date_old", "mod_date_old"),
    ("mod_date_new", "mod_date_new"),
    ("hash_change", "hash_change"),
    ("val_change", "val_change"),
    ("val_old", "val_old"),
    ("val_new", "val_new"),
    ("items.item_path", "item_path"),
];
