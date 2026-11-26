//! Dependency-free MPEG-4 Part 2 decoding for progressive 4:2:0 I and P pictures.
//!
//! Feed an elementary stream to [`decode_elementary_stream`] or an AVI file to
//! [`decode_avi`]. [`Decoder`] also accepts individual start-code units.

#![no_std]
#![forbid(unsafe_code)]

extern crate alloc;

mod avi;
mod bitstream;
mod block;
mod frame;
mod headers;
mod picture;
mod tables;
mod transform;

pub use avi::decode_avi;
pub use bitstream::{units, Unit};
pub use frame::{Frame, Plane};

use alloc::sync::Arc;
use bitstream::Bits;
use core::fmt;
use headers::{Layer, Picture};

pub type Result<T> = core::result::Result<T, Error>;

#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Error {
    Truncated,
    Invalid(&'static str),
    Unsupported(&'static str),
    MissingReference,
    NoVideo,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Truncated => f.write_str("truncated MPEG-4 or AVI data"),
            Self::Invalid(field) => write!(f, "invalid MPEG-4 or AVI syntax ({field})"),
            Self::Unsupported(feature) => {
                write!(f, "unsupported MPEG-4 or AVI feature ({feature})")
            }
            Self::MissingReference => f.write_str("missing MPEG-4 prediction reference"),
            Self::NoVideo => f.write_str("no MPEG-4 pictures found"),
        }
    }
}

impl core::error::Error for Error {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PictureKind {
    I,
    P,
}

/// A serial decoder for complete MPEG-4 Visual start-code units.
#[derive(Default)]
pub struct Decoder {
    layer: Option<Layer>,
    reference: Option<Arc<Frame>>,
}

impl Decoder {
    pub fn new() -> Self {
        Self::default()
    }

    /// Decodes a picture when the unit contains a coded VOP.
    pub fn push(&mut self, unit: Unit<'_>) -> Result<Option<Arc<Frame>>> {
        match unit.code {
            0x20..=0x2f => {
                let layer = Layer::parse(unit.payload)?;
                if self.reference.as_ref().is_some_and(|frame| {
                    (frame.width(), frame.height()) != (layer.width, layer.height)
                }) {
                    self.reference = None;
                }
                self.layer = Some(layer);
            }
            0xb6 => {
                let layer = self
                    .layer
                    .ok_or(Error::Invalid("missing video object layer"))?;
                let mut bits = Bits::new(unit.payload);
                let Some(header) = Picture::parse(&mut bits, layer)? else {
                    return Ok(None);
                };
                let frame = Arc::new(picture::decode(
                    &mut bits,
                    layer,
                    header,
                    self.reference.as_deref(),
                )?);
                self.reference = Some(Arc::clone(&frame));
                return Ok(Some(frame));
            }
            0xb1 => *self = Self::new(),
            0x00..=0x1f | 0xb0 | 0xb2 | 0xb3 | 0xb5 => {} // Object and sequence metadata.
            _ => return Err(Error::Unsupported("start code")),
        }
        Ok(None)
    }
}

/// Decodes a complete MPEG-4 Part 2 elementary stream.
pub fn decode_elementary_stream(bytes: &[u8], mut output: impl FnMut(Arc<Frame>)) -> Result<()> {
    let mut decoder = Decoder::new();
    let mut found = false;
    for unit in units(bytes) {
        if let Some(frame) = decoder.push(unit)? {
            output(frame);
            found = true;
        }
    }
    if !found {
        return Err(Error::NoVideo);
    }
    Ok(())
}
