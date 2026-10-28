//! Independent JPEG-1 primitives. This codec is under active development.
//!
//! JPEG-specific precision, component and coefficient types live here because
//! the shared image model has no 12-bit, CMYK or subsampled representation.
#![forbid(unsafe_code)]

mod arithmetic;
mod arithmetic_encode;
mod decode;
mod encode;
mod entropy;
mod exif;
mod hierarchical;
mod hierarchical_encode;
mod huffman_encode;
mod lossless;
mod lossless_arithmetic;
mod lossless_encode;
mod marker;
mod metadata;
mod model;
mod output;
mod pixel_encode;
mod progressive;
mod pyramid;
mod reconstruct;
mod scan_encode;
mod transform;

pub use arithmetic_encode::ArithmeticConditioning;
pub use decode::{CoefficientComponent, CoefficientImage, decode_coefficients};
pub use encode::{
    CodingOptions, EntropyCoding, IccWrite, MetadataOptions, encode_coefficients,
    encode_coefficients_with_metadata,
};
pub use exif::{ByteOrder, Exif, ExifEntry, ExifIfd};
pub use hierarchical::{
    Expansion, HierarchicalData, HierarchicalFrame, HierarchicalImage, HierarchyHeader,
    decode_hierarchical,
};
pub use hierarchical_encode::{
    HierarchicalFrameOptions, HierarchicalOptions, encode_hierarchical,
    encode_hierarchical_with_metadata,
};
pub use lossless::{LosslessCoding, LosslessImage, decode_lossless};
pub use lossless_encode::{
    LosslessOptions, LosslessScan, encode_lossless, encode_lossless_samples,
    encode_lossless_samples_with_metadata, encode_lossless_with_metadata,
};
pub use metadata::{
    Adobe, DensityUnit, ICC_CHUNK_BYTES, IccChunk, IccHeader, IccProfile, Jfif, Jfxx, MAX_ICC_BYTES,
};
pub use pixel_encode::{
    EncodeInput, EncodeOptions, SampleData, Subsampling, encode, encode_samples,
    encode_samples_with_metadata, encode_with_metadata, quantize,
};
pub use pyramid::{
    PyramidLevel, PyramidOptions, PyramidRasterOptions, build_hierarchical_planes,
    build_hierarchical_samples, encode_hierarchical_image, encode_hierarchical_image_with_metadata,
    encode_hierarchical_planes, encode_hierarchical_planes_with_metadata,
    encode_hierarchical_samples, encode_hierarchical_samples_with_metadata,
};
pub use reconstruct::{
    CmykImage, CmykPolarity, ColorSpace, ComponentPlane, PlanarImage, Upsampling,
};
pub use scan_encode::ScanSpec;

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
