use rusqlite::{Connection, OptionalExtension, Result};
use std::path::PathBuf;
use std::{io, path::Path};
use crate::error::FsPulseError;
use crate::schema::CREATE_SCHEMA_SQL;

const DB_FILENAME: &str = "fspulse.db";
const SCHEMA_VERSION: &str = "2";

#[derive(Debug, PartialEq)]
pub enum ItemType {
    File,
    Directory,
    Symlink,
    Other,
}

impl ItemType {
    pub fn as_db_str(&self) -> &'static str {
        match self {
            ItemType::File => "F",
            ItemType::Directory => "D",
            ItemType::Symlink => "S",
            ItemType::Other => "O",
        }
    }
}

pub struct Database {
    pub conn: Connection,
    #[allow(dead_code)]
    path: String,
}

impl Database {
    pub fn new(db_path: Option<PathBuf>) -> Result<Self, FsPulseError>
    {
        let mut db_path = db_path
            .or_else(dirs::home_dir)
            .ok_or_else(|| FsPulseError::Error("Could not determine home directory".to_string()))?;

        if !db_path.is_dir() {
            return Err(FsPulseError::Error(format!(
                "Database folder '{}' does not exist or is not a directory", 
                db_path.display()
            )));
        }

        db_path.push(DB_FILENAME);

        // Attempt to open the database
        let conn = Connection::open(&db_path).map_err(FsPulseError::Database)?;

        // println!("Database opened at: {}", db_path.display());
        let db = Self { conn, path: db_path.to_string_lossy().into_owned() };
        
        // Ensure schema is current
        db.ensure_schema()?;

        Ok(db)
    }

    pub fn new_old(db_folder: &str) -> Result<Self, FsPulseError> {
        let folder_path = Path::new(db_folder);
        
        // Ensure the folder exists and is a directory
        if !folder_path.is_dir() {
            return Err(io::Error::new(
                io::ErrorKind::NotADirectory, 
                format!("Database folder '{}' does not exist or is not a directory", db_folder)
            ).into());
        }

        let db_path = folder_path.join(DB_FILENAME);

        // Attempt to open the database
        let conn = Connection::open(&db_path).map_err(FsPulseError::Database)?;
        // println!("Database opened at: {}", db_path.display());
        let db = Self { conn, path: db_path.to_string_lossy().into_owned() };
        
        // Ensure schema is current
        db.ensure_schema()?;

        Ok(db)
    }

    #[allow(dead_code)]
    pub fn path(&self) -> &str {
        &self.path
    }

    fn ensure_schema(&self) -> Result<(), FsPulseError> {
        let table_exists: bool = self.conn
            .query_row(
                "SELECT count(*) FROM sqlite_master WHERE type='table' AND name='meta'",
                [],
                |row| row.get::<_, i32>(0),
            )
            .map(|count| count > 0)
            .unwrap_or(false);

        if !table_exists {
            return self.create_schema();
        }

        // Get the stored schema version
        let stored_version: Option<String> = self.conn
            .query_row(
                "SELECT value FROM meta WHERE key = 'schema_version'",
                [],
                |row| row.get(0),
            )
            .optional()?;

            match stored_version.as_deref() {
                Some(SCHEMA_VERSION) => Ok(()), // Schema is up to date
                Some(_) => Err(FsPulseError::Error("Schema version mismatch".to_string())),
                None => Err(FsPulseError::Error("Schema version missing".to_string())),
            }
    }
    
    fn create_schema(&self) -> Result<(), FsPulseError> {
        self.conn.execute_batch(CREATE_SCHEMA_SQL)?;
        Ok(())
    }
}
    