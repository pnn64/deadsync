use crate::codec::Header;
use crate::{AncillaryChunk, ColorType, Error, FrameControl, Metadata, Transparency};
mod decode;
mod encode_rows;
mod read;
mod rows;
mod write;
pub use decode::StreamDecoder;
pub use read::decode_reader;
pub use write::{EncodeStats, encode_writer};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ImageInfo {
    pub width: u32,
    pub height: u32,
    pub color_type: ColorType,
    pub bit_depth: u8,
    pub interlaced: bool,
}
impl From<Header> for ImageInfo {
    fn from(h: Header) -> Self {
        Self {
            width: h.width,
            height: h.height,
            color_type: h.color,
            bit_depth: h.depth,
            interlaced: h.interlaced,
        }
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ImageKind {
    Default,
    AnimationFrame(u32),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RowSamples<'a> {
    U8(&'a [u8]),
    U16(&'a [u16]),
}

/// One reduced-image row. Adam7 pass numbers are 1..=7; plain images use 0.
/// Pixel `n` belongs at `(x+n*x_step,y)` in the current image or frame rectangle.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Row<'a> {
    pub image: ImageKind,
    pub pass: u8,
    pub x: u32,
    pub y: u32,
    pub x_step: u32,
    pub width: u32,
    pub samples: RowSamples<'a>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DecodeStatus {
    NeedInput,
    Event,
    Done,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DecodeProgress {
    pub consumed: usize,
    pub status: DecodeStatus,
}

/// Events borrow decoder-owned buffers until the next `process` call.
/// Rows are provisional until `ImageEnd` verifies the image's zlib stream and
/// all its data-chunk CRCs. `Done` completes all checks under the selected
/// policy; `AnimationError` explicitly reports any discarded APNG fault.
#[derive(Debug)]
pub enum DecodeEvent<'a> {
    Header(ImageInfo),
    Palette(&'a [[u8; 3]]),
    Transparency(&'a Transparency),
    Metadata {
        chunk: &'a AncillaryChunk,
        value: &'a Metadata,
    },
    Animation {
        frames: u32,
        plays: u32,
    },
    /// Animation playback must stop and cached frames be discarded. The static
    /// image remains provisional until its own `ImageEnd(Default)` event.
    AnimationError(&'a Error),
    FrameControl {
        index: u32,
        control: FrameControl,
        uses_default_image: bool,
    },
    Row(Row<'a>),
    ImageEnd(ImageKind),
}
