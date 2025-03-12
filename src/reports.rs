use crate::changes::{Change, ChangeType};
use crate::error::FsPulseError;
use crate::database::Database;
use crate::items::Item;
use crate::roots::Root;
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
        scan_id: Option<u32>, 
        last: u32, 
        format: ReportFormat,
    ) -> Result<(), FsPulseError> 
    {
        if scan_id.is_none() {
            Reports::print_scans(db, last)?;
        } else {
            let scan = Scan::new_from_id_else_latest(db, Utils::opt_u32_to_opt_i64(scan_id))?;
            Self::print_scan(&scan, format)?;
        }

        Ok(())
    }

    pub fn report_roots(db: &Database, root_id: Option<u32>, root_path: Option<String>, _format: ReportFormat) -> Result<(), FsPulseError> {
        if root_id.is_none() && root_path.is_none(){
            let mut stream = Reports::begin_roots_table();
            
            Root::for_each_root(
                db,
                |root| {
                    stream.row(root.clone())?;
                    Ok(())
                }
            )?;

            stream.finish()?;
        } else {
            let root_id: i64 = match (root_id, root_path) {
                (Some(root_id), _) => {
                    root_id.into()
                }
                (_, Some(root_path)) => {
                    Root::get_from_path(db, &root_path)?.id()
                }
                (None, None) => {
                    // should be unreachable
                    return Err(FsPulseError::Error("No path specified".to_string()))
                }
            };

            let root = Root::get_from_id(db, root_id)?
                .ok_or_else(|| FsPulseError::Error("Root Not Found".to_string()))?;
            let mut stream = Self::begin_roots_table()
                .title("Root");

            stream.row(root.clone())?;
            stream.finish()?;
        }

        Ok(())
    }

    pub fn report_items(db: &Database, item_id: Option<u32>, root_id: Option<u32>, format: ReportFormat) -> Result<(), FsPulseError> {

        // TODO: In the single item case, "tree" is not a valid report format
        if let Some(item_id) = item_id {
            let item = Item::get_by_id(db, item_id.into())?;

            let mut stream = Self::begin_items_table("Item", &format!("Item {} Not Found", item_id));

            if let Some(item) = item {
                stream.row(item)?;
            }

            stream.finish()?;
        } else if let Some(root_id) = root_id {
            let root = Root::get_from_id(db, root_id.into())?
                .ok_or_else(|| FsPulseError::Error(format!("Root id {} not found", root_id)))?;

            let scan = Scan::new_for_last_path_scan(db, root.id())?;

            match format {
                ReportFormat::Tree => Self::print_last_seen_scan_items_as_tree(db, &scan, &root)?,
                ReportFormat::Table => Self::print_last_seen_scan_items_as_table(db, &scan, &root)?,
                _ => return Err(FsPulseError::Error("Unsupported format.".to_string())),
            }
        }
        
        Ok(())
    }

    pub fn report_changes(
        db: &Database, 
        change_id: Option<u32>, 
        item_id: Option<u32>, 
        scan_id: Option<u32>, 
        format: ReportFormat
    ) -> Result<(), FsPulseError> {

        match (change_id, item_id, scan_id) {
            (Some(change_id), None, None) => {
                let change = Change::get_by_id(db, change_id.into())?;
                let mut stream = Self::begin_changes_table("Change", "No Change Found");
                if let Some(change) = change {
                    stream.row(change)?;
                }
                stream.finish()?;
            },
            (None, Some(item_id), None) => {

            },
            (None, None, Some(scan_id)) => {
                match format {
                    ReportFormat::Table => Self::print_scan_changes_as_table(db, scan_id.into())?,
                    ReportFormat::Tree => Self::print_scan_changes_as_tree(db, scan_id.into())?,
                    _ => return Err(FsPulseError::Error("Unsupported format.".to_string())),
                }
            },
            _ => {

            },
       }
        Ok(())
    }

    pub fn print_scan(scan: &Scan, _format: ReportFormat) -> Result<(), FsPulseError> {
        let mut stream = Reports::begin_scans_table("Scan", "No Scan");

        stream.row(scan.clone())?;
        stream.finish()?;

        Ok(())
    }

    fn print_scans(db: &Database, last: u32) -> Result<(), FsPulseError> {
        let mut stream = Reports::begin_scans_table("Scans", "No Scans");
        
        Scan::for_each_scan(
            db, 
            last, 
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
            Column::new(|f, s: &Scan| write!(f, "{}", s.root_id())).header("Root ID").right().min_width(6),
            Column::new(|f, s: &Scan| write!(f, "{}", s.is_deep())).header("Deep").center(),
            Column::new(|f, s: &Scan| write!(f, "{}", Utils::format_db_time_short(s.time_of_scan()))).header("Time"),
            Column::new(|f, s: &Scan| write!(f, "{}", Utils::opt_i64_or_none_as_str(s.file_count()))).header("Files").right().min_width(7),
            Column::new(|f, s: &Scan| write!(f, "{}", Utils::opt_i64_or_none_as_str(s.folder_count()))).header("Folders").right().min_width(7),
            Column::new(|f, s: &Scan| write!(f, "{}", s.is_complete())).header("Complete").center(),

            Column::new(|f, s: &Scan| write!(f, "{}", s.change_counts().count_of(ChangeType::Add))).header("Adds").right().min_width(7),
            Column::new(|f, s: &Scan| write!(f, "{}", s.change_counts().count_of(ChangeType::Modify))).header("Modifies").right().min_width(7),
            Column::new(|f, s: &Scan| write!(f, "{}", s.change_counts().count_of(ChangeType::Delete))).header("Deletes").right().min_width(7),
            Column::new(|f, s: &Scan| write!(f, "{}", s.change_counts().count_of(ChangeType::TypeChange))).header("T Changes").right().min_width(7),
        ]).title(title).empty_row(empty_row);

        stream
    }

    fn begin_roots_table() -> Stream<Root, Stdout> {
        let out = io::stdout();
        let stream = Stream::new(out, vec![
            Column::new(|f, root: &Root| write!(f, "{}", root.id())).header("ID").right().min_width(6),
            Column::new(|f, root: &Root| write!(f, "{}", root.path())).header("Path").left().min_width(109),
        ]).title("Roots").empty_row("No Rootss");

        stream
    }

    fn begin_items_table(title: &str, empty_row: &str) -> Stream<Item, Stdout> {
        let out = io::stdout();
        let stream = Stream::new(out, vec![
            Column::new(|f, i: &Item| write!(f, "{}", i.id())).header("ID").right().min_width(6),
            Column::new(|f, i: &Item| write!(f, "{}", i.root_id())).header("Root ID").right(),
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

    fn print_scan_changes_as_table(db: &Database, scan_id: i64) -> Result<(), FsPulseError> {
        let mut stream = Reports::begin_changes_table(&format!("Changes - Scan ID: {}", scan_id), "No Changes");

        Change::for_each_change_in_scan(
            db, 
            scan_id, 
            |change| {
                stream.row(change.clone())?;
                Ok(())
            }
        )?;

        stream.finish()?;

        Ok(())
    }
      
    fn print_scan_changes_as_tree(db: &Database, scan_id: i64) -> Result<(), FsPulseError> {
        let width = 100;

        let scan = Scan::new_from_id(db, scan_id)?;
        let root = Root::get_from_id(db, scan.root_id())?
            .ok_or_else(|| FsPulseError::Error("Root not found".to_string()))?;

        Self::print_center(width, "Changes");
        Self::print_center(width, &format!("Root Path: '{}'", root.path()));

        Self::hr(width);
    
        let root_path = Path::new(root.path());
        let mut path_stack: Vec<PathBuf> = Vec::new(); // Stack storing directory paths
        let mut change_count = 0;

         // TODO: identify changes as metadata and/or hash
        Change::for_each_change_in_scan(
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

                change_count += 1;
                Ok(())
            }
        )?;

        if change_count == 0 {
            Self::print_center(width, "No Changes");
        }

        Self::hr(width);    
        Ok(())
    }

    fn print_last_seen_scan_items_as_table(db: &Database, scan: &Scan, root: &Root) -> Result<(), FsPulseError> {
        let mut stream = 
            Self::begin_items_table(&format!("Items: {}", root.path()), "No Items");

        Item::for_each_item_in_latest_scan(
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

    fn print_last_seen_scan_items_as_tree(db: &Database, scan: &Scan, root: &Root) -> Result<(), FsPulseError> {
        
        // TODO: figure out a default width
        let width = 100;

        Self::print_center(width, "Items");
        Self::print_center(width, &format!("Root Path: '{}'", root.path()));
        Self::hr(width);

        let root_path = Path::new(root.path());
        let mut path_stack: Vec<PathBuf> = Vec::new();
        let mut item_count = 0;

        Item::for_each_item_in_latest_scan(
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
                item_count += 1;
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