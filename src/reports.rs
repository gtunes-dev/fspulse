use crate::changes::{Change, ChangeCounts};
use crate::database::Database;
use crate::error::FsPulseError;
use crate::items::Item;
use crate::query::QueryProcessor;
use crate::roots::Root;
use crate::scans::Scan;
use crate::utils::Utils;

use console::Style;
use rusqlite::Result;
use std::cmp::max;
use std::path::{Path, PathBuf};
use std::str::FromStr;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReportFormat {
    Tree,
    Table,
}

impl FromStr for ReportFormat {
    type Err = FsPulseError;
    fn from_str(s: &str) -> Result<Self, FsPulseError> {
        match s.to_lowercase().as_str() {
            "tree" => Ok(ReportFormat::Tree),
            "table" => Ok(ReportFormat::Table),
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
    ) -> Result<(), FsPulseError> {
        let query = match scan_id {
            Some(scan_id) => &format!("scans where scan_id:({scan_id})"),
            None => &format!("scans order by scan_id desc limit {last}"),
        };

        //println!(">> Generated query: {}", query);
        QueryProcessor::execute_query_and_print(db, query)
    }

    pub fn report_roots(
        db: &Database,
        root_id: Option<u32>,
        root_path: Option<String>,
    ) -> Result<(), FsPulseError> {
        let query = match (root_id, root_path) {
            (None, None) => "roots order by root_id asc",
            (Some(root_id), _) => &format!("roots where root_id:({root_id})"),
            (_, Some(root_path)) => &format!("roots where root_path:('{root_path}')"),
        };

        //println!("Query: {query}");
        QueryProcessor::execute_query_and_print(db, query)?;

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
        if format == ReportFormat::Table {
            let query = match (item_id, item_path, root_id) {
                (Some(item_id), _, _,) =>  &format!("items where item_id:({item_id})"),
                (_, Some(item_path), _) => &format!("items where item_path:({item_path}) order by item_path asc"),
                (_, _, Some(root_id)) => {
                    match invalid {
                        false => &format!("items where root_id:({root_id}), is_ts:(F) order by item_path asc"),
                        true => &format!("items where root_id:({root_id}), val:(I), is_ts:(F) show default, val, val_error order by item_path asc")
                    }
                }
                _ => return Err(FsPulseError::Error("Item reports require additional parameters".into()))
            };

            QueryProcessor::execute_query_and_print(db, query)?;
        } else if let Some(root_id) = root_id {
            // TODO: Does this even make sense???
            let root = Root::get_by_id(db, root_id.into())?
                .ok_or_else(|| FsPulseError::Error(format!("Root Id {root_id} not found")))?;

            let scan = Scan::get_latest_for_root(db, root.root_id())?.ok_or_else(|| {
                FsPulseError::Error(format!("No latest scan found for Root Id {root_id}"))
            })?;

            Self::print_last_seen_scan_items_as_tree(db, &scan, &root)?;
        }

        Ok(())
    }

    pub fn report_changes(
        db: &Database,
        change_id: Option<u32>,
        item_id: Option<u32>,
        scan_id: Option<u32>,
    ) -> Result<(), FsPulseError> {
        let query = match (change_id, item_id, scan_id) {
            (Some(change_id), None, None) => format!("changes where change_id:({change_id})"),
            (None, Some(item_id), None) => format!(
                "changes where item_id:({item_id}) show default, item_path order by change_id desc"
            ),
            (None, None, Some(scan_id)) => {
                format!("changes where scan_id:({scan_id}) order_by change_id asc")
            }
            _ => return Err(FsPulseError::Error("Change reports require additional parameters".into()))
        };

        QueryProcessor::execute_query_and_print(db, &query)?;

        Ok(())
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

    /* *
    fn print_scan_changes_as_tree(db: &Database, scan_id: i64) -> Result<(), FsPulseError> {
        let width = 100;

        let scan = Scan::get_by_id(db, scan_id)?
            .ok_or_else(|| FsPulseError::Error(format!("Scan Id {} not found", scan_id)))?;
        let root = Root::get_by_id(db, scan.root_id())?
            .ok_or_else(|| FsPulseError::Error(format!("Root Id {} not found", scan.root_id())))?;

        Self::print_center(width, "Changes");
        Self::print_center(width, &format!("Root Path: '{}'", root.root_path()));

        Self::hr(width);

        let root_path = Path::new(root.root_path());
        let mut path_stack: Vec<PathBuf> = Vec::new(); // Stack storing directory paths
        let mut change_count = 0;

        // TODO: identify changes as metadata and/or hash
        Change::for_each_change_in_scan(db, scan.scan_id(), |change| {
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
                change.change_id,
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
    */

    /* *

    fn print_item_changes_as_table(db: &Database, item_id: i64) -> Result<(), FsPulseError> {
        let item = Item::get_by_id(db, item_id)?
            .ok_or_else(|| FsPulseError::Error(format!("Item Id {} not found", item_id)))?;

        let mut stream = Self::begin_changes_table(
            &format!(
                "Changes (Item Id: {}, Item Path: '{}'",
                item.item_id(),
                item.item_path()
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
    */

    fn print_last_seen_scan_items_as_tree(
        db: &Database,
        scan: &Scan,
        root: &Root,
    ) -> Result<(), FsPulseError> {
        let title = format!(
            "Items (Root Id: {}, Root Path: '{}'",
            root.root_id(),
            root.root_path()
        );
        let width = max(100, title.len() + 20);

        Self::hr(width);
        Self::print_center(width, &title);
        Self::hr(width);

        let root_path = Path::new(root.root_path());
        let mut path_stack: Vec<PathBuf> = Vec::new();
        let mut item_count = 0;

        Item::for_each_item_in_latest_scan(db, scan.scan_id(), |item| {
            let is_dir = item.item_type() == "D";

            let (indent_level, new_path) =
                Self::get_tree_path(&mut path_stack, root_path, item.item_path(), is_dir);

            // Print the item
            println!(
                "{}[{}] {}{}",
                " ".repeat(indent_level * 4),
                item.item_id(),
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
        let title = Style::new().cyan().bold();
        let header = Style::new().cyan().bold().underlined();
        let label = Style::new().white().bold();
        let command = Style::new().green();

        let root = Root::get_by_id(db, scan.root_id())?
            .ok_or_else(|| FsPulseError::Error("Root Not Found".to_string()))?;

        let changes = ChangeCounts::get_by_scan_id(db, scan.scan_id())?;

        // Build the report line by line
        let mut report = Vec::new();

        // Header Section

        let title_width = 60;

        let bar = title.apply_to("=".repeat(title_width)).to_string();
        let title_text = "FS Pulse Scan Report";
        let inset = title_width - ((title_width / 2) + (title_text.len() / 2));
        let title = format!("{}{}", " ".repeat(inset), title.apply_to(title_text));

        report.push(bar.to_owned());
        report.push(title);
        report.push(bar);

        report.push(String::new());

        // Scan Info Section
        report.push(header.apply_to("Scan Info").to_string());
        report.push(String::new());
        report.push(format!(
            "{}       {}",
            label.apply_to("Scan Id:"),
            scan.scan_id()
        ));
        report.push(format!(
            "{}          {} [root_id: {}]",
            label.apply_to("Path:"),
            root.root_path(),
            root.root_id()
        ));
        report.push(format!(
            "{}         {}",
            label.apply_to("Files:"),
            Utils::display_opt_i64(&scan.file_count())
        ));
        report.push(format!(
            "{}       {}",
            label.apply_to("Folders:"),
            Utils::display_opt_i64(&scan.folder_count())
        ));

        report.push(String::new());

        report.push("Tip: use query to display all scan properties".to_string());
        report.push(format!(
            "> {}",
            command.apply_to(format!(
                "fspulse query 'scans where scan_id:({}) show all'",
                scan.scan_id()
            ))
        ));

        report.push(String::new());

        // Change Summary Section
        report.push(header.apply_to("Change Summary").to_string());
        report.push(String::new());

        report.push(format!(
            "{}     {}",
            label.apply_to("Additions:"),
            changes.add_count
        ));
        report.push(format!(
            "{} {}",
            label.apply_to("Modifications:"),
            changes.modify_count
        ));
        report.push(format!(
            "{}     {}",
            label.apply_to("Deletions:"),
            changes.delete_count
        ));
        report.push(String::new());

        report.push("Tip: use queries to explore changes".to_string());
        report.push(format!(
            "> {}",
            command.apply_to(format!("fspulse query 'changes where scan_id:({}) show default, item_path order by item_path'", scan.scan_id()))
        ));
        report.push(format!(
            "> {}",
            command.apply_to(format!("fspulse query 'changes where scan_id:({}), change_type:(A) show default, item_path order by item_path'", scan.scan_id()))
        ));
        report.push(format!(
            "> {}",
            command.apply_to(format!("fspulse query 'changes where scan_id:({}), change_type:(M) show default, item_path order by item_path'", scan.scan_id()))
        ));
        report.push(format!(
            "> {}",
            command.apply_to(format!("fspulse query 'changes where scan_id:({}), change_type:(D) show default, item_path order by item_path'", scan.scan_id()))
        ));

        report.push(String::new());

        // Hash Mode Section
        report.push(header.apply_to("Hash Changes").to_string());
        report.push(String::new());

        if !scan.analysis_spec().is_hash() {
            report.push("Hash mode was not specified".to_string());
        } else {
            report.push("Tip: query for all hash changes".to_string());
            report.push(format!(
                "> {}",
                command.apply_to(format!("fspulse query 'changes where scan_id:({}), hash_change:(T) show default, hash_change, item_path'", scan.scan_id()))
            ));

            report.push(
                "Tip: query for all hash changes without mod date or file size changes".to_string(),
            );
            report.push(format!(
                "> {}",
                command.apply_to(format!("fspulse query 'changes where scan_id:({}), hash_change:(T), meta_change:(F)  show default, hash_change, item_path'", scan.scan_id()))
            ));
        }
        report.push(String::new());

        // New Validation Transitions Section
        report.push(header.apply_to("Validation Changes").to_string());
        report.push(String::new());

        if !scan.analysis_spec().is_val() {
            report.push("Validation mode was not specified".to_string());
        } else {
            let validation_changes =
                Change::get_validation_transitions_for_scan(db, scan.scan_id())?;

            // From Unknown transitions
            report.push(label.apply_to("From Unknown:").to_string());
            report.push(format!(
                "    {}         {}",
                label.apply_to(format!("{:<17}", "-> Valid:")),
                validation_changes.unknown_to_valid
            ));
            report.push(format!(
                "    {}         {}",
                label.apply_to(format!("{:<17}", "-> Invalid:")),
                validation_changes.unknown_to_invalid,
            ));
            report.push(format!(
                "    {}         {}",
                label.apply_to(format!("{:<17}", "-> No Validator:")),
                validation_changes.unknown_to_no_validator,
            ));
            report.push(String::new());

            // From Valid transitions
            report.push(label.apply_to("From Valid:").to_string());
            report.push(format!(
                "    {}         {}",
                label.apply_to(format!("{:<17}", "-> Invalid:")),
                validation_changes.valid_to_invalid,
            ));
            report.push(format!(
                "    {}         {}",
                label.apply_to(format!("{:<17}", "-> No Validator:")),
                validation_changes.valid_to_no_validator,
            ));
            report.push(String::new());

            // From No Validator transitions
            report.push(label.apply_to("From No Validator:").to_string());
            report.push(format!(
                "    {}         {}",
                label.apply_to(format!("{:<17}", "-> Valid:")),
                validation_changes.no_validator_to_valid,
            ));
            report.push(format!(
                "    {}         {}",
                label.apply_to(format!("{:<17}", "-> Invalid:")),
                validation_changes.no_validator_to_invalid,
            ));
            report.push(String::new());
            report.push("Tip: query for all newly identified invalid items".to_string());
            report.push(format!(
                "> {}",
                command.apply_to(format!("fspulse query 'changes where scan_id:({}), val_change:(T), val_new:(I) show default, item_path, val_new, val_error_new order by item_path'", scan.scan_id()))
            ));
        }

        report.push(String::new());
        // Next Steps
        report.push(format!(
            "{} Use the commands above for further analysis.",
            label.apply_to("Next Steps:")
        ));

        report.push(String::new());

        // Join all lines and print the report
        println!("{}", report.join("\n"));
        Ok(())
    }
}
