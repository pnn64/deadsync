//! Independent MPEG Audio Layer III primitives. The PCM codec is in development.
//!
//! No production codec dependencies and no DeadSync playback integration.
#![deny(unsafe_code)]

pub mod bands;
pub mod bits;
pub mod checksum;
pub mod decoder;
pub mod frame;
pub mod huffman;
pub mod indexed;
pub mod reader;
pub mod reservoir;
pub mod scalefactors;
pub mod side;
pub mod spectral;
pub mod stream;
pub mod synthesis;
pub mod tags;

pub use decoder::{DecodeOptions, DecodedFrame, FrameDecoder, Pcm};
pub use frame::{ChannelMode, Frame, FrameHeader, FreeFormat, MpegVersion};
pub use indexed::{
    DECODER_DELAY, FileFrame, FileOptions, FrameIndex, IndexEntry, IndexedDecoder, SampleMode,
    SampleRange,
};
pub use reader::{Metadata, Packet, PacketReader, ReaderOptions, TruncatedPolicy};
pub use reservoir::Reservoir;
pub use side::{GranuleChannel, SideInfo};
pub use stream::{FrameStatus, Recovery, StreamDecoder, StreamFrame, StreamOptions, StreamStats};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    Truncated,
    Invalid(&'static str),
    Unsupported(&'static str),
    LimitExceeded(&'static str),
    CrcMismatch { expected: u16, actual: u16 },
    MissingReservoir { needed: usize, available: usize },
    Allocation,
    Io(String),
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for Error {}
impl From<std::io::Error> for Error {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error.to_string())
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Limits {
    pub frame_bytes: usize,
    pub metadata_bytes: usize,
    pub sync_bytes: usize,
    pub index_frames: usize,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            frame_bytes: 64 << 10,
            metadata_bytes: 64 << 20,
            sync_bytes: 1 << 20,
            index_frames: 4 << 20,
        }
    }
}

pub(crate) fn reserve<T>(buffer: &mut Vec<T>, additional: usize) -> Result<(), Error> {
    buffer
        .try_reserve(additional)
        .map_err(|_| Error::Allocation)
}
