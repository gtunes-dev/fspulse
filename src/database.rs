use crate::{
    error::FsPulseError,
    schema::{CREATE_SCHEMA_SQL, UPGRADE_2_TO_3_SQL, UPGRADE_3_TO_4_SQL},
};
use directories::BaseDirs;
use log::info;
use rusqlite::{Connection, OptionalExtension, Result};
use std::path::PathBuf;

const DB_FILENAME: &str = "fspulse.db";
const CURRENT_SCHEMA_VERSION: u32 = 4;

#[derive(Debug, Default)]
pub struct Database {
    conn: Option<Connection>,
    #[allow(dead_code)]
    path: String,
}

impl Database {
    pub fn conn(&self) -> &Connection {
        self.conn.as_ref().expect("Expected a database connection")
    }

    pub fn conn_mut(&mut self) -> &mut Connection {
        self.conn.as_mut().expect("Expected a database connection")
    }

    pub fn new(db_path: Option<PathBuf>) -> Result<Self, FsPulseError> {
        let mut db_path = db_path
            .or_else(|| BaseDirs::new().map(|base| base.home_dir().to_path_buf()))
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
        let conn = Connection::open(&db_path).map_err(FsPulseError::DatabaseError)?;

        let db = Self {
            conn: Some(conn),
            path: db_path.to_string_lossy().into_owned(),
        };

        // Ensure schema is current
        db.ensure_schema()?;

        Ok(db)
    }

    #[allow(dead_code)]
    pub fn path(&self) -> &str {
        &self.path
    }

    fn ensure_schema(&self) -> Result<(), FsPulseError> {
        let table_exists: bool = self
            .conn()
            .query_row(
                "SELECT count(*) FROM sqlite_master WHERE type='table' AND name='meta'",
                [],
                |row| row.get::<_, i32>(0),
            )
            .map(|count| count > 0)
            .unwrap_or(false);

        if !table_exists {
            self.create_schema()?;
        } else {
            // Get the stored schema version
            let db_version_str: Option<String> = self
                .conn()
                .query_row(
                    "SELECT value FROM meta WHERE key = 'schema_version'",
                    [],
                    |row| row.get(0),
                )
                .optional()?;

            let db_version_str = match db_version_str {
                Some(s) => s,
                None => return Err(FsPulseError::Error("Schema version missing".to_string())),
            };

            let mut db_version: u32 = match db_version_str.parse() {
                Ok(num) => num,
                Err(_) => return Err(FsPulseError::Error("Schema version mismatch".to_string())),
            };

            loop {
                db_version = match db_version {
                    CURRENT_SCHEMA_VERSION => break,
                    2 => self.upgrade_schema(db_version, UPGRADE_2_TO_3_SQL)?,
                    3 => self.upgrade_schema(db_version, UPGRADE_3_TO_4_SQL)?,
                    _ => {
                        return Err(FsPulseError::Error(
                            "No valid database update available".to_string(),
                        ))
                    }
                }
            }
        }

        Ok(())
    }

    fn create_schema(&self) -> Result<(), FsPulseError> {
        info!(
            "Database is uninitialized - creating schema at version {CURRENT_SCHEMA_VERSION}"
        );
        self.conn().execute_batch(CREATE_SCHEMA_SQL)?;
        info!("Database successfully initialized");
        Ok(())
    }

    fn upgrade_schema(
        &self,
        current_version: u32,
        batch: &'static str,
    ) -> Result<u32, FsPulseError> {
        info!(
            "Upgrading database schema {} => {}",
            current_version,
            current_version + 1
        );
        self.conn().execute_batch(batch)?;
        info!("Database successfully upgraded");

        Ok(current_version + 1)
    }
}
