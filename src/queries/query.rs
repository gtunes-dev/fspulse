use log::{error, info};
use pest::{iterators::Pairs, Parser};
use rusqlite::{Row, Statement, ToSql};
use tabled::{
    builder::Builder,
    settings::{object::Rows, Alignment, Style},
    Table, Tabled,
};

use super::{
    columns::{ColSet, CHANGES_QUERY_COLS, ITEMS_QUERY_COLS, ROOTS_QUERY_COLS, SCANS_QUERY_COLS},
    show::{ScansQueryRow, Show},
};
//use tablestream::{Column, Stream};

use crate::{database::Database, error::FsPulseError, utils::Utils};

use super::{
    filter::{DateFilter, Filter, IdFilter, PathFilter, StringFilter},
    order::Order,
    QueryParser, Rule,
};

#[derive(Debug, Copy, Clone, PartialEq)]
pub enum QueryType {
    Roots,
    Scans,
    Items,
    Changes,
}

impl QueryType {
    fn from_str(query_type: &str) -> Self {
        match query_type {
            "roots" => QueryType::Roots,
            "scans" => QueryType::Scans,
            "items" => QueryType::Items,
            "changes" => QueryType::Changes,
            _ => unreachable!(),
        }
    }
}

pub struct Query;

#[derive(Debug)]
pub struct DomainQuery {
    pub query_type: QueryType,
    pub col_set: ColSet,

    filters: Vec<Box<dyn Filter>>,
    show: Show,
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

            col_set: Self::col_set_for_query_type(query_type),

            filters: Vec::new(),
            show: Show::new(),
            order: None,
            limit: None,
        }
    }

    fn col_set_for_query_type(query_type: QueryType) -> ColSet {
        let query_cols = match query_type {
            QueryType::Roots => &ROOTS_QUERY_COLS,
            QueryType::Items => &ITEMS_QUERY_COLS,
            QueryType::Scans => &SCANS_QUERY_COLS,
            QueryType::Changes => &CHANGES_QUERY_COLS,
        };

        ColSet::new(query_cols)
    }

    pub fn add_filter<F>(&mut self, filter: F)
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
        builder: &mut Builder,
    ) -> Result<Table, FsPulseError> {
        let rows = sql_statment.query_map(sql_params, ChangesQueryRow::from_row)?;

        let mut changes_rows = Vec::new();

        for row in rows {
            let changes_query_row: ChangesQueryRow = row?;
            self.show.append_changes_row(&changes_query_row, builder)?;

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
        println!("SQL: {sql}");

        let mut sql_statment = db.conn().prepare(&sql)?;

        // $TODO: shouldn't have to pass the query type here - can be stored in the show at
        // creation or we can use some other patterh
        let mut builder = self.show.as_builder();

        let table = match self.query_type {
            QueryType::Roots => self.execute_roots(&mut sql_statment, &sql_params)?,
            QueryType::Scans => self.execute_scans(&mut sql_statment, &sql_params)?,
            QueryType::Items => self.execute_items(&mut sql_statment, &sql_params)?,
            QueryType::Changes => {
                self.execute_changes(&mut sql_statment, &sql_params, &mut builder)?
            }
        };

        let qt = self.query_type;
        if qt == QueryType::Changes {
            let mut little_table = builder.build();
            self.show.set_column_aligments(&mut little_table);
            little_table.with(Style::modern());
            little_table.modify(Rows::first(), Alignment::center());

            //little_table.modify(Columns::single(0), Alignment::right());
            println!("{little_table}");

            println!();
            println!();
        } else {
            println!("{table}");
        }

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
pub struct ChangesQueryRow {
    // changes properties
    pub change_id: i64,
    pub root_id: i64,
    pub scan_id: i64,
    pub item_id: i64,
    pub change_type: String,
    #[tabled(display = "Utils::display_opt_bool")]
    pub meta_change: Option<bool>,
    #[tabled(display = "Utils::display_opt_db_time")]
    pub mod_date_old: Option<i64>,
    #[tabled(display = "Utils::display_opt_db_time")]
    pub mod_date_new: Option<i64>,
    #[tabled(display = "Utils::display_opt_bool")]
    pub hash_change: Option<bool>,
    #[tabled(display = "Utils::display_opt_bool")]
    pub val_change: Option<bool>,
    #[tabled(display = "Utils::display_opt_str")]
    pub val_old: Option<String>,
    #[tabled(display = "Utils::display_opt_str")]
    pub val_new: Option<String>,

    // items properties
    #[tabled(display = "Utils::display_short_path")]
    pub item_path: String,
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
                    println!("{}", err);
                    return Ok(());
                }
                _ => {
                    return Err(Box::new(err).into());
                }
            },
        };

        info!("Parsed query: {}", parsed_query);

        let query_pair = parsed_query.next().unwrap();
        let mut query_iter = query_pair.into_inner();

        let query_type_pair = query_iter.next().unwrap();
        let query_type = QueryType::from_str(query_type_pair.as_str());
        let mut query = DomainQuery::new(query_type);

        let res = Query::build(&mut query, &mut query_iter);
        match res {
            Ok(()) => {}
            Err(err) => match err {
                FsPulseError::CustomParsingError(ref msg) => {
                    info!("Query parsing error: {}", msg);
                    println!("Query parsing error: {}", msg);
                    return Ok(());
                }
                other_error => {
                    return Err(other_error);
                }
            },
        };

        query.execute(db, query_type, query_str)?;

        Ok(())
    }

    fn build(query: &mut DomainQuery, query_iter: &mut Pairs<Rule>) -> Result<(), FsPulseError> {
        for token in query_iter {
            println!("{:?}", token.as_rule());
            match token.as_rule() {
                Rule::id_filter => {
                    IdFilter::add_to_query(token, query)?;
                }
                Rule::date_filter => {
                    DateFilter::add_to_query(token, query)?;
                }
                Rule::bool_filter => {
                    StringFilter::add_bool_filter_to_query(token, query)?;
                }
                Rule::val_filter => {
                    StringFilter::add_val_filter_to_query(token, query)?;
                }
                Rule::item_type_filter => {
                    StringFilter::add_item_type_filter_to_query(token, query)?;
                }
                Rule::change_type_filter => {
                    StringFilter::add_change_type_filter_to_query(token, query)?;
                }
                Rule::path_filter => {
                    PathFilter::add_to_query(token, query)?;
                }
                Rule::show_list => {
                    query.show.build_from_pest_pair(token, query.col_set)?;
                }
                Rule::order_list => {
                    let order = Order::from_pest_pair(token, query.col_set)?;
                    query.order = Some(order);
                }
                Rule::limit_val => {
                    query.limit = Some(token.as_str().parse().unwrap());
                }
                _ => {}
            }
        }

        Ok(())
    }
}
