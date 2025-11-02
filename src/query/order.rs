use pest::iterators::Pair;

use crate::error::FsPulseError;

use super::{columns::ColSet, Rule};

#[derive(Debug)]
struct OrderSpec {
    column: String,
    direction: Option<String>,
    collation: Option<&'static str>,
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
        let col_spec = self
            .col_set
            .col_set()
            .get(col_display_name)
            .ok_or_else(|| {
                FsPulseError::CustomParsingError(format!(
                    "Invalid column '{col_display_name}' in order clause"
                ))
            })?;

        let db_col_name = col_spec.name_db;
        let collation = col_spec.col_type.collation();

        for order_spec in &self.order_specs {
            if order_spec.column == db_col_name {
                return Err(FsPulseError::CustomParsingError(format!(
                    "Column '{col_display_name}' was already specified in order clause"
                )));
            }
        }

        self.order_specs.push(OrderSpec {
            column: db_col_name.into(),
            direction,
            collation,
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

            if let Some(collation) = order.collation {
                order_clause.push_str(" COLLATE ");
                order_clause.push_str(collation);
            }

            order_clause.push(' ');

            match &order.direction {
                Some(direction) => order_clause.push_str(direction),
                None => order_clause.push_str("ASC"),
            }
        }
        order_clause
    }
}
