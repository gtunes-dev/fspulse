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
pub struct ChangeFilter {
    include_adds: bool,
    include_modifies: bool,
    include_deletes: bool,
    include_type_changes: bool
}


#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ItemsQuery {
    scan_filter: Option<ScanFilter>,
    date_filter: Option<DateFilter>,
    change_filter: Option<ChangeFilter>,
}

impl ChangeFilter {
    pub fn _get_include_adds(&self) -> bool {
        self.include_adds
    }

    pub fn _get_include_modifies(&self) -> bool {
        self.include_modifies
    }

    pub fn _get_include_deletes(&self) -> bool {
        self.include_deletes
    }

    pub fn _get_include_type_changes(&self) -> bool {
        self.include_type_changes
    }

    pub fn set_include_type(&mut self, include_type: &str) -> Result<(), FsPulseError> {
        let s = include_type.trim().to_uppercase();
        let bool_ref = match s.as_str() {
            "A" => &mut self.include_adds,
            "D" => &mut self.include_deletes,
            "M" => &mut self.include_modifies,
            "T" => &mut self.include_type_changes,
            _ => {
                return Err(FsPulseError::Error("Unknown change type".into()))
            }
        };
        if *bool_ref {
            return Err(FsPulseError::Error(format!("Change filter of '{}' is already defined", s)))
        }
        *bool_ref = true;
        Ok(())
    }
}

impl ItemsQuery {
    pub fn _get_scan_filter(&self) -> &Option<ScanFilter> {
        &self.scan_filter
    }

    pub fn set_scan_filter(&mut self, scan_filter: ScanFilter ) -> Result<(), FsPulseError> {
        if self.scan_filter.is_some() {
            Err(FsPulseError::Error("Scan filter is already defined".into()))
        } else {
            self.scan_filter = Some(scan_filter);
            Ok(())
        }
    }

    pub fn _get_date_filter(&self) -> &Option<DateFilter> {
        &self.date_filter
    }

    pub fn set_date_filter(&mut self, date_filter: DateFilter) -> Result<(), FsPulseError> {
        if self.date_filter.is_some() {
            Err(FsPulseError::Error("Date filter is already defined".into()))
        } else {
            self.date_filter = Some(date_filter);
            Ok(())
        }
    }

    pub fn _get_change_filter(&self) -> &Option<ChangeFilter> {
        &self.change_filter
    }

    fn get_change_filter_mut(&mut self) -> Option<&mut ChangeFilter> {
        self.change_filter.as_mut()
    }

    fn set_change_filter(&mut self, change_filter: ChangeFilter) -> Result<(), FsPulseError> {
        if self.change_filter.is_some() {
            // This shouldn't be reachable. We use a single instance of ChangeFilter
            // to model all changes so if we hit this, it's a code error, not
            // a user error
            error!("Change filter is already set");
            Err(FsPulseError::Error("Change filter is already set".into()))
        } else {
            self.change_filter = Some(change_filter);
            Ok(())
        }
    }
}

pub struct Query;

impl Query {
    pub fn process_query(_db: &Database, query: &str) -> Result<(), FsPulseError> {

        // for testing during coding
        //let query = "items where scan:(32),change:(A),change:(A)";

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
        
        let fs_items = inner.next().ok_or_else(|| FsPulseError::Error("Missing filters on 'items where'".into()))?;
        
        // Now iterate over the children of filter_set_items.
        for fs_items_filter in fs_items.into_inner() {
            let filter = fs_items_filter.into_inner().next().unwrap();
            match filter.as_rule() {
                Rule::filter_scan => {
                    let filter_scan = filter.into_inner().next().unwrap();
                    items_query.set_scan_filter(Self::build_scan_filter(filter_scan))?;
                },
                Rule::filter_date => {
                    let filter_date = filter.into_inner().next().unwrap();
                    items_query.set_date_filter(Self::build_date_filter(filter_date))?;
                },
                Rule::filter_change => {
                    // Process the change filter.
                    let mut change_values = filter.into_inner();
                    let change_value = change_values.next().unwrap().as_str();
                    match items_query.get_change_filter_mut() {
                        Some(cf) => {
                            cf.set_include_type(change_value)?;
                        },
                        None => {
                            let mut change_filter = ChangeFilter::default();
                            change_filter.set_include_type(change_value)?;
                            items_query.set_change_filter(change_filter)?;
                        }
                    }
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