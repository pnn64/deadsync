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
    /// Peeks a five-bit prefix, padding unavailable low bits with zero.
    #[inline(always)]
    pub(crate) fn peek5(&self) -> usize {
        let available = self.remaining();
        let start = self.position >> 3;
        let offset = (self.position & 7) as u32;
        if available >= 5 {
            return if offset <= 3 {
                usize::from(self.data[start] >> (3 - offset)) & 31
            } else {
                usize::from(
                    u16::from_be_bytes([self.data[start], self.data[start + 1]]) >> (11 - offset),
                ) & 31
            };
        }
        if available == 0 {
            return 0;
        }
        let available = available as u32;
        let mut window = u16::from(self.data[start]) << 8;
        if offset + available > 8 {
            window |= u16::from(self.data[start + 1]);
        }
        let prefix = usize::from(window >> (11 - offset)) & 31;
        prefix & (31 << (5 - available))
    }
    /// Peeks eight bits, padding unavailable low bits with zero.
    #[inline(always)]
    pub(crate) fn peek8(&self) -> usize {
        let available = self.remaining().min(8);
        if available == 0 {
            return 0;
        }
        let start = self.position >> 3;
        let offset = (self.position & 7) as u32;
        let word = u16::from_be_bytes([
            self.data[start],
            self.data.get(start + 1).copied().unwrap_or(0),
        ]);
        let prefix = usize::from(word >> (8 - offset)) & 255;
        prefix & (255 << (8 - available))
    }
    pub fn read(&mut self, width: u32) -> Result<u32, Error> {
        if width > 32 {
            return Err(Error::Invalid("bit field width"));
        }
        if width as usize > self.remaining() {
            return Err(Error::Truncated);
        }
        if width == 0 {
            return Ok(0);
        }
        let count = (self.position & 7) as u32 + width;
        let start = self.position >> 3;
        let source = &self.data[start..start + count.div_ceil(8) as usize];
        // A field spans at most five bytes. Fixed-size loads replace the
        // byte-at-a-time extraction loop, including unaligned 32-bit fields.
        let value = match *source {
            [a] => u64::from(a),
            [a, b] => u64::from(u16::from_be_bytes([a, b])),
            [a, b, c] => u64::from(u32::from_be_bytes([0, a, b, c])),
            [a, b, c, d] => u64::from(u32::from_be_bytes([a, b, c, d])),
            [a, b, c, d, e] => u64::from_be_bytes([0, 0, 0, a, b, c, d, e]),
            _ => unreachable!(),
        };
        self.position += width as usize;
        Ok(((value >> ((8 - (count & 7)) & 7)) & ((1u64 << width) - 1)) as u32)
    }
    #[inline(always)]
    pub fn flag(&mut self) -> Result<bool, Error> {
        if self.position == self.end {
            return Err(Error::Truncated);
        }
        let value = self.data[self.position >> 3] & (0x80 >> (self.position & 7)) != 0;
        self.position += 1;
        Ok(value)
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

#[cfg(test)]
mod tests {
    use super::BitReader;

    #[test]
    fn eight_bit_prefixes_preserve_every_two_byte_pattern_and_bound() {
        assert_eq!(BitReader::new(&[]).peek8(), 0);
        for word in 0..=u16::MAX {
            let bytes = word.to_be_bytes();
            for start in 0..=16 {
                for available in 0..=16 - start {
                    let bits = BitReader::bounded(&bytes, start, available).unwrap();
                    let mut expected = 0;
                    for n in 0..available.min(8) {
                        expected |=
                            usize::from((bytes[(start + n) / 8] >> (7 - (start + n) % 8)) & 1)
                                << (7 - n);
                    }
                    assert_eq!(bits.peek8(), expected);
                }
            }
        }
    }

    #[test]
    fn padded_prefixes_preserve_every_two_byte_pattern_and_short_bound() {
        assert_eq!(BitReader::new(&[]).peek5(), 0);
        for word in 0..=u16::MAX {
            let bytes = word.to_be_bytes();
            for start in 0..=16 {
                for available in 0..=5.min(16 - start) {
                    let bits = BitReader::bounded(&bytes, start, available).unwrap();
                    let mut expected = 0;
                    for n in 0..available {
                        let bit = start + n;
                        expected |= usize::from((bytes[bit / 8] >> (7 - bit % 8)) & 1) << (4 - n);
                    }
                    assert_eq!(bits.peek5(), expected);
                    assert_eq!(bits.position(), start);
                    assert_eq!(bits.remaining(), available);
                }
            }
        }
    }
}
