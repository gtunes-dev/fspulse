use crate::{error::FsPulseError, utils::Utils};
use pest::iterators::Pair;
use phf_macros::phf_ordered_map;
use rusqlite::ToSql;
use std::fmt::{self, Debug};

use super::{columns::StringMap, query::QueryType, Rule};

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
    id_type: IdType,
    id_specs: Vec<IdSpec>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IdType {
    Root,
    Item,
    Scan,
    Change,
    LastScan,
    LastHashScan,
    LastValScan,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IdSpec {
    Id(i64),
    IdRange { id_start: i64, id_end: i64 },
}

/*
impl fmt::Display for IdType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            IdType::Root => "roots.root_id",
            IdType::Item => "items.item_id",
            IdType::Scan => "scans.scan_id",
            IdType::Change => "changes.change_id",
        };
        write!(f, "{}", s)
    }
}
    */

impl IdType {
    fn from_rule(rule: Rule) -> Self {
        match rule {
            Rule::root_id_filter => IdType::Root,
            Rule::item_id_filter => IdType::Item,
            Rule::scan_id_filter => IdType::Scan,
            Rule::change_id_filter => IdType::Change,
            Rule::last_scan_filter => IdType::LastScan,
            Rule::last_hash_scan_filter => IdType::LastHashScan,
            Rule::last_val_scan_filter => IdType::LastValScan,
            _ => unreachable!(),
        }
    }
}

impl Filter for IdFilter {
    fn to_predicate_parts(
        &self,
        query_type: QueryType,
    ) -> Result<(String, Vec<Box<dyn ToSql>>), FsPulseError> {
        let mut pred_str = String::new();
        let mut pred_vec: Vec<Box<dyn ToSql>> = Vec::new();
        let mut first = true;

        let id_col = match (query_type, &self.id_type) {
            (QueryType::Roots, IdType::Root) => "roots.root_id",
            (QueryType::Scans, IdType::Scan) => "scans.scan_id",
            (QueryType::Scans, IdType::Root) => "scans.root_id",
            (QueryType::Items, IdType::Item) => "items.item_id",
            (QueryType::Items, IdType::Root) => "items.root_id",
            (QueryType::Items, IdType::LastScan) => "items.last_scan",
            (QueryType::Items, IdType::LastHashScan) => "items.last_hash_scan",
            (QueryType::Items, IdType::LastValScan) => "items.last_val_scan",
            (QueryType::Changes, IdType::Change) => "changes.change_id",
            (QueryType::Changes, IdType::Scan) => "changes.scan_id",
            (QueryType::Changes, IdType::Item) => "changes.item_id",
            (QueryType::Changes, IdType::Root) => "items.root_id",
            _ => unreachable!(),
        };

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
                    pred_str.push_str(&format!("({} = ?)", id_col));
                    pred_vec.push(Box::new(*id));
                }
                IdSpec::IdRange { id_start, id_end } => {
                    pred_str.push_str(&format!("({0} >= ? AND {0} <= ?)", id_col));
                    pred_vec.push(Box::new(*id_start));
                    pred_vec.push(Box::new(*id_end));
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
    fn new(id_type: IdType) -> Self {
        IdFilter {
            id_type,
            id_specs: Vec::new(),
        }
    }

    pub fn build(id_filter_pair: Pair<Rule>) -> Result<Self, FsPulseError> {
        let id_type = IdType::from_rule(id_filter_pair.as_rule());
        let mut id_filter = Self::new(id_type);

        for id_spec in id_filter_pair.into_inner() {
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
                _ => unreachable!(),
            }
        }

        Ok(id_filter)
    }
}

#[derive(Debug, Clone)]
pub struct DateFilter {
    date_type: DateType,
    date_specs: Vec<DateSpec>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DateType {
    ScanTime,
    ModDate,
    ModDateOld,
    ModDateNew,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DateSpec {
    date_start: i64,
    date_end: i64,
}

impl fmt::Display for DateType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            DateType::ScanTime => "scan_time",
            DateType::ModDate => "mod_date",
            DateType::ModDateOld => "mod_date_old",
            DateType::ModDateNew => "mod_date_new",
        };
        write!(f, "{}", s)
    }
}

impl DateType {
    fn from_rule(rule: Rule) -> Self {
        match rule {
            Rule::scan_time_filter => DateType::ScanTime,
            Rule::mod_date_filter => DateType::ModDate,
            Rule::mod_date_old_filter => DateType::ModDateOld,
            Rule::mod_date_new_filter => DateType::ModDateNew,
            _ => unreachable!(),
        }
    }
}

impl Filter for DateFilter {
    fn to_predicate_parts(
        &self,
        query_type: QueryType,
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

            match (&self.date_type, query_type) {
                (DateType::ScanTime, QueryType::Changes) => pred_str.push_str(
                    "(scan_id IN (SELECT scan_id FROM scans WHERE scan_time BETWEEN ? AND ?))",
                ),
                (DateType::ScanTime, QueryType::Scans) => {
                    pred_str.push_str("(scan_time BETWEEN ? AND ?)")
                }
                (DateType::ModDate, QueryType::Items) => {
                    pred_str.push_str("(mod_date BETWEEN ? AND ?)")
                }
                (DateType::ModDateOld, QueryType::Changes) => {
                    pred_str.push_str("(mod_date_old BETWEEN ? AND ?)")
                }
                (DateType::ModDateNew, QueryType::Changes) => {
                    pred_str.push_str("(mod_date_new BETWEEN ? AND ?)")
                }
                _ => unreachable!(),
            }

            pred_vec.push(Box::new(date_spec.date_start));
            pred_vec.push(Box::new(date_spec.date_end));
        }

        if self.date_specs.len() > 1 {
            pred_str.push(')');
        }

        Ok((pred_str, pred_vec))
    }
}

impl DateFilter {
    fn new(date_type: DateType) -> Self {
        DateFilter {
            date_type,
            date_specs: Vec::new(),
        }
    }

    pub fn build(date_filter_pair: Pair<Rule>) -> Result<Self, FsPulseError> {
        let date_type = DateType::from_rule(date_filter_pair.as_rule());
        let mut date_filter = DateFilter::new(date_type);

        for date_spec in date_filter_pair.into_inner() {
            match date_spec.as_rule() {
                Rule::date => {
                    let date_start_str = date_spec.as_str();
                    let (date_start, date_end) = Utils::single_date_bounds(date_start_str)?;
                    date_filter.date_specs.push(DateSpec {
                        date_start,
                        date_end,
                    });
                }
                Rule::date_range => {
                    let mut range_inner = date_spec.into_inner();
                    let date_start_str = range_inner.next().unwrap().as_str();
                    let date_end_str = range_inner.next().unwrap().as_str();
                    let (date_start, date_end) =
                        Utils::range_date_bounds(date_start_str, date_end_str)?;
                    date_filter.date_specs.push(DateSpec {
                        date_start,
                        date_end,
                    });
                }
                _ => unreachable!(),
            }
        }

        Ok(date_filter)
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
    fn from_rule(rule: Rule) -> Self {
        match rule {
            Rule::hashing_filter => Self::Hashing,
            Rule::validating_filter => Self::Validating,
            Rule::change_type_filter => Self::ChangeType,
            Rule::val_filter => Self::Val,
            Rule::meta_change_filter => Self::MetaChange,
            Rule::val_old_filter => Self::ValOld,
            Rule::val_new_filter => Self::ValNew,
            Rule::item_type_filter => Self::ItemType,
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
        let filter_type = StringFilterType::from_rule(string_filter_pair.as_rule());
        let mut string_filter = StringFilter::new(filter_type);

        for str_val_pair in string_filter_pair.into_inner() {
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
    fn from_rule(rule: Rule) -> Self {
        match rule {
            Rule::root_path_filter => PathType::RootPath,
            Rule::item_path_filter => PathType::ItemPath,
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
    fn new(rule: Rule) -> Self {
        PathFilter {
            path_type: PathType::from_rule(rule),
            path_strs: Vec::new(),
        }
    }

    pub fn build(path_filter_pair: Pair<Rule>) -> Result<PathFilter, FsPulseError> {
        let mut path_filter = Self::new(path_filter_pair.as_rule());

        for path_spec in path_filter_pair.into_inner() {
            path_filter.path_strs.push(path_spec.as_str().to_string());
        }

        Ok(path_filter)
    }
}
