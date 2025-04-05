//use pest::Parser;
use pest_derive::Parser;

pub mod query;

pub use query::Query;

//pub mod parser;

#[derive(Parser)]
#[grammar = "queries/query.pest"]
pub struct QueryParser;