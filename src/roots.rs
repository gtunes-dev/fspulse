use std::i64;

use rusqlite::Error::QueryReturnedNoRows;
use crate::database::Database;
use crate::error::FsPulseError;


#[derive(Clone, Debug, Default)]
pub struct Root {
    id: i64,
    path: String
}

impl Root {
    pub fn get_from_id(db: &Database, id: i64) -> Result<Option<Self>, FsPulseError> {
        let conn = &db.conn;

        match conn.query_row(
            "SELECT path FROM roots WHERE id = ?", 
            [id], 
            |row| row.get(0),
        ) {
            Ok(path) => Ok(Some(Root { id, path } )),
            Err(QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(FsPulseError::Database(e)),
        }
    }

    // TODO: I think this is the best pattern for handling a single row
    // queries and the possible errors. Apply this pattern everywhere.
    pub fn get_from_path(db: &Database, path: &str) -> Result<Self, FsPulseError> {
        let conn = &db.conn;

        // TODO: Should we try to canonicalize the path?
    
        match conn.query_row(
            "SELECT id, path FROM roots WHERE path = ?",
            [path],
            |row| Ok(Root {
                id: row.get(0)?,   
                path: row.get(1)?, 
            }),
        ) {
            Ok(root) => Ok(root),
            Err(QueryReturnedNoRows) => {
                Err(FsPulseError::Error(format!("Root '{}' not found", path)))
            }
            Err(e) => Err(FsPulseError::Database(e)),
        }
    }

    pub fn get_or_insert(db: &Database, path: &str) -> Result<Self, FsPulseError> {
        let conn = &db.conn;

        conn.execute("INSERT OR IGNORE INTO roots (path) VALUES (?)", [path])?;

        let id: i64 = conn.query_row(
            "SELECT id FROM roots WHERE path = ?",
            [path],
            |row| row.get(0),
        )?;

        Ok(Root { id, path: path.to_owned() })
    }

    pub fn id(&self) -> i64 {
        self.id
    }

    pub fn path(&self) -> &str {
        &self.path
    }

    pub fn for_each_root<F>(db: &Database, mut func: F) -> Result<(), FsPulseError> 
    where
        F: FnMut(&Root) -> Result<(), FsPulseError>,
    {
   
        let mut stmt = db.conn.prepare(
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

    pub fn latest_scan(&self, db: &Database) -> Result<Option<i64>, FsPulseError> {
        let conn = &db.conn;

        match conn.query_row(
            "SELECT id 
            FROM scans
            WHERE root_id = ?
            ORDER BY ID DESC
            LIMIT 1", 
            [self.id], 
            |row| row.get(0),
        ) {
            Ok(id) => Ok(Some( id )),
            Err(QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(FsPulseError::Database(e)),
        }
    }
}