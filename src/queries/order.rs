use pest::iterators::Pair;

use crate::error::FsPulseError;

use super::{columns::ColSet, Rule};

#[derive(Debug)]
struct OrderSpec {
    column: String,
    direction: Option<String>,
}

#[derive(Debug)]
pub struct Order {
    col_set: ColSet,
    order_specs: Vec<OrderSpec>,
}

impl Order {
    fn new(col_set: ColSet) -> Self {
        Order {
            col_set,
            order_specs: Vec::new(),
        }
    }

    pub fn add_order_spec(
        &mut self,
        col_display_name: &str,
        direction: Option<String>,
    ) -> Result<(), FsPulseError> {
        let db_col_name = self.col_set.get_name_db(col_display_name).ok_or_else(|| {
            FsPulseError::CustomParsingError(format!(
                "Invalid column '{}' in order clause",
                col_display_name
            ))
        })?;

        for order_spec in &self.order_specs {
            if order_spec.column == db_col_name {
                return Err(FsPulseError::CustomParsingError(format!(
                    "Column '{}' was already specified in order clause",
                    col_display_name
                )));
            }
        }

        self.order_specs.push(OrderSpec {
            column: db_col_name.into(),
            direction,
        });
        Ok(())
    }

    pub fn from_pest_pair(order_list: Pair<Rule>, col_set: ColSet) -> Result<Self, FsPulseError> {
        let mut order = Self::new(col_set);

        for element in order_list.into_inner() {
            match element.as_rule() {
                Rule::order_spec => {
                    let mut order_parts = element.into_inner();
                    let column_display_name = order_parts.next().unwrap().as_str();
                    let direction = order_parts.next().map(|r| r.as_str().to_uppercase());

                    order.add_order_spec(column_display_name, direction)?;
                }
                _ => unreachable!(),
            }
        }

        Ok(order)
    }

    pub fn to_order_clause(&self) -> String {
        let mut order_clause = "\nORDER BY ".to_string();
        let mut first = true;

        for order in &self.order_specs {
            match first {
                true => first = false,
                false => order_clause.push_str(", "),
            }

            order_clause.push_str(&order.column);
            order_clause.push(' ');

            match &order.direction {
                Some(direction) => order_clause.push_str(direction),
                None => order_clause.push_str("ASC"),
            }
        }
        order_clause
    }
}
