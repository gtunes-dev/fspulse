use std::{fs::File, io::{BufReader, Read}, path::Path};

use hex::encode;
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use md5::{Digest, Md5};

use crate::error::FsPulseError;


pub struct Hash {
    // no fields
}

impl Hash {
    pub fn compute_md5_hash(item_path: &str, multi_prog: &mut MultiProgress) -> Result<String, FsPulseError> {
        let path = Path::new(item_path);
        let file_name = path.file_name()
            .unwrap_or_else(|| path.as_os_str())
            .to_string_lossy();

        let f = File::open(path)?;
        let len = f.metadata()?.len();

        let hash_prog = multi_prog.add(ProgressBar::new(len));

        hash_prog.set_style(ProgressStyle::default_bar()
                .template("{msg}\n[{bar:80}] {bytes}/{total_bytes} ({eta})")
                .unwrap()
                .progress_chars("#>-"));

        hash_prog.set_message(format!("Computing Hash: '{}'", file_name));

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

        hash_prog.finish_and_clear();
        Ok(encode(hash))
    }

    pub fn short_md5<'a>(hash: &Option<&'a str>) -> &'a str {
        match hash {
            Some(hash) => &hash[..hash.len().min(7)],
            None =>  "-"
        }
    }
    
}