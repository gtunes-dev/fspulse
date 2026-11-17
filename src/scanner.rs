//! Integrity scanner — database-driven hash and validation task.
//!
//! Runs against items that checkpoint has already written to the
//! database. Does NOT walk the filesystem to discover items.
//!
//! Hashes and/or validates eligible items, then computes aggregate
//! integrity statistics and writes them to the `scans` row as part
//! of setting the scan to Completed.

use crate::roots::Root;
use crate::scans::ScanState;
use crate::task::TaskProgress;
use crate::{db::Database, error::FsPulseError, scans::Scan};

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

pub struct Scanner;

impl Scanner {
    pub fn do_scan_machine(
        scan: &mut Scan,
        _root: &Root,
        task_id: i64,
        mut initial_task_state: Option<String>,
        task_progress: Arc<TaskProgress>,
        interrupt_token: Arc<AtomicBool>,
    ) -> Result<(), FsPulseError> {
        if scan.state() == ScanState::Scanning {
            task_progress.set_phase("Scanning");
            crate::integrity::analysis::run_analysis_phase(
                scan,
                task_id,
                initial_task_state.take(),
                task_progress.clone(),
                &interrupt_token,
            )?;

            Self::check_interrupted(&interrupt_token)?;

            let conn = Database::get_connection()?;
            scan.set_state_completed(&conn)?;
        }

        Ok(())
    }

    fn check_interrupted(interrupt_token: &Arc<AtomicBool>) -> Result<(), FsPulseError> {
        if interrupt_token.load(Ordering::Acquire) {
            Err(FsPulseError::TaskInterrupted)
        } else {
            Ok(())
        }
    }
}
