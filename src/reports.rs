use crate::changes::ChangeType;
use crate::error::DirCheckError;
use crate::database::Database;
use crate::root_paths::RootPath;
use crate::scans::Scan;
use crate::utils::Utils;

use std::path::{Path, PathBuf};
use rusqlite::Result;

pub struct Reports {
    // No fields
}

impl Reports {
    const REPORT_WIDTH: usize = 80;

    pub fn do_report_scans(db: &Database, scan_id: Option<i64>, latest: bool, count: Option<u64>, changes: bool, entries: bool) -> Result<(), DirCheckError> {
        // Handle the single scan case. "Latest" conflicts with "id" so if 
        // the caller specified "latest", scan_id will be None
        if count.is_some() {
            Self::print_latest_scans(db, count.unwrap())?;
        } else {
            let scan = Scan::new_from_id(db, scan_id)?;
            Self::print_scan_block(db, &scan)?;

            if changes {
                Self::print_scan_changes(db, &scan)?;
            }

            if entries {
                Self::print_scan_entries(db, &scan)?;
            }
        }

        Ok(())
    }

    pub fn report_root_paths(db: &Database, root_path_id: Option<i64>, _path: Option<String>, _scans: bool, _count: u64) -> Result<(), DirCheckError> {
        if let Some(root_path_id) = root_path_id {
            let root_path = RootPath::get(db, root_path_id)?;
            Self::print_root_path_block(&root_path);
        }
        
        Ok(())
    }

    fn marquee_title_fill_length(s: &str, indent: &str) -> usize {
        Self::REPORT_WIDTH.saturating_sub(
            s.len()
            + 2 // end caps
            + indent.len()
        )
    }

    fn print_marquee(title: &str, subtitle: &str, endcap_char: &str, fill_char: &str) {
        let indent = "  ";
        let space_fill = " ";
        //let subtitle_endchap = "|";
        //let prefix_fill_chars = 5;
        //let title_padding = if title.is_empty() { "" } else { " " };

        println!("{}{}{}", endcap_char, fill_char.repeat(Self::REPORT_WIDTH - 2), endcap_char);

        let mut fill_length = Self::marquee_title_fill_length(title, indent);
        println!("{}{}{}{}{}", endcap_char, indent, title, space_fill.repeat(fill_length), endcap_char);

        // TODO: print subtitle
        if subtitle.len() > 0 {
            fill_length = Self::marquee_title_fill_length(subtitle, indent);
            println!("{}{}{}{}{}", endcap_char, indent, subtitle, space_fill.repeat(fill_length), endcap_char);
        }

        println!("{}{}{}", endcap_char, fill_char.repeat(Self::REPORT_WIDTH - 2), endcap_char);
    }

    fn print_title(title: &str) {
        println!("{}\n{}", title, "=".repeat(title.len()));
    }

    fn print_section_header(header: &str) {
        println!("\n{}\n{}", header, "-".repeat(header.len()));
    }

    fn print_none_if_zero(i: i32) {
        if i == 0 {
            println!("None.");
        }
    }

    pub fn print_root_path_block(root_path: &RootPath) {
        Self::print_title("Root Path");
        println!("Root Path:      {}", root_path.path());
        println!("Id:             {}", root_path.id());
    }

    pub fn print_scan_block(db: &Database, scan: &Scan) -> Result<(), DirCheckError>{
        Self::print_title("Scan Report");
        println!("Id:             {}", scan.id());
        println!("Root Path ID:   {}", scan.root_path_id());
        println!("Root Path:      {}", scan.root_path());
        println!("Deep Scan:      {}", scan.is_deep());
        println!("Time of Scan:   {}", Utils::formatted_db_time(scan.time_of_scan()));
        println!("Completed:      {}", scan.is_complete());
        println!("Database:       {}", db.path());
        // println!("Total Items:    {}", total_items);
        
        Self::print_section_header("Items Seen");
        println!("Files:          {}", Utils::opt_i64_or_none_as_str(scan.file_count()));
        println!("Folders:        {}", Utils::opt_i64_or_none_as_str(scan.folder_count()));

        Self::print_section_header("Changes");
        let change_counts = scan.change_counts();
        println!("Add             {}", change_counts.get(ChangeType::Add));
        println!("Modify          {}", change_counts.get(ChangeType::Modify));
        println!("Delete          {}", change_counts.get(ChangeType::Delete));
        println!("Type Change     {}", change_counts.get(ChangeType::TypeChange));

        Ok(())
    }

    fn print_scan_as_line(id: i64, root_path_id: i64, time_of_scan: i64, is_complete: bool, root_path: &str) {
        println!("[{},{},{},{}] {}",
            id,
            Utils::formatted_db_time(time_of_scan),
            root_path_id,
            is_complete,
            root_path
        );
    }

    fn print_latest_scans(db: &Database, n: u64) -> Result<(), DirCheckError> {
        let mut scan_count = 0;

        Self::print_title("Latest Scans");

 /*        
        Self::print_title(
            &format!("Latest Scans ({})", n),
            "[scan id, root path id, time of scan, scan completed] {path}",
        );
 */

        let mut stmt = db.conn.prepare(
            "SELECT scans.id, scans.root_path_id, scans.time_of_scan, scans.is_complete, root_paths.path
            FROM scans
            JOIN root_paths ON root_paths.id = scans.root_path_id
            ORDER BY scans.id DESC
            LIMIT ?"
        )?;

        let rows = stmt.query_map([n], |row| {
            Ok((
                row.get::<_, i64>(0)?,      // scan id
                row.get::<_, i64>(1)?,      // root path id
                row.get::<_, i64>(2)?,      // time of scan
                row.get::<_, bool>(3)?,     // is complete
                row.get::<_, String>(4)?,   // root path
            ))
        })?;


        for row in rows {
            let (id, root_path_id, time_of_scan, is_complete, root_path) = row?;
            Self::print_scan_as_line(id, root_path_id, time_of_scan, is_complete, &root_path);

            scan_count = scan_count + 1;
        }

        Self::print_none_if_zero(scan_count);

        Ok(())
    }
    

    fn get_tree_path(path_stack: &mut Vec<PathBuf>, root_path: &Path, path: &str, is_dir: bool) -> (usize, PathBuf) {
        // Reduce path to the portion that is relative to the root
        let path = Path::new(path).strip_prefix(root_path).unwrap();
        let parent = path.parent();

        let mut new_path = path;

        // Wind the stack down to the first path that is a parent of the current item
        while let Some(stack_path) = path_stack.last() {
            // if the path at the top of the stack is a prefix of the current path
            // we stop pruning the stack. We now remove the portion of new_path
            // which is covered by the entry at the top of the stack - we only
            // want to print the portion that hasn't already been printed
            if path.starts_with(stack_path) {
                new_path = path.strip_prefix(stack_path).unwrap();
                break;
            }
            path_stack.pop();
        }
        if !is_dir {
            if let Some(structural_component) = new_path.parent() {
                let structural_component_str = structural_component.to_string_lossy();
                if !structural_component_str.is_empty() {
                    println!("{}{}/", " ".repeat(path_stack.len() * 4), structural_component_str);
                    path_stack.push(parent.unwrap().to_path_buf());

                    // The structural path has been pushed. The new_path is now just the filename
                    new_path = Path::new(new_path.file_name().unwrap());
                }
            }
        }

        let indent_level = path_stack.len();

        // If it's a directory, push it onto the stack
        if is_dir {
            path_stack.push(path.to_path_buf());
        }

        (indent_level, new_path.to_path_buf())
    }
      
    fn print_scan_changes(db: &Database, scan: &Scan) -> Result<(), DirCheckError> {
        Self::print_section_header("Changed Entries");
    
        let root_path = Path::new(scan.root_path());
        let mut path_stack: Vec<PathBuf> = Vec::new(); // Stack storing directory paths
    
        // TODO: identify changes as metadata and/or hash
        let change_count = Self::with_each_scan_change(
            db,
            scan.id(),
            |id, change_type, _metadata_changed, _hash_changed, item_type, path| {
                let is_dir = item_type == "D";

                let (indent_level, new_path) = Self::get_tree_path(
                    &mut path_stack, 
                    root_path, 
                    path,
                    is_dir,
                );

                // Print the item
                println!("{}[{}] {}{} ({})", 
                    " ".repeat(indent_level * 4), 
                    change_type, 
                    new_path.to_string_lossy(),
                    Utils::dir_sep_or_empty(is_dir),
                    id,
                );
            }
        )?;
               
        Self::print_none_if_zero(change_count);

        Ok(())
    }

    fn with_each_scan_change<F>(db: &Database, scan_id: i64, mut func: F) -> Result<i32, DirCheckError>
    where
        F: FnMut(i64, &str, Option<bool>, Option<bool>, &str, &str),
    {
        let mut change_count = 0;

        let mut stmt = db.conn.prepare(
            "SELECT entries.id, changes.change_type, changes.metadata_changed, changes.hash_changed, entries.item_type, entries.path
            FROM changes
            JOIN entries ON entries.id = changes.entry_id
            WHERE changes.scan_id = ?
            ORDER BY entries.path ASC"
        )?;
        
        let rows = stmt.query_map([scan_id], |row| {
            Ok((
                row.get::<_, i64>(0)?,          // Entry ID
                row.get::<_, String>(1)?,       // Change type (A, M, D, etc.)
                row.get::<_, Option<bool>>(2)?, // Metadata Changed
                row.get::<_, Option<bool>>(3)?, // Hash Changed
                row.get::<_, String>(4)?,       // Item type (F, D)
                row.get::<_, String>(5)?,       // Path
            ))
        })?;
        
        for row in rows {
            let (id, change_type, metadata_changed, hash_changed, item_type, path) = row?;

            func(id, &change_type, metadata_changed, hash_changed, &item_type, &path);
            change_count = change_count + 1;
        }
        Ok(change_count)
    }

    fn print_scan_entries(db: &Database, scan: &Scan) -> Result<(), DirCheckError> {
        Self::print_section_header("Entries");
        //Self::print_section_header("Entries",  "Legend: [Entry ID, Item Type, Last Modified, Size] path");

        let root_path = Path::new(scan.root_path());
        let mut path_stack: Vec<PathBuf> = Vec::new();

        let entry_count = Self::with_each_scan_entry(
            db, 
            scan.id(), 
            |id, path, item_type, _last_modified, _file_size, _file_hash| {
                let is_dir = item_type == "D";

                let (indent_level, new_path) = Self::get_tree_path(&mut path_stack, root_path, path, is_dir);

                // Print the item
                println!("{}[{}] {}{}",
                    " ".repeat(indent_level * 4), 
                    id,
                    new_path.to_string_lossy(),
                    Utils::dir_sep_or_empty(is_dir),
                );
            }
        )?;

        Self::print_none_if_zero(entry_count);
        Ok(())
    }

    pub fn with_each_scan_entry<F>(db: &Database, scan_id: i64, mut func: F) -> Result<i32, DirCheckError>
    where
        F: FnMut(i64, &str, &str, i64, Option<i64>, Option<String>),
    {
        let mut entry_count = 0;

        let mut stmt = db.conn.prepare(
            "SELECT id, path, item_type, last_modified, file_size, file_hash
            FROM entries
            WHERE last_seen_scan_id = ?
            ORDER BY path ASC"
        )?;
        
        let rows = stmt.query_map([scan_id], |row| {
            Ok((
                row.get::<_, i64>(0)?,              // Entry ID
                row.get::<_, String>(1)?,           // Path
                row.get::<_, String>(2)?,           // Item type
                row.get::<_, i64>(3)?,              // Last modified
                row.get::<_, Option<i64>>(4)?,      // File size (can be null
                row.get::<_, Option<String>>(5)?,   // File Hash (can be null)
            ))
        })?;
        
        for row in rows {
            let (id, path, item_type, last_modified, file_size, file_hash) = row?;

            func(id, &path, &item_type, last_modified, file_size, file_hash);
            entry_count = entry_count + 1;
        }
        Ok(entry_count)
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