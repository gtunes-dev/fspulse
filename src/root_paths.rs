use std::i64;

use crate::database::Database;
use crate::error::DirCheckError;


#[derive(Clone, Debug, Default)]
pub struct RootPath {
    id: i64,
    path: String
}

impl RootPath {
    pub fn get(db: &Database, id: i64) -> Result<Self, DirCheckError> {
        let conn = &db.conn;

        let path: String = conn.query_row(
        "SELECT path FROM root_paths WHERE id = ?",
            [id],
        |row| row.get(0),
        )?;

        Ok(RootPath{ id, path })
    }

    pub fn get_or_insert(db: &Database, path: &str) -> Result<Self, DirCheckError> {
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

    pub fn for_each_root_path<F>(db: &Database, scans: bool, count: Option<i64>, mut func: F) -> Result<i32, DirCheckError> 
    where
        F: FnMut(&Database, &RootPath, bool, Option<i64>) -> Result<(), DirCheckError>,
    {
        // if count isn't specified, the default is 10
        //let count = count.unwrap_or(i64::MAX);
        
        if count == Some(0) {
            return Ok(0); // Nothing to print
        }

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

        let mut path_count = 0;

        for row in rows {

            let root_path= row?;
            func(db, &root_path, scans, count)?;
            path_count += 1;
        }

        Ok(path_count)
    }

    
}