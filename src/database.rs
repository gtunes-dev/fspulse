use log::info;
use rusqlite::{Connection, OptionalExtension, Result};
use std::path::PathBuf;
use crate::error::FsPulseError;
use crate::schema::CREATE_SCHEMA_SQL;

const DB_FILENAME: &str = "fspulse.db";
const SCHEMA_VERSION: &str = "2";

#[derive(Debug)]
pub struct Database {
    conn: Option<Connection>,
    #[allow(dead_code)]
    path: String,
}

impl Default for Database {
    fn default() -> Self {
        Database {
            conn: None,
            path: String::new(),
        }
    }
}

impl Database {
    pub fn conn(&self) -> &Connection {
        self.conn.as_ref().expect("Expected a database connection")
    }

    pub fn conn_mut(&mut self) -> &mut Connection {
            self.conn.as_mut().expect("Expected a database connection")
    }

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
        info!("Opening database: {}", db_path.display());
        let conn = Connection::open(&db_path).map_err(FsPulseError::Database)?;

        let db = Self { conn: Some(conn), path: db_path.to_string_lossy().into_owned() };
        
        // Ensure schema is current
        db.ensure_schema()?;

        Ok(db)
    }

    #[allow(dead_code)]
    pub fn path(&self) -> &str {
        &self.path
    }

    fn ensure_schema(&self) -> Result<(), FsPulseError> {
        let table_exists: bool = self.conn()
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
        let stored_version: Option<String> = self.conn()
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
        self.conn().execute_batch(CREATE_SCHEMA_SQL)?;
        Ok(())
    }
}
    