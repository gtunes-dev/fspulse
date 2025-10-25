use crate::{
    config::CONFIG,
    error::FsPulseError,
    schema::{CREATE_SCHEMA_SQL, UPGRADE_2_TO_3_SQL, UPGRADE_3_TO_4_SQL, UPGRADE_4_TO_5_SQL},
};
use directories::BaseDirs;
use log::info;
use rusqlite::{Connection, OptionalExtension, Result};
use std::env;
use std::path::{Path, PathBuf};

const DB_FILENAME: &str = "fspulse.db";
const CURRENT_SCHEMA_VERSION: u32 = 5;

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

    pub fn new() -> Result<Self, FsPulseError> {
        // Database directory precedence:
        // 1. FSPULSE_DATA_DIR environment variable (Docker / explicit override)
        // 2. Config file database.path (user's persistent choice)
        // 3. BaseDirs home directory (native default)

        let db_dir = Self::determine_database_directory()?;

        // Validate directory exists and is writable
        Self::validate_directory(&db_dir)?;

        let mut db_path = db_dir;
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

    fn determine_database_directory() -> Result<PathBuf, FsPulseError> {
        // 1. Check FSPULSE_DATA_DIR environment variable
        if let Ok(data_dir) = env::var("FSPULSE_DATA_DIR") {
            info!("Using database directory from FSPULSE_DATA_DIR: {}", data_dir);
            return Ok(PathBuf::from(data_dir));
        }

        // 2. Check config file for database.path
        if let Some(config) = CONFIG.get() {
            if let Some(ref database_config) = config.database {
                if let Some(path) = database_config.get_path() {
                    info!("Using database directory from config file: {}", path);
                    return Ok(PathBuf::from(path));
                }
            }
        }

        // 3. Fall back to home directory (native default)
        BaseDirs::new()
            .map(|base| {
                let path = base.home_dir().to_path_buf();
                info!("Using home directory for database: {}", path.display());
                path
            })
            .ok_or_else(|| FsPulseError::Error("Could not determine database directory".to_string()))
    }

    fn validate_directory(path: &Path) -> Result<(), FsPulseError> {
        // Check if directory exists
        if !path.exists() {
            return Err(FsPulseError::Error(format!(
                "Database directory '{}' does not exist",
                path.display()
            )));
        }

        // Check if it's a directory
        if !path.is_dir() {
            return Err(FsPulseError::Error(format!(
                "'{}' is not a directory",
                path.display()
            )));
        }

        // Check if it's writable by attempting to create a test file
        let test_file = path.join(".fspulse_write_test");
        match std::fs::write(&test_file, b"test") {
            Ok(_) => {
                // Clean up test file
                let _ = std::fs::remove_file(&test_file);
                Ok(())
            }
            Err(e) => Err(FsPulseError::Error(format!(
                "Database directory '{}' is not writable: {}",
                path.display(),
                e
            ))),
        }
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
                    4 => self.upgrade_schema(db_version, UPGRADE_4_TO_5_SQL)?,
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

    /// Helper to set FSPULSE_DATA_DIR for a test and restore it afterward
    struct TestEnv {
        old_value: Option<String>,
    }

    impl TestEnv {
        fn set_data_dir(path: &str) -> Self {
            let old_value = env::var("FSPULSE_DATA_DIR").ok();
            env::set_var("FSPULSE_DATA_DIR", path);
            Self { old_value }
        }
    }

    impl Drop for TestEnv {
        fn drop(&mut self) {
            match &self.old_value {
                Some(val) => env::set_var("FSPULSE_DATA_DIR", val),
                None => env::remove_var("FSPULSE_DATA_DIR"),
            }
        }
    }

    #[test]
    fn test_database_new_with_valid_path() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let _env = TestEnv::set_data_dir(temp_dir.path().to_str().unwrap());

        let db = Database::new();
        assert!(db.is_ok(), "Database creation should succeed with valid path");

        let db = db.unwrap();
        assert!(db.conn.is_some(), "Database should have a connection");
    }

    #[test]
    fn test_database_new_with_invalid_path() {
        let _env = TestEnv::set_data_dir("/nonexistent/path/that/does/not/exist");

        let db = Database::new();
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

        let _env = TestEnv::set_data_dir(file_path.to_str().unwrap());
        let db = Database::new();
        assert!(db.is_err(), "Database creation should fail when path is a file");
    }

    #[test]
    fn test_database_schema_creation() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let _env = TestEnv::set_data_dir(temp_dir.path().to_str().unwrap());

        let db = Database::new().expect("Database creation should succeed");

        // Verify meta table exists and has correct schema version
        let version: String = db.conn()
            .query_row(
                "SELECT value FROM meta WHERE key = 'schema_version'",
                [],
                |row| row.get(0),
            )
            .expect("Should be able to query schema version");

        assert_eq!(version, "5", "Schema version should be 5");
    }

    #[test]
    fn test_database_tables_created() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let _env = TestEnv::set_data_dir(temp_dir.path().to_str().unwrap());

        let db = Database::new().expect("Database creation should succeed");

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
        let _env = TestEnv::set_data_dir(temp_dir.path().to_str().unwrap());

        let db = Database::new().expect("Database creation should succeed");

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
        let _env = TestEnv::set_data_dir(temp_dir.path().to_str().unwrap());

        let mut db = Database::new().expect("Database creation should succeed");

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
        let _env = TestEnv::set_data_dir(temp_dir.path().to_str().unwrap());
        let expected_path = temp_dir.path().join(DB_FILENAME);

        let db = Database::new().expect("Database creation should succeed");

        assert_eq!(db.path(), expected_path.to_string_lossy(), "Path should match expected database file path");
    }

    #[test]
    fn test_database_new_defaults_to_home_dir() {
        // Clear FSPULSE_DATA_DIR to test default behavior
        let _env = TestEnv { old_value: env::var("FSPULSE_DATA_DIR").ok() };
        env::remove_var("FSPULSE_DATA_DIR");

        // This test may fail on systems without a home directory, so we'll handle both cases
        let db = Database::new();

        match db {
            Ok(_) => {
                // If successful, home directory was found and database was created
            }
            Err(FsPulseError::Error(msg)) => {
                // If failed, should be due to missing database directory
                assert!(msg.contains("Could not determine database directory"),
                       "Error should be about database directory: {msg}");
            }
            Err(other) => {
                panic!("Unexpected error type: {other:?}");
            }
        }
    }
}
