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
    let bytes = bytes.get(start..)?;
    // Test eight possible final bytes at once. The zero-byte test can also
    // report a neighbour after a borrow, so confirm every candidate.
    const ONES: u64 = 0x0101_0101_0101_0101;
    const HIGH: u64 = 0x8080_8080_8080_8080;
    let (words, _) = bytes.get(2..)?.as_chunks::<8>();
    for (index, word) in words.iter().enumerate() {
        let word = u64::from_le_bytes(*word) ^ ONES;
        let mut hits = word.wrapping_sub(ONES) & !word & HIGH;
        while hits != 0 {
            let offset = index * 8 + (hits.trailing_zeros() as usize / 8);
            if bytes[offset..].starts_with(&[0, 0, 1]) {
                return Some(start + offset);
            }
            hits &= hits - 1;
        }
    }
    let offset = words.len() * 8;
    bytes[offset..]
        .windows(3)
        .position(|window| window == [0, 0, 1])
        .map(|tail| start + offset + tail)
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
        let byte = *self.bytes.get(self.position / 8).ok_or(Error::Truncated)?;
        let value = byte & (0x80 >> (self.position % 8)) != 0;
        self.position = self.position.checked_add(1).ok_or(Error::Truncated)?;
        Ok(value)
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
        self.vlc_with_prefix(table, 0)
    }

    pub(crate) fn inverted_vlc<const N: usize>(&mut self, table: &[u16; N]) -> Result<usize> {
        self.vlc_with_prefix(table, N - 1)
    }

    #[inline]
    fn vlc_with_prefix<const N: usize>(
        &mut self,
        table: &[u16; N],
        invert: usize,
    ) -> Result<usize> {
        let width = N.trailing_zeros() as u8;
        let entry = table[self.peek(width) as usize ^ invert];
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

#[cfg(test)]
mod tests {
    use super::{find_prefix, Bits};

    #[test]
    fn flag_matches_one_bit_read_including_truncation() {
        for byte in 0..=u8::MAX {
            let bytes = [byte, !byte, byte, !byte];
            for len in 0..=bytes.len() {
                for position in 0..=len * 8 {
                    let mut expected = Bits::new(&bytes[..len]);
                    expected.skip(position as u8).unwrap();
                    let mut actual = expected;
                    assert_eq!(actual.flag(), expected.read(1).map(|v| v != 0));
                    assert_eq!(actual.position, expected.position);
                }
            }
        }
    }

    #[test]
    fn prefix_matches_bytewise_search_at_every_offset() {
        let mut seed = 0x1234_5678u32;
        let mut bytes = [0; 97];
        for len in 0..=bytes.len() {
            for _ in 0..32 {
                for byte in &mut bytes[..len] {
                    seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                    *byte = ((seed >> 28) & 3) as u8;
                }
                for start in 0..=len + 1 {
                    let expected = bytes[..len].get(start..).and_then(|tail| {
                        tail.windows(3)
                            .position(|w| w == [0, 0, 1])
                            .map(|i| start + i)
                    });
                    assert_eq!(find_prefix(&bytes[..len], start), expected);
                }
            }
        }
    }
}
