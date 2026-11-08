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
#[derive(Clone, Copy)]
pub struct Page<'a> {
    bytes: &'a [u8],
}
impl core::fmt::Debug for Page<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Page")
            .field("bytes", &self.bytes)
            .field("header", &self.header())
            .field("body_start", &self.body_start())
            .finish()
    }
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
        check_crc(bytes)?;
        Ok((Self::verified(bytes), len))
    }
    /// Only call on an exact slice with verified lacing length and CRC.
    /// Strict callers also validate version/flags; SyncedPage keeps that policy
    /// separate and never exposes an unconverted Page publicly.
    pub(crate) fn verified(bytes: &'a [u8]) -> Self {
        Self { bytes }
    }
    #[inline]
    pub fn header(self) -> PageHeader {
        let bytes = self.bytes;
        PageHeader {
            flags: bytes[5],
            granule_position: u64::from_le_bytes(bytes[6..14].try_into().unwrap()),
            serial: u32::from_le_bytes(bytes[14..18].try_into().unwrap()),
            sequence: u32::from_le_bytes(bytes[18..22].try_into().unwrap()),
        }
    }
    #[inline]
    fn body_start(self) -> usize {
        27 + self.bytes[26] as usize
    }
    pub fn bytes(self) -> &'a [u8] {
        self.bytes
    }
    pub fn lacing(self) -> &'a [u8] {
        &self.bytes[27..self.body_start()]
    }
    pub fn body(self) -> &'a [u8] {
        &self.bytes[self.body_start()..]
    }
    pub fn checksum(self) -> u32 {
        u32::from_le_bytes(self.bytes[22..26].try_into().unwrap())
    }
    #[inline]
    pub fn completed_packets(self) -> usize {
        let lacing = self.lacing();
        if lacing.len() < 8 {
            return lacing.iter().filter(|&&n| n < 255).count();
        }
        count_packets(lacing)
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
                self.header().is_continued()
            } else {
                self.lacing()[segment - 1] == 255
            },
        }
    }
}

// Keep the wide loop out of consumers that usually see only a few laces.
#[inline(never)]
fn count_packets(lacing: &[u8]) -> usize {
    let (words, tail) = lacing.as_chunks::<8>();
    let mut continued = 0u64;
    for word in words {
        let word = u64::from_le_bytes(*word);
        // A byte is 0xff exactly when its low seven bits plus one and
        // its original high bit both set the high bit. No byte carries.
        let flags =
            ((word & 0x7f7f_7f7f_7f7f_7f7f) + 0x0101_0101_0101_0101) & word & 0x8080_8080_8080_8080;
        continued += flags >> 7;
    }
    // At most 31 words: each byte lane and their total fit in one byte.
    let mut continued = (continued.wrapping_mul(0x0101_0101_0101_0101) >> 56) as usize;
    continued += tail.iter().filter(|&&n| n == 255).count();
    lacing.len() - continued
}

// The exact page length and strict header rules must already be verified.
pub(crate) fn check_crc(bytes: &[u8]) -> Result<(), Error> {
    let expected = u32::from_le_bytes(bytes[22..26].try_into().unwrap());
    let actual = crc::page_crc(bytes);
    if actual != expected {
        return Err(Error::Checksum { expected, actual });
    }
    Ok(())
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
    Ok(header + body_size(&input[27..header]))
}

#[inline]
pub(crate) fn body_size(lacing: &[u8]) -> usize {
    if lacing.len() < 8 {
        return lacing.iter().map(|&n| n as usize).sum();
    }
    body_size_words(lacing)
}

#[inline(never)]
fn body_size_words(lacing: &[u8]) -> usize {
    let (words, tail) = lacing.as_chunks::<8>();
    let mut sums = 0;
    for word in words {
        let word = u64::from_le_bytes(*word);
        sums += (word & 0x00ff_00ff_00ff_00ff) + ((word >> 8) & 0x00ff_00ff_00ff_00ff);
    }
    // A page has at most 255 laces, so even the full sum fits in 16 bits.
    (sums.wrapping_mul(0x0001_0001_0001_0001) >> 48) as usize
        + tail.iter().map(|&n| n as usize).sum::<usize>()
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

// Keep the wide scan out of packet loops; most fragments end in the first few laces.
#[inline(never)]
fn fragment_segment_count(lacing: &[u8]) -> usize {
    let (words, tail) = lacing.as_chunks::<8>();
    for (i, word) in words.iter().enumerate() {
        // Only 0xff continues a packet. The first differing bit identifies
        // the first terminating byte in little-endian order.
        let bits = !u64::from_le_bytes(*word);
        if bits != 0 {
            return i * 8 + bits.trailing_zeros() as usize / 8 + 1;
        }
    }
    lacing.len() - tail.len()
        + tail
            .iter()
            .position(|&n| n < 255)
            .map_or(tail.len(), |n| n + 1)
}

impl<'a> Iterator for Fragments<'a> {
    type Item = Fragment<'a>;
    #[inline(always)]
    fn next(&mut self) -> Option<Self::Item> {
        let &first = self.lacing.first()?;
        let (count, len, ends_packet) = if first < 255 {
            (1, first as usize, true)
        } else if let Some(&second) = self.lacing.get(1)
            && second < 255
        {
            (2, 255 + second as usize, true)
        } else if let Some(&third) = self.lacing.get(2)
            && third < 255
        {
            (3, 2 * 255 + third as usize, true)
        } else if let Some(&fourth) = self.lacing.get(3)
            && fourth < 255
        {
            (4, 3 * 255 + fourth as usize, true)
        } else if self.lacing.len() <= 4 {
            (self.lacing.len(), self.lacing.len() * 255, false)
        } else {
            let count = if self.lacing.len() < 8 {
                // The four inline probes already established this prefix is all 255.
                let tail = &self.lacing[4..];
                4 + tail
                    .iter()
                    .position(|&n| n < 255)
                    .map_or(tail.len(), |n| n + 1)
            } else {
                fragment_segment_count(self.lacing)
            };
            let last = self.lacing[count - 1];
            // Every segment before the terminator is exactly 255 bytes.
            (count, (count - 1) * 255 + last as usize, last < 255)
        };
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

#[inline]
fn validate_page(fields: PageHeader, lacing: &[u8], body_length: usize) -> Result<(), Error> {
    if fields.flags & !7 != 0 {
        return Err(Error::Invalid("reserved page flags"));
    }
    if lacing.len() > 255 {
        return Err(Error::Invalid("more than 255 page segments"));
    }
    if body_size(lacing) != body_length {
        return Err(Error::Invalid("lacing/body size mismatch"));
    }
    Ok(())
}

#[inline]
fn encoded_header(fields: PageHeader, segments: u8) -> [u8; 27] {
    let mut header = [0; 27];
    header[..4].copy_from_slice(b"OggS");
    header[5] = fields.flags;
    header[6..14].copy_from_slice(&fields.granule_position.to_le_bytes());
    header[14..18].copy_from_slice(&fields.serial.to_le_bytes());
    header[18..22].copy_from_slice(&fields.sequence.to_le_bytes());
    header[26] = segments;
    header
}

/// Write a page without copying its payload. Handles partial/Interrupted IO.
/// This is a framing primitive; it does not validate logical stream grammar.
pub fn write_page(
    writer: impl Write,
    fields: PageHeader,
    lacing: &[u8],
    body: &[u8],
) -> Result<usize, Error> {
    validate_page(fields, lacing, body.len())?;
    write_page_inner(writer, fields, lacing, body)
}

// The caller has already checked flags, segment count and lacing/body length.
pub(crate) fn write_page_inner(
    mut writer: impl Write,
    fields: PageHeader,
    lacing: &[u8],
    body: &[u8],
) -> Result<usize, Error> {
    let mut header = [0; 282];
    let len = 27 + lacing.len();
    header[..27].copy_from_slice(&encoded_header(fields, lacing.len() as u8));
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
    validate_page(fields, lacing, body.len())?;
    out.extend_from_slice(&encoded_header(fields, lacing.len() as u8));
    out.extend_from_slice(lacing);
    out.extend_from_slice(body);
    let checksum = crc::update(0, &out);
    out[22..26].copy_from_slice(&checksum.to_le_bytes());
    Ok(out)
}
