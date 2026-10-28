use crate::{Error, Result};

pub(crate) struct Output {
    pub bytes: Vec<u8>,
    limit: usize,
    buffer: u64,
    bits: u8,
}

impl Output {
    pub fn new(limit: usize) -> Self {
        Self {
            bytes: Vec::new(),
            limit,
            buffer: 0,
            bits: 0,
        }
    }

    fn reserve(&mut self, n: usize) -> Result<()> {
        let wanted = self
            .bytes
            .len()
            .checked_add(n)
            .ok_or(Error::LimitExceeded("encoded bytes"))?;
        if wanted > self.limit {
            return Err(Error::LimitExceeded("encoded bytes"));
        }
        if wanted > self.bytes.capacity() {
            let capacity = self
                .bytes
                .capacity()
                .saturating_mul(2)
                .max(1024)
                .min(self.limit)
                .max(wanted);
            self.bytes
                .try_reserve_exact(capacity - self.bytes.len())
                .map_err(|_| Error::Allocation)?;
        }
        Ok(())
    }

    pub fn raw(&mut self, bytes: &[u8]) -> Result<()> {
        self.reserve(bytes.len())?;
        self.bytes.extend_from_slice(bytes);
        Ok(())
    }

    pub fn marker(&mut self, marker: u8) -> Result<()> {
        self.raw(&[0xff, marker])
    }

    /// Bound a frame appended to the shared output without another buffer.
    pub fn bounded<T>(
        &mut self,
        additional: usize,
        write: impl FnOnce(&mut Self) -> Result<T>,
    ) -> Result<T> {
        let previous = self.limit;
        self.limit = previous.min(self.bytes.len().saturating_add(additional));
        let result = write(self);
        self.limit = previous;
        result
    }

    pub fn segment(&mut self, marker: u8, data: &[u8]) -> Result<()> {
        let length = u16::try_from(data.len().saturating_add(2))
            .map_err(|_| Error::Invalid("encoder segment length"))?;
        self.reserve(data.len() + 4)?;
        self.marker(marker)?;
        self.raw(&length.to_be_bytes())?;
        self.raw(data)
    }

    #[inline]
    pub fn write_bits(&mut self, value: u32, n: u8) -> Result<()> {
        debug_assert!(n <= 16 && (n == 0 || value < 1u32 << n));
        self.buffer = (self.buffer << n) | u64::from(value);
        self.bits += n;
        while self.bits >= 8 {
            self.bits -= 8;
            let byte = (self.buffer >> self.bits) as u8;
            if byte == 255 {
                self.raw(&[255, 0])?;
            } else {
                self.raw(&[byte])?;
            }
        }
        Ok(())
    }

    /// Write borrowed payload pieces without an intermediate segment buffer.
    pub fn segment_parts(&mut self, marker: u8, parts: &[&[u8]]) -> Result<()> {
        let length = parts
            .iter()
            .try_fold(2usize, |n, p| n.checked_add(p.len()))
            .and_then(|n| u16::try_from(n).ok())
            .ok_or(Error::Invalid("encoder segment length"))?;
        self.reserve(length as usize + 2)?;
        self.marker(marker)?;
        self.raw(&length.to_be_bytes())?;
        for part in parts {
            self.raw(part)?;
        }
        Ok(())
    }

    pub fn pad(&mut self) -> Result<()> {
        if self.bits > 0 {
            let n = 8 - self.bits;
            self.write_bits((1u32 << n) - 1, n)?;
        }
        self.buffer = 0;
        Ok(())
    }
}
