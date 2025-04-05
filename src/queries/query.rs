use log::error;
use pest::{iterators::{Pair, Pairs}, Parser};

use crate::{database::Database, error::FsPulseError};

use super::{QueryParser, Rule};

        //println!("Parsed items_query: {:?}", pairs.next().unwrap());

  /*       for pair in pairs {
            match pair.as_rule() {
                Rule::
            }
        } */
        // testing

        /*
        Query::parse(Rule::scan_filter, "scan:(32..34)");
        Query::parse(Rule::item_filter, "scan:(32..34)");
        Query::parse(Rule::items_query, "items where scan:(32..34)");
        Query::parse(Rule::query, "items where scan:(32..34)");
        */

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScanFilter {
    SingleScan(i64),
    ScanRange { 
        start_scan_id: i64, 
        end_scan_id: i64 
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DateFilter {
    start_date: String,
    end_date: String,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ItemsQuery {
    pub scan_filter: Option<ScanFilter>,
    pub date_filter: Option<DateFilter>,
}

pub struct Query;

impl Query {
    pub fn process_query(_db: &Database, query: &str) -> Result<(), FsPulseError> {
        let mut pairs = Query::parse(Rule::query, query)?;
        println!("Parsed query: {}", pairs);

        let query_pair = pairs.next().unwrap();
        let domain_query_pairs = query_pair.into_inner();

        for pair in domain_query_pairs {
            match pair.as_rule() {
                Rule::items_query => {
                    let _items_query = Query::build_items_query(pair)?;
                },
                Rule::scans_query => {

                },
                Rule::roots_query => {

                },
                Rule::changes_query => {

                },
                Rule::paths_query => {

                },
                _ => {}
            }
        }

        Ok(())
    }

    fn parse(rule: Rule, s: &str) -> Result<Pairs<Rule>, FsPulseError> {
        match QueryParser::parse(rule, s) {
            Ok(pairs) => {
                Ok(pairs)
            }
            Err(e) => {
                let e_str = e.to_string();
                error!("Failed to parse query:\n{}", e_str);
                Err(FsPulseError::Error(format!("Failed to parse query:\n{}", e_str)))
            }
        }
    }

    fn build_items_query(items_query_pair: Pair<Rule>) -> Result<ItemsQuery, FsPulseError> {
        let mut items_query = ItemsQuery::default();
        
        // items_query is defined as: items_where ~ filter_set_items.
        let mut inner = items_query_pair.into_inner();
        
        // The first child is the items_where part. We can skip it.
        let _items_where = inner.next().ok_or_else(|| FsPulseError::Error("Missing 'items where'".into()))?;
        
        // The second child should be the filter_set_items.
        let filter_set_pair = inner.next().ok_or_else(|| FsPulseError::Error("Missing filters on 'items where'".into()))?;
        
        // Now iterate over the children of filter_set_items.
        for filter_outer in filter_set_pair.into_inner() {
            let filter_inner = filter_outer.into_inner().next().unwrap();
            //let y = filter.into_inner().next().unwrap();
            match filter_inner.as_rule() {
                Rule::scan_filter => {
                    if items_query.scan_filter.is_some() {
                        return Err(FsPulseError::Error("Scan filter is already defined".into()));
                    }
                    let inner_scan = filter_inner.into_inner().next().unwrap();
                    items_query.scan_filter = Some(Self::build_scan_filter(inner_scan));
                },
                Rule::date_filter => {
                    let inner_scan = filter_inner.into_inner().next().unwrap();
                    items_query.date_filter = Some(Self::build_date_filter(inner_scan));
                    // Process the date filter similarly.
                    // items_query.date_filter = Some(build_date_filter(filter)?);
                },
                Rule::filter_change => {
                    // Process the change filter.
                    // items_query.change_filter = Some(build_change_filter(filter)?);
                },
                Rule::filter_validity => {
                    // Process the validity filter.
                    //items_query.validity_filter = Some(build_validity_filter(filter)?);
                },
                _ => {
                    // You could log an unexpected rule or ignore it.
                },
            }
        }
        
        Ok(items_query)
    }

    fn build_scan_filter(pair: Pair<Rule>) -> ScanFilter {
        match pair.as_rule() {
            Rule::single_scan => {
                let scan_id_str = pair.as_str();
                ScanFilter::SingleScan(
                    scan_id_str.parse().unwrap()
                )
            },
            Rule::scan_range => {
                let mut range_inner = pair.into_inner();

                let start = range_inner.next().unwrap().as_str();
                let end = range_inner.next().unwrap().as_str();
                ScanFilter::ScanRange { 
                    start_scan_id: start.parse().unwrap(), 
                    end_scan_id: end.parse().unwrap(),
                }
            },
            _ => unreachable!(),
        }
    }

    fn build_date_filter(pair: Pair<Rule>) -> DateFilter {
        let mut date_range_inner = pair.into_inner();

        let start = date_range_inner.next().unwrap().as_str();
        let end = date_range_inner.next().unwrap().as_str();

        DateFilter { 
            start_date: start.into(), 
            end_date: end.into(),
        }
    }
}