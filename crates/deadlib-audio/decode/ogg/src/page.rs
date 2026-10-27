use crate::{BOS, CONTINUED, EOS, Error, crc, reserve};
use std::io::Write;

/// Fields with codec-independent meaning. The granule is kept bit-for-bit.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PageHeader {
    pub flags: u8,
    pub granule_position: u64,
    pub serial: u32,
    pub sequence: u32,
}
impl PageHeader {
    pub fn is_bos(self) -> bool {
        self.flags & BOS != 0
    }
    pub fn is_eos(self) -> bool {
        self.flags & EOS != 0
    }
    pub fn is_continued(self) -> bool {
        self.flags & CONTINUED != 0
    }
}

/// A CRC-verified page borrowing the original contiguous bytes.
#[derive(Clone, Copy, Debug)]
pub struct Page<'a> {
    bytes: &'a [u8],
    header: PageHeader,
    body_start: usize,
}
impl<'a> Page<'a> {
    /// Parse one page, returning the number of bytes consumed. Trailing input
    /// is left to the caller. Stream grammar is checked by `Demuxer`.
    pub fn parse(input: &'a [u8]) -> Result<(Self, usize), Error> {
        let len = page_size(input)?;
        if input.len() < len {
            return Err(Error::Truncated);
        }
        let bytes = &input[..len];
        let expected = u32::from_le_bytes(bytes[22..26].try_into().unwrap());
        let actual = crc::page_crc(bytes);
        if actual != expected {
            return Err(Error::Checksum { expected, actual });
        }
        Ok((Self::verified(bytes), len))
    }
    /// Only call on an exact slice with verified lacing length and CRC.
    /// Strict callers also validate version/flags; SyncedPage keeps that policy
    /// separate and never exposes an unconverted Page publicly.
    pub(crate) fn verified(bytes: &'a [u8]) -> Self {
        Self {
            bytes,
            header: PageHeader {
                flags: bytes[5],
                granule_position: u64::from_le_bytes(bytes[6..14].try_into().unwrap()),
                serial: u32::from_le_bytes(bytes[14..18].try_into().unwrap()),
                sequence: u32::from_le_bytes(bytes[18..22].try_into().unwrap()),
            },
            body_start: 27 + bytes[26] as usize,
        }
    }
    pub fn header(self) -> PageHeader {
        self.header
    }
    pub fn bytes(self) -> &'a [u8] {
        self.bytes
    }
    pub fn lacing(self) -> &'a [u8] {
        &self.bytes[27..self.body_start]
    }
    pub fn body(self) -> &'a [u8] {
        &self.bytes[self.body_start..]
    }
    pub fn checksum(self) -> u32 {
        u32::from_le_bytes(self.bytes[22..26].try_into().unwrap())
    }
    pub fn completed_packets(self) -> usize {
        self.lacing().iter().filter(|&&n| n < 255).count()
    }
    pub fn fragments(self) -> Fragments<'a> {
        self.fragments_from(0, 0)
    }
    // Offsets are maintained only at validated fragment boundaries by capture.
    pub(crate) fn fragments_from(self, segment: usize, body: usize) -> Fragments<'a> {
        Fragments {
            lacing: &self.lacing()[segment..],
            body: &self.body()[body..],
            continued: if segment == 0 {
                self.header.is_continued()
            } else {
                self.lacing()[segment - 1] == 255
            },
        }
    }
}

pub(crate) fn header_size(input: &[u8]) -> Result<usize, Error> {
    if input.len() < 27 {
        return Err(Error::Truncated);
    }
    if &input[..4] != b"OggS" {
        return Err(Error::Invalid("capture pattern"));
    }
    if input[4] != 0 {
        return Err(Error::Invalid("unsupported stream structure version"));
    }
    if input[5] & !7 != 0 {
        return Err(Error::Invalid("reserved page flags"));
    }
    Ok(27 + input[26] as usize)
}
pub(crate) fn page_size(input: &[u8]) -> Result<usize, Error> {
    let header = header_size(input)?;
    if input.len() < header {
        return Err(Error::Truncated);
    }
    Ok(header + input[27..header].iter().map(|&n| n as usize).sum::<usize>())
}

/// A packet or part of a packet; a zero-length terminated fragment is valid.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Fragment<'a> {
    pub data: &'a [u8],
    pub starts_packet: bool,
    pub ends_packet: bool,
}
pub struct Fragments<'a> {
    lacing: &'a [u8],
    body: &'a [u8],
    continued: bool,
}
impl<'a> Iterator for Fragments<'a> {
    type Item = Fragment<'a>;
    fn next(&mut self) -> Option<Self::Item> {
        if self.lacing.is_empty() {
            return None;
        }
        let count = self
            .lacing
            .iter()
            .position(|&n| n < 255)
            .map_or(self.lacing.len(), |n| n + 1);
        let len = self.lacing[..count]
            .iter()
            .map(|&n| n as usize)
            .sum::<usize>();
        let ends_packet = self.lacing[count - 1] < 255;
        let fragment = Fragment {
            data: &self.body[..len],
            starts_packet: !self.continued,
            ends_packet,
        };
        self.lacing = &self.lacing[count..];
        self.body = &self.body[len..];
        self.continued = !ends_packet;
        Some(fragment)
    }
}

/// Write a page without copying its payload. Handles partial/Interrupted IO.
/// This is a framing primitive; it does not validate logical stream grammar.
pub fn write_page(
    mut writer: impl Write,
    fields: PageHeader,
    lacing: &[u8],
    body: &[u8],
) -> Result<usize, Error> {
    if fields.flags & !7 != 0 {
        return Err(Error::Invalid("reserved page flags"));
    }
    if lacing.len() > 255 {
        return Err(Error::Invalid("more than 255 page segments"));
    }
    if lacing.iter().map(|&n| n as usize).sum::<usize>() != body.len() {
        return Err(Error::Invalid("lacing/body size mismatch"));
    }
    let mut header = [0; 282];
    let len = 27 + lacing.len();
    header[..4].copy_from_slice(b"OggS");
    header[5] = fields.flags;
    header[6..14].copy_from_slice(&fields.granule_position.to_le_bytes());
    header[14..18].copy_from_slice(&fields.serial.to_le_bytes());
    header[18..22].copy_from_slice(&fields.sequence.to_le_bytes());
    header[26] = lacing.len() as u8;
    header[27..len].copy_from_slice(lacing);
    let checksum = crc::update(crc::update(0, &header[..len]), body);
    header[22..26].copy_from_slice(&checksum.to_le_bytes());
    writer.write_all(&header[..len])?;
    writer.write_all(body)?;
    Ok(len + body.len())
}

pub fn encode_page(fields: PageHeader, lacing: &[u8], body: &[u8]) -> Result<Vec<u8>, Error> {
    // Validate before allocating, including adversarial slice lengths.
    if lacing.len() > 255 || body.len() > 255 * 255 {
        return Err(Error::Invalid("page too large"));
    }
    let mut out = Vec::new();
    reserve(&mut out, 27 + lacing.len() + body.len())?;
    write_page(&mut out, fields, lacing, body)?;
    Ok(out)
}
