use crate::{Endian, Error, Format, Limits, Pcm, reserve};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Container {
    Riff,
    Rifx,
    Rf64,
    Bw64,
}
impl Container {
    pub const fn endian(self) -> Endian {
        match self {
            Self::Rifx => Endian::Big,
            _ => Endian::Little,
        }
    }
    pub const fn is_64(self) -> bool {
        matches!(self, Self::Rf64 | Self::Bw64)
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Ds64<'a> {
    pub riff_size: u64,
    pub data_size: u64,
    pub sample_count: u64,
    pub table: &'a [u8],
}
impl<'a> Ds64<'a> {
    pub fn parse(b: &'a [u8], limits: Limits) -> Result<Self, Error> {
        if b.len() < 28 {
            return Err(Error::Truncated);
        }
        let n = Endian::Little.u32(&b[24..]) as usize;
        if n > limits.size_table_entries {
            return Err(Error::Limit("ds64 size table"));
        }
        let len = n.checked_mul(12).ok_or(Error::Limit("ds64 size table"))?;
        Ok(Self {
            riff_size: Endian::Little.u64(b),
            data_size: Endian::Little.u64(&b[8..]),
            sample_count: Endian::Little.u64(&b[16..]),
            table: b
                .get(
                    28..28usize
                        .checked_add(len)
                        .ok_or(Error::Limit("ds64 size table"))?,
                )
                .ok_or(Error::Truncated)?,
        })
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Chunk<'a> {
    pub id: [u8; 4],
    /// Offset of the payload, relative to the start of this RIFF form.
    pub offset: u64,
    pub data: &'a [u8],
    /// Padding is excluded from data; retain it for exact container copying.
    pub padding: &'a [u8],
}

/// Checked chunk traversal. Only nonempty ds64 tables require allocation.
pub struct Chunks<'a> {
    bytes: &'a [u8],
    pos: usize,
    endian: Endian,
    ds64: Option<Ds64<'a>>,
    used: Vec<bool>,
    data_seen: bool,
    remaining: usize,
}
impl<'a> Chunks<'a> {
    pub(crate) fn new(
        bytes: &'a [u8],
        endian: Endian,
        ds64: Option<Ds64<'a>>,
        limits: Limits,
    ) -> Result<Self, Error> {
        let n = ds64.map_or(0, |d| d.table.len() / 12);
        let mut used = Vec::new();
        reserve(&mut used, n)?;
        used.resize(n, false);
        Ok(Self {
            bytes,
            pos: 12,
            endian,
            ds64,
            used,
            data_seen: false,
            remaining: limits.chunks,
        })
    }
    fn next_chunk(&mut self) -> Result<Chunk<'a>, Error> {
        if self.remaining == 0 {
            return Err(Error::Limit("chunk count"));
        }
        self.remaining -= 1;
        let h = self
            .bytes
            .get(self.pos..self.pos + 8)
            .ok_or(Error::Truncated)?;
        let id: [u8; 4] = h[..4].try_into().unwrap();
        let size = self.endian.u32(&h[4..]);
        let size = if let (u32::MAX, Some(d)) = (size, self.ds64) {
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
                self.used[i] = true;
                Endian::Little.u64(&d.table[i * 12 + 4..])
            }
        } else {
            u64::from(size)
        };
        if id == *b"data" {
            self.data_seen = true;
        }
        let start = self.pos + 8;
        let end = u64::try_from(start)
            .unwrap()
            .checked_add(size)
            .ok_or(Error::Invalid("chunk size overflow"))?;
        let padded = end
            .checked_add(size & 1)
            .ok_or(Error::Invalid("chunk padding overflow"))?;
        if padded > self.bytes.len() as u64 {
            return Err(Error::Truncated);
        }
        self.pos = padded as usize;
        Ok(Chunk {
            id,
            offset: start as u64,
            data: &self.bytes[start..end as usize],
            padding: &self.bytes[end as usize..padded as usize],
        })
    }
}
impl<'a> Iterator for Chunks<'a> {
    type Item = Result<Chunk<'a>, Error>;
    fn next(&mut self) -> Option<Self::Item> {
        if self.pos == self.bytes.len() {
            return None;
        }
        let result = self.next_chunk();
        if result.is_err() {
            self.pos = self.bytes.len();
        }
        Some(result)
    }
}
impl std::iter::FusedIterator for Chunks<'_> {}

/// Validated borrowed WAVE form. Opening ordinary PCM/float files allocates nothing.
#[derive(Debug)]
pub struct Wave<'a> {
    pub container: Container,
    pub format: Format<'a>,
    pub ds64: Option<Ds64<'a>>,
    pub fact: Option<u64>,
    data: &'a [u8],
    data_offset: u64,
    bytes: &'a [u8],
    limits: Limits,
}
impl<'a> Wave<'a> {
    pub fn parse(b: &'a [u8]) -> Result<Self, Error> {
        Self::with_limits(b, Limits::default())
    }
    pub fn with_limits(b: &'a [u8], limits: Limits) -> Result<Self, Error> {
        if b.len() < 12 {
            return Err(Error::Truncated);
        }
        let container = match &b[..4] {
            b"RIFF" => Container::Riff,
            b"RIFX" => Container::Rifx,
            b"RF64" => Container::Rf64,
            b"BW64" => Container::Bw64,
            _ => return Err(Error::Invalid("RIFF signature")),
        };
        if &b[8..12] != b"WAVE" {
            return Err(Error::Invalid("not a WAVE form"));
        }
        let e = container.endian();
        let size32 = e.u32(&b[4..]);
        let ds64 = if container.is_64() {
            if b.get(12..16) != Some(b"ds64") {
                return Err(Error::Invalid("ds64 must be first"));
            }
            let h = b.get(16..20).ok_or(Error::Truncated)?;
            let n = e.u32(h) as u64;
            let end = 20u64.checked_add(n).ok_or(Error::Invalid("ds64 size"))?;
            if end > b.len() as u64 {
                return Err(Error::Truncated);
            }
            Some(Ds64::parse(&b[20..end as usize], limits)?)
        } else {
            None
        };
        let size = match ds64 {
            Some(d) if size32 == u32::MAX => d.riff_size,
            _ => u64::from(size32),
        };
        if size < 4 {
            return Err(Error::Invalid("RIFF size"));
        }
        let end = size
            .checked_add(8)
            .ok_or(Error::Invalid("RIFF size overflow"))?;
        if end > b.len() as u64 {
            return Err(Error::Truncated);
        }
        let bytes = &b[..end as usize];
        let mut format = None;
        let mut data = None;
        let mut fact = None;
        for c in Chunks::new(bytes, e, ds64, limits)? {
            let c = c?;
            match &c.id {
                b"fmt " => {
                    if format.is_some() {
                        return Err(Error::Invalid("duplicate format chunk"));
                    }
                    if c.data.len() > limits.format_bytes {
                        return Err(Error::Limit("format size"));
                    }
                    format = Some(Format::parse(c.data, e)?);
                }
                b"data" => {
                    if data.is_some() {
                        return Err(Error::Unsupported("multiple data chunks"));
                    }
                    data = Some(c);
                }
                b"fact" => {
                    if fact.is_some() {
                        return Err(Error::Invalid("duplicate fact chunk"));
                    }
                    if c.data.len() < 4 {
                        return Err(Error::Truncated);
                    }
                    let n = e.u32(c.data);
                    fact = Some(if n == u32::MAX {
                        ds64.map_or(u64::from(n), |d| d.sample_count)
                    } else {
                        u64::from(n)
                    });
                }
                b"ds64" if !container.is_64() || c.offset != 20 => {
                    return Err(Error::Invalid("unexpected ds64"));
                }
                _ => {}
            }
        }
        let format = format.ok_or(Error::Invalid("missing format chunk"))?;
        let data = data.ok_or(Error::Invalid("missing data chunk"))?;
        if matches!(
            format.encoding,
            crate::Encoding::Pcm | crate::Encoding::Float
        ) && data.data.len() % usize::from(format.block_align) != 0
        {
            return Err(Error::Invalid("partial PCM frame"));
        }
        Ok(Self {
            container,
            format,
            ds64,
            fact,
            data: data.data,
            data_offset: data.offset,
            bytes,
            limits,
        })
    }
    pub fn chunks(&self) -> Result<Chunks<'a>, Error> {
        Chunks::new(self.bytes, self.format.endian, self.ds64, self.limits)
    }
    pub fn data(&self) -> &'a [u8] {
        self.data
    }
    pub fn data_offset(&self) -> u64 {
        self.data_offset
    }
    /// Original form including unknown chunks and their exact padding bytes.
    pub fn as_bytes(&self) -> &'a [u8] {
        self.bytes
    }
    pub fn pcm(&self) -> Result<Pcm<'a>, Error> {
        Pcm::new(self.data, self.format)
    }
}
