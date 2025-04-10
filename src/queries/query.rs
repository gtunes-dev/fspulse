//use std::io::{self, Stdout};

use log::{error, info};
use pest::{iterators::{Pair, Pairs}, Parser};
use rusqlite::{Row, Statement, ToSql};
use tabled::{settings::{object::Rows, Alignment, Style}, Table, Tabled};
//use tablestream::{Column, Stream};

use crate::{database::Database, error::FsPulseError};

use super::{filters::{ChangeFilter, DateFilter, Filter, IdFilter}, order::Order, QueryParser, Rule};

#[derive(Debug)]
enum QueryType {
    Roots,
    Scans,
    Changes,
}

pub struct Query;

#[derive(Debug)]
struct DomainQuery {
    query_type: QueryType,

    order_cols: &'static [&'static str],
    
    filters: Vec<Box<dyn Filter>>,
    order: Option<Order>,
    limit: Option<i64>,
}

impl DomainQuery {
    const CHANGES_BASE_SQL: &str  = 
        "SELECT
            changes.id as change_id,
            items.root_id as root_id,
            scan_id,
            item_id,
            change_type,
            metadata_changed,
            hash_changed,
            validity_changed,
            validity_state_old,
            validity_state_new,
            items.path as path
        FROM changes
        JOIN items
            ON changes.item_id = items.id";

    const ROOTS_BASE_SQL: &str = "
        SELECT
            id as root_id,
            path
        FROM roots";

    const SCANS_BASE_SQL: &str = "
        SELECT
            id as scan_id,
            root_id,
            state,
            hashing,
            validating,
            time_of_scan,
            file_count,
            folder_count
        FROM scans";

    fn new(query_type: QueryType) -> Self {
        let order_cols = match query_type {
            QueryType::Changes => Order::CHANGES_COLS,
            QueryType::Roots => Order::ROOTS_COLS,
            QueryType::Scans => Order::SCANS_COLS,
        };

        DomainQuery {
            query_type,
            order_cols,

            filters: Vec::new(),
            order: None,
            limit: None,
        }
    }

    fn add_filter<F>(&mut self, filter: F)
    where
        F: Filter + 'static
    {
        self.filters.push(Box::new(filter));
    }

    fn get_base_sql(&self) -> &str {
        match self.query_type {
            QueryType::Roots => Self::ROOTS_BASE_SQL,
            QueryType::Scans => Self::SCANS_BASE_SQL,
            QueryType::Changes => Self::CHANGES_BASE_SQL,
        }
    }

    fn execute_changes(&self, sql_statment: &mut Statement, sql_params: &[&dyn ToSql]) -> Result<Table, FsPulseError> {
        let rows = sql_statment.query_map(sql_params, |row| {
            ChangesQueryRow::from_row(row)
        })?;

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
        
        Ok(table)
    }

    fn execute_roots(&self, sql_statment: &mut Statement, sql_params: &[&dyn ToSql]) -> Result<Table, FsPulseError> {
        let rows = sql_statment.query_map(sql_params, |row| {
            RootsQueryRow::from_row(row)
        })?;

        let mut rows_rows = Vec::new();

        for row in rows {
            let roots_query_row: RootsQueryRow = row?;
            rows_rows.push(roots_query_row);
        }
        
        let mut table = Table::new(&rows_rows);
        table.with(Style::modern());
        table.modify(Rows::first(), Alignment::center());
        
        Ok(table)
    }    
    
    fn execute_scans(&self, sql_statment: &mut Statement, sql_params: &[&dyn ToSql]) -> Result<Table, FsPulseError> {
        let rows = sql_statment.query_map(sql_params, |row| {
            ScansQueryRow::from_row(row)
        })?;

        let mut rows_rows = Vec::new();

        for row in rows {
            let scans_query_row= row?;
            rows_rows.push(scans_query_row);
        }
        
        let mut table = Table::new(&rows_rows);
        table.with(Style::modern());
        table.modify(Rows::first(), Alignment::center());
        
        Ok(table)
    }

    fn execute(&self, db: &Database, _query: &str) -> Result<(), FsPulseError> {
        let mut sql = self.get_base_sql().to_string();
            
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

        let sql_params: Vec<&dyn ToSql> = params_vec.iter().map(|b| &**b).collect();

        let mut sql_statment = db.conn().prepare(&sql)?;

        let table = match self.query_type {
            QueryType::Roots => self.execute_roots(&mut sql_statment, &sql_params)?,
            QueryType::Scans => self.execute_scans(&mut sql_statment, &sql_params)?,
            QueryType::Changes => self.execute_changes(&mut sql_statment, &sql_params)?,
        };

        println!("{table}");

        Ok(())
    }
}

#[derive(Tabled)]
struct ChangesQueryRow {
    // changes properties
    #[tabled(rename = "change\nid")]
    change_id: i64,
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
                change_id: row.get(0)?, 
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

#[derive(Tabled)]
struct RootsQueryRow {
    root_id: i64,
    path: String,
}

impl RootsQueryRow {
    fn from_row(row: &Row) -> rusqlite::Result<Self> {
        Ok(
            RootsQueryRow { 
                root_id: row.get(0)?, 
                path: row.get(1)? 
            }
        )
    }
}

#[derive(Tabled)]
struct ScansQueryRow {
    scan_id: i64,
    root_id: i64,
    state: i64,
    hashing: bool,
    validating: bool,
    time_of_scan: bool,
    file_count: i64,
    folder_count: i64,
}

impl ScansQueryRow {
    fn from_row(row: &Row) -> rusqlite::Result<Self> {
        Ok(
            ScansQueryRow { 
                scan_id: row.get(0)?, 
                root_id: row.get(1)?, 
                state: row.get(2)?, 
                hashing: row.get(3)?, 
                validating: row.get(4)?, 
                time_of_scan: row.get(5)?, 
                file_count: row.get(6)?, 
                folder_count: row.get(7)?, 
            }
        )
    }
}

impl Query {
    pub fn process_query(db: &Database, query_str: &str) -> Result<(), FsPulseError> {
        info!("Preparing to execute query: {}", query_str);
        let mut parsed_query = Query::parse(Rule::query, query_str)?;
        info!("Parsed query: {}", parsed_query);

        let query_pair = parsed_query.next().unwrap();
        let mut query_children = query_pair.into_inner();

        let domain_query = query_children.next().unwrap();
        let query_type = match domain_query.as_rule() {
            Rule::roots_query => QueryType::Roots,
            Rule::scans_query => QueryType::Scans,
            Rule::changes_query => QueryType::Changes,
            _ => {
                return Err(FsPulseError::Error(format!("Unsupported query type:\n{}", query_str)));
            }
        };

        let query = Query::build(query_type, domain_query)?;
        query.execute(db, query_str)?;

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

    fn build(query_type: QueryType, domain_query: Pair<Rule>) -> Result<DomainQuery, FsPulseError> {
        let mut query = DomainQuery::new(query_type);

        // iterate over the children of changes_query
        for token in domain_query.into_inner() {
            match token.as_rule() {
                Rule::root_id_filter | 
                Rule::scan_id_filter |
                Rule::item_id_filter |
                Rule::change_id_filter => {
                    let id_filter = IdFilter::build(token)?;
                    query.add_filter(id_filter);
                },
                Rule::scan_date_filter |
                Rule::mod_date_filter => {
                    let date_filter = DateFilter::build(token)?;
                    query.add_filter(date_filter);
                },
                Rule::change_filter => {
                    let change_filter = ChangeFilter::build(token)?;
                    query.add_filter(change_filter);
                },
                Rule::order_list => {
                    let order = Order::build(token, query.order_cols)?;
                    query.order = Some(order);
                },
                Rule::limit_val => {
                    query.limit = Some(token.as_str().parse().unwrap());
                }
                _ => {}
            }
        }

        Ok(query)
    }
}