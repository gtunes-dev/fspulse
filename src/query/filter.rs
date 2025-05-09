use crate::{error::FsPulseError, utils::Utils};
use pest::iterators::{Pair, Pairs};
use phf_macros::phf_ordered_map;
use rusqlite::ToSql;
use std::fmt::Debug;

type OrderedStrMap = phf::OrderedMap<&'static str, &'static str>;

use super::{process::Query, Rule};

/// Defines the behavior of a filter.
pub trait Filter: Debug {
    /// return predicate text and params
    fn to_predicate_parts(&self) -> Result<(String, Vec<Box<dyn ToSql>>), FsPulseError>;
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
    NotNull,
}

impl Filter for IdFilter {
    fn to_predicate_parts(&self) -> Result<(String, Vec<Box<dyn ToSql>>), FsPulseError> {
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
                IdSpec::Null => pred_str.push_str(&format!("({} IS NULL)", &self.id_col_db)),
                IdSpec::NotNull => pred_str.push_str(&format!("({} IS NOT NULL)", &self.id_col_db)),
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

    pub fn add_to_query(
        id_filter_pair: Pair<Rule>,
        query: &mut dyn Query,
    ) -> Result<(), FsPulseError> {
        let mut iter = id_filter_pair.into_inner();
        let id_col_pair = iter.next().unwrap();
        let id_col = id_col_pair.as_str().to_owned();

        let mut id_filter = match query.col_set().col_name_to_db(&id_col) {
            Some(id_col_db) => Self::new(id_col_db),
            None => {
                return Err(FsPulseError::CustomParsingError(format!(
                    "Column not found: '{}'",
                    id_col
                )))
            }
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
                Rule::null => id_filter.id_specs.push(IdSpec::Null),
                Rule::not_null => id_filter.id_specs.push(IdSpec::NotNull),
                _ => unreachable!(),
            }
        }

        query.add_filter(Box::new(id_filter));

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
    NotNull,
}

impl Filter for DateFilter {
    fn to_predicate_parts(&self) -> Result<(String, Vec<Box<dyn ToSql>>), FsPulseError> {
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
                DateSpec::DateRange {
                    date_start,
                    date_end,
                } => {
                    pred_str.push_str(&format!("({} BETWEEN ? AND ?)", &self.date_col_db));
                    pred_vec.push(Box::new(*date_start));
                    pred_vec.push(Box::new(*date_end));
                }
                DateSpec::Null => pred_str.push_str(&format!("({} IS NULL)", &self.date_col_db)),
                DateSpec::NotNull => {
                    pred_str.push_str(&format!("({} IS NOT NULL)", &self.date_col_db))
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

    pub fn validate_values(pair: &mut Pairs<Rule>) -> Result<(), FsPulseError> {
      let inner_pairs = pair.next().unwrap().into_inner();

       for date_spec in inner_pairs {
            match date_spec.as_rule() {
                Rule::date => {
                    let date_start_str = date_spec.as_str();
                    Utils::single_date_bounds(date_start_str)?;
                }
                Rule::date_range => {
                    let mut range_inner = date_spec.into_inner();
                    let date_start_str = range_inner.next().unwrap().as_str();
                    let date_end_str = range_inner.next().unwrap().as_str();
                    Utils::range_date_bounds(date_start_str, date_end_str)?;
                }
                Rule::null => {}
                Rule::not_null => {}
                Rule::date_filter_EOI => {},
                Rule::EOI => {},
                _ => unreachable!(),
            }
        }

        Ok(())
    }

    pub fn add_to_query(
        date_filter_pair: Pair<Rule>,
        query: &mut dyn Query,
    ) -> Result<(), FsPulseError> {
        let mut iter = date_filter_pair.into_inner();
        let date_col_pair = iter.next().unwrap();
        let date_col = date_col_pair.as_str().to_owned();

        let mut date_filter = match query.col_set().col_name_to_db(&date_col) {
            Some(date_col_db) => Self::new(date_col_db),
            None => {
                return Err(FsPulseError::CustomParsingError(format!(
                    "Column not found: '{}'",
                    date_col
                )))
            }
        };

        for date_spec in iter {
            match date_spec.as_rule() {
                Rule::date => {
                    let date_start_str = date_spec.as_str();
                    let (date_start, date_end) = Utils::single_date_bounds(date_start_str)?;
                    date_filter.date_specs.push(DateSpec::DateRange {
                        date_start,
                        date_end,
                    })
                }
                Rule::date_range => {
                    let mut range_inner = date_spec.into_inner();
                    let date_start_str = range_inner.next().unwrap().as_str();
                    let date_end_str = range_inner.next().unwrap().as_str();
                    let (date_start, date_end) =
                        Utils::range_date_bounds(date_start_str, date_end_str)?;
                    date_filter.date_specs.push(DateSpec::DateRange {
                        date_start,
                        date_end,
                    });
                }
                Rule::null => date_filter.date_specs.push(DateSpec::Null),
                Rule::not_null => date_filter.date_specs.push(DateSpec::NotNull),
                _ => unreachable!(),
            }
        }

        query.add_filter(Box::new(date_filter));

        Ok(())
    }
}

#[derive(Debug)]
pub struct StringFilter {
    str_col_db: &'static str,
    match_null: bool,
    match_not_null: bool,
    str_values: Vec<String>,
}

impl Filter for StringFilter {
    fn to_predicate_parts(&self) -> Result<(String, Vec<Box<dyn ToSql>>), FsPulseError> {
        let mut pred_str = String::new();
        let mut pred_vec: Vec<Box<dyn ToSql>> = Vec::new();
        let mut first: bool = true;

        let mut pred_count = self.str_values.iter().len();
        if self.match_null {
            pred_count += 1
        };
        if self.match_not_null {
            pred_count += 1
        };

        if pred_count > 1 {
            pred_str.push('(');
        }
        if self.match_null {
            first = false;
            pred_str.push_str(&format!("({} IS NULL)", &self.str_col_db));
        }

        if self.match_not_null {
            match first {
                true => first = false,
                false => pred_str.push_str(" OR "),
            }
            pred_str.push_str(&format!("({} IS NOT NULL)", &self.str_col_db));
        }

        for str_val in &self.str_values {
            match first {
                true => first = false,
                false => pred_str.push_str(" OR "),
            }

            pred_str.push_str(&format!("({} LIKE ?)", &self.str_col_db));
            let like_param = format!("%{str_val}%");
            pred_vec.push(Box::new(like_param));
        }

        if pred_count > 1 {
            pred_str.push(')');
        }

        Ok((pred_str, pred_vec))
    }
}

impl StringFilter {
    fn new(str_col_db: &'static str) -> Self {
        StringFilter {
            str_col_db,
            match_null: false,
            match_not_null: false,
            str_values: Vec::new(),
        }
    }

    pub fn add_string_filter_to_query(
        string_filter_pair: Pair<Rule>,
        query: &mut dyn Query,
    ) -> Result<(), FsPulseError> {
        let mut iter = string_filter_pair.into_inner();
        let str_col_pair = iter.next().unwrap();
        let str_col = str_col_pair.as_str().to_owned();

        let mut str_filter = match query.col_set().col_name_to_db(&str_col) {
            Some(str_col_db) => Self::new(str_col_db),
            None => {
                return Err(FsPulseError::CustomParsingError(format!(
                    "Column not found: '{}'",
                    str_col
                )))
            }
        };

        for str_val_pair in iter {
            match str_val_pair.as_rule() {
                Rule::null => str_filter.match_null = true,
                Rule::not_null => str_filter.match_not_null = true,
                _ => {
                    let query_val_str = str_val_pair.as_str();
                    str_filter.str_values.push(query_val_str.to_owned());
                }
            }
        }
        query.add_filter(Box::new(str_filter));

        Ok(())
    }
}

#[derive(Debug)]
pub struct EnumFilter {
    enum_col_db: &'static str,
    match_null: bool,
    match_not_null: bool,
    enum_vals: Vec<String>,
}

impl Filter for EnumFilter {
    fn to_predicate_parts(&self) -> Result<(String, Vec<Box<dyn ToSql>>), FsPulseError> {
        let mut pred_str = String::new();
        let mut pred_vec: Vec<Box<dyn ToSql>> = Vec::new();
        let mut first = true;

        let mut pred_count = self.enum_vals.iter().len();
        if self.match_null {
            pred_count += 1
        };
        if self.match_not_null {
            pred_count += 1
        };

        if pred_count > 1 {
            pred_str.push('(');
        }

        if self.match_null {
            first = false;
            pred_str.push_str(&format!("({} IS NULL)", &self.enum_col_db));
        }

        if self.match_not_null {
            match first {
                true => first = false,
                false => pred_str.push_str(" OR "),
            }
            pred_str.push_str(&format!("({} IS NOT NULL)", &self.enum_col_db));
        }

        for enum_val in &self.enum_vals {
            match first {
                true => first = false,
                false => pred_str.push_str(" OR "),
            }

            pred_str.push_str(&format!("({} = ?)", &self.enum_col_db));
            pred_vec.push(Box::new(enum_val.to_owned()));
        }

        if pred_count > 1 {
            pred_str.push(')');
        }

        Ok((pred_str, pred_vec))
    }
}

impl EnumFilter {
    fn new(enum_col_db: &'static str) -> Self {
        EnumFilter {
            enum_col_db,
            match_null: false,
            match_not_null: false,
            enum_vals: Vec::new(),
        }
    }

    pub fn add_enum_filter_to_query(
        enum_filter_pair: Pair<Rule>,
        query: &mut dyn Query,
    ) -> Result<(), FsPulseError> {
        let mut iter = enum_filter_pair.into_inner();
        let enum_col_pair = iter.next().unwrap();
        let enum_col = enum_col_pair.as_str().to_owned();

        let mut enum_filter = match query.col_set().col_name_to_db(&enum_col) {
            Some(enum_col_db) => Self::new(enum_col_db),
            None => {
                return Err(FsPulseError::CustomParsingError(format!(
                    "Column not found: '{}'",
                    enum_col
                )))
            }
        };

        for enum_val_pair in iter {
            match enum_val_pair.as_rule() {
                Rule::null => enum_filter.match_null = true,
                Rule::not_null => enum_filter.match_not_null = true,
                rule => {
                    let rule_str = format!("{:?}", rule);
                    let val_opt = Self::ENUM_RULES.get(&rule_str).copied();
                    match val_opt {
                        Some(val) => enum_filter.enum_vals.push(val.to_owned()),
                        None => {
                            return Err(FsPulseError::CustomParsingError(format!(
                                "Invalid filter value: '{}'",
                                enum_val_pair.as_str()
                            )));
                        }
                    }
                }
            }
        }

        query.add_filter(Box::new(enum_filter));

        Ok(())
    }

    // We use a single, common map for all of the enum mappings
    // The parser grammar guarantees that values are valid for a given
    // enum type. We stringify rule names because there is no
    // straightforward way to include a rule in a static mapping
    // Null and Not Null are not expressed in these mapping - they
    // are handled directly in code
    const ENUM_RULES: OrderedStrMap = phf_ordered_map! {
        // bool values
        "bool_true" => "1",
        "bool_false" => "0",

        // val values
        "val_valid" => "V",
        "val_invalid" => "I",
        "val_no_validator" => "N",
        "val_unknown" => "U",

        // change type values
        "change_add" => "A",
        "change_modify" => "M",
        "change_delete" => "D",

        // item_type values
        "item_file" => "F",
        "item_directory" => "D",
        "item_symlink" => "S",
    };
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PathFilter {
    path_col_db: &'static str,
    path_strs: Vec<String>,
}

impl Filter for PathFilter {
    fn to_predicate_parts(&self) -> Result<(String, Vec<Box<dyn ToSql>>), FsPulseError> {
        let mut pred_str = " (".to_string();
        let mut pred_vec: Vec<Box<dyn ToSql>> = Vec::new();

        let mut first: bool = true;
        for path_str in &self.path_strs {
            match first {
                true => first = false,
                false => pred_str.push_str(" OR "),
            }

            pred_str.push_str(&format!("({} LIKE ?)", &self.path_col_db));

            let like_str = format!("%{path_str}%");
            pred_vec.push(Box::new(like_str));
        }

        pred_str.push(')');

        Ok((pred_str, pred_vec))
    }
}

impl PathFilter {
    fn new(path_col_db: &'static str) -> Self {
        PathFilter {
            path_col_db,
            path_strs: Vec::new(),
        }
    }

    pub fn add_to_query(
        path_filter_pair: Pair<Rule>,
        query: &mut dyn Query,
    ) -> Result<(), FsPulseError> {
        let mut iter = path_filter_pair.into_inner();
        let path_col = iter.next().unwrap().as_str();

        let mut path_filter = match query.col_set().col_name_to_db(path_col) {
            Some(path_col_db) => Self::new(path_col_db),
            None => {
                return Err(FsPulseError::CustomParsingError(format!(
                    "Column not found: '{}'",
                    path_col
                )))
            }
        };

        for path_spec in iter {
            path_filter.path_strs.push(path_spec.as_str().to_string());
        }

        query.add_filter(Box::new(path_filter));

        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Comparator {
    GreaterThan,
    LessThan,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IntFilter {
    int_col_db: &'static str,
    comparator: Comparator,
    int_value: i64,
}

impl Filter for IntFilter {
    fn to_predicate_parts(&self) -> Result<(String, Vec<Box<dyn ToSql>>), FsPulseError> {
        let mut pred_str = String::new();
        let mut pred_vec: Vec<Box<dyn ToSql>> = Vec::new();

        pred_str.push('(');
        pred_str.push_str(self.int_col_db);

        match self.comparator {
            Comparator::GreaterThan => pred_str.push_str(" > ?)"),
            Comparator::LessThan => pred_str.push_str(" < ?)"),
        }

        pred_vec.push(Box::new(self.int_value));

        Ok((pred_str, pred_vec))
    }
}

impl IntFilter {
    fn new(int_col_db: &'static str, comparator: Comparator, int_value: i64) -> Self {
        IntFilter {
            int_col_db,
            comparator,
            int_value,
        }
    }

    pub fn add_int_filter_to_query(
        int_filter_pair: Pair<Rule>,
        query: &mut dyn Query,
    ) -> Result<(), FsPulseError> {
        let mut iter = int_filter_pair.into_inner();
        let int_col_pair = iter.next().unwrap();

        let int_col = int_col_pair.as_str().to_owned();
        let int_col_db = match query.col_set().col_name_to_db(&int_col) {
            Some(int_col_db) => int_col_db,
            None => {
                return Err(FsPulseError::CustomParsingError(format!(
                    "Column not found: '{}'",
                    int_col
                )))
            }
        };

        let comparator = match iter.next().unwrap().as_rule() {
            Rule::GT => Comparator::GreaterThan,
            Rule::LT => Comparator::LessThan,
            _ => unreachable!(),
        };

        let int_value: i64 = iter.next().unwrap().as_str().parse().unwrap();
        let int_filter = IntFilter::new(int_col_db, comparator, int_value);

        query.add_filter(Box::new(int_filter));

        Ok(())
    }
}
