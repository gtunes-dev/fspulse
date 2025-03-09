use crate::changes::{Change, ChangeType};
use crate::error::FsPulseError;
use crate::database::Database;
use crate::items::Item;
use crate::root_paths::RootPath;
use crate::scans::Scan;
use crate::utils::Utils;

use std::io::{self, Stdout};
use std::path::{Path, PathBuf};
use std::str::FromStr;
use rusqlite::Result;
use tablestream::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReportFormat {
    Tree,
    Table,
    Csv,
}

impl FromStr for ReportFormat {
    type Err = FsPulseError;
    fn from_str(s: &str) -> Result<Self, FsPulseError> {
        match s.to_lowercase().as_str() {
            "tree" => Ok(ReportFormat::Tree),
            "table" => Ok(ReportFormat::Table),
            "csv" => Ok(ReportFormat::Csv),
            _ => Err(FsPulseError::Error("Invalid format specified.".to_string())),
        }
    }
}

pub struct Reports {
    // No fields
}

impl Reports {
    pub fn report_scans(
        db: &Database, 
        scan_id: Option<i64>, 
        latest: bool, 
        count: Option<i64>, 
        changes: bool, 
        items: bool,
        format: ReportFormat,
    ) -> Result<(), FsPulseError> {
        // Handle the single scan case. "Latest" conflicts with "id" so if 
        // the caller specified "latest", scan_id will be None
        if scan_id.is_none() && !latest {
            Reports::print_scans(db, count)?;
        } else {
            let scan = Scan::new_from_id_else_latest(db, scan_id)?;
            Self::print_scan(db, &scan, changes, items, format)?;
        }

        Ok(())
    }

    pub fn report_root_paths(db: &Database, root_path_id: Option<i64>, items: bool) -> Result<(), FsPulseError> {
        if root_path_id.is_none() {
            let mut stream = Reports::begin_root_paths_table();
            
            RootPath::for_each_root_path(
                db,
                |rp| {
                    stream.row(rp.clone())?;
                    Ok(())
                }
            )?;

            stream.finish()?;
        } else {
            let root_path_id = root_path_id.unwrap();
            let root_path = RootPath::get(db, root_path_id)?
                .ok_or_else(|| FsPulseError::Error("Root Path Not Found".to_string()))?;
            let mut stream = Self::begin_root_paths_table()
                .title("Root Path");

            stream.row(root_path.clone())?;
            let table_width = stream.finish()?;

            if items {
                let scan_id = root_path.latest_scan(db)?;

                if scan_id.is_none() {
                    Self::print_center(table_width, "No Last Scan - No Items");
                    Self::hr(table_width);
                    return Ok(());
                }

                let scan = Scan::new_from_id_else_latest(db, scan_id)?;

                Self::print_scan(db, &scan, false, true, ReportFormat::Table)?;
            }
        }

        Ok(())
    }

    pub fn report_items(db: &Database, item_id: i64) -> Result<(), FsPulseError> {
        let mut stream = Self::begin_items_table("Item", "No Item");

        let item = Item::new(db, item_id)?;
        if item.is_some() {
            stream.row(item.unwrap())?;
        }
        stream.finish()?;

        Ok(())
    }

    pub fn print_scan(db: &Database, scan: &Scan, changes: bool, items: bool, format: ReportFormat) -> Result<(), FsPulseError> {
        let mut stream = Reports::begin_scans_table("Scan", "No Scan");

        stream.row(scan.clone())?;
        let table_width = stream.finish()?;

        if changes || items {
            let root_path = RootPath::get(db, scan.root_path_id())?
                .ok_or_else(|| FsPulseError::Error("Root Path Not Found".to_string()))?;

            if changes {
                match format {
                    ReportFormat::Tree => Self::print_scan_changes_as_tree(db, table_width, &scan, &root_path)?,
                    ReportFormat::Table => Self::print_scan_changes_as_table(db, scan, &root_path)?,
                    _ => return Err(FsPulseError::Error("Unsupported format.".to_string())),
                }
            }

            if items {
                match format {
                    ReportFormat::Tree => Self::print_last_seen_scan_items_as_tree(db, table_width, &scan, &root_path)?,
                    ReportFormat::Table => Self::print_last_seen_scan_items_as_table(db, &scan, &root_path)?,
                    _ => return Err(FsPulseError::Error("Unsupported format.".to_string())),
                }
            }
        }

        Ok(())
    }

    fn print_scans(db: &Database, count: Option<i64>) -> Result<(), FsPulseError> {
        let mut stream = Reports::begin_scans_table("Scans", "No Scans");
        
        Scan::for_each_scan(
            db, 
            count, 
            |_db, scan| {
                stream.row(scan.clone())?;
                Ok(())
            }
        )?;

        stream.finish()?;

        Ok(())
    }

    fn begin_scans_table(title: &str, empty_row: &str) -> Stream<Scan, Stdout> {
        let out = io::stdout();
        let stream = Stream::new(out, vec![
            Column::new(|f, s: &Scan| write!(f, "{}", s.id())).header("ID").right().min_width(6),
            Column::new(|f, s: &Scan| write!(f, "{}", s.root_path_id())).header("Path ID").right().min_width(6),
            Column::new(|f, s: &Scan| write!(f, "{}", s.is_deep())).header("Deep").center(),
            Column::new(|f, s: &Scan| write!(f, "{}", Utils::format_db_time_short(s.time_of_scan()))).header("Time"),
            Column::new(|f, s: &Scan| write!(f, "{}", Utils::opt_i64_or_none_as_str(s.file_count()))).header("Files").right().min_width(7),
            Column::new(|f, s: &Scan| write!(f, "{}", Utils::opt_i64_or_none_as_str(s.folder_count()))).header("Folders").right().min_width(7),
            Column::new(|f, s: &Scan| write!(f, "{}", s.is_complete())).header("Complete").center(),

            Column::new(|f, s: &Scan| write!(f, "{}", s.change_counts().get(ChangeType::Add))).header("Adds").right().min_width(7),
            Column::new(|f, s: &Scan| write!(f, "{}", s.change_counts().get(ChangeType::Modify))).header("Modifies").right().min_width(7),
            Column::new(|f, s: &Scan| write!(f, "{}", s.change_counts().get(ChangeType::Delete))).header("Deletes").right().min_width(7),
            Column::new(|f, s: &Scan| write!(f, "{}", s.change_counts().get(ChangeType::TypeChange))).header("T Changes").right().min_width(7),
        ]).title(title).empty_row(empty_row);

        stream
    }

    fn begin_root_paths_table() -> Stream<RootPath, Stdout> {
        let out = io::stdout();
        let stream = Stream::new(out, vec![
            Column::new(|f, rp: &RootPath| write!(f, "{}", rp.id())).header("ID").right().min_width(6),
            Column::new(|f, rp: &RootPath| write!(f, "{}", rp.path())).header("Path").left().min_width(109),
        ]).title("Root Paths").empty_row("No Root Paths");

        stream
    }

    fn begin_items_table(title: &str, empty_row: &str) -> Stream<Item, Stdout> {
        let out = io::stdout();
        let stream = Stream::new(out, vec![
            Column::new(|f, i: &Item| write!(f, "{}", i.id())).header("ID").right().min_width(6),
            Column::new(|f, i: &Item| write!(f, "{}", i.root_path_id())).header("Path ID").right(),
            Column::new(|f, i: &Item| write!(f, "{}", i.last_seen_scan_id())).header("Last Scan").right(),
            Column::new(|f, i: &Item| write!(f, "{}", i.is_tombstone())).header("Tombstone").center(),
            Column::new(|f, i: &Item| write!(f, "{}", i.item_type())).header("Type").center(),
            Column::new(|f, i: &Item| write!(f, "{}", i.path())).header("Path").left(),
            Column::new(|f, i: &Item| write!(f, "{}", Utils::format_db_time_short_or_none(i.last_modified()))).header("Modified").left(),
            Column::new(|f, i: &Item| write!(f, "{}", Utils::opt_i64_or_none_as_str(i.file_size()))).header("Size").right(),
            Column::new(|f, i: &Item| write!(f, "{}", i.file_hash().unwrap_or("-"))).header("Hash").center(),
        ]).title(title).empty_row(empty_row);
        
        stream
    }

    fn begin_changes_table(title: &str, empty_row: &str) -> Stream<Change, Stdout> {
        let out = io::stdout();
        let stream = Stream::new(out, vec![
            Column::new(|f, c: &Change| write!(f, "{}", c.id)).header("ID").right().min_width(6),
            Column::new(|f, c: &Change| write!(f, "{}", c.item_id)).header("Item ID").right(),
            Column::new(|f, c: &Change| write!(f, "{}", c.item_type)).header("Item Type").center(),
            Column::new(|f, c: &Change| write!(f, "{}", c.item_path)).header("Item Path").left(),
            Column::new(|f, c: &Change| write!(f, "{}", c.change_type)).header("Change Type").center(),
            Column::new(|f, c: &Change| write!(f, "{}", Utils::opt_bool_or_none_as_str(c.metadata_changed))).header("Meta Changed").center(),
            Column::new(|f, c: &Change| write!(f, "{}", Utils::format_db_time_short_or_none(c.prev_last_modified))).header("Prev Modified").center(),
            Column::new(|f, c: &Change| write!(f, "{}", Utils::opt_i64_or_none_as_str(c.prev_file_size))).header("Prev Size").right(),
            Column::new(|f, c: &Change| write!(f, "{}", Utils::opt_bool_or_none_as_str(c.hash_changed))).header("Hash Changed").center(),
            Column::new(|f, c: &Change| write!(f, "{}", Utils::opt_string_or_none(&c.prev_hash))).header("Prev Hash").center(),
        ]).title(title).empty_row(empty_row);

        stream
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
            // which is covered by the item at the top of the stack - we only
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

    fn print_scan_changes_as_table(db: &Database, scan: &Scan, _root_path: &RootPath) -> Result<(), FsPulseError> {
        let mut stream = Reports::begin_changes_table("Scans", "No Scans");

        Change::with_each_scan_change(
            db, 
            scan.id(), 
            |change| {
                stream.row(change.clone())?;
                Ok(())
            }
        )?;

        stream.finish()?;

        Ok(())
    }
      
    fn print_scan_changes_as_tree(db: &Database, width: usize, scan: &Scan, root_path: &RootPath) -> Result<(), FsPulseError> {
        Self::print_center(width, "Changes");
        Self::print_center(width, &format!("Root Path: {}", root_path.path()));

        Self::hr(width);
    
        let root_path = Path::new(root_path.path());
        let mut path_stack: Vec<PathBuf> = Vec::new(); // Stack storing directory paths

         // TODO: identify changes as metadata and/or hash
        let change_count = Change::with_each_scan_change(
            db, 
            scan.id(), 
            |change| {
                let is_dir = change.item_type == "D";

                let (indent_level, new_path) = Self::get_tree_path(
                    &mut path_stack, 
                    root_path, 
                    &change.item_path,
                    is_dir,
                );

                // Print the item
                println!("{}[{}] {}{} ({})", 
                    " ".repeat(indent_level * 4), 
                    change.change_type, 
                    new_path.to_string_lossy(),
                    Utils::dir_sep_or_empty(is_dir),
                    change.id,
                );
                Ok(())
            }
        )?;

        if change_count == 0 {
            Self::print_center(width, "No Changes");
        }

        Self::hr(width);    
        Ok(())
    }

    /* 
    fn with_each_scan_change<F>(db: &Database, scan_id: i64, mut func: F) -> Result<i32, FsPulseError>
    where
        F: FnMut(i64, &str, Option<bool>, Option<bool>, &str, &str),
    {
        let mut change_count = 0;

        let mut stmt = db.conn.prepare(
            "SELECT items.id, changes.change_type, changes.metadata_changed, changes.hash_changed, items.item_type, items.path
            FROM changes
            JOIN items ON items.id = changes.item_id
            WHERE changes.scan_id = ?
            ORDER BY items.path ASC"
        )?;
        
        let rows = stmt.query_map([scan_id], |row| {
            Ok((
                row.get::<_, i64>(0)?,          // Item ID
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
    */

    fn print_last_seen_scan_items_as_table(db: &Database, scan: &Scan, root_path: &RootPath) -> Result<(), FsPulseError> {
        let mut stream = 
            Self::begin_items_table(&format!("Items: {}", root_path.path()), "No Items");

        Item::with_each_last_seen_scan_item(
            db, 
            scan.id(),
            |item|  {
                stream.row(item.clone())?;
                Ok(())
            }
        )?;

        stream.finish()?;

        Ok(())
    }

    fn print_last_seen_scan_items_as_tree(db: &Database, width: usize, scan: &Scan, root_path: &RootPath) -> Result<(), FsPulseError> {
        Self::print_center(width, "Items");
        Self::print_center(width, &format!("Root Path: {}", root_path.path()));
        Self::hr(width);

        let root_path = Path::new(root_path.path());
        let mut path_stack: Vec<PathBuf> = Vec::new();

        let item_count = Item::with_each_last_seen_scan_item(
            db, 
            scan.id(), 
            |item| {
                let is_dir = item.item_type() == "D";

                let (indent_level, new_path) = Self::get_tree_path(&mut path_stack, root_path, item.path(), is_dir);

                // Print the item
                println!("{}[{}] {}{}",
                    " ".repeat(indent_level * 4), 
                    item.id(),
                    new_path.to_string_lossy(),
                    Utils::dir_sep_or_empty(is_dir),
                );
                Ok(())
            }
        )?;

        if item_count == 0 {
            Self::print_center(width, "No Items");
        }

        Self::hr(width);

        Ok(())
    }

    fn hr(width: usize) {
        println!("{1:-<0$}", width, ""); 
    }

    fn __print_left(width: usize, value: &str) {
        println!("{0:1$}{3}{0:2$}", "", 0, width - value.len(), value);
    }

    fn print_center(width: usize, value: &str) {
        // determine left padding
        let padding = width - value.len();
        let lpad = padding / 2;
        let rpad = lpad + (padding % 2);
        println!("{0:1$}{3}{0:2$}", "", lpad, rpad, value);

    }
}