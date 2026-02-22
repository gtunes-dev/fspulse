use std::path::{Path, PathBuf};
use std::{env, fs};

use crate::database::Database;
use crate::error::FsPulseError;
use crate::schedules::{delete_schedules_for_root_immediate, root_has_active_scan_immediate};
use rusqlite::{Connection, OptionalExtension};
use serde::Serialize;

#[derive(Clone, Debug, Default, Serialize)]
pub struct Root {
    #[serde(rename = "id")]
    root_id: i64,
    #[serde(rename = "path")]
    root_path: String,
}

impl Root {
    pub fn get_by_id(conn: &Connection, root_id: i64) -> Result<Option<Self>, FsPulseError> {
        conn.query_row(
            "SELECT root_path FROM roots WHERE root_id = ?",
            [root_id],
            |row| {
                Ok(Root {
                    root_id,
                    root_path: row.get(0)?,
                })
            },
        )
        .optional()
        .map_err(FsPulseError::DatabaseError)
    }

    pub fn try_create(root_path: &str) -> Result<Self, FsPulseError> {
        let path_buf = Root::validate_and_canonicalize_path(root_path)?;
        let canon_root_path = path_buf.to_string_lossy().to_string();
        let conn = Database::get_connection()?;
        Root::create(&conn, &canon_root_path)
    }

    pub fn create(conn: &Connection, root_path: &str) -> Result<Self, FsPulseError> {
        let root_id: i64 = conn.query_row(
            "INSERT INTO roots (root_path) VALUES (?) RETURNING root_id",
            [root_path],
            |row| row.get(0),
        )?;

        Ok(Root {
            root_id,
            root_path: root_path.to_owned(),
        })
    }

    pub fn root_id(&self) -> i64 {
        self.root_id
    }

    pub fn root_path(&self) -> &str {
        &self.root_path
    }

    /// Delete a root and all associated data (scans, items, versions, alerts, schedules).
    /// This operation is performed within a transaction to ensure atomicity.
    /// Returns Ok(()) if successful, or an error if the root doesn't exist, has an active scan, or deletion fails.
    pub fn delete_root(root_id: i64) -> Result<(), FsPulseError> {
        let conn = Database::get_connection()?;
        Database::immediate_transaction(&conn, |c| {
            // First, check if root has an active scan
            if root_has_active_scan_immediate(c, root_id)? {
                return Err(FsPulseError::Error(
                    "Cannot delete root because it has an active scan in progress. Please wait for the scan to complete or stop it first.".to_string()
                ));
            }

            // Delete schedules and queue entries for this root
            delete_schedules_for_root_immediate(c, root_id)?;

            // Delete in order based on foreign key constraints:
            // 1. alerts (references scan_id and item_id)
            // 2. scan_undo_log (references version_id from item_versions)
            // 3. item_versions (references item_id from items, scan_id from scans)
            // 4. items (references root_id)
            // 5. scans (references root_id)
            // 6. root itself

            // Delete alerts for all scans of this root
            c.execute(
                "DELETE FROM alerts WHERE scan_id IN (SELECT scan_id FROM scans WHERE root_id = ?)",
                [root_id],
            )?;

            // Delete undo log entries for versions belonging to this root's items
            c.execute(
                "DELETE FROM scan_undo_log WHERE version_id IN (
                    SELECT v.version_id FROM item_versions v
                    JOIN items i ON i.item_id = v.item_id
                    WHERE i.root_id = ?
                )",
                [root_id],
            )?;

            // Delete item versions for this root
            c.execute(
                "DELETE FROM item_versions WHERE item_id IN (
                    SELECT item_id FROM items WHERE root_id = ?
                )",
                [root_id],
            )?;

            // Delete items for this root
            c.execute("DELETE FROM items WHERE root_id = ?", [root_id])?;

            // Delete scans for this root
            c.execute("DELETE FROM scans WHERE root_id = ?", [root_id])?;

            // Finally, delete the root itself
            let rows_affected = c.execute("DELETE FROM roots WHERE root_id = ?", [root_id])?;

            if rows_affected == 0 {
                return Err(FsPulseError::Error(format!(
                    "Root with id {} not found",
                    root_id
                )));
            }

            Ok(())
        })
    }

    pub fn validate_and_canonicalize_path(path_arg: &str) -> Result<PathBuf, FsPulseError> {
        let path_arg = path_arg.trim();
        if path_arg.is_empty() {
            return Err(FsPulseError::Error("Provided path is empty".into()));
        }

        let path = Path::new(path_arg);

        let absolute_path = if path.is_absolute() {
            path.to_owned()
        } else {
            env::current_dir()?.join(path)
        };

        if !absolute_path.exists() {
            return Err(FsPulseError::Error(format!(
                "Path '{}' does not exist",
                absolute_path.display()
            )));
        }

        let metadata = fs::symlink_metadata(&absolute_path)?;

        if metadata.file_type().is_symlink() {
            return Err(FsPulseError::Error(format!(
                "Path '{}' is a symlink and not allowed",
                absolute_path.display()
            )));
        }

        if !metadata.is_dir() {
            return Err(FsPulseError::Error(format!(
                "Path '{}' is not a directory",
                absolute_path.display()
            )));
        }

        // Canonicalize using Dunce (de-UNC) to strip the "UNC" (e.g., \\?\C) on Windows
        let canonical_path = dunce::canonicalize(absolute_path)?;

        Ok(canonical_path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_root_getters() {
        let root = Root {
            root_id: 123,
            root_path: "/test/path".to_string(),
        };

        assert_eq!(root.root_id(), 123);
        assert_eq!(root.root_path(), "/test/path");
    }

    #[test]
    fn test_root_default() {
        let root = Root::default();

        assert_eq!(root.root_id(), 0);
        assert_eq!(root.root_path(), "");
    }

    #[test]
    fn test_validate_and_canonicalize_path_empty() {
        let result = Root::validate_and_canonicalize_path("");
        assert!(result.is_err());
        if let Err(FsPulseError::Error(msg)) = result {
            assert!(msg.contains("Provided path is empty"));
        } else {
            panic!("Expected FsPulseError::Error");
        }
    }

    #[test]
    fn test_validate_and_canonicalize_path_whitespace_only() {
        let result = Root::validate_and_canonicalize_path("   \t\n  ");
        assert!(result.is_err());
        if let Err(FsPulseError::Error(msg)) = result {
            assert!(msg.contains("Provided path is empty"));
        } else {
            panic!("Expected FsPulseError::Error");
        }
    }

    #[test]
    fn test_validate_and_canonicalize_path_nonexistent() {
        let result = Root::validate_and_canonicalize_path("/this/path/does/not/exist/anywhere");
        assert!(result.is_err());
        if let Err(FsPulseError::Error(msg)) = result {
            assert!(msg.contains("does not exist"));
        } else {
            panic!("Expected FsPulseError::Error");
        }
    }

    #[test]
    fn test_validate_and_canonicalize_path_valid_temp_dir() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let temp_path = temp_dir.path().to_str().unwrap();

        let result = Root::validate_and_canonicalize_path(temp_path);
        assert!(result.is_ok());

        let canonical_path = result.unwrap();
        assert!(canonical_path.exists());
        assert!(canonical_path.is_dir());
    }

    #[test]
    fn test_validate_and_canonicalize_path_file_not_directory() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let file_path = temp_dir.path().join("test_file.txt");
        fs::write(&file_path, "test content").expect("Failed to write test file");

        let result = Root::validate_and_canonicalize_path(file_path.to_str().unwrap());
        assert!(result.is_err());
        if let Err(FsPulseError::Error(msg)) = result {
            assert!(msg.contains("is not a directory"));
        } else {
            panic!("Expected FsPulseError::Error");
        }
    }

    #[test]
    fn test_validate_and_canonicalize_path_relative_path() {
        // Use current directory as a relative path (should work)
        let result = Root::validate_and_canonicalize_path(".");
        assert!(result.is_ok());

        let canonical_path = result.unwrap();
        assert!(canonical_path.exists());
        assert!(canonical_path.is_dir());
        assert!(canonical_path.is_absolute());
    }

    #[test]
    fn test_validate_and_canonicalize_path_with_whitespace() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let temp_path = temp_dir.path().to_str().unwrap();
        let path_with_whitespace = format!("  {temp_path}  ");

        let result = Root::validate_and_canonicalize_path(&path_with_whitespace);
        assert!(result.is_ok());

        let canonical_path = result.unwrap();
        assert!(canonical_path.exists());
        assert!(canonical_path.is_dir());
    }

    #[test]
    fn test_validate_edge_cases() {
        // Test paths with special characters (those that are valid directory names)
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let special_dir = temp_dir.path().join("test-dir_with.special.chars");
        fs::create_dir(&special_dir).expect("Failed to create special dir");

        let result = Root::validate_and_canonicalize_path(special_dir.to_str().unwrap());
        assert!(result.is_ok());

        let canonical_path = result.unwrap();
        assert!(canonical_path.exists());
        assert!(canonical_path.is_dir());
    }

    #[test]
    fn test_validate_absolute_vs_relative() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let temp_path = temp_dir.path();

        // Test absolute path
        let result_abs = Root::validate_and_canonicalize_path(temp_path.to_str().unwrap());
        assert!(result_abs.is_ok());

        // Both should result in the same canonical path
        let canonical_abs = result_abs.unwrap();
        assert!(canonical_abs.is_absolute());
        assert!(canonical_abs.exists());
    }

    #[test]
    fn test_path_canonicalization() {
        // Test that paths are properly canonicalized (removing . and .. components)
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let subdir = temp_dir.path().join("subdir");
        fs::create_dir(&subdir).expect("Failed to create subdir");

        // Create a path with .. components
        let complex_path = format!("{}/subdir/../subdir", temp_dir.path().display());

        let result = Root::validate_and_canonicalize_path(&complex_path);
        assert!(result.is_ok());

        let canonical_path = result.unwrap();
        assert!(canonical_path.exists());
        assert!(canonical_path.is_dir());

        // The canonical path should not contain .. components
        let path_str = canonical_path.to_string_lossy();
        assert!(!path_str.contains(".."));
    }
}
