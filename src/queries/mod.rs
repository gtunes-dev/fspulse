use pest_derive::Parser;

mod columns;
mod filters;
mod order;
pub mod query;

pub use query::Query;

//pub mod parser;

#[derive(Parser)]
#[grammar = "queries/query.pest"]
pub struct QueryParser;
