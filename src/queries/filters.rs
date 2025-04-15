use crate::{error::FsPulseError, utils::Utils};
use pest::iterators::Pair;
use phf_macros::phf_ordered_map;
use rusqlite::ToSql;
use std::fmt::Debug;

use super::{columns::StringMap, query::{DomainQuery, QueryType}, Rule};

/// Defines the behavior of a filter.
pub trait Filter: Debug {
    /// return predicate text and params
    fn to_predicate_parts(
        &self,
        query_type: QueryType,
    ) -> Result<(String, Vec<Box<dyn ToSql>>), FsPulseError>;
}

#[derive(Debug, Clone)]
pub struct IdFilter {
    id_col_db: &'static str,
    id_specs: Vec<IdSpec>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IdSpec {
    Id(i64),
    IdRange { id_start: i64, id_end: i64 },
    Null,
}

impl Filter for IdFilter {
    fn to_predicate_parts(
        &self,
        _query_type: QueryType,
    ) -> Result<(String, Vec<Box<dyn ToSql>>), FsPulseError> {
        let mut pred_str = String::new();
        let mut pred_vec: Vec<Box<dyn ToSql>> = Vec::new();
        let mut first = true;

        if self.id_specs.len() > 1 {
            pred_str.push('(');
        }
        
        for id_spec in &self.id_specs {
            match first {
                true => first = false,
                false => pred_str.push_str(" OR "),
            }

            match id_spec {
                IdSpec::Id(id) => {
                    pred_str.push_str(&format!("({} = ?)", &self.id_col_db));
                    pred_vec.push(Box::new(*id));
                }
                IdSpec::IdRange { id_start, id_end } => {
                    pred_str.push_str(&format!("({0} >= ? AND {0} <= ?)", &self.id_col_db));
                    pred_vec.push(Box::new(*id_start));
                    pred_vec.push(Box::new(*id_end));
                }
                IdSpec::Null => {
                    pred_str.push_str(&format!("({} IS NULL)", &self.id_col_db))
                }
            }
        }

        if self.id_specs.len() > 1 {
            pred_str.push(')');
        }

        Ok((pred_str, pred_vec))
    }
}

impl IdFilter {
    fn new(id_col_db: &'static str) -> Self {
        IdFilter {
            id_col_db,
            id_specs: Vec::new(),
        }
    }

    pub fn add_to_query(id_filter_pair: Pair<Rule>, query: &mut DomainQuery) -> Result<(), FsPulseError> {
        let mut iter = id_filter_pair.into_inner();
        let id_col_pair = iter.next().unwrap();
        let id_col = id_col_pair.as_str().to_owned();

        let mut id_filter = match query.get_column_db(&id_col) {
            Some(id_col_db)=> Self::new(id_col_db),
            None => return Err(FsPulseError::CustomParsingError(format!("Column not found: '{}'", id_col)))
        };

        for id_spec in iter {
            match id_spec.as_rule() {
                Rule::id => {
                    let id: i64 = id_spec.as_str().parse().unwrap();
                    id_filter.id_specs.push(IdSpec::Id(id))
                }
                Rule::id_range => {
                    let mut range_inner = id_spec.into_inner();

                    let id_start: i64 = range_inner.next().unwrap().as_str().parse().unwrap();
                    let id_end: i64 = range_inner.next().unwrap().as_str().parse().unwrap();
                    id_filter
                        .id_specs
                        .push(IdSpec::IdRange { id_start, id_end })
                }
                Rule::null => {
                    id_filter.id_specs.push(IdSpec::Null)
                }
                _ => unreachable!(),
            }
        }

        query.add_filter(id_filter);

        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct DateFilter {
    date_col_db: &'static str,
    date_specs: Vec<DateSpec>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DateSpec {
    DateRange { date_start: i64, date_end: i64 },
    Null,
}

impl Filter for DateFilter {
    fn to_predicate_parts(
        &self,
        _query_type: QueryType,
    ) -> Result<(String, Vec<Box<dyn ToSql>>), FsPulseError> {
        let mut pred_str = String::new();
        let mut pred_vec: Vec<Box<dyn ToSql>> = Vec::new();
        let mut first = true;

        if self.date_specs.len() > 1 {
            pred_str.push('(');
        }

        for date_spec in &self.date_specs {
            match first {
                true => first = false,
                false => pred_str.push_str(" OR "),
            }

            // $TODO: We used to sort of support filtering Changes on "scan_time" with:
            //      (scan_id IN (SELECT scan_id FROM scans WHERE scan_time BETWEEN ? AND ?))
            // At present, filtering is limited to actual date columns. Need to revisit the
            // idea of join columns

            match date_spec {
                DateSpec::DateRange { date_start, date_end } =>  {
                    pred_str.push_str(&format!("({} BETWEEN ? AND ?)", &self.date_col_db));
                    pred_vec.push(Box::new(*date_start));
                    pred_vec.push(Box::new(*date_end));
                }
                DateSpec::Null => {
                    pred_str.push_str(&format!("({} IS NULL)", &self.date_col_db))
                }
            }  
        }

        if self.date_specs.len() > 1 {
            pred_str.push(')');
        }

        Ok((pred_str, pred_vec))
    }
}

impl DateFilter {
    fn new(date_col_db: &'static str) -> Self {
        DateFilter {
            date_col_db,
            date_specs: Vec::new(),
        }
    }

    pub fn add_to_query(date_filter_pair: Pair<Rule>, query: &mut DomainQuery) -> Result<(), FsPulseError> {
        let mut iter = date_filter_pair.into_inner();
        let date_col_pair = iter.next().unwrap();
        let date_col = date_col_pair.as_str().to_owned();

        let mut date_filter = match query.get_column_db(&date_col) {
            Some(date_col_db) => Self::new(date_col_db),
            None => return Err(FsPulseError::CustomParsingError(format!("Column not found: '{}'", date_col)))
        };

        for date_spec in iter {
            match date_spec.as_rule() {
                Rule::date => {
                    let date_start_str = date_spec.as_str();
                    let (date_start, date_end) = Utils::single_date_bounds(date_start_str)?;
                    date_filter.date_specs.push(DateSpec::DateRange { date_start, date_end })
                }
                Rule::date_range => {
                    let mut range_inner = date_spec.into_inner();
                    let date_start_str = range_inner.next().unwrap().as_str();
                    let date_end_str = range_inner.next().unwrap().as_str();
                    let (date_start, date_end) =
                        Utils::range_date_bounds(date_start_str, date_end_str)?;
                    date_filter.date_specs.push(DateSpec::DateRange { date_start, date_end });
                }
                Rule::null => {
                    date_filter.date_specs.push(DateSpec::Null);
                }
                _ => unreachable!(),
            }
        }

        query.add_filter(date_filter);

        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum StringFilterType {
    Hashing,
    Validating,
    ChangeType,
    Val,
    MetaChange,
    ValOld,
    ValNew,
    ItemType,
    //HashChange,
    //ValChange
}

impl StringFilterType {
    fn from_column(column: &str) -> Self {
        match column {
            "hashing" => Self::Hashing,
            "validating" => Self::Validating,
            "change_type" => Self::ChangeType,
            "val" => Self::Val,
            "meta_change" => Self::MetaChange,
            "val_old" => Self::ValOld,
            "val_new" => Self::ValNew,
            "item_type" => Self::ItemType,
            _ => unreachable!(),
        }
    }
}

#[derive(Debug)]
pub struct StringFilter {
    filter_type: StringFilterType,
    str_map: StringMap,

    str_values: Vec<String>,
}

impl Filter for StringFilter {
    fn to_predicate_parts(
        &self,
        _query_type: QueryType,
    ) -> Result<(String, Vec<Box<dyn ToSql>>), FsPulseError> {
        let mut pred_str = String::new();
        let mut pred_vec: Vec<Box<dyn ToSql>> = Vec::new();
        let mut first: bool = true;

        let col_name = match self.filter_type {
            StringFilterType::Hashing => "scans.hashing",
            StringFilterType::Validating => "scans.validating",
            StringFilterType::ChangeType => "changes.change_type",
            StringFilterType::Val => "items.val",
            StringFilterType::MetaChange => "changes.meta_change",
            StringFilterType::ValOld => "changes.val_old",
            StringFilterType::ValNew => "changes.val_new",
            StringFilterType::ItemType => "items.item_type",
        };

        if self.str_values.iter().len() > 1 {
            pred_str.push('(');
        }

        for str_val in &self.str_values {
            match first {
                true => first = false,
                false => pred_str.push_str(" OR "),
            }

            if str_val == "NULL" {
                pred_str.push_str(&format!("({} IS NULL)", col_name));
            } else {
                pred_str.push_str(&format!("({} = ?)", col_name));
                pred_vec.push(Box::new(str_val.to_owned()));
            }
        }

        if self.str_values.iter().len() > 1 {
            pred_str.push(')');
        }

        Ok((pred_str, pred_vec))
    }
}

impl StringFilter {
    fn new(filter_type: StringFilterType) -> Self {
        let str_map = match filter_type {
            StringFilterType::Hashing => &Self::BOOL_VALUES,
            StringFilterType::Validating => &Self::BOOL_VALUES,
            StringFilterType::ChangeType => &Self::CHANGE_TYPE_VALUES,
            StringFilterType::Val => &Self::VAL_VALUES,
            StringFilterType::MetaChange => &Self::BOOL_NULLABLE_VALUES,
            StringFilterType::ValOld => &Self::VAL_NULLABLE_VALUES,
            StringFilterType::ValNew => &Self::VAL_NULLABLE_VALUES,
            StringFilterType::ItemType => &Self::ITEM_TYPE_VALUES,
        };

        StringFilter {
            filter_type,
            str_map: StringMap::new(str_map),
            str_values: Vec::new(),
        }
    }

    pub fn build(string_filter_pair: Pair<Rule>) -> Result<Self, FsPulseError> {
        let mut iter = string_filter_pair.into_inner();
        let string_col = iter.next().unwrap().as_str();
        let string_type = StringFilterType::from_column(string_col);
        let mut string_filter = Self::new(string_type);

        for str_val_pair in iter {
            let val_str = str_val_pair.as_str();
            let val_str_upper = val_str.to_ascii_uppercase();

            let mapped_str = string_filter.str_map.get(&val_str_upper);
            match mapped_str {
                Some(s) => string_filter.str_values.push(s.to_owned()),
                None => {
                    return Err(FsPulseError::CustomParsingError(format!(
                        "Invalid filter value: '{}'",
                        val_str
                    )));
                }
            }
        }
        Ok(string_filter)
    }

    pub const BOOL_VALUES: phf::OrderedMap<&'static str, &'static str> = phf_ordered_map! {
        "TRUE" => "1",
        "T" => "1",
        "FALSE" => "0",
        "F" => "0",
    };

    const BOOL_NULLABLE_VALUES: phf::OrderedMap<&'static str, &'static str> = phf_ordered_map! {
        "TRUE" => "1",
        "T" => "1",
        "FALSE" => "0",
        "F" => "0",
        "NULL" => "NULL",
        "-" => "NULL",
    };

    const CHANGE_TYPE_VALUES: phf::OrderedMap<&'static str, &'static str> = phf_ordered_map! {
        "ADD" => "A",
        "A" => "A",
        "DELETE" => "D",
        "D" => "D",
        "MODIFY" => "M",
        "M" => "M",
    };

    const VAL_VALUES: phf::OrderedMap<&'static str, &'static str> = phf_ordered_map! {
        "VALID" => "V",
        "V" => "V",
        "INVALID" => "I",
        "I" => "I",
        "NO_VALIDATOR" => "N",
        "N" => "N",
        "UNKNOWN" => "U",
        "U" => "U",
    };

    const VAL_NULLABLE_VALUES: phf::OrderedMap<&'static str, &'static str> = phf_ordered_map! {
        "VALID" => "V",
        "V" => "V",
        "INVALID" => "I",
        "I" => "I",
        "NOT_VALIDATED" => "N",
        "N" => "N",
        "UNKNOWN" => "U",
        "U" => "U",
        "NULL" => "NULL",
        "-" => "NULL",
    };

    const ITEM_TYPE_VALUES: phf::OrderedMap<&'static str, &'static str> = phf_ordered_map! {
        "FILE" => "F",
        "F" => "F",
        "DIRECTORY" => "D",
        "D" => "D",
    };
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PathFilter {
    path_type: PathType,
    path_strs: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PathType {
    RootPath,
    ItemPath,
}

impl PathType {
    fn from_column(column: &str) -> Self {
        match column {
            "root_path" => PathType::RootPath,
            "item_path" => PathType::ItemPath,
            _ => unreachable!(),
        }
    }
}

impl Filter for PathFilter {
    fn to_predicate_parts(
        &self,
        query_type: QueryType,
    ) -> Result<(String, Vec<Box<dyn ToSql>>), FsPulseError> {
        let mut pred_str = " (".to_string();
        let mut pred_vec: Vec<Box<dyn ToSql>> = Vec::new();

        let mut first: bool = true;
        for path_str in &self.path_strs {
            match first {
                true => first = false,
                false => pred_str.push_str(" OR "),
            }

            let path_col = match (query_type, &self.path_type) {
                (QueryType::Roots, PathType::RootPath) => "roots.root_path",
                (QueryType::Items, PathType::ItemPath) => "items.item_path",
                (QueryType::Changes, PathType::ItemPath) => "items.item_path",
                _ => unreachable!(),
            };

            pred_str.push_str(&format!("({path_col} LIKE ?)"));

            let like_str = format!("%{path_str}%");
            pred_vec.push(Box::new(like_str));
        }

        pred_str.push(')');

        Ok((pred_str, pred_vec))
    }
}

impl PathFilter {
    fn new(path_type: PathType) -> Self {
        PathFilter {
            path_type,
            path_strs: Vec::new(),
        }
    }

    pub fn build(path_filter_pair: Pair<Rule>) -> Result<PathFilter, FsPulseError> {
        let mut iter = path_filter_pair.into_inner();
        let path_col = iter.next().unwrap().as_str();
        let path_type = PathType::from_column(path_col);
        let mut path_filter = PathFilter::new(path_type);

        for path_spec in iter {
            path_filter.path_strs.push(path_spec.as_str().to_string());
        }

        Ok(path_filter)
    }
}
