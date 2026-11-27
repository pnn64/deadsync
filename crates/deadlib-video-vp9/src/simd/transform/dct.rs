//! 8-lane AVX2 inverse DCT butterfly network (spec §8.7.1.3), supporting both 8-bit (`HBD = false`,
//! `_mm256_mullo_epi32` products) and 10/12-bit (`HBD = true`, widened 32x32->64-bit products via
//! `_mm256_mul_epi32` + i64 rounding). Lane storage is `i32` at every bit depth per spec §8.7.1.1's
//! `8 + BitDepth <= 20`-bit bound on stored `T` values.

use core::arch::x86_64::*;

/// `round2(x, 14)` (`transform.rs::round2`) lane-wise; the 8-bit `b_op` and `sh_op` sums fit i32,
/// so `(x + 2^13) >> 14` arithmetic-shifted matches the scalar i64 `round2`.
///
/// # Safety
/// Caller must confirm `avx2_enabled()`.
#[inline]
#[target_feature(enable = "avx2")]
pub(super) unsafe fn round2_14(x: __m256i) -> __m256i {
    _mm256_srai_epi32(_mm256_add_epi32(x, _mm256_set1_epi32(1 << 13)), 14)
}

/// Vector `B(a, b, angle, flip)` butterfly (`transform.rs::b_op`) on 8 rows at once (8-bit).
///
/// # Safety
/// Caller must confirm `avx2_enabled()` and `a, b < t.len()`.
#[inline]
#[target_feature(enable = "avx2")]
pub(in crate::simd) unsafe fn b_op_simd(
    t: &mut [__m256i],
    a: usize,
    b: usize,
    angle: i32,
    flip: bool,
) {
    let cos = _mm256_set1_epi32(crate::transform::cos64(angle) as i32);
    let sin = _mm256_set1_epi32(crate::transform::sin64(angle) as i32);
    let ta = t[a];
    let tb = t[b];
    // x = ta*cos - tb*sin ; y = ta*sin + tb*cos ; each product fits i32 (mullo is exact there).
    let x = _mm256_sub_epi32(_mm256_mullo_epi32(ta, cos), _mm256_mullo_epi32(tb, sin));
    let y = _mm256_add_epi32(_mm256_mullo_epi32(ta, sin), _mm256_mullo_epi32(tb, cos));
    let na = round2_14(x);
    let nb = round2_14(y);
    if flip {
        t[a] = nb;
        t[b] = na;
    } else {
        t[a] = na;
        t[b] = nb;
    }
}

/// High-bit-depth `B(a, b, angle, flip)`: identical network arithmetic to [`b_op_simd`], but
/// the `t * cos64` products are computed with 32x32 -> 64-bit widening multiplies
/// (`_mm256_mul_epi32`) and rounded in i64, because at 10/12-bit a stored T value spans up to
/// signed `8 + BitDepth <= 20` bits (spec §8.7.1.1) and `t * cos64` (20b * 15b) would wrap
/// `_mm256_mullo_epi32`. Overflow-safe by that same bound: |t| < 2^19 and |cos64| <= 2^14, so
/// |ta*cos - tb*sin| + 2^13 < 2^35 fits i64 exactly. The `>> 14` uses a LOGICAL 64-bit shift
/// (AVX2 has no 64-bit arithmetic shift): logical and arithmetic shifts differ only in the top
/// 14 bits of the 64-bit result, and only the low 32 bits are kept -- exact whenever the true
/// `round2` result fits i32, which §8.7.1.1 guarantees (it is a stored T value).
///
/// # Safety
/// Caller must confirm `avx2_enabled()` and `a, b < t.len()`.
#[inline]
#[target_feature(enable = "avx2")]
pub(in crate::simd) unsafe fn b_op_simd_hbd(
    t: &mut [__m256i],
    a: usize,
    b: usize,
    angle: i32,
    flip: bool,
) {
    let cos = _mm256_set1_epi32(crate::transform::cos64(angle) as i32);
    let sin = _mm256_set1_epi32(crate::transform::sin64(angle) as i32);
    let ta = t[a];
    let tb = t[b];
    // `_mm256_mul_epi32` multiplies the (sign-extended) low 32 bits of each 64-bit lane, i.e.
    // i32 lanes 0/2/4/6. The logical 64-bit shift moves lanes 1/3/5/7 into those positions
    // (the zeroed high halves are ignored by the multiply).
    let ta_odd = _mm256_srli_epi64(ta, 32);
    let tb_odd = _mm256_srli_epi64(tb, 32);
    let round = _mm256_set1_epi64x(1 << 13);
    // x = ta*cos - tb*sin ; y = ta*sin + tb*cos, each + 2^13, in exact i64 (per even/odd half).
    let x_even = _mm256_add_epi64(
        _mm256_sub_epi64(_mm256_mul_epi32(ta, cos), _mm256_mul_epi32(tb, sin)),
        round,
    );
    let x_odd = _mm256_add_epi64(
        _mm256_sub_epi64(_mm256_mul_epi32(ta_odd, cos), _mm256_mul_epi32(tb_odd, sin)),
        round,
    );
    let y_even = _mm256_add_epi64(
        _mm256_add_epi64(_mm256_mul_epi32(ta, sin), _mm256_mul_epi32(tb, cos)),
        round,
    );
    let y_odd = _mm256_add_epi64(
        _mm256_add_epi64(_mm256_mul_epi32(ta_odd, sin), _mm256_mul_epi32(tb_odd, cos)),
        round,
    );
    // `round2(., 14)` per 64-bit lane (logical shift, low 32 bits exact -- see above), then
    // re-interleave the even/odd halves back into 8 i32 lanes.
    let combine = |even: __m256i, odd: __m256i| {
        _mm256_blend_epi32(
            _mm256_srli_epi64(even, 14),
            _mm256_slli_epi64(_mm256_srli_epi64(odd, 14), 32),
            0b1010_1010,
        )
    };
    let na = combine(x_even, x_odd);
    let nb = combine(y_even, y_odd);
    if flip {
        t[a] = nb;
        t[b] = na;
    } else {
        t[a] = na;
        t[b] = nb;
    }
}

/// Selects the depth-correct butterfly: `b_op_simd` (i32 products, bit_depth == 8) or
/// `b_op_simd_hbd` (widened i64 products, 10/12-bit). Monomorphized away -- the 8-bit network
/// is unchanged.
///
/// # Safety
/// Caller must confirm `avx2_enabled()` and `a, b < t.len()`.
#[inline]
#[target_feature(enable = "avx2")]
unsafe fn b_op_simd_depth<const HBD: bool>(
    t: &mut [__m256i],
    a: usize,
    b: usize,
    angle: i32,
    flip: bool,
) {
    if HBD {
        b_op_simd_hbd(t, a, b, angle, flip);
    } else {
        b_op_simd(t, a, b, angle, flip);
    }
}

/// Vector `H(a, b, flip)` Hadamard rotation (`transform.rs::h_op`) on 8 rows.
///
/// # Safety
/// Caller must confirm `avx2_enabled()` and `a, b < t.len()`.
#[inline]
#[target_feature(enable = "avx2")]
pub(super) unsafe fn h_op_simd(t: &mut [__m256i], a: usize, b: usize, flip: bool) {
    let (a, b) = if flip { (b, a) } else { (a, b) };
    let x = t[a];
    let y = t[b];
    t[a] = _mm256_add_epi32(x, y);
    t[b] = _mm256_sub_epi32(x, y);
}

/// Vector `idct_permute` (`transform.rs::idct_permute`): bit-reversal reorder of the element
/// vectors (no arithmetic, so lane-agnostic and shared between i32 and i64 networks).
///
/// # Safety
/// Caller must confirm `avx2_enabled()`, `n <= 5`, and `t.len() >= (1 << n)`.
#[inline]
#[target_feature(enable = "avx2")]
pub(in crate::simd) unsafe fn idct_permute_simd(t: &mut [__m256i], n: u32) {
    crate::transform::idct_permute(t, n);
}

/// Vector inverse-DCT butterfly network -- shares [`crate::transform::idct_body!`] with the
/// scalar `i64` network, operating on 8-row `i32` element vectors. Assumes `idct_permute_simd`
/// ran first. `HBD` selects the butterfly's multiply width (`b_op_simd_depth`).
///
/// # Safety
/// Caller must confirm `avx2_enabled()`, `2 <= n <= 5`, and `t.len() >= (1 << n)`.
#[target_feature(enable = "avx2")]
pub(in crate::simd) unsafe fn idct_simd<const HBD: bool>(t: &mut [__m256i], n: u32) {
    crate::transform::idct_body!(t, n, b_op_simd_depth::<HBD>, h_op_simd, idct_simd::<HBD>);
}
