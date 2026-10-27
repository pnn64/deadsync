//! Checked, MSB-first FLAC bit packing. Reads never require padding after input.
use crate::{Error, sample::Sample};

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
    pub(crate) fn at_position(data: &'a [u8], position: usize) -> Result<Self, Error> {
        if position / 8 > data.len() || (position / 8 == data.len() && !position.is_multiple_of(8))
        {
            return Err(Error::Truncated);
        }
        let mut reader = Self::new(data);
        reader.next = position / 8;
        reader.read((position % 8) as u32)?;
        Ok(reader)
    }
    pub fn position(&self) -> usize {
        self.next * 8 - self.bits as usize
    }
    pub fn remaining(&self) -> usize {
        self.data.len() * 8 - self.position()
    }

    /// Borrow an aligned byte field and discard only the speculative cache.
    pub(crate) fn aligned_bytes(&mut self, len: usize) -> Result<&'a [u8], Error> {
        let bit_position = self.position();
        debug_assert!(bit_position.is_multiple_of(8));
        let start = bit_position / 8;
        let end = start.checked_add(len).ok_or(Error::Truncated)?;
        let bytes = self.data.get(start..end).ok_or(Error::Truncated)?;
        self.next = end;
        self.cache = 0;
        self.bits = 0;
        Ok(bytes)
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
    #[inline(always)]
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

    /// Decode a Rice partition, fusing the unary prefix and remainder when
    /// both are already cached. Exact tails and long quotients use checked reads.
    pub(crate) fn rice<T: Sample>(&mut self, output: &mut [T], k: u32) -> Result<(), Error> {
        #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
        {
            x86::rice(self, output, k)
        }
        #[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
        self.rice_inner(output, k)
    }

    #[inline(always)]
    fn rice_inner<T: Sample>(&mut self, output: &mut [T], k: u32) -> Result<(), Error> {
        debug_assert!(k <= 30);
        let mask = (1u64 << k) - 1;
        let limit = u64::from(u32::MAX - 1);
        for sample in output {
            if self.bits <= k {
                self.refill();
            }
            let q = self.cache.leading_zeros();
            let used = q + 1 + k;
            let folded = if used < 64 && used <= self.bits {
                let value = (u64::from(q) << k) | ((self.cache >> (64 - used)) & mask);
                self.cache <<= used;
                self.bits -= used;
                value
            } else {
                let q = self.unary(limit >> k)?;
                (q << k) | self.read(k)?
            };
            if folded > limit {
                return Err(Error::Invalid("residual exceeds 31-bit magnitude"));
            }
            *sample = T::store(((folded >> 1) as i64) ^ -((folded & 1) as i64));
        }
        Ok(())
    }

    pub fn align_zero(&mut self) -> Result<(), Error> {
        let count = (8 - self.position() % 8) % 8;
        if self.read(count as u32)? != 0 {
            return Err(Error::Invalid("nonzero frame padding"));
        }
        Ok(())
    }
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
#[allow(unsafe_code)]
mod x86 {
    use super::*;

    pub(super) fn rice<T: Sample>(
        reader: &mut BitReader<'_>,
        output: &mut [T],
        k: u32,
    ) -> Result<(), Error> {
        if output.len() >= 32
            && std::is_x86_feature_detected!("bmi2")
            && std::is_x86_feature_detected!("lzcnt")
        {
            // SAFETY: both instruction-set extensions are checked at runtime.
            // The same safe, checked implementation is compiled for each path.
            unsafe { accelerated(reader, output, k) }
        } else {
            reader.rice_inner(output, k)
        }
    }

    #[target_feature(enable = "bmi2,lzcnt")]
    unsafe fn accelerated<T: Sample>(
        reader: &mut BitReader<'_>,
        output: &mut [T],
        k: u32,
    ) -> Result<(), Error> {
        reader.rice_inner(output, k)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rice_dispatch_matches_portable_cache_and_tails() {
        for k in 0..=30 {
            for offset in 0..8 {
                let mask = (1u64 << k) - 1;
                let limit = u64::from(u32::MAX - 1);
                let mut writer = BitWriter::new();
                writer.write(0, offset).unwrap();
                for i in 0..65 {
                    let folded = match i % 5 {
                        0 => 0,
                        1 => 1,
                        2 => mask,
                        3 => (65u64 << k).min(limit),
                        _ => (3u64 << k).min(limit),
                    };
                    writer.unary(folded >> k).unwrap();
                    writer.write(folded & mask, k).unwrap();
                }
                let data = writer.into_bytes();
                for end in 0..=data.len() {
                    let mut portable = BitReader::new(&data[..end]);
                    let mut dispatched = portable.clone();
                    let mut expected = [0; 65];
                    let mut actual = [0; 65];
                    let a = portable
                        .read(offset)
                        .and_then(|_| portable.rice_inner(&mut expected, k));
                    let b = dispatched
                        .read(offset)
                        .and_then(|_| dispatched.rice(&mut actual, k));
                    assert_eq!(a, b, "k={k}, offset={offset}, end={end}");
                    assert_eq!(expected, actual);
                    assert_eq!(portable.position(), dispatched.position());
                }
            }
        }
    }

    #[test]
    fn fused_rice_matches_separate_reads_and_every_truncated_tail() {
        for k in 0..=30 {
            for offset in 0..8 {
                let mask = (1u64 << k) - 1;
                let limit = u64::from(u32::MAX - 1);
                let folded = [
                    0,
                    1,
                    2,
                    3,
                    mask,
                    (63u64 << k).min(limit),
                    (130u64 << k).min(limit),
                ];
                let mut writer = BitWriter::new();
                writer.write(0, offset).unwrap();
                for f in folded {
                    writer.unary(f >> k).unwrap();
                    writer.write(f & mask, k).unwrap();
                }
                let data = writer.into_bytes();
                let mut fused = BitReader::new(&data);
                let mut separate = fused.clone();
                fused.read(offset).unwrap();
                separate.read(offset).unwrap();
                let mut actual = [0; 7];
                fused.rice(&mut actual, k).unwrap();
                for (i, f) in folded.into_iter().enumerate() {
                    assert_eq!(actual[i], ((f >> 1) as i64) ^ -((f & 1) as i64));
                    let q = separate.unary(limit >> k).unwrap();
                    assert_eq!((q << k) | separate.read(k).unwrap(), f);
                }
                assert_eq!(fused.position(), separate.position());
                assert_eq!(fused.remaining(), separate.remaining());
                for end in 0..data.len() {
                    let mut reader = BitReader::new(&data[..end]);
                    let result = reader
                        .read(offset)
                        .and_then(|_| reader.rice(&mut actual, k));
                    assert!(result.is_err(), "k={k}, offset={offset}, end={end}");
                }
            }
        }
    }

    #[test]
    fn fused_rice_rejects_forbidden_magnitude() {
        for k in 22..=30 {
            for offset in 0..8 {
                let mut writer = BitWriter::new();
                writer.write(0, offset).unwrap();
                writer.unary(u64::from(u32::MAX) >> k).unwrap();
                writer
                    .write(u64::from(u32::MAX) & ((1u64 << k) - 1), k)
                    .unwrap();
                let data = writer.into_bytes();
                let mut reader = BitReader::new(&data);
                reader.read(offset).unwrap();
                assert!(matches!(reader.rice(&mut [0], k), Err(Error::Invalid(_))));
            }
        }
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
