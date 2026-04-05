// Vendored from ordpath 0.5.0 — https://github.com/yohdeadfall/ordpath/
// License: MIT OR Apache-2.0 — Author: Yoh Deadfall <yoh.deadfall@hotmail.com>

use std::io::Read;

use crate::hierarchy::ordpath::enc::{Encoding, Stage};
use crate::hierarchy::ordpath::{Error, ErrorKind};

/// The `Reader` struct allows reading ORDPATH encoded values directly from any source implementing [`Read`].
pub struct Reader<R: Read + ?Sized, E: Encoding> {
    acc: u64,
    len: u8,
    enc: E,
    src: R,
}

impl<R: Read, E: Encoding> Reader<R, E> {
    /// Creates a new `Reader` for the gives source.
    pub fn new(src: R, enc: E) -> Self {
        Self {
            acc: 0,
            len: 0,
            enc,
            src,
        }
    }

    /// Reads the next value and provides the corresponding stage.
    pub fn read(&mut self) -> Result<Option<(i64, &Stage)>, Error> {
        let prefix = (self.acc >> 56) as u8;
        let stage = self.enc.stage_by_prefix(prefix);

        if let Some(stage) = stage {
            if stage.bits() <= self.len {
                let value = (self.acc << stage.prefix_bits()) >> (64 - stage.ordinal_bits());

                self.acc <<= stage.bits();
                self.len -= stage.bits();

                let value = value as i64 + stage.ordinal_min();
                return Ok(Some((value, stage)));
            }
        }

        let mut buf = [0u8; 8];
        let consumed = self.src.read(&mut buf)?;

        if consumed > 0 {
            let acc_next = u64::from_be_bytes(buf);
            let acc = if self.len > 0 {
                (acc_next >> self.len) | self.acc
            } else {
                acc_next
            };

            let len = self.len + consumed as u8 * 8;
            let prefix = (acc >> 56) as u8;

            if let Some(stage) = self.enc.stage_by_prefix(prefix) {
                if stage.bits() <= len {
                    self.acc = acc_next << (stage.bits() - self.len);
                    self.len = len - stage.bits();

                    let value = ((acc << stage.prefix_bits()) >> (64 - stage.ordinal_bits()))
                        as i64
                        + stage.ordinal_min();
                    return Ok(Some((value, stage)));
                }
            }
        }

        if self.acc == 0 {
            Ok(None)
        } else {
            Err(Error::new(ErrorKind::InvalidInput))
        }
    }
}
