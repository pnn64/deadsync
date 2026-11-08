//! Precision-preserving image storage shared by independent format crates.
//!
//! Samples are interleaved in channel order. Sub-byte samples are unpacked but
//! retain their original integer range; 16-bit samples use native-endian `u16`.
//! Indexed images retain palette indices. No implicit color conversion occurs.
#![forbid(unsafe_code)]

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum ColorType {
    Grayscale = 0,
    Rgb = 2,
    Indexed = 3,
    GrayscaleAlpha = 4,
    Rgba = 6,
}

impl ColorType {
    pub const fn channels(self) -> usize {
        match self {
            Self::Grayscale | Self::Indexed => 1,
            Self::Rgb => 3,
            Self::GrayscaleAlpha => 2,
            Self::Rgba => 4,
        }
    }

    pub const fn allows_depth(self, depth: u8) -> bool {
        match self {
            Self::Grayscale => matches!(depth, 1 | 2 | 4 | 8 | 16),
            Self::Indexed => matches!(depth, 1 | 2 | 4 | 8),
            _ => matches!(depth, 8 | 16),
        }
    }
}

impl TryFrom<u8> for ColorType {
    type Error = InvalidImage;
    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Self::Grayscale),
            2 => Ok(Self::Rgb),
            3 => Ok(Self::Indexed),
            4 => Ok(Self::GrayscaleAlpha),
            6 => Ok(Self::Rgba),
            _ => Err(InvalidImage),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Samples {
    U8(Vec<u8>),
    U16(Vec<u16>),
}

impl Samples {
    pub fn len(&self) -> usize {
        match self {
            Self::U8(v) => v.len(),
            Self::U16(v) => v.len(),
        }
    }
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Image {
    pub width: u32,
    pub height: u32,
    pub color_type: ColorType,
    pub bit_depth: u8,
    pub samples: Samples,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InvalidImage;

impl core::fmt::Display for InvalidImage {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("invalid image dimensions, format, sample count, or sample range")
    }
}
impl std::error::Error for InvalidImage {}

impl Image {
    pub fn validate(&self) -> Result<(), InvalidImage> {
        let count = (self.width as usize)
            .checked_mul(self.height as usize)
            .and_then(|v| v.checked_mul(self.color_type.channels()))
            .ok_or(InvalidImage)?;
        if self.width == 0
            || self.height == 0
            || !self.color_type.allows_depth(self.bit_depth)
            || count != self.samples.len()
        {
            return Err(InvalidImage);
        }
        match &self.samples {
            Samples::U16(_) if self.bit_depth == 16 => Ok(()),
            Samples::U8(v) if self.bit_depth <= 8 => {
                if self.bit_depth < 8 && v.iter().any(|&s| u16::from(s) >= 1 << self.bit_depth) {
                    Err(InvalidImage)
                } else {
                    Ok(())
                }
            }
            _ => Err(InvalidImage),
        }
    }
}
