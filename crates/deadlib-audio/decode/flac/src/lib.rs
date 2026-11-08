//! Independent integer-PCM FLAC codec. See the package coverage ledger.
#![doc = include_str!("../README.md")]
#![deny(unsafe_code)]

pub mod bitstream;
pub mod checksum;
pub mod encode;
mod encoder;
pub mod frame;
pub mod metadata;
pub mod native;
pub mod ogg;
mod ogg_encode;
mod ogg_seek;
mod pcm_md5;
mod predictor;
mod sample;

pub use encode::{EncodeOptions, FrameEncoder};
pub use encoder::{EncodedStream, Encoder};
pub use frame::{Frame, FrameDecoder, FrameHeader, Pcm};
pub use native::{
    DecodeOptions, Decoder, Integrity, Reader, SeekOptions, SeekReader, SeekStats, StreamFrame,
};
pub use ogg::{OggDecoder, OggEvent, OggOptions, OggPcm, OggReader, OggStreamDecoder, StreamId};
pub use ogg_encode::{OggEncodedStream, OggEncoder, OggMuxer, OggWriteOptions};
pub use ogg_seek::{OggSeekOptions, OggSeekReader, OggSeekStream};

pub use metadata::{Metadata, MetadataBlock, StreamInfo};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Error {
    Truncated,
    Invalid(&'static str),
    Checksum(&'static str),
    LimitExceeded(&'static str),
    Allocation,
    Io(String),
    Ogg(deadlib_audio_decode_ogg::Error),
}

impl core::fmt::Display for Error {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Truncated => f.write_str("truncated FLAC data"),
            Self::Invalid(reason) => write!(f, "invalid FLAC data: {reason}"),
            Self::Checksum(kind) => write!(f, "FLAC {kind} checksum mismatch"),
            Self::LimitExceeded(reason) => write!(f, "FLAC resource limit: {reason}"),
            Self::Allocation => f.write_str("FLAC allocation failed"),
            Self::Io(reason) => write!(f, "FLAC I/O error: {reason}"),
            Self::Ogg(error) => write!(f, "FLAC container: {error}"),
        }
    }
}
impl std::error::Error for Error {}
impl From<std::io::Error> for Error {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value.to_string())
    }
}
impl From<deadlib_audio_decode_ogg::Error> for Error {
    fn from(value: deadlib_audio_decode_ogg::Error) -> Self {
        Self::Ogg(value)
    }
}

/// Resource budgets are independent of untrusted STREAMINFO size claims.
#[derive(Clone, Copy, Debug)]
pub struct Limits {
    pub metadata_bytes: usize,
    pub metadata_blocks: usize,
    pub frame_bytes: usize,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            metadata_bytes: 64 << 20,
            metadata_blocks: 4096,
            frame_bytes: 32 << 20,
        }
    }
}

pub(crate) fn reserve<T>(v: &mut Vec<T>, count: usize) -> Result<(), Error> {
    v.try_reserve(count).map_err(|_| Error::Allocation)
}
