#![warn(missing_docs, unused_lifetimes)]

//! DeadSync's compact binary persistence codec.
//!
//! This is a deliberately reduced fork of bincode 2.0.1. It supports the
//! standard bincode wire format and the types used by DeadSync's caches,
//! profiles, scores, noteskins, and analysis data.
//!
//! ```rust
//! use bincode::{Decode, Encode};
//!
//! #[derive(Debug, PartialEq, Encode, Decode)]
//! struct SaveData {
//!     name: String,
//!     score: u32,
//! }
//!
//! let input = SaveData { name: "player".into(), score: 500 };
//! let bytes = bincode::encode_to_vec(&input, bincode::config::standard()).unwrap();
//! let decoded: SaveData =
//!     bincode::decode_from_slice(&bytes, bincode::config::standard()).unwrap().0;
//! assert_eq!(decoded, input);
//! ```

#![doc(html_root_url = "https://docs.rs/deadlib-bincode/2.0.1")]
#![crate_name = "bincode"]
#![crate_type = "rlib"]

mod impls;
mod unty;
pub(crate) mod utils;
pub(crate) mod varint;

pub mod config;
#[macro_use]
pub mod de;
pub mod enc;
pub mod error;

pub use bincode_derive::{BorrowDecode, Decode, Encode};
pub use de::{BorrowDecode, Decode};
pub use enc::Encode;
pub use impls::{encode_into_vec, encode_to_vec};

use config::Config;
use de::Decoder;

/// Decode a value from a byte slice using the given configuration.
///
/// The returned `usize` is the number of source bytes consumed.
pub fn decode_from_slice<D: de::Decode<()>, C: Config>(
    src: &[u8],
    config: C,
) -> Result<(D, usize), error::DecodeError> {
    let reader = de::read::SliceReader::new(src);
    let mut decoder = de::DecoderImpl::<_, C, ()>::new(reader, config, ());
    let result = D::decode(&mut decoder)?;
    let bytes_read = src.len() - decoder.reader().slice.len();
    Ok((result, bytes_read))
}

/// Decode a borrowed value from a byte slice using the given configuration.
///
/// The returned value may borrow strings and byte slices directly from `src`,
/// avoiding the allocations required by owned decoding. The returned `usize`
/// is the number of source bytes consumed.
pub fn borrow_decode_from_slice<'a, D: de::BorrowDecode<'a, ()>, C: Config>(
    src: &'a [u8],
    config: C,
) -> Result<(D, usize), error::DecodeError> {
    let reader = de::read::SliceReader::new(src);
    let mut decoder = de::DecoderImpl::<_, C, ()>::new(reader, config, ());
    let result = D::borrow_decode(&mut decoder)?;
    let bytes_read = src.len() - decoder.reader().slice.len();
    Ok((result, bytes_read))
}

#[cfg(doc)]
pub mod spec {
    #![doc = include_str!("../docs/spec.md")]
}
