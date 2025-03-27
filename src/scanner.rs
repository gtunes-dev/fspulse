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
use crate::analysis::{Analysis, ValidationState};
use crate::items::{Item, ItemType};
use crate::reports::{ReportFormat, Reports};
use crate::utils::Utils;
use crate::{database::Database, error::FsPulseError, scans::Scan};
use crate::roots::Root;
use crate::scans::ScanState;

use crossbeam_channel::bounded;
use dialoguer::theme::ColorfulTheme;
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};

use log::error;
use dialoguer::{MultiSelect, Select};
use threadpool::ThreadPool;
//use md5::digest::typenum::Abs;
use std::collections::VecDeque;
use std::fs;
use std::fs::Metadata;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[derive(Clone, Debug)]
struct QueueEntry {
    path: PathBuf,
    metadata: fs::Metadata,
}


pub struct Scanner {
}

enum ScanDecision {
    NewScan,
    ContinueExisting,
    Exit,
}

impl Scanner {
    pub fn do_interactive_scan(db: &mut Database, multi_prog: &mut MultiProgress) -> Result<(), FsPulseError>{

        let root = match Root::interact_choose_root(db, "Scan which root?")? {
            Some(root) => root,
            None => return Ok(())
        };
        
        // look for an existing, incomplete scan
        let mut existing_scan = Scan::get_latest_for_root(db, root.id())?
                .filter(|s| s.state() != ScanState::Completed && s.state() != ScanState::Stopped);

        // if a scan is found, ask the user if it should be stopped or resumed
        let mut scan = match existing_scan.as_mut() {
            Some(existing_scan) => match Scanner::stop_or_resume_scan(db, &root, existing_scan, true)? {
                ScanDecision::NewScan => Scanner::initiate_scan_interactive(db, &root)?,
                ScanDecision::ContinueExisting => *existing_scan,
                ScanDecision::Exit => return Ok(())
            },
            None => Scanner::initiate_scan_interactive(db, &root)?
        };

        Scanner::do_scan_machine(db, &mut scan, &root, multi_prog)
    }

    fn initiate_scan_interactive(db: &mut Database, root: &Root) -> Result<Scan, FsPulseError> {
        let flags = vec!["hash", "validate"];
        let selection = MultiSelect::new()
            .with_prompt("Hash or Validate (space to select, enter to continue)")
            .items(&flags)
            .interact()
            .unwrap();

        let mut hash = false;
        let mut validate = false;

        for selected_flag in selection.iter() {
            match selected_flag {
                0 => hash = true,
                1 => validate = true,
                _ => ()
            }
        }

        Scanner::initiate_scan(db, root, hash, validate)
    }

    pub fn do_scan_command(
        db: &mut Database, 
        root_id: Option<u32>, 
        root_path: Option<String>,
        last: bool, 
        hash: bool,
        validate: bool,
        multi_prog: &mut MultiProgress,
    ) -> Result<(), FsPulseError> {
            // If an incomplete scan exists, find it.
            let (root, mut existing_scan) = match (root_id, root_path, last) {
                (Some(root_id), _, _) => {
                    let root = Root::get_by_id(db, root_id.into())?
                        .ok_or_else(|| FsPulseError::Error(format!("Root id {} not found", root_id)))?;
                    // Look for an outstanding scan on the root
                    let scan = Scan::get_latest_for_root(db, root.id())?
                        .filter(|s: &Scan| s.state() != ScanState::Completed && s.state() != ScanState::Stopped);
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
                Some(existing_scan) => match Scanner::stop_or_resume_scan(db, &root, existing_scan, false)? {
                    ScanDecision::NewScan => Scanner::initiate_scan(db, &root, hash, validate)?,
                    ScanDecision::ContinueExisting => {
                        multi_prog.println("Resuming scan")?;
                        *existing_scan
                    },
                    ScanDecision::Exit => return Ok(())
                },
                None => Scanner::initiate_scan(db, &root, hash, validate)?
            };

            Scanner::do_scan_machine(db, &mut scan, &root, multi_prog)
    }

    fn do_scan_machine(db: &mut Database, scan: &mut Scan, root: &Root, multi_prog: &mut MultiProgress) -> Result<(), FsPulseError> {
        multi_prog.println(format!("Scanning: {}", root.path()))?;


        loop {
            let current_state = scan.state();

            // When the state is completed, the scan is done
            if current_state == ScanState::Completed {
                break;
            }

            match current_state {
                ScanState::Scanning => Scanner::do_state_scanning(db, &root, scan, multi_prog),
                ScanState::Sweeping => Scanner::do_state_sweeping(db, scan, multi_prog),
                ScanState::Analyzing => {
                    // This is a boundary - we'll take ownership of the database and progress
                    // bars here and then restore them when we're done
                    let owned_db = std::mem::take(db);
                    let db_arc = Arc::new(Mutex::new(owned_db));
                    let multi_prog_arc = Arc::new(multi_prog.clone());
                    
                    let analysis_result = Scanner::do_state_analyzing(db_arc.clone(), scan, multi_prog_arc);

                    // recover the database from the Arc
                    let recovered_db = Arc::try_unwrap(db_arc)
                        .expect("No additional clones should exist")
                        .into_inner()
                        .expect("Mutex isn't poisoned");

                    *db = recovered_db;
                    analysis_result
                },
                _ => Err(FsPulseError::Error(format!("Unexpected incomplete scan state: {}", current_state))),
            }?;
        }   

        Reports::print_scan(db, scan, ReportFormat::Table)?;

        Ok(())
    }



    fn stop_or_resume_scan(db: &mut Database, root: &Root, scan: &mut Scan, report: bool) -> Result<ScanDecision, FsPulseError> {
        let options = vec![
            "Resume the scan", 
            "Stop & exit",
            "Stop & start a new scan",
            "Exit (decide later)",
        ];

        let selection = Select::with_theme(&ColorfulTheme::default())
            .with_prompt(format!("Found in-progress scan on:'{}'\n\nWhat would you like to do?", root.path()))
            .default(0)
            .items(&options)
            .report(report)
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
        dir_prog.enable_steady_tick(Duration::from_millis(250));
        let item_prog = multi_prog.add(ProgressBar::new_spinner());
        item_prog.enable_steady_tick(Duration::from_millis(250));

        q.push_back(QueueEntry {
            path: root_path_buf.clone(),
            metadata,
        });

        while let Some(q_entry) = q.pop_front() {
            dir_prog.set_message(format!("Directory: '{}'", q_entry.path.to_string_lossy()));

            // Handle the directory itself before iterating its contents. The root dir
            // was previously pushed into the queue - if this is that entry, we skip it
            if q_entry.path != root_path_buf {
                Scanner::handle_scan_item(db, scan, ItemType::Directory, q_entry.path.as_path(), &q_entry.metadata)?;
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

                    Scanner::handle_scan_item(db, scan, item_type, &item.path(), &metadata)?;
                }
            }
        }

        dir_prog.finish_with_message("Scan complete");
        item_prog.finish_and_clear();

        scan.set_state_sweeping(db)
    }

    fn do_state_sweeping(db: &mut Database, scan: &mut Scan, multi_prog: &mut MultiProgress) -> Result<(), FsPulseError> { 
        let tx = db.conn_mut().transaction()?;

        let sweep_prog = multi_prog.add(ProgressBar::new_spinner());
        sweep_prog.set_message("Detecting changes and deletions...");
        sweep_prog.enable_steady_tick(Duration::from_millis(250));

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
            "UPDATE items SET 
                is_tombstone = 1,
                last_scan_id = ?
            WHERE root_id = ? AND last_scan_id < ? AND is_tombstone = 0",
            (scan.id(), scan.root_id(), scan.id()),
        )?;

        tx.commit()?;

        sweep_prog.finish_with_message("Change and delete detection complete");
        scan.set_state_analyzing(db)
    }

    fn do_state_analyzing(
        db: Arc<Mutex<Database>>, 
        scan: &mut Scan, 
        multi_prog: Arc<MultiProgress>,
    ) -> Result<(), FsPulseError> {

        let hashing = scan.hashing();
        let validating = scan.validating();

        // If the scan doesn't hash or validate, then the scan
        // can be marked complete and we just return
        if !hashing && !validating {
            scan.set_state_completed(&mut *db.lock().unwrap())?;
            return Ok(());
        }

        let file_count = scan.file_count().unwrap_or_default().max(0) as u64;                   // scan.file_count is the total # of files in the scan
        let analyzed_items= Item::count_analyzed_items(&db.lock().unwrap(), scan.id())?.max(0) as u64;  // may be resuming the scan

        let analysis_prog = multi_prog.add(ProgressBar::new(file_count));
        analysis_prog.set_style(ProgressStyle::default_bar()
            .template("{msg}\n[{bar:80}] {pos}/{len} (Remaining: {eta})")
            .unwrap()
            .progress_chars("#>-"));
        analysis_prog.set_message("Analyzing files:");
        analysis_prog.inc(analyzed_items);

        let multi_prog_arc = Arc::new((*multi_prog).clone());
        let analysis_prog_arc = Arc::new(analysis_prog);

        // Create a bounded channel to limit the number of queued tasks (e.g., max 100 tasks)
        let (sender, receiver) = bounded::<Item>(100);

        // Initialize the thread pool. Current 4 threads
        let pool = ThreadPool::new(20);
        for _ in 0..20 {
            // Clone shared resources for each worker thread.
            let receiver = receiver.clone();
            let multi_prog_clone = Arc::clone(&multi_prog_arc);
            let db = Arc::clone(&db);
            let analysis_prog_clone = Arc::clone(&analysis_prog_arc);
            let scan_id = scan.id();
            
            // Worker thread: continuously receive and process tasks.
            pool.execute(move || {
                while let Ok(item) = receiver.recv() {
                    Scanner::process_item_async(&db, scan_id, item, hashing, validating, multi_prog_clone.clone(), analysis_prog_clone.clone());
                }
            });
        }

        let mut last_item_id = 0;

        loop {
            let items = Item::fetch_next_analysis_batch(
                &db.lock().unwrap(),
                scan.id(),
                scan.hashing(),
                scan.validating(),
                last_item_id,
                10,
            )?;

            if items.is_empty() {
                break;
            }

            for item in items {
                // Items will be ordered by id. We keep track of the last seen id and provide
                // it in calls to fetch_next_analysis_batch to avoid picking up unprocessed
                // items that we've already picked up. This avoids a race condition
                // in which we'd pick up unprocessed items that are currently being processed
                last_item_id = item.id();

                // This send will block if the channel already has 100 items.
                sender.send(item)
                    .expect("Failed to send task into the bounded channel");
            }
        }

        // Drop the sender to signal the workers that no more items will come.
        drop(sender);

        // Wait for all tasks to complete.
        pool.join();
        analysis_prog_arc.finish_with_message("Analysis complete");
        
        scan.set_state_completed(&mut db.lock().unwrap())
    }

    fn process_item_async(
        db: &Arc<Mutex<Database>>, 
        scan_id: i64, 
        item: Item, 
        hashing: bool, 
        validating: bool, 
        multi_prog: Arc<MultiProgress>, 
        analysis_prog: Arc<ProgressBar>) {
        // TODO: Improve the error handling for all analysis. Need to differentiate
        // between file system errors and actual content errors
        
        let path = Path::new(item.path());
        
        let display_path = path.file_name()
            .unwrap_or_else(|| path.as_os_str())
            .to_string_lossy();
        
        //let display_path = Utils::format_path_for_display(&path, 60);

        let mut new_hash = None;

        if hashing {
            let mut hash_prog = multi_prog.add(ProgressBar::new(0));

            hash_prog.set_style(ProgressStyle::default_bar()
                .template("{msg}\n[{bar:80}] {bytes}/{total_bytes} ({eta})")
                .unwrap()
                .progress_chars("-> "));

            hash_prog.set_message(format!("Hashing: '{}'", &display_path));

            new_hash = match Analysis::compute_md5_hash(&path, &mut hash_prog) {
                Ok(hash_s) => Some(hash_s),
                Err(error) => {
                    error!("Error hashing '{}': {}", &display_path, error);
                    None
                },
            };

            hash_prog.finish_and_clear();
        }
        
        let mut new_validation_state = ValidationState::Unknown;
        let mut new_validation_state_desc = None;

        if validating {
            let is_flac = Utils::has_flac_extension(&path);

            if is_flac {
                let is_valid_prog = multi_prog.add(ProgressBar::new_spinner());
                is_valid_prog.set_message(format!("Validating: '{}'", &display_path));
                //is_valid_prog.enable_steady_tick(Duration::from_millis(250));

                match Analysis::validate_flac_claxon2(&path, &display_path, &is_valid_prog) {
                    Ok((res_validation_state, res_validation_state_desc)) => {
                        new_validation_state = res_validation_state;
                        new_validation_state_desc = res_validation_state_desc;
                    },
                    Err(error) => {
                        let e_str = format!("{:?}", error);
                        error!("Error validating '{}': {}", &display_path, e_str);
                        new_validation_state = ValidationState::Invalid;
                    }
                }

                is_valid_prog.finish_and_clear();
            } else {
                new_validation_state = ValidationState::NoValidator;
            }
        }

        match Scanner::update_item_analysis(db, scan_id, hashing, validating, &item, new_hash, new_validation_state, new_validation_state_desc) {
            Err(error) => {
                let e_str = format!("{:?}", error);
                error!("Error updating item analysis '{}': {}", &display_path, e_str);
            },
            _ => {},
        }
        
        analysis_prog.inc(1);
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
                let tx = db.conn_mut().transaction()?;
                let rows_updated = tx.execute(
                    "UPDATE items SET 
                            is_tombstone = 0, 
                            item_type = ?, 
                            last_modified = ?, 
                            file_size = ?, 
                            file_hash = NULL, 
                            validation_state = ?,
                            validation_state_desc = NULL,
                            last_scan_id = ?,
                            last_hash_scan_id = NULL, 
                            last_validation_scan_id = NULL 
                        WHERE id = ?", 
                    (item_type_str, last_modified, file_size, ValidationState::Unknown.to_string(), scan.id(), existing_item.id()))?;
                if rows_updated == 0 {
                        return Err(FsPulseError::Error(format!("Item Id {} not found for update", existing_item.id())));
                }
                tx.execute("INSERT INTO changes (scan_id, item_id, change_type) VALUES (?, ?, ?)", 
                    (scan.id(), existing_item.id(), ChangeType::Add.as_str()))?;
                tx.commit()?;
            } else if existing_item.item_type() != item_type_str {
                //Item type changed file <-> folder
                let tx = db.conn_mut().transaction()?;
                let rows_updated = tx.execute(
                    "UPDATE items SET 
                        item_type = ?, 
                        last_modified = ?, 
                        file_size = ?,
                        file_hash = NULL,
                        validation_state = ?,
                        validation_state_desc = NULL,
                        last_scan_id = ?,
                        last_hash_scan_id = NULL,
                        last_validation_scan_id = NULL 
                    WHERE id = ?", 
                    (item_type_str, last_modified, file_size, ValidationState::Unknown.to_string(), scan.id(), existing_item.id()))?;
                if rows_updated == 0 {
                        return Err(FsPulseError::Error(format!("Item Id {} not found for update", existing_item.id())));
                }
    
                tx.execute("INSERT INTO changes (scan_id, item_id, change_type) VALUES (?, ?, ?)", 
                    (scan.id(), existing_item.id(), ChangeType::TypeChange.as_str()))?;
                tx.commit()?;
            } else if metadata_changed {
                let tx = db.conn_mut().transaction()?;

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
                let rows_updated = db.conn().execute(
                    "UPDATE items SET last_scan_id = ? WHERE id = ?", 
                    (scan.id(), existing_item.id()))?;
                if rows_updated == 0 {
                        return Err(FsPulseError::Error(format!("Item Id {} not found for update", existing_item.id())));
                }
            }
        } else {
            // Item is new, insert into items and changes tables
            let tx = db.conn_mut().transaction()?;
            tx.execute("INSERT INTO items (root_id, path, item_type, last_modified, file_size, validation_state, last_scan_id) VALUES (?, ?, ?, ?, ?, ?, ?)",
                (scan.root_id(), &path_str, item_type.as_str(), last_modified, file_size, ValidationState::Unknown.to_string(), scan.id()))?;
            let item_id: i64 = tx.query_row("SELECT last_insert_rowid()", [], |row| row.get(0))?;
            tx.execute("INSERT INTO changes (scan_id, item_id, change_type) VALUES (?, ?, ?)",
                (scan.id(), item_id, ChangeType::Add.as_str()))?;
            tx.commit()?;
        }
        
        Ok(())
    }

    pub fn update_item_analysis(
        db: &Arc<Mutex<Database>>,
        scan_id: i64,
        hashing: bool,
        validating: bool,
        item: &Item,
        new_hash: Option<String>,
        new_validation_state: ValidationState,
        new_validation_state_desc: Option<String>,
    ) -> Result<(), FsPulseError> {
        let mut update_changes = false;

        // values to use when updating the changes table
        let mut hash_change = None;
        let mut validation_state_change = None;
        let mut validation_state_desc_change = None;

        // values to use in the item update
        let mut hash_item = item.file_hash();
        let mut validation_state_item = item.validation_state().to_str();
        let mut validation_state_desc_item = item.validation_state_desc();
        let mut last_hash_scan_id_item = item.last_hash_scan_id();
        let mut last_validation_scan_id_item = item.last_validation_scan_id();

        if hashing {
            if item.file_hash() != new_hash.as_deref() {
                // update the change record only if a previous hash had been computed
                if item.last_hash_scan_id().is_some() {
                    update_changes = true;

                    // for change update
                    hash_change = item.file_hash();
                }
                
                hash_item = new_hash.as_deref();
            }

            // Update the last scan id whether or not anything changed
            last_hash_scan_id_item = Some(scan_id);
        }

        if validating {
            if (item.validation_state() != new_validation_state) || (item.validation_state_desc() != new_validation_state_desc.as_deref()) {
                // update the change record only if a previous validation had been completed
                if item.last_validation_scan_id().is_some() {
                    update_changes = true;
                    validation_state_change = Some(item.validation_state().to_str());
                    validation_state_desc_change = item.validation_state_desc();
                }

                // always update the item when the validation state changes
                validation_state_item = new_validation_state.to_str();
                validation_state_desc_item = new_validation_state_desc.as_deref();
            }

            // update the last validation scan id whether or not anything changed
            last_validation_scan_id_item = Some(scan_id);
        }

        let mut db_guard = db.lock().unwrap();
        let conn = db_guard.conn_mut();
        
        let tx = conn.transaction()?; // Start transaction

        // Step 1: UPSERT into `changes` table if the change is something other than moving from the default state
        if update_changes {
            tx.execute(
                "INSERT INTO changes (scan_id, item_id, change_type, prev_hash, prev_validation_state, prev_validation_state_desc)
                    VALUES (?, ?, 'M', ?, ?, ?)
                ON CONFLICT(scan_id, item_id, change_type) 
                    DO UPDATE SET 
                        prev_hash = excluded.prev_hash,
                        prev_validation_state = excluded.prev_validation_state,
                        prev_validation_state_desc = excluded.prev_validation_state_desc",
                rusqlite::params![
                    scan_id, 
                    item.id(),
                    hash_change, 
                    validation_state_change, 
                    validation_state_desc_change,
                ],
            )?;
        }

        // Step 2: Update `items` table
        tx.execute(
            "UPDATE items 
                SET 
                    file_hash = ?,
                    validation_state = ?,
                    validation_state_desc = ?,
                    last_hash_scan_id = ?,
                    last_validation_scan_id = ?
            WHERE id = ?",
            rusqlite::params![
                hash_item,
                validation_state_item,
                validation_state_desc_item,
                last_hash_scan_id_item,
                last_validation_scan_id_item,
                item.id()
            ],
        )?;

        tx.commit()?; // Commit transaction

        Ok(())
    }
}
