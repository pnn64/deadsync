use crate::{Error, Result};

/// Monomorphized storage: Vec retains the earlier allocation path; a streaming
/// sink buffers only a bounded window and calls the external writer on drains.
pub(crate) trait Sink {
    const RETAINS_OUTPUT: bool;
    fn len(&self) -> usize;
    fn prepare(&mut self, memory: usize, limit: usize) -> Result<()>;
    fn reserve(&mut self, n: usize, limit: usize) -> Result<()>;
    fn push(&mut self, bytes: &[u8]) -> Result<()>;
}

impl Sink for Vec<u8> {
    const RETAINS_OUTPUT: bool = true;
    fn len(&self) -> usize {
        Vec::len(self)
    }
    fn prepare(&mut self, _memory: usize, _limit: usize) -> Result<()> {
        Ok(())
    }
    fn reserve(&mut self, n: usize, limit: usize) -> Result<()> {
        let wanted = self.len() + n;
        if wanted > self.capacity() {
            let capacity = self
                .capacity()
                .saturating_mul(2)
                .max(1024)
                .min(limit)
                .max(wanted);
            self.try_reserve_exact(capacity - self.len())
                .map_err(|_| Error::Allocation)?;
        }
        Ok(())
    }
    fn push(&mut self, bytes: &[u8]) -> Result<()> {
        self.extend_from_slice(bytes);
        Ok(())
    }
}

impl<S: Sink> Sink for &mut S {
    const RETAINS_OUTPUT: bool = S::RETAINS_OUTPUT;
    fn len(&self) -> usize {
        (**self).len()
    }
    fn prepare(&mut self, memory: usize, limit: usize) -> Result<()> {
        (**self).prepare(memory, limit)
    }
    fn reserve(&mut self, n: usize, limit: usize) -> Result<()> {
        (**self).reserve(n, limit)
    }
    fn push(&mut self, bytes: &[u8]) -> Result<()> {
        (**self).push(bytes)
    }
}

pub(crate) struct Output<S: Sink = Vec<u8>> {
    pub bytes: S,
    limit: usize,
    buffer: u64,
    bits: u8,
    external_bytes: usize,
}

impl Output {
    pub fn new(limit: usize) -> Self {
        Self {
            bytes: Vec::new(),
            limit,
            buffer: 0,
            bits: 0,
            external_bytes: 0,
        }
    }
}

impl<'a, S: Sink> Output<&'a mut S> {
    pub fn with_sink(bytes: &'a mut S, limit: usize, memory: usize) -> Result<Self> {
        bytes.prepare(memory, limit)?;
        Ok(Self {
            bytes,
            limit: if S::RETAINS_OUTPUT {
                limit.min(memory)
            } else {
                limit
            },
            buffer: 0,
            bits: 0,
            external_bytes: 0,
        })
    }
}

impl<S: Sink> Output<S> {
    fn reserve(&mut self, n: usize) -> Result<()> {
        let wanted = self
            .bytes
            .len()
            .checked_add(n)
            .ok_or(Error::LimitExceeded("encoded bytes"))?;
        if wanted > self.limit {
            return Err(Error::LimitExceeded("encoded bytes"));
        }
        self.bytes.reserve(n, self.limit)
    }

    pub fn raw(&mut self, bytes: &[u8]) -> Result<()> {
        self.reserve(bytes.len())?;
        self.bytes.push(bytes)
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
        let external = self.external_bytes;
        self.limit = previous.min(self.bytes.len().saturating_add(additional));
        let result = write(self);
        self.limit = previous.saturating_sub(self.external_bytes - external);
        result
    }

    /// Account for bytes routed to a companion table stream. Preserve this
    /// deduction when a nested frame budget restores its enclosing limit.
    pub fn reserve_external(&mut self, n: usize) -> Result<()> {
        let limit = self
            .limit
            .checked_sub(n)
            .ok_or(Error::LimitExceeded("encoded bytes"))?;
        if self.bytes.len() > limit {
            return Err(Error::LimitExceeded("encoded bytes"));
        }
        self.limit = limit;
        self.external_bytes = self
            .external_bytes
            .checked_add(n)
            .ok_or(Error::LimitExceeded("encoded bytes"))?;
        Ok(())
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
