
use crate::change::{ChangeCounts, ChangeType};
use crate::error::DirCheckError;
use crate::database::Database;
use crate::scan::Scan;
use crate::utils::Utils;

use rusqlite::Result;

pub struct Reports {
    // No fields
}

impl Reports {
    pub fn do_report_scans(db: &mut Database, scan_id: Option<u64>, latest: bool, count: u64) -> Result<(), DirCheckError> {
        if scan_id.is_some() && latest {
            return Err(DirCheckError::Error("Cannot specify --id and --latest together.".to_string()));                    
        }
        
        // Handle the single scan case
        if scan_id.is_some() || latest {
            if count != 0 {
                return Err(DirCheckError::Error("Cannot provide either -id or --latest and --count".to_string()));       
            }

            Scan::with_id_or_latest(db, Utils::opt_u64_to_opt_i64(scan_id), |db, scan| Reports::scan_print_summary(db, scan))?;
        }
    
        /* 
        if verbose {
            Analytics::changes_print_verbose(&db, scan_id)?;
        }
        */

        Ok(())
    }

    pub fn scan_print_summary(db: &Database, scan: &Scan) -> Result<(), DirCheckError> {
        //let conn = &db.conn;
        let scan_id = scan.scan_id();

        let change_counts = ChangeCounts::from_scan_id(db, scan_id)?;

        println!("{}", "=".repeat(40));
        println!(" Scan Report - Scan ID: {}", scan_id);
        println!("{}", "=".repeat(40));

        println!("Root Path ID:   {}", scan.root_path_id());
        println!("Root Path:      {}", scan.root_path());
        println!("Time of Scan:   {}", Utils::formatted_db_time(scan.time_of_scan()));
        println!("");
        println!("{}", "-".repeat(40));
        // println!("Total Items:    {}", total_items);
        println!("Items Seen");
        println!("{}", "-".repeat(40));
        println!("Files:          {}", Utils::opt_i64_or_none_as_str(scan.file_count()));
        println!("Folders:        {}", Utils::opt_i64_or_none_as_str(scan.folder_count()));
        println!("");
        println!("{}", "-".repeat(40));
        println!("Changed Files and Folders");
        println!("{}", "-".repeat(40));
        println!("Added           {}", change_counts.get(ChangeType::Add));
        println!("Modified        {}", change_counts.get(ChangeType::Modify));
        println!("Delete          {}", change_counts.get(ChangeType::Delete));
        println!("Type Changed    {}", change_counts.get(ChangeType::TypeChange));

        Ok(())
    }

    /* 
    fn scan_print_changes(db: &Database, scan: &Scan) -> Result<(), DirCheckError> {
        let scan_id = scan.scan_id();

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
    } */
}