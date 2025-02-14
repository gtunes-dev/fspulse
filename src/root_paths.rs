
use crate::database::Database;
use crate::error::DirCheckError;



#[derive(Debug, Default)]
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
}