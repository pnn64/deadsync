//! Bounded TIFF views for Exif APP1 payloads. Unknown tags remain borrowed;
//! this module does not decode TIFF images or automatically reorient pixels.
use crate::{CoefficientImage, Error, MetadataSegment, Result};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ByteOrder {
    Little,
    Big,
}
impl ByteOrder {
    fn u16(self, b: &[u8]) -> u16 {
        let bytes = [b[0], b[1]];
        match self {
            Self::Little => u16::from_le_bytes(bytes),
            Self::Big => u16::from_be_bytes(bytes),
        }
    }
    fn u32(self, b: &[u8]) -> u32 {
        let bytes = [b[0], b[1], b[2], b[3]];
        match self {
            Self::Little => u32::from_le_bytes(bytes),
            Self::Big => u32::from_be_bytes(bytes),
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Exif<'a> {
    tiff: &'a [u8],
    pub byte_order: ByteOrder,
    pub first_ifd_offset: u32,
}
#[derive(Clone, Copy, Debug)]
pub struct ExifIfd<'a> {
    exif: Exif<'a>,
    entries: &'a [u8],
    pub next_offset: u32,
}
#[derive(Clone, Copy, Debug)]
pub struct ExifEntry<'a> {
    exif: Exif<'a>,
    raw: &'a [u8; 12],
    pub tag: u16,
    pub field_type: u16,
    pub count: u32,
}

impl<'a> Exif<'a> {
    pub fn parse(data: &'a [u8]) -> Result<Self> {
        if data.len() < 14 || !data.starts_with(b"Exif\0\0") {
            return Err(Error::Invalid("Exif TIFF header"));
        }
        let tiff = &data[6..];
        let byte_order = match &tiff[..2] {
            b"II" => ByteOrder::Little,
            b"MM" => ByteOrder::Big,
            _ => return Err(Error::Invalid("Exif byte order")),
        };
        if byte_order.u16(&tiff[2..4]) != 42 {
            return Err(Error::Invalid("Exif TIFF magic"));
        }
        let result = Self {
            tiff,
            byte_order,
            first_ifd_offset: byte_order.u32(&tiff[4..8]),
        };
        result.ifd(result.first_ifd_offset)?;
        Ok(result)
    }
    pub fn tiff_bytes(self) -> &'a [u8] {
        self.tiff
    }
    pub fn ifd(self, offset: u32) -> Result<ExifIfd<'a>> {
        let offset = offset as usize;
        if offset < 8 {
            return Err(Error::Invalid("Exif IFD offset"));
        }
        let count_bytes = self
            .tiff
            .get(
                offset
                    ..offset
                        .checked_add(2)
                        .ok_or(Error::Invalid("Exif IFD range"))?,
            )
            .ok_or(Error::Invalid("Exif IFD range"))?;
        let count = self.byte_order.u16(count_bytes) as usize;
        let end = offset
            .checked_add(2 + count * 12)
            .ok_or(Error::Invalid("Exif IFD range"))?;
        let tail = self
            .tiff
            .get(end..end.checked_add(4).ok_or(Error::Invalid("Exif IFD range"))?)
            .ok_or(Error::Invalid("Exif IFD range"))?;
        Ok(ExifIfd {
            exif: self,
            entries: &self.tiff[offset + 2..end],
            next_offset: self.byte_order.u32(tail),
        })
    }
    pub fn orientation(self) -> Result<Option<u16>> {
        self.ifd(self.first_ifd_offset)?
            .find(0x0112)?
            .map(|entry| {
                if entry.field_type != 3 || entry.count != 1 {
                    return Err(Error::Invalid("Exif orientation type/count"));
                }
                let value = self.byte_order.u16(entry.value_bytes()?);
                if !(1..=8).contains(&value) {
                    return Err(Error::Invalid("Exif orientation value"));
                }
                Ok(value)
            })
            .transpose()
    }
    /// Borrow an IFD1 JPEG thumbnail. Only the first two IFDs are visited, so
    /// cyclic/unbounded pointer chains cannot cause recursive parser work.
    pub fn jpeg_thumbnail(self) -> Result<Option<&'a [u8]>> {
        let first = self.ifd(self.first_ifd_offset)?;
        if first.next_offset == 0 {
            return Ok(None);
        }
        if first.next_offset == self.first_ifd_offset {
            return Err(Error::Invalid("cyclic Exif IFD"));
        }
        let ifd = self.ifd(first.next_offset)?;
        let start = ifd.find(0x0201)?;
        let length = ifd.find(0x0202)?;
        match (start, length) {
            (None, None) => Ok(None),
            (Some(start), Some(length)) => {
                let start = start.scalar_u32()? as usize;
                let length = length.scalar_u32()? as usize;
                let end = start
                    .checked_add(length)
                    .ok_or(Error::Invalid("Exif thumbnail range"))?;
                Ok(Some(
                    self.tiff
                        .get(start..end)
                        .ok_or(Error::Invalid("Exif thumbnail range"))?,
                ))
            }
            _ => Err(Error::Invalid("incomplete Exif thumbnail tags")),
        }
    }
}
impl<'a> ExifIfd<'a> {
    pub fn entries(self) -> impl ExactSizeIterator<Item = ExifEntry<'a>> {
        self.entries
            .as_chunks::<12>()
            .0
            .iter()
            .map(move |raw| ExifEntry {
                exif: self.exif,
                raw,
                tag: self.exif.byte_order.u16(&raw[..2]),
                field_type: self.exif.byte_order.u16(&raw[2..4]),
                count: self.exif.byte_order.u32(&raw[4..8]),
            })
    }
    pub fn find(self, tag: u16) -> Result<Option<ExifEntry<'a>>> {
        let mut found = None;
        for entry in self.entries() {
            if entry.tag == tag {
                if found.is_some() {
                    return Err(Error::Invalid("duplicate Exif tag"));
                }
                found = Some(entry);
            }
        }
        Ok(found)
    }
}
impl<'a> ExifEntry<'a> {
    pub fn value_bytes(self) -> Result<&'a [u8]> {
        let size = match self.field_type {
            1 | 2 | 6 | 7 => 1,
            3 | 8 => 2,
            4 | 9 | 11 | 13 => 4,
            5 | 10 | 12 => 8,
            _ => return Err(Error::Unsupported("Exif TIFF field type")),
        };
        let length = (self.count as usize)
            .checked_mul(size)
            .ok_or(Error::Invalid("Exif entry size"))?;
        if length <= 4 {
            return Ok(&self.raw[8..8 + length]);
        }
        let offset = self.exif.byte_order.u32(&self.raw[8..12]) as usize;
        if offset < 8 {
            return Err(Error::Invalid("Exif value offset"));
        }
        let end = offset
            .checked_add(length)
            .ok_or(Error::Invalid("Exif value range"))?;
        self.exif
            .tiff
            .get(offset..end)
            .ok_or(Error::Invalid("Exif value range"))
    }
    pub fn scalar_u32(self) -> Result<u32> {
        if self.count != 1 {
            return Err(Error::Invalid("Exif scalar count"));
        }
        let bytes = self.value_bytes()?;
        match self.field_type {
            3 => Ok(u32::from(self.exif.byte_order.u16(bytes))),
            4 | 13 => Ok(self.exif.byte_order.u32(bytes)),
            _ => Err(Error::Invalid("Exif scalar type")),
        }
    }
}
impl<'a> MetadataSegment<'a> {
    pub fn exif(&self) -> Result<Option<Exif<'a>>> {
        if self.marker == 0xe1 && self.data.starts_with(b"Exif\0\0") {
            Exif::parse(self.data).map(Some)
        } else {
            Ok(None)
        }
    }
}
impl<'a> CoefficientImage<'a> {
    pub fn exif(&self) -> Result<Option<Exif<'a>>> {
        for segment in &self.metadata {
            if let Some(exif) = segment.exif()? {
                return Ok(Some(exif));
            }
        }
        Ok(None)
    }
}
