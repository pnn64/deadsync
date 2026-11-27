//! Standalone, dependency-free H.265/HEVC video decoding.
//!
//! Decode an indexed MP4 with [`decode_mp4`] or a complete Annex B stream
//! with [`decode_annex_b`]. Both functions emit cropped planar YCbCr frames
//! in display order. For incremental decoding, feed [`Decoder`] with NAL
//! units obtained from your container or elementary stream.

// --- Internal modules (not part of the public API) ---
// allow(dead_code): internal modules contain utility functions, struct fields,
// and table constants kept for spec completeness / future use.
#[allow(dead_code)]
mod bitstream;
#[allow(dead_code)]
mod cabac;
#[allow(dead_code)]
mod cabac_tables;
#[allow(dead_code)]
mod cu_tree;
#[allow(dead_code)]
mod deblock;
#[allow(dead_code)]
mod inter_pred;
#[allow(dead_code)]
mod intra_pred;
#[allow(dead_code)]
mod inverse_transform;
mod output;
mod parallel;
#[allow(dead_code)]
pub(crate) mod pixel;
#[allow(dead_code)]
mod pps;
#[allow(dead_code)]
mod profile_tier_level;
#[allow(dead_code)]
mod residual_coding;
mod rowfilter;
#[allow(dead_code)]
mod sao;
#[allow(dead_code)]
mod scaling_list;
#[allow(dead_code)]
mod shared;
#[allow(dead_code)]
mod slice;
#[allow(dead_code)]
mod sps;
#[allow(dead_code)]
mod vps;
mod wavefront;

// --- Internal but needs pub(crate) access from decoder ---
#[allow(dead_code)]
pub(crate) mod dpb;

// --- Public modules ---
pub mod decoder;
pub mod error;
mod mp4;
pub mod nal;
mod transport;

// --- Top-level re-exports for convenience ---
pub use decoder::{DecodeLimits, Decoder, Frame};
pub use error::DecodeError;
pub use nal::{NalUnit, NalUnitType, parse_annex_b, parse_hvcc, parse_nal};
pub use pixel::{Pixel, PixelData};

pub use mp4::{Mp4Error, decode_mp4};
pub use transport::decode_annex_b;
