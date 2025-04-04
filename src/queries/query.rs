use pest::Parser;

use crate::{database::Database, error::FsPulseError};

use super::{QueryParser, Rule};


pub struct Query;

impl Query {

    pub fn process_query(_db: &Database, query: &str) -> Result<(), FsPulseError> {
        
        Query::parse(Rule::query, query);
        // testing

        Query::parse(Rule::scan_filter, "scan:(32..34)");
        Query::parse(Rule::item_filter, "scan:(32..34)");
        Query::parse(Rule::items_query, "items where scan:(32..34)");
        Query::parse(Rule::query, "items where scan:(32..34)");

        Ok(())
    }

    fn parse(rule: Rule, s: &str) {

        match QueryParser::parse(rule, s) {
            Ok(mut pairs) => {
                println!("Parsed items_query: {:?}", pairs.next().unwrap());
            }
            Err(e) => {
                println!("Failed to parse items_query:\n{}", e);
            }
        }
    }
}