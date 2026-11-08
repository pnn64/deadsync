//! Borrowed ID3 and VBR metadata. Unknown ID3 frames remain available verbatim.
use crate::{Error, FrameHeader, Limits, reserve};
use std::borrow::Cow;

fn take<'a>(data: &'a [u8], position: &mut usize, bytes: usize) -> Result<&'a [u8], Error> {
    let end = position.checked_add(bytes).ok_or(Error::Truncated)?;
    let value = data.get(*position..end).ok_or(Error::Truncated)?;
    *position = end;
    Ok(value)
}
fn be32(data: &[u8], position: &mut usize) -> Result<u32, Error> {
    Ok(u32::from_be_bytes(
        take(data, position, 4)?.try_into().unwrap(),
    ))
}
fn be16(data: &[u8], position: &mut usize) -> Result<u16, Error> {
    Ok(u16::from_be_bytes(
        take(data, position, 2)?.try_into().unwrap(),
    ))
}
fn synchsafe(data: &[u8]) -> Result<usize, Error> {
    let mut value = 0;
    for &byte in data {
        if byte & 128 != 0 {
            return Err(Error::Invalid("ID3 synchsafe integer"));
        }
        value = (value << 7) | byte as usize;
    }
    Ok(value)
}
pub fn deunsynchronize(data: &[u8]) -> Result<Cow<'_, [u8]>, Error> {
    if !data.windows(2).any(|w| w == [0xff, 0]) {
        return Ok(Cow::Borrowed(data));
    }
    let mut result = Vec::new();
    reserve(&mut result, data.len())?;
    let mut position = 0;
    while position < data.len() {
        let byte = data[position];
        result.push(byte);
        position += 1;
        if byte == 0xff && data.get(position) == Some(&0) {
            position += 1;
        }
    }
    Ok(Cow::Owned(result))
}

#[derive(Clone, Copy, Debug)]
pub struct Id3Tag<'a> {
    pub version: u8,
    pub revision: u8,
    pub flags: u8,
    pub payload: &'a [u8],
    pub bytes: usize,
}
impl<'a> Id3Tag<'a> {
    /// Validates the ten-byte header and returns its complete on-disk size,
    /// including a v2.4 footer. The payload need not be buffered yet.
    pub fn encoded_len(data: &[u8], limits: Limits) -> Result<usize, Error> {
        let header = data.get(..10).ok_or(Error::Truncated)?;
        if &header[..3] != b"ID3" {
            return Err(Error::Invalid("ID3 signature"));
        }
        let version = header[3];
        if !(2..=4).contains(&version) || header[4] == 255 {
            return Err(Error::Unsupported("ID3 version"));
        }
        let flags = header[5];
        let mask = match version {
            2 => 0xc0,
            3 => 0xe0,
            _ => 0xf0,
        };
        if flags & !mask != 0 {
            return Err(Error::Invalid("ID3 flags"));
        }
        let length = synchsafe(&header[6..10])?;
        let footer = version == 4 && flags & 0x10 != 0;
        let bytes = 10 + length + usize::from(footer) * 10;
        if bytes > limits.metadata_bytes {
            return Err(Error::LimitExceeded("ID3 bytes"));
        }
        Ok(bytes)
    }
    pub fn parse(data: &'a [u8], limits: Limits) -> Result<Self, Error> {
        let bytes = Self::encoded_len(data, limits)?;
        let header = &data[..10];
        let version = header[3];
        let flags = header[5];
        let footer = version == 4 && flags & 0x10 != 0;
        let length = bytes - 10 - usize::from(footer) * 10;
        let tag = data.get(..bytes).ok_or(Error::Truncated)?;
        if footer {
            let tail = &tag[bytes - 10..];
            if &tail[..3] != b"3DI" || tail[3..] != header[3..] {
                return Err(Error::Invalid("ID3 footer"));
            }
        }
        Ok(Self {
            version,
            revision: header[4],
            flags,
            payload: &tag[10..10 + length],
            bytes,
        })
    }
    /// Frame walking deunsynchronizes an entire v2.2/v2.3 tag first. In v2.4
    /// on-disk frame lengths are used and payload unsynchronization stays explicit.
    pub fn frames(self) -> Result<Id3Frames<'a>, Error> {
        if self.version == 2 && self.flags & 0x40 != 0 {
            return Err(Error::Unsupported("compressed ID3v2.2 tag"));
        }
        let data = if self.version < 4 && self.flags & 0x80 != 0 {
            deunsynchronize(self.payload)?
        } else {
            Cow::Borrowed(self.payload)
        };
        let mut position = 0;
        if self.version >= 3 && self.flags & 0x40 != 0 {
            let size = take(&data, &mut position, 4)?;
            let bytes = if self.version == 4 {
                synchsafe(size)?
            } else {
                (u32::from_be_bytes(size.try_into().unwrap()) as usize)
                    .checked_add(4)
                    .ok_or(Error::Invalid("ID3 extended header size"))?
            };
            if bytes < if self.version == 4 { 6 } else { 10 } {
                return Err(Error::Invalid("ID3 extended header size"));
            }
            if bytes > data.len() {
                return Err(Error::Truncated);
            }
            position = bytes;
        }
        Ok(Id3Frames {
            data,
            position,
            version: self.version,
            unsynchronized: self.version == 4 && self.flags & 0x80 != 0,
        })
    }
}
pub struct Id3Frames<'a> {
    data: Cow<'a, [u8]>,
    position: usize,
    version: u8,
    unsynchronized: bool,
}
#[derive(Debug)]
pub struct Id3Frame<'a> {
    pub id: &'a [u8],
    pub flags: u16,
    pub payload: &'a [u8],
    pub unsynchronized: bool,
}
impl Id3Frames<'_> {
    /// The borrowed frame remains valid until the next mutable iterator operation.
    pub fn next_frame(&mut self) -> Result<Option<Id3Frame<'_>>, Error> {
        if self.position == self.data.len() {
            return Ok(None);
        }
        if self.data[self.position] == 0 {
            if self.data[self.position..].iter().any(|&b| b != 0) {
                return Err(Error::Invalid("nonzero ID3 padding"));
            }
            self.position = self.data.len();
            return Ok(None);
        }
        let id = take(
            &self.data,
            &mut self.position,
            if self.version == 2 { 3 } else { 4 },
        )?;
        if !id
            .iter()
            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
        {
            return Err(Error::Invalid("ID3 frame identifier"));
        }
        let size = take(
            &self.data,
            &mut self.position,
            if self.version == 2 { 3 } else { 4 },
        )?;
        let bytes = if self.version == 4 {
            synchsafe(size)?
        } else {
            size.iter().fold(0usize, |v, &b| (v << 8) | b as usize)
        };
        let flags = if self.version == 2 {
            0
        } else {
            be16(&self.data, &mut self.position)?
        };
        if (self.version == 3 && flags & !0xe0e0 != 0)
            || (self.version == 4 && flags & !0x704f != 0)
        {
            return Err(Error::Invalid("ID3 frame flags"));
        }
        let payload = take(&self.data, &mut self.position, bytes)?;
        Ok(Some(Id3Frame {
            id,
            flags,
            payload,
            unsynchronized: self.unsynchronized || (self.version == 4 && flags & 2 != 0),
        }))
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Id3v1<'a> {
    pub title: &'a [u8],
    pub artist: &'a [u8],
    pub album: &'a [u8],
    pub year: &'a [u8],
    pub comment: &'a [u8],
    pub track: Option<u8>,
    pub genre: u8,
}
impl<'a> Id3v1<'a> {
    pub fn parse(data: &'a [u8]) -> Option<Self> {
        let tail = data.get(data.len().checked_sub(128)?..)?;
        if &tail[..3] != b"TAG" {
            return None;
        }
        let track = (tail[125] == 0 && tail[126] != 0).then_some(tail[126]);
        Some(Self {
            title: &tail[3..33],
            artist: &tail[33..63],
            album: &tail[63..93],
            year: &tail[93..97],
            comment: &tail[97..if track.is_some() { 125 } else { 127 }],
            track,
            genre: tail[127],
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Gapless {
    pub encoder_delay: u16,
    pub encoder_padding: u16,
}
#[derive(Clone, Debug)]
pub struct Xing<'a> {
    pub cbr_info: bool,
    pub flags: u32,
    pub frames: Option<u32>,
    pub bytes: Option<u32>,
    pub toc: Option<&'a [u8; 100]>,
    pub quality: Option<u32>,
    pub encoder: Option<&'a [u8; 9]>,
    pub gapless: Option<Gapless>,
}
#[derive(Clone, Debug)]
pub struct Vbri<'a> {
    pub delay: u16,
    pub quality: u16,
    pub bytes: u32,
    pub frames: u32,
    pub scale: u16,
    pub entry_bytes: u8,
    pub frames_per_entry: u16,
    pub entries: &'a [u8],
}
impl Vbri<'_> {
    pub fn byte_offset(&self, frame: u32) -> Result<u64, Error> {
        if self.frames_per_entry == 0 || !(1..=4).contains(&self.entry_bytes) {
            return Err(Error::Invalid("VBRI table parameters"));
        }
        let count = (frame / self.frames_per_entry as u32) as usize;
        let mut offset = 0u64;
        for entry in self
            .entries
            .chunks_exact(self.entry_bytes as usize)
            .take(count)
        {
            let value = entry.iter().fold(0u64, |n, &b| (n << 8) | b as u64);
            offset = offset
                .checked_add(value * self.scale as u64)
                .ok_or(Error::Invalid("VBRI offset overflow"))?;
        }
        Ok(offset)
    }
}
#[derive(Clone, Debug)]
pub enum VbrHeader<'a> {
    Xing(Xing<'a>),
    Vbri(Vbri<'a>),
}
impl<'a> VbrHeader<'a> {
    pub fn parse(header: FrameHeader, data: &'a [u8]) -> Result<Option<Self>, Error> {
        // LAME's compatibility placement stays at header+side-info even with
        // protection. Also accept the natural CRC-aware ancillary-data offset.
        for offset in [4 + header.side_info_bytes(), header.main_data_offset()] {
            let Some(marker) = data.get(offset..offset + 4) else {
                continue;
            };
            if marker != b"Xing" && marker != b"Info" {
                continue;
            }
            let mut position = offset + 4;
            let flags = be32(data, &mut position)?;
            let frames = if flags & 1 != 0 {
                Some(be32(data, &mut position)?)
            } else {
                None
            };
            let bytes = if flags & 2 != 0 {
                Some(be32(data, &mut position)?)
            } else {
                None
            };
            let toc = if flags & 4 != 0 {
                Some(take(data, &mut position, 100)?.try_into().unwrap())
            } else {
                None
            };
            let quality = if flags & 8 != 0 {
                Some(be32(data, &mut position)?)
            } else {
                None
            };
            let encoder = data
                .get(position..position + 9)
                .map(|s| s.try_into().unwrap());
            let known = data
                .get(position..position + 4)
                .is_some_and(|s| s == b"LAME" || s == b"Lavc" || s == b"Lavf" || s == b"GOGO");
            let gapless = if known {
                data.get(position + 21..position + 24).map(|s| Gapless {
                    encoder_delay: (s[0] as u16) << 4 | (s[1] as u16) >> 4,
                    encoder_padding: (s[1] as u16 & 15) << 8 | s[2] as u16,
                })
            } else {
                None
            };
            return Ok(Some(Self::Xing(Xing {
                cbr_info: marker == b"Info",
                flags,
                frames,
                bytes,
                toc,
                quality,
                encoder,
                gapless,
            })));
        }
        if data.get(36..40) == Some(b"VBRI".as_slice()) {
            let mut position = 40;
            if be16(data, &mut position)? != 1 {
                return Err(Error::Unsupported("VBRI version"));
            }
            let delay = be16(data, &mut position)?;
            let quality = be16(data, &mut position)?;
            let bytes = be32(data, &mut position)?;
            let frames = be32(data, &mut position)?;
            let count = be16(data, &mut position)? as usize;
            let scale = be16(data, &mut position)?;
            let entry_bytes = be16(data, &mut position)?;
            let frames_per_entry = be16(data, &mut position)?;
            if !(1..=4).contains(&entry_bytes) || scale == 0 || frames_per_entry == 0 {
                return Err(Error::Invalid("VBRI table parameters"));
            }
            let entries = take(data, &mut position, count * entry_bytes as usize)?;
            return Ok(Some(Self::Vbri(Vbri {
                delay,
                quality,
                bytes,
                frames,
                scale,
                entry_bytes: entry_bytes as u8,
                frames_per_entry,
                entries,
            })));
        }
        Ok(None)
    }
}
