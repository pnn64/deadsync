//! Dependency-free MPEG-4 Visual and Microsoft MPEG-4 v2 decoding for progressive 4:2:0 pictures.
//!
//! Feed an elementary stream to [`decode_elementary_stream`] or an AVI file to
//! [`decode_avi`]. [`Decoder`] accepts MPEG-4 Visual start-code units and
//! [`MsMpeg4v2Decoder`] accepts individual MP42 picture packets.

#![no_std]
#![deny(unsafe_code)]

extern crate alloc;

mod avi;
mod bitstream;
mod block;
mod frame;
mod headers;
mod motion;
mod msmpeg4v2;
mod picture;
mod tables;
mod transform;

pub use avi::decode_avi;
pub use bitstream::{units, Unit};
pub use frame::{Frame, Plane};
pub use msmpeg4v2::MsMpeg4v2Decoder;

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
    B,
}

/// A serial decoder for complete MPEG-4 Visual start-code units.
#[derive(Default)]
pub struct Decoder {
    layer: Option<Layer>,
    reference: Option<(i64, Arc<Frame>)>,
    older: Option<(i64, Arc<Frame>)>,
    time_base: i64,
    last_time_base: i64,
    spare: Option<Frame>,
    bidirectional_frames: [Option<Arc<Frame>>; 2],
    workspace: picture::Workspace,
}

impl Decoder {
    pub fn new() -> Self {
        Self::default()
    }

    /// Accepts a video unit and returns a reconstructed picture in display order.
    pub fn push(&mut self, unit: Unit<'_>) -> Result<Option<Arc<Frame>>> {
        match unit.code {
            0x20..=0x2f => {
                let layer = Layer::parse(unit.payload)?;
                if self.reference.as_ref().is_some_and(|(_, frame)| {
                    (frame.width(), frame.height()) != (layer.width, layer.height)
                }) {
                    self.reference = None;
                    self.older = None;
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
                if header.kind == PictureKind::B {
                    let time = picture_time(self.last_time_base, &header, layer)?;
                    let (Some((old_time, old)), Some((next_time, next))) =
                        (&self.older, &self.reference)
                    else {
                        return Ok(None); // A seek can enter a GOP without its first reference.
                    };
                    if time <= *old_time || time >= *next_time {
                        return Ok(None);
                    }
                    let distance = next_time
                        .checked_sub(*old_time)
                        .filter(|&distance| i32::try_from(distance).is_ok())
                        .ok_or(Error::Invalid("reference time span"))?;
                    // The player can retain the last display picture until
                    // its successor supplies the closing timestamp.
                    let reusable = self.bidirectional_frames[1]
                        .take()
                        .and_then(|frame| Arc::try_unwrap(frame).ok());
                    let mut frame = picture::decode_bidirectional(
                        &mut bits,
                        layer,
                        header,
                        [old, next],
                        [time - old_time, distance],
                        reusable,
                        &mut self.workspace,
                    )?;
                    frame.presentation_time = Some((time, layer.time_resolution));
                    let frame = Arc::new(frame);
                    self.bidirectional_frames[1] =
                        self.bidirectional_frames[0].replace(Arc::clone(&frame));
                    return Ok(Some(frame));
                }
                let time = picture_time(self.time_base, &header, layer)?;
                let time_base = self.time_base + header.modulo_time;
                let mut frame = picture::decode(
                    &mut bits,
                    layer,
                    header,
                    self.reference.as_ref().map(|(_, frame)| frame.as_ref()),
                    self.spare.take(),
                    &mut self.workspace,
                )?;
                frame.presentation_time = Some((time, layer.time_resolution));
                let frame = Arc::new(frame);
                self.last_time_base = self.time_base;
                self.time_base = time_base;
                let retired = self.reference.replace((time, Arc::clone(&frame)));
                let displayed = if layer.low_delay {
                    self.spare = retired.and_then(|(_, frame)| Arc::try_unwrap(frame).ok());
                    Some(frame)
                } else {
                    let old = core::mem::replace(&mut self.older, retired);
                    self.spare = old.and_then(|(_, frame)| Arc::try_unwrap(frame).ok());
                    self.older.as_ref().map(|(_, frame)| Arc::clone(frame))
                };
                return Ok(displayed);
            }
            0xb1 => {
                let last = self.finish();
                *self = Self::new();
                return Ok(last);
            }
            0x00..=0x1f | 0xb0 | 0xb2 | 0xb3 | 0xb5 => {} // Object and sequence metadata.
            _ => return Err(Error::Unsupported("start code")),
        }
        Ok(None)
    }

    /// Drains the final pending reference picture of a reordered stream.
    pub fn finish(&mut self) -> Option<Arc<Frame>> {
        if self.layer.is_some_and(|layer| !layer.low_delay) {
            self.reference.take().map(|(_, frame)| frame)
        } else {
            None
        }
    }
}

fn picture_time(base: i64, picture: &Picture, layer: Layer) -> Result<i64> {
    base.checked_add(picture.modulo_time)
        .and_then(|time| time.checked_mul(layer.time_resolution))
        .and_then(|time| time.checked_add(picture.time_increment))
        .ok_or(Error::Invalid("picture time overflow"))
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
    if let Some(frame) = decoder.finish() {
        output(frame);
        found = true;
    }
    if !found {
        return Err(Error::NoVideo);
    }
    Ok(())
}
