use log::{error, info};
use pest::{iterators::Pairs, Parser};
use rusqlite::{Row, Statement, ToSql};
use tabled::{
    builder::Builder,
    settings::{object::Rows, Alignment, Style},
    Table,
};

use super::{
    columns::{
        ColSet, ALERTS_QUERY_COLS, CHANGES_QUERY_COLS, ITEMS_QUERY_COLS, ROOTS_QUERY_COLS,
        SCANS_QUERY_COLS,
    },
    filter::{BoolFilter, EnumFilter, IntFilter},
    show::{Format, Show},
};
//use tablestream::{Column, Stream};

use crate::{
    alerts::{AlertStatus, AlertType},
    changes::ChangeType,
    database::Database,
    error::FsPulseError,
    items::{Access, ItemType},
    scans::ScanState,
    validate::validator::ValidationState,
};

use super::{
    columns::ColAlign,
    filter::{DateFilter, Filter, IdFilter, PathFilter, StringFilter},
    order::Order,
    QueryParser, Rule,
};

/// Query result type: (rows, column headers, column alignments)
pub type QueryResultData = (Vec<Vec<String>>, Vec<String>, Vec<ColAlign>);

pub trait QueryResult {
    fn prepare(&mut self, show: &mut Show);
    fn add_row(&mut self, row: Vec<String>);
    fn finalize(&mut self, show: &mut Show);
}

struct QueryResultBuilder {
    tabled_builder: Option<Builder>,
    table: Option<Table>,
}

impl QueryResult for QueryResultBuilder {
    fn prepare(&mut self, show: &mut Show) {
        if let Some(builder) = self.tabled_builder.as_mut() {
            show.prepare_builder(builder);
        } else {
            panic!("QueryResultBuilder used after finalize");
        }
    }

    fn add_row(&mut self, row: Vec<String>) {
        self.tabled_builder
            .as_mut()
            .expect("QueryResultBuilder used after finalize")
            .push_record(row);
    }

    fn finalize(&mut self, show: &mut Show) {
        if let Some(builder) = self.tabled_builder.take() {
            let mut table = builder.build();
            show.set_column_aligments(&mut table);
            self.table = Some(table);
        } else {
            panic!("Attempted to finalize twice");
        }
    }
}

impl QueryResultBuilder {
    fn new() -> Self {
        QueryResultBuilder {
            tabled_builder: Some(Builder::new()),
            table: None,
        }
    }
}

struct QueryResultVector {
    row_vec: Vec<Vec<String>>,
    column_headers: Vec<String>,
    column_alignments: Vec<ColAlign>,
}

impl QueryResult for QueryResultVector {
    fn prepare(&mut self, show: &mut Show) {
        show.ensure_columns();
    }
    fn add_row(&mut self, row: Vec<String>) {
        self.row_vec.push(row);
    }
    fn finalize(&mut self, show: &mut Show) {
        // Extract column headers and alignments from Show after columns are finalized
        self.column_headers = show
            .get_column_headers()
            .into_iter()
            .map(|s| s.to_string())
            .collect();
        self.column_alignments = show.get_column_alignments();
    }
}

impl QueryResultVector {
    fn new() -> Self {
        QueryResultVector {
            row_vec: Vec::new(),
            column_headers: Vec::new(),
            column_alignments: Vec::new(),
        }
    }
}

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
            match first {
                true => first = false,
                false => select_list.push_str(", "),
            }
            select_list.push_str(col_spec.name_db);
        }

        select_list
    }

    fn build_query_result(
        &mut self,
        sql_statement: &mut Statement,
        sql_params: &[&dyn ToSql],
        query_result: &mut dyn QueryResult,
    ) -> Result<(), FsPulseError>;

    /// Executes a count query using COUNT(*) SQL
    /// This is more efficient than loading all rows and counting them
    fn prepare_and_execute_count(&self) -> Result<i64, FsPulseError> {
        let conn = Database::get_connection()?;
        let (sql, params_vec) = self.build_sql(true, None, None);
        let sql_params: Vec<&dyn ToSql> = params_vec.iter().map(|b| &**b).collect();
        let mut sql_statement = conn.prepare(&sql)?;

        let count: i64 = sql_statement.query_row(&sql_params[..], |row| row.get(0))?;

        Ok(count)
    }

    /// Builds the SQL query string with optional count mode and limit/offset overrides
    /// When count_only=true, wraps query in subquery for COUNT(*) and omits ORDER BY
    /// When limit_override/offset_add are provided, adjusts pagination accordingly
    fn build_sql(
        &self,
        count_only: bool,
        limit_override: Option<i64>,
        offset_add: Option<i64>,
    ) -> (String, Vec<Box<dyn ToSql>>) {
        // Build SELECT clause
        let select_list = if count_only {
            "COUNT(*)".to_string()
        } else {
            self.cols_as_select_list()
        };

        // Build WHERE clause and collect parameters
        let mut params_vec: Vec<Box<dyn ToSql>> = Vec::new();
        let mut where_clause = String::new();

        if !self.query_impl().filters.is_empty() {
            let mut first = true;
            where_clause.push_str("\nWHERE ");
            for filter in &self.query_impl().filters {
                if !first {
                    where_clause.push_str(" AND ");
                }
                first = false;

                // Note: This unwrap is safe because filters validate during parsing
                let (pred_str, pred_vec) = filter.to_predicate_parts().unwrap();
                where_clause.push_str(&pred_str);
                params_vec.extend(pred_vec);
            }
        }

        // Build ORDER BY, LIMIT, OFFSET clauses (omitted for count queries)
        let order_clause = if count_only {
            String::new()
        } else {
            match self.order() {
                Some(order) => order.to_order_clause(),
                None => String::new(),
            }
        };

        let limit_clause = match (self.query_impl().limit, limit_override) {
            (Some(user_limit), Some(override_limit)) => {
                // Calculate remaining rows within user's limit
                let offset = offset_add.unwrap_or(0);
                let remaining = user_limit - offset;
                let effective = remaining.min(override_limit).max(0);
                format!("\nLIMIT {effective}")
            }
            (None, Some(override_limit)) => format!("\nLIMIT {override_limit}"),
            (Some(limit), None) => format!("\nLIMIT {limit}"),
            (None, None) => String::new(),
        };

        let final_offset = match (self.query_impl().offset, offset_add) {
            (None, None) => None,
            (Some(o), None) => Some(o),
            (None, Some(a)) => Some(a),
            (Some(o), Some(a)) => Some(o + a),
        };

        let offset_clause = final_offset
            .map(|o| format!("\nOFFSET {o}"))
            .unwrap_or_default();

        // Assemble final SQL
        let sql = if count_only {
            // For count queries, use subquery to correctly count with limit/offset
            let inner_sql = self
                .query_impl()
                .sql_template
                .replace("{select_list}", "1")
                .replace("{where_clause}", &where_clause)
                .replace("{order_clause}", "")
                .replace("{limit_clause}", &limit_clause)
                .replace("{offset_clause}", &offset_clause);

            format!("SELECT COUNT(*) FROM ({}) AS subquery", inner_sql)
        } else {
            self.query_impl()
                .sql_template
                .replace("{select_list}", &select_list)
                .replace("{where_clause}", &where_clause)
                .replace("{order_clause}", &order_clause)
                .replace("{limit_clause}", &limit_clause)
                .replace("{offset_clause}", &offset_clause)
        };

        (sql, params_vec)
    }

    fn prepare_and_execute(
        &mut self,
        query_result: &mut dyn QueryResult,
    ) -> Result<(), FsPulseError> {
        let conn = Database::get_connection()?;
        let (sql, params_vec) = self.build_sql(false, None, None);
        let sql_params: Vec<&dyn ToSql> = params_vec.iter().map(|b| &**b).collect();
        let mut sql_statement = conn.prepare(&sql)?;

        self.build_query_result(&mut sql_statement, &sql_params, query_result)?;

        Ok(())
    }

    fn prepare_and_execute_override(
        &mut self,
        limit_override: i64,
        offset_add: i64,
        query_result: &mut dyn QueryResult,
    ) -> Result<(), FsPulseError> {
        let conn = Database::get_connection()?;
        let (sql, params_vec) = self.build_sql(false, Some(limit_override), Some(offset_add));
        let sql_params: Vec<&dyn ToSql> = params_vec.iter().map(|b| &**b).collect();
        let mut sql_statement = conn.prepare(&sql)?;

        self.build_query_result(&mut sql_statement, &sql_params, query_result)?;

        Ok(())
    }
}

fn make_query(query_type: &str, count_only: bool) -> Box<dyn Query> {
    match (query_type, count_only) {
        ("roots", _) => Box::new(RootsQuery {
            imp: QueryImpl::new(QueryImpl::ROOTS_SQL_QUERY, ColSet::new(&ROOTS_QUERY_COLS)),
        }),
        ("scans", false) => Box::new(ScansQuery {
            imp: QueryImpl::new(QueryImpl::SCANS_SQL_QUERY, ColSet::new(&SCANS_QUERY_COLS)),
        }),
        ("scans", true) => Box::new(ScansQuery {
            imp: QueryImpl::new(
                QueryImpl::SCANS_SQL_QUERY_COUNT,
                ColSet::new(&SCANS_QUERY_COLS),
            ),
        }),
        ("items", _) => Box::new(ItemsQuery {
            imp: QueryImpl::new(QueryImpl::ITEMS_SQL_QUERY, ColSet::new(&ITEMS_QUERY_COLS)),
        }),
        ("changes", _) => Box::new(ChangesQuery {
            imp: QueryImpl::new(
                QueryImpl::CHANGES_SQL_QUERY,
                ColSet::new(&CHANGES_QUERY_COLS),
            ),
        }),
        ("alerts", _) => Box::new(AlertsQuery {
            imp: QueryImpl::new(QueryImpl::ALERTS_SQL_QUERY, ColSet::new(&ALERTS_QUERY_COLS)),
        }),
        _ => unreachable!(),
    }
}

struct AlertsQuery {
    imp: QueryImpl,
}

impl Query for AlertsQuery {
    fn query_impl(&self) -> &QueryImpl {
        &self.imp
    }

    fn query_impl_mut(&mut self) -> &mut QueryImpl {
        &mut self.imp
    }

    fn build_query_result(
        &mut self,
        sql_statement: &mut Statement,
        sql_params: &[&dyn ToSql],
        query_result: &mut dyn QueryResult,
    ) -> Result<(), FsPulseError> {
        let rows = sql_statement.query_map(sql_params, AlertsQueryRow::from_row)?;

        query_result.prepare(&mut self.query_impl_mut().show);

        for row in rows {
            let alerts_query_row: AlertsQueryRow = row?;
            self.append_alerts_row(&alerts_query_row, query_result)?;
        }

        Ok(())
    }
}

impl AlertsQuery {
    pub fn append_alerts_row(
        &self,
        alert: &AlertsQueryRow,
        query_result: &mut dyn QueryResult,
    ) -> Result<(), FsPulseError> {
        let mut row: Vec<String> = Vec::new();

        for col in &self.show().display_cols {
            let col_string = match col.display_col {
                "alert_id" => Format::format_i64(alert.alert_id),
                "alert_type" => Format::format_alert_type(alert.alert_type, col.format)?,
                "alert_status" => Format::format_alert_status(alert.alert_status, col.format)?,
                "root_id" => Format::format_i64(alert.root_id),
                "scan_id" => Format::format_i64(alert.scan_id),
                "item_id" => Format::format_i64(alert.item_id),
                "item_path" => Format::format_path(&alert.item_path, col.format)?,
                "created_at" => Format::format_date(alert.created_at, col.format)?,
                "updated_at" => Format::format_opt_date(alert.updated_at, col.format)?,
                "prev_hash_scan" => Format::format_opt_i64(alert.prev_hash_scan),
                "hash_old" => Format::format_opt_string(&alert.hash_old),
                "hash_new" => Format::format_opt_string(&alert.hash_new),
                "val_error" => Format::format_opt_string(&alert.val_error),
                _ => {
                    return Err(FsPulseError::Error("Invalid column".into()));
                }
            };

            row.push(col_string);
        }

        query_result.add_row(row);

        Ok(())
    }
}

impl Query for RootsQuery {
    fn query_impl(&self) -> &QueryImpl {
        &self.imp
    }
    fn query_impl_mut(&mut self) -> &mut QueryImpl {
        &mut self.imp
    }

    fn build_query_result(
        &mut self,
        sql_statement: &mut Statement,
        sql_params: &[&dyn ToSql],
        query_result: &mut dyn QueryResult,
    ) -> Result<(), FsPulseError> {
        let rows = sql_statement.query_map(sql_params, RootsQueryRow::from_row)?;

        query_result.prepare(&mut self.query_impl_mut().show);

        for row in rows {
            let roots_query_row: RootsQueryRow = row?;
            self.append_roots_row(&roots_query_row, query_result)?;
        }

        Ok(())
    }
}
struct RootsQuery {
    imp: QueryImpl,
}

impl RootsQuery {
    pub fn append_roots_row(
        &self,
        root: &RootsQueryRow,
        query_result: &mut dyn QueryResult,
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

        query_result.add_row(row);

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

    fn build_query_result(
        &mut self,
        sql_statment: &mut Statement,
        sql_params: &[&dyn ToSql],
        query_result: &mut dyn QueryResult,
    ) -> Result<(), FsPulseError> {
        let rows = sql_statment.query_map(sql_params, ItemsQueryRow::from_row)?;

        query_result.prepare(&mut self.query_impl_mut().show);

        for row in rows {
            let items_query_row = row?;
            self.append_items_row(&items_query_row, query_result)?;
        }

        Ok(())
    }
}

struct ItemsQuery {
    imp: QueryImpl,
}

impl ItemsQuery {
    pub fn append_items_row(
        &self,
        item: &ItemsQueryRow,
        query_result: &mut dyn QueryResult,
    ) -> Result<(), FsPulseError> {
        let mut row: Vec<String> = Vec::new();

        for col in &self.show().display_cols {
            let col_string = match col.display_col {
                "item_id" => Format::format_i64(item.item_id),
                "root_id" => Format::format_i64(item.root_id),
                "item_path" => Format::format_path(&item.item_path, col.format)?,
                "item_type" => Format::format_item_type(item.item_type, col.format)?,
                "last_scan" => Format::format_i64(item.last_scan),
                "is_ts" => Format::format_bool(item.is_ts, col.format)?,
                "access" => Format::format_access(item.access, col.format)?,
                "mod_date" => Format::format_opt_date(item.mod_date, col.format)?,
                "size" => Format::format_opt_i64(item.size),
                "file_hash" => Format::format_opt_string(&item.file_hash),
                "last_hash_scan" => Format::format_opt_i64(item.last_hash_scan),
                "last_val_scan" => Format::format_opt_i64(item.last_val_scan),
                "val" => Format::format_val(ValidationState::from_i64(item.val), col.format)?,
                "val_error" => Format::format_opt_string(&item.val_error),
                _ => {
                    return Err(FsPulseError::Error("Invalid column".into()));
                }
            };

            row.push(col_string);
        }

        query_result.add_row(row);

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

    fn build_query_result(
        &mut self,
        sql_statment: &mut Statement,
        sql_params: &[&dyn ToSql],
        query_result: &mut dyn QueryResult,
    ) -> Result<(), FsPulseError> {
        let rows = sql_statment.query_map(sql_params, ScansQueryRow::from_row)?;

        query_result.prepare(&mut self.query_impl_mut().show);

        for row in rows {
            let scans_query_row = row?;
            self.append_scans_row(&scans_query_row, query_result)?;
        }

        Ok(())
    }
}
struct ScansQuery {
    imp: QueryImpl,
}

impl ScansQuery {
    pub fn append_scans_row(
        &self,
        scan: &ScansQueryRow,
        query_result: &mut dyn QueryResult,
    ) -> Result<(), FsPulseError> {
        let mut row: Vec<String> = Vec::new();

        for col in &self.show().display_cols {
            let col_string = match col.display_col {
                "scan_id" => Format::format_i64(scan.scan_id),
                "root_id" => Format::format_i64(scan.root_id),
                "schedule_id" => Format::format_opt_i64(scan.schedule_id),
                "started_at" => Format::format_i64(scan.started_at),
                "ended_at" => Format::format_opt_i64(scan.ended_at),
                "was_restarted" => Format::format_bool(scan.was_restarted, col.format)?,
                "scan_state" => Format::format_scan_state(scan.state, col.format)?,
                "is_hash" => Format::format_bool(scan.is_hash, col.format)?,
                "hash_all" => Format::format_bool(scan.hash_all, col.format)?,
                "is_val" => Format::format_bool(scan.is_val, col.format)?,
                "val_all" => Format::format_bool(scan.val_all, col.format)?,
                "file_count" => Format::format_opt_i64(scan.file_count),
                "folder_count" => Format::format_opt_i64(scan.folder_count),
                "total_size" => Format::format_opt_i64(scan.total_size),
                "alert_count" => Format::format_opt_i64(scan.alert_count),
                "add_count" => Format::format_opt_i64(scan.add_count),
                "modify_count" => Format::format_opt_i64(scan.modify_count),
                "delete_count" => Format::format_opt_i64(scan.delete_count),
                "error" => Format::format_opt_string(&scan.error),
                _ => {
                    return Err(FsPulseError::Error("Invalid column".into()));
                }
            };

            row.push(col_string);
        }

        query_result.add_row(row);

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

    fn build_query_result(
        &mut self,
        sql_statment: &mut Statement,
        sql_params: &[&dyn ToSql],
        query_result: &mut dyn QueryResult,
    ) -> Result<(), FsPulseError> {
        let rows = sql_statment.query_map(sql_params, ChangesQueryRow::from_row)?;

        query_result.prepare(&mut self.query_impl_mut().show);

        for row in rows {
            let changes_query_row: ChangesQueryRow = row?;

            self.append_changes_row(&changes_query_row, query_result)?;
        }

        Ok(())
    }
}
struct ChangesQuery {
    imp: QueryImpl,
}

impl ChangesQuery {
    pub fn append_changes_row(
        &self,
        change: &ChangesQueryRow,
        query_result: &mut dyn QueryResult,
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
                "change_type" => Format::format_change_type(change.change_type, col.format)?,
                "access_old" => {
                    Format::format_opt_access(change.access_old.map(Access::from_i64), col.format)?
                }
                "access_new" => {
                    Format::format_opt_access(change.access_new.map(Access::from_i64), col.format)?
                }
                "is_undelete" => Format::format_opt_bool(change.is_undelete, col.format)?,
                "meta_change" => Format::format_opt_bool(change.meta_change, col.format)?,
                "mod_date_old" => Format::format_opt_date(change.mod_date_old, col.format)?,
                "mod_date_new" => Format::format_opt_date(change.mod_date_new, col.format)?,
                "size_old" => Format::format_opt_i64(change.size_old),
                "size_new" => Format::format_opt_i64(change.size_new),
                "hash_change" => Format::format_opt_bool(change.hash_change, col.format)?,
                "last_hash_scan_old" => Format::format_opt_i64(change.last_hash_scan_old),
                "hash_old" => Format::format_opt_string(&change.hash_old),
                "hash_new" => Format::format_opt_string(&change.hash_new),
                "val_change" => Format::format_opt_bool(change.val_change, col.format)?,
                "last_val_scan_old" => Format::format_opt_i64(change.last_val_scan_old),
                "val_old" => Format::format_opt_val(
                    change.val_old.map(ValidationState::from_i64),
                    col.format,
                )?,
                "val_new" => Format::format_opt_val(
                    change.val_new.map(ValidationState::from_i64),
                    col.format,
                )?,
                "val_error_old" => Format::format_opt_string(&change.val_error_old),
                "val_error_new" => Format::format_opt_string(&change.val_error_new),
                _ => {
                    return Err(FsPulseError::Error("Invalid column".into()));
                }
            };

            row.push(col_string);
        }

        query_result.add_row(row);

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
    offset: Option<i64>,
}

impl QueryImpl {
    const ROOTS_SQL_QUERY: &str = "SELECT {select_list}
        FROM roots
        {where_clause}
        {order_clause}
        {limit_clause}
        {offset_clause}";

    const SCANS_SQL_QUERY: &str = "SELECT {select_list}
        FROM scans
        {where_clause}
        {order_clause}
        {limit_clause}
        {offset_clause}";

    const SCANS_SQL_QUERY_COUNT: &str = "SELECT {select_list}
        FROM scans
        {where_clause}
        {order_clause}
        {limit_clause}
        {offset_clause}";

    const ITEMS_SQL_QUERY: &str = "SELECT {select_list}
        FROM items
        {where_clause}
        {order_clause}
        {limit_clause}
        {offset_clause}";

    const CHANGES_SQL_QUERY: &str = "SELECT {select_list}
        FROM changes
        JOIN items
            ON changes.item_id = items.item_id
        {where_clause}
        {order_clause}
        {limit_clause}
        {offset_clause}";

    const ALERTS_SQL_QUERY: &str = "SELECT {select_list}
        FROM alerts
        JOIN items
          ON alerts.item_id = items.item_id
        JOIN scans
            on alerts.scan_id = scans.scan_id
        {where_clause}
        {order_clause}
        {limit_clause}
        {offset_clause}";

    fn new(sql_template: &'static str, col_set: ColSet) -> Self {
        QueryImpl {
            sql_template,
            col_set,

            filters: Vec::new(),
            show: Show::new(col_set),
            order: None,
            limit: None,
            offset: None,
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
    item_type: ItemType,
    last_scan: i64,
    is_ts: bool,
    access: Access,
    mod_date: Option<i64>,
    size: Option<i64>,
    last_hash_scan: Option<i64>,
    file_hash: Option<String>,
    last_val_scan: Option<i64>,
    val: i64,
    val_error: Option<String>,
}

impl ItemsQueryRow {
    fn from_row(row: &rusqlite::Row) -> rusqlite::Result<Self> {
        Ok(ItemsQueryRow {
            item_id: row.get(0)?,
            root_id: row.get(1)?,
            item_path: row.get(2)?,
            item_type: ItemType::from_i64(row.get(3)?),
            last_scan: row.get(4)?,
            is_ts: row.get(5)?,
            access: Access::from_i64(row.get(6)?),
            mod_date: row.get(7)?,
            size: row.get(8)?,
            last_hash_scan: row.get(9)?,
            file_hash: row.get(10)?,
            last_val_scan: row.get(11)?,
            val: row.get(12)?,
            val_error: row.get(13)?,
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
    pub change_type: ChangeType,
    pub access_old: Option<i64>,
    pub access_new: Option<i64>,
    pub is_undelete: Option<bool>,
    pub meta_change: Option<bool>,
    pub mod_date_old: Option<i64>,
    pub mod_date_new: Option<i64>,
    pub size_old: Option<i64>,
    pub size_new: Option<i64>,
    pub hash_change: Option<bool>,
    pub last_hash_scan_old: Option<i64>,
    pub hash_old: Option<String>,
    pub hash_new: Option<String>,
    pub val_change: Option<bool>,
    pub last_val_scan_old: Option<i64>,
    pub val_old: Option<i64>,
    pub val_new: Option<i64>,
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
            change_type: ChangeType::from_i64(row.get(5)?),
            access_old: row.get(6)?,
            access_new: row.get(7)?,
            is_undelete: row.get(8)?,
            meta_change: row.get(9)?,
            mod_date_old: row.get(10)?,
            mod_date_new: row.get(11)?,
            size_old: row.get(12)?,
            size_new: row.get(13)?,
            hash_change: row.get(14)?,
            last_hash_scan_old: row.get(15)?,
            hash_old: row.get(16)?,
            hash_new: row.get(17)?,
            val_change: row.get(18)?,
            last_val_scan_old: row.get(19)?,
            val_old: row.get(20)?,
            val_new: row.get(21)?,
            val_error_old: row.get(22)?,
            val_error_new: row.get(23)?,
        })
    }
}

pub struct ScansQueryRow {
    scan_id: i64,
    root_id: i64,
    schedule_id: Option<i64>,
    started_at: i64,
    ended_at: Option<i64>,
    was_restarted: bool,
    state: ScanState,
    is_hash: bool,
    hash_all: bool,
    is_val: bool,
    val_all: bool,
    file_count: Option<i64>,
    folder_count: Option<i64>,
    total_size: Option<i64>,
    alert_count: Option<i64>,
    add_count: Option<i64>,
    modify_count: Option<i64>,
    delete_count: Option<i64>,
    error: Option<String>,
}

impl ScansQueryRow {
    pub fn from_row(row: &Row) -> rusqlite::Result<Self> {
        let state_i64: i64 = row.get(6)?;
        Ok(ScansQueryRow {
            scan_id: row.get(0)?,
            root_id: row.get(1)?,
            schedule_id: row.get(2)?,
            started_at: row.get(3)?,
            ended_at: row.get(4)?,
            was_restarted: row.get(5)?,
            state: ScanState::from_i64(state_i64),
            is_hash: row.get(7)?,
            hash_all: row.get(8)?,
            is_val: row.get(9)?,
            val_all: row.get(10)?,
            file_count: row.get(11)?,
            folder_count: row.get(12)?,
            total_size: row.get(13)?,
            alert_count: row.get(14)?,
            add_count: row.get(15)?,
            modify_count: row.get(16)?,
            delete_count: row.get(17)?,
            error: row.get(18)?,
        })
    }
}

pub struct AlertsQueryRow {
    alert_id: i64,
    alert_type: AlertType,
    alert_status: AlertStatus,
    root_id: i64,
    scan_id: i64,
    item_id: i64,
    item_path: String,
    created_at: i64,
    updated_at: Option<i64>,
    prev_hash_scan: Option<i64>,
    hash_old: Option<String>,
    hash_new: Option<String>,
    val_error: Option<String>,
}

impl AlertsQueryRow {
    fn from_row(row: &rusqlite::Row) -> rusqlite::Result<Self> {
        Ok(AlertsQueryRow {
            alert_id: row.get(0)?,
            alert_type: AlertType::from_i64(row.get(1)?),
            alert_status: AlertStatus::from_i64(row.get(2)?),
            root_id: row.get(3)?,
            scan_id: row.get(4)?,
            item_id: row.get(5)?,
            item_path: row.get(6)?,
            created_at: row.get(7)?,
            updated_at: row.get(8)?,
            prev_hash_scan: row.get(9)?,
            hash_old: row.get(10)?,
            hash_new: row.get(11)?,
            val_error: row.get(12)?,
        })
    }
}

impl QueryProcessor {
    pub fn execute_query(query_str: &str) -> Result<QueryResultData, FsPulseError> {
        let mut qrv = QueryResultVector::new();
        Self::process_query(query_str, &mut qrv)?;
        Ok((qrv.row_vec, qrv.column_headers, qrv.column_alignments))
    }

    pub fn execute_query_override(
        query_str: &str,
        limit_override: i64,
        offset_add: i64,
    ) -> Result<QueryResultData, FsPulseError> {
        let mut qrv = QueryResultVector::new();
        Self::process_query_override(query_str, limit_override, offset_add, &mut qrv)?;
        Ok((qrv.row_vec, qrv.column_headers, qrv.column_alignments))
    }

    pub fn execute_query_count(query_str: &str) -> Result<i64, FsPulseError> {
        Self::process_query_count(query_str)
    }

    pub fn execute_query_and_print(query_str: &str) -> Result<(), FsPulseError> {
        let mut qrb = QueryResultBuilder::new();

        match Self::process_query(query_str, &mut qrb) {
            Ok(()) => {}
            Err(err) => match err {
                FsPulseError::ParsingError(inner) => {
                    error!("Query parsing error: {inner}");
                    println!("{inner}");
                    return Ok(());
                }
                FsPulseError::CustomParsingError(msg) => {
                    info!("Query parsing error: {msg}");
                    println!("{msg}");
                    return Ok(());
                }
                _ => {
                    return Err(err);
                }
            },
        };

        let table = qrb.table.as_mut().unwrap();

        table.with(Style::modern());
        table.modify(Rows::first(), Alignment::center());

        println!("{table}");

        Ok(())
    }

    fn process_query(
        query_str: &str,
        query_result: &mut dyn QueryResult,
    ) -> Result<(), FsPulseError> {
        let mut parsed_query = QueryParser::parse(Rule::query, query_str)
            .map_err(|err| FsPulseError::ParsingError(Box::new(err)))?;

        let query_pair = parsed_query.next().unwrap();
        let mut query_iter = query_pair.into_inner();

        let query_type_pair = query_iter.next().unwrap();

        let mut query = make_query(query_type_pair.as_str(), false);

        QueryProcessor::build(&mut *query, &mut query_iter)?;

        query.prepare_and_execute(query_result)?;
        query_result.finalize(&mut query.query_impl_mut().show);

        Ok(())
    }

    fn process_query_override(
        query_str: &str,
        limit_override: i64,
        offset_add: i64,
        query_result: &mut dyn QueryResult,
    ) -> Result<(), FsPulseError> {
        let mut parsed_query = QueryParser::parse(Rule::query, query_str)
            .map_err(|err| FsPulseError::ParsingError(Box::new(err)))?;

        let query_pair = parsed_query.next().unwrap();
        let mut query_iter = query_pair.into_inner();

        let query_type_pair = query_iter.next().unwrap();

        let mut query = make_query(query_type_pair.as_str(), false);

        QueryProcessor::build(&mut *query, &mut query_iter)?;

        query.prepare_and_execute_override(limit_override, offset_add, query_result)?;
        query_result.finalize(&mut query.query_impl_mut().show);

        Ok(())
    }

    fn process_query_count(query_str: &str) -> Result<i64, FsPulseError> {
        info!("Parsing count query: {query_str}");

        let mut parsed_query = QueryParser::parse(Rule::query, query_str)
            .map_err(|err| FsPulseError::ParsingError(Box::new(err)))?;

        info!("Parsed count query: {parsed_query}");

        let query_pair = parsed_query.next().unwrap();
        let mut query_iter = query_pair.into_inner();

        let query_type_pair = query_iter.next().unwrap();

        let mut query = make_query(query_type_pair.as_str(), true);

        QueryProcessor::build(&mut *query, &mut query_iter)?;

        // Use the count execution path - access directly through dyn Query
        let count = (*query).prepare_and_execute_count()?;

        Ok(count)
    }

    pub fn validate_parsed_filter(rule: Rule, pairs: &mut Pairs<Rule>) -> Option<String> {
        if rule == Rule::date_filter_EOI {
            match DateFilter::validate_values(pairs) {
                Ok(_) => return None,
                Err(e) => return Some(e.to_string()),
            }
        };
        None
    }

    pub fn validate_filter(rule: Rule, filter: &str) -> Option<String> {
        match QueryParser::parse(rule, filter).as_mut() {
            // In cases such as "dates", input valiation happens during
            // query building, not parsing, since the parser doesn't understand
            // date validity. For these cases, we need to explicitly validate
            Ok(parsed_query) => QueryProcessor::validate_parsed_filter(rule, parsed_query),
            Err(e) => Some(e.to_string()),
        }
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
                Rule::bool_filter => {
                    BoolFilter::add_bool_filter_to_query(token, query)?;
                }
                Rule::scan_state_filter
                | Rule::item_type_filter
                | Rule::change_type_filter
                | Rule::alert_type_filter
                | Rule::alert_status_filter
                | Rule::val_filter
                | Rule::access_filter => {
                    EnumFilter::add_enum_filter_to_query(token, query)?;
                }
                Rule::path_filter => {
                    PathFilter::add_to_query(token, query)?;
                }
                Rule::string_filter => {
                    StringFilter::add_string_filter_to_query(token, query)?;
                }
                Rule::int_filter => {
                    IntFilter::add_int_filter_to_query(token, query)?;
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
                Rule::offset_val => {
                    query.query_impl_mut().offset = Some(token.as_str().parse().unwrap());
                }
                _ => {}
            }
        }

        Ok(())
    }
}
