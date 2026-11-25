use crate::{BOS, CONTINUED, EOS, Error, crc, reserve};
use std::io::Write;

pub(crate) const FIXED_HEADER_SIZE: usize = 27;
pub(crate) const MAX_HEADER_SIZE: usize = FIXED_HEADER_SIZE + 255;

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
        let bytes = self.bytes.first_chunk::<FIXED_HEADER_SIZE>().unwrap();
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
        if let [lace] = lacing {
            return usize::from(*lace != 255);
        }
        if lacing.len() < 8 {
            return lacing.iter().filter(|&&lace| lace < 255).count();
        }
        // A 255 lace needs 255 body bytes, so every lace in a smaller body terminates.
        if self.bytes.len() < FIXED_HEADER_SIZE + lacing.len() + 255 {
            return lacing.len();
        }
        // At maximum body size every lace continues a packet.
        if self.bytes.len() == FIXED_HEADER_SIZE + 256 * lacing.len() {
            return 0;
        }
        // Each of the at most 255 laces terminates at most one packet.
        usize::from(lacing.iter().map(|&lace| u8::from(lace != 255)).sum::<u8>())
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
    check_version_flags(input[4], input[5])?;
    Ok(27 + input[26] as usize)
}

#[inline]
pub(crate) fn check_version_flags(version: u8, flags: u8) -> Result<(), Error> {
    if version != 0 {
        return Err(Error::Invalid("unsupported stream structure version"));
    }
    if flags & !7 != 0 {
        return Err(Error::Invalid("reserved page flags"));
    }
    Ok(())
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
fn fragment_segment_count(lacing: &[u8], body_len: usize) -> usize {
    // The laces sum to the remaining body length.
    if body_len == lacing.len() * 255 {
        return lacing.len();
    }
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

impl Fragments<'_> {
    pub(crate) fn remaining(&self) -> (usize, usize) {
        (self.lacing.len(), self.body.len())
    }
}

impl<'a> Iterator for Fragments<'a> {
    type Item = Fragment<'a>;
    #[inline(always)]
    fn next(&mut self) -> Option<Self::Item> {
        let (rest, len, ends_packet) = match self.lacing {
            [] => return None,
            [len @ 0..=254, rest @ ..] => (rest, *len as usize, true),
            [255, len @ 0..=254, rest @ ..] => (rest, 255 + *len as usize, true),
            [255, 255, len @ 0..=254, rest @ ..] => (rest, 510 + *len as usize, true),
            // A maximum-sized body requires all 255 laces to be 255.
            _ if self.body.len() == 255 * 255 => (&[][..], 255 * 255, false),
            lacing => {
                let count = fragment_segment_count(lacing, self.body.len());
                let last = lacing[count - 1];
                // Every segment before the terminator is exactly 255 bytes.
                (
                    &lacing[count..],
                    (count - 1) * 255 + last as usize,
                    last < 255,
                )
            }
        };
        let (data, body) = self.body.split_at(len);
        self.body = body;
        self.lacing = rest;
        // Only the first fragment can continue a packet from the previous page.
        Some(Fragment {
            data,
            starts_packet: !core::mem::take(&mut self.continued),
            ends_packet,
        })
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
    let mut header = [0; MAX_HEADER_SIZE];
    let len = FIXED_HEADER_SIZE + lacing.len();
    header[..FIXED_HEADER_SIZE].copy_from_slice(&encoded_header(fields, lacing.len() as u8));
    header[FIXED_HEADER_SIZE..len].copy_from_slice(lacing);
    write_page_inner::<_, false>(writer, &mut header[..len], body, (0, 0))
}

// The prefix contains the header and laces.
pub(crate) fn write_page_inner<W: Write, const FIXED_SERIAL: bool>(
    mut writer: W,
    header: &mut [u8],
    body: &[u8],
    serial_crc: (u32, u32),
) -> Result<usize, Error> {
    let checksum = if FIXED_SERIAL
        && let [payload] = *body
        && let Ok(prefix) = <&[u8; 28]>::try_from(&*header)
    {
        crc::packet_byte_crc(serial_crc.1, prefix, payload)
    } else {
        // Keep long prefixes contiguous for the hardware fold.
        let prefix = if header.len() < 64 {
            if FIXED_SERIAL {
                crc::packet_page_crc(serial_crc.0, header)
            } else {
                crc::page_crc(header)
            }
        } else {
            header[22..26].fill(0);
            crc::update(0, header)
        };
        crc::update(prefix, body)
    };
    header[22..26].copy_from_slice(&checksum.to_le_bytes());
    writer.write_all(header)?;
    writer.write_all(body)?;
    Ok(header.len() + body.len())
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
