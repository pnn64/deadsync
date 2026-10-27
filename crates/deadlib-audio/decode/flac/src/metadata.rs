//! Borrowed metadata keeps application, reserved blocks, and tag bytes intact.
use crate::{
    Error, Limits,
    bitstream::{BitReader, BitWriter},
    reserve,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StreamInfo {
    pub min_block_size: u16,
    pub max_block_size: u16,
    pub min_frame_size: u32,
    pub max_frame_size: u32,
    pub sample_rate: u32,
    pub channels: u8,
    pub bits_per_sample: u8,
    pub total_samples: u64,
    pub md5: [u8; 16],
}
impl StreamInfo {
    pub fn parse(data: &[u8]) -> Result<Self, Error> {
        if data.len() != 34 {
            return Err(Error::Invalid("STREAMINFO length"));
        }
        let mut r = BitReader::new(data);
        let info = Self {
            min_block_size: r.read(16)? as u16,
            max_block_size: r.read(16)? as u16,
            min_frame_size: r.read(24)? as u32,
            max_frame_size: r.read(24)? as u32,
            sample_rate: r.read(20)? as u32,
            channels: r.read(3)? as u8 + 1,
            bits_per_sample: r.read(5)? as u8 + 1,
            total_samples: r.read(36)?,
            md5: data[18..].try_into().unwrap(),
        };
        info.validate()?;
        Ok(info)
    }
    pub fn validate(&self) -> Result<(), Error> {
        if self.min_block_size < 16 || self.max_block_size < self.min_block_size {
            return Err(Error::Invalid("STREAMINFO block sizes"));
        }
        if self.sample_rate >= 1 << 20
            || !(1..=8).contains(&self.channels)
            || !(4..=32).contains(&self.bits_per_sample)
            || self.total_samples >= 1 << 36
        {
            return Err(Error::Invalid("STREAMINFO audio properties"));
        }
        if self.min_frame_size >= 1 << 24
            || self.max_frame_size >= 1 << 24
            || (self.max_frame_size != 0 && self.min_frame_size > self.max_frame_size)
        {
            return Err(Error::Invalid("STREAMINFO frame sizes"));
        }
        Ok(())
    }
    pub fn encode(&self) -> Result<[u8; 34], Error> {
        self.validate()?;
        let mut w = BitWriter::new();
        for (x, n) in [
            (self.min_block_size as u64, 16),
            (self.max_block_size as u64, 16),
            (self.min_frame_size as u64, 24),
            (self.max_frame_size as u64, 24),
            (self.sample_rate as u64, 20),
            (self.channels as u64 - 1, 3),
            (self.bits_per_sample as u64 - 1, 5),
            (self.total_samples, 36),
        ] {
            w.write(x, n)?;
        }
        let mut result = [0; 34];
        result[..18].copy_from_slice(w.as_bytes());
        result[18..].copy_from_slice(&self.md5);
        Ok(result)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MetadataBlock<'a> {
    pub kind: u8,
    pub data: &'a [u8],
}
impl<'a> MetadataBlock<'a> {
    pub fn parse(data: &'a [u8]) -> Result<(Self, bool, usize), Error> {
        let header = data.get(..4).ok_or(Error::Truncated)?;
        let kind = header[0] & 127;
        if kind == 127 {
            return Err(Error::Invalid("forbidden metadata type"));
        }
        let len =
            (usize::from(header[1]) << 16) | (usize::from(header[2]) << 8) | usize::from(header[3]);
        let body = data.get(4..4 + len).ok_or(Error::Truncated)?;
        Ok((Self { kind, data: body }, header[0] & 128 != 0, len + 4))
    }
    pub fn validate(&self) -> Result<(), Error> {
        match self.kind {
            0 => {
                StreamInfo::parse(self.data)?;
            }
            1 => {
                if self.data.iter().any(|&x| x != 0) {
                    return Err(Error::Invalid("nonzero metadata padding"));
                }
            }
            2 => {
                if self.data.len() < 4 {
                    return Err(Error::Invalid("application ID missing"));
                }
            }
            3 => {
                SeekTable::parse(self.data)?;
            }
            4 => {
                Comments::parse(self.data)?;
            }
            5 => {
                CueSheet::parse(self.data)?;
            }
            6 => {
                Picture::parse(self.data)?;
            }
            7..=126 => (),
            _ => return Err(Error::Invalid("metadata type")),
        }
        Ok(())
    }
    pub fn write(&self, last: bool, output: &mut Vec<u8>) -> Result<(), Error> {
        if self.kind >= 127 || self.data.len() >= 1 << 24 {
            return Err(Error::Invalid("metadata header"));
        }
        reserve(output, self.data.len() + 4)?;
        output.push(self.kind | if last { 128 } else { 0 });
        output.extend_from_slice(&(self.data.len() as u32).to_be_bytes()[1..]);
        output.extend_from_slice(self.data);
        Ok(())
    }
}

#[derive(Debug)]
pub struct Metadata<'a> {
    pub stream_info: StreamInfo,
    /// Includes STREAMINFO; the original ordering and all bytes are preserved.
    pub blocks: Vec<MetadataBlock<'a>>,
    pub audio_offset: usize,
}
impl<'a> Metadata<'a> {
    pub fn parse(data: &'a [u8], limits: Limits) -> Result<Self, Error> {
        if data.get(..4).ok_or(Error::Truncated)? != b"fLaC" {
            return Err(Error::Invalid("native signature"));
        }
        let mut pos = 4;
        let mut blocks = Vec::new();
        let mut seen = 0u8;
        let mut icons = 0u8;
        loop {
            if blocks.len() >= limits.metadata_blocks {
                return Err(Error::LimitExceeded("metadata blocks"));
            }
            // Check the declared length against the budget before waiting for its payload.
            let header = data.get(pos..pos + 4).ok_or(Error::Truncated)?;
            let len = u32::from_be_bytes([0, header[1], header[2], header[3]]) as usize;
            if pos - 4 + len + 4 > limits.metadata_bytes {
                return Err(Error::LimitExceeded("metadata bytes"));
            }
            let (block, last, used) = MetadataBlock::parse(&data[pos..])?;
            if blocks.is_empty() && block.kind != 0 {
                return Err(Error::Invalid("STREAMINFO must be first"));
            }
            if matches!(block.kind, 0 | 3 | 4 | 5) {
                let bit = 1 << block.kind;
                if seen & bit != 0 {
                    return Err(Error::Invalid("duplicate singleton metadata"));
                }
                seen |= bit;
            }
            block.validate()?;
            if block.kind == 6 {
                let kind = Picture::parse(block.data)?.kind;
                if kind == 1 || kind == 2 {
                    let bit = 1 << kind;
                    if icons & bit != 0 {
                        return Err(Error::Invalid("duplicate picture icon"));
                    }
                    icons |= bit;
                }
            }
            reserve(&mut blocks, 1)?;
            blocks.push(block);
            pos += used;
            if last {
                break;
            }
        }
        Ok(Self {
            stream_info: StreamInfo::parse(blocks[0].data)?,
            blocks,
            audio_offset: pos,
        })
    }
    pub fn encode(&self) -> Result<Vec<u8>, Error> {
        let mut result = b"fLaC".to_vec();
        for (i, block) in self.blocks.iter().enumerate() {
            block.write(i + 1 == self.blocks.len(), &mut result)?;
        }
        Ok(result)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SeekPoint {
    pub sample: u64,
    pub offset: u64,
    pub samples: u16,
}
#[derive(Clone, Copy, Debug)]
pub struct SeekTable<'a> {
    data: &'a [u8],
}
impl<'a> SeekTable<'a> {
    pub fn parse(data: &'a [u8]) -> Result<Self, Error> {
        if !data.len().is_multiple_of(18) {
            return Err(Error::Invalid("SEEKTABLE length"));
        }
        let result = Self { data };
        let mut prev = None;
        for point in result.points() {
            if prev.is_some_and(|v| point.sample <= v && point.sample != u64::MAX) {
                return Err(Error::Invalid("SEEKTABLE ordering"));
            }
            prev = Some(point.sample);
        }
        Ok(result)
    }
    pub fn points(&self) -> impl DoubleEndedIterator<Item = SeekPoint> + 'a {
        self.data.as_chunks::<18>().0.iter().map(|p| SeekPoint {
            sample: u64::from_be_bytes(p[..8].try_into().unwrap()),
            offset: u64::from_be_bytes(p[8..16].try_into().unwrap()),
            samples: u16::from_be_bytes(p[16..18].try_into().unwrap()),
        })
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Comments<'a> {
    pub vendor: &'a [u8],
    data: &'a [u8],
    count: u32,
}
impl<'a> Comments<'a> {
    pub fn parse(data: &'a [u8]) -> Result<Self, Error> {
        let mut r = Cursor(data);
        let len = r.le32()? as usize;
        let vendor = r.take(len)?;
        let count = r.le32()?;
        if count as usize > r.0.len() / 4 {
            return Err(Error::Truncated);
        }
        let fields = r.0;
        for _ in 0..count {
            let len = r.le32()? as usize;
            r.take(len)?;
        }
        r.end()?;
        // Keep eight-bit-clean tags from real-world files; interpretation is opt-in.
        Ok(Self {
            vendor,
            data: fields,
            count,
        })
    }
    pub fn fields(&self) -> impl Iterator<Item = &'a [u8]> {
        let mut r = Cursor(self.data);
        (0..self.count).map(move |_| {
            let len = r.le32().unwrap() as usize;
            r.take(len).unwrap()
        })
    }
    pub fn values<'s>(&'s self, name: &'s str) -> impl Iterator<Item = &'a [u8]> + 's {
        self.fields().filter_map(move |field| {
            let sep = field.iter().position(|&b| b == b'=')?;
            field[..sep]
                .eq_ignore_ascii_case(name.as_bytes())
                .then_some(&field[sep + 1..])
        })
    }
    pub fn channel_mask(&self) -> Option<u32> {
        let raw = self.values("WAVEFORMATEXTENSIBLE_CHANNEL_MASK").next()?;
        let text = std::str::from_utf8(raw).ok()?;
        let hex = text
            .strip_prefix("0x")
            .or_else(|| text.strip_prefix("0X"))?;
        u32::from_str_radix(hex, 16).ok()
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Picture<'a> {
    pub kind: u32,
    pub media_type: &'a [u8],
    pub description: &'a [u8],
    pub width: u32,
    pub height: u32,
    pub depth: u32,
    pub colors: u32,
    pub data: &'a [u8],
}
impl<'a> Picture<'a> {
    pub fn parse(data: &'a [u8]) -> Result<Self, Error> {
        let mut r = Cursor(data);
        let kind = r.be32()?;
        let n = r.be32()? as usize;
        let media_type = r.take(n)?;
        let n = r.be32()? as usize;
        let description = r.take(n)?;
        let width = r.be32()?;
        let height = r.be32()?;
        let depth = r.be32()?;
        let colors = r.be32()?;
        let n = r.be32()? as usize;
        let data = r.take(n)?;
        r.end()?;
        Ok(Self {
            kind,
            media_type,
            description,
            width,
            height,
            depth,
            colors,
            data,
        })
    }
    pub fn is_uri(&self) -> bool {
        self.media_type == b"-->"
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CueIndex {
    pub offset: u64,
    pub number: u8,
}
#[derive(Debug)]
pub struct CueTrack<'a> {
    pub offset: u64,
    pub number: u8,
    pub isrc: &'a [u8],
    pub non_audio: bool,
    pub pre_emphasis: bool,
    pub indices: Vec<CueIndex>,
}
#[derive(Debug)]
pub struct CueSheet<'a> {
    pub catalog: &'a [u8],
    pub lead_in: u64,
    pub is_cd: bool,
    pub tracks: Vec<CueTrack<'a>>,
}
impl<'a> CueSheet<'a> {
    pub fn parse(data: &'a [u8]) -> Result<Self, Error> {
        let mut r = Cursor(data);
        let catalog = r.take(128)?;
        let lead_in = r.be64()?;
        let flags = r.byte()?;
        if flags & 127 != 0 {
            return Err(Error::Invalid("cuesheet reserved bits"));
        }
        r.zeros(258)?;
        let is_cd = flags & 128 != 0;
        let count = r.byte()? as usize;
        if count == 0 || (is_cd && (count > 100 || lead_in < 88200 || !lead_in.is_multiple_of(588)))
        {
            return Err(Error::Invalid("cuesheet lead-in or track count"));
        }
        let mut tracks = Vec::new();
        reserve(&mut tracks, count)?;
        let mut seen = [false; 256];
        for i in 0..count {
            let offset = r.be64()?;
            let number = r.byte()?;
            let isrc = r.take(12)?;
            let flags = r.byte()?;
            if flags & 63 != 0 {
                return Err(Error::Invalid("cue track reserved bits"));
            }
            r.zeros(13)?;
            let n = r.byte()? as usize;
            let last = i + 1 == count;
            if number == 0
                || seen[number as usize]
                || (last && (number != if is_cd { 170 } else { 255 } || n != 0))
                || (!last && (n == 0 || (is_cd && (number > 99 || n > 100))))
                || (is_cd && !offset.is_multiple_of(588))
            {
                return Err(Error::Invalid("cue track"));
            }
            seen[number as usize] = true;
            let mut indices = Vec::new();
            reserve(&mut indices, n)?;
            let mut previous = None;
            for _ in 0..n {
                let offset = r.be64()?;
                let number = r.byte()?;
                r.zeros(3)?;
                if (is_cd && !offset.is_multiple_of(588))
                    || match previous {
                        None => number > 1,
                        Some(p) => u16::from(number) != p + 1,
                    }
                {
                    return Err(Error::Invalid("cue index"));
                }
                previous = Some(u16::from(number));
                indices.push(CueIndex { offset, number });
            }
            tracks.push(CueTrack {
                offset,
                number,
                isrc,
                non_audio: flags & 128 != 0,
                pre_emphasis: flags & 64 != 0,
                indices,
            });
        }
        r.end()?;
        Ok(Self {
            catalog,
            lead_in,
            is_cd,
            tracks,
        })
    }
}

struct Cursor<'a>(&'a [u8]);
impl<'a> Cursor<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], Error> {
        let result = self.0.get(..n).ok_or(Error::Truncated)?;
        self.0 = &self.0[n..];
        Ok(result)
    }
    fn byte(&mut self) -> Result<u8, Error> {
        Ok(self.take(1)?[0])
    }
    fn be32(&mut self) -> Result<u32, Error> {
        Ok(u32::from_be_bytes(self.take(4)?.try_into().unwrap()))
    }
    fn le32(&mut self) -> Result<u32, Error> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }
    fn be64(&mut self) -> Result<u64, Error> {
        Ok(u64::from_be_bytes(self.take(8)?.try_into().unwrap()))
    }
    fn zeros(&mut self, n: usize) -> Result<(), Error> {
        if self.take(n)?.iter().any(|&b| b != 0) {
            Err(Error::Invalid("metadata reserved bits"))
        } else {
            Ok(())
        }
    }
    fn end(&self) -> Result<(), Error> {
        if self.0.is_empty() {
            Ok(())
        } else {
            Err(Error::Invalid("metadata trailing bytes"))
        }
    }
}
