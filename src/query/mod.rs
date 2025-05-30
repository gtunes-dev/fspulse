use pest_derive::Parser;

pub mod columns;
mod filter;
mod order;
mod process;
mod show;

pub use process::QueryProcessor;

#[derive(Parser)]
#[grammar = "query/query.pest"]
pub struct QueryParser;
