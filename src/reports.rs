use crate::changes::{Change, ChangeCounts, ChangeType};
use crate::database::Database;
use crate::error::FsPulseError;
use crate::hash::Hash;
use crate::items::Item;
use crate::roots::Root;
use crate::scans::Scan;
use crate::utils::Utils;

use console::Style;
use rusqlite::Result;
use std::cmp::max;
use std::io::{self, Stdout};
use std::path::{Path, PathBuf};
use std::str::FromStr;
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
    ) -> Result<(), FsPulseError> {
        match scan_id {
            Some(scan_id) => {
                let scan = Scan::get_by_id(db, scan_id.into())?
                    .ok_or_else(|| FsPulseError::Error(format!("Scan Id {} not found", scan_id)))?;
                Self::print_scan(db, &scan, format)?;
            }
            None => Reports::print_scans(db, last)?,
        }

        Ok(())
    }

    pub fn report_roots(
        db: &Database,
        root_id: Option<u32>,
        root_path: Option<String>,
        _format: ReportFormat,
    ) -> Result<(), FsPulseError> {
        if root_id.is_none() && root_path.is_none() {
            let mut stream = Reports::begin_roots_table();

            Root::for_each_root(db, |root| {
                stream.row(root.clone())?;
                Ok(())
            })?;

            stream.finish()?;
        } else {
            let root_id: i64 = match (root_id, root_path) {
                (Some(root_id), _) => root_id.into(),
                (_, Some(root_path)) => Root::get_by_path(db, &root_path)?
                    .ok_or_else(|| {
                        FsPulseError::Error(format!("Root Path '{}' not found", &root_path))
                    })?
                    .id(),
                (None, None) => {
                    // should be unreachable
                    return Err(FsPulseError::Error("No path specified".to_string()));
                }
            };

            let root = Root::get_by_id(db, root_id)?
                .ok_or_else(|| FsPulseError::Error("Root Not Found".to_string()))?;
            let mut stream = Self::begin_roots_table().title("Root");

            stream.row(root.clone())?;
            stream.finish()?;
        }

        Ok(())
    }

    pub fn report_items(
        db: &Database,
        item_id: Option<u32>,
        item_path: Option<String>,
        root_id: Option<u32>,
        invalid: bool,
        format: ReportFormat,
    ) -> Result<(), FsPulseError> {
        match (item_id, item_path, root_id) {
            (Some(item_id), _, _) => {
                // TODO: In the single item case, "tree" is not a valid report format
                let item = Item::get_by_id(db, item_id.into())?;

                let mut stream =
                    Self::begin_items_table("Item", &format!("Item {} Not Found", item_id));

                if let Some(item) = item {
                    stream.row(item)?;
                }

                stream.finish()?;
            }
            (_, Some(item_path), _) => {
                let mut stream = Self::begin_items_table(
                    "Items",
                    &format!("Item Path '{}' Not Found", item_path),
                );
                Item::for_each_item_with_path(db, &item_path, |item| {
                    stream.row(item.clone())?;
                    Ok(())
                })?;

                stream.finish()?;
            }
            (_, _, Some(root_id)) => {
                let root = Root::get_by_id(db, root_id.into())?
                    .ok_or_else(|| FsPulseError::Error(format!("Root Id {} not found", root_id)))?;

                if invalid {
                    Self::print_invalid_items_as_table(db, &root)?;
                } else {
                    let scan = Scan::get_latest_for_root(db, root.id())?.ok_or_else(|| {
                        FsPulseError::Error(format!("No latest scan found for Root Id {}", root_id))
                    })?;

                    match format {
                        ReportFormat::Tree => {
                            Self::print_last_seen_scan_items_as_tree(db, &scan, &root)?
                        }
                        ReportFormat::Table => {
                            Self::print_last_seen_scan_items_as_table(db, &scan, &root)?
                        }
                        _ => return Err(FsPulseError::Error("Unsupported format.".to_string())),
                    }
                }
            }
            _ => {
                // Should never get here
            }
        }

        Ok(())
    }

    pub fn report_changes(
        db: &Database,
        change_id: Option<u32>,
        item_id: Option<u32>,
        scan_id: Option<u32>,
        format: ReportFormat,
    ) -> Result<(), FsPulseError> {
        match (change_id, item_id, scan_id) {
            (Some(change_id), None, None) => {
                let change = Change::get_by_id(db, change_id.into())?;
                let mut stream = Self::begin_changes_table("Change", "No Change Found");
                if let Some(change) = change {
                    stream.row(change)?;
                }
                stream.finish()?;
            }
            (None, Some(item_id), None) => {
                Self::print_item_changes_as_table(db, item_id.into())?;
            }
            (None, None, Some(scan_id)) => match format {
                ReportFormat::Table => Self::print_scan_changes_as_table(db, scan_id.into())?,
                ReportFormat::Tree => Self::print_scan_changes_as_tree(db, scan_id.into())?,
                _ => return Err(FsPulseError::Error("Unsupported format.".to_string())),
            },
            _ => {}
        }
        Ok(())
    }

    pub fn print_scan(
        db: &Database,
        scan: &Scan,
        _format: ReportFormat,
    ) -> Result<(), FsPulseError> {
        let root = Root::get_by_id(db, scan.root_id())?
            .ok_or_else(|| FsPulseError::Error(format!("Root Id {} not found", scan.root_id())))?;
        let table_title = format!("Scan (Root Path: '{}')", root.path());

        let mut stream = Reports::begin_scans_table(&table_title, "No Scan");

        let change_counts = ChangeCounts::get_by_scan_id(db, scan.id())?;
        stream.row((*scan, change_counts))?;

        stream.finish()?;

        Ok(())
    }

    fn print_scans(db: &Database, last: u32) -> Result<(), FsPulseError> {
        let mut stream = Reports::begin_scans_table("Scans", "No Scans");

        Scan::for_each_scan(db, last, |db, scan| {
            let change_counts = ChangeCounts::get_by_scan_id(db, scan.id())?;
            stream.row((*scan, change_counts))?;
            Ok(())
        })?;

        stream.finish()?;

        Ok(())
    }

    fn begin_scans_table(title: &str, empty_row: &str) -> Stream<(Scan, ChangeCounts), Stdout> {
        Stream::new(
            io::stdout(),
            vec![
                Column::new(|f, (s, _): &(Scan, ChangeCounts)| write!(f, "{}", s.id()))
                    .header("ID")
                    .right()
                    .min_width(6),
                Column::new(|f, (s, _): &(Scan, ChangeCounts)| write!(f, "{}", s.root_id()))
                    .header("Root ID")
                    .right()
                    .min_width(6),
                Column::new(|f, (s, _): &(Scan, ChangeCounts)| write!(f, "{}", s.state()))
                    .header("State")
                    .center()
                    .min_width(10),
                Column::new(|f, (s, _): &(Scan, ChangeCounts)| write!(f, "{}", s.hashing()))
                    .header("Hashing")
                    .center(),
                Column::new(|f, (s, _): &(Scan, ChangeCounts)| write!(f, "{}", s.validating()))
                    .header("Validating")
                    .center(),
                Column::new(|f, (s, _): &(Scan, ChangeCounts)| {
                    write!(f, "{}", Utils::format_db_time_short(s.time_of_scan()))
                })
                .header("Time"),
                Column::new(|f, (s, _): &(Scan, ChangeCounts)| {
                    write!(f, "{}", Utils::display_opt_i64(&s.file_count()))
                })
                .header("Files")
                .right()
                .min_width(7),
                Column::new(|f, (s, _): &(Scan, ChangeCounts)| {
                    write!(f, "{}", Utils::display_opt_i64(&s.folder_count()))
                })
                .header("Folders")
                .right()
                .min_width(7),
                Column::new(|f, (_, c): &(Scan, ChangeCounts)| {
                    write!(f, "{}", c.count_of(ChangeType::Add))
                })
                .header("Adds")
                .right()
                .min_width(7),
                Column::new(|f, (_, c): &(Scan, ChangeCounts)| {
                    write!(f, "{}", c.count_of(ChangeType::Modify))
                })
                .header("Modifies")
                .right()
                .min_width(7),
                Column::new(|f, (_, c): &(Scan, ChangeCounts)| {
                    write!(f, "{}", c.count_of(ChangeType::Delete))
                })
                .header("Deletes")
                .right()
                .min_width(7),
            ],
        )
        .title(title)
        .empty_row(empty_row)
    }

    fn begin_roots_table() -> Stream<Root, Stdout> {
        Stream::new(
            io::stdout(),
            vec![
                Column::new(|f, root: &Root| write!(f, "{}", root.id()))
                    .header("ID")
                    .right()
                    .min_width(6),
                Column::new(|f, root: &Root| write!(f, "{}", root.path()))
                    .header("Path")
                    .left()
                    .min_width(109),
            ],
        )
        .title("Roots")
        .empty_row("No Roots")
    }

    fn begin_invalid_items_table(title: &str, empty_row: &str) -> Stream<Item, Stdout> {
        Stream::new(
            io::stdout(),
            vec![
                Column::new(|f, i: &Item| write!(f, "{}", i.id()))
                    .header("ID")
                    .right()
                    .min_width(6),
                Column::new(|f, i: &Item| write!(f, "{}", i.path()))
                    .header("Path")
                    .left(),
                Column::new(|f, i: &Item| {
                    write!(f, "{}", Utils::format_db_time_short_or_none(i.mod_date()))
                })
                .header("Modified")
                .left(),
                Column::new(|f, i: &Item| write!(f, "{}", Utils::display_opt_i64(&i.file_size())))
                    .header("Size")
                    .right(),
                Column::new(|f, i: &Item| {
                    write!(
                        f,
                        "{}",
                        Utils::display_opt_i64(&i.last_validation_scan_id())
                    )
                })
                .header("Last Valid Scan")
                .right(),
                Column::new(|f, i: &Item| {
                    write!(f, "{}", Utils::display_opt_str(&i.validation_error()))
                })
                .header("Validation Error")
                .left(),
            ],
        )
        .title(title)
        .empty_row(empty_row)
    }

    fn begin_items_table(title: &str, empty_row: &str) -> Stream<Item, Stdout> {
        Stream::new(
            io::stdout(),
            vec![
                Column::new(|f, i: &Item| write!(f, "{}", i.id()))
                    .header("ID")
                    .right()
                    .min_width(6),
                Column::new(|f, i: &Item| write!(f, "{}", i.root_id()))
                    .header("Root ID")
                    .right(),
                Column::new(|f, i: &Item| write!(f, "{}", i.path()))
                    .header("Path")
                    .left(),
                Column::new(|f, i: &Item| write!(f, "{}", i.is_tombstone()))
                    .header("Tombstone")
                    .center(),
                Column::new(|f, i: &Item| write!(f, "{}", i.item_type()))
                    .header("Type")
                    .center(),
                Column::new(|f, i: &Item| {
                    write!(f, "{}", Utils::format_db_time_short_or_none(i.mod_date()))
                })
                .header("Modified")
                .left(),
                Column::new(|f, i: &Item| write!(f, "{}", Utils::display_opt_i64(&i.file_size())))
                    .header("Size")
                    .right(),
                Column::new(|f, i: &Item| write!(f, "{}", Hash::short_md5(&i.file_hash())))
                    .center(),
                Column::new(|f, i: &Item| write!(f, "{}", i.validity_state()))
                    .header("Valid State")
                    .center(),
                Column::new(|f, i: &Item| write!(f, "{}", i.last_scan_id()))
                    .header("Last Scan")
                    .right(),
                Column::new(|f, i: &Item| {
                    write!(f, "{}", Utils::display_opt_i64(&i.last_hash_scan_id()))
                })
                .header("Last Hash Scan")
                .right(),
                Column::new(|f, i: &Item| {
                    write!(
                        f,
                        "{}",
                        Utils::display_opt_i64(&i.last_validation_scan_id())
                    )
                })
                .header("Last Valid Scan")
                .right(),
            ],
        )
        .title(title)
        .empty_row(empty_row)
    }

    fn begin_changes_table(title: &str, empty_row: &str) -> Stream<Change, Stdout> {
        Stream::new(
            io::stdout(),
            vec![
                Column::new(|f, c: &Change| write!(f, "{}", c.id))
                    .header("Id")
                    .right()
                    .min_width(6),
                Column::new(|f, c: &Change| write!(f, "{}", c.scan_id))
                    .header("Scan Id")
                    .right(),
                Column::new(|f, c: &Change| write!(f, "{}", c.item_id))
                    .header("Item Id")
                    .right(),
                Column::new(|f, c: &Change| write!(f, "{}", c.change_type))
                    .header("Change Type")
                    .center(),
                Column::new(|f, c: &Change| write!(f, "{}", c.item_type))
                    .header("Item Type")
                    .center(),
                Column::new(|f, c: &Change| write!(f, "{}", c.item_path))
                    .header("Item Path")
                    .left(),
                Column::new(|f, c: &Change| {
                    write!(f, "{}", Utils::display_opt_bool(&c.is_undelete))
                })
                .header("Undelete")
                .center(),
                Column::new(|f, c: &Change| {
                    write!(f, "{}", Utils::display_opt_bool(&c.metadata_changed))
                })
                .header("MD Changed")
                .center(),
                Column::new(|f, c: &Change| {
                    write!(f, "{}", Utils::format_db_time_short_or_none(c.mod_date_old))
                })
                .header("Mod Date (old)")
                .center(),
                Column::new(|f, c: &Change| {
                    write!(f, "{}", Utils::format_db_time_short_or_none(c.mod_date_new))
                })
                .header("Mod Date (new)")
                .center(),
                Column::new(|f, c: &Change| {
                    write!(f, "{}", Utils::display_opt_i64(&c.file_size_old))
                })
                .header("Size (old)")
                .right(),
                Column::new(|f, c: &Change| {
                    write!(f, "{}", Utils::display_opt_i64(&c.file_size_new))
                })
                .header("Size (new)")
                .right(),
                Column::new(|f, c: &Change| {
                    write!(f, "{}", Utils::display_opt_bool(&c.hash_changed))
                })
                .header("Hash Changed")
                .center(),
                Column::new(|f, c: &Change| write!(f, "{}", Hash::short_md5(&c.hash_old())))
                    .header("Prev Hash")
                    .center(),
                Column::new(|f, c: &Change| {
                    write!(f, "{}", Utils::display_opt_bool(&c.validity_changed))
                })
                .header("Val Changed")
                .center(),
                Column::new(|f, c: &Change| {
                    write!(f, "{}", Utils::display_opt_str(&c.validity_state_new()))
                })
                .header("Val State")
                .center(),
                Column::new(|f, c: &Change| {
                    write!(f, "{}", Utils::display_opt_str(&c.validity_state_old()))
                })
                .header("Prev Val State")
                .center(),
            ],
        )
        .title(title)
        .empty_row(empty_row)
    }

    fn get_tree_path(
        path_stack: &mut Vec<PathBuf>,
        root_path: &Path,
        path: &str,
        is_dir: bool,
    ) -> (usize, PathBuf) {
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
                    println!(
                        "{}{}/",
                        " ".repeat(path_stack.len() * 4),
                        structural_component_str
                    );
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
        let mut stream =
            Reports::begin_changes_table(&format!("Changes - Scan ID: {}", scan_id), "No Changes");

        Change::for_each_change_in_scan(db, scan_id, |change| {
            stream.row(change.clone())?;
            Ok(())
        })?;

        stream.finish()?;

        Ok(())
    }

    fn print_scan_changes_as_tree(db: &Database, scan_id: i64) -> Result<(), FsPulseError> {
        let width = 100;

        let scan = Scan::get_by_id(db, scan_id)?
            .ok_or_else(|| FsPulseError::Error(format!("Scan Id {} not found", scan_id)))?;
        let root = Root::get_by_id(db, scan.root_id())?
            .ok_or_else(|| FsPulseError::Error(format!("Root Id {} not found", scan.root_id())))?;

        Self::print_center(width, "Changes");
        Self::print_center(width, &format!("Root Path: '{}'", root.path()));

        Self::hr(width);

        let root_path = Path::new(root.path());
        let mut path_stack: Vec<PathBuf> = Vec::new(); // Stack storing directory paths
        let mut change_count = 0;

        // TODO: identify changes as metadata and/or hash
        Change::for_each_change_in_scan(db, scan.id(), |change| {
            let is_dir = change.item_type == "D";

            let (indent_level, new_path) =
                Self::get_tree_path(&mut path_stack, root_path, &change.item_path, is_dir);

            // Print the item
            println!(
                "{}[{}] {}{} ({})",
                " ".repeat(indent_level * 4),
                change.change_type,
                new_path.to_string_lossy(),
                Utils::dir_sep_or_empty(is_dir),
                change.id,
            );

            change_count += 1;
            Ok(())
        })?;

        if change_count == 0 {
            Self::print_center(width, "No Changes");
        }

        Self::hr(width);
        Ok(())
    }

    fn print_item_changes_as_table(db: &Database, item_id: i64) -> Result<(), FsPulseError> {
        let item = Item::get_by_id(db, item_id)?
            .ok_or_else(|| FsPulseError::Error(format!("Item Id {} not found", item_id)))?;

        let mut stream = Self::begin_changes_table(
            &format!(
                "Changes (Item Id: {}, Item Path: '{}'",
                item.id(),
                item.path()
            ),
            "No Changes",
        );

        Change::for_each_change_in_item(db, item_id, |change| {
            stream.row(change.clone())?;
            Ok(())
        })?;

        stream.finish()?;

        Ok(())
    }

    pub fn print_invalid_items_as_table(db: &Database, root: &Root) -> Result<(), FsPulseError> {
        let mut stream = Self::begin_invalid_items_table(
            &format!("Invalid Items (Root Path: '{}'", root.path()),
            "No Invalid Items",
        );

        Item::for_each_invalid_item_in_root(db, root.id(), |item| {
            stream.row(item.clone())?;
            Ok(())
        })?;

        stream.finish()?;
        Ok(())
    }

    fn print_last_seen_scan_items_as_table(
        db: &Database,
        scan: &Scan,
        root: &Root,
    ) -> Result<(), FsPulseError> {
        let mut stream =
            Self::begin_items_table(&format!("Items (Root Path: '{}'", root.path()), "No Items");

        Item::for_each_item_in_latest_scan(db, scan.id(), |item| {
            stream.row(item.clone())?;
            Ok(())
        })?;

        stream.finish()?;

        Ok(())
    }

    fn print_last_seen_scan_items_as_tree(
        db: &Database,
        scan: &Scan,
        root: &Root,
    ) -> Result<(), FsPulseError> {
        let title = format!(
            "Items (Root Id: {}, Root Path: '{}'",
            root.id(),
            root.path()
        );
        let width = max(100, title.len() + 20);

        Self::hr(width);
        Self::print_center(width, &title);
        Self::hr(width);

        let root_path = Path::new(root.path());
        let mut path_stack: Vec<PathBuf> = Vec::new();
        let mut item_count = 0;

        Item::for_each_item_in_latest_scan(db, scan.id(), |item| {
            let is_dir = item.item_type() == "D";

            let (indent_level, new_path) =
                Self::get_tree_path(&mut path_stack, root_path, item.path(), is_dir);

            // Print the item
            println!(
                "{}[{}] {}{}",
                " ".repeat(indent_level * 4),
                item.id(),
                new_path.to_string_lossy(),
                Utils::dir_sep_or_empty(is_dir),
            );
            item_count += 1;
            Ok(())
        })?;

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

    pub fn report_scan(db: &Database, scan: &Scan) -> Result<(), FsPulseError> {
        // Define your styles
        let header_style = Style::new().cyan().bold().underlined();
        let label_style = Style::new().white();
        let command_style = Style::new().green();

        let root = Root::get_by_id(db, scan.root_id())?
            .ok_or_else(|| FsPulseError::Error("Root Not Found".to_string()))?;

        let changes = ChangeCounts::get_by_scan_id(db, scan.id())?;

        // Build the report line by line
        let mut report = Vec::new();

        // Header Section
        report.push(
            header_style
                .apply_to("===============================================")
                .to_string(),
        );
        report.push(
            header_style
                .apply_to("           FS Pulse Scan Report")
                .to_string(),
        );
        report.push(
            header_style
                .apply_to("===============================================")
                .to_string(),
        );
        report.push(String::new());

        // Scan Info Section
        report.push(header_style.apply_to("Scan Info").to_string());
        report.push(format!(
            "{}         {}",
            label_style.apply_to("Scan Id:"),
            scan.id()
        ));
        report.push(format!(
            "{}          {}   (Root Id: {})",
            label_style.apply_to("Path:"),
            root.path(),
            root.id()
        ));
        report.push(format!(
            "{}         {}",
            label_style.apply_to("Files:"),
            Utils::display_opt_i64(&scan.file_count())
        ));
        report.push(format!(
            "{}       {}",
            label_style.apply_to("Folders:"),
            Utils::display_opt_i64(&scan.folder_count())
        ));
        report.push(String::new());

        // Change Summary Section
        report.push(header_style.apply_to("Change Summary").to_string());
        report.push(format!(
            "{}     {}",
            label_style.apply_to("Additions:"),
            changes.add_count
        ));
        report.push(format!(
            "{} {}",
            label_style.apply_to("Modifications:"),
            changes.modify_count
        ));
        report.push(format!(
            "{}     {}",
            label_style.apply_to("Deletions:"),
            changes.delete_count
        ));
        report.push(String::new());

        // Hash Mode Section
        report.push(header_style.apply_to("Hash Summary").to_string());
        if !scan.hashing() {
            report.push("Hash mode was not specified".to_string());
        }
        report.push(format!(
            "{}   {}    • {} for complete info.",
            label_style.apply_to("Changed Hashes:"),
            4,
            command_style.apply_to("Run 'fs hash-details'")
        ));
        report.push(String::new());

        // New Validation Transitions Section
        report.push(header_style.apply_to("Validation Transitions").to_string());
        if scan.validating() {
            let validation_changes = Change::get_validation_transitions_for_scan(db, scan.id())?;

            // From Unknown transitions
            report.push(label_style.apply_to("From Unknown:").to_string());
            report.push(format!(
                "    {}         {}    • {}",
                label_style.apply_to("-> Valid:"),
                validation_changes.unknown_to_valid,
                command_style.apply_to("Run 'fs list-new-valid'")
            ));
            report.push(format!(
                "    {}       {}    • {}",
                label_style.apply_to("-> Invalid:"),
                validation_changes.unknown_to_invalid,
                command_style.apply_to("Run 'fs list-new-invalid'")
            ));
            report.push(format!(
                "    {}  {}    • {}",
                label_style.apply_to("-> No Validator:"),
                validation_changes.unknown_to_no_validator,
                command_style.apply_to("Run 'fs list-new-no-validator'")
            ));
            report.push(String::new());

            // From Valid transitions
            report.push(label_style.apply_to("From Valid:").to_string());
            report.push(format!(
                "    {}       {}    • {}",
                label_style.apply_to("-> Invalid:"),
                validation_changes.valid_to_invalid,
                command_style.apply_to("Run 'fs list-valid-to-invalid'")
            ));
            report.push(format!(
                "    {}  {}    • {}",
                label_style.apply_to("-> No Validator:"),
                validation_changes.valid_to_no_validator,
                command_style.apply_to("Run 'fs list-valid-to-no-validator'")
            ));
            report.push(String::new());

            // From No Validator transitions
            report.push(label_style.apply_to("From No Validator:").to_string());
            report.push(format!(
                "    {}         {}    • {}",
                label_style.apply_to("-> Valid:"),
                validation_changes.no_validator_to_valid,
                command_style.apply_to("Run 'fs list-no-validator-to-valid'")
            ));
            report.push(format!(
                "    {}       {}    • {}",
                label_style.apply_to("-> Invalid:"),
                validation_changes.no_validator_to_invalid,
                command_style.apply_to("Run 'fs list-no-validator-to-invalid'")
            ));
            report.push(String::new());
        } else {
            report.push("Validation mode was not specified".to_string());
        };

        // Next Steps
        report.push(format!(
            "{} Use the commands above for further analysis.",
            label_style.apply_to("Next Steps:")
        ));

        // Join all lines and print the report
        println!("{}", report.join("\n"));
        Ok(())
    }
}
