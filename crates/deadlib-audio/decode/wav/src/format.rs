use crate::Error;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Endian {
    Little,
    Big,
}
impl Endian {
    pub(crate) fn u16(self, b: &[u8]) -> u16 {
        let a = [b[0], b[1]];
        match self {
            Self::Little => u16::from_le_bytes(a),
            Self::Big => u16::from_be_bytes(a),
        }
    }
    pub(crate) fn u32(self, b: &[u8]) -> u32 {
        let a = [b[0], b[1], b[2], b[3]];
        match self {
            Self::Little => u32::from_le_bytes(a),
            Self::Big => u32::from_be_bytes(a),
        }
    }
    pub(crate) fn u64(self, b: &[u8]) -> u64 {
        let a: [u8; 8] = b[..8].try_into().unwrap();
        match self {
            Self::Little => u64::from_le_bytes(a),
            Self::Big => u64::from_be_bytes(a),
        }
    }
    pub(crate) fn native(self) -> bool {
        matches!(
            (self, cfg!(target_endian = "little")),
            (Self::Little, true) | (Self::Big, false)
        )
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SampleFormat {
    U8,
    I16,
    I24,
    I32,
    F32,
    F64,
}
impl SampleFormat {
    pub const fn bits(self) -> u16 {
        match self {
            Self::U8 => 8,
            Self::I16 => 16,
            Self::I24 => 24,
            Self::I32 | Self::F32 => 32,
            Self::F64 => 64,
        }
    }
    pub const fn is_float(self) -> bool {
        matches!(self, Self::F32 | Self::F64)
    }
}

/// The container recognizes encoded formats without depending on their decoders.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Encoding {
    Pcm,
    Float,
    MsAdpcm,
    ImaAdpcm,
    Other(u16),
    Guid([u8; 16]),
}

#[derive(Clone, Copy, Debug)]
pub struct Format<'a> {
    pub tag: u16,
    pub encoding: Encoding,
    pub channels: u16,
    pub sample_rate: u32,
    pub byte_rate: u32,
    pub block_align: u16,
    pub bits_per_sample: u16,
    /// Native integer valid bits are left justified within the sample container.
    /// For an extensible block codec this is the raw Samples union instead:
    /// its samples-per-block value is interpreted by the individual codec.
    pub valid_bits: u16,
    /// Zero means unspecified; otherwise channels follow ascending mask bits.
    pub channel_mask: u32,
    pub endian: Endian,
    pub extra: &'a [u8],
    pub raw: &'a [u8],
}
impl<'a> Format<'a> {
    pub fn parse(b: &'a [u8], e: Endian) -> Result<Self, Error> {
        if b.len() < 16 {
            return Err(Error::Truncated);
        }
        let tag = e.u16(b);
        let mut f = Self {
            tag,
            encoding: encoding(tag),
            channels: e.u16(&b[2..]),
            sample_rate: e.u32(&b[4..]),
            byte_rate: e.u32(&b[8..]),
            block_align: e.u16(&b[12..]),
            bits_per_sample: e.u16(&b[14..]),
            valid_bits: e.u16(&b[14..]),
            channel_mask: 0,
            endian: e,
            extra: &[],
            raw: b,
        };
        if b.len() != 16 {
            if b.len() < 18 {
                return Err(Error::Truncated);
            }
            let n = e.u16(&b[16..]) as usize;
            f.extra = b.get(18..18 + n).ok_or(Error::Truncated)?;
        } else if tag != 1 && tag != 3 {
            return Err(Error::Invalid("encoded format lacks cbSize"));
        }
        if tag == 0xfffe {
            if f.extra.len() < 22 {
                return Err(Error::Invalid("short extensible format"));
            }
            f.valid_bits = e.u16(f.extra);
            f.channel_mask = e.u32(&f.extra[2..]);
            let guid: [u8; 16] = f.extra[6..22].try_into().unwrap();
            // GUID fields Data1/Data2/Data3 follow container byte order, Data4 is bytes.
            if e.u16(&guid[4..]) == 0
                && e.u16(&guid[6..]) == 0x10
                && guid[8..] == [0x80, 0, 0, 0xaa, 0, 0x38, 0x9b, 0x71]
                && e.u32(&guid) <= u16::MAX as u32
            {
                f.encoding = encoding(e.u32(&guid) as u16);
            } else {
                f.encoding = Encoding::Guid(guid);
            }
        }
        if f.channels == 0 || f.sample_rate == 0 || f.block_align == 0 {
            return Err(Error::Invalid(
                "zero channels, sample rate or block alignment",
            ));
        }
        if f.channel_mask != 0
            && f.channel_mask != 0x8000_0000
            && f.channel_mask.count_ones() != u32::from(f.channels)
        {
            return Err(Error::Invalid("channel mask count"));
        }
        if matches!(f.encoding, Encoding::Pcm | Encoding::Float) {
            f.sample_format()?;
            if f.valid_bits == 0 || f.valid_bits > f.bits_per_sample {
                return Err(Error::Invalid("valid bits outside sample container"));
            }
            if f.encoding == Encoding::Float && f.valid_bits != f.bits_per_sample {
                return Err(Error::Invalid("floating-point valid bits"));
            }
            let align = u32::from(f.channels) * u32::from(f.bits_per_sample / 8);
            if align != u32::from(f.block_align) {
                return Err(Error::Invalid("PCM block alignment"));
            }
            if f.sample_rate.checked_mul(align) != Some(f.byte_rate) {
                return Err(Error::Invalid("PCM byte rate"));
            }
        }
        Ok(f)
    }
    pub fn sample_format(self) -> Result<SampleFormat, Error> {
        match (self.encoding, self.bits_per_sample) {
            (Encoding::Pcm, 8) => Ok(SampleFormat::U8),
            (Encoding::Pcm, 16) => Ok(SampleFormat::I16),
            (Encoding::Pcm, 24) => Ok(SampleFormat::I24),
            (Encoding::Pcm, 32) => Ok(SampleFormat::I32),
            (Encoding::Float, 32) => Ok(SampleFormat::F32),
            (Encoding::Float, 64) => Ok(SampleFormat::F64),
            (Encoding::Pcm | Encoding::Float, _) => {
                Err(Error::Unsupported("sample container width"))
            }
            _ => Err(Error::Unsupported("encoded audio requires its own decoder")),
        }
    }
}
fn encoding(tag: u16) -> Encoding {
    match tag {
        1 => Encoding::Pcm,
        3 => Encoding::Float,
        2 => Encoding::MsAdpcm,
        0x11 => Encoding::ImaAdpcm,
        _ => Encoding::Other(tag),
    }
}
