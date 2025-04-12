use std::{
    fs::File,
    io::{BufReader, Read},
    path::Path,
};

use hex::encode;
use indicatif::ProgressBar;
use md5::{Digest, Md5};

use crate::error::FsPulseError;

pub struct Hash;

impl Hash {
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

    pub fn short_md5<'a>(hash: &Option<&'a str>) -> &'a str {
        match hash {
            Some(hash) => &hash[..hash.len().min(7)],
            None => "-",
        }
    }
}
