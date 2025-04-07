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
        changes.prev_modified,
        prev_size,
        hash_changed,
        prev_last_hash_scan_id,
        prev_hash,
        validation_changed,
        validation_state,
        prev_last_validation_scan_id,
        prev_validation_state,
        prev_validation_error
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
        changes.prev_modified,
        prev_size,
        hash_changed,
        prev_last_hash_scan_id,
        prev_hash,
        validation_changed,
        validation_state,
        prev_last_validation_scan_id,
        prev_validation_state,
        prev_validation_error
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
    pub prev_modified: Option<i64>,                 // Meaningful if undelete or metadata_changed
    pub prev_size: Option<i64>,                     // Meaningful if undelete or metadata_changed
    pub hash_changed: Option<bool>,                 // Present if "M". True if hash changed, else False
    #[allow(dead_code)]
    pub prev_last_hash_scan_id: Option<i64>,        // Present if "M" and hash_changed
    pub prev_hash: Option<String>,                  // Meaningful if undelete or hash_changed
    pub validation_changed: Option<bool>,           // Present if "M", True if validation changed, else False
    pub validation_state: Option<String>,           // Validation state of the item if validation_changed = true
    #[allow(dead_code)]
    pub prev_last_validation_scan_id: Option<i64>,  // Present if "M" and validation changed
    pub prev_validation_state : Option<String>,     // Meaningful if undelete or validation_changed
    #[allow(dead_code)]
    pub prev_validation_error: Option<String>,      // Meaningful if undelete validation_changed
    
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
    pub fn prev_hash(&self) -> Option<&str> { self.prev_hash.as_deref() }
    pub fn validation_state(&self) -> Option<&str> { self.validation_state.as_deref()}
    pub fn prev_validation_state(&self) -> Option<&str> {self.prev_validation_state.as_deref()}

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
                prev_modified,
                prev_size,
                hash_changed,
                prev_last_hash_scan_id,
                prev_hash,
                validation_changed,
                validation_state,
                prev_last_validation_scan_id,
                prev_validation_state,
                prev_validation_error
            FROM changes
            JOIN items ON items.id = changes.item_id
            WHERE changes.id = ?", 
            [change_id], 
            |row| Ok(Change {
                item_type: row.get(0)?,
                item_path: row.get(1)?,
                id: row.get(2)?,
                scan_id: row.get(3)?,
                item_id: row.get(4)?,
                change_type: row.get(5)?,
                is_undelete: row.get(6)?,
                metadata_changed: row.get(7)?,
                prev_modified: row.get(8)?,
                prev_size: row.get(9)?,
                hash_changed: row.get(10)?,
                prev_last_hash_scan_id: row.get(11)?,
                prev_hash: row.get(12)?,
                validation_changed: row.get(13)?,
                validation_state: row.get(14)?,
                prev_last_validation_scan_id: row.get(15)?,
                prev_validation_state: row.get(16)?,
                prev_validation_error: row.get(17)?
            })
        )
        .optional()
        .map_err(FsPulseError::Database)
    }

    pub fn get_validation_transitions_for_scan(db: &Database, scan_id: i64) -> Result<ValidationTransitions, FsPulseError> {
        let conn = db.conn();
        let sql = 
            "SELECT 
                COALESCE(SUM(CASE 
                    WHEN c.change_type IN ('A','M')
                        AND COALESCE(c.prev_validation_state, 'U') = 'U'
                        AND i.validation_state = 'V'
                    THEN 1 ELSE 0 END), 0) AS unknown_to_valid,
                COALESCE(SUM(CASE 
                    WHEN c.change_type IN ('A','M')
                        AND COALESCE(c.prev_validation_state, 'U') = 'U'
                        AND i.validation_state = 'I'
                    THEN 1 ELSE 0 END), 0) AS unknown_to_invalid,
                COALESCE(SUM(CASE 
                    WHEN c.change_type IN ('A','M')
                        AND COALESCE(c.prev_validation_state, 'U') = 'U'
                        AND i.validation_state = 'N'
                    THEN 1 ELSE 0 END), 0) AS unknown_to_no_validator,
                COALESCE(SUM(CASE 
                    WHEN c.change_type IN ('A','M')
                        AND COALESCE(c.prev_validation_state, 'U') = 'V'
                        AND i.validation_state = 'I'
                    THEN 1 ELSE 0 END), 0) AS valid_to_invalid,
                COALESCE(SUM(CASE 
                    WHEN c.change_type IN ('A','M')
                        AND COALESCE(c.prev_validation_state, 'U') = 'V'
                        AND i.validation_state = 'N'
                    THEN 1 ELSE 0 END), 0) AS valid_to_no_validator,
                COALESCE(SUM(CASE 
                    WHEN c.change_type IN ('A','M')
                        AND COALESCE(c.prev_validation_state, 'U') = 'N'
                        AND i.validation_state = 'V'
                    THEN 1 ELSE 0 END), 0) AS no_validator_to_valid,
                COALESCE(SUM(CASE 
                    WHEN c.change_type IN ('A','M')
                        AND COALESCE(c.prev_validation_state, 'U') = 'N'
                        AND i.validation_state = 'I'
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
                    prev_modified: row.get(8)?,
                    prev_size: row.get(9)?,
                    hash_changed: row.get(10)?,
                    prev_last_hash_scan_id: row.get(11)?,
                    prev_hash: row.get(12)?,
                    validation_changed: row.get(13)?,
                    validation_state: row.get(14)?,
                    prev_last_validation_scan_id: row.get(15)?,
                    prev_validation_state: row.get(16)?,
                    prev_validation_error: row.get(17)?
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




