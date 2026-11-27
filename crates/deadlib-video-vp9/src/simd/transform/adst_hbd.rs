//! High-bit-depth (10/12-bit) AVX2 inverse ADST + mixed-axis DCT networks on 4-lane `i64`
//! element vectors.
//!
//! At 10/12-bit, spec §8.7.1.1 bounds ADST's unrounded `S` array to signed `24 + BitDepth` bits
//! (up to 36 bits at 12-bit), exceeding `i32` lane storage. These kernels mirror the scalar
//! `i64` networks verbatim on 4 `i64` lanes per `__m256i` (emulating 64-bit multiplies and
//! arithmetic right shifts in AVX2). `TX_32X32` is DCT-only and uses the `i32`-lane HBD DCT in
//! `dct.rs`, so this module only runs for `n = 2..=4` (4x4, 8x8, 16x16).

use core::arch::x86_64::*;

use super::dct::idct_permute_simd;

/// Exact `lane * c` on 4 `i64` lanes (low 64 bits):
/// `a*c mod 2^64 = a_lo*c_lo + ((a_hi*c_lo + a_lo*c_hi) << 32)`.
///
/// # Safety
/// Caller must confirm `avx2_enabled()`.
#[inline]
#[target_feature(enable = "avx2")]
unsafe fn mul_i64(a: __m256i, c: i64) -> __m256i {
    let b = _mm256_set1_epi64x(c);
    let a_hi = _mm256_srli_epi64(a, 32);
    let b_hi = _mm256_srli_epi64(b, 32);
    let cross = _mm256_add_epi32(_mm256_mullo_epi32(a_hi, b), _mm256_mullo_epi32(a, b_hi));
    _mm256_add_epi64(_mm256_mul_epu32(a, b), _mm256_slli_epi64(cross, 32))
}

/// `round2(x, n)` (`transform.rs::round2`) on 4 `i64` lanes using an emulated 64-bit arithmetic
/// right shift. Exact for every `i64` lane value.
///
/// # Safety
/// Caller must confirm `avx2_enabled()` and `1 <= n < 64`.
#[inline]
#[target_feature(enable = "avx2")]
unsafe fn round2_i64(x: __m256i, n: u32) -> __m256i {
    let biased = _mm256_add_epi64(x, _mm256_set1_epi64x(1i64 << (n - 1)));
    let sign = _mm256_cmpgt_epi64(_mm256_setzero_si256(), biased);
    _mm256_or_si256(
        _mm256_srl_epi64(biased, _mm_cvtsi32_si128(n as i32)),
        _mm256_sll_epi64(sign, _mm_cvtsi32_si128(64 - n as i32)),
    )
}

/// `B(a, b, angle, flip)` (`transform.rs::b_op`) on 4 `i64` lanes.
///
/// # Safety
/// Caller must confirm `avx2_enabled()` and `a, b < t.len()`.
#[inline]
#[target_feature(enable = "avx2")]
unsafe fn b_op_i64(t: &mut [__m256i], a: usize, b: usize, angle: i32, flip: bool) {
    let cos = crate::transform::cos64(angle);
    let sin = crate::transform::sin64(angle);
    let ta = t[a];
    let tb = t[b];
    let x = _mm256_sub_epi64(mul_i64(ta, cos), mul_i64(tb, sin));
    let y = _mm256_add_epi64(mul_i64(ta, sin), mul_i64(tb, cos));
    let na = round2_i64(x, 14);
    let nb = round2_i64(y, 14);
    if flip {
        t[a] = nb;
        t[b] = na;
    } else {
        t[a] = na;
        t[b] = nb;
    }
}

/// `H(a, b, flip)` (`transform.rs::h_op`) on 4 `i64` lanes.
///
/// # Safety
/// Caller must confirm `avx2_enabled()` and `a, b < t.len()`.
#[inline]
#[target_feature(enable = "avx2")]
unsafe fn h_op_i64(t: &mut [__m256i], a: usize, b: usize, flip: bool) {
    let (a, b) = if flip { (b, a) } else { (a, b) };
    let x = t[a];
    let y = t[b];
    t[a] = _mm256_add_epi64(x, y);
    t[b] = _mm256_sub_epi64(x, y);
}

/// `SB(a, b, angle, flip)` (`transform.rs::sb_op`) on 4 `i64` lanes.
///
/// # Safety
/// Caller must confirm `avx2_enabled()` and `a, b < t.len().min(s.len())`.
#[inline]
#[target_feature(enable = "avx2")]
unsafe fn sb_op_i64(s: &mut [__m256i], t: &[__m256i], a: usize, b: usize, angle: i32, flip: bool) {
    let cos = crate::transform::cos64(angle);
    let sin = crate::transform::sin64(angle);
    let ta = t[a];
    let tb = t[b];
    let sa = _mm256_sub_epi64(mul_i64(ta, cos), mul_i64(tb, sin));
    let sb = _mm256_add_epi64(mul_i64(ta, sin), mul_i64(tb, cos));
    if flip {
        s[a] = sb;
        s[b] = sa;
    } else {
        s[a] = sa;
        s[b] = sb;
    }
}

/// `SH(a, b)` (`transform.rs::sh_op`) on 4 `i64` lanes.
///
/// # Safety
/// Caller must confirm `avx2_enabled()` and `a, b < t.len().min(s.len())`.
#[inline]
#[target_feature(enable = "avx2")]
unsafe fn sh_op_i64(t: &mut [__m256i], s: &[__m256i], a: usize, b: usize) {
    t[a] = round2_i64(_mm256_add_epi64(s[a], s[b]), 14);
    t[b] = round2_i64(_mm256_sub_epi64(s[a], s[b]), 14);
}

/// Inverse ADST4 (`transform.rs::iadst4_impl`, spec §8.7.1.6) on 4 `i64` lanes.
///
/// # Safety
/// Caller must confirm `avx2_enabled()` and `t.len() >= 4`.
#[target_feature(enable = "avx2")]
unsafe fn iadst4_i64(t: &mut [__m256i]) {
    crate::transform::iadst4_body!(t, mul_i64, _mm256_add_epi64, _mm256_sub_epi64, |v| {
        round2_i64(v, 14)
    });
}

/// Inverse ADST8 (`transform.rs::iadst8_impl`, spec §8.7.1.7) on 4 `i64` lanes.
///
/// # Safety
/// Caller must confirm `avx2_enabled()` and `t.len() >= 8`.
#[target_feature(enable = "avx2")]
unsafe fn iadst8_i64(t: &mut [__m256i]) {
    let zero = _mm256_setzero_si256();
    crate::transform::iadst8_body!(t, zero, sb_op_i64, sh_op_i64, h_op_i64, b_op_i64, |v| {
        _mm256_sub_epi64(zero, v)
    });
}

/// Inverse ADST16 (`transform.rs::iadst16_impl`, spec §8.7.1.8) on 4 `i64` lanes.
///
/// # Safety
/// Caller must confirm `avx2_enabled()` and `t.len() >= 16`.
#[target_feature(enable = "avx2")]
unsafe fn iadst16_i64(t: &mut [__m256i]) {
    let zero = _mm256_setzero_si256();
    crate::transform::iadst16_body!(t, zero, sb_op_i64, sh_op_i64, h_op_i64, b_op_i64, |v| {
        _mm256_sub_epi64(zero, v)
    });
}

/// `iadst(t, n)` (`transform.rs::iadst`, spec §8.7.1.9) on 4 `i64` lanes: ADST4/8/16 by size.
///
/// # Safety
/// Caller must confirm `avx2_enabled()`, `2 <= n <= 4`, and `t.len() >= (1 << n)`.
#[target_feature(enable = "avx2")]
pub(in crate::simd) unsafe fn iadst_i64(t: &mut [__m256i], n: u32) {
    match n {
        2 => iadst4_i64(t),
        3 => iadst8_i64(t),
        4 => iadst16_i64(t),
        _ => unreachable!("iadst_i64 only supports n = 2..=4"),
    }
}

/// Inverse-DCT butterfly network (`transform.rs::idct`, spec §8.7.1.3) on 4 `i64` lanes -- the
/// mixed types' DCT axis. Assumes `idct_permute_simd` ran first.
///
/// # Safety
/// Caller must confirm `avx2_enabled()`, `2 <= n <= 5`, and `t.len() >= (1 << n)`.
#[target_feature(enable = "avx2")]
pub(in crate::simd) unsafe fn idct_i64(t: &mut [__m256i], n: u32) {
    crate::transform::idct_body!(t, n, b_op_i64, h_op_i64, idct_i64);
}

/// Transposes a 4x4 `i64` matrix (`r[i]` = row i, 4 `i64` lanes) so `out[j]` = column j.
///
/// # Safety
/// Caller must confirm `avx2_enabled()`.
#[inline]
#[target_feature(enable = "avx2")]
unsafe fn transpose4x4_i64(r: &[__m256i; 4]) -> [__m256i; 4] {
    let t0 = _mm256_unpacklo_epi64(r[0], r[1]);
    let t1 = _mm256_unpackhi_epi64(r[0], r[1]);
    let t2 = _mm256_unpacklo_epi64(r[2], r[3]);
    let t3 = _mm256_unpackhi_epi64(r[2], r[3]);
    [
        _mm256_permute2x128_si256(t0, t2, 0x20),
        _mm256_permute2x128_si256(t1, t3, 0x20),
        _mm256_permute2x128_si256(t0, t2, 0x31),
        _mm256_permute2x128_si256(t1, t3, 0x31),
    ]
}

/// One separable pass of the `i64`-lane 2D inverse transform (10/12-bit ADST-containing driver),
/// transforming 4 rows at a time and writing the result transposed into `output`.
///
/// # Safety
/// Caller must confirm `avx2_enabled()` and `2 <= n <= 4`.
#[inline(never)]
#[target_feature(enable = "avx2")]
pub(super) unsafe fn xform_pass_i64<const ADST: bool>(
    input: &[i64; 256],
    output: &mut [i64; 256],
    n: u32,
    round_shift: Option<u32>,
) {
    debug_assert!((2..=4).contains(&n));
    let n0 = 1usize << n;
    let mut cs = 0usize;
    while cs < n0 {
        let mut t = [_mm256_setzero_si256(); 16];
        let mut g = 0usize;
        while g < n0 {
            let mut rows4 = [_mm256_setzero_si256(); 4];
            for (r, slot) in rows4.iter_mut().enumerate() {
                *slot = _mm256_loadu_si256(input.as_ptr().add((cs + r) * n0 + g) as *const __m256i);
            }
            let cols = transpose4x4_i64(&rows4);
            t[g..g + 4].copy_from_slice(&cols);
            g += 4;
        }
        if ADST {
            iadst_i64(&mut t, n);
        } else {
            idct_permute_simd(&mut t, n);
            idct_i64(&mut t, n);
        }
        for (k, tk) in t[..n0].iter().enumerate() {
            let v = round_shift.map_or(*tk, |sh| round2_i64(*tk, sh));
            _mm256_storeu_si256(output.as_mut_ptr().add(k * n0 + cs) as *mut __m256i, v);
        }
        cs += 4;
    }
}
