//! Checked, MSB-first FLAC bit packing. Reads never require padding after input.
use crate::Error;

#[derive(Clone)]
pub struct BitReader<'a> {
    data: &'a [u8],
    next: usize,
    cache: u64,
    bits: u32,
}
impl<'a> BitReader<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Self {
            data,
            next: 0,
            cache: 0,
            bits: 0,
        }
    }
    pub fn position(&self) -> usize {
        self.next * 8 - self.bits as usize
    }
    pub fn remaining(&self) -> usize {
        self.data.len() * 8 - self.position()
    }

    #[inline]
    fn refill(&mut self) {
        if self.bits <= 56 && self.data.len() - self.next >= 8 {
            // Load eight available bytes, retaining only the whole bytes that
            // fit after the cached prefix. No padding or unaligned access is
            // required; the tail below handles exact end-of-input slices.
            let word = u64::from_be_bytes(self.data[self.next..self.next + 8].try_into().unwrap());
            let bytes = (64 - self.bits) / 8;
            let added = bytes * 8;
            self.cache |= (word >> (64 - added)) << (64 - self.bits - added);
            self.next += bytes as usize;
            self.bits += added;
            return;
        }
        while self.bits <= 56 && self.next < self.data.len() {
            self.cache |= (self.data[self.next] as u64) << (56 - self.bits);
            self.next += 1;
            self.bits += 8;
        }
    }

    /// Read 0..=56 bits. The codec's largest individual field is 36 bits.
    #[inline]
    pub fn read(&mut self, width: u32) -> Result<u64, Error> {
        if width > 56 {
            return Err(Error::Invalid("bit read width"));
        }
        if width == 0 {
            return Ok(0);
        }
        if self.bits < width {
            self.refill();
        }
        if self.bits < width {
            return Err(Error::Truncated);
        }
        let value = self.cache >> (64 - width);
        self.cache <<= width;
        self.bits -= width;
        Ok(value)
    }

    #[inline]
    pub fn signed(&mut self, width: u32) -> Result<i64, Error> {
        if width == 0 {
            return Ok(0);
        }
        Ok(((self.read(width)? << (64 - width)) as i64) >> (64 - width))
    }

    /// Count zero bits before a one, rejecting values beyond a caller's bound.
    #[inline]
    pub fn unary(&mut self, limit: u64) -> Result<u64, Error> {
        let mut total = 0u64;
        loop {
            if self.bits == 0 {
                self.refill();
            }
            if self.bits == 0 {
                return Err(Error::Truncated);
            }
            let zeros = self.cache.leading_zeros().min(self.bits);
            total += u64::from(zeros);
            if total > limit {
                return Err(Error::Invalid("unary value overflow"));
            }
            if zeros < self.bits {
                let used = zeros + 1;
                self.cache = self.cache.checked_shl(used).unwrap_or(0);
                self.bits -= used;
                return Ok(total);
            }
            self.cache = 0;
            self.bits = 0;
        }
    }

    pub fn align_zero(&mut self) -> Result<(), Error> {
        let count = (8 - self.position() % 8) % 8;
        if self.read(count as u32)? != 0 {
            return Err(Error::Invalid("nonzero frame padding"));
        }
        Ok(())
    }
}

#[derive(Clone, Default)]
pub struct BitWriter {
    data: Vec<u8>,
    bits: u8,
}
impl BitWriter {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn clear(&mut self) {
        self.data.clear();
        self.bits = 0;
    }
    pub fn position(&self) -> usize {
        self.data.len() * 8
            - if self.bits == 0 {
                0
            } else {
                8 - self.bits as usize
            }
    }
    pub fn write(&mut self, value: u64, width: u32) -> Result<(), Error> {
        if width > 64 {
            return Err(Error::Invalid("bit write width"));
        }
        crate::reserve(&mut self.data, (width as usize + 14) / 8)?;
        let mut left = width;
        while left != 0 {
            if self.bits == 0 {
                self.data.push(0);
            }
            let n = left.min(u32::from(8 - self.bits));
            left -= n;
            let last = self.data.last_mut().unwrap();
            *last |= (((value >> left) & ((1 << n) - 1)) as u8) << (8 - self.bits - n as u8);
            self.bits = (self.bits + n as u8) % 8;
        }
        Ok(())
    }
    pub fn signed(&mut self, value: i64, width: u32) -> Result<(), Error> {
        self.write(value as u64, width)
    }
    pub fn unary(&mut self, zeros: u64) -> Result<(), Error> {
        let mut left = zeros;
        while left >= 64 {
            self.write(0, 64)?;
            left -= 64;
        }
        self.write(0, left as u32)?;
        self.write(1, 1)
    }
    pub fn align_zero(&mut self) {
        self.bits = 0;
    }
    pub fn as_bytes(&self) -> &[u8] {
        &self.data
    }
    pub fn into_bytes(self) -> Vec<u8> {
        self.data
    }
}
