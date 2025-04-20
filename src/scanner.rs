// Scan States
// 1. Initial scan.
//      - New items: create Item with metadata,last_scan; create change (Add)
//      - For each found item:
//          - If tombstone: Update item type, metadata, is_ts, last_scan; null hash, valid; create change (Add)
//          - If folder <-> file change: update Item metadata, last_scan; null hash, valid; create change (Type Changed)
//          - If metadata change: update Item metadata, last_scan; create change (Modify)
//  (Set State to 2)
// 2. Tombstone
//      - For each previously seen, non-tombstone item:
//          - Set is_ts; create change (Delete)
//  (If --hash or --validate, set state to 3 else set state to [TBD])
// 3. Hash and/or Validate
//      - For each non-tombstone, file item with last_scan < current scan:
//          - Hash and/or Validate per scan configuration
//          - If Hash and/or Valid are non-null and have changed, create change record with old value(s) of the changed value(s)
// 4. Completed
// 5. Stopped

use crate::changes::ChangeType;
use crate::config::CONFIG;
use crate::hash::Hash;
use crate::items::{Item, ItemType};
use crate::reports::Reports;
use crate::roots::Root;
use crate::scans::ScanState;
use crate::utils::Utils;
use crate::validators::validator::{from_path, ValidationState};
use crate::{database::Database, error::FsPulseError, scans::Scan};

use crossbeam_channel::bounded;
use dialoguer::theme::ColorfulTheme;
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};

use dialoguer::{MultiSelect, Select};
use log::error;
use threadpool::ThreadPool;
//use md5::digest::typenum::Abs;
use std::collections::VecDeque;
use std::fs::Metadata;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use std::{cmp, fs};

#[derive(Clone, Debug)]
struct QueueEntry {
    path: PathBuf,
    metadata: fs::Metadata,
}

pub struct Scanner {}

enum ScanDecision {
    NewScan,
    ContinueExisting,
    Exit,
}

impl Scanner {
    pub fn do_interactive_scan(
        db: &mut Database,
        multi_prog: &mut MultiProgress,
    ) -> Result<(), FsPulseError> {
        let root = match Root::interact_choose_root(db, "Scan which root?")? {
            Some(root) => root,
            None => return Ok(()),
        };

        // look for an existing, incomplete scan
        let mut existing_scan = Scan::get_latest_for_root(db, root.root_id())?
            .filter(|s| s.state() != ScanState::Completed && s.state() != ScanState::Stopped);

        // if a scan is found, ask the user if it should be stopped or resumed
        let mut scan = match existing_scan.as_mut() {
            Some(existing_scan) => {
                match Scanner::stop_or_resume_scan(db, &root, existing_scan, true)? {
                    ScanDecision::NewScan => Scanner::initiate_scan_interactive(db, &root)?,
                    ScanDecision::ContinueExisting => *existing_scan,
                    ScanDecision::Exit => return Ok(()),
                }
            }
            None => Scanner::initiate_scan_interactive(db, &root)?,
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
                _ => (),
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
                let scan = Scan::get_latest_for_root(db, root.root_id())?.filter(|s: &Scan| {
                    s.state() != ScanState::Completed && s.state() != ScanState::Stopped
                });
                (root, scan)
            }
            (_, Some(root_path), _) => {
                let root_path_buf = Root::validate_and_canonicalize_path(&root_path)?;
                let root_path_str = root_path_buf.to_string_lossy().to_string();

                let root = Root::get_by_path(db, &root_path_str)?;
                match root {
                    Some(root) => {
                        // Found the root. Look for an outstanding scan
                        let scan = Scan::get_latest_for_root(db, root.root_id())?.filter(|s| {
                            s.state() != ScanState::Completed && s.state() != ScanState::Stopped
                        });
                        (root, scan)
                    }
                    None => {
                        // Create the new root
                        let new_root = Root::create(db, &root_path_str)?;
                        (new_root, None)
                    }
                }
            }
            (_, _, true) => {
                let scan = Scan::get_latest(db)?
                    .ok_or_else(|| FsPulseError::Error("No latest scan found".to_string()))?;
                let root = Root::get_by_id(db, scan.root_id())?.ok_or_else(|| {
                    FsPulseError::Error(format!(
                        "No root found for latest Scan Id {}",
                        scan.scan_id()
                    ))
                })?;

                let return_scan = if scan.state() != ScanState::Completed {
                    Some(scan)
                } else {
                    None
                };

                (root, return_scan)
            }
            _ => {
                return Err(FsPulseError::Error("Invalid arguments".into()));
            }
        };

        // If scan is present, it is incomplete. Ask the user to decide if it should be resumed or stopped.
        // Also allows the user to exit without making the choice now

        let mut scan = match existing_scan.as_mut() {
            Some(existing_scan) => {
                match Scanner::stop_or_resume_scan(db, &root, existing_scan, false)? {
                    ScanDecision::NewScan => Scanner::initiate_scan(db, &root, hash, validate)?,
                    ScanDecision::ContinueExisting => {
                        multi_prog.println("Resuming scan")?;
                        *existing_scan
                    }
                    ScanDecision::Exit => return Ok(()),
                }
            }
            None => Scanner::initiate_scan(db, &root, hash, validate)?,
        };

        Scanner::do_scan_machine(db, &mut scan, &root, multi_prog)?;

        Reports::report_scan(db, &scan)
    }

    fn do_scan_machine(
        db: &mut Database,
        scan: &mut Scan,
        root: &Root,
        multi_prog: &mut MultiProgress,
    ) -> Result<(), FsPulseError> {
        multi_prog.println("-- FsPulse Scan --")?;

        // Loop through all states, even if resuming, to allow progress updates
        let mut loop_state = ScanState::Scanning;

        loop {
            // When the state is completed, the scan is done
            if loop_state == ScanState::Completed {
                break;
            }

            match loop_state {
                ScanState::Scanning => {
                    let bar = Utils::add_section_bar(multi_prog, 1, "Quick scanning...");
                    if scan.state() == ScanState::Scanning {
                        Scanner::do_state_scanning(db, root, scan, multi_prog)?;
                    }
                    Utils::finish_section_bar(&bar, "✔ Quick scanning");
                    loop_state = ScanState::Sweeping;
                }
                ScanState::Sweeping => {
                    let bar = Utils::add_section_bar(multi_prog, 2, "Tombstoning deletes...");
                    if scan.state() == ScanState::Sweeping {
                        Scanner::do_state_sweeping(db, scan)?;
                    }
                    Utils::finish_section_bar(&bar, "✔ Tombstoning deletes");
                    loop_state = ScanState::Analyzing;
                }
                ScanState::Analyzing => {
                    // Should never get here in a situation in which scan.state() isn't Analyzing
                    // but we protect against it just in case
                    let bar = Utils::add_section_bar(multi_prog, 3, "Analyzing...");
                    let mut analysis_result = Ok(());
                    if scan.state() == ScanState::Analyzing {
                        // This is a boundary - we'll take ownership of the database and progress
                        // bars here and then restore them when we're done
                        let owned_db = std::mem::take(db);
                        let db_arc = Arc::new(Mutex::new(owned_db));

                        analysis_result =
                            Scanner::do_state_analyzing(db_arc.clone(), scan, multi_prog);

                        // recover the database from the Arc
                        let recovered_db = Arc::try_unwrap(db_arc)
                            .expect("No additional clones should exist")
                            .into_inner()
                            .expect("Mutex isn't poisoned");

                        *db = recovered_db;
                    }

                    analysis_result?;

                    Utils::finish_section_bar(&bar, "✔ Analyzing");
                    loop_state = ScanState::Completed;
                }
                unexpected => {
                    return Err(FsPulseError::Error(format!(
                        "Unexpected incomplete scan state: {}",
                        unexpected
                    )));
                }
            };
        }

        println!();
        println!();
        Reports::report_scan(db, scan)?;

        //println!("Completed scan and several previous scans on this root:");
        //let query = format!("scans where root_id:({}) show all order by scan_id desc limit 5", scan.root_id());
        //QueryProcessor::process_query(db, &query)?;

        Ok(())
    }

    fn stop_or_resume_scan(
        db: &mut Database,
        root: &Root,
        scan: &mut Scan,
        report: bool,
    ) -> Result<ScanDecision, FsPulseError> {
        let options = vec![
            "Resume the scan",
            "Stop & exit",
            "Stop & start a new scan",
            "Exit (decide later)",
        ];

        let selection = Select::with_theme(&ColorfulTheme::default())
            .with_prompt(format!(
                "Found in-progress scan on:'{}'\n\nWhat would you like to do?",
                root.root_path()
            ))
            .default(0)
            .items(&options)
            .report(report)
            .interact()
            .unwrap();

        let decision = match selection {
            0 => match scan.state() {
                ScanState::Scanning => ScanDecision::ContinueExisting,
                ScanState::Sweeping => ScanDecision::ContinueExisting,
                ScanState::Analyzing => ScanDecision::ContinueExisting,
                _ => {
                    return Err(FsPulseError::Error(format!(
                        "Unexpected incomplete scan state: {}",
                        scan.state()
                    )))
                }
            },
            1 => {
                scan.set_state_stopped(db)?;
                ScanDecision::Exit
            }
            2 => {
                scan.set_state_stopped(db)?;
                ScanDecision::NewScan
            }
            _ => ScanDecision::Exit, // exit
        };

        Ok(decision)
    }

    fn initiate_scan(
        db: &mut Database,
        root: &Root,
        hashing: bool,
        validating: bool,
    ) -> Result<Scan, FsPulseError> {
        Scan::create(db, root, hashing, validating)
    }

    fn do_state_scanning(
        db: &mut Database,
        root: &Root,
        scan: &mut Scan,
        multi_prog: &mut MultiProgress,
    ) -> Result<(), FsPulseError> {
        let root_path_buf = PathBuf::from(root.root_path());
        let metadata = fs::symlink_metadata(&root_path_buf)?;

        let mut q = VecDeque::new();

        let dir_prog = Utils::add_spinner_bar(multi_prog, "   ", "Directory:", true);

        let item_prog = Utils::add_spinner_bar(multi_prog, "   ", "File:", true);

        q.push_back(QueueEntry {
            path: root_path_buf.clone(),
            metadata,
        });

        while let Some(q_entry) = q.pop_front() {
            dir_prog.set_message(format!("Directory: '{}'", q_entry.path.to_string_lossy()));

            // Handle the directory itself before iterating its contents. The root dir
            // was previously pushed into the queue - if this is that entry, we skip it
            if q_entry.path != root_path_buf {
                Scanner::handle_scan_item(
                    db,
                    scan,
                    ItemType::Directory,
                    q_entry.path.as_path(),
                    &q_entry.metadata,
                )?;
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

        item_prog.finish_and_clear();
        dir_prog.finish_and_clear();

        scan.set_state_sweeping(db)
    }

    fn do_state_sweeping(db: &mut Database, scan: &mut Scan) -> Result<(), FsPulseError> {
        let tx = db.conn_mut().transaction()?;

        // Insert deletion records into changes
        tx.execute(
            "INSERT INTO changes (scan_id, item_id, change_type)
                SELECT ?, item_id, ?
                FROM items
                WHERE root_id = ? AND is_ts = 0 AND last_scan < ?",
            (
                scan.scan_id(),
                ChangeType::Delete.as_str(),
                scan.root_id(),
                scan.scan_id(),
            ),
        )?;

        // Mark unseen items as tombstones
        tx.execute(
            "UPDATE items SET 
                is_ts = 1,
                last_scan = ?
            WHERE root_id = ? AND last_scan < ? AND is_ts = 0",
            (scan.scan_id(), scan.root_id(), scan.scan_id()),
        )?;

        tx.commit()?;

        scan.set_state_analyzing(db)
    }

    fn do_state_analyzing(
        db: Arc<Mutex<Database>>,
        scan: &mut Scan,
        multi_prog: &MultiProgress,
    ) -> Result<(), FsPulseError> {
        let hashing = scan.hashing();
        let validating = scan.validating();

        // If the scan doesn't hash or validate, then the scan
        // can be marked complete and we just return
        if !hashing && !validating {
            scan.set_state_completed(&mut db.lock().unwrap())?;
            return Ok(());
        }

        let file_count = scan.file_count().unwrap_or_default().max(0) as u64; // scan.file_count is the total # of files in the scan
        let analyzed_items =
            Item::count_analyzed_items(&db.lock().unwrap(), scan.scan_id())?.max(0) as u64; // may be resuming the scan

        let analysis_prog = multi_prog.add(
            ProgressBar::new(file_count)
                .with_style(
                    ProgressStyle::default_bar()
                        .template("{prefix}{msg} [{bar:80}] {pos}/{len} (Remaining: {eta})")
                        .unwrap()
                        .progress_chars("#>-"),
                )
                .with_prefix("   ")
                .with_message("Files"),
        );

        analysis_prog.inc(analyzed_items);

        // Create a bounded channel to limit the number of queued tasks (e.g., max 100 tasks)
        let (sender, receiver) = bounded::<Item>(100);

        // Initialize the thread pool

        let items_remaining = file_count.saturating_sub(analyzed_items); // avoids underflow
        let items_remaining_usize = items_remaining.try_into().unwrap_or(usize::MAX);

        let config_threads = CONFIG
            .get()
            .expect("Config not initialized")
            .analysis
            .threads;

        let num_threads = cmp::min(items_remaining_usize, config_threads);
        let pool = ThreadPool::new(num_threads.max(1)); // ensure at least one thread

        for thread_index in 0..num_threads {
            // Clone shared resources for each worker thread.
            let receiver = receiver.clone();
            let db = Arc::clone(&db);
            let analysis_prog_clone = analysis_prog.clone();
            let scan_copy = *scan;
            //let scan_id = scan.id();

            // Create a reusable progress bar for this thread
            let thread_prog = multi_prog.add(ProgressBar::new(0));
            thread_prog.set_style(
                ProgressStyle::default_spinner()
                    .template("{prefix} {spinner} {msg}")
                    .unwrap(),
            );
            // Format thread label like [01/20], [02/20], ..., [20/20]
            let thread_prog_label = format!(
                "   [{:0width$}/{}]",
                thread_index + 1,
                num_threads,
                width = if num_threads >= 10 { 2 } else { 1 }
            );
            thread_prog.set_prefix(thread_prog_label.clone());
            thread_prog.set_message("Waiting...".to_string());

            // Worker thread: continuously receive and process tasks.
            pool.execute(move || {
                while let Ok(item) = receiver.recv() {
                    Scanner::process_item_async(
                        &db,
                        scan_copy,
                        item,
                        &analysis_prog_clone,
                        &thread_prog,
                    );
                }
                thread_prog.set_style(
                    ProgressStyle::default_spinner()
                        .template("{prefix} {msg}")
                        .unwrap(),
                );
                thread_prog.finish_and_clear();
                //thread_prog.finish_with_message(format!("✔ Done"));
            });
        }

        let mut last_item_id = 0;

        loop {
            let items = Item::fetch_next_analysis_batch(
                &db.lock().unwrap(),
                scan.scan_id(),
                scan.hashing(),
                scan.validating(),
                last_item_id,
                100,
            )?;

            if items.is_empty() {
                break;
            }

            for item in items {
                // Items will be ordered by id. We keep track of the last seen id and provide
                // it in calls to fetch_next_analysis_batch to avoid picking up unprocessed
                // items that we've already picked up. This avoids a race condition
                // in which we'd pick up unprocessed items that are currently being processed
                last_item_id = item.item_id();

                // This send will block if the channel already has 100 items.
                sender
                    .send(item)
                    .expect("Failed to send task into the bounded channel");
            }
        }

        // Drop the sender to signal the workers that no more items will come.
        drop(sender);

        // Wait for all tasks to complete.
        pool.join();
        analysis_prog.set_style(
            ProgressStyle::default_spinner()
                .template("{prefix} ✔ Analyzing ({pos} files)")
                .unwrap(),
        );
        //analysis_prog.finish_with_message("✔ Analyzing");
        analysis_prog.finish_and_clear();
        scan.set_state_completed(&mut db.lock().unwrap())
    }

    fn process_item_async(
        db: &Arc<Mutex<Database>>,
        scan: Scan,
        item: Item,
        analysis_prog: &ProgressBar,
        thread_prog: &ProgressBar,
    ) {
        // TODO: Improve the error handling for all analysis. Need to differentiate
        // between file system errors and actual content errors

        let path = Path::new(item.item_path());

        let display_path = path
            .file_name()
            .unwrap_or(path.as_os_str())
            .to_string_lossy();

        let mut new_hash = None;

        if scan.hashing() {
            thread_prog.set_style(
                ProgressStyle::default_bar()
                    .template("{prefix} {msg} [{bar:40}] {bytes}/{total_bytes}")
                    .unwrap()
                    .progress_chars("=> "),
            );

            thread_prog.set_message(format!("Hashing: '{}'", &display_path));
            thread_prog.set_position(0); // reset in case left from previous
            thread_prog.set_length(0);

            new_hash = match Hash::compute_md5_hash(path, thread_prog) {
                Ok(hash_s) => Some(hash_s),
                Err(error) => {
                    error!("Error hashing '{}': {}", &display_path, error);
                    None
                }
            };
        }

        let mut new_val = ValidationState::Unknown;
        let mut new_val_error = None;

        if scan.validating() {
            let validator = from_path(path);
            match validator {
                Some(v) => {
                    thread_prog.set_style(
                        ProgressStyle::default_spinner()
                            .template("{prefix} {spinner} {msg}")
                            .unwrap(),
                    );
                    thread_prog.set_message(format!("Validating: '{}'", &display_path));
                    let steady_tick = v.wants_steady_tick();

                    if steady_tick {
                        thread_prog.enable_steady_tick(Duration::from_millis(250));
                    }
                    match v.validate(path, thread_prog) {
                        Ok((res_validity_state, res_validation_error)) => {
                            new_val = res_validity_state;
                            new_val_error = res_validation_error;
                        }
                        Err(e) => {
                            let e_str = e.to_string();
                            error!("Error validating '{}': {}", &display_path, e_str);
                            new_val = ValidationState::Invalid;
                        }
                    }
                    if steady_tick {
                        thread_prog.disable_steady_tick();
                    }
                }
                None => new_val = ValidationState::NoValidator,
            }
        }

        if let Err(error) =
            Scanner::update_item_analysis(db, &scan, &item, new_hash, new_val, new_val_error)
        {
            let e_str = error.to_string();
            error!(
                "Error updating item analysis '{}': {}",
                &display_path, e_str
            );
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
        let existing_item = Item::get_by_root_path_type(db, scan.root_id(), &path_str, item_type)?;

        let mod_date = metadata
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs() as i64);

        let file_size = if metadata.is_file() {
            Some(metadata.len() as i64)
        } else {
            None
        };

        // If the item was already processed for this scan, just skip it. We intentionally
        // do not handle the case where the item was seen within this scan, but has since
        // either been mod_date or has changed type. There are edge cases where this might
        // cause strangeness in reports such as when an item was seen as a file, the scan
        // was resumed and the item has changed into a directory. In this case, we'll still
        // traverse the children within the resumed scan and a tree report will look odd
        if let Some(existing_item) = existing_item {
            if existing_item.last_scan() == scan.scan_id() {
                return Ok(());
            }

            let meta_change =
                existing_item.mod_date() != mod_date || existing_item.file_size() != file_size;

            if existing_item.is_ts() {
                // Rehydrate a tombstone
                let tx = db.conn_mut().transaction()?;
                let rows_updated = tx.execute(
                    "UPDATE items SET 
                            is_ts = 0, 
                            mod_date = ?, 
                            file_size = ?, 
                            file_hash = NULL, 
                            val = ?,
                            val_error = NULL,
                            last_scan = ?,
                            last_hash_scan = NULL, 
                            last_val_scan = NULL 
                        WHERE item_id = ?",
                    (
                        mod_date,
                        file_size,
                        ValidationState::Unknown.to_string(),
                        scan.scan_id(),
                        existing_item.item_id(),
                    ),
                )?;
                if rows_updated == 0 {
                    return Err(FsPulseError::Error(format!(
                        "Item Id {} not found for update",
                        existing_item.item_id()
                    )));
                }

                tx.execute(
                    "INSERT INTO changes 
                        (
                            scan_id, 
                            item_id, 
                            change_type, 
                            is_undelete, 
                            mod_date_old, 
                            mod_date_new, 
                            file_size_old, 
                            file_size_new, 
                            hash_old,
                            val_old, 
                            val_error_old
                        ) 
                    VALUES 
                        (?, ?, ?, 1, ?, ?, ?, ?, ?, ?, ?)",
                    (
                        scan.scan_id(),
                        existing_item.item_id(),
                        ChangeType::Add.as_str(),
                        existing_item.mod_date(),
                        mod_date,
                        existing_item.file_size(),
                        file_size,
                        existing_item.file_hash(),
                        existing_item.validity_state_as_str(),
                        existing_item.val_error(),
                    ),
                )?;

                tx.commit()?;
            } else if meta_change {
                let tx = db.conn_mut().transaction()?;

                let rows_updated = tx.execute(
                    "UPDATE items SET
                        mod_date = ?, 
                        file_size = ?,             
                        last_scan = ? 
                    WHERE item_id = ?",
                    (mod_date, file_size, scan.scan_id(), existing_item.item_id()),
                )?;
                if rows_updated == 0 {
                    return Err(FsPulseError::Error(format!(
                        "Item Id {} not found for update",
                        existing_item.item_id()
                    )));
                }
                tx.execute(
                    "INSERT INTO changes 
                            (
                                scan_id, 
                                item_id, 
                                change_type, 
                                meta_change, 
                                mod_date_old, 
                                mod_date_new, 
                                file_size_old, 
                                file_size_new) 
                            VALUES (?, ?, ?, 1, ?, ?, ?, ?)",
                    (
                        scan.scan_id(),
                        existing_item.item_id(),
                        ChangeType::Modify.as_str(),
                        meta_change.then_some(existing_item.mod_date()),
                        meta_change.then_some(mod_date),
                        meta_change.then_some(existing_item.file_size()),
                        meta_change.then_some(file_size),
                    ),
                )?;

                tx.commit()?;
            } else {
                // No change - just update last_scan
                let rows_updated = db.conn().execute(
                    "UPDATE items SET last_scan = ? WHERE item_id = ?",
                    (scan.scan_id(), existing_item.item_id()),
                )?;

                if rows_updated == 0 {
                    return Err(FsPulseError::Error(format!(
                        "Item Id {} not found for update",
                        existing_item.item_id()
                    )));
                }
            }
        } else {
            // Item is new, insert into items and changes tables
            let tx = db.conn_mut().transaction()?;

            tx.execute("INSERT INTO items (root_id, item_path, item_type, mod_date, file_size, val, last_scan) VALUES (?, ?, ?, ?, ?, ?, ?)",
                (scan.root_id(), &path_str, item_type.as_str(), mod_date, file_size, ValidationState::Unknown.to_string(), scan.scan_id()))?;

            let item_id: i64 = tx.query_row("SELECT last_insert_rowid()", [], |row| row.get(0))?;

            tx.execute("INSERT INTO changes (scan_id, item_id, change_type, is_undelete, mod_date_new, file_size_new, hash_change, val_change) VALUES (?, ?, ?, 0, ?, ?, 0, 0)",
                (scan.scan_id(), item_id, ChangeType::Add.as_str(), mod_date, file_size))?;
            tx.commit()?;
        }

        Ok(())
    }

    pub fn update_item_analysis(
        db: &Arc<Mutex<Database>>,
        scan: &Scan,
        item: &Item,
        new_hash: Option<String>,
        new_val: ValidationState,
        new_val_error: Option<String>,
    ) -> Result<(), FsPulseError> {
        let mut update_changes = false;

        // values to use when updating the changes table
        let mut c_hash_change = Some(false);
        let mut c_last_hash_scan_old = None;
        let mut c_hash_old = None;
        let mut c_hash_new = None;
        let mut c_val_change = Some(false);
        let mut c_last_val_scan_old = None;
        let mut c_val_old = None;
        let mut c_val_new = None;
        let mut c_val_error_old = None;
        let mut c_val_error_new = None;

        // values to use in the item update
        let mut i_hash = item.file_hash();
        let mut i_val = item.val();
        let mut i_val_error = item.val_error();
        let mut i_last_hash_scan = item.last_hash_scan();
        let mut i_last_val_scan = item.last_val_scan();

        if scan.hashing() {
            if item.file_hash() != new_hash.as_deref() {
                // if either the hash or validation state changes, we update changes
                update_changes = true;
                c_hash_change = Some(true);
                c_last_hash_scan_old = item.last_hash_scan();
                c_hash_old = item.file_hash();
                c_hash_new = new_hash.as_deref();

                i_hash = new_hash.as_deref();
            }

            // Update the last scan id whether or not anything changed
            i_last_hash_scan = Some(scan.scan_id());
        }

        if scan.validating() {
            if (item.val() != new_val) || (item.val_error() != new_val_error.as_deref()) {
                // if either the hash or validation state changes, we update changes
                update_changes = true;
                c_val_change = Some(true);
                c_last_val_scan_old = item.last_val_scan();
                c_val_old = Some(item.val().as_str());
                c_val_new = Some(new_val.as_str());
                c_val_error_old = item.val_error();
                c_val_error_new = new_val_error.as_deref();

                // always update the item when the validation state changes
                i_val = new_val;
                i_val_error = new_val_error.as_deref();
            }

            // update the last validation scan id whether or not anything changed
            i_last_val_scan = Some(scan.scan_id());
        }

        let mut db_guard = db.lock().unwrap();
        let conn = db_guard.conn_mut();

        let tx = conn.transaction()?; // Start transaction

        // Step 1: UPSERT into `changes` table if the change is something other than moving from the default state
        if update_changes {
            tx.execute(
                "INSERT INTO changes (
                        scan_id, 
                        item_id, 
                        change_type, 
                        meta_change, 
                        hash_change, 
                        last_hash_scan_old,
                        hash_old,
                        hash_new,
                        val_change,
                        last_val_scan_old,
                        val_old, 
                        val_new,
                        val_error_old,
                        val_error_new
                    )
                    VALUES (?, ?, 'M', 0, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                ON CONFLICT(scan_id, item_id) 
                    DO UPDATE SET
                        hash_change = excluded.hash_change,
                        last_hash_scan_old = excluded.last_hash_scan_old,
                        hash_old = excluded.hash_old,
                        hash_new = excluded.hash_new,
                        val_change = excluded.val_change,
                        last_val_scan_old = excluded.last_val_scan_old,
                        val_old = excluded.val_old,
                        val_new = excluded.val_new,
                        val_error_old = excluded.val_error_old,
                        val_error_new = excluded.val_error_new",
                rusqlite::params![
                    scan.scan_id(),
                    item.item_id(),
                    c_hash_change,
                    c_last_hash_scan_old,
                    c_hash_old,
                    c_hash_new,
                    c_val_change,
                    c_last_val_scan_old,
                    c_val_old,
                    c_val_new,
                    c_val_error_old,
                    c_val_error_new,
                ],
            )?;
        }

        // Step 2: Update `items` table
        tx.execute(
            "UPDATE items 
            SET 
                file_hash = ?,
                val = ?,
                val_error = ?,
                last_hash_scan = ?,
                last_val_scan = ?
            WHERE item_id = ?",
            rusqlite::params![
                i_hash,
                i_val.as_str(),
                i_val_error,
                i_last_hash_scan,
                i_last_val_scan,
                item.item_id()
            ],
        )?;

        tx.commit()?; // Commit transaction

        Ok(())
    }
}
