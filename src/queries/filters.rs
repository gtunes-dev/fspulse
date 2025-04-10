use pest::iterators::Pair;
use rusqlite::ToSql;
use std::fmt::Debug;
use crate::{error::FsPulseError, utils::Utils};

use super::Rule;

/// Defines the behavior of a filter.
pub trait Filter: Debug {
    /// return predicate text and params
    fn to_predicate_parts(&self) -> Result<(String, Vec<Box<dyn ToSql>>), FsPulseError>;
}
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ScanFilter {
    elements: Vec<ScanFilterElement>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScanFilterElement {
    SingleScan(i64),
    ScanRange { 
        start_scan_id: i64, 
        end_scan_id: i64 
    },
    DateRange {
        start_datetime: i64,
        end_datetime: i64
    }
}

impl Filter for ScanFilter {
    fn to_predicate_parts(&self) -> Result<(String, Vec<Box<dyn ToSql>>), FsPulseError> {
        let mut pred_str = String::new();
        let mut pred_vec: Vec<Box<dyn ToSql>> = Vec::new();
        let mut first = true;

        if self.elements.len() > 1 {
            pred_str.push('(');
        }

        for element in &self.elements {
            match first {
                true => first = false,
                false => pred_str.push_str(" OR "),
            }
            
            match element {
                ScanFilterElement::SingleScan (scan_id) => {
                    pred_str.push_str("(scan_id = ?)");
                    pred_vec.push(Box::new(*scan_id));
                },
                ScanFilterElement::ScanRange { start_scan_id, end_scan_id} => {
                    pred_str.push_str("(scan_id >= ? AND scan_id <= ?)");
                    pred_vec.push(Box::new(*start_scan_id));
                    pred_vec.push(Box::new(*end_scan_id));
                },
                ScanFilterElement::DateRange { start_datetime, end_datetime } => {
                    pred_str.push_str("(scan_id IN (SELECT id FROM scans WHERE time_of_scan >= ? AND time_of_scan <= ?))");
                    pred_vec.push(Box::new(*start_datetime));
                    pred_vec.push(Box::new(*end_datetime));
                },
            }
        };

        if self.elements.len() > 1 {
            pred_str.push(')');
        }

        Ok((pred_str, pred_vec))
    }
}

impl ScanFilter {
    fn  new() -> Self {
        ScanFilter::default()
    }

    pub fn build(filter_scan: Pair<Rule>) -> Result<ScanFilter, FsPulseError> {
        let mut scan_filter = Self::new();

        for element in filter_scan.into_inner() {
            match element.as_rule() {
                Rule::scan_id => {
                    let scan_id_str = element.as_str();
                    scan_filter.elements.push(
                        ScanFilterElement::SingleScan(
                            scan_id_str.parse().unwrap()
                        )
                    );
                },
                Rule::scan_range => {
                    let mut range_inner = element.into_inner();

                    let start = range_inner.next().unwrap().as_str();
                    let end = range_inner.next().unwrap().as_str();
                    scan_filter.elements.push(
                        ScanFilterElement::ScanRange { 
                            start_scan_id: start.parse().unwrap(), 
                            end_scan_id: end.parse().unwrap(),
                        }
                    );
                },
                Rule::date => {
                    let start_date_str = element.as_str();
                    let (start_datetime, end_datetime) = Utils::single_date_bounds(start_date_str)?;
                    scan_filter.elements.push(
                        ScanFilterElement::DateRange { 
                            start_datetime, 
                            end_datetime, 
                        }
                    );
                },
                Rule::date_range => {
                    let mut range_inner = element.into_inner();

                    let start_date_str = range_inner.next().unwrap().as_str();
                    let end_date_str = range_inner.next().unwrap().as_str();
                    let (start_datetime, end_datetime) = Utils::range_date_bounds(start_date_str, end_date_str)?;
                    scan_filter.elements.push(
                        ScanFilterElement::DateRange { 
                            start_datetime, 
                            end_datetime, 
                        }
                    );
                },
                _ => unreachable!(),
            }
        }

        Ok(scan_filter)
    }
}

/*

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DateFilter {
    start_date: String,
    end_date: String,
}

impl DateFilter {
    pub fn build(filter_date: Pair<Rule>) -> DateFilter {
        let mut date_range_inner = filter_date.into_inner();

        let start = date_range_inner.next().unwrap().as_str();
        let end = date_range_inner.next().unwrap().as_str();

        DateFilter { 
            start_date: start.into(), 
            end_date: end.into(),
        }
    }
}
    */

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

impl RootFilter {
    fn new() -> Self {
        RootFilter::default()
    }

    pub fn build(root_filter_pair: Pair<Rule>) -> Result<RootFilter, FsPulseError> {
        let mut root_filter = RootFilter::new();
        for root in root_filter_pair.into_inner() {

            let root_str = root.as_str().to_string();
            let root_id: i64 = root_str.parse().unwrap();

            // disallow specifying the same type multiple times
            if root_filter.root_ids.contains(&root_id) {
                return Err(FsPulseError::Error(format!("Root filter contains multiple instances of '{}'", root_str)));
            }
            root_filter.root_ids.push(root_id);
        }
        Ok(root_filter)
    }
}