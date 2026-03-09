use std::path::Path;

use log::debug;

/// Check whether a file's metadata still matches the expected values
/// from the walk phase.
///
/// Returns `true` if the file's size and modification time match the expected
/// values, meaning it is safe to proceed with hashing or validation.
///
/// Returns `false` if the file has changed since the walk phase observed it,
/// or if the metadata cannot be read (file deleted, permissions changed, etc.).
/// In this case, the caller should skip the hash/validation write — the file
/// will be picked up on the next scan when the metadata change is detected
/// by the walk phase.
pub fn check_file_unchanged(
    path: &Path,
    expected_mod_date: Option<i64>,
    expected_size: Option<i64>,
) -> bool {
    let metadata = match path.metadata() {
        Ok(m) => m,
        Err(_) => {
            debug!("file_guard: cannot stat {:?}, skipping", path);
            return false;
        }
    };

    // Compare size
    if let Some(expected) = expected_size {
        let actual = metadata.len() as i64;
        if actual != expected {
            debug!(
                "file_guard: size changed for {:?} (expected {}, got {})",
                path, expected, actual
            );
            return false;
        }
    }

    // Compare modification time
    if let Some(expected) = expected_mod_date {
        match metadata.modified() {
            Ok(modified) => {
                let actual = modified
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs() as i64)
                    .unwrap_or(0);
                if actual != expected {
                    debug!(
                        "file_guard: mod_date changed for {:?} (expected {}, got {})",
                        path, expected, actual
                    );
                    return false;
                }
            }
            Err(_) => {
                debug!("file_guard: cannot read mod_date for {:?}, skipping", path);
                return false;
            }
        }
    }

    true
}
