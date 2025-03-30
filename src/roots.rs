use std::{env, fs};
use std::path::{Path, PathBuf};

use dialoguer::theme::ColorfulTheme;
use dialoguer::Select;
use rusqlite::OptionalExtension;
use crate::database::Database;
use crate::error::FsPulseError;


#[derive(Clone, Debug, Default)]
pub struct Root {
    id: i64,
    path: String
}

impl Root {
    pub fn interact_choose_root(db: &Database, prompt: &str) -> Result<Option<Root>, FsPulseError> {
        let mut roots = Root::roots_as_vec(db)?;
        if roots.is_empty() {
            print!("No roots in database");
            return Ok(None);
        }

        let mut labels: Vec<&str> = roots.iter().map(|root| root.path()).collect();
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
    
    pub fn get_by_id(db: &Database, id: i64) -> Result<Option<Self>, FsPulseError> {
        let conn = db.conn();

        conn.query_row(
            "SELECT path FROM roots WHERE id = ?", 
            [id], 
            |row| Ok(Root {
                id,
                path: row.get(0)?,
            }),
        )
        .optional()
        .map_err(FsPulseError::Database)
    }

    pub fn get_by_path(db: &Database, path: &str) -> Result<Option<Self>, FsPulseError> {
        let conn = db.conn();

        conn.query_row(
            "SELECT id, path FROM roots WHERE path = ?",
            [path],
            |row| Ok(Root {
                id: row.get(0)?,   
                path: row.get(1)?, 
            }),
        )
        .optional()
        .map_err(FsPulseError::Database)
    }

    pub fn create(db: &Database, path: &str) -> Result<Self, FsPulseError> {
        let conn = db.conn();

        let id: i64 = conn.query_row(
            "INSERT INTO roots (path) VALUES (?) RETURNING id",
            [path],
            |row| row.get(0),
        )?;

        Ok(Root { id, path: path.to_owned() })
    }

    /* 
    pub fn get_or_insert(db: &Database, path: &str) -> Result<Self, FsPulseError> {
        let conn = &db.conn;

        conn.execute("INSERT OR IGNORE INTO roots (path) VALUES (?)", [path])?;

        let id: i64 = conn.query_row(
            "SELECT id FROM roots WHERE path = ?",
            [path],
            |row| row.get(0),
        )?;

        Ok(Root { id, path: path.to_owned() })
    } */

    pub fn id(&self) -> i64 {
        self.id
    }

    pub fn path(&self) -> &str {
        &self.path
    }

    pub fn roots_as_vec(db: &Database) -> Result<Vec<Root>, FsPulseError> {
        let mut roots: Vec<Root> = Vec::new();

        Root::for_each_root(
            db, 
            |root| {
                roots.push(root.clone());
                Ok(())
            }
        )?;

        Ok(roots)
    }

    pub fn for_each_root<F>(db: &Database, mut func: F) -> Result<(), FsPulseError> 
    where
        F: FnMut(&Root) -> Result<(), FsPulseError>,
    {
   
        let mut stmt = db.conn().prepare(
            "SELECT id, path
            FROM roots
            ORDER BY id ASC"
        )?;

        let rows = stmt.query_map([], |row| {
            Ok(Root {
                id: row.get::<_, i64>(0)?,      // root path id
                path: row.get::<_, String>(1)?, // path
            })
        })?;

        for row in rows {

            let root= row?;
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
            return Err(FsPulseError::Error(format!("Path '{}' does not exist", absolute_path.display())));
        }
    
        let metadata = fs::symlink_metadata(&absolute_path)?;

        if metadata.file_type().is_symlink() {
            return Err(FsPulseError::Error(format!("Path '{}' is a symlink and not allowed", absolute_path.display())));
        }
        
        if !metadata.is_dir() {
            return Err(FsPulseError::Error(format!("Path '{}' is not a directory", absolute_path.display())));
        }

        let canonical_path = absolute_path.canonicalize()?;
    
        Ok(canonical_path)
    }
}