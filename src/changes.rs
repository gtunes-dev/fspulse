use std::str::FromStr;

use log::info;
use rusqlite::{params, OptionalExtension};

use crate::database::Database;
use crate::error::FsPulseError;

const SQL_FOR_EACH_CHANGE_IN_SCAN: &str = 
    "SELECT 
        items.item_type,
        items.path,
        id,
        scan_id,
        item_id,
        change_type,
        is_undelete,
        metadata_changed,
        changes.mod_date_old,
        changes.mod_date_new,
        file_size_old,
        file_size_new,
        hash_changed,
        last_hash_scan_id_old,
        hash_old,
        hash_new,
        validity_changed,
        last_validation_scan_id_old,
        validity_state_old,
        validity_state_new,
        validation_error_old,
        validation_error_new
    FROM changes
    JOIN items ON items.id = changes.item_id
    WHERE changes.scan_id = ?
    ORDER BY items.path ASC";

const SQL_FOR_EACH_CHANGE_IN_ITEM: &str = 
    "SELECT 
        items.item_type,
        items.path,
        id,
        scan_id,
        item_id,
        change_type,
        is_undelete,
        metadata_changed,
        changes.mod_date_old,
        changes.mod_date_new,
        file_size_old,
        file_size_new,
        hash_changed,
        last_hash_scan_id_old,
        hash_old,
        hash_new,
        validity_changed,
        last_validation_scan_id_old,
        validation_state_old,
        validation_state_new,
        validation_error_old,
        validation_error_new
    FROM changes
    JOIN items ON items.id = changes.item_id
    WHERE changes.item_id = ?
    ORDER BY changes.id ASC";

#[derive(Clone, Debug, Default)]
pub struct Change {
    pub id: i64,
    pub scan_id: i64,
    pub item_id: i64,
    pub change_type: String,
    pub is_undelete: Option<bool>,                  // Present if "A". True if add is undelete
    pub metadata_changed: Option<bool>,             // Present if "M". True if metadata changed, else False
    pub mod_date_old: Option<i64>,                 // Meaningful if undelete or metadata_changed
    pub mod_date_new: Option<i64>,                  // Meaningful if metdata_changed
    pub file_size_old: Option<i64>,                // Meaningful if undelete or metadata_changed
    pub file_size_new: Option<i64>,                // Meaningful if undelete or metadata_changed
    pub hash_changed: Option<bool>,                 // Present if "M". True if hash changed, else False
    #[allow(dead_code)]
    pub last_hash_scan_id_old: Option<i64>,        // Present if "M" and hash_changed
    pub hash_old: Option<String>,                  // Meaningful if undelete or hash_changed
    #[allow(dead_code)]
    pub hash_new: Option<String>,                   // Meaningful if hash_changed
    pub validity_changed: Option<bool>,             // Present if "M", True if validation changed, else False
    #[allow(dead_code)]
    pub last_validation_scan_id_old: Option<i64>,  // Present if "M" and validation changed
    pub validity_state_new: Option<String>,           // Validation state of the item if validity_changed = true
    #[allow(dead_code)]
    pub validity_state_old : Option<String>,     // Meaningful if undelete or validity_changed
    #[allow(dead_code)]
    pub validation_error_old: Option<String>,      // Meaningful if undelete or validity_changed
    #[allow(dead_code)]
    pub validation_error_new: Option<String>,       // Meaningful if validity changed
    
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

#[derive(Debug, PartialEq, Eq, Copy, Clone)]
pub enum ChangeType {
    Add,
    Delete,
    Modify,
    NoChange,
}

impl ChangeType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Add => "A",
            Self::Delete => "D",
            Self::Modify => "M",
            Self::NoChange => "N",
        }
    }
}

impl std::fmt::Display for ChangeType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

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

impl Change {
    // TODO: Implement accessors for other fields
    pub fn hash_old(&self) -> Option<&str> { self.hash_old.as_deref() }
    pub fn validity_state_old(&self) -> Option<&str> {self.validity_state_old.as_deref()}
    pub fn validity_state_new(&self) -> Option<&str> { self.validity_state_new.as_deref()}
    
    pub fn get_by_id(db: &Database, change_id: i64) -> Result<Option<Self>, FsPulseError> {
        let conn = db.conn();
    
        conn.query_row(
            "SELECT 
                items.item_type, 
                items.path, 
                id,
                scan_id,
                item_id,
                change_type,
                is_undelete,
                metadata_changed,
                mod_date_old,
                mod_date_new,
                file_size_old,
                file_size_new,
                hash_changed,
                last_hash_scan_id_old,
                hash_old,
                hash_new,
                validity_changed,
                last_validation_scan_id_old,
                validity_state_old,
                validity_state_new,
                validation_error_old,
                validation_error_new
            FROM changes
            JOIN items ON items.id = changes.item_id
            WHERE changes.id = ?", 
            [change_id], 
            |row| Ok(
                Change {
                    item_type: row.get(0)?,
                    item_path: row.get(1)?,
                    id: row.get(2)?,
                    scan_id: row.get(3)?,
                    item_id: row.get(4)?,
                    change_type: row.get(5)?,
                    is_undelete: row.get(6)?,
                    metadata_changed: row.get(7)?,
                    mod_date_old: row.get(8)?,
                    mod_date_new: row.get(9)?,
                    file_size_old: row.get(10)?,
                    file_size_new: row.get(11)?,
                    hash_changed: row.get(12)?,
                    last_hash_scan_id_old: row.get(13)?,
                    hash_old: row.get(14)?,
                    hash_new: row.get(15)?,
                    validity_changed: row.get(16)?,
                    last_validation_scan_id_old: row.get(17)?,
                    validity_state_old: row.get(18)?,
                    validity_state_new: row.get(19)?,
                    validation_error_old: row.get(20)?,
                    validation_error_new: row.get(21)?,
                }
            )
        )
        .optional()
        .map_err(FsPulseError::DatabaseError)
    }

    pub fn get_validation_transitions_for_scan(db: &Database, scan_id: i64) -> Result<ValidationTransitions, FsPulseError> {
        let conn = db.conn();
        // TODO: This is unnecessarily complex now that we have old and new validation states in the change record
        let sql = 
            "SELECT 
                COALESCE(SUM(CASE 
                    WHEN c.change_type IN ('A','M')
                        AND COALESCE(c.validity_state_old, 'U') = 'U'
                        AND i.validity_state = 'V'
                    THEN 1 ELSE 0 END), 0) AS unknown_to_valid,
                COALESCE(SUM(CASE 
                    WHEN c.change_type IN ('A','M')
                        AND COALESCE(c.validity_state_old, 'U') = 'U'
                        AND i.validity_state = 'I'
                    THEN 1 ELSE 0 END), 0) AS unknown_to_invalid,
                COALESCE(SUM(CASE 
                    WHEN c.change_type IN ('A','M')
                        AND COALESCE(c.validity_state_old, 'U') = 'U'
                        AND i.validity_state = 'N'
                    THEN 1 ELSE 0 END), 0) AS unknown_to_no_validator,
                COALESCE(SUM(CASE 
                    WHEN c.change_type IN ('A','M')
                        AND COALESCE(c.validity_state_old, 'U') = 'V'
                        AND i.validity_state = 'I'
                    THEN 1 ELSE 0 END), 0) AS valid_to_invalid,
                COALESCE(SUM(CASE 
                    WHEN c.change_type IN ('A','M')
                        AND COALESCE(c.validity_state_old, 'U') = 'V'
                        AND i.validity_state = 'N'
                    THEN 1 ELSE 0 END), 0) AS valid_to_no_validator,
                COALESCE(SUM(CASE 
                    WHEN c.change_type IN ('A','M')
                        AND COALESCE(c.validity_state_old, 'U') = 'N'
                        AND i.validity_state = 'V'
                    THEN 1 ELSE 0 END), 0) AS no_validator_to_valid,
                COALESCE(SUM(CASE 
                    WHEN c.change_type IN ('A','M')
                        AND COALESCE(c.validity_state_old, 'U') = 'N'
                        AND i.validity_state = 'I'
                    THEN 1 ELSE 0 END), 0) AS no_validator_to_invalid
            FROM changes c
                JOIN items i ON c.item_id = i.id
            WHERE c.scan_id = ?
                AND i.item_type = 'F'
                AND i.is_tombstone = 0;";

            let validation_transitions = conn.query_row(
                sql, 
                params![scan_id], 
                |row| {
                    Ok(ValidationTransitions {
                        unknown_to_valid: row.get(0)?,
                        unknown_to_invalid: row.get(1)?,
                        unknown_to_no_validator: row.get(2)?,
                        valid_to_invalid: row.get(3)?,
                        valid_to_no_validator: row.get(4)?,
                        no_validator_to_valid: row.get(5)?,
                        no_validator_to_invalid: row.get(6)?,
                    })
                },
            )?;

            Ok(validation_transitions)
    }

    pub fn for_each_change_in_scan<F>(db: &Database, scan_id: i64, func: F) -> Result<(), FsPulseError> 
    where
        F: FnMut(&Change) -> Result<(), FsPulseError>,   
    {
        Self::for_each_change_impl(db, SQL_FOR_EACH_CHANGE_IN_SCAN, scan_id, func)
    }

    pub fn for_each_change_in_item<F>(db: &Database, item_id: i64, func: F) -> Result<(), FsPulseError> 
    where
        F: FnMut(&Change) -> Result<(), FsPulseError>,   
    {
        Self::for_each_change_impl(db, SQL_FOR_EACH_CHANGE_IN_ITEM, item_id, func)
    }

    pub fn for_each_change_impl<F>(db: &Database, sql_query: &str, sql_query_param: i64, mut func: F) -> Result<(), FsPulseError>
    where
        F: FnMut(&Change) -> Result<(), FsPulseError>,
    {
        let mut _change_count = 0;  // used only for logging

        let mut stmt = db.conn().prepare(sql_query)?;
        
        let rows = stmt.query_map([sql_query_param], |row| {
            Ok(
                 Change {
                    item_type: row.get(0)?,
                    item_path: row.get(1)?,
                    id: row.get(2)?,
                    scan_id: row.get(3)?,
                    item_id: row.get(4)?,
                    change_type: row.get(5)?,
                    is_undelete: row.get(6)?,
                    metadata_changed: row.get(7)?,
                    mod_date_old: row.get(8)?,
                    mod_date_new: row.get(9)?,
                    file_size_old: row.get(10)?,
                    file_size_new: row.get(11)?,
                    hash_changed: row.get(12)?,
                    last_hash_scan_id_old: row.get(13)?,
                    hash_old: row.get(14)?,
                    hash_new: row.get(15)?,
                    validity_changed: row.get(16)?,
                    last_validation_scan_id_old: row.get(17)?,
                    validity_state_old: row.get(18)?,
                    validity_state_new: row.get(19)?,
                    validation_error_old: row.get(20)?,
                    validation_error_new: row.get(21)?,
                }
            )
        })?;
        
        for row in rows {
            let change = row?;

            func(&change)?;
            _change_count += 1;
        }
        info!("for_each_scan_change_impl - id: {}, changes: {}", sql_query_param, _change_count);
        Ok(())
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

            let change_type = ChangeType::from_str(&change_type)?;

            match change_type {
                ChangeType::Add => change_counts.set_count_of(ChangeType::Add, count),
                ChangeType::Delete => change_counts.set_count_of(ChangeType::Delete, count),
                ChangeType::Modify => change_counts.set_count_of(ChangeType::Modify, count),
                ChangeType::NoChange => change_counts.set_count_of(ChangeType::Modify, count),
            }
        }

        Ok(change_counts)
    }
    
    pub fn count_of(&self, change_type: ChangeType) -> i64 {
        match change_type {
            ChangeType::Add => self.add_count,
            ChangeType::Delete => self.delete_count,
            ChangeType::Modify => self.modify_count,
            ChangeType::NoChange => self.no_change_count,
        }
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




