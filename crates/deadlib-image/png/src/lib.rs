//! Independent PNG codec. See the coverage document for verified capabilities.
#![forbid(unsafe_code)]

mod checksum;
pub mod zlib;

pub use deadlib_image::{ColorType, Image, Samples};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Error {
    Truncated,
    Invalid(&'static str),
    Checksum,
    LimitExceeded,
    Allocation,
    UnsupportedCriticalChunk([u8; 4]),
    Io(String),
}

impl core::fmt::Display for Error {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Truncated => f.write_str("truncated input"),
            Self::Invalid(reason) => write!(f, "invalid PNG/zlib: {reason}"),
            Self::Checksum => f.write_str("checksum mismatch"),
            Self::LimitExceeded => f.write_str("configured resource limit exceeded"),
            Self::Allocation => f.write_str("allocation failed"),
            Self::UnsupportedCriticalChunk(kind) => write!(f, "unknown critical chunk {kind:?}"),
            Self::Io(reason) => write!(f, "I/O error: {reason}"),
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
