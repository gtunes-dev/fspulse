// Scan States
// 1. Initial scan.
//      - New items: create Item with metadata,last_scan; create change (Add)
//      - For each found item:
//          - If tombstone: Update item type, metadata, is_tombstone, last_scan; null hash, valid; create change (Add)
//          - If folder <-> file change: update Item metadata, last_scan; null hash, valid; create change (Type Changed)
//          - If metadata change: update Item metadata, last_scan; create change (Modify)
//  (Set State to 2)
// 2. Tombstone
//      - For each previously seen, non-tombstone item:
//          - Set is_tombstone; create change (Delete)
//  (If --hash or --validate, set state to 3 else set state to [TBD])
// 3. Hash and/or Validate
//      - For each non-tombstone, file item with last_scan < current scan:
//          - Hash and/or Validate per scan configuration
//          - If Hash and/or Valid are non-null and have changed, create change record with old value(s) of the changed value(s)
// 4. Completed
// 5. Aborted

use crate::items::ItemType;
use crate::{database::Database, error::FsPulseError, scans::Scan};
use crate::roots::Root;
use crate::scans::ScanState;

use indicatif::{MultiProgress, ProgressBar, ProgressStyle};

use dialoguer::Select;
use log::Metadata;
use std::collections::VecDeque;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

#[derive(Clone, Debug)]
struct QueueEntry {
    path: PathBuf,
    metadata: fs::Metadata,
}


pub fn do_scan_machine(
    db: &mut Database, 
    root_id: Option<u32>, 
    root_path: Option<String>,
    last: bool, 
    hash: bool,
    validate: bool) -> Result<(), FsPulseError> {
        // If an incomplete scan exists, find it.
        // TODO: Allow incomplete scans on different roots to exist. We won't, however,
        // allow the user to initiate a new scan on a root that has an outstanding scan until they
        // either resume/complete it or abort it
        
        let (root, mut scan) = match (root_id, root_path, last) {
            (Some(root_id), _, _) => {
                let root = Root::get_by_id(db, root_id.into())?
                    .ok_or_else(|| FsPulseError::Error(format!("Root iI {} not found", root_id)))?;
                // Look for an outstanding scan on the root
                let scan = Scan::get_latest_for_root(db, root.id())?
                    .filter(|s| s.state() != ScanState::Completed && s.state() != ScanState::Aborted);
                (root, scan)
            },
            (_, Some(root_path), _) => {
                let root_path_buf = Root::validate_and_canonicalize_path(&root_path)?;
                let root_path_str = root_path_buf.to_string_lossy().to_string();

                let root = Root::get_by_path(db, &root_path_str)?;
                match root {
                    Some(root) => {
                        // Found the root. Look for an outstanding scan
                        let scan = Scan::get_latest_for_root(db, root.id())?
                            .filter(|s| s.state() != ScanState::Completed && s.state() != ScanState::Aborted);
                        (root, scan)
                    },
                    None => {
                        // Create the new root
                        let new_root = Root::create(db, &root_path_str)?;
                        (new_root, None)
                    }
                }
            },
            (_, _, true) => {
                let scan = Scan::get_latest(db)?
                    .ok_or_else(|| FsPulseError::Error(format!("No latest scan found")))?;
                let root = Root::get_by_id(db, scan.id())?
                    .ok_or_else(|| FsPulseError::Error(format!("No root found for latest Scan Id {}", scan.id())))?;

                (root, Some(scan))
            },
            _ => {
                return Err(FsPulseError::Error("Invalid arguments".into()));
            }
        };

        // If scan is present, it is incomplete. Ask the user to decide if it should be resumed or aborted.
        // Also allows the user to exit without making the choice now
        match scan.as_mut() {
            Some(scan) => abort_or_resume_scan(db, &root, scan),
            None => initiate_scan(db, &root, hash, validate),
        }
}

fn abort_or_resume_scan(db: &mut Database, root: &Root, scan: &mut Scan) -> Result<(), FsPulseError> {
    let options = vec!["resume scan", "abort scan", "exit"];

    let selection = Select::new()
        .with_prompt(format!("Scan Id {} did not complete.\nYou can choose to resume, abort, or exit", scan.id()))
        .items(&options)
        .interact()
        .unwrap();

    match (selection) {
        0 => {
            match scan.state() {
                ScanState::Scanning => do_state_scanning(db, root, scan),
                ScanState::Sweeping => do_state_sweeping(db, root, scan),
                ScanState::Analyzing => do_state_analyzing(db, root, scan),
                _ => Err(FsPulseError::Error(format!("Unexpected incomplete scan state: {}", scan.state()))),
            }
        }, 
        1 => {
            scan.abort(db)   // abort and exit
        },
        _ => Ok(()), // exit
    }
}

fn initiate_scan(db: &Database, root: &Root, hashing: bool, validating: bool) -> Result<(), FsPulseError> {
    let scan = Scan::create(db, root, hashing, validating)?;
    do_state_scanning(db, root, &scan)
}

fn do_state_scanning(db: &Database, root: &Root, scan: &Scan) -> Result<(), FsPulseError> {
    let root_path_buf = PathBuf::from(root.path());
    let metadata = fs::symlink_metadata(&root_path_buf)?;

    let mut q = VecDeque::new();

    let multi = MultiProgress::new();
    multi.println(format!("Scanning: {}", root.path()))?;
    let dir_bar = multi.add(ProgressBar::new_spinner());
    dir_bar.enable_steady_tick(Duration::from_millis(100));
    let item_bar = multi.add(ProgressBar::new_spinner());
    item_bar.enable_steady_tick(Duration::from_millis(100));

    let mut progress_bar = if scan.hashing() || scan.validating() {
        let bar = ProgressBar::new(0); // Initialize with 0 length
    
        // TODO: this error will panic
        bar.set_style(ProgressStyle::default_bar()
            .template("{msg}\n[{bar:40}] {bytes}/{total_bytes} ({eta})")
            .unwrap()
            .progress_chars("#>-"));
        Some(bar)
    } else {
        None
    };

    q.push_back(QueueEntry {
        path: root_path_buf.clone(),
        metadata,
    });

    while let Some(q_entry) = q.pop_front() {
        dir_bar.set_message(format!("Directory: '{}'", q_entry.path.to_string_lossy()));

        // The root was previously pushed onto the queue to enable it to be scanned but
        // we don't want to insert it into the database as an item, so we skip this

    }




    do_state_sweeping(db, root, scan)
}

fn do_state_sweeping(db: &Database, root: &Root, scan: &Scan) -> Result<(), FsPulseError> { 
    
    do_state_analyzing(db, root, scan)
}

fn do_state_analyzing(db: &Database, root: &Root, scan: &Scan) -> Result<(), FsPulseError> {

    Ok(())
}

fn handle_scan_item(
    db: &Database, 
    item_type: ItemType, 
    path: &Path, 
    metadata: &Metadata,
    file_hash: Option<&str>,
    file_is_valid: Option<bool>,
) -> Result<(), FsPulseError> {
    
}

