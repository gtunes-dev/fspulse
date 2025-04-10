//use std::io::{self, Stdout};

use log::{error, info};
use pest::{iterators::{Pair, Pairs}, Parser};
use rusqlite::ToSql;
use tabled::{settings::{object::Rows, Alignment, Style}, Table, Tabled};
//use tablestream::{Column, Stream};

use crate::{database::Database, error::FsPulseError};

use super::{filters::{ChangeFilter, DateFilter, Filter, RootFilter, ScanFilter}, order::Order, QueryParser, Rule};


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
                Rule::scan_filter => {
                    let filter_scan = filter.into_inner().next().unwrap();
                    items_query.set_scan_filter(ScanFilter::build(filter_scan)?)?;
                },
                Rule::date_filter => {
                    let filter_date = filter.into_inner().next().unwrap();
                    items_query.set_date_filter(DateFilter::build(filter_date))?;
                },
                Rule::change_filter => {
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
                Rule::validity_filter => {
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
    order: Option<Order>,
    limit: Option<i64>,
}

#[derive(Tabled)]
struct ChangesQueryRow {
    // changes properties
    #[tabled(rename = "change\nid")]
    id: i64,
    #[tabled(rename = "root\nid")]
    root_id: i64,
    #[tabled(rename = "scan\nid")]
    scan_id: i64,
    #[tabled(rename = "item\nid")]
    item_id: i64,
    #[tabled(rename = "change\ntype")]
    change_type: String,
    #[tabled(rename = "meta\nchange", display = "ChangesQueryRow::display_opt_bool")]
    metadata_changed: Option<bool>,
    #[tabled(rename = "hash\nchange", display = "ChangesQueryRow::display_opt_bool")]
    hash_changed: Option<bool>,
    #[tabled(rename = "valid\nchange", display = "ChangesQueryRow::display_opt_bool")]
    validity_changed: Option<bool>,
    #[tabled(rename = "old\nvalid", display = "ChangesQueryRow::display_opt_string")]
    validity_state_old: Option<String>,
    #[tabled(rename = "new\nvalid", display = "ChangesQueryRow::display_opt_string")]
    validity_state_new: Option<String>,

    // items properties
    path: String,
}

impl ChangesQueryRow {
    const COLUMNS: &str = 
        "changes.id as change_id,
        items.root_id as root_id,
        scan_id,
        item_id,
        change_type,
        metadata_changed,
        hash_changed,
        validity_changed,
        validity_state_old,
        validity_state_new,
        items.path as path";

    pub fn display_opt_bool(opt_bool: &Option<bool>) -> String {
        match opt_bool {
            Some(true) => "T".into(),
            Some(false) => "F".into(),
            None => "-".into(),
        }
    }

    pub fn display_opt_string(opt_string: &Option<String>) -> String {
        match opt_string {
            Some(s) => s.into(),
            None => "-".into(),
        }
    }

    fn from_row(row: &rusqlite::Row) -> rusqlite::Result<Self> {
        Ok(
            ChangesQueryRow { 
                id: row.get(0)?, 
                root_id: row.get(1)?,
                scan_id: row.get(2)?, 
                item_id: row.get(3)?, 
                change_type: row.get(4)?, 
                metadata_changed: row.get(5)?, 
                hash_changed: row.get(6)?, 
                validity_changed: row.get(7)?,
                validity_state_old: row.get(8)?,
                validity_state_new: row.get(9)?,
                path: row.get(10)?,
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
                Rule::scan_filter => {
                    let scan_filter = ScanFilter::build(token)?;
                    changes_query.add_filter(scan_filter);
                },
                Rule::change_filter => {
                    let change_filter = ChangeFilter::build(token)?;
                    changes_query.add_filter(change_filter);
                },
                Rule::root_filter => {
                    let root_filter = RootFilter::build(token)?;
                    changes_query.add_filter(root_filter);
                },
                Rule::order_list => {
                    let order = Order::build(token, Order::CHANGE_COLS)?;
                    changes_query.order = Some(order);
                },
                Rule::limit_val => {
                    changes_query.limit = Some(token.as_str().parse().unwrap());
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

/*     fn begin_table(title: &str, empty_row: &str) -> Stream<ChangesQueryRow, Stdout> {
        Stream::new(io::stdout(), vec![
            Column::new(|f, c: &ChangesQueryRow| write!(f, "{}", c.id)).header("Id").right().min_width(6),
            Column::new(|f, c: &ChangesQueryRow| write!(f, "{}", c.scan_id)).header("Scan").right(),
            Column::new(|f, c: &ChangesQueryRow| write!(f, "{}", c.item_id)).header("Item").right(),
            Column::new(|f, c: &ChangesQueryRow| write!(f, "{}", Utils::opt_bool_or_none_as_str(c.metadata_changed))).header("Meta").center(),
            Column::new(|f, c: &ChangesQueryRow| write!(f, "{}", c.change_type)).header("Type").center(),
            Column::new(|f, c: &ChangesQueryRow| write!(f, "{}", c.path)).header("Path").left(),
        ]).title(title).empty_row(empty_row)
    } */

 /*    fn begin_comfy_table() -> Table {
        let mut table = Table::new();
        table
            .apply_modifier(UTF8_NO_BORDERS)
            .set_content_arrangement(ContentArrangement::DynamicFullWidth)
            .set_header(vec![
                "Id", "Scan", "Item", "Meta\nChanged"
            ]);
        table
    }
 */
    fn execute(&self, db: &Database, _query: &str) -> Result<(), FsPulseError> {
        let mut sql = format!(
            "SELECT {} 
            FROM changes
            JOIN items
                ON changes.item_id = items.id", 
            ChangesQueryRow::COLUMNS
        );
            //let table = Self::begin_comfy_table();
            //println!("{table}");
            
        // $TODO: Wrap Filters into a struct that can generate the entire WHERE clause
        let mut params_vec: Vec<Box<dyn ToSql>> = Vec::new();

        if !self.filters.is_empty() {
            let mut first = true;
            sql.push_str("\nWHERE ");
            for filter in &self.filters {
                match first {
                    true => {
                        first = false;
                    },
                    false => {
                        sql.push_str(" AND");
                    }
                }
                let (pred_str, pred_vec) = filter.to_predicate_parts()?;
                sql.push_str(&pred_str);
                params_vec.extend(pred_vec);
            }
        }

        if let Some(order) = &self.order {
            let order_clause = order.to_order_clause();
            sql.push_str(&order_clause);
        }

        if let Some(limit) = &self.limit {
            sql.push_str(&format!("\nLIMIT {}", limit));
        }

        let param_refs: Vec<&dyn ToSql> = params_vec.iter().map(|b| &**b).collect();

        let mut stmt = db.conn().prepare(&sql)?;
        let rows = stmt.query_map(&param_refs[..], |row| {
            ChangesQueryRow::from_row(row)
        })?;

        //let mut table = Self::begin_table(query, "No Changes");

        let mut changes_rows = Vec::new();

        for row in rows {
            let changes_query_row: ChangesQueryRow = row?;
            changes_rows.push(changes_query_row);
            //table.row(changes_query_row)?;
            //println!("id: {}, item_id: {}", changes_query_row.id, changes_query_row.item_id);
        }
        
        let mut table = Table::new(&changes_rows);
        table.with(Style::modern());
        table.modify(Rows::first(), Alignment::center());
        println!("{table}");

        //table.finish()?;

        Ok(())
    }
}

pub struct Query;

impl Query {
    pub fn process_query(db: &Database, query: &str) -> Result<(), FsPulseError> {
        // for testing during coding
        //let query = "changes where scan:(1) order scan_id asc, id desc limit 10";
        info!("Preparing to execute query: {}", query);
        let mut parsed_query = Query::parse(Rule::query, query)?;
        info!("Parsed query: {}", parsed_query);

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