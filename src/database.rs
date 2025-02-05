use rusqlite::{Connection, Result};
use std::{io, path::Path};
use crate::error::{DirCheckError};

const DB_FILENAME: &str = "dircheck.db";

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

        Ok(Self { conn })
    }
}