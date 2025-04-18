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
    show::{Format, Show},
};
//use tablestream::{Column, Stream};

use crate::{database::Database, error::FsPulseError, utils::Utils};

use super::{
    filter::{DateFilter, Filter, IdFilter, PathFilter, StringFilter},
    order::Order,
    QueryParser, Rule,
};

/// Defines the behavior of a validator.
pub trait Query {
    fn query_impl(&self) -> &QueryImpl;
    fn query_impl_mut(&mut self) -> &mut QueryImpl;

    fn col_set(&self) -> &ColSet {
        &self.query_impl().col_set
    }

    fn show(&self) -> &Show {
        &self.query_impl().show
    }
    fn show_mut(&mut self) -> &mut Show {
        &mut self.query_impl_mut().show
    }

    fn order(&self) -> &Option<Order> {
        &self.query_impl().order
    }
    fn set_order(&mut self, order: Option<Order>) {
        self.query_impl_mut().order = order
    }

    fn add_filter(&mut self, filter: Box<dyn Filter>) {
        self.query_impl_mut().filters.push(filter);
    }

    fn cols_as_select(&self) -> String {
        let mut sql = "SELECT ".to_string();

        let mut first = true;

        for col_spec in self.query_impl().col_set.values() {
            match first {
                true => first = false,
                false => sql.push_str(", "),
            }
            sql.push_str(col_spec.name_db);
        }

        sql
    }

    fn build_query_table(
        &mut self,
        sql_statement: &mut Statement,
        sql_params: &[&dyn ToSql],
    ) -> Result<Table, FsPulseError>;

    fn prepare_and_execute(&mut self, db: &Database) -> Result<(), FsPulseError> {
        let mut sql = format!("{}{}", self.cols_as_select(), self.query_impl().base_sql);

        // $TODO: Wrap Filters into a struct that can generate the entire WHERE clause
        let mut params_vec: Vec<Box<dyn ToSql>> = Vec::new();

        if !self.query_impl().filters.is_empty() {
            let mut first = true;
            sql.push_str("\nWHERE ");
            for filter in &self.query_impl().filters {
                match first {
                    true => {
                        first = false;
                    }
                    false => {
                        sql.push_str(" AND ");
                    }
                }
                let (pred_str, pred_vec) = filter.to_predicate_parts()?;
                sql.push_str(&pred_str);
                params_vec.extend(pred_vec);
            }
        }

        if let Some(order) = self.order() {
            let order_clause = order.to_order_clause();
            sql.push_str(&order_clause);
        }

        if let Some(limit) = &self.query_impl().limit {
            sql.push_str(&format!("\nLIMIT {}", limit));
        }

        let sql_params: Vec<&dyn ToSql> = params_vec.iter().map(|b| &**b).collect();
        println!("SQL: {sql}");

        let mut sql_statement = db.conn().prepare(&sql)?;

        //let mut builder = self.query_impl_mut().show.make_builder();

        let mut table = self.build_query_table(&mut sql_statement, &sql_params)?;

        // new table strategy
        //let mut new_table = builder.build();
        self.query_impl().show.set_column_aligments(&mut table);
        table.with(Style::modern());
        table.modify(Rows::first(), Alignment::center());

        println!("{table}");

        Ok(())
    }
}

fn make_query(query_type: &str) -> Box<dyn Query> {
    match query_type {
        "roots" => Box::new(RootsQuery {
            imp: QueryImpl::new(QueryImpl::ROOTS_BASE_SQL, ColSet::new(&ROOTS_QUERY_COLS)),
        }),
        "scans" => Box::new(ScansQuery {
            imp: QueryImpl::new(QueryImpl::SCANS_BASE_SQL, ColSet::new(&SCANS_QUERY_COLS)),
        }),
        "items" => Box::new(ItemsQuery {
            imp: QueryImpl::new(QueryImpl::ITEMS_BASE_SQL, ColSet::new(&ITEMS_QUERY_COLS)),
        }),
        "changes" => Box::new(ChangesQuery {
            imp: QueryImpl::new(
                QueryImpl::CHANGES_BASE_SQL,
                ColSet::new(&CHANGES_QUERY_COLS),
            ),
        }),
        _ => unreachable!(),
    }
}

impl Query for RootsQuery {
    fn query_impl(&self) -> &QueryImpl {
        &self.imp
    }
    fn query_impl_mut(&mut self) -> &mut QueryImpl {
        &mut self.imp
    }

    fn build_query_table(
        &mut self,
        sql_statement: &mut Statement,
        sql_params: &[&dyn ToSql],
    ) -> Result<Table, FsPulseError> {
        let rows = sql_statement.query_map(sql_params, RootsQueryRow::from_row)?;
        let mut builder = self.query_impl_mut().show.make_builder();

        for row in rows {
            let roots_query_row: RootsQueryRow = row?;
            self.append_roots_row(&roots_query_row, &mut builder)?;
        }

        let table = builder.build();

        Ok(table)
    }
}
struct RootsQuery {
    imp: QueryImpl,
}

impl RootsQuery {
    pub fn append_roots_row(
        &self,
        root: &RootsQueryRow,
        builder: &mut Builder,
    ) -> Result<(), FsPulseError> {
        let mut row: Vec<String> = Vec::new();

        for col in &self.show().display_cols {
            let col_string = match col.display_col {
                "root_id" => Format::format_i64(root.root_id),
                "root_path" => Format::format_path(&root.root_path, col.format)?,
                _ => {
                    return Err(FsPulseError::Error("Invalid column".into()));
                }
            };

            row.push(col_string);
        }

        builder.push_record(row);

        Ok(())
    }
}

impl Query for ItemsQuery {
    fn query_impl(&self) -> &QueryImpl {
        &self.imp
    }
    fn query_impl_mut(&mut self) -> &mut QueryImpl {
        &mut self.imp
    }

    fn build_query_table(
        &mut self,
        sql_statment: &mut Statement,
        sql_params: &[&dyn ToSql],
    ) -> Result<Table, FsPulseError> {
        let rows = sql_statment.query_map(sql_params, ItemsQueryRow::from_row)?;
        let mut builder: Builder = self.query_impl_mut().show.make_builder();

        for row in rows {
            let items_query_row = row?;
            self.append_items_row(&items_query_row, &mut builder)?;
        }

        let table = builder.build();

        Ok(table)
    }
}

struct ItemsQuery {
    imp: QueryImpl,
}

impl ItemsQuery {
    pub fn append_items_row(
        &self,
        item: &ItemsQueryRow,
        builder: &mut Builder,
    ) -> Result<(), FsPulseError> {
        let mut row: Vec<String> = Vec::new();

        for col in &self.show().display_cols {
            let col_string = match col.display_col {
                "item_id" => Format::format_i64(item.item_id),
                "root_id" => Format::format_i64(item.root_id),
                "item_path" => Format::format_path(&item.item_path, col.format)?,
                "item_type" => Format::format_item_type(&item.item_type, col.format)?,
                "last_scan" => Format::format_i64(item.last_scan),
                "is_ts" => Format::format_bool(item.is_ts, col.format)?,
                "mod_date" => Format::format_opt_date(item.mod_date, col.format)?,
                "file_size" => Format::format_opt_i64(item.file_size),
                "file_hash" => Format::format_opt_string(&item.file_hash),
                "last_hash_scan" => Format::format_opt_i64(item.last_hash_scan),
                "last_val_scan" => Format::format_opt_i64(item.last_val_scan),
                "val" => Format::format_val(&item.val, col.format)?,
                "val_error" => Format::format_opt_string(&item.val_error),
                _ => {
                    return Err(FsPulseError::Error("Invalid column".into()));
                }
            };

            row.push(col_string);
        }

        builder.push_record(row);

        Ok(())
    }
}

impl Query for ScansQuery {
    fn query_impl(&self) -> &QueryImpl {
        &self.imp
    }
    fn query_impl_mut(&mut self) -> &mut QueryImpl {
        &mut self.imp
    }

    fn build_query_table(
        &mut self,
        sql_statment: &mut Statement,
        sql_params: &[&dyn ToSql],
    ) -> Result<Table, FsPulseError> {
        let rows = sql_statment.query_map(sql_params, ScansQueryRow::from_row)?;
        let mut builder = self.query_impl_mut().show.make_builder();

        for row in rows {
            let scans_query_row = row?;
            self.append_scans_row(&scans_query_row, &mut builder)?;
        }

        let table = builder.build();

        Ok(table)
    }
}
struct ScansQuery {
    imp: QueryImpl,
}

impl ScansQuery {
    pub fn append_scans_row(
        &self,
        scan: &ScansQueryRow,
        builder: &mut Builder,
    ) -> Result<(), FsPulseError> {
        let mut row: Vec<String> = Vec::new();

        for col in &self.show().display_cols {
            let col_string = match col.display_col {
                "scan_id" => Format::format_i64(scan.scan_id),
                "root_id" => Format::format_i64(scan.root_id),
                "state" => Format::format_i64(scan.state),
                "hashing" => Format::format_bool(scan.hashing, col.format)?,
                "validating" => Format::format_bool(scan.validating, col.format)?,
                "scan_time" => Format::format_date(scan.scan_time, col.format)?,
                "file_count" => Format::format_i64(scan.file_count),
                "folder_count" => Format::format_i64(scan.folder_count),
                _ => {
                    return Err(FsPulseError::Error("Invalid column".into()));
                }
            };

            row.push(col_string);
        }

        builder.push_record(row);

        Ok(())
    }
}

impl Query for ChangesQuery {
    fn query_impl(&self) -> &QueryImpl {
        &self.imp
    }
    fn query_impl_mut(&mut self) -> &mut QueryImpl {
        &mut self.imp
    }

    fn build_query_table(
        &mut self,
        sql_statment: &mut Statement,
        sql_params: &[&dyn ToSql],
    ) -> Result<Table, FsPulseError> {
        let rows = sql_statment.query_map(sql_params, ChangesQueryRow::from_row)?;
        let mut builder: Builder = self.query_impl_mut().show.make_builder();

        for row in rows {
            let changes_query_row: ChangesQueryRow = row?;

            self.append_changes_row(&changes_query_row, &mut builder)?;
        }
        let table = builder.build();

        Ok(table)
    }
}
struct ChangesQuery {
    imp: QueryImpl,
}

impl ChangesQuery {
    pub fn append_changes_row(
        &self,
        change: &ChangesQueryRow,
        builder: &mut Builder,
    ) -> Result<(), FsPulseError> {
        let mut row: Vec<String> = Vec::new();

        for col in &self.show().display_cols {
            //for col in &self.impl.display_cols {
            let col_string = match col.display_col {
                "change_id" => Format::format_i64(change.change_id),
                "root_id" => Format::format_i64(change.root_id),
                "scan_id" => Format::format_i64(change.scan_id),
                "item_id" => Format::format_i64(change.item_id),
                "change_type" => Format::format_change_type(&change.change_type, col.format)?,
                "meta_change" => Format::format_opt_bool(change.meta_change, col.format)?,
                "mod_date_old" => Format::format_opt_date(change.mod_date_old, col.format)?,
                "mod_date_new" => Format::format_opt_date(change.mod_date_new, col.format)?,
                "hash_change" => Format::format_opt_bool(change.hash_change, col.format)?,
                "val_change" => Format::format_opt_bool(change.val_change, col.format)?,
                "val_old" => Format::format_opt_val(change.val_old.as_deref(), col.format)?,
                "val_new" => Format::format_opt_val(change.val_new.as_deref(), col.format)?,
                _ => {
                    return Err(FsPulseError::Error("Invalid column".into()));
                }
            };

            row.push(col_string);
        }

        builder.push_record(row);

        Ok(())
    }
}

pub struct QueryProcessor;

#[derive(Debug)]
pub struct QueryImpl {
    base_sql: &'static str,
    col_set: ColSet,

    filters: Vec<Box<dyn Filter>>,
    show: Show,
    order: Option<Order>,
    limit: Option<i64>,
}

impl QueryImpl {
    pub const ROOTS_BASE_SQL: &str = "\nFROM roots";
    const SCANS_BASE_SQL: &str = "\nFROM scans";
    const ITEMS_BASE_SQL: &str = "\nFROM items";
    const CHANGES_BASE_SQL: &str = "\nFROM changes
        JOIN items
            ON changes.item_id = items.item_id";

    fn new(base_sql: &'static str, col_set: ColSet) -> Self {
        QueryImpl {
            base_sql,
            col_set,

            filters: Vec::new(),
            show: Show::new(col_set),
            order: None,
            limit: None,
        }
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

struct ItemsQueryRow {
    item_id: i64,
    root_id: i64,
    item_path: String,
    item_type: String,
    last_scan: i64,
    is_ts: bool,
    mod_date: Option<i64>,
    file_size: Option<i64>,
    file_hash: Option<String>,
    last_hash_scan: Option<i64>,
    last_val_scan: Option<i64>,
    val: String,
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
            file_hash: row.get(8)?,
            last_hash_scan: row.get(9)?,
            last_val_scan: row.get(10)?,
            val: row.get(11)?,
            val_error: row.get(12)?,
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

#[derive(Tabled)]
pub struct ScansQueryRow {
    scan_id: i64,
    root_id: i64,
    state: i64,
    #[tabled(display = "Utils::display_bool")]
    hashing: bool,
    #[tabled(display = "Utils::display_bool")]
    validating: bool,
    #[tabled(display = "Utils::display_db_time")]
    scan_time: i64,
    file_count: i64,
    folder_count: i64,
}

impl ScansQueryRow {
    pub fn from_row(row: &Row) -> rusqlite::Result<Self> {
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

impl QueryProcessor {
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
        //let query_type = QueryType::from_str(query_type_pair.as_str());

        let mut query = make_query(query_type_pair.as_str());

        let res = QueryProcessor::build(&mut *query, &mut query_iter);
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

        query.prepare_and_execute(db)?;

        Ok(())
    }

    fn build(query: &mut dyn Query, query_iter: &mut Pairs<Rule>) -> Result<(), FsPulseError> {
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
                Rule::string_filter => {
                    StringFilter::add_string_filter_to_query(token, query)?;
                }
                Rule::show_list => {
                    query.show_mut().build_from_pest_pair(token)?;
                }
                Rule::order_list => {
                    let order = Order::from_pest_pair(token, *query.col_set())?;
                    query.set_order(Some(order));
                }
                Rule::limit_val => {
                    query.query_impl_mut().limit = Some(token.as_str().parse().unwrap());
                }
                _ => {}
            }
        }

        Ok(())
    }
}
