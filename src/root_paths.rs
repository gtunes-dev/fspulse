use std::i64;

use rusqlite::Error::QueryReturnedNoRows;
use crate::database::Database;
use crate::error::FsPulseError;


#[derive(Clone, Debug, Default)]
pub struct RootPath {
    id: i64,
    path: String
}

impl RootPath {
    pub fn get(db: &Database, id: i64) -> Result<Option<Self>, FsPulseError> {
        let conn = &db.conn;

        match conn.query_row(
            "SELECT path FROM root_paths WHERE id = ?", 
            [id], 
            |row| row.get(0),
        ) {
            Ok(path) => Ok(Some(RootPath { id, path } )),
            Err(QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(FsPulseError::Database(e)),
        }
    }

    pub fn get_or_insert(db: &Database, path: &str) -> Result<Self, FsPulseError> {
        let conn = &db.conn;

        conn.execute("INSERT OR IGNORE INTO root_paths (path) VALUES (?)", [path])?;

        let id: i64 = conn.query_row(
            "SELECT id FROM root_paths WHERE path = ?",
            [path],
            |row| row.get(0),
        )?;

        Ok(RootPath { id, path: path.to_owned() })
    }

    pub fn id(&self) -> i64 {
        self.id
    }

    pub fn path(&self) -> &str {
        &self.path
    }

    pub fn for_each_root_path<F>(db: &Database, mut func: F) -> Result<(), FsPulseError> 
    where
        F: FnMut(&RootPath) -> Result<(), FsPulseError>,
    {
   
        let mut stmt = db.conn.prepare(
            "SELECT id, path
            FROM root_paths
            ORDER BY id ASC"
        )?;

        let rows = stmt.query_map([], |row| {
            Ok(RootPath {
                id: row.get::<_, i64>(0)?,      // root path id
                path: row.get::<_, String>(1)?, // path
            })
        })?;

        for row in rows {

            let root_path= row?;
            func(&root_path)?;
        }

        Ok(())
    }

    pub fn latest_scan(&self, db: &Database) -> Result<Option<i64>, FsPulseError> {
        let conn = &db.conn;

        match conn.query_row(
            "SELECT id 
            FROM scans
            WHERE root_path_id = ?
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