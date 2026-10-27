//! Independent integer-PCM FLAC codec. See the package coverage ledger.
#![forbid(unsafe_code)]

pub mod bitstream;
pub mod checksum;
pub mod metadata;

pub use metadata::{Metadata, MetadataBlock, StreamInfo};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Error {
    Truncated,
    Invalid(&'static str),
    Checksum(&'static str),
    LimitExceeded(&'static str),
    Allocation,
    Io(String),
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
        }
    }
}
impl std::error::Error for Error {}
impl From<std::io::Error> for Error {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value.to_string())
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
