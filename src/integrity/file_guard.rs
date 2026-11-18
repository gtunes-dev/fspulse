use std::path::Path;

use log::debug;

/// A file whose modification time is this close to the present may
/// still be written to without its recorded metadata changing: a second
/// write in the same clock second leaves the same `mod_date`, and can
/// leave the same size. Such a file is given this long to settle before
/// it is read.
pub const SETTLE_SECS: i64 = 2;

/// How a file on disk compares with the version recorded for it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileCheck {
    /// Size and modification time match the record, and the file was
    /// not modified within the last `SETTLE_SECS`.
    Unchanged,
    /// Matches the record, but was modified so recently that a write in
    /// progress could not be told apart from a finished one.
    Settling,
    /// Differs from the record, or cannot be read. The record is stale:
    /// the change belongs to the observation pipeline, and nothing
    /// computed from the file may be attached to this version.
    Changed,
}

/// Compare a file's metadata with the values recorded for the version
/// about to be hashed or validated.
///
/// Called before the file is read and again after: a hash is only
/// attached to a version if the file matched it at both ends of the
/// read. Together with `Settling` this guarantees the content read is
/// the content the version describes, short of a writer that restores
/// the modification time.
pub fn check_file(
    path: &Path,
    expected_mod_date: Option<i64>,
    expected_size: Option<i64>,
    now: i64,
) -> FileCheck {
    let metadata = match path.metadata() {
        Ok(m) => m,
        Err(_) => {
            debug!("file_guard: cannot stat {:?}", path);
            return FileCheck::Changed;
        }
    };

    if let Some(expected) = expected_size {
        let actual = metadata.len() as i64;
        if actual != expected {
            debug!(
                "file_guard: size changed for {:?} (expected {}, got {})",
                path, expected, actual
            );
            return FileCheck::Changed;
        }
    }

    let actual_mod_date = match metadata.modified() {
        Ok(modified) => modified
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0),
        Err(_) => {
            debug!("file_guard: cannot read mod_date for {:?}", path);
            return FileCheck::Changed;
        }
    };

    if let Some(expected) = expected_mod_date {
        if actual_mod_date != expected {
            debug!(
                "file_guard: mod_date changed for {:?} (expected {}, got {})",
                path, expected, actual_mod_date
            );
            return FileCheck::Changed;
        }
    }

    // A modification time in the future (clock skew, a network mount)
    // says nothing about whether the file is being written; only one
    // near the present does.
    if (now - actual_mod_date).abs() < SETTLE_SECS {
        return FileCheck::Settling;
    }

    FileCheck::Unchanged
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    fn mod_date(path: &Path) -> i64 {
        fs::metadata(path)
            .unwrap()
            .modified()
            .unwrap()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64
    }

    fn set_mod_date(path: &Path, secs: i64) {
        let file = fs::File::options().write(true).open(path).unwrap();
        file.set_modified(UNIX_EPOCH + Duration::from_secs(secs as u64))
            .unwrap();
    }

    fn now() -> i64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64
    }

    #[test]
    fn a_file_matching_its_record_and_not_recently_written_is_unchanged() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.flac");
        fs::write(&path, "music").unwrap();
        set_mod_date(&path, now() - 60);

        let check = check_file(&path, Some(mod_date(&path)), Some(5), now());
        assert_eq!(check, FileCheck::Unchanged);
    }

    #[test]
    fn a_file_written_this_second_is_settling_even_though_it_matches() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.flac");
        fs::write(&path, "music").unwrap();

        let recorded = mod_date(&path);
        assert_eq!(
            check_file(&path, Some(recorded), Some(5), recorded),
            FileCheck::Settling
        );
        // The same file, looked at once the settle time has passed.
        assert_eq!(
            check_file(&path, Some(recorded), Some(5), recorded + SETTLE_SECS),
            FileCheck::Unchanged
        );
    }

    #[test]
    fn a_different_size_or_time_or_a_missing_file_is_changed() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.flac");
        fs::write(&path, "music").unwrap();
        set_mod_date(&path, now() - 60);
        let recorded = mod_date(&path);

        assert_eq!(
            check_file(&path, Some(recorded), Some(4), now()),
            FileCheck::Changed
        );
        assert_eq!(
            check_file(&path, Some(recorded - 1), Some(5), now()),
            FileCheck::Changed
        );
        assert_eq!(
            check_file(&dir.path().join("gone"), Some(recorded), Some(5), now()),
            FileCheck::Changed
        );
    }

    #[test]
    fn a_modification_time_far_in_the_future_does_not_block_the_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.flac");
        fs::write(&path, "music").unwrap();
        set_mod_date(&path, now() + 86_400);

        let check = check_file(&path, Some(mod_date(&path)), Some(5), now());
        assert_eq!(check, FileCheck::Unchanged);
    }
}
