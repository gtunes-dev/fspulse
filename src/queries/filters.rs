use pest::iterators::Pair;
use rusqlite::ToSql;
use std::fmt::{self, Debug};
use crate::{error::FsPulseError, utils::Utils};

use super::Rule;

/// Defines the behavior of a filter.
pub trait Filter: Debug {
    /// return predicate text and params
    fn to_predicate_parts(&self) -> Result<(String, Vec<Box<dyn ToSql>>), FsPulseError>;
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
    IdRange { 
        id_start: i64, 
        id_end: i64 
    },
}

impl fmt::Display for IdType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            IdType::Root   => "root_id",
            IdType::Item   => "item_id",
            IdType::Scan   => "scan_id",
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
            _ => unreachable!()
        }
    }
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
                IdSpec::Id (id) => {
                    pred_str.push_str(&format!("({} = ?)", self.id_type));
                    pred_vec.push(Box::new(*id));
                },
                IdSpec::IdRange { id_start, id_end} => {
                    pred_str.push_str(&format!("({0} >= ? AND {0} <= ?)", self.id_type));
                    pred_vec.push(Box::new(*id_start));
                    pred_vec.push(Box::new(*id_end));
                },
            }
        };

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
            id_specs: Vec::new()
        }
    }

    pub fn build(id_filter_pair: Pair<Rule>) -> Result<IdFilter, FsPulseError> {

        let id_type = IdType::from_rule(id_filter_pair.as_rule());
        let mut id_filter = Self::new(id_type);

        for id_spec in id_filter_pair.into_inner() {
            match id_spec.as_rule() {
                Rule::id => {
                    let id: i64 = id_spec.as_str().parse().unwrap();
                    id_filter.id_specs.push(
                        IdSpec::Id(
                            id,
                        )
                    )
                },
                Rule::id_range => {
                    let mut range_inner = id_spec.into_inner();

                    let id_start: i64 = range_inner.next().unwrap().as_str().parse().unwrap();
                    let id_end: i64 = range_inner.next().unwrap().as_str().parse().unwrap();
                    id_filter.id_specs.push(
                        IdSpec::IdRange { 
                            id_start, 
                            id_end,
                        }
                    )
                },
                _ => unreachable!()
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
    ScanDate,
    ModDate,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DateSpec {
    date_start: i64, 
    date_end: i64,
}
impl fmt::Display for DateType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            DateType::ScanDate   => "time_of_scan",
            DateType::ModDate   => "mod_date",
        };
        write!(f, "{}", s)
    }
}

impl DateType {
    fn from_rule(rule: Rule) -> Self {
        match rule {
            Rule::scan_date_filter => DateType::ScanDate,
            Rule::mod_date_filter => DateType::ModDate,
            _ => unreachable!()
        }
    }
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

            match self.date_type {
                DateType::ScanDate => {
                    // TODO$: This is broken because this query needs to be different for a scans
                    // query versus a changes query. this current version is inteded for changes
                    pred_str.push_str("(scan_id IN (SELECT id FROM scans WHERE time_of_scan >= ? AND time_of_scan <= ?))");
                }
                DateType::ModDate => {
                    return Err(FsPulseError::Error("Date filter isn't yet implemented for moddate".into()));
                }
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
            date_specs: Vec::new()
        }
    }
    
    pub fn build(date_filter_pair: Pair<Rule>) -> Result<DateFilter, FsPulseError> {
        let date_type = DateType::from_rule(date_filter_pair.as_rule());
        let mut date_filter = DateFilter::new(date_type);

        for date_spec in date_filter_pair.into_inner() {
            match date_spec.as_rule() {
                Rule::date => {
                    let date_start_str = date_spec.as_str();
                    let (date_start, date_end) = Utils::single_date_bounds(date_start_str)?;
                    date_filter.date_specs.push(
                        DateSpec { 
                            date_start, 
                            date_end, 
                        }
                    );
                },
                Rule::date_range => {
                    let mut range_inner = date_spec.into_inner();
                    let date_start_str = range_inner.next().unwrap().as_str();
                    let date_end_str = range_inner.next().unwrap().as_str();
                    let (date_start, date_end) = Utils::range_date_bounds(date_start_str, date_end_str)?;
                    date_filter.date_specs.push(
                        DateSpec { 
                            date_start, 
                            date_end, 
                        }
                    );
                },
                _ => unreachable!()
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
    fn to_predicate_parts(&self) -> Result<(String, Vec<Box<dyn ToSql>>), FsPulseError> {
        let mut pred_str = " (change_type IN (".to_string();
        let mut pred_vec: Vec<Box<dyn ToSql>> = Vec::new();

        let mut first: bool = true;
        for c in self.change_types.chars() {
            match first {
                true => {
                    first = false;
                    pred_str.push('?');
                },
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

    pub fn build(filter_change: Pair<Rule>) -> Result<ChangeFilter, FsPulseError> {
        let mut change_filter = ChangeFilter::new();
        for change in filter_change.into_inner() {

            let change_str = change.as_str();
            let change_str_upper = change_str.to_uppercase();

            // disallow specifying the same type multiple times
            if change_filter.change_types.contains(&change_str_upper) {
                return Err(FsPulseError::Error(format!("Change filter contains multiple instances of '{}'", change_str)));
            }
            change_filter.change_types.push_str(&change_str_upper);
        }
        Ok(change_filter)
    }
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct RootFilter {
    root_ids: Vec<i64>,

}

impl Filter for RootFilter {
    fn to_predicate_parts(&self) -> Result<(String, Vec<Box<dyn ToSql>>), FsPulseError> {
        /* 
        let pred_str = " (root_id IN (?))";
        let pred_vec: Vec<&dyn ToSql> = self.root_ids.iter()
            .map(|id| id as &dyn ToSql)
            .collect();
        */

        let mut first = true;
        let mut pred_str = " (root_id IN (".to_string();
        let mut pred_vec: Vec<Box<dyn ToSql>> = Vec::new();
        for root_id in &self.root_ids {
            match first {
                true => {
                    first = false;
                    pred_str.push('?');
                }
                false => {
                    pred_str.push_str(", ?");
                }
            }
            pred_vec.push(Box::new(*root_id));
        }
        pred_str.push_str("))");

        Ok((pred_str, pred_vec))
    }
}