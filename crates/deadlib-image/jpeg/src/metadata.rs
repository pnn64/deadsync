//! JPEG application metadata views. Payloads are borrowed. Typed validation is
//! opt-in; pixel decoding preserves even
//! malformed optional application data for callers that need exact rewriting.
use crate::{CoefficientImage, Error, MetadataSegment, Result};

pub const ICC_CHUNK_BYTES: usize = 65519;
pub const MAX_ICC_BYTES: usize = ICC_CHUNK_BYTES * 255;
pub(crate) const ICC_IDENTIFIER: &[u8; 12] = b"ICC_PROFILE\0";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum DensityUnit {
    AspectRatio = 0,
    Inches = 1,
    Centimeters = 2,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Jfif<'a> {
    pub version: [u8; 2],
    pub unit: DensityUnit,
    pub x_density: u16,
    pub y_density: u16,
    pub thumbnail_width: u8,
    pub thumbnail_height: u8,
    pub thumbnail_rgb: &'a [u8],
}

impl<'a> Jfif<'a> {
    pub fn parse(data: &'a [u8]) -> Result<Self> {
        if data.len() < 14 || !data.starts_with(b"JFIF\0") {
            return Err(Error::Invalid("JFIF header"));
        }
        if data[5] != 1 {
            return Err(Error::Unsupported("JFIF major version"));
        }
        let unit = match data[7] {
            0 => DensityUnit::AspectRatio,
            1 => DensityUnit::Inches,
            2 => DensityUnit::Centimeters,
            _ => return Err(Error::Invalid("JFIF density unit")),
        };
        let result = Self {
            version: [data[5], data[6]],
            unit,
            x_density: u16::from_be_bytes([data[8], data[9]]),
            y_density: u16::from_be_bytes([data[10], data[11]]),
            thumbnail_width: data[12],
            thumbnail_height: data[13],
            thumbnail_rgb: &data[14..],
        };
        result.validate()?;
        Ok(result)
    }

    fn validate(&self) -> Result<()> {
        if self.version[0] != 1 {
            return Err(Error::Unsupported("JFIF major version"));
        }
        if self.x_density == 0
            || self.y_density == 0
            || ((self.thumbnail_width == 0) != (self.thumbnail_height == 0))
            || self.thumbnail_rgb.len()
                != self.thumbnail_width as usize * self.thumbnail_height as usize * 3
        {
            return Err(Error::Invalid("JFIF density or thumbnail"));
        }
        if self.thumbnail_rgb.len() > 65533 - 14 {
            return Err(Error::Invalid("JFIF thumbnail exceeds segment"));
        }
        Ok(())
    }

    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        self.validate()?;
        let mut bytes = Vec::new();
        bytes
            .try_reserve_exact(14 + self.thumbnail_rgb.len())
            .map_err(|_| Error::Allocation)?;
        bytes.extend_from_slice(b"JFIF\0");
        bytes.extend_from_slice(&self.version);
        bytes.push(self.unit as u8);
        bytes.extend_from_slice(&self.x_density.to_be_bytes());
        bytes.extend_from_slice(&self.y_density.to_be_bytes());
        bytes.extend_from_slice(&[self.thumbnail_width, self.thumbnail_height]);
        bytes.extend_from_slice(self.thumbnail_rgb);
        Ok(bytes)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Jfxx<'a> {
    /// Embedded JPEG bytes. Decode separately if the thumbnail is needed.
    Jpeg(&'a [u8]),
    Indexed {
        width: u8,
        height: u8,
        palette_rgb: &'a [u8; 768],
        indices: &'a [u8],
    },
    Rgb {
        width: u8,
        height: u8,
        pixels: &'a [u8],
    },
    Unknown {
        code: u8,
        data: &'a [u8],
    },
}

impl<'a> Jfxx<'a> {
    pub fn parse(data: &'a [u8]) -> Result<Self> {
        if data.len() < 6 || !data.starts_with(b"JFXX\0") {
            return Err(Error::Invalid("JFXX header"));
        }
        match data[5] {
            0x10 => {
                if data.len() < 8 || data[6..8] != [255, 216] {
                    return Err(Error::Invalid("JFXX JPEG thumbnail"));
                }
                Ok(Self::Jpeg(&data[6..]))
            }
            code @ (0x11 | 0x13) => {
                if data.len() < 8 || data[6] == 0 || data[7] == 0 {
                    return Err(Error::Invalid("JFXX thumbnail dimensions"));
                }
                let (width, height) = (data[6], data[7]);
                let pixels = width as usize * height as usize;
                if code == 0x11 {
                    if data.len() != 8 + 768 + pixels {
                        return Err(Error::Invalid("JFXX indexed thumbnail length"));
                    }
                    Ok(Self::Indexed {
                        width,
                        height,
                        palette_rgb: data[8..776].try_into().unwrap(),
                        indices: &data[776..],
                    })
                } else {
                    if data.len() != 8 + 3 * pixels {
                        return Err(Error::Invalid("JFXX RGB thumbnail length"));
                    }
                    Ok(Self::Rgb {
                        width,
                        height,
                        pixels: &data[8..],
                    })
                }
            }
            code => Ok(Self::Unknown {
                code,
                data: &data[6..],
            }),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Adobe {
    pub version: u16,
    pub flags0: u16,
    pub flags1: u16,
    /// 0 = no transform, 1 = YCbCr, 2 = YCCK. Unknown values are preserved.
    pub transform: u8,
}

impl Adobe {
    pub fn parse(data: &[u8]) -> Result<Self> {
        if data.len() < 12 || !data.starts_with(b"Adobe") {
            return Err(Error::Invalid("Adobe APP14 header"));
        }
        Ok(Self {
            version: u16::from_be_bytes([data[5], data[6]]),
            flags0: u16::from_be_bytes([data[7], data[8]]),
            flags1: u16::from_be_bytes([data[9], data[10]]),
            transform: data[11],
        })
    }
    pub fn to_bytes(self) -> [u8; 12] {
        let mut bytes = [0; 12];
        bytes[..5].copy_from_slice(b"Adobe");
        bytes[5..7].copy_from_slice(&self.version.to_be_bytes());
        bytes[7..9].copy_from_slice(&self.flags0.to_be_bytes());
        bytes[9..11].copy_from_slice(&self.flags1.to_be_bytes());
        bytes[11] = self.transform;
        bytes
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct IccChunk<'a> {
    pub sequence: u8,
    pub count: u8,
    pub data: &'a [u8],
}

impl<'a> MetadataSegment<'a> {
    pub fn jfif(&self) -> Result<Option<Jfif<'a>>> {
        if self.marker == 0xe0 && self.data.starts_with(b"JFIF\0") {
            Jfif::parse(self.data).map(Some)
        } else {
            Ok(None)
        }
    }
    pub fn jfxx(&self) -> Result<Option<Jfxx<'a>>> {
        if self.marker == 0xe0 && self.data.starts_with(b"JFXX\0") {
            Jfxx::parse(self.data).map(Some)
        } else {
            Ok(None)
        }
    }
    pub fn adobe(&self) -> Result<Option<Adobe>> {
        if self.marker == 0xee && self.data.starts_with(b"Adobe") {
            Adobe::parse(self.data).map(Some)
        } else {
            Ok(None)
        }
    }
    pub fn icc_chunk(&self) -> Result<Option<IccChunk<'a>>> {
        if !is_icc(self) {
            return Ok(None);
        }
        if self.data.len() < 14 {
            return Err(Error::Invalid("short ICC chunk header"));
        }
        if self.data.len() > 65533 {
            return Err(Error::Invalid("ICC chunk exceeds marker length"));
        }
        let sequence = self.data[12];
        let count = self.data[13];
        if sequence == 0 || count == 0 || sequence > count {
            return Err(Error::Invalid("ICC chunk sequence/count"));
        }
        Ok(Some(IccChunk {
            sequence,
            count,
            data: &self.data[14..],
        }))
    }
}

pub(crate) fn is_icc(segment: &MetadataSegment<'_>) -> bool {
    segment.marker == 0xe2 && segment.data.starts_with(ICC_IDENTIFIER)
}

/// Borrowed ordered ICC chunks. Building this view never copies profile data.
/// This validates JPEG embedding, independently from the profile's own syntax.
pub struct IccProfile<'a> {
    chunks: [Option<&'a [u8]>; 255],
    count: usize,
    len: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct IccHeader {
    pub size: u32,
    pub version: u32,
    pub device_class: [u8; 4],
    pub data_color_space: [u8; 4],
    pub connection_space: [u8; 4],
    pub rendering_intent: u32,
}

impl<'a> IccProfile<'a> {
    pub fn from_segments(
        segments: &[MetadataSegment<'a>],
        max_bytes: usize,
    ) -> Result<Option<Self>> {
        let mut result = Self {
            chunks: [None; 255],
            count: 0,
            len: 0,
        };
        for segment in segments {
            let Some(chunk) = segment.icc_chunk()? else {
                continue;
            };
            if segment.preceding_scans != 0 {
                return Err(Error::Invalid("ICC chunk after first scan"));
            }
            if result.count != 0 && result.count != chunk.count as usize {
                return Err(Error::Invalid("inconsistent ICC chunk count"));
            }
            result.count = chunk.count as usize;
            let slot = &mut result.chunks[chunk.sequence as usize - 1];
            if slot.is_some() {
                return Err(Error::Invalid("duplicate ICC chunk"));
            }
            result.len = result
                .len
                .checked_add(chunk.data.len())
                .ok_or(Error::LimitExceeded("ICC profile bytes"))?;
            if result.len > max_bytes || result.len > MAX_ICC_BYTES {
                return Err(Error::LimitExceeded("ICC profile bytes"));
            }
            *slot = Some(chunk.data);
        }
        if result.count == 0 {
            return Ok(None);
        }
        if result.chunks[..result.count].iter().any(Option::is_none) {
            return Err(Error::Invalid("missing ICC chunk"));
        }
        if result.len == 0 {
            return Err(Error::Invalid("empty ICC profile"));
        }
        Ok(Some(result))
    }
    pub fn len(&self) -> usize {
        self.len
    }
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
    pub fn chunks(&self) -> impl ExactSizeIterator<Item = &'a [u8]> + '_ {
        self.chunks[..self.count].iter().map(|s| s.unwrap())
    }
    pub fn contiguous(&self) -> Option<&'a [u8]> {
        if self.count == 1 {
            self.chunks[0]
        } else {
            None
        }
    }
    pub fn copy_to(&self, output: &mut [u8]) -> Result<()> {
        if output.len() != self.len {
            return Err(Error::Invalid("ICC destination length"));
        }
        let mut position = 0;
        for chunk in self.chunks() {
            output[position..position + chunk.len()].copy_from_slice(chunk);
            position += chunk.len();
        }
        Ok(())
    }
    pub fn to_vec(&self, max_bytes: usize) -> Result<Vec<u8>> {
        if self.len > max_bytes {
            return Err(Error::LimitExceeded("ICC destination bytes"));
        }
        let mut bytes = Vec::new();
        bytes
            .try_reserve_exact(self.len)
            .map_err(|_| Error::Allocation)?;
        for chunk in self.chunks() {
            bytes.extend_from_slice(chunk);
        }
        Ok(bytes)
    }
    /// Inspect the profile header without allocating even for split headers.
    /// This is not a color-management transform or full ICC tag validation.
    pub fn header(&self) -> Result<IccHeader> {
        if self.len < 128 {
            return Err(Error::Invalid("short ICC profile header"));
        }
        let mut header = [0u8; 128];
        let mut position = 0;
        for chunk in self.chunks() {
            let count = chunk.len().min(128 - position);
            header[position..position + count].copy_from_slice(&chunk[..count]);
            position += count;
            if position == 128 {
                break;
            }
        }
        let word = |offset| u32::from_be_bytes(header[offset..offset + 4].try_into().unwrap());
        if header[36..40] != *b"acsp" || word(0) as usize != self.len || word(64) > 3 {
            return Err(Error::Invalid("ICC profile header fields"));
        }
        Ok(IccHeader {
            size: word(0),
            version: word(8),
            device_class: header[12..16].try_into().unwrap(),
            data_color_space: header[16..20].try_into().unwrap(),
            connection_space: header[20..24].try_into().unwrap(),
            rendering_intent: word(64),
        })
    }
}

impl<'a> CoefficientImage<'a> {
    pub fn icc_profile(&self) -> Result<Option<IccProfile<'a>>> {
        IccProfile::from_segments(&self.metadata, self.limits.max_metadata_bytes)
    }
    pub fn jfif(&self) -> Result<Option<Jfif<'a>>> {
        for s in &self.metadata {
            if s.preceding_scans != 0 {
                continue;
            }
            if let Some(jfif) = s.jfif()? {
                return Ok(Some(jfif));
            }
        }
        Ok(None)
    }
    pub fn adobe(&self) -> Result<Option<Adobe>> {
        for s in self.metadata.iter().rev() {
            if s.preceding_scans != 0 {
                continue;
            }
            if let Some(adobe) = s.adobe()? {
                return Ok(Some(adobe));
            }
        }
        Ok(None)
    }
}
