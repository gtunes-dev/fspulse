use crate::error::DirCheckError;
use crate::database::Database;

use rusqlite::{ OptionalExtension, Result };

pub struct Analytics<'a> {
    db: &'a mut Database,
}

impl<'a> Analytics<'a> {
    pub fn do_latest_changes(db: &mut Database) -> Result<(), DirCheckError> {
        // Step 1: Get the most recent scan_id and root path
        let latest_scan: Option<(i64, String)> = db.conn.query_row(
            "SELECT scans.id, root_paths.path 
             FROM scans 
             JOIN root_paths ON scans.root_path_id = root_paths.id 
             ORDER BY scans.id DESC LIMIT 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        ).optional()?;
    
        let Some((scan_id, root_path)) = latest_scan else {
            println!("No scans found in the database.");
            return Ok(());
        };
    
        // Step 2: Query the changes table to count occurrences of each change type
        let mut stmt = db.conn.prepare(
            "SELECT change_type, COUNT(*) FROM changes WHERE scan_id = ? GROUP BY change_type",
        )?;
        
        let mut rows = stmt.query([scan_id])?;
    
        let mut add_count = 0;
        let mut modify_count = 0;
        let mut delete_count = 0;
        let mut type_change_count = 0;
    
        while let Some(row) = rows.next()? {
            let change_type: String = row.get(0)?;
            let count: i64 = row.get(1)?;
    
            match change_type.as_str() {
                "A" => add_count = count,
                "M" => modify_count = count,
                "D" => delete_count = count,
                "T" => type_change_count = count,
                _ => println!("Warning: Unknown change type found in DB: {}", change_type),
            }
        }
    
        // Step 3: Count total files and directories seen in this scan
        let (file_count, folder_count): (i64, i64) = db.conn.query_row(
            "SELECT 
                SUM(CASE WHEN item_type = 'F' THEN 1 ELSE 0 END) AS file_count, 
                SUM(CASE WHEN item_type = 'D' THEN 1 ELSE 0 END) AS folder_count 
             FROM entries WHERE last_seen_scan_id = ?",
            [scan_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        ).unwrap_or((0, 0)); // If no data, default to 0
    
        let total_items = file_count + folder_count;
    
        // Step 4: Print results
        println!("Latest Scan ID: {}", scan_id);
        println!("Scanned Path:   {}", root_path);
        println!("Total Items:    {}", total_items);
        println!(" - Files:       {}", file_count);
        println!(" - Folders:     {}", folder_count);
        println!("+--------------------+--------+");
        println!("| Change Type       | Count  |");
        println!("+--------------------+--------+");
        println!("| Added Files       | {:>6} |", add_count);
        println!("| Modified Files    | {:>6} |", modify_count);
        println!("| Deleted Files     | {:>6} |", delete_count);
        println!("| Type Changes      | {:>6} |", type_change_count);
        println!("+--------------------+--------+");
    
        Ok(())
    }
}