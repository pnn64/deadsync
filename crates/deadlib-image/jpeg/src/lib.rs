//! Independent JPEG-1 primitives. This codec is under active development.
//!
//! JPEG-specific precision, component and coefficient types live here because
//! the shared image model has no 12-bit, CMYK or subsampled representation.
#![forbid(unsafe_code)]

mod decode;
mod encode;
mod entropy;
mod huffman_encode;
mod marker;
mod model;
mod output;
mod progressive;
mod reconstruct;
mod transform;

pub use decode::{CoefficientComponent, CoefficientImage, decode_coefficients};
pub use encode::{CodingOptions, encode_coefficients};
pub use reconstruct::{ColorSpace, ComponentPlane, PlanarImage, Upsampling};

pub use marker::{Event, MarkerReader};
pub use model::*;

/// Failure categories remain stable across input and output APIs.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error {
    Truncated,
    Invalid(&'static str),
    Unsupported(&'static str),
    LimitExceeded(&'static str),
    Allocation,
}

impl core::fmt::Display for Error {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Truncated => f.write_str("truncated JPEG input"),
            Self::Invalid(s) => write!(f, "invalid JPEG: {s}"),
            Self::Unsupported(s) => write!(f, "unsupported JPEG feature: {s}"),
            Self::LimitExceeded(s) => write!(f, "JPEG resource limit exceeded: {s}"),
            Self::Allocation => f.write_str("JPEG allocation failed"),
        }
    }
}
impl std::error::Error for Error {}

pub type Result<T> = core::result::Result<T, Error>;

/// Resource ceilings are checked before coefficient or sample allocation.
#[derive(Clone, Copy, Debug)]
pub struct Limits {
    pub max_pixels: usize,
    pub max_memory_bytes: usize,
    pub max_metadata_bytes: usize,
    pub max_scans: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_pixels: 100_000_000,
            max_memory_bytes: 512 * 1024 * 1024,
            max_metadata_bytes: 16 * 1024 * 1024,
            max_scans: 4096,
        }
    }
}
