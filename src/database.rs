use crate::{
    config::Config,
    error::FsPulseError,
    schema::{
        Migration, CREATE_SCHEMA_SQL, MIGRATION_10_TO_11, MIGRATION_11_TO_12, MIGRATION_12_TO_13,
        MIGRATION_13_TO_14, MIGRATION_14_TO_15, MIGRATION_15_TO_16, MIGRATION_16_TO_17, MIGRATION_17_TO_18, MIGRATION_18_TO_19, MIGRATION_19_TO_20, MIGRATION_20_TO_21, MIGRATION_21_TO_22, MIGRATION_22_TO_23, MIGRATION_2_TO_3, MIGRATION_3_TO_4,
        MIGRATION_4_TO_5, MIGRATION_5_TO_6, MIGRATION_6_TO_7, MIGRATION_7_TO_8, MIGRATION_8_TO_9,
        MIGRATION_9_TO_10,
    },
    sort::compare_paths,
};
use log::{error, info};
use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;
use rusqlite::{Connection, OptionalExtension};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::Duration;

#[cfg(test)]
use std::env;

const DB_FILENAME: &str = "fspulse.db";
const CURRENT_SCHEMA_VERSION: u32 = 23;

// Connection pool configuration
const POOL_MAX_SIZE: u32 = 15;
const POOL_CONNECTION_TIMEOUT_SECS: u64 = 30;
const DB_BUSY_TIMEOUT_SECS: u64 = 5;

// Connection pool types
pub type DbPool = Pool<SqliteConnectionManager>;
pub type PooledConnection = r2d2::PooledConnection<SqliteConnectionManager>;

// Global connection pool
static GLOBAL_POOL: OnceLock<DbPool> = OnceLock::new();

/// Database access and management
pub struct Database;

impl Database {
    /// Initialize the database system.
    /// This must be called once at application startup before any database operations.
    /// It will:
    /// - Initialize the connection pool
    /// - Validate the database file path
    /// - Create the schema if it doesn't exist
    /// - Run any pending schema migrations
    ///
    /// This function fails fast if any initialization step fails.
    pub fn init() -> Result<(), FsPulseError> {
        let db_path = Self::get_path()?;

        info!("Initializing database at: {}", db_path.display());

        // Create the connection pool with initialization for each connection
        let manager = SqliteConnectionManager::file(&db_path).with_init(|conn| {
            // Register custom collations on each connection from the pool
            conn.create_collation("natural_path", compare_paths)?;

            // Enable WAL mode for better concurrency (readers don't block writers)
            conn.pragma_update(None, "journal_mode", "WAL")?;

            // Set busy timeout for lock contention handling
            conn.busy_timeout(Duration::from_secs(DB_BUSY_TIMEOUT_SECS))?;

            Ok(())
        });

        let pool = Pool::builder()
            .max_size(POOL_MAX_SIZE)
            .connection_timeout(Duration::from_secs(POOL_CONNECTION_TIMEOUT_SECS))
            .build(manager)
            .map_err(|e| FsPulseError::Error(format!("Failed to create connection pool: {}", e)))?;

        GLOBAL_POOL
            .set(pool)
            .map_err(|_| FsPulseError::Error("Connection pool already initialized".into()))?;

        info!("Database connection pool initialized");

        // Ensure schema is current (create or migrate)
        ensure_schema_current()?;

        info!("Database initialization complete");

        Ok(())
    }

    /// Get a connection from the global pool.
    /// The connection will be automatically returned to the pool when dropped (RAII).
    ///
    /// # Errors
    /// Returns an error if the pool has not been initialized via `Database::init()`.
    pub fn get_connection() -> Result<PooledConnection, FsPulseError> {
        GLOBAL_POOL
            .get()
            .ok_or_else(|| {
                FsPulseError::Error(
                    "Connection pool not initialized - call Database::init() first".into(),
                )
            })?
            .get()
            .map_err(|e| FsPulseError::Error(format!("Failed to get connection from pool: {}", e)))
    }

    /// Get the database file path based on configuration.
    pub fn get_path() -> Result<PathBuf, FsPulseError> {
        let db_dir_str = Config::get_database_dir();

        // Empty string means use data directory
        let db_dir = if db_dir_str.is_empty() {
            PathBuf::from(Config::get_data_dir())
        } else {
            PathBuf::from(db_dir_str)
        };

        // Validate directory exists and is writable
        validate_directory(&db_dir)?;

        let mut db_path = db_dir;
        db_path.push(DB_FILENAME);

        Ok(db_path)
    }

    /// Read the schema version from the database.
    pub fn get_schema_version() -> Result<String, FsPulseError> {
        let conn = Self::get_connection()?;
        let version: String = conn.query_row(
            "SELECT value FROM meta WHERE key = 'schema_version'",
            [],
            |row| row.get(0),
        )?;
        Ok(version)
    }

    /// Execute a function within an IMMEDIATE transaction.
    /// Use for read-then-write patterns to prevent lock upgrade failures.
    ///
    /// # Example
    /// ```
    /// let conn = Database::get_connection()?;
    /// let result = Database::immediate_transaction(&conn, |c| {
    ///     let count: i32 = c.query_row("SELECT COUNT(*) ...", [], |row| row.get(0))?;
    ///     c.execute("UPDATE ... WHERE count = ?", [count])?;
    ///     Ok(count)
    /// })?;
    /// ```
    pub fn immediate_transaction<F, T>(conn: &Connection, f: F) -> Result<T, FsPulseError>
    where
        F: FnOnce(&Connection) -> Result<T, FsPulseError>,
    {
        conn.execute("BEGIN IMMEDIATE", [])
            .map_err(FsPulseError::DatabaseError)?;

        // Use catch_unwind to ensure the transaction is rolled back even if the
        // closure panics. Without this, a panic would skip both COMMIT and
        // ROLLBACK, leaving the connection with an open transaction. When the
        // connection is returned to the pool, subsequent uses would fail with
        // "cannot start a transaction within a transaction".
        match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| f(conn))) {
            Ok(Ok(result)) => {
                conn.execute("COMMIT", [])
                    .map_err(FsPulseError::DatabaseError)?;
                Ok(result)
            }
            Ok(Err(e)) => {
                // Attempt rollback, but preserve original error
                let _ = conn.execute("ROLLBACK", []);
                Err(e)
            }
            Err(panic_payload) => {
                // Closure panicked — rollback to clean up the connection before
                // resuming the panic so the connection isn't returned to the pool
                // with an open transaction.
                let _ = conn.execute("ROLLBACK", []);

                // Extract a message from the panic payload for structured logging.
                // Panic payloads are typically &str or String.
                let msg = if let Some(s) = panic_payload.downcast_ref::<&str>() {
                    s.to_string()
                } else if let Some(s) = panic_payload.downcast_ref::<String>() {
                    s.clone()
                } else {
                    "unknown panic".to_string()
                };
                log::error!("Panic inside immediate_transaction (rolled back): {}", msg);

                std::panic::resume_unwind(panic_payload);
            }
        }
    }

    /// Get database statistics including size and wasted space.
    pub fn get_stats() -> Result<DbStats, FsPulseError> {
        let conn = Self::get_connection()?;

        // Get SQLite page information
        let page_count: i64 = conn
            .pragma_query_value(None, "page_count", |row| row.get(0))
            .map_err(FsPulseError::DatabaseError)?;
        let page_size: i64 = conn
            .pragma_query_value(None, "page_size", |row| row.get(0))
            .map_err(FsPulseError::DatabaseError)?;
        let freelist_count: i64 = conn
            .pragma_query_value(None, "freelist_count", |row| row.get(0))
            .map_err(FsPulseError::DatabaseError)?;

        let total_size = (page_count * page_size) as u64;
        let wasted_size = (freelist_count * page_size) as u64;

        let path = Self::get_path()?.to_string_lossy().into_owned();

        Ok(DbStats {
            path,
            total_size,
            wasted_size,
        })
    }

    /// Compact the database using VACUUM.
    pub fn compact() -> Result<(), FsPulseError> {
        // Get stats before compaction
        let stats_before = Self::get_stats()?;
        info!(
            "Database before compaction: total={} bytes, wasted={} bytes",
            stats_before.total_size, stats_before.wasted_size
        );

        let conn = Self::get_connection()?;

        info!("Starting database compaction (VACUUM)");
        let vacuum_start = std::time::Instant::now();
        conn.execute("VACUUM", [])
            .map_err(FsPulseError::DatabaseError)?;
        let vacuum_duration = vacuum_start.elapsed();
        info!(
            "Database compaction (VACUUM) completed in {:?}",
            vacuum_duration
        );

        // Get stats after compaction
        let stats_after = Self::get_stats()?;
        info!(
            "Database after compaction: total={} bytes, wasted={} bytes, reclaimed={} bytes",
            stats_after.total_size,
            stats_after.wasted_size,
            stats_before
                .total_size
                .saturating_sub(stats_after.total_size)
        );

        Ok(())
    }

    /// Get a value from the meta table by key.
    /// Returns None if the key doesn't exist.
    /// This function expects to be called within a transaction (connection lock held).
    pub fn get_meta_value_locked(
        conn: &Connection,
        key: &str,
    ) -> Result<Option<String>, FsPulseError> {
        conn.query_row("SELECT value FROM meta WHERE key = ?", [key], |row| {
            row.get(0)
        })
        .optional()
        .map_err(FsPulseError::DatabaseError)
    }

    /// Set a value in the meta table by key.
    /// Creates the key if it doesn't exist, updates if it does.
    /// This function expects to be called within a transaction (connection lock held).
    pub fn set_meta_value_locked(
        conn: &Connection,
        key: &str,
        value: &str,
    ) -> Result<(), FsPulseError> {
        conn.execute(
            "INSERT OR REPLACE INTO meta (key, value) VALUES (?, ?)",
            [key, value],
        )
        .map_err(FsPulseError::DatabaseError)?;
        Ok(())
    }

    /// Delete a key from the meta table.
    /// No error if the key doesn't exist.
    /// This function expects to be called within a transaction (connection lock held).
    pub fn delete_meta_locked(conn: &Connection, key: &str) -> Result<(), FsPulseError> {
        conn.execute("DELETE FROM meta WHERE key = ?", [key])
            .map_err(FsPulseError::DatabaseError)?;
        Ok(())
    }
}

/// Database statistics
#[derive(Debug, Clone, serde::Serialize)]
pub struct DbStats {
    pub path: String,
    pub total_size: u64,
    pub wasted_size: u64,
}

// ============================================================================
// Private implementation functions
// ============================================================================

/// Validate that a directory exists and is writable.
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

/// Log an informational message to both the log file and stderr.
/// Used during schema migration so progress is visible on the console.
pub(crate) fn migration_info(msg: &str) {
    info!("{}", msg);
    eprintln!("{}", msg);
}

/// Log an error message to both the log file and stderr.
/// Used during schema migration so failures are visible on the console.
pub(crate) fn migration_error(msg: &str) {
    error!("{}", msg);
    eprintln!("{}", msg);
}

/// Ensure the database schema is current.
/// This is called by init() and should not be called directly.
fn ensure_schema_current() -> Result<(), FsPulseError> {
    let conn = Database::get_connection()?;

    let table_exists: bool = conn
        .query_row(
            "SELECT count(*) FROM sqlite_master WHERE type='table' AND name='meta'",
            [],
            |row| row.get::<_, i32>(0),
        )
        .map(|count| count > 0)
        .unwrap_or(false);

    if !table_exists {
        create_schema(&conn)?;
    } else {
        // Get the stored schema version
        let db_version_str: Option<String> = conn
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

        if db_version < CURRENT_SCHEMA_VERSION {
            migration_info(&format!(
                "Database schema upgrade required: v{} -> v{}",
                db_version, CURRENT_SCHEMA_VERSION
            ));
        }

        loop {
            db_version = match db_version {
                CURRENT_SCHEMA_VERSION => break,
                2 => upgrade_schema(&conn, db_version, &MIGRATION_2_TO_3)?,
                3 => upgrade_schema(&conn, db_version, &MIGRATION_3_TO_4)?,
                4 => upgrade_schema(&conn, db_version, &MIGRATION_4_TO_5)?,
                5 => upgrade_schema(&conn, db_version, &MIGRATION_5_TO_6)?,
                6 => upgrade_schema(&conn, db_version, &MIGRATION_6_TO_7)?,
                7 => upgrade_schema(&conn, db_version, &MIGRATION_7_TO_8)?,
                8 => upgrade_schema(&conn, db_version, &MIGRATION_8_TO_9)?,
                9 => upgrade_schema(&conn, db_version, &MIGRATION_9_TO_10)?,
                10 => upgrade_schema(&conn, db_version, &MIGRATION_10_TO_11)?,
                11 => upgrade_schema(&conn, db_version, &MIGRATION_11_TO_12)?,
                12 => upgrade_schema(&conn, db_version, &MIGRATION_12_TO_13)?,
                13 => upgrade_schema(&conn, db_version, &MIGRATION_13_TO_14)?,
                14 => upgrade_schema(&conn, db_version, &MIGRATION_14_TO_15)?,
                15 => upgrade_schema(&conn, db_version, &MIGRATION_15_TO_16)?,
                16 => upgrade_schema(&conn, db_version, &MIGRATION_16_TO_17)?,
                17 => upgrade_schema(&conn, db_version, &MIGRATION_17_TO_18)?,
                18 => upgrade_schema(&conn, db_version, &MIGRATION_18_TO_19)?,
                19 => upgrade_schema(&conn, db_version, &MIGRATION_19_TO_20)?,
                20 => upgrade_schema(&conn, db_version, &MIGRATION_20_TO_21)?,
                21 => upgrade_schema(&conn, db_version, &MIGRATION_21_TO_22)?,
                22 => upgrade_schema(&conn, db_version, &MIGRATION_22_TO_23)?,
                _ => {
                    let msg = format!(
                        "No migration path from schema v{} to v{}",
                        db_version, CURRENT_SCHEMA_VERSION
                    );
                    migration_error(&msg);
                    return Err(FsPulseError::Error(msg));
                }
            }
        }
    }

    Ok(())
}

fn create_schema(conn: &Connection) -> Result<(), FsPulseError> {
    migration_info(&format!(
        "Creating database schema at v{CURRENT_SCHEMA_VERSION}"
    ));
    conn.execute_batch(CREATE_SCHEMA_SQL)?;
    migration_info("Database schema created");
    Ok(())
}

fn upgrade_schema(
    conn: &Connection,
    current_version: u32,
    migration: &Migration,
) -> Result<u32, FsPulseError> {
    let next_version = current_version + 1;
    migration_info(&format!(
        "  Upgrading schema v{} -> v{}...",
        current_version, next_version
    ));

    let result = match migration {
        Migration::Transacted {
            pre_sql,
            code_fn,
            post_sql,
        } => {
            // Disable foreign key constraints during migration (required for table reconstruction)
            conn.execute("PRAGMA foreign_keys = OFF", [])
                .map_err(FsPulseError::DatabaseError)?;

            // Run all migration phases within a transaction for atomicity
            let result = Database::immediate_transaction(conn, |conn| {
                if let Some(pre_sql) = pre_sql {
                    conn.execute_batch(pre_sql)?;
                }
                if let Some(code_fn) = code_fn {
                    code_fn(conn)?;
                }
                if let Some(post_sql) = post_sql {
                    conn.execute_batch(post_sql)?;
                }
                Ok(())
            });

            // Re-enable foreign key constraints (always, even on error)
            conn.execute("PRAGMA foreign_keys = ON", [])
                .map_err(FsPulseError::DatabaseError)?;

            result
        }
        Migration::Standalone { code_fn } => {
            // Code manages its own transactions and bumps the schema version
            // itself (typically in the same transaction as its final cleanup).
            code_fn(conn)
        }
    };

    match result {
        Ok(()) => {
            migration_info(&format!(
                "  Schema v{} -> v{} complete",
                current_version, next_version
            ));
            Ok(next_version)
        }
        Err(e) => {
            migration_error(&format!(
                "  Schema upgrade v{} -> v{} failed: {}",
                current_version, next_version, e
            ));
            Err(e)
        }
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;
    use std::sync::Mutex;
    use std::sync::Once;
    use tempfile::TempDir;

    static INIT: Once = Once::new();
    static TEST_DIR: Mutex<Option<TempDir>> = Mutex::new(None);

    /// Initialize CONFIG and shared test directory once for all tests
    fn init_test_config() {
        INIT.call_once(|| {
            // Create a persistent temp directory for all tests
            let temp_dir = TempDir::new().expect("Failed to create test dir");
            let test_path = temp_dir.path().to_str().unwrap().to_string();

            // Store the temp directory to keep it alive
            *TEST_DIR.lock().unwrap() = Some(temp_dir);

            env::set_var("FSPULSE_DATA_DIR", &test_path);

            // Initialize CONFIG with the test directory
            use crate::config::CONFIG;
            if CONFIG.get().is_none() {
                let project_dirs = directories::ProjectDirs::from("", "", "fspulse-test").unwrap();
                crate::config::Config::load_config(&project_dirs).ok();
            }

            // Initialize the database pool
            Database::init().expect("Failed to initialize database");
        });
    }

    #[test]
    #[serial]
    fn test_database_init() {
        use crate::config::CONFIG;
        if CONFIG.get().is_some() {
            return;
        }

        init_test_config();

        let conn = Database::get_connection().expect("Should get connection from pool");

        // Verify we can execute a simple query
        let result: i32 = conn
            .query_row("SELECT 1", [], |row| row.get(0))
            .expect("Should be able to execute simple query");

        assert_eq!(result, 1, "Simple query should return 1");
    }

    #[test]
    #[serial]
    fn test_database_schema_creation() {
        use crate::config::CONFIG;
        if CONFIG.get().is_some() {
            return;
        }

        init_test_config();

        let conn = Database::get_connection().expect("Should get connection");

        // Verify meta table exists and has correct schema version
        let version: String = conn
            .query_row(
                "SELECT value FROM meta WHERE key = 'schema_version'",
                [],
                |row| row.get(0),
            )
            .expect("Should be able to query schema version");

        assert_eq!(version, "14", "Schema version should be 14");
    }

    #[test]
    #[serial]
    fn test_database_tables_created() {
        use crate::config::CONFIG;
        if CONFIG.get().is_some() {
            return;
        }

        init_test_config();

        let conn = Database::get_connection().expect("Should get connection");

        // Verify all expected tables exist
        let expected_tables = ["meta", "roots", "scans", "items"];
        for table in expected_tables {
            let count: i32 = conn
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
    #[serial]
    fn test_immediate_transaction() {
        use crate::config::CONFIG;
        if CONFIG.get().is_some() {
            return;
        }

        init_test_config();

        let conn = Database::get_connection().expect("Should get connection");

        // Test immediate transaction
        let result = Database::immediate_transaction(&conn, |c| {
            c.execute(
                "INSERT OR REPLACE INTO meta (key, value) VALUES ('test_key', 'test_value')",
                [],
            )?;
            Ok(())
        });

        assert!(result.is_ok(), "Transaction should succeed");

        // Verify the data was written
        let value: String = conn
            .query_row("SELECT value FROM meta WHERE key = 'test_key'", [], |row| {
                row.get(0)
            })
            .expect("Should be able to query inserted value");

        assert_eq!(value, "test_value", "Inserted value should match");
    }

    #[test]
    #[serial]
    fn test_get_database_path() {
        use crate::config::CONFIG;
        if CONFIG.get().is_some() {
            return;
        }

        init_test_config();

        let path = Database::get_path().expect("Should get database path");

        // Verify path ends with the database filename
        assert!(
            path.to_string_lossy().ends_with(DB_FILENAME),
            "Path should end with {DB_FILENAME}"
        );
    }

    #[test]
    #[serial]
    fn test_collation_registered() {
        use crate::config::CONFIG;
        if CONFIG.get().is_some() {
            return;
        }

        init_test_config();

        let conn = Database::get_connection().expect("Should get connection");

        // Create a temporary table with test paths
        conn.execute("CREATE TEMPORARY TABLE test_paths (path TEXT)", [])
            .expect("Should create test table");

        // Insert paths in scrambled order
        let test_paths = vec!["/proj-A/file1", "/proj", "/proj/file3", "/proj/file2"];

        for path in &test_paths {
            conn.execute("INSERT INTO test_paths (path) VALUES (?)", [path])
                .expect("Should insert test path");
        }

        // Query with the natural_path collation
        let mut stmt = conn
            .prepare("SELECT path FROM test_paths ORDER BY path COLLATE natural_path")
            .expect("Should prepare query with collation");

        let sorted_paths: Vec<String> = stmt
            .query_map([], |row| row.get(0))
            .expect("Should execute query")
            .map(|r| r.expect("Should read row"))
            .collect();

        // Expected order: /proj, then its children, then /proj-A
        let expected = vec!["/proj", "/proj/file2", "/proj/file3", "/proj-A/file1"];

        assert_eq!(
            sorted_paths, expected,
            "Paths should be sorted correctly using natural_path collation"
        );
    }

    #[test]
    #[serial]
    fn test_meta_operations() {
        use crate::config::CONFIG;
        if CONFIG.get().is_some() {
            return;
        }

        init_test_config();

        let conn = Database::get_connection().expect("Should get connection");

        // Test set
        Database::set_meta_value_locked(&conn, "test_meta", "test_value")
            .expect("Should set meta value");

        // Test get
        let value =
            Database::get_meta_value_locked(&conn, "test_meta").expect("Should get meta value");
        assert_eq!(value, Some("test_value".to_string()));

        // Test delete
        Database::delete_meta_locked(&conn, "test_meta").expect("Should delete meta value");

        let value =
            Database::get_meta_value_locked(&conn, "test_meta").expect("Should get meta value");
        assert_eq!(value, None);
    }
}
