//! Independent RIFF/WAVE container and native audio codecs.
#![doc = include_str!("../README.md")]
#![deny(unsafe_op_in_unsafe_fn)]

pub mod adpcm;
pub mod container;
pub mod encode;
pub mod format;
pub mod mux;
pub mod pcm;
pub mod reader;
pub mod stream;

pub use adpcm::{Adpcm, AdpcmData};
pub use container::{Chunk, Chunks, Container, Ds64, Wave};
pub use encode::{RawChunk, Spec, WriteOptions, Writer};
pub use format::{Encoding, Endian, Format, SampleFormat};
pub use mux::Muxer;
pub use pcm::Pcm;
pub use reader::Reader;
pub use stream::{ChunkInfo, Stream};

#[derive(Debug)]
pub enum Error {
    Truncated,
    Invalid(&'static str),
    Unsupported(&'static str),
    Limit(&'static str),
    Allocation,
    Io(std::io::Error),
}
impl core::fmt::Display for Error {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Truncated => f.write_str("truncated WAVE data"),
            Self::Invalid(s) => write!(f, "invalid WAVE: {s}"),
            Self::Unsupported(s) => write!(f, "unsupported WAVE: {s}"),
            Self::Limit(s) => write!(f, "WAVE resource limit: {s}"),
            Self::Allocation => f.write_str("WAVE allocation failed"),
            Self::Io(e) => write!(f, "WAVE I/O: {e}"),
        }
    }
}
impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(e) => Some(e),
            _ => None,
        }
    }
}
impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        if e.kind() == std::io::ErrorKind::UnexpectedEof {
            Self::Truncated
        } else {
            Self::Io(e)
        }
    }
}

/// Bounds parsing work, not audio duration. Payloads remain borrowed.
#[derive(Clone, Copy, Debug)]
pub struct Limits {
    pub chunks: usize,
    pub size_table_entries: usize,
    pub format_bytes: usize,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            chunks: 1_000_000,
            size_table_entries: 4096,
            format_bytes: 1 << 20,
        }
    }
}

pub(crate) fn reserve<T>(v: &mut Vec<T>, additional: usize) -> Result<(), Error> {
    v.try_reserve(additional).map_err(|_| Error::Allocation)
}
