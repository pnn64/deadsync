//! Dependency-free MPEG-2 (H.262) video decoding for progressive 4:2:0 frames.
//!
//! Use [`decode_elementary_stream`] for raw video, [`decode_program_stream`] for
//! MPEG Program Streams, or [`Decoder`] to feed individual [`Unit`]s.

#![no_std]
#![deny(unsafe_code)]

extern crate alloc;

mod bitstream;
mod block;
mod frame;
mod headers;
mod motion;
mod program_stream;
mod slice;
mod tables;
mod transform;

pub use bitstream::{units, Unit};
pub use frame::{Frame, Plane};
pub use program_stream::video_elementary_stream;

use alloc::sync::Arc;
use core::fmt;

use bitstream::Bits;
use headers::{Coding, PictureHeader, Sequence};
use slice::Slice;

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
            Self::Truncated => f.write_str("truncated MPEG data"),
            Self::Invalid(field) => write!(f, "invalid MPEG syntax ({field})"),
            Self::Unsupported(feature) => write!(f, "unsupported MPEG feature ({feature})"),
            Self::MissingReference => f.write_str("missing MPEG-2 prediction reference"),
            Self::NoVideo => f.write_str("no MPEG-2 video pictures found"),
        }
    }
}

impl core::error::Error for Error {}

/// The coding type of a frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PictureKind {
    I,
    P,
    B,
}

struct Picture {
    header: PictureHeader,
    coding: Option<Coding>,
    frame: Option<Frame>,
    next_macroblock: usize,
    discard: bool,
}

/// Accepts complete video start-code units and emits reconstructed frames
/// in display order. Call [`Decoder::finish`] to drain the last reference frame.
///
/// At an open GOP entry point, leading B pictures with a missing reference
/// are discarded. After a decoding error, start a new decoder.
#[derive(Default)]
pub struct Decoder {
    sequence: Option<Sequence>,
    picture: Option<Picture>,
    older: Option<Arc<Frame>>,
    newest: Option<Arc<Frame>>,
    spare: Option<Frame>,
}

impl Decoder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, unit: Unit<'_>, mut output: impl FnMut(Arc<Frame>)) -> Result<()> {
        match unit.code {
            0x00 => {
                self.finish_picture(&mut output)?;
                let sequence = self
                    .sequence
                    .as_ref()
                    .ok_or(Error::Invalid("missing sequence header"))?;
                if !sequence.extended {
                    return Err(Error::Unsupported("MPEG-1 video"));
                }
                let header = PictureHeader::parse(unit.payload)?;
                let discard = header.kind == PictureKind::B && self.older.is_none();
                self.picture = Some(Picture {
                    header,
                    coding: None,
                    frame: None,
                    next_macroblock: 0,
                    discard,
                });
                Ok(())
            }
            0x01..=0xaf => self.decode_slice(unit),
            0xb2 => Ok(()), // User data.
            0xb3 => {
                self.finish_picture(&mut output)?;
                self.sequence = Some(Sequence::parse(unit.payload)?);
                Ok(())
            }
            0xb5 => self.extension(unit.payload, &mut output),
            0xb7 => self.finish(&mut output),
            0xb8 => self.finish_picture(&mut output), // GOP header.
            _ => Err(Error::Invalid("video start code")),
        }
    }

    /// Emits the remaining frames and resets the decoder for the next stream.
    pub fn finish(&mut self, mut output: impl FnMut(Arc<Frame>)) -> Result<()> {
        self.finish_picture(&mut output)?;
        self.drain_references(&mut output);
        self.sequence = None;
        Ok(())
    }

    fn extension(&mut self, bytes: &[u8], output: &mut impl FnMut(Arc<Frame>)) -> Result<()> {
        let mut bits = Bits::new(bytes);
        match bits.read(4)? {
            1 => {
                let sequence = self
                    .sequence
                    .as_mut()
                    .ok_or(Error::Invalid("orphan sequence extension"))?;
                sequence.extend(&mut bits)?;
                if self
                    .newest
                    .as_ref()
                    .is_some_and(|frame| !frame.matches(sequence))
                {
                    self.drain_references(output);
                }
            }
            3 => {
                let sequence = self
                    .sequence
                    .as_mut()
                    .ok_or(Error::Invalid("orphan quantization matrix extension"))?;
                sequence.update_matrices(&mut bits)?;
            }
            8 => {
                let picture = self
                    .picture
                    .as_mut()
                    .ok_or(Error::Invalid("orphan picture coding extension"))?;
                if picture.coding.is_some() {
                    return Err(Error::Invalid("duplicate picture coding extension"));
                }
                picture.coding = Some(Coding::parse(&mut bits)?);
            }
            2 | 4 | 7 => {} // Display and copyright metadata.
            _ => return Err(Error::Unsupported("MPEG extension identifier")),
        }
        Ok(())
    }

    fn decode_slice(&mut self, unit: Unit<'_>) -> Result<()> {
        let sequence = self
            .sequence
            .as_ref()
            .ok_or(Error::Invalid("missing sequence header"))?;
        let picture = self
            .picture
            .as_mut()
            .ok_or(Error::Invalid("slice outside a picture"))?;
        let coding = picture
            .coding
            .as_ref()
            .ok_or(Error::Invalid("missing picture coding extension"))?;
        if picture.discard {
            return Ok(());
        }
        let references = match picture.header.kind {
            PictureKind::I => [None, None],
            PictureKind::P => {
                let reference = self.newest.as_deref().ok_or(Error::MissingReference)?;
                [Some(reference), None]
            }
            PictureKind::B => [self.older.as_deref(), self.newest.as_deref()],
        };
        let frame = picture
            .frame
            .get_or_insert_with(|| match self.spare.take() {
                Some(frame) => frame.reuse(sequence, &picture.header),
                None => Frame::new(sequence, &picture.header),
            });
        let slice = Slice {
            sequence,
            coding,
            references,
        };
        slice.decode(unit, frame, &mut picture.next_macroblock)
    }

    fn finish_picture(&mut self, output: &mut impl FnMut(Arc<Frame>)) -> Result<()> {
        let Some(picture) = self.picture.take() else {
            return Ok(());
        };
        if picture.discard {
            return Ok(());
        }
        let sequence = self
            .sequence
            .as_ref()
            .ok_or(Error::Invalid("missing sequence header"))?;
        let count = sequence.width.div_ceil(16) * sequence.macroblock_rows();
        if picture.next_macroblock != count {
            return Err(Error::Truncated);
        }
        let frame = Arc::new(picture.frame.ok_or(Error::Truncated)?);
        if frame.picture_kind() == PictureKind::B {
            output(Arc::clone(&frame));
            self.spare = Arc::try_unwrap(frame).ok();
        } else {
            let retired = core::mem::replace(&mut self.older, self.newest.replace(frame));
            self.spare = retired.and_then(|old| Arc::try_unwrap(old).ok());
            if let Some(previous) = &self.older {
                output(Arc::clone(previous));
            }
        }
        Ok(())
    }

    fn drain_references(&mut self, output: &mut impl FnMut(Arc<Frame>)) {
        if let Some(last) = self.newest.take() {
            output(last);
        }
        self.older = None;
        self.spare = None;
    }
}

/// Decodes a complete MPEG-2 video elementary stream.
pub fn decode_elementary_stream(bytes: &[u8], mut output: impl FnMut(Arc<Frame>)) -> Result<()> {
    let mut decoder = Decoder::new();
    let mut found = false;
    let mut emit = |frame| {
        found = true;
        output(frame);
    };
    for unit in units(bytes) {
        decoder.push(unit, &mut emit)?;
    }
    decoder.finish(&mut emit)?;
    if !found {
        return Err(Error::NoVideo);
    }
    Ok(())
}

/// Extracts and decodes the first video stream in an MPEG Program Stream.
pub fn decode_program_stream(bytes: &[u8], output: impl FnMut(Arc<Frame>)) -> Result<()> {
    decode_elementary_stream(&video_elementary_stream(bytes)?, output)
}
