//! 8-bit AVX2 inverse ADST networks (`iadst4/8/16`, spec §8.7.1.4-§8.7.1.9) on 8-lane `i32`
//! element vectors.
//!
//! Overflow safety at `bit_depth == 8` (from spec §8.7.1.1 / §8.7.2 conformance: every stored `T`
//! value fits signed `8 + BitDepth == 16` bits, so `|T| <= 2^15`; `|cos64| <= 2^14`; max
//! `|cos64| + |sin64| == 23170`):
//! - every `SB` product `T * cos64` is `<= 2^29`, so `_mm256_mullo_epi32` is exact;
//! - every unrounded `S` value is `<= 2^15 * 23170 < 2^30`, inside the `i32` lanes;
//! - every `SH` sum `S[a] +/- S[b]` (+ the `2^13` round) is `< 2^31`;
//! - the `iadst4` chains peak at `(SINPI_1 + SINPI_2 + SINPI_3 + SINPI_4) * 2^15 < 2^31`, and
//!   `SINPI_3_9 * (T0 - T2 + T3) <= 13377 * 3 * 2^15 < 2^31`.
//!
//! At 10/12-bit, the spec's bound on `S` is `24 + BitDepth` (up to 36) bits -- beyond `i32` lane
//! storage -- so `tile/residual.rs` routes 10/12-bit ADST to `adst_hbd.rs` (i64 lanes).

use core::arch::x86_64::*;

use super::dct::{b_op_simd, h_op_simd, round2_14};

/// Vector `SB(a, b, angle, flip)` (`transform.rs::sb_op`, spec §8.7.1.1) on 8 rows: the
/// butterfly rotation into the high-precision `S` array, unrounded. 8-bit only.
///
/// # Safety
/// Caller must confirm `avx2_enabled()` and `a, b < t.len().min(s.len())`.
#[inline]
#[target_feature(enable = "avx2")]
pub(in crate::simd) unsafe fn sb_op_simd(
    s: &mut [__m256i],
    t: &[__m256i],
    a: usize,
    b: usize,
    angle: i32,
    flip: bool,
) {
    let cos = _mm256_set1_epi32(crate::transform::cos64(angle) as i32);
    let sin = _mm256_set1_epi32(crate::transform::sin64(angle) as i32);
    let ta = t[a];
    let tb = t[b];
    let sa = _mm256_sub_epi32(_mm256_mullo_epi32(ta, cos), _mm256_mullo_epi32(tb, sin));
    let sb = _mm256_add_epi32(_mm256_mullo_epi32(ta, sin), _mm256_mullo_epi32(tb, cos));
    if flip {
        s[a] = sb;
        s[b] = sa;
    } else {
        s[a] = sa;
        s[b] = sb;
    }
}

/// Vector `SH(a, b)` (`transform.rs::sh_op`, spec §8.7.1.1) on 8 rows: Hadamard + `round2(14)`
/// from `S` back into `T`. 8-bit only.
///
/// # Safety
/// Caller must confirm `avx2_enabled()` and `a, b < t.len().min(s.len())`.
#[inline]
#[target_feature(enable = "avx2")]
pub(in crate::simd) unsafe fn sh_op_simd(t: &mut [__m256i], s: &[__m256i], a: usize, b: usize) {
    t[a] = round2_14(_mm256_add_epi32(s[a], s[b]));
    t[b] = round2_14(_mm256_sub_epi32(s[a], s[b]));
}

/// Vector inverse ADST4 (`transform.rs::iadst4_impl`, spec §8.7.1.6) on 8 rows.
///
/// # Safety
/// Caller must confirm `avx2_enabled()` and `t.len() >= 4`.
#[target_feature(enable = "avx2")]
unsafe fn iadst4_simd(t: &mut [__m256i]) {
    let mul = |v: __m256i, c: i64| _mm256_mullo_epi32(v, _mm256_set1_epi32(c as i32));
    crate::transform::iadst4_body!(t, mul, _mm256_add_epi32, _mm256_sub_epi32, round2_14);
}

/// Vector inverse ADST8 (`transform.rs::iadst8_impl`, spec §8.7.1.7) on 8 rows.
///
/// # Safety
/// Caller must confirm `avx2_enabled()` and `t.len() >= 8`.
#[target_feature(enable = "avx2")]
unsafe fn iadst8_simd(t: &mut [__m256i]) {
    let zero = _mm256_setzero_si256();
    crate::transform::iadst8_body!(t, zero, sb_op_simd, sh_op_simd, h_op_simd, b_op_simd, |v| {
        _mm256_sub_epi32(zero, v)
    });
}

/// Vector inverse ADST16 (`transform.rs::iadst16_impl`, spec §8.7.1.8) on 8 rows.
///
/// # Safety
/// Caller must confirm `avx2_enabled()` and `t.len() >= 16`.
#[target_feature(enable = "avx2")]
unsafe fn iadst16_simd(t: &mut [__m256i]) {
    let zero = _mm256_setzero_si256();
    crate::transform::iadst16_body!(t, zero, sb_op_simd, sh_op_simd, h_op_simd, b_op_simd, |v| {
        _mm256_sub_epi32(zero, v)
    });
}

/// Vector `iadst(t, n)` (`transform.rs::iadst`, spec §8.7.1.9): ADST4/8/16 by size.
///
/// # Safety
/// Caller must confirm `avx2_enabled()`, `2 <= n <= 4`, and `t.len() >= (1 << n)`.
#[target_feature(enable = "avx2")]
pub(in crate::simd) unsafe fn iadst_simd(t: &mut [__m256i], n: u32) {
    match n {
        2 => iadst4_simd(t),
        3 => iadst8_simd(t),
        4 => iadst16_simd(t),
        _ => unreachable!("iadst_simd only supports n = 2..=4"),
    }
}
