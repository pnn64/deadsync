//! Codec-independent Ogg container primitives, implemented from RFC 3533.
//!
//! Granule positions are raw codec-defined values, not timestamps or samples.
//! See the package coverage document for validation evidence and limitations.
#![forbid(unsafe_code)]

mod capture;
mod crc;
mod demux;
mod index;
mod page;
mod reader;
mod validation;
mod writer;

pub use capture::{CaptureInfo, CapturedPacket, PacketCapture};
pub use crc::crc32;
pub use demux::{Demuxer, Limits, Packet, PacketInfo};
pub use index::{
    GranuleLocation, IndexEvent, IndexOptions, IndexedPage, OggIndex, StreamIndex, find_granule,
};
pub use page::{Fragment, Fragments, Page, PageHeader, encode_page, write_page};
pub use reader::{LocatedPage, PageDecoder, PageProgress, PageReader, SyncMode};
pub use validation::{CompatibilityStats, ValidationOptions};
pub use writer::{PacketWriter, WriteOptions};

pub const MAX_PAGE_SIZE: usize = 27 + 255 + 255 * 255;
pub const UNKNOWN_GRANULE_POSITION: u64 = u64::MAX;
pub const CONTINUED: u8 = 1;
pub const BOS: u8 = 2;
pub const EOS: u8 = 4;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Error {
    Truncated,
    Invalid(&'static str),
    Checksum {
        expected: u32,
        actual: u32,
    },
    Sequence {
        serial: u32,
        expected: u32,
        actual: u32,
    },
    LimitExceeded(&'static str),
    Allocation,
    Failed,
    Io(String),
}

impl core::fmt::Display for Error {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Truncated => f.write_str("truncated Ogg stream"),
            Self::Invalid(reason) => write!(f, "invalid Ogg stream: {reason}"),
            Self::Checksum { expected, actual } => write!(
                f,
                "Ogg CRC mismatch: stored {expected:08x}, computed {actual:08x}"
            ),
            Self::Sequence {
                serial,
                expected,
                actual,
            } => write!(
                f,
                "Ogg stream {serial}: expected page {expected}, got {actual}"
            ),
            Self::LimitExceeded(reason) => write!(f, "Ogg resource limit: {reason}"),
            Self::Allocation => f.write_str("Ogg allocation failed"),
            Self::Failed => f.write_str("Ogg state is terminal after an earlier error"),
            Self::Io(reason) => write!(f, "Ogg IO error: {reason}"),
        }
    }
}
impl std::error::Error for Error {}
impl From<std::io::Error> for Error {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error.to_string())
    }
}

pub(crate) fn reserve<T>(v: &mut Vec<T>, additional: usize) -> Result<(), Error> {
    v.try_reserve(additional).map_err(|_| Error::Allocation)
}
