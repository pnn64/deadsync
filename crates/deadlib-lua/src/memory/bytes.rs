//! Byte sinks shared by standalone loaders and VM-owned scratch storage.
use super::{Buffer, fallible};
use crate::Error;
use std::ops::Deref;

pub(crate) trait ByteOutput: Deref<Target = [u8]> {
    fn reserve_bytes(&mut self, additional: usize) -> Result<(), Error>;
    fn push_byte(&mut self, byte: u8);
    fn extend_bytes(&mut self, bytes: &[u8]) -> Result<(), Error>;
    fn clear_bytes(&mut self);
}

impl ByteOutput for Vec<u8> {
    fn clear_bytes(&mut self) {
        self.clear();
    }
    fn reserve_bytes(&mut self, additional: usize) -> Result<(), Error> {
        fallible::reserve(self, additional)
    }
    fn push_byte(&mut self, byte: u8) {
        self.push(byte);
    }
    fn extend_bytes(&mut self, bytes: &[u8]) -> Result<(), Error> {
        self.reserve_bytes(bytes.len())?;
        self.extend_from_slice(bytes);
        Ok(())
    }
}

impl ByteOutput for Buffer<u8> {
    fn clear_bytes(&mut self) {
        self.clear();
    }
    fn reserve_bytes(&mut self, additional: usize) -> Result<(), Error> {
        self.try_reserve(additional)
    }
    fn push_byte(&mut self, byte: u8) {
        self.push(byte);
    }
    fn extend_bytes(&mut self, bytes: &[u8]) -> Result<(), Error> {
        self.try_extend_from_slice(bytes)
    }
}
