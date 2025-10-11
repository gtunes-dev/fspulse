use std::path::{Path, PathBuf};
use std::{env, fs};

use dialoguer::theme::ColorfulTheme;
use dialoguer::Select;

use crate::database::Database;
use crate::error::FsPulseError;
use rusqlite::OptionalExtension;
use serde::Serialize;

#[derive(Clone, Debug, Default, Serialize)]
pub struct Root {
    #[serde(rename = "id")]
    root_id: i64,
    #[serde(rename = "path")]
    root_path: String,
}

impl Root {
    pub fn interact_choose_root(db: &Database, prompt: &str) -> Result<Option<Root>, FsPulseError> {
        let mut roots = Root::roots_as_vec(db)?;
        if roots.is_empty() {
            print!("No roots in database");
            return Ok(None);
        }

        let mut labels: Vec<&str> = roots.iter().map(|root| root.root_path()).collect();
        labels.push("Exit");

        let selection = Select::with_theme(&ColorfulTheme::default())
            .with_prompt(prompt)
            .default(0)
            .items(&labels)
            .interact()
            .unwrap();

        // "Exit" is the last option in the prompt
        if selection == roots.len() {
            Ok(None)
        } else {
            Ok(Some(roots.remove(selection)))
        }
    }

    pub fn get_by_id(db: &Database, root_id: i64) -> Result<Option<Self>, FsPulseError> {
        let conn = db.conn();

        conn.query_row("SELECT root_path FROM roots WHERE root_id = ?", [root_id], |row| {
            Ok(Root {
                root_id,
                root_path: row.get(0)?,
            })
        })
        .optional()
        .map_err(FsPulseError::DatabaseError)
    }

    pub fn get_by_path(db: &Database, root_path: &str) -> Result<Option<Self>, FsPulseError> {
        let conn = db.conn();

        conn.query_row("SELECT root_id, root_path FROM roots WHERE root_path = ?", [root_path], |row| {
            Ok(Root {
                root_id: row.get(0)?,
                root_path: row.get(1)?,
            })
        })
        .optional()
        .map_err(FsPulseError::DatabaseError)
    }

    pub fn create(db: &Database, root_path: &str) -> Result<Self, FsPulseError> {
        let conn = db.conn();

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

    pub fn roots_as_vec(db: &Database) -> Result<Vec<Root>, FsPulseError> {
        let mut roots: Vec<Root> = Vec::new();

        Root::for_each_root(db, |root| {
            roots.push(root.clone());
            Ok(())
        })?;

        Ok(roots)
    }

    pub fn for_each_root<F>(db: &Database, mut func: F) -> Result<(), FsPulseError>
    where
        F: FnMut(&Root) -> Result<(), FsPulseError>,
    {
        let mut stmt = db.conn().prepare(
            "SELECT root_id, root_path
            FROM roots
            ORDER BY root_id ASC",
        )?;

        let rows = stmt.query_map([], |row| {
            Ok(Root {
                root_id: row.get::<_, i64>(0)?,      
                root_path: row.get::<_, String>(1)?,
            })
        })?;

        for row in rows {
            let root = row?;
            func(&root)?;
        }

        Ok(())
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
    use tempfile::TempDir;
    use std::fs;

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
