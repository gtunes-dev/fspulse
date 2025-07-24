use rusqlite::params;
use strum_macros::{AsRefStr, Display, EnumIter, EnumString};

use crate::database::Database;
use crate::error::FsPulseError;

#[derive(Clone, Debug, Default)]
#[allow(dead_code)]
pub struct Change {
    pub change_id: i64,
    pub scan_id: i64,
    pub item_id: i64,
    pub change_type: String,
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
    pub val_old: Option<String>,   // Validation state of the item if val_change = true
    #[allow(dead_code)]
    pub val_new: Option<String>, // Meaningful if undelete or val_change
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

#[derive(AsRefStr, EnumIter, EnumString, Debug, Display, PartialEq, Eq, Copy, Clone)]
pub enum ChangeType {
    #[strum(serialize = "A")]
    Add,
    #[strum(serialize = "D")]
    Delete,
    #[strum(serialize = "M")]
    Modify,
    #[strum(serialize = "N")]
    NoChange,
}

impl ChangeType {
    pub fn long_name(&self) -> &'static str {
        match self {
            ChangeType::Add => "Add",
            ChangeType::Delete => "Delete",
            ChangeType::Modify => "Modify",
            ChangeType::NoChange => "No Change",
        }
    }
}

/* 
impl std::fmt::Display for ChangeType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}
    */

/* 

impl FromStr for ChangeType {
    type Err = FsPulseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "A" => Ok(Self::Add),
            "D" => Ok(Self::Delete),
            "M" => Ok(Self::Modify),
            "N" => Ok(Self::NoChange),
            _ => Err(FsPulseError::Error(format!("Invalid change type: '{}'", s))),
        }
    }
}
    */

impl Change {
    // TODO: Implement accessors for other fields
    #[allow(dead_code)]
    pub fn hash_old(&self) -> Option<&str> {
        self.hash_old.as_deref()
    }
    #[allow(dead_code)]
    pub fn val_old(&self) -> Option<&str> {
        self.val_old.as_deref()
    }
    #[allow(dead_code)]
    pub fn val_new(&self) -> Option<&str> {
        self.val_new.as_deref()
    }

    pub fn get_validation_transitions_for_scan(
        db: &Database,
        scan_id: i64,
    ) -> Result<ValidationTransitions, FsPulseError> {
        let conn = db.conn();
        // TODO: This is unnecessarily complex now that we have old and new validation states in the change record
        let sql = "SELECT 
                COALESCE(SUM(CASE 
                    WHEN c.change_type IN ('A','M')
                        AND COALESCE(c.val_old, 'U') = 'U'
                        AND i.val = 'V'
                    THEN 1 ELSE 0 END), 0) AS unknown_to_valid,
                COALESCE(SUM(CASE 
                    WHEN c.change_type IN ('A','M')
                        AND COALESCE(c.val_old, 'U') = 'U'
                        AND i.val = 'I'
                    THEN 1 ELSE 0 END), 0) AS unknown_to_invalid,
                COALESCE(SUM(CASE 
                    WHEN c.change_type IN ('A','M')
                        AND COALESCE(c.val_old, 'U') = 'U'
                        AND i.val = 'N'
                    THEN 1 ELSE 0 END), 0) AS unknown_to_no_validator,
                COALESCE(SUM(CASE 
                    WHEN c.change_type IN ('A','M')
                        AND COALESCE(c.val_old, 'U') = 'V'
                        AND i.val = 'I'
                    THEN 1 ELSE 0 END), 0) AS valid_to_invalid,
                COALESCE(SUM(CASE 
                    WHEN c.change_type IN ('A','M')
                        AND COALESCE(c.val_old, 'U') = 'V'
                        AND i.val = 'N'
                    THEN 1 ELSE 0 END), 0) AS valid_to_no_validator,
                COALESCE(SUM(CASE 
                    WHEN c.change_type IN ('A','M')
                        AND COALESCE(c.val_old, 'U') = 'N'
                        AND i.val = 'V'
                    THEN 1 ELSE 0 END), 0) AS no_validator_to_valid,
                COALESCE(SUM(CASE 
                    WHEN c.change_type IN ('A','M')
                        AND COALESCE(c.val_old, 'U') = 'N'
                        AND i.val = 'I'
                    THEN 1 ELSE 0 END), 0) AS no_validator_to_invalid
            FROM changes c
                JOIN items i ON c.item_id = i.item_id
            WHERE c.scan_id = ?
                AND i.item_type = 'F'
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
            let change_type: String = row.get(0)?;
            let count: i64 = row.get(1)?;

            let change_type = change_type.parse()?;

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
    use strum::IntoEnumIterator;
    
    #[test]
    fn test_change_type_as_ref_str() {
        assert_eq!(ChangeType::Add.as_ref(), "A");
        assert_eq!(ChangeType::Delete.as_ref(), "D");
        assert_eq!(ChangeType::Modify.as_ref(), "M");
        assert_eq!(ChangeType::NoChange.as_ref(), "N");
    }
    
    #[test]
    fn test_change_type_display() {
        assert_eq!(ChangeType::Add.to_string(), "A");
        assert_eq!(ChangeType::Delete.to_string(), "D");
        assert_eq!(ChangeType::Modify.to_string(), "M");
        assert_eq!(ChangeType::NoChange.to_string(), "N");
    }
    
    #[test]
    fn test_change_type_from_str() {
        assert_eq!("A".parse::<ChangeType>().unwrap(), ChangeType::Add);
        assert_eq!("D".parse::<ChangeType>().unwrap(), ChangeType::Delete);
        assert_eq!("M".parse::<ChangeType>().unwrap(), ChangeType::Modify);
        assert_eq!("N".parse::<ChangeType>().unwrap(), ChangeType::NoChange);
        
        // Test invalid parse
        assert!("X".parse::<ChangeType>().is_err());
        assert!("".parse::<ChangeType>().is_err());
        assert!("Add".parse::<ChangeType>().is_err()); // Should be "A", not "Add"
    }
    
    #[test]
    fn test_change_type_long_name() {
        assert_eq!(ChangeType::Add.long_name(), "Add");
        assert_eq!(ChangeType::Delete.long_name(), "Delete");
        assert_eq!(ChangeType::Modify.long_name(), "Modify");
        assert_eq!(ChangeType::NoChange.long_name(), "No Change");
    }
    
    #[test]
    fn test_change_type_enum_iter() {
        let all_types: Vec<ChangeType> = ChangeType::iter().collect();
        assert_eq!(all_types.len(), 4);
        assert!(all_types.contains(&ChangeType::Add));
        assert!(all_types.contains(&ChangeType::Delete));
        assert!(all_types.contains(&ChangeType::Modify));
        assert!(all_types.contains(&ChangeType::NoChange));
    }
    
    #[test]
    fn test_change_type_round_trip() {
        let types = [ChangeType::Add, ChangeType::Delete, ChangeType::Modify, ChangeType::NoChange];
        
        for change_type in types {
            let str_val = change_type.as_ref();
            let parsed_back = str_val.parse::<ChangeType>().unwrap();
            assert_eq!(change_type, parsed_back, "Round trip failed for {change_type:?}");
        }
    }
    
    #[test]
    fn test_change_default() {
        let change = Change::default();
        
        assert_eq!(change.change_id, 0);
        assert_eq!(change.scan_id, 0);
        assert_eq!(change.item_id, 0);
        assert_eq!(change.change_type, "");
        assert_eq!(change.is_undelete, None);
        assert_eq!(change.meta_change, None);
        assert_eq!(change.mod_date_old, None);
        assert_eq!(change.mod_date_new, None);
        assert_eq!(change.file_size_old, None);
        assert_eq!(change.file_size_new, None);
        assert_eq!(change.hash_change, None);
        assert_eq!(change.hash_old, None);
        assert_eq!(change.val_change, None);
        assert_eq!(change.val_old, None);
        assert_eq!(change.item_type, "");
        assert_eq!(change.item_path, "");
    }
    
    #[test]
    fn test_change_accessor_methods() {
        let mut change = Change::default();
        
        // Test None values
        assert_eq!(change.hash_old(), None);
        assert_eq!(change.val_old(), None);
        assert_eq!(change.val_new(), None);
        
        // Test Some values
        change.hash_old = Some("abc123".to_string());
        change.val_old = Some("V".to_string());
        change.val_new = Some("I".to_string());
        
        assert_eq!(change.hash_old(), Some("abc123"));
        assert_eq!(change.val_old(), Some("V"));
        assert_eq!(change.val_new(), Some("I"));
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
    
    #[test]
    fn test_change_clone() {
        let change = Change {
            change_id: 123,
            scan_id: 456,
            hash_old: Some("test_hash".to_string()),
            item_path: "/test/path".to_string(),
            ..Change::default()
        };
        
        let change_clone = change.clone();
        
        assert_eq!(change.change_id, change_clone.change_id);
        assert_eq!(change.scan_id, change_clone.scan_id);
        assert_eq!(change.hash_old(), change_clone.hash_old());
        assert_eq!(change.item_path, change_clone.item_path);
    }
}
