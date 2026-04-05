// Vendored from ordpath 0.5.0 — https://github.com/yohdeadfall/ordpath/
// License: MIT OR Apache-2.0 — Author: Yoh Deadfall <yoh.deadfall@hotmail.com>

use std::io::Write;

use crate::hierarchy::ordpath::enc::Encoding;
use crate::hierarchy::ordpath::{Error, ErrorKind};

/// The `Writer` struct allows reading ORDPATH encoded values directly from any source implementing [`Write`].
pub struct Writer<W: Write + ?Sized, E: Encoding> {
    acc: u64,
    len: u8,
    enc: E,
    dst: W,
}

impl<W: Write, E: Encoding> Writer<W, E> {
    /// Creates a new `Writer` for the givev destination.
    pub fn new(dst: W, enc: E) -> Self {
        Self {
            acc: 0,
            len: 0,
            enc,
            dst,
        }
    }

    /// Write a value into this writer.
    pub fn write(&mut self, value: i64) -> Result<(), Error> {
        let stage = self
            .enc
            .stage_by_value(value)
            .ok_or_else(|| Error::new(ErrorKind::InvalidInput))?;
        let prefix = stage.prefix() as u64;
        let value = (value - stage.ordinal_min()) as u64;

        let buf = match stage.bits() < 64 {
            true => ((prefix << stage.ordinal_bits()) | value) << (64 - stage.bits()),
            false => (prefix << (64 - stage.prefix_bits())) | (value >> (stage.bits() - 64)),
        };

        let len = self.len & 127;
        self.acc |= buf >> len;

        let len = len + stage.bits();
        self.len = 128
            | if len < 64 {
                len
            } else {
                let left = len - 64;

                self.len = 0;
                self.dst.write_all(&self.acc.to_be_bytes())?;
                self.acc = if stage.bits() <= 64 {
                    buf << (stage.bits() - left)
                } else {
                    value << (stage.bits() - left)
                };

                left
            };

        Ok(())
    }
}

impl<W: Write + ?Sized, E: Encoding> Drop for Writer<W, E> {
    fn drop(&mut self) {
        if self.len > 0 {
            let len = (self.len as usize & 127).div_ceil(8);
            let acc = &self.acc.to_be_bytes()[..len];

            _ = self.dst.write_all(acc);
        }
    }
}
