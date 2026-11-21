//! A safe, dependency-free BMP codec for indexed and truecolor assets.
//!
//! Decoding preserves palettes and returns rows in top-to-bottom order. The
//! encoder writes RGB24 or explicitly masked RGBA32 BMPs. See the crate README
//! for the supported header/compression combinations and compatibility policies.
//!
//! ```
//! use deadlib_image_bmp::{Bitmap, Pixels, decode, encode};
//!
//! let bitmap = Bitmap {
//!     width: 1,
//!     height: 1,
//!     pixels: Pixels::Rgba(vec![255, 0, 128, 0]),
//! };
//! let bytes = encode(&bitmap)?;
//! assert_eq!(decode(&bytes)?.to_rgba8()?, vec![255, 0, 128, 0]);
//! # Ok::<(), deadlib_image_bmp::Error>(())
//! ```
#![forbid(unsafe_code)]

mod decode;
mod encode;
mod model;

pub use decode::{Info, decode, decode_with_limits, probe, probe_with_limits};
pub use encode::{EncodeOptions, encode, encode_with_options};
pub use model::{Bitmap, Limits, Pixels};

/// A decoding, conversion, or encoding failure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Error {
    /// Required header, palette, mask, or pixel bytes are absent.
    Truncated,
    /// The input or caller-supplied bitmap is structurally invalid.
    Invalid(&'static str),
    /// The file uses a BMP feature outside this codec's supported subset.
    Unsupported(&'static str),
    /// A configured limit or the platform's representable size was exceeded.
    LimitExceeded,
    /// Reserving the required output storage failed.
    Allocation,
}

impl core::fmt::Display for Error {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Truncated => f.write_str("truncated BMP input"),
            Self::Invalid(reason) => write!(f, "invalid BMP: {reason}"),
            Self::Unsupported(reason) => write!(f, "unsupported BMP: {reason}"),
            Self::LimitExceeded => f.write_str("configured BMP resource limit exceeded"),
            Self::Allocation => f.write_str("BMP allocation failed"),
        }
    }
}

impl std::error::Error for Error {}

pub(crate) fn reserve<T>(output: &mut Vec<T>, additional: usize) -> Result<(), Error> {
    output
        .try_reserve_exact(additional)
        .map_err(|_| Error::Allocation)
}
