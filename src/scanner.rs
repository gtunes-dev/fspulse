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

use crate::alerts::Alerts;
use crate::changes::ChangeType;
use crate::config::{HashFunc, CONFIG};
use crate::hash::Hash;
use crate::items::{AnalysisItem, Item, ItemType};
use crate::progress::{ProgressConfig, ProgressId, ProgressReporter, ProgressStyle, WorkUpdate};
use crate::reports::Reports;
use crate::roots::Root;
use crate::scans::{AnalysisSpec, ScanState};
use crate::validate::validator::{from_path, ValidationState};
use crate::{database::Database, error::FsPulseError, scans::Scan};

use console::style;
use crossbeam_channel::bounded;
use dialoguer::theme::ColorfulTheme;
use dialoguer::{MultiSelect, Select};
use log::{error, info};
use threadpool::ThreadPool;

use std::collections::VecDeque;
use std::fs::Metadata;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
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
        reporter: Arc<dyn ProgressReporter>,
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
                    ScanDecision::ContinueExisting => existing_scan.clone(),
                    ScanDecision::Exit => return Ok(()),
                }
            }
            None => Scanner::initiate_scan_interactive(db, &root)?,
        };

        // CLI mode doesn't support cancellation - use a dummy token that's always false
        let cancel_token = Arc::new(AtomicBool::new(false));

        // Wrap scan execution with error handling
        match Scanner::do_scan_machine(db, &mut scan, &root, reporter, cancel_token) {
            Ok(()) => {
                println!();
                println!();
                Reports::report_scan(db, &scan)?;
                Ok(())
            },
            Err(e) => {
                // Stop scan with error message
                Scan::stop_scan(db, &scan, Some(&e.to_string()))?;
                Err(e)
            }
        }
    }

    fn initiate_scan_interactive(db: &mut Database, root: &Root) -> Result<Scan, FsPulseError> {
        let flags = vec!["No Hash", "Hash All", "No Validate", "Validate All"];

        let analysis_spec = loop {
            let selection = MultiSelect::new()
                .with_prompt("Scan options (by default, all files are hashed and new/changed files are validated):")
                .items(&flags)
                .interact()
                .unwrap();

            let mut no_hash = false;
            let mut hash_new = false;
            let mut no_validate = false;
            let mut validate_all = false;

            for selected_flag in selection.iter() {
                match selected_flag {
                    0 => no_hash = true,
                    1 => hash_new = true,
                    2 => no_validate = true,
                    3 => validate_all = true,
                    _ => (),
                }
            }

            if no_hash && hash_new {
                println!(
                    "{}",
                    style("Conflicting selections: 'No Hash' and 'Hash New'").yellow()
                );
                continue;
            }

            if no_validate && validate_all {
                println!(
                    "{}",
                    style("Conflicting selections: 'No Validate' and 'Validate All'").yellow()
                );
                continue;
            }

            break AnalysisSpec::new(no_hash, hash_new, no_validate, validate_all);
        };

        Scanner::initiate_scan(db, root, &analysis_spec)
    }

    pub fn do_scan_command(
        db: &mut Database,
        root_id: Option<u32>,
        root_path: Option<String>,
        last: bool,
        analysis_spec: &AnalysisSpec,
        reporter: Arc<dyn ProgressReporter>,
    ) -> Result<(), FsPulseError> {
        // If an incomplete scan exists, find it.
        let (root, mut existing_scan) = match (root_id, root_path, last) {
            (Some(root_id), _, _) => {
                let root = Root::get_by_id(db.conn(), root_id.into())?
                    .ok_or_else(|| FsPulseError::Error(format!("Root id {root_id} not found")))?;
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
                let root = Root::get_by_id(db.conn(), scan.root_id())?.ok_or_else(|| {
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
                    ScanDecision::NewScan => Scanner::initiate_scan(db, &root, analysis_spec)?,
                    ScanDecision::ContinueExisting => {
                        reporter
                            .println("Resuming scan")
                            .map_err(|e| FsPulseError::Error(e.to_string()))?;
                        existing_scan.clone()
                    }
                    ScanDecision::Exit => return Ok(()),
                }
            }
            None => Scanner::initiate_scan(db, &root, analysis_spec)?,
        };

        // CLI mode doesn't support cancellation - use a dummy token that's always false
        let cancel_token = Arc::new(AtomicBool::new(false));

        // Wrap scan execution with error handling
        match Scanner::do_scan_machine(db, &mut scan, &root, reporter, cancel_token) {
            Ok(()) => Reports::report_scan(db, &scan),
            Err(e) => {
                // Stop scan with error message
                Scan::stop_scan(db, &scan, Some(&e.to_string()))?;
                Err(e)
            }
        }
    }

    pub fn do_scan_machine(
        db: &mut Database,
        scan: &mut Scan,
        root: &Root,
        reporter: Arc<dyn ProgressReporter>,
        cancel_token: Arc<AtomicBool>,
    ) -> Result<(), FsPulseError> {
        // NOTE: We check for cancellation at appropriate points in the scanner code with:
        //
        //   if cancel_token.load(Ordering::Relaxed) {
        //       return Err(FsPulseError::ScanCancelled);
        //   }
        //
        //
        // After detecting cancellation and returning the error, the calling code will
        // invoke Scan::set_state_stopped() to atomically rollback all changes.

        reporter
            .println("-- FsPulse Scan --")
            .map_err(|e| FsPulseError::Error(e.to_string()))?;

        // Loop through all states, even if resuming, to allow progress updates
        let mut loop_state = ScanState::Scanning;

        loop {
            // When the state is completed, the scan is done. We check this before checking
            // for cancellation because a complete scan should not be treated as a successfully
            // cancelled scan
            if loop_state == ScanState::Completed {
                break;
            }

            // Check for cancellation at the top of the loop
            if cancel_token.load(Ordering::Acquire) {
                return Err(FsPulseError::ScanCancelled);
            }

            match loop_state {
                ScanState::Scanning => {
                    let section_id = reporter.section_start(1, "Quick scanning...");
                    if scan.state() == ScanState::Scanning {
                        Scanner::do_state_scanning(
                            db,
                            root,
                            scan,
                            reporter.clone(),
                            &cancel_token,
                        )?;
                    }
                    reporter.section_finish(section_id, "✔ Quick scanning");
                    loop_state = ScanState::Sweeping;
                }
                ScanState::Sweeping => {
                    let section_id = reporter.section_start(2, "Tombstoning deletes...");
                    if scan.state() == ScanState::Sweeping {
                        Scanner::do_state_sweeping(db, scan)?;
                    }
                    reporter.section_finish(section_id, "✔ Tombstoning deletes");
                    loop_state = ScanState::Analyzing;
                }
                ScanState::Analyzing => {
                    let section_id = reporter.section_start(3, "Analyzing...");

                    let mut analysis_result = Ok(());
                    // Should never get here in a situation in which scan.state() isn't Analyzing
                    // but we protect against it just in case

                    if scan.state() == ScanState::Analyzing {
                        // This is a boundary - we'll take ownership of the database and progress
                        // bars here and then restore them when we're done
                        let owned_db = std::mem::take(db);
                        let db_arc = Arc::new(Mutex::new(owned_db));

                        analysis_result = Scanner::do_state_analyzing(
                            db_arc.clone(),
                            scan,
                            reporter.clone(),
                            &cancel_token,
                        );

                        // recover the database from the Arc
                        let recovered_db = Arc::try_unwrap(db_arc)
                            .expect("No additional clones should exist")
                            .into_inner()
                            .expect("Mutex isn't poisoned");

                        *db = recovered_db;
                    }

                    analysis_result?;

                    reporter.section_finish(section_id, "✔ Analyzing");
                    loop_state = ScanState::Completed;
                }
                unexpected => {
                    return Err(FsPulseError::Error(format!(
                        "Unexpected incomplete scan state: {unexpected}"
                    )));
                }
            };
        }

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
        analysis_spec: &AnalysisSpec,
    ) -> Result<Scan, FsPulseError> {
        Scan::create(db.conn(), root, analysis_spec)
    }

    fn do_state_scanning(
        db: &mut Database,
        root: &Root,
        scan: &mut Scan,
        reporter: Arc<dyn ProgressReporter>,
        cancel_token: &Arc<AtomicBool>,
    ) -> Result<(), FsPulseError> {
        let root_path_buf = PathBuf::from(root.root_path());
        let metadata = fs::symlink_metadata(&root_path_buf)?;

        let mut q = VecDeque::new();

        let dir_prog = reporter.create(ProgressConfig {
            style: ProgressStyle::Spinner,
            prefix: "   ".to_string(),
            message: "Directory:".to_string(),
            steady_tick: Some(Duration::from_millis(250)),
        });

        let item_prog = reporter.create(ProgressConfig {
            style: ProgressStyle::Spinner,
            prefix: "   ".to_string(),
            message: "File:".to_string(),
            steady_tick: Some(Duration::from_millis(250)),
        });

        q.push_back(QueueEntry {
            path: root_path_buf.clone(),
            metadata,
        });

        let mut items_processed: i32 = 100;

        while let Some(q_entry) = q.pop_front() {
            // Check for cancellation every 100 items
            items_processed += 1;
            if items_processed % 100 == 0 && cancel_token.load(Ordering::Acquire) {
                return Err(FsPulseError::ScanCancelled);
            }

            reporter.update_work(
                dir_prog,
                WorkUpdate::Directory {
                    path: q_entry.path.to_string_lossy().to_string(),
                },
            );

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
                reporter.update_work(
                    item_prog,
                    WorkUpdate::File {
                        path: item.file_name().to_string_lossy().to_string(),
                    },
                );

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

        reporter.finish_and_clear(item_prog);
        reporter.finish_and_clear(dir_prog);

        scan.set_state_sweeping(db)
    }

    fn do_state_sweeping(db: &mut Database, scan: &mut Scan) -> Result<(), FsPulseError> {
        db.immediate_transaction(|conn| {
            // Insert deletion records into changes
            conn.execute(
                "INSERT INTO changes (scan_id, item_id, change_type)
                    SELECT ?, item_id, ?
                    FROM items
                    WHERE root_id = ? AND is_ts = 0 AND last_scan < ?",
                (
                    scan.scan_id(),
                    ChangeType::Delete.as_i64(),
                    scan.root_id(),
                    scan.scan_id(),
                ),
            )?;

            // Mark unseen items as tombstones
            conn.execute(
                "UPDATE items SET
                    is_ts = 1,
                    last_scan = ?
                WHERE root_id = ? AND last_scan < ? AND is_ts = 0",
                (scan.scan_id(), scan.root_id(), scan.scan_id()),
            )?;

            Ok(())
        })?;

        scan.set_state_analyzing(db)
    }

    fn do_state_analyzing(
        db: Arc<Mutex<Database>>,
        scan: &mut Scan,
        reporter: Arc<dyn ProgressReporter>,
        cancel_token: &Arc<AtomicBool>,
    ) -> Result<(), FsPulseError> {
        let is_hash = scan.analysis_spec().is_hash();
        let is_val = scan.analysis_spec().is_val();

        // If the scan doesn't hash or validate, then the scan
        // can be marked complete and we just return
        if !is_hash && !is_val {
            scan.set_state_completed(&mut db.lock().unwrap())?;
            return Ok(());
        }

        //let file_count = scan.file_count().unwrap_or_default().max(0) as u64; // scan.file_count is the total # of files in the scan
        let (analyze_total, analyze_done) =
            Item::get_analysis_counts(&db.lock().unwrap(), scan.scan_id(), scan.analysis_spec())?;

        //let analyzed_items =
        //    Item::count_analyzed_items(&db.lock().unwrap(), scan.scan_id())?.max(0) as u64; // may be resuming the scan

        let analysis_prog = reporter.create(ProgressConfig {
            style: ProgressStyle::Bar {
                total: analyze_total,
            },
            prefix: "   ".to_string(),
            message: "Files".to_string(),
            steady_tick: None,
        });

        reporter.set_position(analysis_prog, analyze_done);

        // Create a bounded channel to limit the number of queued tasks (e.g., max 100 tasks)
        let (sender, receiver) = bounded::<AnalysisItem>(100);

        // Initialize the thread pool

        let items_remaining = analyze_total.saturating_sub(analyze_done); // avoids underflow
        let items_remaining_usize = items_remaining.try_into().unwrap_or(usize::MAX);

        let (thread_count, hash_func) = {
            let config = CONFIG.get().expect("Config not initialized");
            let thread_count = config.analysis.threads();
            let hash_func = config.analysis.hash_func();

            (thread_count, hash_func)
        };

        let num_threads = cmp::min(items_remaining_usize, thread_count);
        let pool = ThreadPool::new(num_threads.max(1)); // ensure at least one thread

        for thread_index in 0..num_threads {
            // Clone shared resources for each worker thread.
            let receiver = receiver.clone();
            let db = Arc::clone(&db);
            let scan_copy = scan.clone();
            let reporter_clone = reporter.clone_reporter();
            let cancel_token_clone = Arc::clone(cancel_token);

            // Format thread label like [01/20], [02/20], ..., [20/20]
            let thread_prog_prefix = format!(
                "   [{:0width$}/{}]",
                thread_index + 1,
                num_threads,
                width = if num_threads >= 10 { 2 } else { 1 }
            );

            // Create a reusable progress indicator for this thread
            let thread_prog = reporter.create(ProgressConfig {
                style: ProgressStyle::Spinner,
                prefix: thread_prog_prefix,
                message: "".to_string(), // Initial message set via update_work below
                steady_tick: None,
            });

            // Set initial idle state
            reporter.update_work(thread_prog, WorkUpdate::Idle);

            // Worker thread: continuously receive and process tasks.
            pool.execute(move || {
                while let Ok(analysis_item) = receiver.recv() {
                    Scanner::process_item_async(
                        &db,
                        &scan_copy,
                        analysis_item,
                        analysis_prog,
                        thread_prog,
                        &reporter_clone,
                        &cancel_token_clone,
                        hash_func,
                    );
                }
                reporter_clone.finish_and_clear(thread_prog);
            });
        }

        let mut last_item_id = 0;

        loop {
            if cancel_token.load(Ordering::Acquire) {
                break;
            }

            let analysis_items = Item::fetch_next_analysis_batch(
                &db.lock().unwrap(),
                scan.scan_id(),
                scan.analysis_spec(),
                last_item_id,
                100,
            )?;

            if analysis_items.is_empty() {
                break;
            }

            for analysis_item in analysis_items {
                // Items will be ordered by id. We keep track of the last seen id and provide
                // it in calls to fetch_next_analysis_batch to avoid picking up unprocessed
                // items that we've already picked up. This avoids a race condition
                // in which we'd pick up unprocessed items that are currently being processed
                last_item_id = analysis_item.item_id();

                // This send will block if the channel already has 100 items.
                sender
                    .send(analysis_item)
                    .expect("Failed to send task into the bounded channel");
            }
        }

        // Drop the sender to signal the workers that no more items will come.
        drop(sender);

        // Wait for all tasks to complete.
        pool.join();

        // It is critical that we check for completion and return the cancellation error
        // without marking the scan completed. Once the scan is marked completed, attempting to
        // "stop" the scan will be a no-op and the scan will remain in a completed state.
        // Because we may have detected the cancellation and aborted or never started
        // some hashing or validation operations, we have to be careful to not allow it to
        // appear complete
        if cancel_token.load(Ordering::Acquire) {
            return Err(FsPulseError::ScanCancelled);
        }

        reporter.finish_and_clear(analysis_prog);
        scan.set_state_completed(&mut db.lock().unwrap())
    }

    #[allow(clippy::too_many_arguments)]
    fn process_item_async(
        db: &Arc<Mutex<Database>>,
        scan: &Scan,
        analysis_item: AnalysisItem,
        analysis_prog_id: ProgressId,
        thread_prog_id: ProgressId,
        reporter: &Arc<dyn ProgressReporter>,
        cancel_token: &Arc<AtomicBool>,
        hash_func: HashFunc,
    ) {
        // TODO: Improve the error handling for all analysis. Need to differentiate
        // between file system errors and actual content errors

        // This function is the entry point for each worker thread to process an item.
        // It performs hashing and/or validation as needed and updates the database.
        // It does not return errors, but it does need to check for cancellation.
        // If cancellation is detected, it should exit promptly without updating
        // the database. The hashing and validation processes exit when detecting
        // cancellation and may return a cancellation error, but we ignore that here.
        // The calling code will always check for cancellation and will ensure that
        // the scan operation is rolled back to the "stoped" stated which means that
        // it doesn't matter what state is written to the database for this particular item

        let path = Path::new(analysis_item.item_path());

        info!("Beginning analysis of: {path:?}");

        let display_path = path
            .file_name()
            .unwrap_or(path.as_os_str())
            .to_string_lossy();

        let mut new_hash = None;

        if analysis_item.needs_hash() {
            reporter.update_work(
                thread_prog_id,
                WorkUpdate::Hashing {
                    file: display_path.to_string(),
                },
            );
            reporter.set_position(thread_prog_id, 0); // reset in case left from previous
            reporter.set_length(thread_prog_id, 0);

            if cancel_token.load(Ordering::Acquire) {
                info!("Cancellation detected before hashing: {path:?}");
                return;
            }

            new_hash =
                match Hash::compute_hash(path, thread_prog_id, reporter, hash_func, cancel_token) {
                    Ok(hash_s) => Some(hash_s),
                    Err(error) => {
                        error!("Error hashing '{}': {}", &display_path, error);
                        // If hashing fails, we set the hash to the error string
                        // This isn't great, but it allows us to have a string value when stopping a scan
                        // and leaves an error artifact behind for investigation
                        Some(error.to_string())
                    }
                };
        }

        let mut new_val = ValidationState::Unknown;
        let mut new_val_error = None;

        if analysis_item.needs_val() {
            if cancel_token.load(Ordering::Acquire) {
                info!("Cancellation detected before validation: {path:?}");
                return;
            }

            let validator = from_path(path);
            match validator {
                Some(v) => {
                    reporter.update_work(
                        thread_prog_id,
                        WorkUpdate::Validating {
                            file: display_path.to_string(),
                        },
                    );
                    let steady_tick = v.wants_steady_tick();

                    if steady_tick {
                        reporter.enable_steady_tick(thread_prog_id, Duration::from_millis(250));
                    }
                    match v.validate(path, thread_prog_id, reporter, cancel_token) {
                        Ok((res_validity_state, res_validation_error)) => {
                            new_val = res_validity_state;
                            new_val_error = res_validation_error;
                        }
                        Err(e) => {
                            let e_str = e.to_string();
                            error!("Error validating '{}': {}", &display_path, e_str);
                            new_val = ValidationState::Invalid;
                            new_val_error = Some(e_str);
                        }
                    }
                    if steady_tick {
                        reporter.disable_steady_tick(thread_prog_id);
                    }
                }
                None => new_val = ValidationState::NoValidator,
            }
        }

        if let Err(error) = Scanner::update_item_analysis(
            db,
            scan,
            &analysis_item,
            new_hash,
            new_val,
            new_val_error,
        ) {
            let e_str = error.to_string();
            error!(
                "Error updating item analysis '{}': {}",
                &display_path, e_str
            );
        }

        reporter.inc(analysis_prog_id, 1);

        // Set thread back to idle after completing work
        reporter.update_work(thread_prog_id, WorkUpdate::Idle);

        info!("Done analyzing: {path:?}");
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
                db.immediate_transaction(|conn| {
                    let rows_updated = conn.execute(
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
                            ValidationState::Unknown.as_i64(),
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

                    conn.execute(
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
                            ChangeType::Add.as_i64(),
                            existing_item.mod_date(),
                            mod_date,
                            existing_item.file_size(),
                            file_size,
                            existing_item.file_hash(),
                            existing_item.validity_state_as_str(),
                            existing_item.val_error(),
                        ),
                    )?;

                    Ok(())
                })?;
            } else if meta_change {
                db.immediate_transaction(|conn| {
                    let rows_updated = conn.execute(
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
                    conn.execute(
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
                            ChangeType::Modify.as_i64(),
                            meta_change.then_some(existing_item.mod_date()),
                            meta_change.then_some(mod_date),
                            meta_change.then_some(existing_item.file_size()),
                            meta_change.then_some(file_size),
                        ),
                    )?;

                    Ok(())
                })?;
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
            db.immediate_transaction(|conn| {
                conn.execute("INSERT INTO items (root_id, item_path, item_type, mod_date, file_size, val, last_scan) VALUES (?, ?, ?, ?, ?, ?, ?)",
                    (scan.root_id(), &path_str, item_type.as_i64(), mod_date, file_size, ValidationState::Unknown.as_i64(), scan.scan_id()))?;

                let item_id: i64 = conn.query_row("SELECT last_insert_rowid()", [], |row| row.get(0))?;

                conn.execute("INSERT INTO changes (scan_id, item_id, change_type, is_undelete, mod_date_new, file_size_new, hash_change, val_change) VALUES (?, ?, ?, 0, ?, ?, 0, 0)",
                    (scan.scan_id(), item_id, ChangeType::Add.as_i64(), mod_date, file_size))?;

                Ok(())
            })?;
        }

        Ok(())
    }

    pub fn update_item_analysis(
        db: &Arc<Mutex<Database>>,
        scan: &Scan,
        analysis_item: &AnalysisItem,
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
        let mut i_hash = analysis_item.file_hash();
        let mut i_val = analysis_item.val();
        let mut i_val_error = analysis_item.val_error();
        let mut i_last_hash_scan = analysis_item.last_hash_scan();
        let mut i_last_val_scan = analysis_item.last_val_scan();

        let mut alert_possible_hash = false;
        let mut alert_invalid_item = false;

        if analysis_item.needs_hash() {
            if analysis_item.file_hash() != new_hash.as_deref() {
                // if either the hash or validation state changes, we update changes
                update_changes = true;
                c_hash_change = Some(true);
                c_last_hash_scan_old = analysis_item.last_hash_scan();
                c_hash_old = analysis_item.file_hash();
                c_hash_new = new_hash.as_deref();

                i_hash = new_hash.as_deref();

                // The hash changed. Assess whether it's suspicious or not
                // It's only suspicious if metadata (file size and mod date)
                // didn't change in this update or any update since that last
                // hash scan
                alert_possible_hash = match analysis_item.meta_change() {
                    Some(true) => false,
                    Some(false) | None => true,
                }
            }

            // Update the last scan id whether or not anything changed
            i_last_hash_scan = Some(scan.scan_id());
        }

        if analysis_item.needs_val() {
            if (analysis_item.val() != new_val)
                || (analysis_item.val_error() != new_val_error.as_deref())
            {
                if new_val == ValidationState::Invalid {
                    // if we're here, then either the previous validation
                    // state wasn't Invalid or it was invalid but the
                    // error message changed. In both fo these cases,
                    // we should alert
                    alert_invalid_item = true;
                }

                // if either the hash or validation state changes, we update changes
                update_changes = true;
                c_val_change = Some(true);
                c_last_val_scan_old = analysis_item.last_val_scan();
                c_val_old = Some(analysis_item.val().as_i64());
                c_val_new = Some(new_val.as_i64());
                c_val_error_old = analysis_item.val_error();
                c_val_error_new = new_val_error.as_deref();

                // always update the item when the validation state changes
                i_val = new_val;
                i_val_error = new_val_error.as_deref();
            }

            // update the last validation scan id whether or not anything changed
            i_last_val_scan = Some(scan.scan_id());
        }

        let db_guard = db.lock().unwrap();

        // Use IMMEDIATE transaction for read-then-write pattern
        db_guard.immediate_transaction(|conn| {
            if alert_possible_hash {
                if let Some(last_hash_scan) = analysis_item.last_hash_scan() {
                    if !Alerts::meta_changed_between(
                        conn,
                        analysis_item.item_id(),
                        last_hash_scan,
                        scan.scan_id(),
                    )? {
                        Alerts::add_suspicious_hash_alert(
                            conn,
                            scan.scan_id(),
                            analysis_item.item_id(),
                            analysis_item.last_hash_scan(),
                            analysis_item.file_hash(),
                            c_hash_new.unwrap(),
                        )?;
                    }
                }
            }

            if alert_invalid_item {
                Alerts::add_invalid_item_alert(
                    conn,
                    scan.scan_id(),
                    analysis_item.item_id(),
                    c_val_error_new.unwrap(),
                )?;
            }

            // Step 1: UPSERT into `changes` table if the change is something other than moving from the default state
            if update_changes {
                conn.execute(
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
                        VALUES (?, ?, 2, 0, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
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
                        analysis_item.item_id(),
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
            conn.execute(
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
                    i_val.as_i64(),
                    i_val_error,
                    i_last_hash_scan,
                    i_last_val_scan,
                    analysis_item.item_id()
                ],
            )?;

            Ok(())
        })?;

        Ok(())
    }
}
