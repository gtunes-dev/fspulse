use crate::{error::FsPulseError, utils::Utils};
use pest::iterators::Pair;
use rusqlite::ToSql;
use std::fmt::{self, Debug};

use super::{query::QueryType, Rule};

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
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IdSpec {
    Id(i64),
    IdRange { id_start: i64, id_end: i64 },
}

impl fmt::Display for IdType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            IdType::Root => "root_id",
            IdType::Item => "item_id",
            IdType::Scan => "scan_id",
            IdType::Change => "change_id",
        };
        write!(f, "{}", s)
    }
}

impl IdType {
    fn from_rule(rule: Rule) -> Self {
        match rule {
            Rule::root_id_filter => IdType::Root,
            Rule::item_id_filter => IdType::Item,
            Rule::scan_id_filter => IdType::Scan,
            Rule::change_id_filter => IdType::Change,
            _ => unreachable!(),
        }
    }
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
                    pred_str.push_str(&format!("({} = ?)", self.id_type));
                    pred_vec.push(Box::new(*id));
                }
                IdSpec::IdRange { id_start, id_end } => {
                    pred_str.push_str(&format!("({0} >= ? AND {0} <= ?)", self.id_type));
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
    TimeOfScan,
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
            DateType::TimeOfScan => "scan_time",
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
            Rule::time_of_scan_filter => DateType::TimeOfScan,
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
                (DateType::TimeOfScan, QueryType::Changes) => pred_str.push_str(
                    "(scan_id IN (SELECT scan_id FROM scans WHERE scan_time BETWEEN ? AND ?))",
                ),
                (DateType::TimeOfScan, QueryType::Scans) => {
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

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ChangeFilter {
    change_types: String,
}

impl Filter for ChangeFilter {
    fn to_predicate_parts(
        &self,
        _query_type: QueryType,
    ) -> Result<(String, Vec<Box<dyn ToSql>>), FsPulseError> {
        let mut pred_str = " (change_type IN (".to_string();
        let mut pred_vec: Vec<Box<dyn ToSql>> = Vec::new();

        let mut first: bool = true;
        for c in self.change_types.chars() {
            match first {
                true => {
                    first = false;
                    pred_str.push('?');
                }
                false => pred_str.push_str(", ?"),
            }
            let change_type_str = c.to_string();
            pred_vec.push(Box::new(change_type_str));
        }

        pred_str.push_str("))");

        Ok((pred_str, pred_vec))
    }
}

impl ChangeFilter {
    fn new() -> Self {
        Self::default()
    }

    pub fn build(filter_change: Pair<Rule>) -> Result<Self, FsPulseError> {
        let mut change_filter = ChangeFilter::new();
        for change in filter_change.into_inner() {
            let change_str = change.as_str();
            let change_str_upper = change_str.to_uppercase();

            // disallow specifying the same type multiple times
            if change_filter.change_types.contains(&change_str_upper) {
                return Err(FsPulseError::Error(format!(
                    "Change filter contains multiple instances of '{}'",
                    change_str
                )));
            }
            change_filter.change_types.push_str(&change_str_upper);
        }
        Ok(change_filter)
    }
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct PathFilter {
    path_strs: Vec<String>,
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
            match query_type {
                QueryType::Roots => pred_str.push_str("(root_path LIKE ?)"),
                QueryType::Items => pred_str.push_str("(item_path LIKE ?)"),
                _ => unreachable!()
            }

            //let change_type_str = c.to_string();
            let like_str = format!("%{path_str}%");
            pred_vec.push(Box::new(like_str));
        }

        pred_str.push(')');

        Ok((pred_str, pred_vec))
    }
}

impl PathFilter {
    fn new() -> Self {
        PathFilter {
            path_strs: Vec::new(),
        }
    }

    pub fn build(path_filter_pair: Pair<Rule>) -> Result<PathFilter, FsPulseError> {
        let mut path_filter = Self::new();

        for path_spec in path_filter_pair.into_inner() {
            path_filter.path_strs.push(path_spec.as_str().to_string());
        }

        Ok(path_filter)
    }
}
