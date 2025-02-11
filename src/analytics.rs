use crate::error::DirCheckError;
use crate::database::Database;

use chrono::{ DateTime, Local, Utc };
use rusqlite::{ OptionalExtension, Result };

pub struct Analytics {
    // No fields
}

impl Analytics {
    pub fn do_changes(scan_id: Option<u64>, verbose: bool, db: &mut Database) -> Result<(), DirCheckError> {

        // Step1: Either use the provided scan_id or get the latest from the database
        let scan_id = scan_id.map(|id| id as i64).or_else(|| {
            db.conn.query_row(
                "SELECT scans.id
                FROM scans
                ORDER BY scans.id DESC LIMIT 1", 
                [], 
                |row| Ok(row.get(0)?),
            ).optional().ok().flatten()
        });

        let scan_id: i64 = if let Some(id) = scan_id {
            id
        } else {
            println!("No scan ID available.");
            return Ok(());
        };

        // Step 2: Get the root path
        let root_path: Option<String> = db.conn.query_row(
            "SELECT root_paths.path 
            FROM scans
            JOIN root_paths ON scans.root_path_id = root_paths.id
            WHERE scans.id = ?",
            [scan_id],
            |row| Ok(row.get(0)?),
        ).optional()?;
        
        let root_path = if let Some(path) = root_path {
            path
        } else {
            println!("Specified scan not found in the database.");
            return Ok(());
        };
    
        Analytics::changes_print_summary(&db, scan_id, &root_path)?;

        if verbose {
            Analytics::changes_print_verbose(&db, scan_id)?;
        }

        Ok(())
    }

    fn changes_print_summary(db: &Database, scan_id: i64, root_path: &String) -> Result<(), DirCheckError> {
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
        println!("Scan ID: {}", scan_id);
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

    fn changes_print_verbose(db: &Database, scan_id: i64) -> Result<(), DirCheckError> {

        let mut stmt = db.conn.prepare(
            "SELECT changes.change_type, entries.path
            FROM changes
            JOIN entries ON entries.id = changes.entry_id
            WHERE changes.scan_id = ?
            ORDER BY entries.path ASC",
        )?;
        
        let rows = stmt.query_map([scan_id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;

        for row in rows {
            let (change_type, path) = row?;
            println!("{}: {}", change_type, path);
        }

        Ok(())
    }

    pub fn do_scans(db: &mut Database, all: bool, count: u64) -> Result<(), DirCheckError> {
        let count: i64 = if all { -1 } else { count as i64 };
        let query = format!("
            SELECT scans.id, scans.scan_time, root_paths.path
            FROM scans
            JOIN root_paths ON scans.root_path_id = root_paths.id
            ORDER BY scans.id DESC
            LIMIT {}",
            count
        );

        let mut stmt = db.conn.prepare(&query)?;
        let rows = stmt.query_map([], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?, row.get::<_, String>(2)?))
        })?;

        for row in rows {
            let (id, scan_time, path) = row?;

            // Convert scan_time from UNIX timestamp to DateTime<Utc>
            let datetime_utc = DateTime::<Utc>::from_timestamp(scan_time, 0)
                .unwrap_or_default();

            // Convert to local time and format it
            let datetime_local = datetime_utc.with_timezone(&Local);
            let formatted_time = datetime_local.format("%Y-%m-%d %H:%M:%S");

            println!("Scan ID: {}, Time: {}, Path: {}", id, formatted_time, path);
        }

        Ok(())
    }
}