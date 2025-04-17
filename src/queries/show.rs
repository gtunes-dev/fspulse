use crate::{
    changes::ChangeType, error::FsPulseError, utils::Utils, validators::validator::ValidationState,
};
use chrono::{DateTime, Local, Utc};
use pest::iterators::Pair;
use rusqlite::Row;
use tabled::{
    builder::Builder,
    settings::{object::Columns, Alignment},
    Table, Tabled,
};

use super::{columns::ColSet, query::ChangesQueryRow, Rule};

#[derive(Tabled)]
pub struct ScansQueryRow {
    scan_id: i64,
    root_id: i64,
    state: i64,
    #[tabled(display = "Utils::display_bool")]
    hashing: bool,
    #[tabled(display = "Utils::display_bool")]
    validating: bool,
    #[tabled(display = "Utils::display_db_time")]
    scan_time: i64,
    file_count: i64,
    folder_count: i64,
}

impl ScansQueryRow {
    pub fn from_row(row: &Row) -> rusqlite::Result<Self> {
        Ok(ScansQueryRow {
            scan_id: row.get(0)?,
            root_id: row.get(1)?,
            state: row.get(2)?,
            hashing: row.get(3)?,
            validating: row.get(4)?,
            scan_time: row.get(5)?,
            file_count: row.get(6)?,
            folder_count: row.get(7)?,
        })
    }
}

#[derive(Debug, Copy, Clone)]
pub enum Format {
    None,
    NoDisplay,
    Short,
    Full,
    Relative,
    Name,
}

impl Format {
    fn from_str(format: &str) -> Self {
        let format_upper = format.to_ascii_uppercase();

        match format_upper.as_str() {
            "NODISPLAY" => Self::NoDisplay,
            "SHORT" => Self::Short,
            "FULL" => Self::Full,
            "RELATIVE" => Self::Relative,
            "NAME" => Self::Name,
            _ => unreachable!(),
        }
    }

    fn format_i64(val: i64) -> String {
        val.to_string()
    }

    fn _format_opt_i64(val: Option<i64>) -> String {
        match val {
            Some(i) => Format::format_i64(i),
            None => "-".into(),
        }
    }

    fn format_date(val: i64, format: Format) -> Result<String, FsPulseError> {
        let datetime_utc = DateTime::<Utc>::from_timestamp(val, 0)
            .unwrap_or_else(|| DateTime::<Utc>::from_timestamp(0, 0).unwrap());

        let datetime_local: DateTime<Local> = datetime_utc.with_timezone(&Local);

        let format_str = match format {
            Format::Short | Format::None => "%Y-%m-%d",
            Format::Full => "%Y-%m-%d %H:%M:%S",
            _ => {
                return Err(FsPulseError::Error("Invalid date format".into()));
            }
        };

        Ok(datetime_local.format(format_str).to_string())
    }

    fn format_opt_date(val: Option<i64>, format: Format) -> Result<String, FsPulseError> {
        match val {
            Some(val) => Self::format_date(val, format),
            None => Ok("-".into()),
        }
    }

    fn format_bool(val: bool, format: Format) -> Result<String, FsPulseError> {
        let s = match (val, format) {
            (true, Format::Short | Format::None) => "T",
            (true, Format::Full) => "True",
            (false, Format::Short | Format::None) => "F",
            (false, Format::Full) => "False",
            _ => {
                return Err(FsPulseError::Error("Invalid bool format".into()));
            }
        };

        Ok(s.into())
    }

    fn format_opt_bool(val: Option<bool>, format: Format) -> Result<String, FsPulseError> {
        match val {
            Some(val) => Self::format_bool(val, format),
            None => Ok("-".into()),
        }
    }

    // $TODO format (need the root path)
    fn _format_path(path_val: &str, format: Format) -> String {
        match format {
            Format::Short => Utils::display_short_path(path_val),
            _ => path_val.to_owned(),
        }
    }

    fn format_change_type(val: &str, format: Format) -> Result<String, FsPulseError> {
        match format {
            Format::Full => Ok(ChangeType::short_str_to_full(val)?.to_owned()),
            Format::Short | Format::None => Ok(val.to_owned()),
            _ => Err(FsPulseError::Error("Invalid change_type format".into())),
        }
    }

    fn format_val(val: &str, format: Format) -> Result<String, FsPulseError> {
        match format {
            Format::Full => Ok(ValidationState::short_str_to_full(val)?.to_owned()),
            Format::Short | Format::None => Ok(val.to_owned()),
            _ => Err(FsPulseError::Error(
                "Invalid validation state format".into(),
            )),
        }
    }

    fn format_opt_val(val: Option<&str>, format: Format) -> Result<String, FsPulseError> {
        match val {
            Some(val) => Self::format_val(val, format),
            None => Ok("-".into()),
        }
    }
}

#[derive(Debug)]
struct DisplayCol {
    display_col: &'static str,
    alignment: Alignment,
    format: Format,
}

#[derive(Debug)]
pub struct Show {
    display_cols: Vec<DisplayCol>,
}

impl Show {
    pub fn new() -> Self {
        Show {
            display_cols: Vec::new(),
        }
    }

    pub fn as_builder(&self) -> Builder {
        let mut builder = Builder::default();

        let header: Vec<&'static str> = self.display_cols.iter().map(|dc| dc.display_col).collect();
        builder.push_record(header);
        builder
    }

    pub fn append_changes_row(
        &self,
        change: &ChangesQueryRow,
        builder: &mut Builder,
    ) -> Result<(), FsPulseError> {
        let mut row: Vec<String> = Vec::new();

        for col in &self.display_cols {
            let col_string = match col.display_col {
                "change_id" => Format::format_i64(change.change_id),
                "root_id" => Format::format_i64(change.root_id),
                "scan_id" => Format::format_i64(change.scan_id),
                "item_id" => Format::format_i64(change.item_id),
                "change_type" => Format::format_change_type(&change.change_type, col.format)?,
                "meta_change" => Format::format_opt_bool(change.meta_change, col.format)?,
                "mod_date_old" => Format::format_opt_date(change.mod_date_old, col.format)?,
                "mod_date_new" => Format::format_opt_date(change.mod_date_new, col.format)?,
                "hash_change" => Format::format_opt_bool(change.hash_change, col.format)?,
                "val_change" => Format::format_opt_bool(change.val_change, col.format)?,
                "val_old" => Format::format_opt_val(change.val_old.as_deref(), col.format)?,
                "val_new" => Format::format_opt_val(change.val_new.as_deref(), col.format)?,
                _ => {
                    return Err(FsPulseError::Error("Invalid column".into()));
                }
            };

            row.push(col_string);
        }

        builder.push_record(row);

        Ok(())
    }

    pub fn set_column_aligments(&self, table: &mut Table) {
        for (col_index, col) in self.display_cols.iter().enumerate() {
            table.modify(Columns::single(col_index), col.alignment);
        }
    }

    pub fn build_from_pest_pair(
        &mut self,
        show_list: Pair<Rule>,
        col_set: ColSet,
    ) -> Result<(), FsPulseError> {
        for element in show_list.into_inner() {
            match element.as_rule() {
                Rule::id_show
                | Rule::date_show
                | Rule::bool_show
                | Rule::path_show
                | Rule::val_show
                | Rule::item_type_show
                | Rule::change_type_show => {
                    let mut path_show_parts = element.into_inner();
                    let display_col = path_show_parts.next().unwrap().as_str();

                    match col_set.col_set().get_entry(display_col) {
                        Some((key, display_col)) => {
                            let format = match path_show_parts.next() {
                                Some(format_pair) => Format::from_str(format_pair.as_str()),
                                None => Format::None,
                            };
                            self.display_cols.push(DisplayCol {
                                display_col: key,
                                alignment: display_col.align,
                                format,
                            })
                        }
                        None => {
                            return Err(FsPulseError::CustomParsingError(format!(
                                "Invalid column in show clause: '{}'",
                                display_col
                            )));
                        }
                    }
                }
                _ => {}
            }
        }

        Ok(())
    }
}
