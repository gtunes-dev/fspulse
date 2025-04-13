use log::{error, info};
use pest::{iterators::Pair, Parser};
use rusqlite::{Row, Statement, ToSql};
use tabled::{
    settings::{object::Rows, Alignment, Style},
    Table, Tabled,
};
//use tablestream::{Column, Stream};

use crate::utils::Utils;
use crate::{database::Database, error::FsPulseError};

use super::{
    columns::ColumnSet,
    filters::{ChangeFilter, DateFilter, Filter, IdFilter, PathFilter},
    order::Order,
    QueryParser, Rule,
};

#[derive(Debug, Copy, Clone)]
pub enum QueryType {
    Roots,
    Scans,
    Items,
    Changes,
}

pub struct Query;

#[derive(Debug)]
struct DomainQuery {
    query_type: QueryType,

    col_set: ColumnSet,

    filters: Vec<Box<dyn Filter>>,
    order: Option<Order>,
    limit: Option<i64>,
}

impl DomainQuery {
    const ROOTS_BASE_SQL: &str = "\nFROM roots";

    const SCANS_BASE_SQL: &str = "\nFROM scans";

    const ITEMS_BASE_SQL: &str = "\nFROM items";

    const CHANGES_BASE_SQL: &str = "\nFROM changes
        JOIN items
            ON changes.item_id = items.item_id";

    fn new(query_type: QueryType) -> Self {
        DomainQuery {
            query_type,

            col_set: ColumnSet::for_query_type(query_type),

            filters: Vec::new(),
            order: None,
            limit: None,
        }
    }

    fn add_filter<F>(&mut self, filter: F)
    where
        F: Filter + 'static,
    {
        self.filters.push(Box::new(filter));
    }

    fn get_base_sql(&self) -> &str {
        match self.query_type {
            QueryType::Roots => Self::ROOTS_BASE_SQL,
            QueryType::Scans => Self::SCANS_BASE_SQL,
            QueryType::Items => Self::ITEMS_BASE_SQL,
            QueryType::Changes => Self::CHANGES_BASE_SQL,
        }
    }

    fn execute_roots(
        &self,
        sql_statment: &mut Statement,
        sql_params: &[&dyn ToSql],
    ) -> Result<Table, FsPulseError> {
        let rows = sql_statment.query_map(sql_params, RootsQueryRow::from_row)?;

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

    fn execute_scans(
        &self,
        sql_statment: &mut Statement,
        sql_params: &[&dyn ToSql],
    ) -> Result<Table, FsPulseError> {
        let rows = sql_statment.query_map(sql_params, ScansQueryRow::from_row)?;

        let mut rows_rows = Vec::new();

        for row in rows {
            let scans_query_row = row?;
            rows_rows.push(scans_query_row);
        }

        let mut table = Table::new(&rows_rows);
        table.with(Style::modern());
        table.modify(Rows::first(), Alignment::center());

        Ok(table)
    }

    fn execute_items(
        &self,
        sql_statment: &mut Statement,
        sql_params: &[&dyn ToSql],
    ) -> Result<Table, FsPulseError> {
        let rows = sql_statment.query_map(sql_params, ItemsQueryRow::from_row)?;

        let mut rows_rows = Vec::new();

        for row in rows {
            let items_query_row = row?;
            rows_rows.push(items_query_row);
        }

        let mut table = Table::new(&rows_rows);
        table.with(Style::modern());
        table.modify(Rows::first(), Alignment::center());

        Ok(table)
    }

    fn execute_changes(
        &self,
        sql_statment: &mut Statement,
        sql_params: &[&dyn ToSql],
    ) -> Result<Table, FsPulseError> {
        let rows = sql_statment.query_map(sql_params, ChangesQueryRow::from_row)?;

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

    fn execute(
        &self,
        db: &Database,
        query_type: QueryType,
        _query: &str,
    ) -> Result<(), FsPulseError> {
        let mut sql = format!("{}{}", self.col_set.as_select(), self.get_base_sql(),);

        // $TODO: Wrap Filters into a struct that can generate the entire WHERE clause
        let mut params_vec: Vec<Box<dyn ToSql>> = Vec::new();

        if !self.filters.is_empty() {
            let mut first = true;
            sql.push_str("\nWHERE ");
            for filter in &self.filters {
                match first {
                    true => {
                        first = false;
                    }
                    false => {
                        sql.push_str(" AND ");
                    }
                }
                let (pred_str, pred_vec) = filter.to_predicate_parts(query_type)?;
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
            QueryType::Items => self.execute_items(&mut sql_statment, &sql_params)?,
            QueryType::Changes => self.execute_changes(&mut sql_statment, &sql_params)?,
        };

        println!("{table}");

        Ok(())
    }
}

#[derive(Tabled)]
struct RootsQueryRow {
    root_id: i64,
    root_path: String,
}

impl RootsQueryRow {
    fn from_row(row: &Row) -> rusqlite::Result<Self> {
        Ok(RootsQueryRow {
            root_id: row.get(0)?,
            root_path: row.get(1)?,
        })
    }
}

#[derive(Tabled)]
struct ScansQueryRow {
    scan_id: i64,
    root_id: i64,
    state: i64,
    hashing: bool,
    validating: bool,
    #[tabled(display = "Utils::display_db_time")]
    scan_time: i64,
    file_count: i64,
    folder_count: i64,
}

impl ScansQueryRow {
    fn from_row(row: &Row) -> rusqlite::Result<Self> {
        Ok(ScansQueryRow {
            scan_id: row.get(0)?,
            root_id: row.get(1)?,
            state: row.get(2)?,
            hashing: row.get(3)?,
            validating: row.get(4)?,
            scan_time: row.get(5)?,
            file_count: row.get(6)?,
            folder_count: row.get(7)?,
        })
    }
}

#[derive(Tabled)]
struct ItemsQueryRow {
    item_id: i64,
    root_id: i64,
    #[tabled(display = "Utils::display_short_path")]
    item_path: String,
    item_type: String,
    last_scan: i64,
    #[tabled(display = "Utils::display_bool")]
    is_ts: bool,
    #[tabled(display = "Utils::display_opt_db_time")]
    mod_date: Option<i64>,
    #[tabled(display = "Utils::display_opt_i64")]
    file_size: Option<i64>,
    #[tabled(display = "Utils::display_opt_i64")]
    last_hash_scan: Option<i64>,
    #[tabled(display = "Utils::display_opt_i64")]
    last_val_scan: Option<i64>,
    val: String,
    #[tabled(display = "Utils::display_opt_str")]
    val_error: Option<String>,
}

impl ItemsQueryRow {
    fn from_row(row: &rusqlite::Row) -> rusqlite::Result<Self> {
        Ok(ItemsQueryRow {
            item_id: row.get(0)?,
            root_id: row.get(1)?,
            item_path: row.get(2)?,
            item_type: row.get(3)?,
            last_scan: row.get(4)?,
            is_ts: row.get(5)?,
            mod_date: row.get(6)?,
            file_size: row.get(7)?,
            last_hash_scan: row.get(8)?,
            last_val_scan: row.get(9)?,
            val: row.get(10)?,
            val_error: row.get(11)?,
        })
    }
}

#[derive(Tabled)]
struct ChangesQueryRow {
    // changes properties
    change_id: i64,
    root_id: i64,
    scan_id: i64,
    item_id: i64,
    change_type: String,
    #[tabled(display = "Utils::display_opt_bool")]
    meta_change: Option<bool>,
    #[tabled(display = "Utils::display_opt_db_time")]
    mod_date_old: Option<i64>,
    #[tabled(display = "Utils::display_opt_db_time")]
    mod_date_new: Option<i64>,
    #[tabled(display = "Utils::display_opt_bool")]
    hash_change: Option<bool>,
    #[tabled(display = "Utils::display_opt_bool")]
    val_change: Option<bool>,
    #[tabled(display = "Utils::display_opt_str")]
    val_old: Option<String>,
    #[tabled(display = "Utils::display_opt_str")]
    val_new: Option<String>,

    // items properties
    #[tabled(display = "Utils::display_short_path")]
    item_path: String,
}

impl ChangesQueryRow {
    fn from_row(row: &rusqlite::Row) -> rusqlite::Result<Self> {
        Ok(ChangesQueryRow {
            change_id: row.get(0)?,
            root_id: row.get(1)?,
            scan_id: row.get(2)?,
            item_id: row.get(3)?,
            change_type: row.get(4)?,
            meta_change: row.get(5)?,
            mod_date_old: row.get(6)?,
            mod_date_new: row.get(7)?,
            hash_change: row.get(8)?,
            val_change: row.get(9)?,
            val_old: row.get(10)?,
            val_new: row.get(11)?,
            item_path: row.get(12)?,
        })
    }
}

impl Query {
    pub fn process_query(db: &Database, query_str: &str) -> Result<(), FsPulseError> {
        info!("Parsing query: {}", query_str);
        let mut parsed_query = match QueryParser::parse(Rule::query, query_str) {
            Ok(parsed_query) => parsed_query,
            Err(err) => match err.variant {
                pest::error::ErrorVariant::ParsingError { .. } => {
                    error!("Query parsing error: {}", err);
                    println!("Query parsing error: {}", err);
                    return Ok(());
                }
                _ => {
                    return Err(Box::new(err).into());
                }
            },
        };

        info!("Parsed query: {}", parsed_query);

        let query_pair = parsed_query.next().unwrap();
        let mut query_children = query_pair.into_inner();

        let domain_query = query_children.next().unwrap();
        let query_type = match domain_query.as_rule() {
            Rule::roots_query => QueryType::Roots,
            Rule::scans_query => QueryType::Scans,
            Rule::items_query => QueryType::Items,
            Rule::changes_query => QueryType::Changes,
            _ => {
                return Err(FsPulseError::Error(format!(
                    "Unsupported query type:\n{}",
                    query_str
                )));
            }
        };

        let query = Query::build(query_type, domain_query)?;
        query.execute(db, query_type, query_str)?;

        Ok(())
    }

    fn build(query_type: QueryType, domain_query: Pair<Rule>) -> Result<DomainQuery, FsPulseError> {
        let mut query = DomainQuery::new(query_type);

        // iterate over the children of changes_query
        for token in domain_query.into_inner() {
            match token.as_rule() {
                Rule::root_id_filter
                | Rule::scan_id_filter
                | Rule::item_id_filter
                | Rule::change_id_filter => {
                    let id_filter = IdFilter::build(token)?;
                    query.add_filter(id_filter);
                }
                Rule::scan_time_filter
                | Rule::mod_date_filter
                | Rule::mod_date_old_filter
                | Rule::mod_date_new_filter => {
                    let date_filter = DateFilter::build(token)?;
                    query.add_filter(date_filter);
                }
                Rule::change_filter => {
                    let change_filter = ChangeFilter::build(token)?;
                    query.add_filter(change_filter);
                }
                Rule::order_list => {
                    let order = Order::build(token, query.col_set)?;
                    query.order = Some(order);
                }
                Rule::limit_val => {
                    query.limit = Some(token.as_str().parse().unwrap());
                }
                Rule::root_path_filter
                | Rule::item_path_filter => {
                    let path_filter = PathFilter::build(token)?;
                    query.add_filter(path_filter);
                }
                _ => {}
            }
        }

        Ok(query)
    }
}
