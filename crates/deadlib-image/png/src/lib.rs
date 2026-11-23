//! Independent PNG codec. See the coverage document for verified capabilities.
#![deny(unsafe_code)]

mod apng;
mod checksum;
mod codec;
mod composite;
mod metadata;
mod reference;
mod scanline;
mod stream;
pub mod zlib;

pub use codec::{
    AncillaryChunk, AnimationPolicy, ChunkPosition, DecodeOptions, EncodeOptions, Filter, Limits,
    MetadataPolicy, PalettePolicy, Png, Transparency, decode, decode_with_options, encode,
};

pub use apng::{Animation, BlendOp, DisposeOp, Frame, FrameControl};
pub use composite::{Compositor, RenderedFrame};
pub use deadlib_image::{ColorType, Image, Samples};
pub use metadata::{LANGUAGE_SUBTAG_REGISTRY_DATE, Metadata};
pub use stream::{
    DecodeEvent, DecodeProgress, DecodeStatus, ImageInfo, ImageKind, Row, RowSamples,
    StreamDecoder, decode_reader,
};
pub use stream::{EncodeStats, encode_writer};

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

/// `len` zero bytes with exact capacity, from a zeroed allocation. Fresh
/// pages are already zero, so this skips the pass `resize` would make.
#[allow(unsafe_code)]
pub(crate) fn zeroed(len: usize) -> Result<Vec<u8>, Error> {
    if len == 0 {
        return Ok(Vec::new());
    }
    let layout = std::alloc::Layout::array::<u8>(len).map_err(|_| Error::Allocation)?;
    // SAFETY: `layout` has a nonzero size.
    let bytes = unsafe { std::alloc::alloc_zeroed(layout) };
    if bytes.is_null() {
        return Err(Error::Allocation);
    }
    // SAFETY: the global allocator returned `bytes` for `layout`: `len`
    // bytes at alignment 1, all initialized to zero.
    Ok(unsafe { Vec::from_raw_parts(bytes, len, len) })
}
