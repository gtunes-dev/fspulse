use rusqlite::params;
use serde::{Deserialize, Serialize};

use crate::database::Database;
use crate::error::FsPulseError;

#[derive(Clone, Debug)]
#[allow(dead_code)]
pub struct Change {
    pub change_id: i64,
    pub scan_id: i64,
    pub item_id: i64,
    pub change_type: ChangeType,
    pub is_undelete: Option<bool>, // Present if "A". True if add is undelete
    pub meta_change: Option<bool>, // Present if "M". True if metadata changed, else False
    pub mod_date_old: Option<i64>, // Meaningful if undelete or meta_change
    pub mod_date_new: Option<i64>, // Meaningful if metdata_changed
    pub file_size_old: Option<i64>, // Meaningful if undelete or meta_change
    pub file_size_new: Option<i64>, // Meaningful if undelete or meta_change
    pub hash_change: Option<bool>, // Present if "M". True if hash changed, else False
    #[allow(dead_code)]
    pub last_hash_scan_old: Option<i64>, // Present if "M" and hash_change
    pub hash_old: Option<String>,  // Meaningful if undelete or hash_change
    #[allow(dead_code)]
    pub hash_new: Option<String>, // Meaningful if hash_change
    pub val_change: Option<bool>,  // Present if "M", True if validation changed, else False
    #[allow(dead_code)]
    pub last_val_scan_old: Option<i64>, // Present if "M" and validation changed
    pub val_old: Option<i64>,   // Validation state of the item if val_change = true
    #[allow(dead_code)]
    pub val_new: Option<i64>, // Meaningful if undelete or val_change
    #[allow(dead_code)]
    pub val_error_old: Option<String>, // Meaningful if undelete or val_change
    #[allow(dead_code)]
    pub val_error_new: Option<String>, // Meaningful if validity changed

    // $TODO: Remove this. Was a bad idea to have this in the first place
    // Changes should be a simple struct that models a Changes entity
    // Additional non-entity fields
    pub item_type: String,
    pub item_path: String,
}

#[derive(Copy, Clone, Debug, Default)]
pub struct ChangeCounts {
    pub add_count: i64,
    pub modify_count: i64,
    pub delete_count: i64,
    pub no_change_count: i64,
}

#[derive(Copy, Clone, Debug, Default)]
pub struct ValidationTransitions {
    pub unknown_to_valid: i32,
    pub unknown_to_invalid: i32,
    pub unknown_to_no_validator: i32,
    pub valid_to_invalid: i32,
    pub valid_to_no_validator: i32,
    pub no_validator_to_valid: i32,
    pub no_validator_to_invalid: i32,
}

#[repr(i64)]
#[derive(Debug, PartialEq, Eq, Copy, Clone, Serialize, Deserialize)]
pub enum ChangeType {
    Add = 0,
    Delete = 1,
    Modify = 2,
    NoChange = 3,
}

impl ChangeType {
    pub fn as_i64(&self) -> i64 {
        *self as i64
    }

    pub fn from_i64(value: i64) -> Self {
        match value {
            0 => ChangeType::Add,
            1 => ChangeType::Delete,
            2 => ChangeType::Modify,
            3 => ChangeType::NoChange,
            _ => ChangeType::Add, // Default for invalid values
        }
    }

    pub fn short_name(&self) -> &'static str {
        match self {
            ChangeType::Add => "A",
            ChangeType::Delete => "D",
            ChangeType::Modify => "M",
            ChangeType::NoChange => "N",
        }
    }

    pub fn full_name(&self) -> &'static str {
        match self {
            ChangeType::Add => "Add",
            ChangeType::Delete => "Delete",
            ChangeType::Modify => "Modify",
            ChangeType::NoChange => "No Change",
        }
    }

    pub fn from_string(s: &str) -> Option<Self> {
        match s.to_ascii_uppercase().as_str() {
            // Full names
            "ADD" => Some(ChangeType::Add),
            "DELETE" => Some(ChangeType::Delete),
            "MODIFY" => Some(ChangeType::Modify),
            "NO CHANGE" | "NOCHANGE" => Some(ChangeType::NoChange),
            // Short names
            "A" => Some(ChangeType::Add),
            "D" => Some(ChangeType::Delete),
            "M" => Some(ChangeType::Modify),
            "N" => Some(ChangeType::NoChange),
            _ => None,
        }
    }
}

impl std::fmt::Display for ChangeType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.full_name())
    }
}

impl crate::query::QueryEnum for ChangeType {
    fn from_token(s: &str) -> Option<i64> {
        Self::from_string(s).map(|change_type| change_type.as_i64())
    }
}

impl Change {
    // TODO: Implement accessors for other fields
    #[allow(dead_code)]
    pub fn hash_old(&self) -> Option<&str> {
        self.hash_old.as_deref()
    }
    #[allow(dead_code)]
    pub fn val_old(&self) -> Option<i64> {
        self.val_old
    }
    #[allow(dead_code)]
    pub fn val_new(&self) -> Option<i64> {
        self.val_new
    }

    pub fn get_validation_transitions_for_scan(
        db: &Database,
        scan_id: i64,
    ) -> Result<ValidationTransitions, FsPulseError> {
        let conn = db.conn();
        // TODO: This is unnecessarily complex now that we have old and new validation states in the change record
        let sql = "SELECT
                COALESCE(SUM(CASE
                    WHEN c.change_type IN (0,2)
                        AND COALESCE(c.val_old, 0) = 0
                        AND i.val = 1
                    THEN 1 ELSE 0 END), 0) AS unknown_to_valid,
                COALESCE(SUM(CASE
                    WHEN c.change_type IN (0,2)
                        AND COALESCE(c.val_old, 0) = 0
                        AND i.val = 2
                    THEN 1 ELSE 0 END), 0) AS unknown_to_invalid,
                COALESCE(SUM(CASE
                    WHEN c.change_type IN (0,2)
                        AND COALESCE(c.val_old, 0) = 0
                        AND i.val = 3
                    THEN 1 ELSE 0 END), 0) AS unknown_to_no_validator,
                COALESCE(SUM(CASE
                    WHEN c.change_type IN (0,2)
                        AND COALESCE(c.val_old, 0) = 1
                        AND i.val = 2
                    THEN 1 ELSE 0 END), 0) AS valid_to_invalid,
                COALESCE(SUM(CASE
                    WHEN c.change_type IN (0,2)
                        AND COALESCE(c.val_old, 0) = 1
                        AND i.val = 3
                    THEN 1 ELSE 0 END), 0) AS valid_to_no_validator,
                COALESCE(SUM(CASE
                    WHEN c.change_type IN (0,2)
                        AND COALESCE(c.val_old, 0) = 3
                        AND i.val = 1
                    THEN 1 ELSE 0 END), 0) AS no_validator_to_valid,
                COALESCE(SUM(CASE
                    WHEN c.change_type IN (0,2)
                        AND COALESCE(c.val_old, 0) = 3
                        AND i.val = 2
                    THEN 1 ELSE 0 END), 0) AS no_validator_to_invalid
            FROM changes c
                JOIN items i ON c.item_id = i.item_id
            WHERE c.scan_id = ?
                AND i.item_type = 0
                AND i.is_ts = 0";

        let validation_transitions = conn.query_row(sql, params![scan_id], |row| {
            Ok(ValidationTransitions {
                unknown_to_valid: row.get(0)?,
                unknown_to_invalid: row.get(1)?,
                unknown_to_no_validator: row.get(2)?,
                valid_to_invalid: row.get(3)?,
                valid_to_no_validator: row.get(4)?,
                no_validator_to_valid: row.get(5)?,
                no_validator_to_invalid: row.get(6)?,
            })
        })?;

        Ok(validation_transitions)
    }
}

impl ChangeCounts {
    pub fn get_by_scan_id(db: &Database, scan_id: i64) -> Result<Self, FsPulseError> {
        let conn = db.conn();
        let mut change_counts = ChangeCounts::default();

        let mut stmt = conn.prepare(
            "SELECT change_type, COUNT(*) FROM changes WHERE scan_id = ? GROUP BY change_type",
        )?;

        let mut rows = stmt.query([scan_id])?;

        while let Some(row) = rows.next()? {
            let change_type_value: i64 = row.get(0)?;
            let count: i64 = row.get(1)?;

            let change_type = ChangeType::from_i64(change_type_value);

            match change_type {
                ChangeType::Add => change_counts.set_count_of(ChangeType::Add, count),
                ChangeType::Delete => change_counts.set_count_of(ChangeType::Delete, count),
                ChangeType::Modify => change_counts.set_count_of(ChangeType::Modify, count),
                ChangeType::NoChange => change_counts.set_count_of(ChangeType::NoChange, count),
            }
        }

        Ok(change_counts)
    }

    pub fn set_count_of(&mut self, change_type: ChangeType, count: i64) {
        let target = match change_type {
            ChangeType::Add => &mut self.add_count,
            ChangeType::Delete => &mut self.delete_count,
            ChangeType::Modify => &mut self.modify_count,
            ChangeType::NoChange => &mut self.no_change_count,
        };
        *target = count;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_change_type_short_name() {
        assert_eq!(ChangeType::Add.short_name(), "A");
        assert_eq!(ChangeType::Delete.short_name(), "D");
        assert_eq!(ChangeType::Modify.short_name(), "M");
        assert_eq!(ChangeType::NoChange.short_name(), "N");
    }
    
    #[test]
    fn test_change_type_display() {
        assert_eq!(ChangeType::Add.to_string(), "Add");
        assert_eq!(ChangeType::Delete.to_string(), "Delete");
        assert_eq!(ChangeType::Modify.to_string(), "Modify");
        assert_eq!(ChangeType::NoChange.to_string(), "No Change");
    }
    
    #[test]
    fn test_change_type_from_string() {
        assert_eq!(ChangeType::from_string("A"), Some(ChangeType::Add));
        assert_eq!(ChangeType::from_string("D"), Some(ChangeType::Delete));
        assert_eq!(ChangeType::from_string("M"), Some(ChangeType::Modify));
        assert_eq!(ChangeType::from_string("N"), Some(ChangeType::NoChange));
        assert_eq!(ChangeType::from_string("Add"), Some(ChangeType::Add));
        assert_eq!(ChangeType::from_string("DELETE"), Some(ChangeType::Delete));

        // Test invalid parse
        assert_eq!(ChangeType::from_string("X"), None);
        assert_eq!(ChangeType::from_string(""), None);
    }

    #[test]
    fn test_change_type_round_trip() {
        let types = [ChangeType::Add, ChangeType::Delete, ChangeType::Modify, ChangeType::NoChange];

        for change_type in types {
            let str_val = change_type.short_name();
            let parsed_back = ChangeType::from_string(str_val).unwrap();
            assert_eq!(change_type, parsed_back, "Round trip failed for {change_type:?}");
        }
    }
    
    #[test]
    fn test_change_counts_default() {
        let counts = ChangeCounts::default();
        
        assert_eq!(counts.add_count, 0);
        assert_eq!(counts.modify_count, 0);
        assert_eq!(counts.delete_count, 0);
        assert_eq!(counts.no_change_count, 0);
    }
    
    #[test]
    fn test_change_counts_set_count_of() {
        let mut counts = ChangeCounts::default();
        
        // Test setting each count type
        counts.set_count_of(ChangeType::Add, 10);
        assert_eq!(counts.add_count, 10);
        assert_eq!(counts.modify_count, 0); // Others unchanged
        
        counts.set_count_of(ChangeType::Delete, 5);
        assert_eq!(counts.delete_count, 5);
        assert_eq!(counts.add_count, 10); // Previous value preserved
        
        counts.set_count_of(ChangeType::Modify, 20);
        assert_eq!(counts.modify_count, 20);
        
        counts.set_count_of(ChangeType::NoChange, 100);
        assert_eq!(counts.no_change_count, 100);
        
        // Test overwriting existing values
        counts.set_count_of(ChangeType::Add, 99);
        assert_eq!(counts.add_count, 99);
    }
    
    #[test]
    fn test_validation_transitions_default() {
        let transitions = ValidationTransitions::default();
        
        assert_eq!(transitions.unknown_to_valid, 0);
        assert_eq!(transitions.unknown_to_invalid, 0);
        assert_eq!(transitions.unknown_to_no_validator, 0);
        assert_eq!(transitions.valid_to_invalid, 0);
        assert_eq!(transitions.valid_to_no_validator, 0);
        assert_eq!(transitions.no_validator_to_valid, 0);
        assert_eq!(transitions.no_validator_to_invalid, 0);
    }
    
    #[test]
    fn test_change_type_copy_clone() {
        let change_type = ChangeType::Add;
        let change_type_copy = change_type;
        let change_type_clone = change_type;
        
        // All should be equal
        assert_eq!(change_type, change_type_copy);
        assert_eq!(change_type, change_type_clone);
        assert_eq!(change_type_copy, change_type_clone);
    }
    
    #[test]
    fn test_change_counts_copy_clone() {
        let counts = ChangeCounts {
            add_count: 5,
            modify_count: 10,
            ..ChangeCounts::default()
        };
        
        let counts_copy = counts;
        let counts_clone = counts;
        
        // All should have the same values
        assert_eq!(counts.add_count, counts_copy.add_count);
        assert_eq!(counts.modify_count, counts_clone.modify_count);
        assert_eq!(counts_copy.add_count, counts_clone.add_count);
    }
}
