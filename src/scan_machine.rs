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
// 5. Stopped

use crate::changes::ChangeType;
use crate::analysis::Analysis;
use crate::items::{Item, ItemType};
use crate::reports::{ReportFormat, Reports};
use crate::{database::Database, error::FsPulseError, scans::Scan};
use crate::roots::Root;
use crate::scans::ScanState;

use indicatif::{MultiProgress, ProgressBar, ProgressStyle};

use dialoguer::Select;
//use md5::digest::typenum::Abs;
use std::collections::VecDeque;
use std::fs;
use std::fs::Metadata;
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
    validate: bool,
    multi_prog: &mut MultiProgress,
) -> Result<(), FsPulseError> {
        // If an incomplete scan exists, find it.
        // TODO: Allow incomplete scans on different roots to exist. We won't, however,
        // allow the user to initiate a new scan on a root that has an outstanding scan until they
        // either resume/complete it or stop it
        
        let (root, mut existing_scan) = match (root_id, root_path, last) {
            (Some(root_id), _, _) => {
                let root = Root::get_by_id(db, root_id.into())?
                    .ok_or_else(|| FsPulseError::Error(format!("Root iI {} not found", root_id)))?;
                // Look for an outstanding scan on the root
                let scan = Scan::get_latest_for_root(db, root.id())?
                    .filter(|s| s.state() != ScanState::Completed && s.state() != ScanState::Stopped);
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
                            .filter(|s| s.state() != ScanState::Completed && s.state() != ScanState::Stopped);
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
                let root = Root::get_by_id(db, scan.root_id())?
                    .ok_or_else(|| FsPulseError::Error(format!("No root found for latest Scan Id {}", scan.id())))?;

                let return_scan = if scan.state() != ScanState::Completed {
                    Some(scan)
                } else {
                    None
                };

                (root, return_scan)
            },
            _ => {
                return Err(FsPulseError::Error("Invalid arguments".into()));
            }
        };

        // If scan is present, it is incomplete. Ask the user to decide if it should be resumed or stopped.
        // Also allows the user to exit without making the choice now

        let mut scan = match existing_scan.as_mut() {
            Some(existing_scan) => match stop_or_resume_scan(db, &root, existing_scan)? {
                ScanDecision::NewScan => initiate_scan(db, &root, hash, validate)?,
                ScanDecision::ContinueExisting => *existing_scan,
                ScanDecision::Exit => return Ok(())
            },
            None => initiate_scan(db, &root, hash, validate)?
        };


        //let mut multi_prog = MultiProgress::new();
        multi_prog.println(format!("Scanning: {}", root.path()))?;
        
        while scan.state() != ScanState::Completed {
            match scan.state() {
                ScanState::Scanning => do_state_scanning(db, &root, &mut scan, multi_prog),
                ScanState::Sweeping => do_state_sweeping(db, &mut scan, multi_prog),
                ScanState::Analyzing => do_state_analyzing(db, &root, &mut scan, multi_prog),
                _ => Err(FsPulseError::Error(format!("Unexpected incomplete scan state: {}", scan.state()))),
            }?;
        }

        Reports::print_scan(db, &Some(scan), ReportFormat::Table)?;

        Ok(())
}

enum ScanDecision {
    NewScan,
    ContinueExisting,
    Exit,
}

fn stop_or_resume_scan(db: &mut Database, root: &Root, scan: &mut Scan) -> Result<ScanDecision, FsPulseError> {
    let options = vec![
        "Resume the scan", 
        "Stop & exit",
        "Stop & start a new scan",
        "Exit (decide later)",
    ];

    let selection = Select::new()
        .with_prompt(format!("Found in-progress scan on:'{}'\n\nWhat would you like to do?", root.path()))
        .items(&options)
        .report(false)
        .interact()
        .unwrap();

    let decision = match selection {
        0 => {
            match scan.state() {
                ScanState::Scanning => ScanDecision::ContinueExisting,
                ScanState::Sweeping => ScanDecision::ContinueExisting,
                ScanState::Analyzing => ScanDecision::ContinueExisting,
                _ => return Err(FsPulseError::Error(format!("Unexpected incomplete scan state: {}", scan.state()))),
            }
        }, 
        1 => {
            scan.set_state_stopped(db)?;
            ScanDecision::Exit   
        },
        2 => {
            scan.set_state_stopped(db)?;
            ScanDecision::NewScan
        },
        _ => ScanDecision::Exit // exit
    };

    Ok(decision)
}

fn initiate_scan(db: &mut Database, root: &Root, hashing: bool, validating: bool) -> Result<Scan, FsPulseError> {
    Scan::create(db, root, hashing, validating)
}

fn do_state_scanning(db: &mut Database, root: &Root, scan: &mut Scan, multi_prog: &mut MultiProgress) -> Result<(), FsPulseError> {
    let root_path_buf = PathBuf::from(root.path());
    let metadata = fs::symlink_metadata(&root_path_buf)?;

    let mut q = VecDeque::new();

    let dir_prog = multi_prog.add(ProgressBar::new_spinner());
    dir_prog.enable_steady_tick(Duration::from_millis(100));
    let item_prog = multi_prog.add(ProgressBar::new_spinner());
    item_prog.enable_steady_tick(Duration::from_millis(100));

    q.push_back(QueueEntry {
        path: root_path_buf.clone(),
        metadata,
    });

    while let Some(q_entry) = q.pop_front() {
        dir_prog.set_message(format!("Directory: '{}'", q_entry.path.to_string_lossy()));

        // Handle the directory itself before iterating its contents. The root dir
        // was previously pushed into the queue - if this is that entry, we skip it
        if q_entry.path != root_path_buf {
            handle_scan_item(db, scan, ItemType::Directory, q_entry.path.as_path(), &q_entry.metadata)?;
        }

        let items = fs::read_dir(&q_entry.path)?;

        for item in items {
            let item = item?;

            let metadata = fs::symlink_metadata(item.path())?; // Use symlink_metadata to check for symlinks
            item_prog.set_message(format!("Item: '{}'", item.file_name().to_string_lossy()));

            if metadata.is_dir() {
                q.push_back(QueueEntry {
                    path: item.path(),
                    metadata,
                });
            } else {
                let item_type = if metadata.is_file() {
                    ItemType::File
                } else if metadata.is_symlink() {
                    ItemType::Symlink
                } else {
                    ItemType::Other
                };

                handle_scan_item(db, scan, item_type, &item.path(), &metadata)?;
            }
        }
    }

    dir_prog.finish_with_message("Scan complete");
    item_prog.finish_and_clear();

    scan.set_state_sweeping(db)
}

fn do_state_sweeping(db: &mut Database, scan: &mut Scan, multi_prog: &mut MultiProgress) -> Result<(), FsPulseError> { 
    let tx = db.conn.transaction()?;

    let sweep_prog = multi_prog.add(ProgressBar::new_spinner());
    sweep_prog.set_message("Detecting changes and deletions...");
    sweep_prog.enable_steady_tick(Duration::from_millis(100));

    // Insert deletion records into changes
        tx.execute(
            "INSERT INTO changes (scan_id, item_id, change_type)
             SELECT ?, id, ?
             FROM items
             WHERE root_id = ? AND is_tombstone = 0 AND last_scan_id < ?",
            (scan.id(), ChangeType::Delete.as_str(), scan.root_id(), scan.id()),
        )?;
    
       // Mark unseen items as tombstones
       tx.execute(
        "UPDATE items SET is_tombstone = 1 WHERE root_id = ? AND last_scan_id < ? AND is_tombstone = 0",
        (scan.root_id(), scan.root_id()),
    )?;

    tx.commit()?;

    sweep_prog.finish_with_message("Change and delete detection complete");
    scan.set_state_analyzing(db)
}

fn do_state_analyzing(db: &mut Database, _root: &Root, scan: &mut Scan, multi_prog: &mut MultiProgress) -> Result<(), FsPulseError> {
    if scan.hashing() || scan.validating() {
        let file_count = scan.file_count().unwrap_or_default().max(0) as u64;            // scan.file_count is the total # of files in the scan
        let analyzed_items= Item::count_analyzed_items(db, scan.id())?.max(0) as u64;  // may be resuming the scan
    
        let analysis_prog = multi_prog.add(ProgressBar::new(file_count));
        analysis_prog.set_style(ProgressStyle::default_bar()
                .template("{msg}\n[{bar:80}] {pos}/{len} (Remaining: {eta})")
                .unwrap()
                .progress_chars("#>-"));
        analysis_prog.set_message("Analyzing files:");
        analysis_prog.inc(analyzed_items);

        loop {
            let items = Item::fetch_next_analysis_batch(
                db, 
                scan.id(), 
                scan.hashing(), 
                scan.validating(), 
                10,
            )?;

            if items.len() == 0 {
                break;
            }

            for item in items {
                let path = Path::new(item.path());
                let file_name = path.file_name()
                    .unwrap_or_else(|| path.as_os_str())
                    .to_string_lossy();


                // TODO: Improve the error handling for all analysis. Need to differentiate
                // between file system errors and actual content errors
                let (hash, hash_prog) = match scan.hashing() {
                    true => {
                        let mut hash_prog = multi_prog.add(ProgressBar::new(0));

                        hash_prog.set_style(ProgressStyle::default_bar()
                            .template("{msg}\n[{bar:80}] {bytes}/{total_bytes} ({eta})")
                            .unwrap()
                            .progress_chars("#>-"));

                        hash_prog.set_message(format!("Computing hash: '{}'", file_name));

                        match Analysis::compute_md5_hash(&path, &file_name, &mut hash_prog) {
                            Ok(hash_s) => (Some(hash_s), Some(hash_prog)),
                            Err(error) => {
                                eprintln!("Error hashing '{}': {}", file_name, error);
                                (None, Some(hash_prog))
                            }
                        }
                    },
                    false => (None, None)
                };

                let (is_valid, is_valid_prog) = match scan.validating() {
                    true => {
                        if let Some(ref hash_prog) = hash_prog {
                            hash_prog.set_message("Hash complete");
                        }
                        let is_valid_prog = multi_prog.add(ProgressBar::new_spinner());
                        is_valid_prog.enable_steady_tick(Duration::from_millis(100));
                        /* 
                        match Analysis::validate(&path, &file_name, &is_valid_prog) {
                            Ok(is_valid_b) => (Some(is_valid_b), Some(is_valid_prog)),
                            Err(error) => {
                                eprintln!("Error validating '{}': {}", file_name, error);
                                (Some(false), Some(is_valid_prog))
                            }
                        }
                        */
                        if !path.ends_with(".flac") {
                            match Analysis::validate_flac(&path, &file_name, &is_valid_prog) {
                                Ok(is_valid) => (Some(is_valid), Some(is_valid_prog)),
                                Err(_) => (Some(false), Some(is_valid_prog))
                            }
                        } else {
                            (None, Some(is_valid_prog))
                        }

                    }
                    false => (None, None),
                };

                update_item_analysis(db, scan.id(), item.id(), scan.hashing(), scan.validating(), hash, is_valid)?;
                if let Some(hash_prog) = hash_prog {
                    hash_prog.finish_and_clear();
                }
                if let Some(is_valid_prog) = is_valid_prog {
                    is_valid_prog.finish_and_clear();
                }
                analysis_prog.inc(1);
            }
        }
        analysis_prog.finish_with_message("Analysis complete");
    }
   
    scan.set_state_completed(db)
}

fn handle_scan_item(
    db: &mut Database,
    scan: &Scan,
    item_type: ItemType, 
    path: &Path, 
    metadata: &Metadata,
) -> Result<(), FsPulseError> {
    //let conn = &mut db.conn;

    // load the item
    let path_str = path.to_string_lossy();
    let existing_item = Item::get_by_root_and_path(db, scan.root_id(), &path_str)?;

    let last_modified = metadata.modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64);

    let file_size = if metadata.is_file() { Some(metadata.len() as i64) } else { None };


    // If the item was already processed for this scan, just skip it. We intentionally
    // do not handle the case where the item was seen within this scan, but has since
    // either been modified or has changed type. There are edge cases where this might
    // cause strangeness in reports such as when an item was seen as a file, the scan
    // was resumed and the item has changed into a directory. In this case, we'll still
    // traverse the children within the resumed scan and a tree report will look odd
    if let Some(existing_item) = existing_item {
        if existing_item.last_scan_id() == scan.id() {
            return Ok(())
        }

        let item_type_str = item_type.as_str();
        let metadata_changed = existing_item.last_modified() != last_modified || existing_item.file_size() != file_size;
        
        if existing_item.is_tombstone() {
            // Rehydrate a tombstone
            let tx = db.conn.transaction()?;
            let rows_updated = tx.execute(
                "UPDATE items SET 
                        is_tombstone = 0, 
                        item_type = ?, 
                        last_modified = ?, 
                        file_size = ?, 
                        file_hash = NULL, 
                        file_is_valid = NULL, 
                        last_scan_id = ?,
                        last_hash_scan_id = NULL, 
                        last_is_valid_scan_id = NULL 
                    WHERE id = ?", 
                (item_type_str, last_modified, file_size, scan.id(), existing_item.id()))?;
            if rows_updated == 0 {
                    return Err(FsPulseError::Error(format!("Item Id {} not found for update", existing_item.id())));
            }
            tx.execute("INSERT INTO changes (scan_id, item_id, change_type) VALUES (?, ?, ?)", 
                (scan.id(), existing_item.id(), ChangeType::Add.as_str()))?;
            tx.commit()?;
        } else if existing_item.item_type() != item_type_str {
            //Item type changed file <-> folder
            let tx = db.conn.transaction()?;
            let rows_updated = tx.execute(
                "UPDATE items SET 
                    item_type = ?, 
                    last_modified = ?, 
                    file_size = ?,
                    file_hash = NULL,
                    file_is_valid = NULL,
                    last_scan_id = ?,
                    last_hash_scan_id = NULL,
                    last_is_valid_scan_id = NULL 
                WHERE id = ?", 
                (item_type_str, last_modified, file_size, scan.id(), existing_item.id()))?;
            if rows_updated == 0 {
                    return Err(FsPulseError::Error(format!("Item Id {} not found for update", existing_item.id())));
            }
 
            tx.execute("INSERT INTO changes (scan_id, item_id, change_type) VALUES (?, ?, ?)", 
                (scan.id(), existing_item.id(), ChangeType::TypeChange.as_str()))?;
            tx.commit()?;
        } else if metadata_changed {
            let tx = db.conn.transaction()?;

           let rows_updated = tx.execute(
            "UPDATE items SET
                last_modified = ?, 
                file_size = ?,             
                last_scan_id = ? 
               WHERE id = ?", 
               (last_modified, file_size, scan.id(), existing_item.id()))?;
            if rows_updated == 0 {
                return Err(FsPulseError::Error(format!("Item Id {} not found for update", existing_item.id())));
            }
            tx.execute(
                "INSERT INTO changes 
                    (scan_id, item_id, change_type, prev_last_modified, prev_file_size) 
                    VALUES (?, ?, ?, ?, ?)", 
               (
                   scan.id(), 
                   existing_item.id(), 
                   ChangeType::Modify.as_str(),
                   metadata_changed.then_some(existing_item.last_modified()),
                   metadata_changed.then_some(existing_item.file_size())
               ))?;
           tx.commit()?;            
        } else {
            // No change - just update last_scan_id
            let rows_updated = db.conn.execute(
                "UPDATE items SET last_scan_id = ? WHERE id = ?", 
                (scan.id(), existing_item.id()))?;
            if rows_updated == 0 {
                    return Err(FsPulseError::Error(format!("Item Id {} not found for update", existing_item.id())));
            }
        }
    } else {
        // Item is new, insert into items and changes tables
        let tx = db.conn.transaction()?;
        tx.execute("INSERT INTO items (root_id, path, item_type, last_modified, file_size, last_scan_id) VALUES (?, ?, ?, ?, ?, ?)",
            (scan.root_id(), &path_str, item_type.as_str(), last_modified, file_size, scan.id()))?;
        let item_id: i64 = tx.query_row("SELECT last_insert_rowid()", [], |row| row.get(0))?;
        tx.execute("INSERT INTO changes (scan_id, item_id, change_type) VALUES (?, ?, ?)",
            (scan.id(), item_id, ChangeType::Add.as_str()))?;
        tx.commit()?;
    }
    
    Ok(())
}

pub fn update_item_analysis(
    db: &mut Database,
    scan_id: i64,
    item_id: i64,
    hashing: bool,
    validating: bool,
    new_hash: Option<String>,
    new_is_valid: Option<bool>,
) -> Result<(), FsPulseError> {
    let tx = db.conn.transaction()?; // Start transaction

    // Step 1: UPSERT into `changes` table
    tx.execute(
        "INSERT INTO changes (scan_id, item_id, change_type, prev_hash, prev_is_valid)
         SELECT ?, ?, 'M', 
                CASE WHEN ? = 1 THEN file_hash ELSE NULL END, 
                CASE WHEN ? = 1 THEN file_is_valid ELSE NULL END
         FROM items WHERE id = ?
         ON CONFLICT(scan_id, item_id, change_type) 
         DO UPDATE SET 
            prev_hash = CASE WHEN ? = 1 THEN (SELECT file_hash FROM items WHERE id = ?) ELSE NULL END,
            prev_is_valid = CASE WHEN ? = 1 THEN (SELECT file_is_valid FROM items WHERE id = ?) ELSE NULL END",
        rusqlite::params![
            scan_id, item_id, 
            hashing as i64, validating as i64, item_id, 
            hashing as i64, item_id, 
            validating as i64, item_id
        ],
    )?;

    // Step 2: Update `items` table
    tx.execute(
        "UPDATE items 
         SET file_hash = CASE WHEN ? = 1 THEN ? ELSE file_hash END,
             last_hash_scan_id = CASE WHEN ? = 1 THEN ? ELSE last_hash_scan_id END,
             file_is_valid = CASE WHEN ? = 1 THEN ? ELSE file_is_valid END,
             last_is_valid_scan_id = CASE WHEN ? = 1 THEN ? ELSE last_is_valid_scan_id END
         WHERE id = ?",
        rusqlite::params![
            hashing as i64, new_hash, hashing as i64, scan_id,
            validating as i64, new_is_valid, validating as i64, scan_id,
            item_id
        ],
    )?;

    tx.commit()?; // Commit transaction

    Ok(())
}
