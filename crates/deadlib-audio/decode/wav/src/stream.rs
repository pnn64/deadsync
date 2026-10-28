use crate::{Container, Ds64, Endian, Error, Limits, reserve};
use std::io::{Read, Seek, SeekFrom};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ChunkInfo {
    pub id: [u8; 4],
    pub offset: u64,
    pub size: u64,
}

/// Forward-only, bounded chunk demuxer. No audio codec dependency and no audio
/// allocation. `next_chunk` drains the previous payload/padding if unread.
pub struct Stream<R> {
    inner: R,
    container: Container,
    end: u64,
    pos: u64,
    remaining: u64,
    pad: bool,
    ds64: Vec<u8>,
    used: Vec<bool>,
    data_seen: bool,
    chunks_left: usize,
    failed: bool,
}
impl<R: Read> Stream<R> {
    pub fn new(inner: R) -> Result<Self, Error> {
        Self::with_limits(inner, Limits::default())
    }
    pub fn with_limits(mut inner: R, limits: Limits) -> Result<Self, Error> {
        let mut h = [0u8; 12];
        inner.read_exact(&mut h)?;
        let container = match &h[..4] {
            b"RIFF" => Container::Riff,
            b"RIFX" => Container::Rifx,
            b"RF64" => Container::Rf64,
            b"BW64" => Container::Bw64,
            _ => return Err(Error::Invalid("RIFF signature")),
        };
        if &h[8..] != b"WAVE" {
            return Err(Error::Invalid("not a WAVE form"));
        }
        let size32 = container.endian().u32(&h[4..]);
        let size = u64::from(size32);
        if size < 4 {
            return Err(Error::Invalid("RIFF size"));
        }
        let mut s = Self {
            inner,
            container,
            end: size + 8,
            pos: 12,
            remaining: 0,
            pad: false,
            ds64: Vec::new(),
            used: Vec::new(),
            data_seen: false,
            chunks_left: limits.chunks,
            failed: false,
        };
        if container.is_64() {
            if s.chunks_left == 0 {
                return Err(Error::Limit("chunk count"));
            }
            s.chunks_left -= 1;
            let mut h = [0; 8];
            s.inner.read_exact(&mut h)?;
            if &h[..4] != b"ds64" {
                return Err(Error::Invalid("ds64 must be first"));
            }
            let n = Endian::Little.u32(&h[4..]) as u64;
            if n < 28 {
                return Err(Error::Invalid("short ds64"));
            }
            let ds_end = 20 + n + (n & 1);
            if size32 != u32::MAX && ds_end > s.end {
                return Err(Error::Truncated);
            }
            let mut b = [0; 28];
            s.inner.read_exact(&mut b)?;
            let entries = Endian::Little.u32(&b[24..]) as usize;
            if entries > limits.size_table_entries {
                return Err(Error::Limit("ds64 size table"));
            }
            let table_bytes = entries
                .checked_mul(12)
                .ok_or(Error::Limit("ds64 size table"))?;
            let bytes = 28usize
                .checked_add(table_bytes)
                .ok_or(Error::Limit("ds64 size table"))?;
            if bytes as u64 > n {
                return Err(Error::Truncated);
            }
            reserve(&mut s.ds64, bytes)?;
            s.ds64.extend(b);
            s.ds64.resize(bytes, 0);
            s.inner.read_exact(&mut s.ds64[28..])?;
            let d = Ds64::parse(&s.ds64, limits)?;
            if size32 == u32::MAX {
                s.end = d
                    .riff_size
                    .checked_add(8)
                    .ok_or(Error::Invalid("RIFF size overflow"))?;
            }
            if s.end < ds_end {
                return Err(Error::Truncated);
            }
            reserve(&mut s.used, entries)?;
            s.used.resize(entries, false);
            s.pos = 20 + bytes as u64;
            s.remaining = n - bytes as u64;
            s.pad = n & 1 != 0;
            s.drain()?;
        }
        Ok(s)
    }
    pub fn container(&self) -> Container {
        self.container
    }
    pub fn form_bytes(&self) -> u64 {
        self.end
    }
    pub fn ds64(&self) -> Option<Ds64<'_>> {
        if self.ds64.is_empty() {
            None
        } else {
            // Validated during construction, with the same or tighter table bound.
            Some(
                Ds64::parse(
                    &self.ds64,
                    Limits {
                        size_table_entries: self.used.len(),
                        ..Limits::default()
                    },
                )
                .unwrap(),
            )
        }
    }
    pub fn into_inner(self) -> R {
        self.inner
    }
    pub fn next_chunk(&mut self) -> Result<Option<ChunkInfo>, Error> {
        self.check()?;
        let result = self.drain().and_then(|()| self.header());
        if result.is_err() {
            self.failed = true;
        }
        result
    }
    /// Read only the current chunk payload. Never consumes padding or next headers.
    pub fn read_chunk(&mut self, out: &mut [u8]) -> Result<usize, Error> {
        self.check()?;
        let n = self.remaining.min(out.len() as u64) as usize;
        // read_exact handles short reads and Interrupted, without exposing partial frames.
        if let Err(e) = self.inner.read_exact(&mut out[..n]) {
            self.failed = true;
            return Err(e.into());
        }
        self.remaining -= n as u64;
        self.pos += n as u64;
        Ok(n)
    }
    /// Validate and consume everything through the form boundary.
    pub fn finish(mut self) -> Result<R, Error> {
        while self.next_chunk()?.is_some() {}
        Ok(self.inner)
    }
    fn check(&self) -> Result<(), Error> {
        if self.failed {
            Err(Error::Invalid(
                "stream unusable after I/O or structure error",
            ))
        } else {
            Ok(())
        }
    }
    fn drain(&mut self) -> Result<(), Error> {
        let mut scratch = [0u8; 8192];
        while self.remaining != 0 {
            self.read_chunk(&mut scratch)?;
        }
        self.padding()
    }
    fn padding(&mut self) -> Result<(), Error> {
        if self.pad {
            self.inner.read_exact(&mut [0u8; 1])?;
            self.pos += 1;
            self.pad = false;
        }
        Ok(())
    }
    fn header(&mut self) -> Result<Option<ChunkInfo>, Error> {
        if self.pos == self.end {
            return Ok(None);
        }
        if self.end - self.pos < 8 {
            return Err(Error::Truncated);
        }
        if self.chunks_left == 0 {
            return Err(Error::Limit("chunk count"));
        }
        self.chunks_left -= 1;
        let mut h = [0; 8];
        self.inner.read_exact(&mut h)?;
        self.pos += 8;
        let id: [u8; 4] = h[..4].try_into().unwrap();
        let n = self.container.endian().u32(&h[4..]);
        let size = if n == u32::MAX && !self.ds64.is_empty() {
            let d = self.ds64().unwrap();
            if id == *b"data" && !self.data_seen {
                d.data_size
            } else {
                let i = d
                    .table
                    .as_chunks::<12>()
                    .0
                    .iter()
                    .enumerate()
                    .position(|(i, row)| row[..4] == id && !self.used[i])
                    .ok_or(Error::Invalid("missing ds64 table entry"))?;
                let n = Endian::Little.u64(&d.table[i * 12 + 4..]);
                self.used[i] = true;
                n
            }
        } else {
            u64::from(n)
        };
        if id == *b"ds64" {
            return Err(Error::Invalid("unexpected ds64"));
        }
        let end = self
            .pos
            .checked_add(size)
            .and_then(|n| n.checked_add(size & 1))
            .ok_or(Error::Invalid("chunk size overflow"))?;
        if end > self.end {
            return Err(Error::Truncated);
        }
        if id == *b"data" {
            self.data_seen = true;
        }
        self.remaining = size;
        self.pad = size & 1 != 0;
        Ok(Some(ChunkInfo {
            id,
            offset: self.pos,
            size,
        }))
    }
}
impl<R: Read + Seek> Stream<R> {
    /// Seek past payloads while still reading padding. The caller must verify the
    /// physical stream length separately; seeking alone cannot detect truncation.
    pub(crate) fn next_chunk_seek(&mut self) -> Result<Option<ChunkInfo>, Error> {
        self.check()?;
        let result = (|| {
            let current = self.inner.stream_position()?;
            let end = current
                .checked_add(self.remaining)
                .ok_or(Error::Invalid("seek overflow"))?;
            self.inner.seek(SeekFrom::Start(end))?;
            self.pos += self.remaining;
            self.remaining = 0;
            self.padding()?;
            self.header()
        })();
        if result.is_err() {
            self.failed = true;
        }
        result
    }
}
