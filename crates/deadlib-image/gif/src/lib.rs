//! Independent GIF87a/GIF89a codec. Indexed pixels are always retained exactly.
//!
//! [`Decoder`] accepts arbitrary input fragments. [`Compositor`] optionally
//! renders onto a reusable RGBA8 logical screen. Timing is in original
//! centiseconds; scheduling, user input, and loop playback belong to the caller.
#![forbid(unsafe_code)]

mod composite;
mod decode;
mod encode;
pub mod lzw;
mod model;

pub use composite::{Background, CompositeOptions, Compositor, RenderedFrame, TextRasterizer};
pub use deadlib_image::{ColorType, Image, Samples};
pub use decode::{DecodeOptions, Decoder, Event, Limits, Progress, decode, decode_with_options};
pub use encode::{EncodeOptions, Encoder, encode, encode_into};
pub use model::*;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Error {
    Truncated,
    Invalid(&'static str),
    LimitExceeded,
    Allocation,
    MissingPalette,
    TextRendererRequired,
    UnsupportedGraphicExtension(u8),
    Io(String),
}

impl core::fmt::Display for Error {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Truncated => f.write_str("truncated GIF input"),
            Self::Invalid(reason) => write!(f, "invalid GIF: {reason}"),
            Self::LimitExceeded => f.write_str("configured GIF resource limit exceeded"),
            Self::Allocation => f.write_str("GIF allocation failed"),
            Self::MissingPalette => {
                f.write_str("no active color table; provide an external palette")
            }
            Self::TextRendererRequired => {
                f.write_str("plain-text graphics require a text rasterizer")
            }
            Self::UnsupportedGraphicExtension(label) => write!(
                f,
                "unknown graphic extension {label:#04x}; caller rendering required"
            ),
            Self::Io(reason) => write!(f, "GIF I/O error: {reason}"),
        }
    }
}
impl std::error::Error for Error {}
impl From<std::io::Error> for Error {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value.to_string())
    }
}

pub(crate) fn reserve<T>(vec: &mut Vec<T>, extra: usize) -> Result<(), Error> {
    vec.try_reserve(extra).map_err(|_| Error::Allocation)
}
