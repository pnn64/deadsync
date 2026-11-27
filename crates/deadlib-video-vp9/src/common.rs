//! Spec constants/helpers shared verbatim by two or more otherwise-unrelated modules
//! (previously duplicated independently in each). Doc-hidden internal module, same
//! convention as the rest of `src/` (see `lib.rs`); not part of the crate's public API.

use crate::prob_tables::{BLOCK_8X8, MAX_TXSIZE_LOOKUP, SS_SIZE_LOOKUP, TX_4X4};

/// Spec §4.6 `Clip3(x, y, z)` for the decoder's signed arithmetic paths.
#[inline]
pub(crate) fn clip3(low: i32, high: i32, value: i32) -> i32 {
    if value < low {
        low
    } else if value > high {
        high
    } else {
        value
    }
}

/// Spec §4.7 `Round2(x, n)` for the decoder's `i32` prediction/filter paths.
#[inline]
pub(crate) fn round2(value: i32, bits: u32) -> i32 {
    if bits == 0 {
        value
    } else {
        (value + (1 << (bits - 1))) >> bits
    }
}

/// `MAX_SEGMENTS` (spec §3). The number of `segment_id` values / size of the
/// segmentation-map-indexed arrays.
pub const MAX_SEGMENTS: usize = 8;

/// `ref_frame[ 0 ]`/`ref_frame[ 1 ]` value for intra blocks / "no second reference" (spec
/// §7.4.12). Also usable as a plain reference-frame-type index (`RefFrame` enum, spec §3).
pub const INTRA_FRAME: u8 = 0;

/// `get_uv_tx_size( )` (spec §6.4.22). Free-function form so it can be shared between
/// `TileDecoder::get_uv_tx_size` (`src/tile/residual.rs`, which has a `self.subsampling_x`/
/// `self.subsampling_y` to supply) and the loop filter's frame-wide traversal
/// (`src/loop_filter.rs`, a free function with no `TileDecoder` to borrow).
pub fn get_uv_tx_size(mi_size: u8, tx_size: u8, subsampling_x: u32, subsampling_y: u32) -> u8 {
    if mi_size < BLOCK_8X8 {
        return TX_4X4;
    }
    let plane_sz = SS_SIZE_LOOKUP[mi_size as usize][subsampling_x as usize][subsampling_y as usize];
    tx_size.min(MAX_TXSIZE_LOOKUP[plane_sz as usize])
}
