use crate::alerts::{AlertStatus, AlertType};
use crate::validate::validator::ValidationState;
use crate::{changes::ChangeType, error::FsPulseError, items::ItemType, utils::Utils};

use chrono::{DateTime, Local, Utc};
use pest::iterators::Pair;
use tabled::{
    builder::Builder,
    settings::{object::Columns, Alignment},
    Table,
};

use super::{columns::ColSet, Rule};

#[derive(Debug, Copy, Clone)]
pub enum Format {
    None,
    Short,
    Full,
    Relative,
    Name,
}

impl Format {
    fn from_str(format: &str) -> Self {
        let format_upper = format.to_ascii_uppercase();

        match format_upper.as_str() {
            "SHORT" => Self::Short,
            "FULL" => Self::Full,
            "RELATIVE" => Self::Relative,
            "NAME" => Self::Name,
            _ => unreachable!(),
        }
    }

    pub fn format_i64(val: i64) -> String {
        val.to_string()
    }

    pub fn format_opt_i64(val: Option<i64>) -> String {
        match val {
            Some(i) => Format::format_i64(i),
            None => "-".into(),
        }
    }

    pub fn format_date(val: i64, format: Format) -> Result<String, FsPulseError> {
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

    pub fn format_opt_date(val: Option<i64>, format: Format) -> Result<String, FsPulseError> {
        match val {
            Some(val) => Self::format_date(val, format),
            None => Ok("-".into()),
        }
    }

    pub fn format_bool(val: bool, format: Format) -> Result<String, FsPulseError> {
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

    pub fn format_opt_bool(val: Option<bool>, format: Format) -> Result<String, FsPulseError> {
        match val {
            Some(val) => Self::format_bool(val, format),
            None => Ok("-".into()),
        }
    }

    // $TODO format (need the root path)
    pub fn format_path(val: &str, format: Format) -> Result<String, FsPulseError> {
        match format {
            Format::Short => Ok(Utils::display_short_path(val)),
            Format::Full | Format::None => Ok(val.to_owned()),
            Format::Relative => Ok(Utils::display_short_path(val)),
            _ => Err(FsPulseError::Error("Invalid path format".into())),
        }
    }

    pub fn format_item_type(val: &str, format: Format) -> Result<String, FsPulseError> {
        match format {
            Format::Full => Ok(ItemType::short_str_to_full(val)?.to_owned()),
            Format::Short | Format::None => Ok(val.to_owned()),
            _ => Err(FsPulseError::Error("Invalid item_type format".into())),
        }
    }

    pub fn format_change_type(val: &str, format: Format) -> Result<String, FsPulseError> {
        let change_type: ChangeType = val.parse()?;

        match format {
            Format::Full => Ok(change_type.long_name().to_owned()),
            Format::Short | Format::None => Ok(val.to_owned()),
            _ => Err(FsPulseError::Error("Invalid change_type format".into())),
        }
    }

    pub fn format_val(val: &str, format: Format) -> Result<String, FsPulseError> {
        match format {
            Format::Full => Ok(ValidationState::short_str_to_full(val)?.to_owned()),
            Format::Short | Format::None => Ok(val.to_owned()),
            _ => Err(FsPulseError::Error(
                "Invalid validation state format".into(),
            )),
        }
    }

    pub fn format_opt_val(val: Option<&str>, format: Format) -> Result<String, FsPulseError> {
        match val {
            Some(val) => Self::format_val(val, format),
            None => Ok("-".into()),
        }
    }

    pub fn format_alert_type(alert_type: &str, format: Format) -> Result<String, FsPulseError> {
        match format {
            Format::Full => Ok(AlertType::short_str_to_full(alert_type)?.to_owned()),
            Format::Short | Format::None => Ok(alert_type.to_owned()),
            _ => Err(FsPulseError::Error(
                "Invalid alert type state format".into(),
            )),
        }
    }

    pub fn format_alert_status(alert_status: &str, format: Format) -> Result<String, FsPulseError> {
        match format {
            Format::Full => Ok(AlertStatus::short_str_to_full(alert_status)?.to_owned()),
            Format::Short | Format::None => Ok(alert_status.to_owned()),
            _ => Err(FsPulseError::Error(
                "Invalid alert type state format".into(),
            )),
        }
    }

    pub fn format_string(val: &str) -> String {
        val.to_owned()
    }

    pub fn format_opt_string(val: &Option<String>) -> String {
        match val {
            Some(val) => Self::format_string(val),
            None => "-".into(),
        }
    }
}

#[derive(Debug)]
pub struct DisplayCol {
    pub display_col: &'static str,
    pub alignment: Alignment,
    pub format: Format,
}

#[derive(Debug)]
pub struct Show {
    col_set: ColSet,
    pub display_cols: Vec<DisplayCol>,
}

impl Show {
    pub fn new(col_set: ColSet) -> Self {
        Show {
            col_set,
            display_cols: Vec::new(),
        }
    }

    pub fn ensure_columns(&mut self) {
        // If no display columns were specified, add the default column set
        if self.display_cols.is_empty() {
            self.add_default_columns();
        }
    }

    pub fn prepare_builder(&mut self, builder: &mut Builder) {
        self.ensure_columns();
        let header: Vec<&'static str> = self.display_cols.iter().map(|dc| dc.display_col).collect();
        builder.push_record(header);
    }

    pub fn set_column_aligments(&self, table: &mut Table) {
        for (col_index, col) in self.display_cols.iter().enumerate() {
            table.modify(Columns::single(col_index), col.alignment);
        }
    }
    pub fn add_all_columns(&mut self) {
        for (col, col_spec) in self.col_set.entries() {
            self.display_cols.push(DisplayCol {
                display_col: col,
                alignment: col_spec.col_align.to_tabled(),
                format: Format::None,
            });
        }
    }

    pub fn add_default_columns(&mut self) {
        for (col, col_spec) in self.col_set.entries() {
            if col_spec.is_default {
                self.display_cols.push(DisplayCol {
                    display_col: col,
                    alignment: col_spec.col_align.to_tabled(),
                    format: Format::None,
                });
            }
        }
    }

    pub fn build_from_pest_pair(&mut self, show_list: Pair<Rule>) -> Result<(), FsPulseError> {
        for element in show_list.into_inner() {
            match element.as_rule() {
                Rule::all => self.add_all_columns(),
                Rule::default => self.add_default_columns(),
                Rule::int_show
                | Rule::id_show
                | Rule::date_show
                | Rule::bool_show
                | Rule::string_show
                | Rule::path_show
                | Rule::val_show
                | Rule::item_type_show
                | Rule::change_type_show
                | Rule::alert_type_show
                | Rule::alert_status_show => {
                    let mut path_show_parts = element.into_inner();
                    let display_col = path_show_parts.next().unwrap().as_str();

                    match self.col_set.col_set().get_entry(display_col) {
                        Some((key, display_col)) => {
                            let format = match path_show_parts.next() {
                                Some(format_pair) => Format::from_str(format_pair.as_str()),
                                None => Format::None,
                            };
                            self.display_cols.push(DisplayCol {
                                display_col: key,
                                alignment: display_col.col_align.to_tabled(),
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
