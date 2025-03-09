use std::{fs::File, io::{BufReader, Read}, path::PathBuf};

use hex::encode;
use md5::{Digest, Md5};

use crate::error::FsPulseError;


pub struct Hash {
    // no fields
}

impl Hash {
    pub fn compute_md5_hash(path: &PathBuf) -> Result<String, FsPulseError> {
        let f = File::open(path)?;
        let mut reader = BufReader::new(f);
        let mut hasher = Md5::new();
        let mut buffer = [0; 8192]; // Read in 8KB chunks

        loop {
            let bytes_read = reader.read(&mut buffer)?;
            if bytes_read == 0 {
                break;
            }
            hasher.update(&buffer[..bytes_read]);
        }

        let hash = hasher.finalize();
        Ok(encode(hash))
    }
}