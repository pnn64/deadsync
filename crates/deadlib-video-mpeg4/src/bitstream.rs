use crate::{Error, Result};

/// A byte-aligned start code and its payload, excluding the start code prefix.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Unit<'a> {
    pub code: u8,
    pub payload: &'a [u8],
}

/// Splits MPEG-4 Visual elementary bytes at their start codes.
pub fn units(bytes: &[u8]) -> impl Iterator<Item = Unit<'_>> {
    let mut remaining = find_prefix(bytes, 0).map_or(&[][..], |start| &bytes[start..]);
    core::iter::from_fn(move || {
        let &code = remaining.get(3)?;
        let next = find_prefix(remaining, 4).unwrap_or(remaining.len());
        let payload = &remaining[4..next];
        remaining = &remaining[next..];
        Some(Unit { code, payload })
    })
}

pub(crate) fn find_prefix(bytes: &[u8], start: usize) -> Option<usize> {
    bytes
        .get(start..)?
        .windows(3)
        .position(|window| window == [0, 0, 1])
        .map(|offset| start + offset)
}

#[derive(Clone, Copy)]
pub(crate) struct Bits<'a> {
    bytes: &'a [u8],
    position: usize,
}

impl<'a> Bits<'a> {
    pub(crate) fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, position: 0 }
    }

    // VLC lookup can look past the last byte; consumption still checks bounds.
    pub(crate) fn peek(&self, count: u8) -> u32 {
        debug_assert!((1..=24).contains(&count));
        let offset = self.position / 8;
        let tail = &self.bytes[offset..];
        let word = if let Some(bytes) = tail.first_chunk::<4>() {
            u32::from_be_bytes(*bytes)
        } else {
            let mut bytes = [0; 4];
            bytes[..tail.len()].copy_from_slice(tail);
            u32::from_be_bytes(bytes)
        };
        (word << (self.position % 8)) >> (32 - count)
    }

    pub(crate) fn skip(&mut self, count: u8) -> Result<()> {
        let end = self
            .position
            .checked_add(usize::from(count))
            .ok_or(Error::Truncated)?;
        if end.div_ceil(8) > self.bytes.len() {
            return Err(Error::Truncated);
        }
        self.position = end;
        Ok(())
    }

    pub(crate) fn read(&mut self, count: u8) -> Result<u32> {
        if count == 0 {
            return Ok(0);
        }
        let value = self.peek(count);
        self.skip(count)?;
        Ok(value)
    }

    pub(crate) fn flag(&mut self) -> Result<bool> {
        Ok(self.read(1)? != 0)
    }

    pub(crate) fn marker(&mut self) -> Result<()> {
        if !self.flag()? {
            return Err(Error::Invalid("marker bit"));
        }
        Ok(())
    }

    pub(crate) fn alignment_stuffing(&self) -> u8 {
        8 - (self.position % 8) as u8
    }

    pub(crate) fn vlc<const N: usize>(&mut self, table: &[u16; N]) -> Result<usize> {
        let width = N.trailing_zeros() as u8;
        let entry = table[self.peek(width) as usize];
        let length = (entry & 31) as u8;
        if length == 0 {
            return Err(Error::Invalid("variable-length code"));
        }
        self.skip(length)?;
        Ok(usize::from(entry >> 5))
    }
}

pub(crate) const fn vlc_table<const N: usize>(codes: &[(u16, u8)]) -> [u16; N] {
    let width = N.trailing_zeros();
    let mut table = [0; N];
    let mut symbol = 0;
    while symbol < codes.len() {
        let (code, length) = codes[symbol];
        if length == 0 {
            symbol += 1;
            continue;
        }
        let suffix = width - length as u32;
        let start = (code as usize) << suffix;
        let end = start + (1 << suffix);
        let mut index = start;
        while index < end {
            assert!(table[index] == 0);
            table[index] = ((symbol as u16) << 5) | length as u16;
            index += 1;
        }
        symbol += 1;
    }
    table
}
