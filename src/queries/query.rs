use log::{error, info};
use pest::{iterators::Pairs, Parser};
use rusqlite::{Row, Statement, ToSql};
use tabled::{
    builder::Builder,
    settings::{object::Rows, Alignment, Style},
    Table,
};

use super::{
    columns::{ColSet, CHANGES_QUERY_COLS, ITEMS_QUERY_COLS, ROOTS_QUERY_COLS, SCANS_QUERY_COLS},
    filter::EnumFilter,
    show::{Format, Show},
};
//use tablestream::{Column, Stream};

use crate::{database::Database, error::FsPulseError};

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

    fn cols_as_select_list(&self) -> String {
        let mut select_list = String::new();

        let mut first = true;

        for col_spec in self.query_impl().col_set.values() {
            if col_spec.in_select_list {
                match first {
                    true => first = false,
                    false => select_list.push_str(", "),
                }
                select_list.push_str(col_spec.name_db);
            }
        }

        select_list
    }

    fn build_query_table(
        &mut self,
        sql_statement: &mut Statement,
        sql_params: &[&dyn ToSql],
    ) -> Result<Table, FsPulseError>;

    fn prepare_and_execute(&mut self, db: &Database) -> Result<(), FsPulseError> {
        let select_list = self.cols_as_select_list();

        // $TODO: Wrap Filters into a struct that can generate the entire WHERE clause
        let mut params_vec: Vec<Box<dyn ToSql>> = Vec::new();

        let mut where_clause = String::new();

        if !self.query_impl().filters.is_empty() {
            let mut first = true;
            where_clause.push_str("\nWHERE ");
            for filter in &self.query_impl().filters {
                match first {
                    true => {
                        first = false;
                    }
                    false => {
                        where_clause.push_str(" AND ");
                    }
                }
                let (pred_str, pred_vec) = filter.to_predicate_parts()?;
                where_clause.push_str(&pred_str);
                params_vec.extend(pred_vec);
            }
        }

        let order_clause = match self.order() {
            Some(order) => order.to_order_clause(),
            None => String::new(),
        };

        let limit_clause = match &self.query_impl().limit {
            Some(limit) => format!("\nLIMIT {}", limit),
            None => String::new(),
        };

        let sql = self
            .query_impl()
            .sql_template
            .replace("{select_list}", &select_list)
            .replace("{where_clause}", &where_clause)
            .replace("{order_clause}", &order_clause)
            .replace("{limit_clause}", &limit_clause);

        // println!("SQL: {sql}");

        let sql_params: Vec<&dyn ToSql> = params_vec.iter().map(|b| &**b).collect();

        let mut sql_statement = db.conn().prepare(&sql)?;

        let mut table = self.build_query_table(&mut sql_statement, &sql_params)?;

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
            imp: QueryImpl::new(QueryImpl::ROOTS_SQL_QUERY, ColSet::new(&ROOTS_QUERY_COLS)),
        }),
        "scans" => Box::new(ScansQuery {
            imp: QueryImpl::new(QueryImpl::SCANS_SQL_QUERY, ColSet::new(&SCANS_QUERY_COLS)),
        }),
        "items" => Box::new(ItemsQuery {
            imp: QueryImpl::new(QueryImpl::ITEMS_SQL_QUERY, ColSet::new(&ITEMS_QUERY_COLS)),
        }),
        "changes" => Box::new(ChangesQuery {
            imp: QueryImpl::new(
                QueryImpl::CHANGES_SQL_QUERY,
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
                "file_count" => Format::format_opt_i64(scan.file_count),
                "folder_count" => Format::format_opt_i64(scan.folder_count),
                "adds" => Format::format_i64(scan.adds),
                "modifies" => Format::format_i64(scan.modifies),
                "deletes" => Format::format_i64(scan.deletes),
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
                "item_path" => Format::format_path(&change.item_path, col.format)?,
                "change_type" => Format::format_change_type(&change.change_type, col.format)?,
                "meta_change" => Format::format_opt_bool(change.meta_change, col.format)?,
                "mod_date_old" => Format::format_opt_date(change.mod_date_old, col.format)?,
                "mod_date_new" => Format::format_opt_date(change.mod_date_new, col.format)?,
                "hash_change" => Format::format_opt_bool(change.hash_change, col.format)?,
                "val_change" => Format::format_opt_bool(change.val_change, col.format)?,
                "val_old" => Format::format_opt_val(change.val_old.as_deref(), col.format)?,
                "val_new" => Format::format_opt_val(change.val_new.as_deref(), col.format)?,
                "val_error_old" => Format::format_opt_string(&change.val_error_old),
                "val_error_new" => Format::format_opt_string(&change.val_error_new),
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
    sql_template: &'static str,
    col_set: ColSet,

    filters: Vec<Box<dyn Filter>>,
    show: Show,
    order: Option<Order>,
    limit: Option<i64>,
}

impl QueryImpl {
    const ROOTS_SQL_QUERY: &str = "SELECT {select_list}
        FROM roots
        {where_clause}
        {order_clause}
        {limit_clause}";

    const SCANS_SQL_QUERY: &str = "SELECT {select_list},
            COUNT(*) FILTER (WHERE changes.change_type = 'A') AS adds,
            COUNT(*) FILTER (WHERE changes.change_type = 'M') AS modifies,
            COUNT(*) FILTER (WHERE changes.change_type = 'D') AS deletes
        FROM scans
        LEFT JOIN changes
            ON changes.scan_id = scans.scan_id
        {where_clause}
        GROUP BY scans.scan_id
        {order_clause}
        {limit_clause}";

    const ITEMS_SQL_QUERY: &str = "SELECT {select_list}
        FROM items
        {where_clause}
        {order_clause}
        {limit_clause}";

    const CHANGES_SQL_QUERY: &str = "SELECT {select_list}
        FROM changes
        JOIN items
            ON changes.item_id = items.item_id
        {where_clause}
        {order_clause}
        {limit_clause}";

    fn new(sql_template: &'static str, col_set: ColSet) -> Self {
        QueryImpl {
            sql_template,
            col_set,

            filters: Vec::new(),
            show: Show::new(col_set),
            order: None,
            limit: None,
        }
    }
}

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
    last_hash_scan: Option<i64>,
    file_hash: Option<String>,
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
            last_hash_scan: row.get(8)?,
            file_hash: row.get(9)?,
            last_val_scan: row.get(10)?,
            val: row.get(11)?,
            val_error: row.get(12)?,
        })
    }
}

pub struct ChangesQueryRow {
    // changes properties
    pub change_id: i64,
    pub root_id: i64,
    pub scan_id: i64,
    pub item_id: i64,
    pub item_path: String,
    pub change_type: String,
    pub meta_change: Option<bool>,
    pub mod_date_old: Option<i64>,
    pub mod_date_new: Option<i64>,
    pub hash_change: Option<bool>,
    pub val_change: Option<bool>,
    pub val_old: Option<String>,
    pub val_new: Option<String>,
    pub val_error_old: Option<String>,
    pub val_error_new: Option<String>,
}

impl ChangesQueryRow {
    fn from_row(row: &rusqlite::Row) -> rusqlite::Result<Self> {
        Ok(ChangesQueryRow {
            change_id: row.get(0)?,
            root_id: row.get(1)?,
            scan_id: row.get(2)?,
            item_id: row.get(3)?,
            item_path: row.get(4)?,
            change_type: row.get(5)?,
            meta_change: row.get(6)?,
            mod_date_old: row.get(7)?,
            mod_date_new: row.get(8)?,
            hash_change: row.get(9)?,
            val_change: row.get(10)?,
            val_old: row.get(11)?,
            val_new: row.get(12)?,
            val_error_old: row.get(13)?,
            val_error_new: row.get(14)?,
        })
    }
}

pub struct ScansQueryRow {
    scan_id: i64,
    root_id: i64,
    state: i64,
    hashing: bool,
    validating: bool,
    scan_time: i64,
    file_count: Option<i64>,
    folder_count: Option<i64>,
    adds: i64,
    modifies: i64,
    deletes: i64,
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
            adds: row.get(8)?,
            modifies: row.get(9)?,
            deletes: row.get(10)?,
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
            //println!("{:?}", token.as_rule());
            match token.as_rule() {
                Rule::id_filter => {
                    IdFilter::add_to_query(token, query)?;
                }
                Rule::date_filter => {
                    DateFilter::add_to_query(token, query)?;
                }
                Rule::bool_filter
                | Rule::val_filter
                | Rule::item_type_filter
                | Rule::change_type_filter => {
                    EnumFilter::add_enum_filter_to_query(token, query)?;
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
