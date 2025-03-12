use std::str::FromStr;

use log::info;
use rusqlite::OptionalExtension;

use crate::database::Database;
use crate::error::FsPulseError;

#[derive(Clone, Debug, Default)]
pub struct Change {
    pub id: i64,
    #[allow(dead_code)]
    pub scan_id: i64,   // scan_id is currently set but is never read
    pub item_id: i64,
    pub change_type: String,
    pub metadata_changed: Option<bool>,
    pub hash_changed: Option<bool>,
    pub prev_last_modified: Option<i64>,
    pub prev_file_size: Option<i64>,
    pub prev_hash: Option<String>,

    // Additional non-entity fields
    pub item_type: String,
    pub item_path: String,
}


#[derive(Copy, Clone, Debug, Default)]
pub struct ChangeCounts {
    pub add_count: i64,
    pub modify_count: i64,
    pub delete_count: i64,
    pub type_change_count: i64,
    pub no_change_count: i64,
}

#[derive(Debug, PartialEq, Eq, Copy, Clone)]
pub enum ChangeType {
    Add,
    Delete,
    Modify,
    TypeChange,
    NoChange,
}

impl ChangeType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Add => "A",
            Self::Delete => "D",
            Self::Modify => "M",
            Self::TypeChange => "T",
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
            "T" => Ok(Self::TypeChange),
            "N" => Ok(Self::NoChange),
            _ => Err(FsPulseError::Error(format!("Invalid change type: '{}'", s))), 
        }
    }
}

impl Change {
    pub fn get_by_id(db: &Database, change_id: i64) -> Result<Option<Self>, FsPulseError> {
        let conn = &db.conn;
    
        conn.query_row(
            "SELECT items.item_type, items.path, changes.id, changes.scan_id, changes.item_id, changes.change_type, 
                    changes.metadata_changed, changes.hash_changed, changes.prev_last_modified, changes.prev_file_size, changes.prev_hash
            FROM changes
            JOIN items ON items.id = changes.item_id
            WHERE changes.id = ?", 
            [change_id], 
            |row| Ok(Change {
                id: row.get(2)?,  
                scan_id: row.get(3)?,  
                item_id: row.get(4)?,  
                change_type: row.get(5)?,  
                metadata_changed: row.get(6)?,  
                hash_changed: row.get(7)?,  
                prev_last_modified: row.get(8)?,  
                prev_file_size: row.get(9)?,  
                prev_hash: row.get(10)?,  
                item_type: row.get(0)?,  
                item_path: row.get(1)?  
            })
        )
        .optional()
        .map_err(FsPulseError::Database)
    }

    pub fn for_each_change_in_scan<F>(db: &Database, scan_id: i64, mut func: F) -> Result<(), FsPulseError>
    where
        F: FnMut(&Change) -> Result<(), FsPulseError>,
    {
        let mut _change_count = 0;  // used only for logging

        let mut stmt = db.conn.prepare(
            "SELECT items.item_type, items.path, changes.id, changes.scan_id, changes.item_id, changes.change_type, changes.metadata_changed, changes.hash_changed, changes.prev_last_modified, prev_file_size, prev_hash
            FROM changes
            JOIN items ON items.id = changes.item_id
            WHERE changes.scan_id = ?
            ORDER BY items.path ASC"
        )?;
        
        let rows = stmt.query_map([scan_id], |row| {
            Ok(
                 Change {
                    id: row.get::<_, i64>(2)?,                          // changes.id
                    scan_id: row.get::<_, i64>(3)?,                     // changes.scan_id
                    item_id: row.get::<_, i64>(4)?,                     // changes.item_id
                    change_type: row.get::<_, String>(5)?,              // changes.change_type
                    metadata_changed: row.get::<_, Option<bool>>(6)?,   // changes.metadata_changed
                    hash_changed: row.get::<_, Option<bool>>(7)?,       // changes.hash_changed
                    prev_last_modified: row.get::<_, Option<i64>>(8)?,  // changes.prev_last_modified
                    prev_file_size: row.get::<_, Option<i64>>(9)?,      // changes.prev_file_size
                    prev_hash: row.get::<_, Option<String>>(10)?,       // changes.prev_hash

                    // Additional fields
                    item_type: row.get::<_, String>(0)?,                // items.item_type
                    item_path: row.get::<_, String>(1)?,                // items.path
                }
            )
        })?;
        
        for row in rows {
            let change = row?;

            func(&change)?;
            _change_count += 1;
        }
        info!("for_each_scan_change - scan_id: {}, changes: {}", scan_id, _change_count);
        Ok(())
    }
}

impl ChangeCounts {
    pub fn new(add_count: i64, modify_count: i64, delete_count: i64, type_change_count: i64, no_change_count: i64) -> Self {
        Self {
            add_count,
            modify_count,
            delete_count,
            type_change_count,
            no_change_count,
        }
    }

    pub fn get_by_scan_id(db: &Database, scan_id: i64) -> Result<Self, FsPulseError> {
        let conn = &db.conn;
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
                ChangeType::TypeChange => change_counts.set_count_of(ChangeType::TypeChange, count),
            }
        }

        Ok(change_counts)
    }
    
    pub fn count_of(&self, change_type: ChangeType) -> i64 {
        match change_type {
            ChangeType::Add => self.add_count,
            ChangeType::Delete => self.delete_count,
            ChangeType::Modify => self.modify_count,
            ChangeType::TypeChange => self.type_change_count,
            ChangeType::NoChange => self.no_change_count,
        }
    }

    pub fn increment_count_of(&mut self, change_type: ChangeType) {
        let target = match change_type {
            ChangeType::Add => &mut self.add_count,
            ChangeType::Delete => &mut self.delete_count,
            ChangeType::Modify => &mut self.modify_count,
            ChangeType::TypeChange => &mut self.type_change_count,
            ChangeType::NoChange => &mut self.no_change_count,
       };
       *target += 1;
    }

    pub fn set_count_of(&mut self, change_type: ChangeType, count: i64) {
        let target = match change_type {
            ChangeType::Add => &mut self.add_count,
            ChangeType::Delete => &mut self.delete_count,
            ChangeType::Modify => &mut self.modify_count,
            ChangeType::TypeChange => &mut self.type_change_count,
            ChangeType::NoChange => &mut self.no_change_count,
       };
       *target = count;   
    }
}




