use rusqlite::{Connection, OptionalExtension, Result};
use std::{io, path::Path};
use crate::error::DirCheckError;
use crate::schema::CREATE_SCHEMA_SQL;

const DB_FILENAME: &str = "dircheck.db";
const SCHEMA_VERSION: &str = "1";

pub struct Database {
    conn: Connection,
}

impl Database {
    pub fn connect(db_folder: &str) -> Result<Self, DirCheckError> {
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
        let conn = Connection::open(&db_path).map_err(DirCheckError::Database)?;
        println!("Database opened at: {}", db_path.display());
        let db = Self { conn };
        
        // Ensure schema is current
        db.ensure_schema()?;

        Ok(db)
    }

    fn ensure_schema(&self) -> Result<(), DirCheckError> {
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
                Some(_) => Err(DirCheckError::Error("Schema version mismatch".to_string())),
                None => Err(DirCheckError::Error("Schema version missing".to_string())),
            }
    }
    
    fn create_schema(&self) -> Result<(), DirCheckError> {
        self.conn.execute_batch(CREATE_SCHEMA_SQL)?;
        Ok(())
    }  
}