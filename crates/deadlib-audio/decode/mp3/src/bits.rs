//! Checked, MSB-first fields with explicit bit boundaries for part2_3 data.
use crate::{Error, reserve};

#[derive(Clone, Copy)]
pub struct BitReader<'a> {
    data: &'a [u8],
    position: usize,
    end: usize,
}
impl<'a> BitReader<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Self {
            data,
            position: 0,
            end: data.len().saturating_mul(8),
        }
    }
    pub fn bounded(data: &'a [u8], start: usize, bits: usize) -> Result<Self, Error> {
        let end = start.checked_add(bits).ok_or(Error::Truncated)?;
        if end > data.len().saturating_mul(8) {
            return Err(Error::Truncated);
        }
        Ok(Self {
            data,
            position: start,
            end,
        })
    }
    pub fn position(&self) -> usize {
        self.position
    }
    pub fn remaining(&self) -> usize {
        self.end - self.position
    }
    pub fn read(&mut self, width: u32) -> Result<u32, Error> {
        if width > 32 {
            return Err(Error::Invalid("bit field width"));
        }
        if width as usize > self.remaining() {
            return Err(Error::Truncated);
        }
        let mut value = 0;
        let mut left = width;
        while left != 0 {
            let shift = self.position & 7;
            let count = left.min((8 - shift) as u32);
            let bits = (self.data[self.position >> 3] >> (8 - shift - count as usize)) as u32;
            value = (value << count) | (bits & ((1 << count) - 1));
            self.position += count as usize;
            left -= count;
        }
        Ok(value)
    }
    pub fn flag(&mut self) -> Result<bool, Error> {
        Ok(self.read(1)? != 0)
    }
    pub fn skip(&mut self, width: usize) -> Result<(), Error> {
        if width > self.remaining() {
            return Err(Error::Truncated);
        }
        self.position += width;
        Ok(())
    }
}

#[derive(Default)]
pub struct BitWriter {
    data: Vec<u8>,
    bits: usize,
}
impl BitWriter {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn len_bits(&self) -> usize {
        self.bits
    }
    pub fn as_bytes(&self) -> &[u8] {
        &self.data
    }
    pub fn into_bytes(self) -> Vec<u8> {
        self.data
    }
    pub fn clear(&mut self) {
        self.data.clear();
        self.bits = 0;
    }
    pub fn write(&mut self, value: u32, width: u32) -> Result<(), Error> {
        if width > 32 || (width < 32 && value >> width != 0) {
            return Err(Error::Invalid("bit field value"));
        }
        let end = self
            .bits
            .checked_add(width as usize)
            .ok_or(Error::Allocation)?;
        let bytes = end.checked_add(7).ok_or(Error::Allocation)? / 8;
        let additional = bytes - self.data.len();
        reserve(&mut self.data, additional)?;
        self.data.resize(bytes, 0);
        for bit in (0..width).rev() {
            self.data[self.bits / 8] |= (((value >> bit) & 1) as u8) << (7 - self.bits % 8);
            self.bits += 1;
        }
        Ok(())
    }
}
