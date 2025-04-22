use std::str::FromStr;

use rusqlite::params;

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

    pub fn short_str_to_full(s: &str) -> Result<&str, FsPulseError> {
        match s {
            "A" => Ok("Add"),
            "D" => Ok("Delete"),
            "M" => Ok("Modify"),
            "N" => Ok("No Change"),
            _ => Err(FsPulseError::Error(format!("Invalid change type: '{}'", s))),
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

            let change_type = ChangeType::from_str(&change_type)?;

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
