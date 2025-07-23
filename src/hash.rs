use std::{
    fs::File,
    io::{BufReader, Read},
    path::Path,
};

use hex::encode;
use indicatif::ProgressBar;
use md5::{Digest, Md5};
use sha2::Sha256;

use crate::{config::HashFunc, error::FsPulseError};

pub struct Hash;

impl Hash {
    pub fn compute_hash(path: &Path, hash_prog: &ProgressBar, hash_func: HashFunc) -> Result<String, FsPulseError> {
        match hash_func {
            HashFunc::MD5 => Self::compute_md5_hash(path, hash_prog),
            HashFunc::SHA2 => Self::compute_sha2_256_hash(path, hash_prog),
        }
    }

    pub fn compute_sha2_256_hash(
        path: &Path,
        hash_prog: &ProgressBar,
    ) -> Result<String, FsPulseError> {
        let f = File::open(path)?;
        let len = f.metadata()?.len();
        

        hash_prog.set_length(len);

        let mut hasher = Sha256::new();

        let mut reader = BufReader::new(f);
        let mut buffer = [0; 8192]; // Read in 8KB chunks

        loop {
            let bytes_read = reader.read(&mut buffer)?;
            if bytes_read == 0 {
                break;
            }
            hasher.update(&buffer[..bytes_read]);
            hash_prog.inc(bytes_read.try_into().unwrap());
        }
        let hash = hasher.finalize();

        Ok(encode(hash))
        
    }

    pub fn compute_md5_hash(path: &Path, hash_prog: &ProgressBar) -> Result<String, FsPulseError> {
        let f = File::open(path)?;
        let len = f.metadata()?.len();

        // The progress bar is mostly set up by our caller. We just need to set the
        // length and go
        hash_prog.set_length(len);

        let mut reader = BufReader::new(f);
        let mut hasher = Md5::new();
        let mut buffer = [0; 8192]; // Read in 8KB chunks

        loop {
            let bytes_read = reader.read(&mut buffer)?;
            if bytes_read == 0 {
                break;
            }
            hasher.update(&buffer[..bytes_read]);
            hash_prog.inc(bytes_read.try_into().unwrap());
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
    use indicatif::ProgressBar;
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn create_test_progress_bar() -> ProgressBar {
        ProgressBar::hidden()
    }

    #[test]
    fn test_compute_md5_hash_empty_file() {
        let mut temp_file = NamedTempFile::new().expect("Failed to create temp file");
        temp_file.write_all(b"").expect("Failed to write to temp file");
        
        let progress_bar = create_test_progress_bar();
        let result = Hash::compute_md5_hash(temp_file.path(), &progress_bar);
        
        assert!(result.is_ok());
        let hash = result.unwrap();
        // MD5 of empty string is d41d8cd98f00b204e9800998ecf8427e
        assert_eq!(hash, "d41d8cd98f00b204e9800998ecf8427e");
    }

    #[test]
    fn test_compute_sha2_256_hash_empty_file() {
        let mut temp_file = NamedTempFile::new().expect("Failed to create temp file");
        temp_file.write_all(b"").expect("Failed to write to temp file");
        
        let progress_bar = create_test_progress_bar();
        let result = Hash::compute_sha2_256_hash(temp_file.path(), &progress_bar);
        
        assert!(result.is_ok());
        let hash = result.unwrap();
        // SHA256 of empty string is e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855
        assert_eq!(hash, "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855");
    }

    #[test]
    fn test_compute_md5_hash_known_content() {
        let mut temp_file = NamedTempFile::new().expect("Failed to create temp file");
        temp_file.write_all(b"hello world").expect("Failed to write to temp file");
        
        let progress_bar = create_test_progress_bar();
        let result = Hash::compute_md5_hash(temp_file.path(), &progress_bar);
        
        assert!(result.is_ok());
        let hash = result.unwrap();
        // MD5 of "hello world" is 5eb63bbbe01eeed093cb22bb8f5acdc3
        assert_eq!(hash, "5eb63bbbe01eeed093cb22bb8f5acdc3");
    }

    #[test]
    fn test_compute_sha2_256_hash_known_content() {
        let mut temp_file = NamedTempFile::new().expect("Failed to create temp file");
        temp_file.write_all(b"hello world").expect("Failed to write to temp file");
        
        let progress_bar = create_test_progress_bar();
        let result = Hash::compute_sha2_256_hash(temp_file.path(), &progress_bar);
        
        assert!(result.is_ok());
        let hash = result.unwrap();
        // SHA256 of "hello world" is b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9
        assert_eq!(hash, "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9");
    }

    #[test]
    fn test_compute_hash_with_md5_func() {
        let mut temp_file = NamedTempFile::new().expect("Failed to create temp file");
        temp_file.write_all(b"test").expect("Failed to write to temp file");
        
        let progress_bar = create_test_progress_bar();
        let result = Hash::compute_hash(temp_file.path(), &progress_bar, HashFunc::MD5);
        
        assert!(result.is_ok());
        let hash = result.unwrap();
        // MD5 of "test" is 098f6bcd4621d373cade4e832627b4f6
        assert_eq!(hash, "098f6bcd4621d373cade4e832627b4f6");
    }

    #[test]
    fn test_compute_hash_with_sha2_func() {
        let mut temp_file = NamedTempFile::new().expect("Failed to create temp file");
        temp_file.write_all(b"test").expect("Failed to write to temp file");
        
        let progress_bar = create_test_progress_bar();
        let result = Hash::compute_hash(temp_file.path(), &progress_bar, HashFunc::SHA2);
        
        assert!(result.is_ok());
        let hash = result.unwrap();
        // SHA256 of "test" is 9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08
        assert_eq!(hash, "9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08");
    }

    #[test]
    fn test_compute_hash_nonexistent_file() {
        let nonexistent_path = std::path::Path::new("/this/path/does/not/exist.txt");
        let progress_bar = create_test_progress_bar();
        
        let result = Hash::compute_hash(nonexistent_path, &progress_bar, HashFunc::MD5);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), FsPulseError::IoError(_)));
    }
}
