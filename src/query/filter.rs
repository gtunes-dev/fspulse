use crate::{
    alerts::{AlertStatus, AlertType},
    error::FsPulseError,
    items::{Access, ItemType},
    scans::ScanState,
    utils::Utils,
    validate::validator::ValidationState,
};
use pest::iterators::{Pair, Pairs};
use phf::Map;
use phf_macros::{phf_map, phf_ordered_map};
use rusqlite::ToSql;
use std::fmt::Debug;

type OrderedStrMap = phf::OrderedMap<&'static str, &'static str>;

use super::{process::Query, Rule};

/// Defines the behavior of a filter.
pub trait Filter: Debug {
    /// return predicate text and params
    fn to_predicate_parts(&self) -> Result<(String, Vec<Box<dyn ToSql>>), FsPulseError>;
}

/// Trait for enums that can be used in query filters with integer backing
pub trait QueryEnum {
    /// Parse a query token (short or full name) and return the database i64 value
    fn from_token(s: &str) -> Option<i64>;
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
                    "Column not found: '{id_col}'"
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

            // $TODO: We used to sort of support filtering Changes on "started_at" with:
            //      (scan_id IN (SELECT scan_id FROM scans WHERE started_at BETWEEN ? AND ?))
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
                Rule::date_filter_EOI => {}
                Rule::EOI => {}
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
                    "Column not found: '{date_col}'"
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
                    "Column not found: '{str_col}'"
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
/// Filter for boolean columns
pub struct BoolFilter {
    bool_col_db: &'static str,
    match_null: bool,
    match_not_null: bool,
    bool_vals: Vec<String>,
}

impl Filter for BoolFilter {
    fn to_predicate_parts(&self) -> Result<(String, Vec<Box<dyn ToSql>>), FsPulseError> {
        let mut pred_str = String::new();
        let mut pred_vec: Vec<Box<dyn ToSql>> = Vec::new();
        let mut first = true;

        let mut pred_count = self.bool_vals.iter().len();
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
            pred_str.push_str(&format!("({} IS NULL)", &self.bool_col_db));
        }

        if self.match_not_null {
            match first {
                true => first = false,
                false => pred_str.push_str(" OR "),
            }
            pred_str.push_str(&format!("({} IS NOT NULL)", &self.bool_col_db));
        }

        for bool_val in &self.bool_vals {
            match first {
                true => first = false,
                false => pred_str.push_str(" OR "),
            }

            pred_str.push_str(&format!("({} = ?)", &self.bool_col_db));
            pred_vec.push(Box::new(bool_val.to_owned()));
        }

        if pred_count > 1 {
            pred_str.push(')');
        }

        Ok((pred_str, pred_vec))
    }
}

impl BoolFilter {
    fn new(bool_col_db: &'static str) -> Self {
        BoolFilter {
            bool_col_db,
            match_null: false,
            match_not_null: false,
            bool_vals: Vec::new(),
        }
    }

    pub fn add_bool_filter_to_query(
        bool_filter_pair: Pair<Rule>,
        query: &mut dyn Query,
    ) -> Result<(), FsPulseError> {
        let mut iter = bool_filter_pair.into_inner();
        let bool_col_pair = iter.next().unwrap();
        let bool_col = bool_col_pair.as_str().to_owned();

        let mut bool_filter = match query.col_set().col_name_to_db(&bool_col) {
            Some(bool_col_db) => Self::new(bool_col_db),
            None => {
                return Err(FsPulseError::CustomParsingError(format!(
                    "Column not found: '{bool_col}'"
                )))
            }
        };

        for bool_val_pair in iter {
            match bool_val_pair.as_rule() {
                Rule::null => bool_filter.match_null = true,
                Rule::not_null => bool_filter.match_not_null = true,
                rule => {
                    let rule_str = format!("{rule:?}");
                    let val_opt = Self::BOOL_VALUES.get(&rule_str).copied();
                    match val_opt {
                        Some(val) => bool_filter.bool_vals.push(val.to_owned()),
                        None => {
                            return Err(FsPulseError::CustomParsingError(format!(
                                "Invalid filter value: '{}'",
                                bool_val_pair.as_str()
                            )));
                        }
                    }
                }
            }
        }

        query.add_filter(Box::new(bool_filter));

        Ok(())
    }

    // Map boolean rule names to database values (1 for true, 0 for false)
    // Null and Not Null are handled directly in code
    const BOOL_VALUES: OrderedStrMap = phf_ordered_map! {
        "bool_true" => "1",
        "bool_false" => "0",
    };
}

// ==================================================================================
// Integer-backed Enum Filter (for scan_state and future integer-backed enums)
// ==================================================================================

/// Function pointer type for parsing enum tokens to database values
type EnumParser = fn(&str) -> Option<i64>;

/// Static map from column names to enum parsers
static ENUM_PARSERS: Map<&'static str, EnumParser> = phf_map! {
    "scan_state" => ScanState::from_token,
"item_type" => ItemType::from_token,
"alert_type" => AlertType::from_token,
"alert_status" => AlertStatus::from_token,
"val" => ValidationState::from_token,
"access" => Access::from_token,
};

/// Filter for integer-backed enums (like scan_state)
#[derive(Debug)]
pub struct EnumFilter {
    enum_col_db: &'static str,
    enum_vals: Vec<i64>,
    match_null: bool,
    match_not_null: bool,
}

impl Filter for EnumFilter {
    fn to_predicate_parts(&self) -> Result<(String, Vec<Box<dyn ToSql>>), FsPulseError> {
        let mut pred_str = String::new();
        let mut pred_vec: Vec<Box<dyn ToSql>> = Vec::new();
        let mut first = true;

        let mut pred_count = self.enum_vals.len();
        if self.match_null {
            pred_count += 1
        };
        if self.match_not_null {
            pred_count += 1
        };

        if pred_count > 1 {
            pred_str.push('(');
        }

        // if match_null is true, it will always be first
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
            pred_vec.push(Box::new(*enum_val));
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
            enum_vals: Vec::new(),
            match_null: false,
            match_not_null: false,
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
                    "Column not found: '{enum_col}'"
                )))
            }
        };

        // Get the parser for this enum column
        let parser = ENUM_PARSERS.get(enum_col.as_str()).ok_or_else(|| {
            FsPulseError::CustomParsingError(format!("Unknown enum column: '{}'", enum_col))
        })?;

        // Parse each enum value using the parser
        for enum_val_pair in iter {
            match enum_val_pair.as_rule() {
                Rule::null => enum_filter.match_null = true,
                Rule::not_null => enum_filter.match_not_null = true,
                _ => {
                    let token_str = enum_val_pair.as_str();
                    match parser(token_str) {
                        Some(db_val) => enum_filter.enum_vals.push(db_val),
                        None => {
                            return Err(FsPulseError::CustomParsingError(format!(
                                "Invalid {} value: '{}'",
                                enum_col, token_str
                            )))
                        }
                    }
                }
            }
        }

        query.add_filter(Box::new(enum_filter));

        Ok(())
    }
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
                    "Column not found: '{path_col}'"
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
                    "Column not found: '{int_col}'"
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::query::QueryParser;
    use pest::Parser;

    // ==================================================================================
    // ID Filter Tests
    // ==================================================================================

    #[test]
    fn test_id_filter_single_id() {
        let result = QueryParser::parse(Rule::id_filter_EOI, "42");
        assert!(
            result.is_ok(),
            "Failed to parse single ID '42': {:?}",
            result.err()
        );
    }

    #[test]
    fn test_id_filter_multiple_ids() {
        let result = QueryParser::parse(Rule::id_filter_EOI, "1, 5, 10");
        assert!(
            result.is_ok(),
            "Failed to parse multiple IDs: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_id_filter_range() {
        let result = QueryParser::parse(Rule::id_filter_EOI, "1..10");
        assert!(
            result.is_ok(),
            "Failed to parse range '1..10': {:?}",
            result.err()
        );
    }

    #[test]
    fn test_id_filter_multiple_ranges() {
        let result = QueryParser::parse(Rule::id_filter_EOI, "1..10, 20..30");
        assert!(
            result.is_ok(),
            "Failed to parse multiple ranges: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_id_filter_mixed() {
        let result = QueryParser::parse(Rule::id_filter_EOI, "1, 5..10, 15");
        assert!(
            result.is_ok(),
            "Failed to parse mixed IDs and ranges: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_id_filter_null() {
        let result = QueryParser::parse(Rule::id_filter_EOI, "null");
        assert!(result.is_ok(), "Failed to parse 'null': {:?}", result.err());
    }

    #[test]
    fn test_id_filter_not_null() {
        let result = QueryParser::parse(Rule::id_filter_EOI, "not null");
        assert!(
            result.is_ok(),
            "Failed to parse 'not null': {:?}",
            result.err()
        );
    }

    #[test]
    fn test_id_filter_whitespace() {
        let result = QueryParser::parse(Rule::id_filter_EOI, "  1 ,  5 .. 10  ,  15  ");
        assert!(
            result.is_ok(),
            "Failed to parse with whitespace: {:?}",
            result.err()
        );
    }

    // ==================================================================================
    // Date Filter Tests
    // ==================================================================================

    #[test]
    fn test_date_filter_single_date() {
        let result = QueryParser::parse(Rule::date_filter_EOI, "2025-01-15");
        assert!(
            result.is_ok(),
            "Failed to parse single date: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_date_filter_range() {
        let result = QueryParser::parse(Rule::date_filter_EOI, "2025-01-01..2025-01-31");
        assert!(
            result.is_ok(),
            "Failed to parse date range: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_date_filter_multiple_dates() {
        let result = QueryParser::parse(Rule::date_filter_EOI, "2025-01-01, 2025-02-01");
        assert!(
            result.is_ok(),
            "Failed to parse multiple dates: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_date_filter_null() {
        let result = QueryParser::parse(Rule::date_filter_EOI, "null");
        assert!(result.is_ok(), "Failed to parse 'null': {:?}", result.err());
    }

    #[test]
    fn test_date_filter_not_null() {
        let result = QueryParser::parse(Rule::date_filter_EOI, "not null");
        assert!(
            result.is_ok(),
            "Failed to parse 'not null': {:?}",
            result.err()
        );
    }

    #[test]
    fn test_date_filter_invalid_format() {
        let result = QueryParser::parse(Rule::date_filter_EOI, "01/15/2025");
        assert!(result.is_err(), "Should reject non-ISO date format");
    }

    // ==================================================================================
    // Bool Filter Tests
    // ==================================================================================

    #[test]
    fn test_bool_filter_true() {
        let result = QueryParser::parse(Rule::bool_filter_EOI, "true");
        assert!(result.is_ok(), "Failed to parse 'true': {:?}", result.err());

        let result = QueryParser::parse(Rule::bool_filter_EOI, "TRUE");
        assert!(result.is_ok(), "Failed to parse 'TRUE': {:?}", result.err());

        let result = QueryParser::parse(Rule::bool_filter_EOI, "T");
        assert!(result.is_ok(), "Failed to parse 'T': {:?}", result.err());
    }

    #[test]
    fn test_bool_filter_false() {
        let result = QueryParser::parse(Rule::bool_filter_EOI, "false");
        assert!(
            result.is_ok(),
            "Failed to parse 'false': {:?}",
            result.err()
        );

        let result = QueryParser::parse(Rule::bool_filter_EOI, "FALSE");
        assert!(
            result.is_ok(),
            "Failed to parse 'FALSE': {:?}",
            result.err()
        );

        let result = QueryParser::parse(Rule::bool_filter_EOI, "F");
        assert!(result.is_ok(), "Failed to parse 'F': {:?}", result.err());
    }

    #[test]
    fn test_bool_filter_null() {
        let result = QueryParser::parse(Rule::bool_filter_EOI, "null");
        assert!(result.is_ok(), "Failed to parse 'null': {:?}", result.err());
    }

    #[test]
    fn test_bool_filter_not_null() {
        let result = QueryParser::parse(Rule::bool_filter_EOI, "not null");
        assert!(
            result.is_ok(),
            "Failed to parse 'not null': {:?}",
            result.err()
        );
    }

    #[test]
    fn test_bool_filter_multiple() {
        let result = QueryParser::parse(Rule::bool_filter_EOI, "true, false");
        assert!(
            result.is_ok(),
            "Failed to parse multiple bool values: {:?}",
            result.err()
        );
    }

    // ==================================================================================
    // String Filter Tests
    // ==================================================================================

    #[test]
    fn test_string_filter_single() {
        let result = QueryParser::parse(Rule::string_filter_EOI, "'error'");
        assert!(
            result.is_ok(),
            "Failed to parse single string: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_string_filter_multiple() {
        let result = QueryParser::parse(Rule::string_filter_EOI, "'error', 'warning'");
        assert!(
            result.is_ok(),
            "Failed to parse multiple strings: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_string_filter_empty() {
        let result = QueryParser::parse(Rule::string_filter_EOI, "''");
        assert!(
            result.is_ok(),
            "Failed to parse empty string: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_string_filter_with_spaces() {
        let result = QueryParser::parse(Rule::string_filter_EOI, "'file not found'");
        assert!(
            result.is_ok(),
            "Failed to parse string with spaces: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_string_filter_escaped_quote() {
        let result = QueryParser::parse(Rule::string_filter_EOI, r"'can\'t open file'");
        assert!(
            result.is_ok(),
            "Failed to parse string with escaped quote: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_string_filter_null() {
        let result = QueryParser::parse(Rule::string_filter_EOI, "null");
        assert!(result.is_ok(), "Failed to parse 'null': {:?}", result.err());
    }

    #[test]
    fn test_string_filter_not_null() {
        let result = QueryParser::parse(Rule::string_filter_EOI, "not null");
        assert!(
            result.is_ok(),
            "Failed to parse 'not null': {:?}",
            result.err()
        );
    }

    // ==================================================================================
    // Path Filter Tests
    // ==================================================================================

    #[test]
    fn test_path_filter_single() {
        let result = QueryParser::parse(Rule::path_filter_EOI, "'/home/user'");
        assert!(
            result.is_ok(),
            "Failed to parse single path: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_path_filter_multiple() {
        let result = QueryParser::parse(Rule::path_filter_EOI, "'/var/log', '/tmp'");
        assert!(
            result.is_ok(),
            "Failed to parse multiple paths: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_path_filter_windows() {
        let result = QueryParser::parse(Rule::path_filter_EOI, r"'C:\Users\Documents'");
        assert!(
            result.is_ok(),
            "Failed to parse Windows path: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_path_filter_with_spaces() {
        let result = QueryParser::parse(Rule::path_filter_EOI, "'/home/my documents'");
        assert!(
            result.is_ok(),
            "Failed to parse path with spaces: {:?}",
            result.err()
        );
    }

    // ==================================================================================
    // Val Filter Tests (EnumFilter - integer-backed)
    // ==================================================================================

    #[test]
    fn test_val_filter_valid() {
        let result = QueryParser::parse(Rule::val_filter_EOI, "Valid");
        assert!(
            result.is_ok(),
            "Failed to parse 'Valid': {:?}",
            result.err()
        );

        let result = QueryParser::parse(Rule::val_filter_EOI, "V");
        assert!(result.is_ok(), "Failed to parse 'V': {:?}", result.err());
    }

    #[test]
    fn test_val_filter_invalid() {
        let result = QueryParser::parse(Rule::val_filter_EOI, "Invalid");
        assert!(
            result.is_ok(),
            "Failed to parse 'Invalid': {:?}",
            result.err()
        );

        let result = QueryParser::parse(Rule::val_filter_EOI, "I");
        assert!(result.is_ok(), "Failed to parse 'I': {:?}", result.err());
    }

    #[test]
    fn test_val_filter_no_validator() {
        let result = QueryParser::parse(Rule::val_filter_EOI, "No Validator");
        assert!(
            result.is_ok(),
            "Failed to parse 'No Validator': {:?}",
            result.err()
        );

        let result = QueryParser::parse(Rule::val_filter_EOI, "N");
        assert!(result.is_ok(), "Failed to parse 'N': {:?}", result.err());
    }

    #[test]
    fn test_val_filter_unknown() {
        let result = QueryParser::parse(Rule::val_filter_EOI, "Unknown");
        assert!(
            result.is_ok(),
            "Failed to parse 'Unknown': {:?}",
            result.err()
        );

        let result = QueryParser::parse(Rule::val_filter_EOI, "U");
        assert!(result.is_ok(), "Failed to parse 'U': {:?}", result.err());
    }

    #[test]
    fn test_val_filter_multiple() {
        let result = QueryParser::parse(Rule::val_filter_EOI, "Valid, Invalid");
        assert!(
            result.is_ok(),
            "Failed to parse multiple values: {:?}",
            result.err()
        );

        let result = QueryParser::parse(Rule::val_filter_EOI, "V, I, N");
        assert!(
            result.is_ok(),
            "Failed to parse multiple short codes: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_val_filter_null() {
        let result = QueryParser::parse(Rule::val_filter_EOI, "null");
        assert!(result.is_ok(), "Failed to parse 'null': {:?}", result.err());
    }

    // ==================================================================================
    // Item Type Filter Tests (EnumFilter - integer-backed)
    // ==================================================================================

    #[test]
    fn test_item_type_filter_file() {
        let result = QueryParser::parse(Rule::item_type_filter_EOI, "File");
        assert!(result.is_ok(), "Failed to parse 'File': {:?}", result.err());

        let result = QueryParser::parse(Rule::item_type_filter_EOI, "F");
        assert!(result.is_ok(), "Failed to parse 'F': {:?}", result.err());
    }

    #[test]
    fn test_item_type_filter_directory() {
        let result = QueryParser::parse(Rule::item_type_filter_EOI, "Directory");
        assert!(
            result.is_ok(),
            "Failed to parse 'Directory': {:?}",
            result.err()
        );

        let result = QueryParser::parse(Rule::item_type_filter_EOI, "Dir");
        assert!(result.is_ok(), "Failed to parse 'Dir': {:?}", result.err());

        let result = QueryParser::parse(Rule::item_type_filter_EOI, "D");
        assert!(result.is_ok(), "Failed to parse 'D': {:?}", result.err());
    }

    #[test]
    fn test_item_type_filter_symlink() {
        let result = QueryParser::parse(Rule::item_type_filter_EOI, "Symlink");
        assert!(
            result.is_ok(),
            "Failed to parse 'Symlink': {:?}",
            result.err()
        );

        let result = QueryParser::parse(Rule::item_type_filter_EOI, "S");
        assert!(result.is_ok(), "Failed to parse 'S': {:?}", result.err());
    }

    #[test]
    fn test_item_type_filter_multiple() {
        let result = QueryParser::parse(Rule::item_type_filter_EOI, "File, Directory");
        assert!(
            result.is_ok(),
            "Failed to parse multiple types: {:?}",
            result.err()
        );

        let result = QueryParser::parse(Rule::item_type_filter_EOI, "F, D, S");
        assert!(
            result.is_ok(),
            "Failed to parse multiple short codes: {:?}",
            result.err()
        );
    }

    // ==================================================================================
    // Alert Type Filter Tests (EnumFilter - integer-backed)
    // ==================================================================================

    #[test]
    fn test_alert_type_filter_suspicious_hash() {
        let result = QueryParser::parse(Rule::alert_type_filter_EOI, "H");
        assert!(result.is_ok(), "Failed to parse 'H': {:?}", result.err());

        let result = QueryParser::parse(Rule::alert_type_filter_EOI, "h");
        assert!(result.is_ok(), "Failed to parse 'h': {:?}", result.err());
    }

    #[test]
    fn test_alert_type_filter_invalid_file() {
        let result = QueryParser::parse(Rule::alert_type_filter_EOI, "I");
        assert!(result.is_ok(), "Failed to parse 'I': {:?}", result.err());

        let result = QueryParser::parse(Rule::alert_type_filter_EOI, "i");
        assert!(result.is_ok(), "Failed to parse 'i': {:?}", result.err());
    }

    #[test]
    fn test_alert_type_filter_multiple() {
        let result = QueryParser::parse(Rule::alert_type_filter_EOI, "H, I");
        assert!(
            result.is_ok(),
            "Failed to parse multiple alert types: {:?}",
            result.err()
        );
    }

    // ==================================================================================
    // Alert Status Filter Tests (EnumFilter - integer-backed)
    // ==================================================================================

    #[test]
    fn test_alert_status_filter_dismissed() {
        let result = QueryParser::parse(Rule::alert_status_filter_EOI, "D");
        assert!(result.is_ok(), "Failed to parse 'D': {:?}", result.err());

        let result = QueryParser::parse(Rule::alert_status_filter_EOI, "d");
        assert!(result.is_ok(), "Failed to parse 'd': {:?}", result.err());
    }

    #[test]
    fn test_alert_status_filter_flagged() {
        let result = QueryParser::parse(Rule::alert_status_filter_EOI, "F");
        assert!(result.is_ok(), "Failed to parse 'F': {:?}", result.err());

        let result = QueryParser::parse(Rule::alert_status_filter_EOI, "f");
        assert!(result.is_ok(), "Failed to parse 'f': {:?}", result.err());
    }

    #[test]
    fn test_alert_status_filter_open() {
        let result = QueryParser::parse(Rule::alert_status_filter_EOI, "O");
        assert!(result.is_ok(), "Failed to parse 'O': {:?}", result.err());

        let result = QueryParser::parse(Rule::alert_status_filter_EOI, "o");
        assert!(result.is_ok(), "Failed to parse 'o': {:?}", result.err());
    }

    #[test]
    fn test_alert_status_filter_multiple() {
        let result = QueryParser::parse(Rule::alert_status_filter_EOI, "D, F, O");
        assert!(
            result.is_ok(),
            "Failed to parse multiple alert statuses: {:?}",
            result.err()
        );
    }

    // ==================================================================================
    // Scan State Filter Tests (EnumFilter - Integer)
    // ==================================================================================

    #[test]
    fn test_scan_state_filter_single_full_name() {
        let result = QueryParser::parse(Rule::scan_state_filter_EOI, "Scanning");
        assert!(
            result.is_ok(),
            "Failed to parse 'Scanning': {:?}",
            result.err()
        );

        let result = QueryParser::parse(Rule::scan_state_filter_EOI, "Completed");
        assert!(
            result.is_ok(),
            "Failed to parse 'Completed': {:?}",
            result.err()
        );
    }

    #[test]
    fn test_scan_state_filter_single_short_code() {
        let result = QueryParser::parse(Rule::scan_state_filter_EOI, "S");
        assert!(result.is_ok(), "Failed to parse 'S': {:?}", result.err());

        let result = QueryParser::parse(Rule::scan_state_filter_EOI, "C");
        assert!(result.is_ok(), "Failed to parse 'C': {:?}", result.err());

        let result = QueryParser::parse(Rule::scan_state_filter_EOI, "P");
        assert!(
            result.is_ok(),
            "Failed to parse 'P' (Stopped): {:?}",
            result.err()
        );

        let result = QueryParser::parse(Rule::scan_state_filter_EOI, "AF");
        assert!(
            result.is_ok(),
            "Failed to parse 'AF' (Analyzing Files): {:?}",
            result.err()
        );

        let result = QueryParser::parse(Rule::scan_state_filter_EOI, "AS");
        assert!(
            result.is_ok(),
            "Failed to parse 'AS' (Analyzing Scan): {:?}",
            result.err()
        );
    }

    #[test]
    fn test_scan_state_filter_case_variations() {
        let result = QueryParser::parse(Rule::scan_state_filter_EOI, "scanning");
        assert!(
            result.is_ok(),
            "Failed to parse 'scanning' (lowercase): {:?}",
            result.err()
        );

        let result = QueryParser::parse(Rule::scan_state_filter_EOI, "SCANNING");
        assert!(
            result.is_ok(),
            "Failed to parse 'SCANNING' (uppercase): {:?}",
            result.err()
        );

        let result = QueryParser::parse(Rule::scan_state_filter_EOI, "Scanning");
        assert!(
            result.is_ok(),
            "Failed to parse 'Scanning' (titlecase): {:?}",
            result.err()
        );

        let result = QueryParser::parse(Rule::scan_state_filter_EOI, "s");
        assert!(
            result.is_ok(),
            "Failed to parse 's' (lowercase short code): {:?}",
            result.err()
        );

        let result = QueryParser::parse(Rule::scan_state_filter_EOI, "S");
        assert!(
            result.is_ok(),
            "Failed to parse 'S' (uppercase short code): {:?}",
            result.err()
        );
    }

    #[test]
    fn test_scan_state_filter_multiple_values() {
        let result = QueryParser::parse(Rule::scan_state_filter_EOI, "Scanning, Completed");
        assert!(
            result.is_ok(),
            "Failed to parse 'Scanning, Completed': {:?}",
            result.err()
        );

        let result = QueryParser::parse(Rule::scan_state_filter_EOI, "S, C, E");
        assert!(
            result.is_ok(),
            "Failed to parse 'S, C, E': {:?}",
            result.err()
        );

        let result = QueryParser::parse(Rule::scan_state_filter_EOI, "Scanning, C, Error");
        assert!(
            result.is_ok(),
            "Failed to parse mixed 'Scanning, C, Error': {:?}",
            result.err()
        );
    }

    #[test]
    fn test_scan_state_filter_all_states() {
        let result = QueryParser::parse(Rule::scan_state_filter_EOI, "Scanning");
        assert!(
            result.is_ok(),
            "Failed to parse 'Scanning': {:?}",
            result.err()
        );

        let result = QueryParser::parse(Rule::scan_state_filter_EOI, "Sweeping");
        assert!(
            result.is_ok(),
            "Failed to parse 'Sweeping': {:?}",
            result.err()
        );

        let result = QueryParser::parse(Rule::scan_state_filter_EOI, "Analyzing Files");
        assert!(
            result.is_ok(),
            "Failed to parse 'Analyzing Files': {:?}",
            result.err()
        );

        let result = QueryParser::parse(Rule::scan_state_filter_EOI, "Analyzing");
        assert!(
            result.is_ok(),
            "Failed to parse 'Analyzing' (backward compat): {:?}",
            result.err()
        );

        let result = QueryParser::parse(Rule::scan_state_filter_EOI, "Analyzing Scan");
        assert!(
            result.is_ok(),
            "Failed to parse 'Analyzing Scan': {:?}",
            result.err()
        );

        let result = QueryParser::parse(Rule::scan_state_filter_EOI, "Completed");
        assert!(
            result.is_ok(),
            "Failed to parse 'Completed': {:?}",
            result.err()
        );

        let result = QueryParser::parse(Rule::scan_state_filter_EOI, "Stopped");
        assert!(
            result.is_ok(),
            "Failed to parse 'Stopped': {:?}",
            result.err()
        );

        let result = QueryParser::parse(Rule::scan_state_filter_EOI, "Error");
        assert!(
            result.is_ok(),
            "Failed to parse 'Error': {:?}",
            result.err()
        );

        let result = QueryParser::parse(Rule::scan_state_filter_EOI, "S");
        assert!(result.is_ok(), "Failed to parse 'S': {:?}", result.err());

        let result = QueryParser::parse(Rule::scan_state_filter_EOI, "W");
        assert!(result.is_ok(), "Failed to parse 'W': {:?}", result.err());

        let result = QueryParser::parse(Rule::scan_state_filter_EOI, "AF");
        assert!(result.is_ok(), "Failed to parse 'AF': {:?}", result.err());

        let result = QueryParser::parse(Rule::scan_state_filter_EOI, "A");
        assert!(result.is_ok(), "Failed to parse 'A': {:?}", result.err());

        let result = QueryParser::parse(Rule::scan_state_filter_EOI, "AS");
        assert!(result.is_ok(), "Failed to parse 'AS': {:?}", result.err());

        let result = QueryParser::parse(Rule::scan_state_filter_EOI, "C");
        assert!(result.is_ok(), "Failed to parse 'C': {:?}", result.err());

        let result = QueryParser::parse(Rule::scan_state_filter_EOI, "P");
        assert!(result.is_ok(), "Failed to parse 'P': {:?}", result.err());

        let result = QueryParser::parse(Rule::scan_state_filter_EOI, "E");
        assert!(result.is_ok(), "Failed to parse 'E': {:?}", result.err());
    }

    #[test]
    fn test_scan_state_filter_invalid_value() {
        let result = QueryParser::parse(Rule::scan_state_filter_EOI, "Invalid");
        assert!(result.is_err(), "Should reject invalid state 'Invalid'");

        let result = QueryParser::parse(Rule::scan_state_filter_EOI, "Pending");
        assert!(result.is_err(), "Should reject removed state 'Pending'");

        let result = QueryParser::parse(Rule::scan_state_filter_EOI, "T");
        assert!(result.is_err(), "Should reject old Stopped code 'T'");
    }

    #[test]
    fn test_scan_state_filter_whitespace() {
        let result = QueryParser::parse(Rule::scan_state_filter_EOI, " Scanning ");
        assert!(
            result.is_ok(),
            "Failed to parse with whitespace: {:?}",
            result.err()
        );

        let result = QueryParser::parse(Rule::scan_state_filter_EOI, "Scanning , Completed");
        assert!(
            result.is_ok(),
            "Failed to parse with comma whitespace: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_enum_filter_predicate_generation() {
        let filter = EnumFilter {
            enum_col_db: "state",
            enum_vals: vec![1],
            match_null: false,
            match_not_null: false,
        };

        let result = filter.to_predicate_parts();
        assert!(result.is_ok());
        let (pred_str, pred_vec) = result.unwrap();
        assert_eq!(pred_str, "(state = ?)");
        assert_eq!(pred_vec.len(), 1);
    }

    #[test]
    fn test_enum_filter_predicate_multiple_values() {
        let filter = EnumFilter {
            enum_col_db: "state",
            enum_vals: vec![1, 4, 6],
            match_null: false,
            match_not_null: false,
        };

        let result = filter.to_predicate_parts();
        assert!(result.is_ok());
        let (pred_str, pred_vec) = result.unwrap();
        assert_eq!(pred_str, "((state = ?) OR (state = ?) OR (state = ?))");
        assert_eq!(pred_vec.len(), 3);
    }

    #[test]
    fn test_enum_filter_predicate_null_only() {
        let filter = EnumFilter {
            enum_col_db: "state",
            enum_vals: vec![],
            match_null: true,
            match_not_null: false,
        };
        let (pred_str, pred_vec) = filter.to_predicate_parts().unwrap();
        assert_eq!(pred_str, "(state IS NULL)");
        assert_eq!(pred_vec.len(), 0);
    }

    #[test]
    fn test_enum_filter_predicate_not_null_only() {
        let filter = EnumFilter {
            enum_col_db: "state",
            enum_vals: vec![],
            match_null: false,
            match_not_null: true,
        };
        let (pred_str, pred_vec) = filter.to_predicate_parts().unwrap();
        assert_eq!(pred_str, "(state IS NOT NULL)");
        assert_eq!(pred_vec.len(), 0);
    }

    #[test]
    fn test_enum_filter_predicate_null_with_values() {
        let filter = EnumFilter {
            enum_col_db: "state",
            enum_vals: vec![1, 2],
            match_null: true,
            match_not_null: false,
        };
        let (pred_str, pred_vec) = filter.to_predicate_parts().unwrap();
        assert_eq!(pred_str, "((state IS NULL) OR (state = ?) OR (state = ?))");
        assert_eq!(pred_vec.len(), 2);
    }

    #[test]
    fn test_enum_filter_predicate_null_and_not_null() {
        let filter = EnumFilter {
            enum_col_db: "state",
            enum_vals: vec![],
            match_null: true,
            match_not_null: true,
        };
        let (pred_str, pred_vec) = filter.to_predicate_parts().unwrap();
        assert_eq!(pred_str, "((state IS NULL) OR (state IS NOT NULL))");
        assert_eq!(pred_vec.len(), 0);
    }

    // ==================================================================================
    // Int Filter Tests
    // ==================================================================================

    #[test]
    fn test_int_filter_greater_than() {
        let result = QueryParser::parse(Rule::int_filter_EOI, "> 100");
        assert!(
            result.is_ok(),
            "Failed to parse '> 100': {:?}",
            result.err()
        );

        let result = QueryParser::parse(Rule::int_filter_EOI, ">100");
        assert!(result.is_ok(), "Failed to parse '>100': {:?}", result.err());
    }

    #[test]
    fn test_int_filter_less_than() {
        let result = QueryParser::parse(Rule::int_filter_EOI, "< 100");
        assert!(
            result.is_ok(),
            "Failed to parse '< 100': {:?}",
            result.err()
        );

        let result = QueryParser::parse(Rule::int_filter_EOI, "<100");
        assert!(result.is_ok(), "Failed to parse '<100': {:?}", result.err());
    }

    #[test]
    fn test_int_filter_zero() {
        let result = QueryParser::parse(Rule::int_filter_EOI, "> 0");
        assert!(result.is_ok(), "Failed to parse '> 0': {:?}", result.err());
    }

    #[test]
    fn test_int_filter_large_value() {
        let result = QueryParser::parse(Rule::int_filter_EOI, "> 1000000000");
        assert!(
            result.is_ok(),
            "Failed to parse large value: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_int_filter_whitespace() {
        let result = QueryParser::parse(Rule::int_filter_EOI, "  >   100  ");
        assert!(
            result.is_ok(),
            "Failed to parse with whitespace: {:?}",
            result.err()
        );
    }
}
