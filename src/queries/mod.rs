//use pest::Parser;
use pest_derive::Parser;

pub mod query;

pub use query::Query;

//pub mod parser;

#[derive(Parser)]
#[grammar = "queries/parser.pest"]
pub struct QueryParser;