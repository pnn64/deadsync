//! AVX2 SIMD kernels, x86_64-only (`core::arch::x86_64` intrinsics, zero dependencies),
//! enabled at compile time via [`crate::simd::avx2_enabled`]. The scalar decode paths own
//! every dispatch point and fallback; this module owns only the vector kernels, split by
//! pipeline stage into submodules (all entry points re-exported here, so call sites use
//! `crate::simd::<kernel>`):
//!
//! - `inter`: the inter-prediction two-pass 8-tap subpel convolution (spec §8.5.2.4),
//!   direct-load unscaled kernels (widths 8+ and 4) plus a general edge-clamping kernel
//!   for scaled references and unscaled reference-edge blocks; `predict.rs` dispatches.
//! - `intra`: the high-frequency intra-prediction modes (`V_PRED`, `H_PRED`, `DC_PRED`,
//!   `TM_PRED`, spec §8.5.1) across sizes 4/8/16/32 and all bit depths; `predict.rs`
//!   dispatches.
//! - `loop_filter`: the deblocking edge filters (spec §8.8.5, narrow / wide8 / wide16) for
//!   both edge orientations; `loop_filter.rs` dispatches.
//! - `transform`: the inverse DCT/ADST/WHT transforms fused with reconstruction (spec §8.7);
//!   `tile/residual.rs` dispatches (DCT_DCT, the ADST-containing types, and lossless 4x4 WHT
//!   at all bit depths -- see the landmines in docs/implementation-notes.md).
//!
//! A NEON mirror for aarch64 would be sibling `#[cfg(target_arch = "aarch64")]` modules
//! behind the same dispatch points -- not implemented.

mod inter;
mod intra;
mod loop_filter;
mod transform;

pub(crate) use inter::{block_inter_predict_scaled_u16_avx2, block_inter_predict_u16_avx2};
pub(crate) use intra::{
    intra_pred_dc_avx2, intra_pred_fill_avx2, intra_pred_h_avx2, intra_pred_tm_avx2,
    intra_pred_v_avx2,
};
pub(crate) use loop_filter::{loop_filter_horiz8_uniform_avx2, loop_filter_vert8_uniform_avx2};
pub(crate) use transform::{inverse_transform_reconstruct_avx2, reconstruct_add_const_avx2};

#[inline]
pub(crate) const fn avx2_enabled() -> bool {
    cfg!(target_feature = "avx2")
}

/// Narrows `src` (`u16` samples, each known to be in `0..=255`) into `dst` (`u8`) using AVX2
/// `_mm256_packus_epi16` (32 pixels per iteration) + SSE2 (16 pixels) + scalar tail.
///
/// # Safety
/// Caller must confirm `avx2_enabled()` and `dst.len() >= src.len()`.
#[target_feature(enable = "avx2")]
pub(crate) unsafe fn narrow_u16_to_u8_avx2(src: &[u16], dst: *mut u8) {
    use core::arch::x86_64::*;
    let len = src.len();
    let sp = src.as_ptr();
    let mut i = 0usize;
    while i + 32 <= len {
        let lo = _mm256_loadu_si256(sp.add(i) as *const __m256i);
        let hi = _mm256_loadu_si256(sp.add(i + 16) as *const __m256i);
        // packus_epi16 operates per 128-bit lane: [lo_0..8, hi_0..8 | lo_8..16, hi_8..16].
        // permute4x64(0xD8 = 0b11_01_10_00) restores contiguous order [lo_0..16, hi_0..16].
        let packed = _mm256_packus_epi16(lo, hi);
        let ordered = _mm256_permute4x64_epi64(packed, 0xD8);
        _mm256_storeu_si256(dst.add(i) as *mut __m256i, ordered);
        i += 32;
    }
    while i + 16 <= len {
        let lo = _mm_loadu_si128(sp.add(i) as *const __m128i);
        let hi = _mm_loadu_si128(sp.add(i + 8) as *const __m128i);
        let packed = _mm_packus_epi16(lo, hi);
        _mm_storeu_si128(dst.add(i) as *mut __m128i, packed);
        i += 16;
    }
    while i < len {
        *dst.add(i) = *sp.add(i) as u8;
        i += 1;
    }
}
