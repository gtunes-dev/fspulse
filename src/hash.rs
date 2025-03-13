use std::{fs::File, io::{BufReader, Read}, path::PathBuf};

use hex::encode;
use indicatif::ProgressBar;
use md5::{Digest, Md5};

use crate::error::FsPulseError;


pub struct Hash {
    // no fields
}

impl Hash {
    pub fn compute_md5_hash(path: &PathBuf, bar: &ProgressBar) -> Result<String, FsPulseError> {
        let file_name = path.file_name()
            .unwrap_or_else(|| path.as_os_str())
            .to_string_lossy();

        let f = File::open(path)?;
        let len = f.metadata()?.len();

        bar.reset();
        bar.set_length(len);
        bar.set_message(format!("Computing hash for: {}", file_name));

        let mut reader = BufReader::new(f);
        let mut hasher = Md5::new();
        let mut buffer = [0; 8192]; // Read in 8KB chunks

        loop {
            let bytes_read = reader.read(&mut buffer)?;
            if bytes_read == 0 {
                break;
            }
            hasher.update(&buffer[..bytes_read]);
            bar.inc(bytes_read.try_into().unwrap());
        }

        let hash = hasher.finalize();

        bar.finish_and_clear();
        Ok(encode(hash))
    }
}