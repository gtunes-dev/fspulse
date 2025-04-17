use pest_derive::Parser;

mod columns;
mod filter;
mod order;
pub mod query;
mod show;

pub use query::Query;

//pub mod parser;

#[derive(Parser)]
#[grammar = "queries/query.pest"]
pub struct QueryParser;
