use crate::{ColorType, Error, Image, Samples};

pub type Palette = Vec<[u8; 3]>;
/// Original nonempty sub-blocks, including their boundaries (application data
/// can assign meaning to them). The zero terminator is implicit.
pub type SubBlocks = Vec<Vec<u8>>;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Version {
    Gif87a,
    #[default]
    Gif89a,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Header {
    pub version: Version,
    pub width: u16,
    pub height: u16,
    /// Bits per primary color in the original source, 1..=8.
    pub color_resolution: u8,
    pub global_palette: Option<Palette>,
    pub sorted: bool,
    pub background_index: u8,
    /// Zero means unspecified; otherwise pixel width/height = (value + 15)/64.
    pub pixel_aspect_ratio: u8,
}

impl Header {
    pub fn new(width: u16, height: u16) -> Self {
        Self {
            version: Version::Gif89a,
            width,
            height,
            color_resolution: 8,
            global_palette: None,
            sorted: false,
            background_index: 0,
            pixel_aspect_ratio: 0,
        }
    }
    pub fn aspect_ratio(&self) -> Option<f64> {
        (self.pixel_aspect_ratio != 0).then(|| (f64::from(self.pixel_aspect_ratio) + 15.0) / 64.0)
    }
    pub(crate) fn validate(&self) -> Result<(), Error> {
        if self.width == 0 || self.height == 0 || !(1..=8).contains(&self.color_resolution) {
            return Err(Error::Invalid(
                "logical screen dimensions or color resolution",
            ));
        }
        if let Some(p) = &self.global_palette {
            validate_palette(p)?;
            if usize::from(self.background_index) >= p.len() {
                return Err(Error::Invalid("background index outside global palette"));
            }
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[repr(u8)]
pub enum Disposal {
    #[default]
    Unspecified = 0,
    Keep = 1,
    Background = 2,
    Previous = 3,
    Reserved4 = 4,
    Reserved5 = 5,
    Reserved6 = 6,
    Reserved7 = 7,
}
impl Disposal {
    pub(crate) fn from_bits(bits: u8) -> Self {
        match bits & 7 {
            0 => Self::Unspecified,
            1 => Self::Keep,
            2 => Self::Background,
            3 => Self::Previous,
            4 => Self::Reserved4,
            5 => Self::Reserved5,
            6 => Self::Reserved6,
            _ => Self::Reserved7,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct GraphicControl {
    pub disposal: Disposal,
    /// Hundredths of a second, preserved including zero.
    pub delay: u16,
    pub transparent_index: Option<u8>,
    pub user_input: bool,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Frame {
    pub left: u16,
    pub top: u16,
    pub width: u16,
    pub height: u16,
    pub interlaced: bool,
    pub sorted: bool,
    pub local_palette: Option<Palette>,
    pub control: Option<GraphicControl>,
    /// Row-major, deinterlaced palette indices; never implicitly converted.
    pub indices: Vec<u8>,
}

impl Frame {
    pub fn new(width: u16, height: u16, indices: Vec<u8>) -> Self {
        Self {
            width,
            height,
            indices,
            ..Default::default()
        }
    }
    pub fn palette<'a>(&'a self, header: &'a Header) -> Option<&'a [[u8; 3]]> {
        self.local_palette
            .as_deref()
            .or(header.global_palette.as_deref())
    }
    /// Move native indices into the shared image representation; palette and
    /// GIF-specific control data remain separate.
    pub fn into_image(self) -> Image {
        Image {
            width: self.width.into(),
            height: self.height.into(),
            color_type: ColorType::Indexed,
            bit_depth: 8,
            samples: Samples::U8(self.indices),
        }
    }
    pub fn from_image(image: Image) -> Result<Self, Error> {
        image
            .validate()
            .map_err(|_| Error::Invalid("indexed image"))?;
        if image.color_type != ColorType::Indexed || image.width > 65535 || image.height > 65535 {
            return Err(Error::Invalid(
                "GIF requires indexed u8 images with u16 dimensions",
            ));
        }
        let Samples::U8(indices) = image.samples else {
            return Err(Error::Invalid("indexed samples"));
        };
        Ok(Self::new(image.width as u16, image.height as u16, indices))
    }
    pub(crate) fn validate(&self, header: &Header, bounds: bool) -> Result<(), Error> {
        validate_rect(self.left, self.top, self.width, self.height, header, bounds)?;
        if self.indices.len() != self.width as usize * self.height as usize {
            return Err(Error::Invalid("frame sample count"));
        }
        if let Some(p) = &self.local_palette {
            validate_palette(p)?;
        }
        if let Some(p) = self.palette(header) {
            validate_indices(&self.indices, p.len())?;
            validate_control(self.control, p.len())?;
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlainText {
    pub left: u16,
    pub top: u16,
    pub width: u16,
    pub height: u16,
    pub cell_width: u8,
    pub cell_height: u8,
    pub foreground_index: u8,
    pub background_index: u8,
    pub control: Option<GraphicControl>,
    pub data: SubBlocks,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Extension {
    Comment(SubBlocks),
    Application {
        identifier: [u8; 8],
        authentication: [u8; 3],
        data: SubBlocks,
    },
    PlainText(PlainText),
    Unknown {
        label: u8,
        /// A future graphic extension (label 0x00..=0x7f, except 0x3b)
        /// delimits control scope even when its payload is uninterpreted.
        control: Option<GraphicControl>,
        data: SubBlocks,
    },
}

pub(crate) fn is_graphic_label(label: u8) -> bool {
    label < 0x80 && label != 0x3b
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LoopCount {
    Infinite,
    /// Repetitions after the first playback, 1..=65535, as stored by NETSCAPE2.0.
    /// Zero normalizes to Infinite; omit the extension for a single playback.
    Finite(u16),
}

impl Extension {
    pub fn looping(count: LoopCount) -> Self {
        let n = match count {
            LoopCount::Infinite => 0,
            LoopCount::Finite(n) => n,
        };
        Self::Application {
            identifier: *b"NETSCAPE",
            authentication: *b"2.0",
            data: vec![vec![1, n as u8, (n >> 8) as u8]],
        }
    }
    pub fn loop_count(&self) -> Option<LoopCount> {
        let Self::Application {
            identifier,
            authentication,
            data,
        } = self
        else {
            return None;
        };
        if !((identifier == b"NETSCAPE" && authentication == b"2.0")
            || (identifier == b"ANIMEXTS" && authentication == b"1.0"))
        {
            return None;
        }
        data.iter().find_map(|block| {
            if block.len() != 3 || block[0] != 1 {
                return None;
            }
            let n = u16::from_le_bytes([block[1], block[2]]);
            Some(if n == 0 {
                LoopCount::Infinite
            } else {
                LoopCount::Finite(n)
            })
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Block {
    Frame(Frame),
    Extension(Extension),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Gif {
    pub header: Header,
    pub blocks: Vec<Block>,
}
impl Gif {
    pub fn new(header: Header) -> Self {
        Self {
            header,
            blocks: Vec::new(),
        }
    }
    pub fn frames(&self) -> impl Iterator<Item = &Frame> {
        self.blocks.iter().filter_map(|b| match b {
            Block::Frame(f) => Some(f),
            _ => None,
        })
    }
    /// The first recognized looping extension wins. Absence means one playback.
    pub fn loop_count(&self) -> Option<LoopCount> {
        self.blocks.iter().find_map(|b| match b {
            Block::Extension(e) => e.loop_count(),
            _ => None,
        })
    }
}

pub(crate) fn validate_palette(p: &[[u8; 3]]) -> Result<(), Error> {
    if !(2..=256).contains(&p.len()) || !p.len().is_power_of_two() {
        Err(Error::Invalid(
            "palette must have a power-of-two length in 2..=256",
        ))
    } else {
        Ok(())
    }
}
pub(crate) fn validate_indices(data: &[u8], len: usize) -> Result<(), Error> {
    if len < 256 && data.iter().any(|&v| v as usize >= len) {
        Err(Error::Invalid("pixel index outside active palette"))
    } else {
        Ok(())
    }
}
pub(crate) fn validate_control(control: Option<GraphicControl>, len: usize) -> Result<(), Error> {
    if control
        .and_then(|v| v.transparent_index)
        .is_some_and(|v| v as usize >= len)
    {
        Err(Error::Invalid("transparent index outside active palette"))
    } else {
        Ok(())
    }
}
pub(crate) fn validate_rect(
    left: u16,
    top: u16,
    width: u16,
    height: u16,
    h: &Header,
    bounds: bool,
) -> Result<(), Error> {
    if width == 0
        || height == 0
        || (bounds
            && (u32::from(left) + u32::from(width) > u32::from(h.width)
                || u32::from(top) + u32::from(height) > u32::from(h.height)))
    {
        Err(Error::Invalid(
            "graphic rectangle outside logical screen or empty",
        ))
    } else {
        Ok(())
    }
}
