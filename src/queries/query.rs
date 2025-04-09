use std::io::{self, Stdout};

use log::error;
use pest::{iterators::{Pair, Pairs}, Parser};
use rusqlite::ToSql;
use tablestream::{Column, Stream};

use crate::{database::Database, error::FsPulseError};

use super::{filters::{ChangeFilter, DateFilter, Filter, ScanFilter}, QueryParser, Rule};


#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ItemsQuery {
    scan_filter: Option<ScanFilter>,
    date_filter: Option<DateFilter>,
    change_filter: Option<ChangeFilter>,
}

impl ItemsQuery {
    pub fn build(items_query_pair: Pair<Rule>) -> Result<ItemsQuery, FsPulseError> {
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
                    items_query.set_scan_filter(ScanFilter::build(filter_scan)?)?;
                },
                Rule::filter_date => {
                    let filter_date = filter.into_inner().next().unwrap();
                    items_query.set_date_filter(DateFilter::build(filter_date))?;
                },
                Rule::filter_change => {
                    // Process the change filter.
                    /* 
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
                    */
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
}

#[derive(Debug, Default)]
pub struct ChangesQuery {
    filters: Vec<Box<dyn Filter>>,
}

struct ChangesQueryRow {
    id: i64,
    #[allow(dead_code)]
    scan_id: i64,
    item_id: i64,
    #[allow(dead_code)]
    change_type: String,
    #[allow(dead_code)]
    metadata_changed: Option<bool>,
    #[allow(dead_code)]
    hash_changed: Option<bool>,
    #[allow(dead_code)]
    validity_changed: Option<bool>,
    #[allow(dead_code)]
    validity_state_old: Option<String>,
    #[allow(dead_code)]
    validity_state_new: Option<String>,
}

impl ChangesQueryRow {
    const COLUMNS: &str = 
        "id,
        scan_id,
        item_id,
        change_type,
        metadata_changed,
        hash_changed,
        validity_changed,
        validity_state_old,
        validity_state_new";

    fn from_row(row: &rusqlite::Row) -> rusqlite::Result<Self> {
        Ok(
            ChangesQueryRow { 
                id: row.get(0)?, 
                scan_id: row.get(1)?, 
                item_id: row.get(2)?, 
                change_type: row.get(3)?, 
                metadata_changed: row.get(4)?, 
                hash_changed: row.get(5)?, 
                validity_changed: row.get(6)?,
                validity_state_old: row.get(7)?,
                validity_state_new: row.get(8)?,
            }
        )
    }
}

impl ChangesQuery {
    pub fn build(changes_query_token: Pair<Rule>) -> Result<ChangesQuery, FsPulseError> {
        let mut changes_query = ChangesQuery::default();

        // iterate over the children of changes_query
        for token in changes_query_token.into_inner() {
            match token.as_rule() {
                Rule::changes_where => {
                    // skip this...it's compound to deal with whitespace correctly,
                    // so can't be made silent
                },
                Rule::filter_scan => {
                    let scan_filter = ScanFilter::build(token)?;
                    changes_query.add_filter(scan_filter);
                },
                Rule::filter_change => {
                    let change_filter = ChangeFilter::build(token)?;
                    changes_query.add_filter(change_filter);
                }
                _ => {}
            }
        }

        Ok(changes_query)
    }

    fn add_filter<F>(&mut self, filter: F)
    where
        F: Filter + 'static
    {
        self.filters.push(Box::new(filter));
    }

    fn begin_table(title: &str, empty_row: &str) -> Stream<ChangesQueryRow, Stdout> {
        Stream::new(io::stdout(), vec![
            Column::new(|f, c: &ChangesQueryRow| write!(f, "{}", c.id)).header("Id").right().min_width(6),
            Column::new(|f, c: &ChangesQueryRow| write!(f, "{}", c.scan_id)).header("Scan Id").right(),
            Column::new(|f, c: &ChangesQueryRow| write!(f, "{}", c.item_id)).header("Item Id").right(),
            Column::new(|f, c: &ChangesQueryRow| write!(f, "{}", c.change_type)).header("Type").center(),
        ]).title(title).empty_row(empty_row)
    }

    fn execute(&self, db: &Database, query: &str) -> Result<(), FsPulseError> {
        let mut sql = format!("SELECT {} FROM changes WHERE", ChangesQueryRow::COLUMNS);

        let mut params_vec: Vec<&dyn ToSql> = Vec::new();

        for filter in &self.filters {
            let (pred_str, pred_vec) = filter.to_predicate_parts()?;
            sql.push_str(&pred_str);
            params_vec.extend(pred_vec);
        }

        let mut stmt = db.conn().prepare(&sql)?;
        let rows = stmt.query_map(&params_vec[..], |row| {
            ChangesQueryRow::from_row(row)
        })?;

        let mut table = Self::begin_table(query, "No Changes");

        for row in rows {
            let changes_query_row = row?;
            table.row(changes_query_row)?;
            //println!("id: {}, item_id: {}", changes_query_row.id, changes_query_row.item_id);
        }

        table.finish()?;

        Ok(())
    }
}

pub struct Query;

impl Query {
    pub fn process_query(db: &Database, _query: &str) -> Result<(), FsPulseError> {
        // for testing during coding
        let query = "changes where scan:(1, 2)";

        let mut parsed_query = Query::parse(Rule::query, query)?;
        println!("Parsed query: {}", parsed_query);

        let query_pair = parsed_query.next().unwrap();
        let domain_query_pairs = query_pair.into_inner();

        for pair in domain_query_pairs {
            match pair.as_rule() {
                Rule::items_query => {
                    let _items_query = ItemsQuery::build(pair)?;
                },
                Rule::scans_query => {
                    
                },
                Rule::roots_query => {

                },
                Rule::changes_query => {
                    let changes_query = ChangesQuery::build(pair)?;
                    changes_query.execute(db, query)?;
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
}