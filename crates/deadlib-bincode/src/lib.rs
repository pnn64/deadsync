#![warn(missing_docs, unused_lifetimes)]

//! `DeadSync`'s compact binary persistence codec.
//!
//! This is a deliberately reduced fork of bincode 2.0.1. It supports the
//! standard bincode wire format and the types used by `DeadSync`'s caches,
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
pub use impls::{encode_into_slice, encode_into_vec, encode_to_vec, encoded_size};

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

/// Decode a vector from a byte slice while reusing its allocation.
///
/// `values` is cleared before decoding but retains its capacity. After one
/// sufficiently large call, repeated calls can therefore decode without
/// allocator traffic. Allocations owned by `String` elements and nested numeric,
/// byte, or string vectors are reused as well. On failure, its contents are
/// unspecified and may include a successfully decoded prefix.
pub fn decode_from_slice_into_vec<T: de::Decode<()>, C: Config>(
    src: &[u8],
    values: &mut Vec<T>,
    config: C,
) -> Result<usize, error::DecodeError> {
    let reader = de::read::SliceReader::new(src);
    let mut decoder = de::DecoderImpl::<_, C, ()>::new(reader, config, ());
    impls::decode_vec_into(&mut decoder, values)?;
    Ok(src.len() - decoder.reader().slice.len())
}

/// Decode a string from a byte slice while reusing its allocation.
///
/// `value` is cleared before decoding but retains its capacity. After one
/// sufficiently large call, repeated calls can therefore decode without
/// allocator traffic. On failure, `value` is empty.
pub fn decode_from_slice_into_string<C: Config>(
    src: &[u8],
    value: &mut String,
    config: C,
) -> Result<usize, error::DecodeError> {
    let reader = de::read::SliceReader::new(src);
    let mut decoder = de::DecoderImpl::<_, C, ()>::new(reader, config, ());
    impls::decode_string_into(&mut decoder, value)?;
    Ok(src.len() - decoder.reader().slice.len())
}

/// Decode a hash map from a byte slice while reusing its allocation.
///
/// `values` is cleared before decoding but retains its buckets and hasher. On
/// warmed calls, allocations owned by `String`, `Vec<u8>`, and `Vec<String>`
/// keys and values are reused through a temporary pool. For `Vec<String>`,
/// both the vector and every nested string allocation are retained. On
/// failure, the map may contain a successfully decoded prefix.
pub fn decode_from_slice_into_hash_map<K, V, S, C>(
    src: &[u8],
    values: &mut std::collections::HashMap<K, V, S>,
    config: C,
) -> Result<usize, error::DecodeError>
where
    K: de::Decode<()> + Eq + std::hash::Hash,
    V: de::Decode<()>,
    S: std::hash::BuildHasher,
    C: Config,
{
    let reader = de::read::SliceReader::new(src);
    let mut decoder = de::DecoderImpl::<_, C, ()>::new(reader, config, ());
    impls::decode_hash_map_into(&mut decoder, values)?;
    Ok(src.len() - decoder.reader().slice.len())
}

/// Decode a hash set from a byte slice while reusing its allocation.
///
/// `values` is cleared before decoding but retains its buckets and hasher.
/// Allocations owned by `String`, `Vec<u8>`, and `Vec<String>` elements are
/// reused through a temporary pool on warmed calls. For `Vec<String>`, both
/// the vector and every nested string allocation are retained. On failure,
/// the set may contain a successfully decoded prefix.
pub fn decode_from_slice_into_hash_set<T, S, C>(
    src: &[u8],
    values: &mut std::collections::HashSet<T, S>,
    config: C,
) -> Result<usize, error::DecodeError>
where
    T: de::Decode<()> + Eq + std::hash::Hash,
    S: std::hash::BuildHasher,
    C: Config,
{
    let reader = de::read::SliceReader::new(src);
    let mut decoder = de::DecoderImpl::<_, C, ()>::new(reader, config, ());
    impls::decode_hash_set_into(&mut decoder, values)?;
    Ok(src.len() - decoder.reader().slice.len())
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

/// Decode a borrowed vector while reusing its allocation.
///
/// `values` is cleared before decoding but retains its capacity. Elements may
/// borrow directly from `src`. On failure, it may contain a successfully
/// decoded prefix.
pub fn borrow_decode_from_slice_into_vec<'a, T, C>(
    src: &'a [u8],
    values: &mut Vec<T>,
    config: C,
) -> Result<usize, error::DecodeError>
where
    T: de::BorrowDecode<'a, ()>,
    C: Config,
{
    let reader = de::read::SliceReader::new(src);
    let mut decoder = de::DecoderImpl::<_, C, ()>::new(reader, config, ());
    impls::borrow_decode_vec_into(&mut decoder, values)?;
    Ok(src.len() - decoder.reader().slice.len())
}

/// Decode a borrowed hash map while reusing its allocation.
///
/// `values` is cleared before decoding but retains its buckets and hasher.
/// Keys and values may borrow directly from `src`. On failure, the map may
/// contain a successfully decoded prefix.
pub fn borrow_decode_from_slice_into_hash_map<'a, K, V, S, C>(
    src: &'a [u8],
    values: &mut std::collections::HashMap<K, V, S>,
    config: C,
) -> Result<usize, error::DecodeError>
where
    K: de::BorrowDecode<'a, ()> + Eq + std::hash::Hash,
    V: de::BorrowDecode<'a, ()>,
    S: std::hash::BuildHasher,
    C: Config,
{
    let reader = de::read::SliceReader::new(src);
    let mut decoder = de::DecoderImpl::<_, C, ()>::new(reader, config, ());
    impls::borrow_decode_hash_map_into(&mut decoder, values)?;
    Ok(src.len() - decoder.reader().slice.len())
}

/// Decode a borrowed hash set while reusing its allocation.
///
/// `values` is cleared before decoding but retains its buckets and hasher.
/// Elements may borrow directly from `src`. On failure, the set may contain a
/// successfully decoded prefix.
pub fn borrow_decode_from_slice_into_hash_set<'a, T, S, C>(
    src: &'a [u8],
    values: &mut std::collections::HashSet<T, S>,
    config: C,
) -> Result<usize, error::DecodeError>
where
    T: de::BorrowDecode<'a, ()> + Eq + std::hash::Hash,
    S: std::hash::BuildHasher,
    C: Config,
{
    let reader = de::read::SliceReader::new(src);
    let mut decoder = de::DecoderImpl::<_, C, ()>::new(reader, config, ());
    impls::borrow_decode_hash_set_into(&mut decoder, values)?;
    Ok(src.len() - decoder.reader().slice.len())
}

#[cfg(doc)]
pub mod spec {
    #![doc = include_str!("../docs/spec.md")]
}
