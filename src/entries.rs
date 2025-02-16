use crate::database::Database;
use crate::error::DirCheckError;

pub struct Entry {
    // No fields yet
}

impl Entry {
    pub fn with_each_scan_entry<F>(db: &Database, scan_id: i64, func: F) -> Result<i32, DirCheckError>
    where
        F: Fn(i64, &str, &str, i64, Option<i64>),
    {
        // id, path, item_type, last_modified, file_size

        let mut entry_count = 0;

        let mut stmt = db.conn.prepare(
            "SELECT id, path, item_type, last_modified, file_size
            FROM entries
            WHERE last_seen_scan_id = ?
            ORDER BY path ASC"
        )?;
        
        let rows = stmt.query_map([scan_id], |row| {
            Ok((
                row.get::<_, i64>(0)?,          // Entry ID
                row.get::<_, String>(1)?,       // Path
                row.get::<_, String>(2)?,       // Item type
                row.get::<_, i64>(3)?,          // Last modified
                row.get::<_, Option<i64>>(4)?,  // File size (can be null
            ))
        })?;
        
        for row in rows {
            let (id, path, item_type, last_modified, file_size) = row?;

            func(id, &path, &item_type, last_modified, file_size);
            entry_count = entry_count + 1;
        }
        Ok(entry_count)
    }
}