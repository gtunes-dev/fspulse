use std::io::ErrorKind;
use std::path::Path;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use rusqlite::Connection;

use crate::error::FsPulseError;
use super::analysis::AnalysisItem;
use crate::scans::Scan;
use crate::undo_log::UndoLog;
use crate::validate::validator::{self, ValidationState};

use super::analysis::ValAnalysisError;
use super::val_version::{ValState, ValVersion};

/// Run validation on a file, returning the result or an error category.
pub fn run_validation(
    path: &Path,
    interrupt_token: &Arc<AtomicBool>,
) -> Result<(ValidationState, Option<String>), ValAnalysisError> {
    let validator = validator::from_path(path);
    match validator {
        Some(v) => {
            match v.validate(path, interrupt_token) {
                Ok((state, err)) => Ok((state, err)),
                Err(FsPulseError::IoError(ref io_err))
                    if io_err.kind() == ErrorKind::PermissionDenied =>
                {
                    Err(ValAnalysisError::PermissionDenied)
                }
                Err(FsPulseError::IoError(ref io_err))
                    if io_err.kind() == ErrorKind::NotFound =>
                {
                    Err(ValAnalysisError::NotFound)
                }
                Err(e) => {
                    Err(ValAnalysisError::ValidationError(e.to_string()))
                }
            }
        }
        None => {
            // Should not reach here — has_validator check in analysis.rs
            // prevents calling run_validation for files without a validator.
            log::warn!("run_validation called for file without validator: {:?}", path);
            Err(ValAnalysisError::NoValidator)
        }
    }
}

/// Check if validation state has changed from the current analysis item's state.
pub fn is_val_changed(
    analysis_item: &AnalysisItem,
    new_val: ValidationState,
    new_val_error: Option<&str>,
) -> bool {
    if !analysis_item.needs_val() {
        return false;
    }
    let new_val_state = match ValState::from_validation_state(new_val) {
        Some(s) => s,
        None => return false, // NoValidator/Unknown — should not reach here
    };
    let old_val_state = analysis_item.val_state().and_then(ValState::from_validation_state);
    old_val_state != Some(new_val_state)
        || analysis_item.val_error() != new_val_error
}

/// Persist validation results to `val_versions`.
///
/// If the val state changed (or is new), inserts a new row. If unchanged,
/// extends `last_scan_id` on the existing row.
pub fn persist_val(
    conn: &Connection,
    scan: &Scan,
    analysis_item: &AnalysisItem,
    new_val: ValidationState,
    new_val_error: Option<&str>,
    val_state_changed: bool,
) -> Result<(), FsPulseError> {
    let new_val_state = match ValState::from_validation_state(new_val) {
        Some(s) => s,
        None => return Ok(()), // NoValidator/Unknown — nothing to persist
    };

    if val_state_changed {
        // Val changed (or first val) — insert new val_version row
        ValVersion::insert(
            conn,
            analysis_item.item_id(),
            scan.scan_id(),
            new_val_state,
            new_val_error,
        )?;
    } else if let Some(first_scan_id) = analysis_item.val_first_scan_id() {
        // Val unchanged — extend the existing val_version's last_scan_id
        let current_vv = ValVersion::get_current(conn, analysis_item.item_id())?;
        if let Some(vv) = current_vv {
            UndoLog::log_val_version_extend(
                conn, analysis_item.item_id(), first_scan_id, vv.last_scan_id(),
            )?;
        }
        ValVersion::extend_last_scan(
            conn, analysis_item.item_id(), first_scan_id, scan.scan_id(),
        )?;
    }

    Ok(())
}
