use std::io::ErrorKind;
use std::path::Path;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use rusqlite::{params, Connection};

use crate::error::FsPulseError;
use super::analysis::AnalysisItem;
use crate::scans::Scan;
use crate::validate::validator::{self, ValidationState};

use super::analysis::ValAnalysisError;
use super::val_version::ValState;

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

/// Persist validation results to `item_versions`.
///
/// Validation is tightly coupled to the item_version. The val_scan_id, val_state,
/// and val_error columns are set on the current version. Validation is a one-time
/// operation per version — there is no "extend" like hash.
pub fn persist_val(
    conn: &Connection,
    scan: &Scan,
    analysis_item: &AnalysisItem,
    new_val: ValidationState,
    new_val_error: Option<&str>,
) -> Result<(), FsPulseError> {
    let new_val_state = match ValState::from_validation_state(new_val) {
        Some(s) => s,
        None => return Ok(()), // NoValidator/Unknown — nothing to persist
    };

    // Write val state directly onto the item_version row
    conn.execute(
        "UPDATE item_versions SET val_scan_id = ?, val_state = ?, val_error = ?
         WHERE version_id = ?",
        params![scan.scan_id(), new_val_state.as_i64(), new_val_error, analysis_item.version_id()],
    )?;

    Ok(())
}
