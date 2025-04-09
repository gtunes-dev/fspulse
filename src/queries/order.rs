use pest::iterators::Pair;

use crate::error::FsPulseError;

use super::Rule;

#[derive(Debug)]
struct OrderSpec {
    column: String,
    direction: Option<String>
}

#[derive(Debug)]
pub struct Order {
    allowed_cols: &'static [&'static str],
    order_specs: Vec<OrderSpec>,
}

impl Order {
    pub const CHANGE_COLS: &'static [&'static str] = &["item_id", "scan_id"];
    pub const ITEM_COLS: &'static [&'static str] = &["item_id", "scan_id"];
    
    fn new(col_set: &'static [&'static str]) -> Self {
        Order {
            allowed_cols: col_set,
            order_specs: Vec::new()
        }
    }

    pub fn add_order_spec(&mut self, column: String, direction: Option<String>) -> Result<(), FsPulseError>{
        if !self.allowed_cols.contains(&column.as_str()) {
            return Err(FsPulseError::Error(format!("Invalid column '{}' in order clause", column)));
        }

        for order_spec in &self.order_specs {
            if order_spec.column  == column {
                return Err(FsPulseError::Error(format!("Column '{}' was already specified in order clause", column)));
            }
        }

        self.order_specs.push(OrderSpec { column, direction }  );
        Ok(())
    }

    pub fn build(order_list: Pair<Rule>, col_set: &'static [&'static str]) -> Result<Self, FsPulseError> {
        let mut order = Self::new(col_set);

        for element in order_list.into_inner() {
            match element.as_rule() {
                Rule::order_spec => {
                    let mut order_parts = element.into_inner();
                    let column = order_parts.next().unwrap().as_str();
                    let direction = order_parts.next().map(|r| r.as_str().to_string());
                    
                    order.add_order_spec(column.into(), direction)?;
                },
                _ => unreachable!(),
            }
        }

        Ok(order)
    }
}