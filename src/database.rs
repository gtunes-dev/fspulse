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

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_database_new_with_valid_path() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let db_path = Some(temp_dir.path().to_path_buf());
        
        let db = Database::new(db_path);
        assert!(db.is_ok(), "Database creation should succeed with valid path");
        
        let db = db.unwrap();
        assert!(db.conn.is_some(), "Database should have a connection");
    }

    #[test]
    fn test_database_new_with_invalid_path() {
        let invalid_path = Some("/nonexistent/path/that/does/not/exist".into());
        
        let db = Database::new(invalid_path);
        assert!(db.is_err(), "Database creation should fail with invalid path");
        
        match db.unwrap_err() {
            FsPulseError::Error(msg) => {
                assert!(msg.contains("does not exist"), "Error should mention path doesn't exist");
            }
            _ => panic!("Expected FsPulseError::Error"),
        }
    }

    #[test]
    fn test_database_new_with_file_instead_of_directory() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let file_path = temp_dir.path().join("not_a_directory.txt");
        std::fs::write(&file_path, "test").expect("Failed to create test file");
        
        let db = Database::new(Some(file_path));
        assert!(db.is_err(), "Database creation should fail when path is a file");
    }

    #[test]
    fn test_database_schema_creation() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let db_path = Some(temp_dir.path().to_path_buf());
        
        let db = Database::new(db_path).expect("Database creation should succeed");
        
        // Verify meta table exists and has correct schema version
        let version: String = db.conn()
            .query_row(
                "SELECT value FROM meta WHERE key = 'schema_version'",
                [],
                |row| row.get(0),
            )
            .expect("Should be able to query schema version");
        
        assert_eq!(version, "4", "Schema version should be 4");
    }

    #[test]
    fn test_database_tables_created() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let db_path = Some(temp_dir.path().to_path_buf());
        
        let db = Database::new(db_path).expect("Database creation should succeed");
        
        // Verify all expected tables exist
        let expected_tables = ["meta", "roots", "scans", "items"];
        for table in expected_tables {
            let count: i32 = db.conn()
                .query_row(
                    "SELECT count(*) FROM sqlite_master WHERE type='table' AND name=?",
                    [table],
                    |row| row.get(0),
                )
                .expect("Should be able to query table existence");
            
            assert_eq!(count, 1, "Table '{table}' should exist");
        }
    }

    #[test]
    fn test_conn_access() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let db_path = Some(temp_dir.path().to_path_buf());
        
        let db = Database::new(db_path).expect("Database creation should succeed");
        
        // Test conn() method
        let _conn = db.conn();
        
        // Test that we can execute a simple query
        let result: i32 = db.conn()
            .query_row("SELECT 1", [], |row| row.get(0))
            .expect("Should be able to execute simple query");
        
        assert_eq!(result, 1, "Simple query should return 1");
    }

    #[test]
    fn test_conn_mut_access() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let db_path = Some(temp_dir.path().to_path_buf());
        
        let mut db = Database::new(db_path).expect("Database creation should succeed");
        
        // Test conn_mut() method
        let _conn_mut = db.conn_mut();
        
        // Test that we can execute a write operation
        let rows_affected = db.conn_mut()
            .execute("INSERT OR REPLACE INTO meta (key, value) VALUES ('test_key', 'test_value')", [])
            .expect("Should be able to execute write query");
        
        assert_eq!(rows_affected, 1, "Insert should affect 1 row");
        
        // Verify the data was written
        let value: String = db.conn()
            .query_row(
                "SELECT value FROM meta WHERE key = 'test_key'",
                [],
                |row| row.get(0),
            )
            .expect("Should be able to query inserted value");
        
        assert_eq!(value, "test_value", "Inserted value should match");
    }

    #[test]
    fn test_database_path_method() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let db_path = Some(temp_dir.path().to_path_buf());
        let expected_path = temp_dir.path().join(DB_FILENAME);
        
        let db = Database::new(db_path).expect("Database creation should succeed");
        
        assert_eq!(db.path(), expected_path.to_string_lossy(), "Path should match expected database file path");
    }

    #[test]
    fn test_database_new_with_none_path() {
        // This test may fail on systems without a home directory, so we'll handle both cases
        let db = Database::new(None);
        
        match db {
            Ok(_) => {
                // If successful, home directory was found and database was created
            }
            Err(FsPulseError::Error(msg)) => {
                // If failed, should be due to missing home directory
                assert!(msg.contains("Could not determine home directory"), 
                       "Error should be about home directory: {msg}");
            }
            Err(other) => {
                panic!("Unexpected error type: {other:?}");
            }
        }
    }
}
