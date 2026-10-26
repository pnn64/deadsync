use crate::{
    Error, Limits,
    bitpack::{BitReader, BitWriter},
    reserve,
};

pub(crate) fn header(packet: &[u8], kind: u8) -> Result<&[u8], Error> {
    if packet.len() < 7 {
        return Err(Error::EndOfPacket);
    }
    if packet[0] != kind || &packet[1..7] != b"vorbis" {
        return Err(Error::Invalid("header type/signature"));
    }
    Ok(&packet[7..])
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Identification {
    pub channels: u8,
    pub sample_rate: u32,
    pub bitrate_maximum: i32,
    pub bitrate_nominal: i32,
    pub bitrate_minimum: i32,
    pub block_sizes: [usize; 2],
}
impl Identification {
    pub fn parse(packet: &[u8]) -> Result<Self, Error> {
        let mut bits = BitReader::new(header(packet, 1)?);
        if bits.read(32)? != 0 {
            return Err(Error::Invalid("Vorbis version"));
        }
        let channels = bits.read(8)? as u8;
        let sample_rate = bits.read(32)?;
        let bitrate_maximum = bits.read(32)? as i32;
        let bitrate_nominal = bits.read(32)? as i32;
        let bitrate_minimum = bits.read(32)? as i32;
        let block_sizes = [1usize << bits.read(4)?, 1usize << bits.read(4)?];
        if !bits.flag()? {
            return Err(Error::Invalid("identification framing bit"));
        }
        let value = Self {
            channels,
            sample_rate,
            bitrate_maximum,
            bitrate_nominal,
            bitrate_minimum,
            block_sizes,
        };
        value.validate()?;
        Ok(value)
    }
    pub(crate) fn validate(&self) -> Result<(), Error> {
        if self.channels == 0 || self.sample_rate == 0 {
            return Err(Error::Invalid("zero channels/sample rate"));
        }
        for &n in &self.block_sizes {
            if !(64..=8192).contains(&n) || !n.is_power_of_two() {
                return Err(Error::Invalid("block size"));
            }
        }
        if self.block_sizes[0] > self.block_sizes[1] {
            return Err(Error::Invalid("block size order"));
        }
        Ok(())
    }
    pub fn encode(&self) -> Result<Vec<u8>, Error> {
        self.validate()?;
        let mut bits = BitWriter::new();
        bits.write(0, 32)?;
        bits.write(self.channels as u32, 8)?;
        bits.write(self.sample_rate, 32)?;
        bits.write(self.bitrate_maximum as u32, 32)?;
        bits.write(self.bitrate_nominal as u32, 32)?;
        bits.write(self.bitrate_minimum as u32, 32)?;
        bits.write(self.block_sizes[0].trailing_zeros(), 4)?;
        bits.write(self.block_sizes[1].trailing_zeros(), 4)?;
        bits.write(1, 1)?;
        let mut packet = Vec::new();
        reserve(&mut packet, 30)?;
        packet.extend_from_slice(b"\x01vorbis");
        packet.extend_from_slice(&bits.finish()?);
        Ok(packet)
    }
}

/// Borrowed, eight-bit-clean metadata. Duplicate/unknown fields and their order
/// are preserved. UTF-8/field-name interpretation is an explicit accessor.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Comments<'a> {
    pub vendor: &'a [u8],
    pub comments: Vec<&'a [u8]>,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CommentField<'a> {
    pub name: &'a [u8],
    pub value: &'a [u8],
}
impl<'a> Comments<'a> {
    /// Strict metadata parse. A future stream decoder must treat malformed
    /// comment metadata as nonfatal, per Vorbis section 4.2 (unlike setup).
    pub fn parse(packet: &'a [u8], limits: &Limits) -> Result<Self, Error> {
        if packet.len() > limits.max_packet_bytes || packet.len() > limits.max_comment_bytes {
            return Err(Error::LimitExceeded("comment packet bytes"));
        }
        let mut input = header(packet, 3)?;
        let vendor = vector(&mut input)?;
        let count = uint(&mut input)? as usize;
        if count > limits.max_comments {
            return Err(Error::LimitExceeded("comment count"));
        }
        if count > input.len() / 4 {
            return Err(Error::EndOfPacket);
        }
        let mut comments = Vec::new();
        reserve(&mut comments, count)?;
        for _ in 0..count {
            comments.push(vector(&mut input)?);
        }
        if input.first().is_none_or(|n| n & 1 == 0) {
            return Err(Error::Invalid("comment framing bit"));
        }
        Ok(Self { vendor, comments })
    }
    pub fn fields(&self) -> impl Iterator<Item = CommentField<'a>> + '_ {
        self.comments.iter().filter_map(|field| {
            let equal = field.iter().position(|&n| n == b'=')?;
            let name = &field[..equal];
            if name.is_empty() || !name.iter().all(|n| (0x20..=0x7d).contains(n)) {
                return None;
            }
            Some(CommentField {
                name,
                value: &field[equal + 1..],
            })
        })
    }
    pub fn values<'s>(&'s self, name: &'s [u8]) -> impl Iterator<Item = &'a [u8]> + 's {
        self.fields()
            .filter(move |field| field.name.eq_ignore_ascii_case(name))
            .map(|field| field.value)
    }
    pub fn encode(&self, limits: &Limits) -> Result<Vec<u8>, Error> {
        if self.comments.len() > limits.max_comments {
            return Err(Error::LimitExceeded("comment count"));
        }
        let count = u32::try_from(self.comments.len())
            .map_err(|_| Error::LimitExceeded("comment count"))?;
        let mut size = 7usize + 4 + 4 + 1;
        size = size
            .checked_add(self.vendor.len())
            .ok_or(Error::LimitExceeded("comment packet bytes"))?;
        for comment in &self.comments {
            size = size
                .checked_add(4)
                .and_then(|n| n.checked_add(comment.len()))
                .ok_or(Error::LimitExceeded("comment packet bytes"))?;
        }
        if size > limits.max_comment_bytes || size > limits.max_packet_bytes {
            return Err(Error::LimitExceeded("comment packet bytes"));
        }
        let mut out = Vec::new();
        reserve(&mut out, size)?;
        out.extend_from_slice(b"\x03vorbis");
        append_vector(&mut out, self.vendor)?;
        out.extend_from_slice(&count.to_le_bytes());
        for comment in &self.comments {
            append_vector(&mut out, comment)?;
        }
        out.push(1);
        Ok(out)
    }
}
fn uint(input: &mut &[u8]) -> Result<u32, Error> {
    if input.len() < 4 {
        return Err(Error::EndOfPacket);
    }
    let value = u32::from_le_bytes(input[..4].try_into().unwrap());
    *input = &input[4..];
    Ok(value)
}
fn vector<'a>(input: &mut &'a [u8]) -> Result<&'a [u8], Error> {
    let len = uint(input)? as usize;
    if input.len() < len {
        return Err(Error::EndOfPacket);
    }
    let value = &input[..len];
    *input = &input[len..];
    Ok(value)
}
fn append_vector(output: &mut Vec<u8>, value: &[u8]) -> Result<(), Error> {
    let len =
        u32::try_from(value.len()).map_err(|_| Error::LimitExceeded("comment vector bytes"))?;
    output.extend_from_slice(&len.to_le_bytes());
    output.extend_from_slice(value);
    Ok(())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Channel {
    Mono,
    FrontLeft,
    FrontCenter,
    FrontRight,
    SideLeft,
    SideRight,
    RearLeft,
    RearCenter,
    RearRight,
    Lfe,
}

/// Vorbis mapping-zero channel order. More than eight channels have no
/// assigned speaker locations; return None without inventing a layout.
pub fn channel_order(channels: u8) -> Option<&'static [Channel]> {
    use Channel::*;
    Some(match channels {
        1 => &[Mono],
        2 => &[FrontLeft, FrontRight],
        3 => &[FrontLeft, FrontCenter, FrontRight],
        4 => &[FrontLeft, FrontRight, RearLeft, RearRight],
        5 => &[FrontLeft, FrontCenter, FrontRight, RearLeft, RearRight],
        6 => &[FrontLeft, FrontCenter, FrontRight, RearLeft, RearRight, Lfe],
        7 => &[
            FrontLeft,
            FrontCenter,
            FrontRight,
            SideLeft,
            SideRight,
            RearCenter,
            Lfe,
        ],
        8 => &[
            FrontLeft,
            FrontCenter,
            FrontRight,
            SideLeft,
            SideRight,
            RearLeft,
            RearRight,
            Lfe,
        ],
        _ => return None,
    })
}
