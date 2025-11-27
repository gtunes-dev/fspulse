use std::{
    fs::File,
    io::Read,
    path::Path,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};

use hex::encode;
use sha2::{Digest, Sha256};

use crate::error::FsPulseError;

pub struct Hash;

impl Hash {
    pub fn compute_sha2_256_hash(
        path: &Path,
        interrupt_token: &Arc<AtomicBool>,
    ) -> Result<String, FsPulseError> {
        // Check for interrupt before doing any work
        if interrupt_token.load(Ordering::Acquire) {
            return Err(FsPulseError::ScanInterrupted);
        }

        let mut f = File::open(path)?;

        let mut hasher = Sha256::new();
        let mut buffer = [0; 131_072]; // Read in 128KB chunks

        let mut loop_counter = 0;

        loop {
            loop_counter += 1;
            // Every 16 loops, check for interrupt
            if loop_counter % 16 == 0 && interrupt_token.load(Ordering::Acquire) {
                return Err(FsPulseError::ScanInterrupted);
            }

            let bytes_read = f.read(&mut buffer)?;
            if bytes_read == 0 {
                break;
            }
            hasher.update(&buffer[..bytes_read]);
        }

        let hash = hasher.finalize();

        Ok(encode(hash))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_compute_sha2_256_hash_empty_file() {
        let mut temp_file = NamedTempFile::new().expect("Failed to create temp file");
        temp_file
            .write_all(b"")
            .expect("Failed to write to temp file");

        let interrupt_token = Arc::new(AtomicBool::new(false));
        let result = Hash::compute_sha2_256_hash(temp_file.path(), &interrupt_token);

        assert!(result.is_ok());
        let hash = result.unwrap();
        // SHA256 of empty string is e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855
        assert_eq!(
            hash,
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn test_compute_sha2_256_hash_known_content() {
        let mut temp_file = NamedTempFile::new().expect("Failed to create temp file");
        temp_file
            .write_all(b"hello world")
            .expect("Failed to write to temp file");

        let interrupt_token = Arc::new(AtomicBool::new(false));
        let result = Hash::compute_sha2_256_hash(temp_file.path(), &interrupt_token);

        assert!(result.is_ok());
        let hash = result.unwrap();
        // SHA256 of "hello world" is b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9
        assert_eq!(
            hash,
            "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9"
        );
    }

    #[test]
    fn test_compute_hash_with_sha2_func() {
        let mut temp_file = NamedTempFile::new().expect("Failed to create temp file");
        temp_file
            .write_all(b"test")
            .expect("Failed to write to temp file");

        let interrupt_token = Arc::new(AtomicBool::new(false));
        let result = Hash::compute_sha2_256_hash(temp_file.path(), &interrupt_token);

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
        let interrupt_token = Arc::new(AtomicBool::new(false));

        let result = Hash::compute_sha2_256_hash(nonexistent_path, &interrupt_token);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), FsPulseError::IoError(_)));
    }

    #[test]
    fn test_compute_sha2_256_hash_interrupt() {
        // Create a large file to ensure interrrupt check is triggered
        // Need at least 256 * 8KB = 2MB to trigger the interrupt check
        let mut temp_file = NamedTempFile::new().expect("Failed to create temp file");
        let large_data = vec![0u8; 3_000_000]; // 3MB
        temp_file
            .write_all(&large_data)
            .expect("Failed to write to temp file");

        let interrupt_token = Arc::new(AtomicBool::new(true)); // Set to true to trigger interrupt

        let result = Hash::compute_sha2_256_hash(temp_file.path(), &interrupt_token);

        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), FsPulseError::ScanInterrupted));
    }
}
