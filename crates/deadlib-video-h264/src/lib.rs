#![doc = include_str!("../README.md")]
#![cfg_attr(not(feature = "std"), no_std)]

#[macro_use]
extern crate alloc;

#[cfg(all(test, not(feature = "std")))]
extern crate std;

#[allow(dead_code)]
mod bitstream;
mod cabac;
mod cabac_tables;
mod cavlc;
mod chroma;
#[allow(dead_code)]
mod deblock;
mod decode_cabac;
mod decode_cavlc;
pub mod decoder;
#[allow(dead_code)]
mod dpb;
pub mod error;
mod inter_pred;
mod intra_pred;
mod mv_pred;
pub mod nal;
mod neighbor;
mod once;
mod picture_sync;
mod pixel;
#[allow(dead_code)]
mod pps;
#[allow(dead_code)]
mod residual;
#[allow(dead_code)]
mod sei;
mod simd;
#[allow(dead_code)]
mod slice;
mod slice_context;
#[allow(dead_code)]
mod sps;
#[cfg(feature = "std")]
pub mod threading;

pub use decoder::{ChromaFormat, Decoder, Frame, OrderedDecoder};
pub use error::DecodeError;
