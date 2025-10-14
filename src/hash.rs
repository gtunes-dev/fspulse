use std::{
    fs::File,
    io::{BufReader, Read},
    path::Path,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};

use hex::encode;
use md5::{Digest, Md5};
use sha2::Sha256;

use crate::{
    config::HashFunc,
    error::FsPulseError,
    progress::{ProgressId, ProgressReporter},
};

pub struct Hash;

impl Hash {
    pub fn compute_hash(
        path: &Path,
        prog_id: ProgressId,
        reporter: &Arc<dyn ProgressReporter>,
        hash_func: HashFunc,
        cancel_token: &Arc<AtomicBool>,
    ) -> Result<String, FsPulseError> {
        match hash_func {
            HashFunc::MD5 => Self::compute_md5_hash(path, prog_id, reporter, cancel_token),
            HashFunc::SHA2 => Self::compute_sha2_256_hash(path, prog_id, reporter, cancel_token),
        }
    }

    pub fn compute_sha2_256_hash(
        path: &Path,
        prog_id: ProgressId,
        reporter: &Arc<dyn ProgressReporter>,
        cancel_token: &Arc<AtomicBool>,
    ) -> Result<String, FsPulseError> {
        let f = File::open(path)?;
        let len = f.metadata()?.len();

        reporter.set_length(prog_id, len);

        let mut hasher = Sha256::new();

        let mut reader = BufReader::new(f);
        let mut buffer = [0; 8192]; // Read in 8KB chunks

        let mut loop_counter = 0;

        loop {
            loop_counter += 1;
            // Every 256 loops, check for cancellation
            if loop_counter % 256 == 0 && cancel_token.load(Ordering::Relaxed) {
                return Err(FsPulseError::ScanCancelled);
            }

            let bytes_read = reader.read(&mut buffer)?;
            if bytes_read == 0 {
                break;
            }
            hasher.update(&buffer[..bytes_read]);
            reporter.inc(prog_id, bytes_read.try_into().unwrap());
        }

        let hash = hasher.finalize();

        Ok(encode(hash))
    }

    pub fn compute_md5_hash(
        path: &Path,
        prog_id: ProgressId,
        reporter: &Arc<dyn ProgressReporter>,
        cancel_token: &Arc<AtomicBool>,
    ) -> Result<String, FsPulseError> {
        let f = File::open(path)?;
        let len = f.metadata()?.len();

        // The progress bar is mostly set up by our caller. We just need to set the
        // length and go
        reporter.set_length(prog_id, len);

        let mut reader = BufReader::new(f);
        let mut hasher = Md5::new();
        let mut buffer = [0; 8192]; // Read in 8KB chunks

        let mut loop_counter = 0;

        loop {
            loop_counter += 1;
            // Every 256 loops, check for cancellation
            if loop_counter % 256 == 0 && cancel_token.load(Ordering::Relaxed) {
                return Err(FsPulseError::ScanCancelled);
            }

            let bytes_read = reader.read(&mut buffer)?;
            if bytes_read == 0 {
                break;
            }
            hasher.update(&buffer[..bytes_read]);
            reporter.inc(prog_id, bytes_read.try_into().unwrap());
        }

        let hash = hasher.finalize();

        Ok(encode(hash))
    }

    /*
    pub fn short_md5<'a>(hash: &Option<&'a str>) -> &'a str {
        match hash {
            Some(hash) => &hash[..hash.len().min(7)],
            None => "-",
        }
    }
    */
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::progress::{ProgressConfig, ProgressReporter};
    use std::collections::HashMap;
    use std::io::Write;
    use std::sync::{Arc, Mutex};
    use tempfile::NamedTempFile;

    /// Simple mock reporter for testing that does nothing
    struct MockReporter {
        _state: Arc<Mutex<HashMap<ProgressId, u64>>>,
    }

    impl MockReporter {
        fn new() -> Self {
            Self {
                _state: Arc::new(Mutex::new(HashMap::new())),
            }
        }
    }

    impl ProgressReporter for MockReporter {
        fn section_start(&self, _stage_index: u32, _message: &str) -> ProgressId {
            ProgressId::new()
        }

        fn section_finish(&self, _id: ProgressId, _message: &str) {}

        fn create(&self, _config: ProgressConfig) -> ProgressId {
            ProgressId::new()
        }

        fn update_work(&self, _id: ProgressId, _work: crate::progress::WorkUpdate) {}

        fn set_position(&self, _id: ProgressId, _position: u64) {}

        fn set_length(&self, _id: ProgressId, _length: u64) {}

        fn inc(&self, _id: ProgressId, _delta: u64) {}

        fn enable_steady_tick(&self, _id: ProgressId, _interval: std::time::Duration) {}

        fn disable_steady_tick(&self, _id: ProgressId) {}

        fn finish_and_clear(&self, _id: ProgressId) {}

        fn println(&self, _message: &str) -> Result<(), Box<dyn std::error::Error>> {
            Ok(())
        }

        fn clone_reporter(&self) -> Arc<dyn ProgressReporter> {
            Arc::new(Self::new())
        }
    }

    fn create_test_reporter() -> Arc<dyn ProgressReporter> {
        Arc::new(MockReporter::new())
    }

    #[test]
    fn test_compute_md5_hash_empty_file() {
        let mut temp_file = NamedTempFile::new().expect("Failed to create temp file");
        temp_file
            .write_all(b"")
            .expect("Failed to write to temp file");

        let reporter = create_test_reporter();
        let prog_id = ProgressId::new();
        let cancel_token = Arc::new(AtomicBool::new(false));
        let result = Hash::compute_md5_hash(temp_file.path(), prog_id, &reporter, &cancel_token);

        assert!(result.is_ok());
        let hash = result.unwrap();
        // MD5 of empty string is d41d8cd98f00b204e9800998ecf8427e
        assert_eq!(hash, "d41d8cd98f00b204e9800998ecf8427e");
    }

    #[test]
    fn test_compute_sha2_256_hash_empty_file() {
        let mut temp_file = NamedTempFile::new().expect("Failed to create temp file");
        temp_file
            .write_all(b"")
            .expect("Failed to write to temp file");

        let reporter = create_test_reporter();
        let prog_id = ProgressId::new();
        let cancel_token = Arc::new(AtomicBool::new(false));
        let result =
            Hash::compute_sha2_256_hash(temp_file.path(), prog_id, &reporter, &cancel_token);

        assert!(result.is_ok());
        let hash = result.unwrap();
        // SHA256 of empty string is e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855
        assert_eq!(
            hash,
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn test_compute_md5_hash_known_content() {
        let mut temp_file = NamedTempFile::new().expect("Failed to create temp file");
        temp_file
            .write_all(b"hello world")
            .expect("Failed to write to temp file");

        let reporter = create_test_reporter();
        let prog_id = ProgressId::new();
        let cancel_token = Arc::new(AtomicBool::new(false));
        let result = Hash::compute_md5_hash(temp_file.path(), prog_id, &reporter, &cancel_token);

        assert!(result.is_ok());
        let hash = result.unwrap();
        // MD5 of "hello world" is 5eb63bbbe01eeed093cb22bb8f5acdc3
        assert_eq!(hash, "5eb63bbbe01eeed093cb22bb8f5acdc3");
    }

    #[test]
    fn test_compute_sha2_256_hash_known_content() {
        let mut temp_file = NamedTempFile::new().expect("Failed to create temp file");
        temp_file
            .write_all(b"hello world")
            .expect("Failed to write to temp file");

        let reporter = create_test_reporter();
        let prog_id = ProgressId::new();
        let cancel_token = Arc::new(AtomicBool::new(false));
        let result =
            Hash::compute_sha2_256_hash(temp_file.path(), prog_id, &reporter, &cancel_token);

        assert!(result.is_ok());
        let hash = result.unwrap();
        // SHA256 of "hello world" is b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9
        assert_eq!(
            hash,
            "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9"
        );
    }

    #[test]
    fn test_compute_hash_with_md5_func() {
        let mut temp_file = NamedTempFile::new().expect("Failed to create temp file");
        temp_file
            .write_all(b"test")
            .expect("Failed to write to temp file");

        let reporter = create_test_reporter();
        let prog_id = ProgressId::new();
        let cancel_token = Arc::new(AtomicBool::new(false));
        let result = Hash::compute_hash(
            temp_file.path(),
            prog_id,
            &reporter,
            HashFunc::MD5,
            &cancel_token,
        );

        assert!(result.is_ok());
        let hash = result.unwrap();
        // MD5 of "test" is 098f6bcd4621d373cade4e832627b4f6
        assert_eq!(hash, "098f6bcd4621d373cade4e832627b4f6");
    }

    #[test]
    fn test_compute_hash_with_sha2_func() {
        let mut temp_file = NamedTempFile::new().expect("Failed to create temp file");
        temp_file
            .write_all(b"test")
            .expect("Failed to write to temp file");

        let reporter = create_test_reporter();
        let prog_id = ProgressId::new();
        let cancel_token = Arc::new(AtomicBool::new(false));
        let result = Hash::compute_hash(
            temp_file.path(),
            prog_id,
            &reporter,
            HashFunc::SHA2,
            &cancel_token,
        );

        assert!(result.is_ok());
        let hash = result.unwrap();
        // SHA256 of "test" is 9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08
        assert_eq!(
            hash,
            "9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08"
        );
    }

    #[test]
    fn test_compute_hash_nonexistent_file() {
        let nonexistent_path = std::path::Path::new("/this/path/does/not/exist.txt");
        let reporter = create_test_reporter();
        let prog_id = ProgressId::new();
        let cancel_token = Arc::new(AtomicBool::new(false));

        let result = Hash::compute_hash(
            nonexistent_path,
            prog_id,
            &reporter,
            HashFunc::MD5,
            &cancel_token,
        );
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), FsPulseError::IoError(_)));
    }

    #[test]
    fn test_compute_md5_hash_cancellation() {
        // Create a large file to ensure cancellation check is triggered
        // Need at least 256 * 8KB = 2MB to trigger the cancellation check
        let mut temp_file = NamedTempFile::new().expect("Failed to create temp file");
        let large_data = vec![0u8; 3_000_000]; // 3MB
        temp_file
            .write_all(&large_data)
            .expect("Failed to write to temp file");

        let reporter = create_test_reporter();
        let prog_id = ProgressId::new();
        let cancel_token = Arc::new(AtomicBool::new(true)); // Set to true to trigger cancellation

        let result = Hash::compute_md5_hash(temp_file.path(), prog_id, &reporter, &cancel_token);

        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), FsPulseError::ScanCancelled));
    }

    #[test]
    fn test_compute_sha2_256_hash_cancellation() {
        // Create a large file to ensure cancellation check is triggered
        // Need at least 256 * 8KB = 2MB to trigger the cancellation check
        let mut temp_file = NamedTempFile::new().expect("Failed to create temp file");
        let large_data = vec![0u8; 3_000_000]; // 3MB
        temp_file
            .write_all(&large_data)
            .expect("Failed to write to temp file");

        let reporter = create_test_reporter();
        let prog_id = ProgressId::new();
        let cancel_token = Arc::new(AtomicBool::new(true)); // Set to true to trigger cancellation

        let result =
            Hash::compute_sha2_256_hash(temp_file.path(), prog_id, &reporter, &cancel_token);

        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), FsPulseError::ScanCancelled));
    }
}
