

use crate::change::{ChangeCounts, ChangeType};
use crate::error::DirCheckError;
use crate::database::Database;
use crate::scans::Scan;
use crate::utils::Utils;

use rusqlite::Result;

pub struct Reports {
    // No fields
}

impl Reports {
    pub fn do_report_scans(db: &Database, scan_id: Option<i64>, latest: bool, count: u64, changes: bool) -> Result<(), DirCheckError> {
        
        // Handle the single scan case
        if scan_id.is_some() || latest {
            let scan = Scan::new_from_id(db, scan_id)?;
            //Scan::with_id_or_latest(db, scan_id,|db, scan| Self::scan_print_block(db, scan))?;
            Self::scan_print_block(db, &scan)?;

            if changes {
                Self::scan_print_changes(db, scan.id())?;
            }
        }

        Ok(())
    }

    fn print_marquee(title: &str, prefix_suffix: &str, fill_char: &str) {
        let total_width = 40;
        let prefix_fill_chars = 5;
        let title_padding = " ";

        let suffix_fill_length = total_width - ((prefix_suffix.len() * 2) + prefix_fill_chars + (title_padding.len() * 2) + title.len());

        println!(
            "{}{}{}{}{}{}{}",
            prefix_suffix,
            fill_char.repeat(prefix_fill_chars),
            title_padding,
            title,
            title_padding,
            fill_char.repeat(suffix_fill_length),
            prefix_suffix
        );
    }

    fn print_title(title: &str) {
        Self::print_marquee(title, "+", "="); 
    }

    fn print_section_header(header: &str) {
        println!();
        Self::print_marquee(header, "", "-");
    }

    fn print_none_if_zero(i: i32) {
        if i == 0 {
            println!("None.");
        }
    }

    pub fn scan_print_block(db: &Database, scan: &Scan) -> Result<(), DirCheckError>{
        // Still necessary?
        // let change_counts = ChangeCounts::from_scan_id(db, scan_id)?;
        Self::print_title("Scan");
        println!("Database:       {}", db.path());
        println!("Id:             {}", scan.id());
        println!("Root Path ID:   {}", scan.root_path_id());
        println!("Root Path:      {}", scan.root_path());
        println!("Time of Scan:   {}", Utils::formatted_db_time(scan.time_of_scan()));
        println!("");
        // println!("Total Items:    {}", total_items);
        
        Self::print_section_header("Items Seen");
        println!("Files:          {}", Utils::opt_i64_or_none_as_str(scan.file_count()));
        println!("Folders:        {}", Utils::opt_i64_or_none_as_str(scan.folder_count()));
        println!("");

        Self::print_section_header("Changes");
        let change_counts = scan.change_counts();
        println!("Add             {}", change_counts.get(ChangeType::Add));
        println!("Modify          {}", change_counts.get(ChangeType::Modify));
        println!("Delete          {}", change_counts.get(ChangeType::Delete));
        println!("Type Change     {}", change_counts.get(ChangeType::TypeChange));

        Ok(())
    }
    

    fn scan_print_changes(db: &Database, scan_id: i64) -> Result<(), DirCheckError> {
        Self::print_section_header("Changed Entries");

        let entry_change_count = Self::with_each_scan_change(db, scan_id, Self::print_change_as_line)?;
        Self::print_none_if_zero(entry_change_count);

        Ok(())
    }

    fn with_each_scan_change<F>(db: &Database, scan_id: i64, func: F) -> Result<i32, DirCheckError>
    where
        F: Fn(i64, &str, &str, &str),
    {
        let mut change_count = 0;

        let mut stmt = db.conn.prepare(
            "SELECT entries.id, changes.change_type, entries.item_type, entries.path
            FROM changes
            JOIN entries ON entries.id = changes.entry_id
            WHERE changes.scan_id = ?
            ORDER BY entries.path ASC",
        )?;
        
        let rows = stmt.query_map([scan_id], |row| {
            Ok((
                row.get::<_, i64>(0)?,      // Entry ID
                row.get::<_, String>(1)?,   // Change type (A, M, D, etc.)
                row.get::<_, String>(2)?,   // Item type (F, D)
                row.get::<_, String>(3)?,   // Path
            ))
        })?;
        
        for row in rows {
            let (id, change_type, item_type, path) = row?;

            func(id, &change_type, &item_type, &path);
            change_count = change_count + 1;
        }
        Ok(change_count)
    }

    fn print_change_as_line(id: i64, change_type: &str, item_type: &str, path: &str) {
        println!("[{},{},{}] {}", id, change_type, item_type, path);
    }


     /* 

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