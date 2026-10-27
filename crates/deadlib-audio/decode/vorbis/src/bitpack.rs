//! Vorbis section 2: integers are least-significant-bit first, widths 0..=32.
use crate::{Error, reserve};

#[derive(Clone, Debug)]
pub struct BitReader<'a> {
    input: &'a [u8],
    offset: usize,
    cache: u64,
    available: u32,
    exhausted: bool,
}
impl<'a> BitReader<'a> {
    pub fn new(input: &'a [u8]) -> Self {
        Self {
            input,
            offset: 0,
            cache: 0,
            available: 0,
            exhausted: false,
        }
    }
    pub fn position(&self) -> usize {
        self.offset * 8 - self.available as usize
    }
    pub fn remaining(&self) -> usize {
        (self.input.len() - self.offset) * 8 + self.available as usize
    }
    pub fn is_exhausted(&self) -> bool {
        self.exhausted
    }

    /// Speculative lookahead. Insufficient bits do not set end-of-packet;
    /// `read` does. Useful for a Huffman fast table near a packet boundary.
    #[inline]
    pub fn peek(&mut self, bits: u32) -> Option<u32> {
        if self.exhausted || bits > 32 {
            return None;
        }
        if self.available < bits {
            self.refill();
        }
        if self.available < bits {
            return None;
        }
        Some((self.cache & ((1u64 << bits) - 1)) as u32)
    }
    #[inline(never)]
    fn refill(&mut self) {
        let count = (((64 - self.available) / 8) as usize).min(self.input.len() - self.offset);
        if count == 8 {
            self.cache =
                u64::from_le_bytes(self.input[self.offset..self.offset + 8].try_into().unwrap());
            self.available = 64;
        } else {
            for &byte in &self.input[self.offset..self.offset + count] {
                self.cache |= (byte as u64) << self.available;
                self.available += 8;
            }
        }
        self.offset += count;
    }
    #[inline]
    pub fn read(&mut self, bits: u32) -> Result<u32, Error> {
        if bits > 32 {
            return Err(Error::Invalid("integer width exceeds 32"));
        }
        match self.peek(bits) {
            Some(value) => {
                self.consume(bits);
                Ok(value)
            }
            None => {
                self.exhausted = true;
                Err(Error::EndOfPacket)
            }
        }
    }
    /// Consume a successful lookahead without rechecking/refilling it.
    #[inline]
    pub(crate) fn consume(&mut self, bits: u32) {
        debug_assert!(bits <= 32 && bits <= self.available && !self.exhausted);
        self.cache >>= bits;
        self.available -= bits;
    }
    pub fn flag(&mut self) -> Result<bool, Error> {
        Ok(self.read(1)? != 0)
    }
}

#[derive(Default)]
pub struct BitWriter {
    output: Vec<u8>,
    cache: u64,
    available: u32,
}
impl BitWriter {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn position(&self) -> usize {
        self.output.len() * 8 + self.available as usize
    }
    /// Start another packet while retaining byte capacity.
    pub fn reset(&mut self) {
        self.output.clear();
        self.cache = 0;
        self.available = 0;
    }
    pub(crate) fn with_capacity(bytes: usize) -> Result<Self, Error> {
        let mut value = Self::new();
        value
            .output
            .try_reserve_exact(bytes)
            .map_err(|_| Error::Allocation)?;
        Ok(value)
    }
    pub(crate) fn capacity_bytes(&self) -> usize {
        self.output.capacity()
    }
    /// Discard high bits, as required by the bitpacking convention.
    pub fn write(&mut self, value: u32, bits: u32) -> Result<(), Error> {
        if bits > 32 {
            return Err(Error::Invalid("integer width exceeds 32"));
        }
        reserve(&mut self.output, ((self.available + bits) / 8) as usize)?;
        self.cache |= ((value as u64) & ((1u64 << bits) - 1)) << self.available;
        self.available += bits;
        while self.available >= 8 {
            self.output.push(self.cache as u8);
            self.cache >>= 8;
            self.available -= 8;
        }
        Ok(())
    }
    /// Pad the final byte with zero bits.
    pub fn finish_packet(&mut self) -> Result<&[u8], Error> {
        if self.available != 0 {
            reserve(&mut self.output, 1)?;
            self.output.push(self.cache as u8);
            self.available = 0;
            self.cache = 0;
        }
        Ok(&self.output)
    }
    pub fn finish(mut self) -> Result<Vec<u8>, Error> {
        self.finish_packet()?;
        Ok(self.output)
    }
}

pub fn ilog(value: u32) -> u32 {
    32 - value.leading_zeros()
}
