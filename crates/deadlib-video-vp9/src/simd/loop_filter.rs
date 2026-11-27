//! AVX2 SIMD mirror of `loop_filter.rs`'s deblocking edge filters (spec §8.8.5: narrow /
//! wide8 / wide16), for both edge orientations: horizontal (`loop_filter_horiz8*_avx2`) and
//! vertical (`loop_filter_vert8*_avx2`, which transposes the tap window into the horizontal
//! kernel's layout and reuses its arithmetic).
//!
//! Includes early exits when no lane passes `filter_mask`, when no lane passes `flat_mask`
//! (skipping `wide8`, `p4..q7` loads, and `wide16`), and when no lane passes `flat_mask2`
//! (skipping `wide16`), plus sliding-sum formulations of `wide8`/`wide16` with zero multiplies.
//! `loop_filter.rs` owns the dispatch, MI-level parameter derivation, and scalar fallback.

use crate::predict::RefSample;
use core::arch::x86_64::*;

#[inline]
#[target_feature(enable = "avx2")]
unsafe fn load8<T: RefSample>(ptr: *const T) -> __m128i {
    if T::IS_U8 {
        _mm_cvtepu8_epi16(_mm_loadl_epi64(ptr.cast()))
    } else {
        _mm_loadu_si128(ptr.cast())
    }
}

#[inline]
#[target_feature(enable = "avx2")]
unsafe fn store8<T: RefSample>(ptr: *mut T, value: __m128i) {
    if T::IS_U8 {
        _mm_storel_epi64(ptr.cast(), _mm_packus_epi16(value, value));
    } else {
        _mm_storeu_si128(ptr.cast(), value);
    }
}

/// Result of running the horizontal kernel on an 8-lane batch, telling [`loop_filter_vert8_core`]
/// how much of the transposed tap window (if any) needs to be transposed back to the plane.
#[derive(Clone, Copy, PartialEq, Eq)]
enum FilterWriteScope {
    /// No lane passed `eligible && filter_mask`: nothing was written.
    None,
    /// Only rows within `p3..q3` (specifically `p1..q1` or `p2..q2`) were written.
    NarrowOrWide8,
    /// Rows across `p6..q6` were written (`wide16` active on at least one lane).
    Wide16,
}

/// Core 8-lane horizontal deblocking kernel. Returns which tap window was modified so the
/// vertical transpose wrapper can skip unchanged write-backs.
///
/// # Safety
/// Caller must confirm `avx2_enabled()` and that columns `x0..x0+8` over rows `y0-4..=y0+3`
/// (and `y0-8..=y0+7` whenever any eligible lane has `is_tx16_v` set) lie within `plane_base`
/// (stride `plane_width`).
#[inline(never)]
#[target_feature(enable = "avx2")]
#[allow(clippy::too_many_arguments)]
unsafe fn loop_filter_horiz8_core<T: RefSample>(
    plane_base: *mut T,
    plane_width: usize,
    x0: usize,
    y0: usize,
    eligible_v: __m256i,
    is_tx8_v: __m256i,
    is_tx16_v: __m256i,
    limit_v: __m256i,
    blimit_v: __m256i,
    thresh_v: __m256i,
    bit_depth: u8,
) -> FilterWriteScope {
    let base = plane_base;
    let load_row = |dy: i64| -> __m256i {
        let row_ptr = base.offset(((y0 as i64 + dy) * plane_width as i64 + x0 as i64) as isize);
        _mm256_cvtepu16_epi32(load8(row_ptr))
    };
    let p3 = load_row(-4);
    let p2 = load_row(-3);
    let p1 = load_row(-2);
    let p0 = load_row(-1);
    let q0 = load_row(0);
    let q1 = load_row(1);
    let q2 = load_row(2);
    let q3 = load_row(3);

    let abs_diff = |a: __m256i, b: __m256i| _mm256_abs_epi32(_mm256_sub_epi32(a, b));
    let gt = |a: __m256i, b: __m256i| _mm256_cmpgt_epi32(a, b);
    let any_diff_gt = |pairs: &[(__m256i, __m256i)], thresh: __m256i| {
        pairs.iter().fold(_mm256_setzero_si256(), |m, &(a, b)| {
            _mm256_or_si256(m, gt(abs_diff(a, b), thresh))
        })
    };

    // Spec §8.8.5.1 "Filter mask process".
    let p0q0 = abs_diff(p0, q0);
    let blimit_sum = _mm256_add_epi32(
        _mm256_add_epi32(p0q0, p0q0),
        _mm256_srli_epi32(abs_diff(p1, q1), 1),
    );
    let mask = _mm256_or_si256(
        any_diff_gt(
            &[(p3, p2), (p2, p1), (p1, p0), (q1, q0), (q2, q1), (q3, q2)],
            limit_v,
        ),
        gt(blimit_sum, blimit_v),
    );
    let elig_and_fm = _mm256_andnot_si256(mask, eligible_v);

    // Early exit: no eligible lane passed `filter_mask` -> skip all filter math and stores.
    if _mm256_testz_si256(elig_and_fm, elig_and_fm) != 0 {
        return FilterWriteScope::None;
    }

    let hev_mask = _mm256_or_si256(
        gt(abs_diff(p1, p0), thresh_v),
        gt(abs_diff(q1, q0), thresh_v),
    );

    // Spec §8.8.5.2 "Narrow filter process" (`filter4`).
    let clamp_hi = (128i32 << (bit_depth - 8)) - 1;
    let clamp_lo = -(clamp_hi + 1);
    let clamp_lo_v = _mm256_set1_epi32(clamp_lo);
    let clamp_hi_v = _mm256_set1_epi32(clamp_hi);
    let c128 = _mm256_set1_epi32(1 << (bit_depth - 1));
    let clamp4 = |v: __m256i| _mm256_min_epi32(_mm256_max_epi32(v, clamp_lo_v), clamp_hi_v);

    let ps1 = _mm256_sub_epi32(p1, c128);
    let ps0 = _mm256_sub_epi32(p0, c128);
    let qs0 = _mm256_sub_epi32(q0, c128);
    let qs1 = _mm256_sub_epi32(q1, c128);

    let filt_hev = clamp4(_mm256_sub_epi32(ps1, qs1));
    let filt_base = _mm256_and_si256(filt_hev, hev_mask);
    let q0_m_p0 = _mm256_sub_epi32(qs0, ps0);
    let three_q0_m_p0 = _mm256_add_epi32(q0_m_p0, _mm256_add_epi32(q0_m_p0, q0_m_p0));
    let filt = clamp4(_mm256_add_epi32(filt_base, three_q0_m_p0));
    let filter1 = _mm256_srai_epi32(clamp4(_mm256_add_epi32(filt, _mm256_set1_epi32(4))), 3);
    let filter2 = _mm256_srai_epi32(clamp4(_mm256_add_epi32(filt, _mm256_set1_epi32(3))), 3);

    let oq0_narrow = _mm256_add_epi32(clamp4(_mm256_sub_epi32(qs0, filter1)), c128);
    let op0_narrow = _mm256_add_epi32(clamp4(_mm256_add_epi32(ps0, filter2)), c128);

    let round_f = _mm256_srai_epi32(_mm256_add_epi32(filter1, _mm256_set1_epi32(1)), 1);
    let oq1_narrow = _mm256_blendv_epi8(
        _mm256_add_epi32(clamp4(_mm256_sub_epi32(qs1, round_f)), c128),
        q1,
        hev_mask,
    );
    let op1_narrow = _mm256_blendv_epi8(
        _mm256_add_epi32(clamp4(_mm256_add_epi32(ps1, round_f)), c128),
        p1,
        hev_mask,
    );

    let store_row = |dy: i64, v: __m256i| {
        let lo = _mm256_castsi256_si128(v);
        let hi = _mm256_extracti128_si256(v, 1);
        let u16x8 = _mm_packus_epi32(lo, hi);
        let row_ptr = base.offset(((y0 as i64 + dy) * plane_width as i64 + x0 as i64) as isize);
        store8(row_ptr, u16x8);
    };

    let store_narrow_only = || {
        store_row(-2, _mm256_blendv_epi8(p1, op1_narrow, elig_and_fm));
        store_row(-1, _mm256_blendv_epi8(p0, op0_narrow, elig_and_fm));
        store_row(0, _mm256_blendv_epi8(q0, oq0_narrow, elig_and_fm));
        store_row(1, _mm256_blendv_epi8(q1, oq1_narrow, elig_and_fm));
    };

    let is_wide_v = _mm256_or_si256(is_tx8_v, is_tx16_v);
    let elig_wide = _mm256_and_si256(elig_and_fm, is_wide_v);

    // Fast path 1: no active lane is TX_8X8 or TX_16X16 -> store narrow p1..q1 and return.
    if _mm256_testz_si256(elig_wide, elig_wide) != 0 {
        store_narrow_only();
        return FilterWriteScope::NarrowOrWide8;
    }

    // Spec §8.8.5.1 `flat_mask`.
    let one = _mm256_set1_epi32(1 << (bit_depth - 8));
    let fm = any_diff_gt(
        &[(p1, p0), (q1, q0), (p2, p0), (q2, q0), (p3, p0), (q3, q0)],
        one,
    );
    let flat_mask = _mm256_andnot_si256(fm, is_wide_v);
    let elig_flat = _mm256_and_si256(elig_and_fm, flat_mask);

    // Fast path 1b: no active wide lane passed `flat_mask` -> every active lane takes narrow!
    // Avoids computing wide8, loading p4..q7, computing wide16, and writing p2/q2.
    if _mm256_testz_si256(elig_flat, elig_flat) != 0 {
        store_narrow_only();
        return FilterWriteScope::NarrowOrWide8;
    }

    // Spec §8.8.5.3 "Wide filter process" (`log2_size == 3`, 8-tap flat filter) via sliding sum.
    let add = _mm256_add_epi32;
    let sub = _mm256_sub_epi32;
    let c4 = _mm256_set1_epi32(4);
    let round2_3 = |t: __m256i| _mm256_srai_epi32(add(t, c4), 3);

    // Sliding base = 3*p3 + p2 + p1 + p0 + q0 (7 terms; each output adds its center sample).
    let taps8 = [p3, p2, p1, p0, q0, q1, q2, q3];
    let mut b8 = add(add(add(p3, add(p3, p3)), add(p2, p1)), add(p0, q0));
    let mut w8 = [b8; 6];
    w8[0] = round2_3(add(b8, p2));
    for k in 1..6usize {
        b8 = add(sub(b8, taps8[k.saturating_sub(3)]), taps8[(k + 4).min(7)]);
        w8[k] = round2_3(add(b8, taps8[k + 1]));
    }
    let [op2_wide, op1_wide, op0_wide, oq0_wide, oq1_wide, oq2_wide] = w8;

    let store_narrow_or_wide8 = || {
        let use_wide8 = elig_flat;
        let use_narrow = _mm256_andnot_si256(flat_mask, elig_and_fm);
        let blend3 = |orig: __m256i, narrow: __m256i, wide: __m256i| {
            _mm256_blendv_epi8(
                _mm256_blendv_epi8(orig, narrow, use_narrow),
                wide,
                use_wide8,
            )
        };
        store_row(-3, _mm256_blendv_epi8(p2, op2_wide, use_wide8));
        store_row(-2, blend3(p1, op1_narrow, op1_wide));
        store_row(-1, blend3(p0, op0_narrow, op0_wide));
        store_row(0, blend3(q0, oq0_narrow, oq0_wide));
        store_row(1, blend3(q1, oq1_narrow, oq1_wide));
        store_row(2, _mm256_blendv_epi8(q2, oq2_wide, use_wide8));
    };

    // Fast path 2: no active flat lane is TX_16X16 -> skip p4..q7 loads & wide16.
    let elig_flat_tx16 = _mm256_and_si256(elig_flat, is_tx16_v);
    if _mm256_testz_si256(elig_flat_tx16, elig_flat_tx16) != 0 {
        store_narrow_or_wide8();
        return FilterWriteScope::NarrowOrWide8;
    }

    // Slow path: at least one active flat lane is TX_16X16. Load p4..p7 / q4..q7 and test flat_mask2.
    let [p4, p5, p6, p7, q4, q5, q6, q7] = [-5, -6, -7, -8, 4, 5, 6, 7].map(load_row);
    let fm2 = any_diff_gt(
        &[
            (p7, p0),
            (q7, q0),
            (p6, p0),
            (q6, q0),
            (p5, p0),
            (q5, q0),
            (p4, p0),
            (q4, q0),
        ],
        one,
    );
    let flat_mask2 = _mm256_andnot_si256(fm2, is_tx16_v);
    let use_wide16 = _mm256_and_si256(elig_flat, flat_mask2);

    // Fast path 2b: flat_mask2 failed on all active TX_16X16 lanes -> skip all 14 wide16 sums!
    if _mm256_testz_si256(use_wide16, use_wide16) != 0 {
        store_narrow_or_wide8();
        return FilterWriteScope::NarrowOrWide8;
    }

    let use_wide8 = _mm256_andnot_si256(flat_mask2, elig_flat);
    let use_narrow = _mm256_andnot_si256(flat_mask, elig_and_fm);

    // Spec §8.8.5.3 "Wide filter process" (`log2_size == 4`, 16-tap wide2 filter) via sliding sum:
    // `b16` holds the 15-term window sum without the doubled center tap; each output adds its
    // center sample and shifts with round2(4). Zero multiplies, exact integer reordering.
    let c8 = _mm256_set1_epi32(8);
    let round2_4 = |t: __m256i| _mm256_srai_epi32(add(t, c8), 4);
    let taps = [
        p7, p6, p5, p4, p3, p2, p1, p0, q0, q1, q2, q3, q4, q5, q6, q7,
    ];
    let mul7_p7 = sub(_mm256_slli_epi32(p7, 3), p7);
    let mut b16 = add(
        add(add(mul7_p7, p6), add(p5, p4)),
        add(add(p3, p2), add(p1, add(p0, q0))),
    );
    let mut w16 = [b16; 14];
    w16[0] = round2_4(add(b16, p6));
    for k in 1..14usize {
        let drop_tap = taps[k.saturating_sub(7)];
        let add_tap = taps[(k + 8).min(15)];
        b16 = add(sub(b16, drop_tap), add_tap);
        w16[k] = round2_4(add(b16, taps[k + 1]));
    }

    let blend_w16 = |orig: __m256i, w: __m256i| _mm256_blendv_epi8(orig, w, use_wide16);
    let blend_w8_w16 = |orig: __m256i, w8: __m256i, w: __m256i| {
        _mm256_blendv_epi8(_mm256_blendv_epi8(orig, w8, use_wide8), w, use_wide16)
    };
    let blend_all = |orig: __m256i, narrow: __m256i, w8: __m256i, w: __m256i| {
        _mm256_blendv_epi8(
            _mm256_blendv_epi8(_mm256_blendv_epi8(orig, narrow, use_narrow), w8, use_wide8),
            w,
            use_wide16,
        )
    };

    for k in 0..4 {
        store_row(-7 + k as i64, blend_w16(taps[k + 1], w16[k]));
        store_row(3 + k as i64, blend_w16(taps[k + 11], w16[k + 10]));
    }
    store_row(-3, blend_w8_w16(p2, op2_wide, w16[4]));
    store_row(-2, blend_all(p1, op1_narrow, op1_wide, w16[5]));
    store_row(-1, blend_all(p0, op0_narrow, op0_wide, w16[6]));
    store_row(0, blend_all(q0, oq0_narrow, oq0_wide, w16[7]));
    store_row(1, blend_all(q1, oq1_narrow, oq1_wide, w16[8]));
    store_row(2, blend_w8_w16(q2, oq2_wide, w16[9]));
    FilterWriteScope::Wide16
}

/// Builds `(eligible_v, is_tx8_v, is_tx16_v)` lane masks for uniform-parameter 8-lane batches
/// (`active_lanes` is 4 or 8; `filter_size` is `TX_4X4` (0), `TX_8X8` (1), or `TX_16X16` (2)).
///
/// # Safety
/// Caller must confirm `avx2_enabled()`.
#[inline]
#[target_feature(enable = "avx2")]
unsafe fn uniform_lane_masks(active_lanes: u32, filter_size: u8) -> (__m256i, __m256i, __m256i) {
    let eligible_v = if active_lanes == 8 {
        _mm256_set1_epi32(-1)
    } else {
        _mm256_setr_epi32(-1, -1, -1, -1, 0, 0, 0, 0)
    };
    let zero = _mm256_setzero_si256();
    let is_tx8_v = if filter_size == 1 { eligible_v } else { zero };
    let is_tx16_v = if filter_size == 2 { eligible_v } else { zero };
    (eligible_v, is_tx8_v, is_tx16_v)
}

/// Uniform-parameter horizontal 8-lane AVX2 loop filter: all active lanes (`active_lanes == 8`
/// for the common interior case, or `4` for the odd-MI chroma edge case) share one `(filter_size,
/// limit, blimit, thresh)` derived from a single MI lookup — no 6 stack `[i32; 8]` arrays.
///
/// # Safety
/// Caller must confirm `avx2_enabled()`, `active_lanes` is 4 or 8, `filter_size` is `TX_4X4` (0),
/// `TX_8X8` (1), or `TX_16X16` (2), and columns `x0..x0+8` over rows `y0-4..=y0+3` (and
/// `y0-8..=y0+7` when `filter_size == 2`) lie within `plane_base` (stride `plane_width`).
#[target_feature(enable = "avx2")]
#[allow(clippy::too_many_arguments)]
pub unsafe fn loop_filter_horiz8_uniform_avx2<T: RefSample>(
    plane_base: *mut T,
    plane_width: usize,
    x0: usize,
    y0: usize,
    active_lanes: u32,
    filter_size: u8,
    limit: i32,
    blimit: i32,
    thresh: i32,
    bit_depth: u8,
) {
    let (eligible_v, is_tx8_v, is_tx16_v) = uniform_lane_masks(active_lanes, filter_size);
    let _ = loop_filter_horiz8_core(
        plane_base,
        plane_width,
        x0,
        y0,
        eligible_v,
        is_tx8_v,
        is_tx16_v,
        _mm256_set1_epi32(limit),
        _mm256_set1_epi32(blimit),
        _mm256_set1_epi32(thresh),
        bit_depth,
    );
}

/// Transposes an 8x8 matrix of `u16` (`r[i]` = input row `i`, 8 lanes) so the returned
/// `out[j]` holds input column `j` across all 8 rows.
///
/// # Safety
/// Pure SSE2 register shuffle; caller must be on x86_64 (SSE2 is baseline).
#[inline]
unsafe fn transpose8x8_u16(r: &[__m128i; 8]) -> [__m128i; 8] {
    let a0 = _mm_unpacklo_epi16(r[0], r[1]);
    let a1 = _mm_unpackhi_epi16(r[0], r[1]);
    let a2 = _mm_unpacklo_epi16(r[2], r[3]);
    let a3 = _mm_unpackhi_epi16(r[2], r[3]);
    let a4 = _mm_unpacklo_epi16(r[4], r[5]);
    let a5 = _mm_unpackhi_epi16(r[4], r[5]);
    let a6 = _mm_unpacklo_epi16(r[6], r[7]);
    let a7 = _mm_unpackhi_epi16(r[6], r[7]);
    let b0 = _mm_unpacklo_epi32(a0, a2);
    let b1 = _mm_unpackhi_epi32(a0, a2);
    let b2 = _mm_unpacklo_epi32(a1, a3);
    let b3 = _mm_unpackhi_epi32(a1, a3);
    let b4 = _mm_unpacklo_epi32(a4, a6);
    let b5 = _mm_unpackhi_epi32(a4, a6);
    let b6 = _mm_unpacklo_epi32(a5, a7);
    let b7 = _mm_unpackhi_epi32(a5, a7);
    [
        _mm_unpacklo_epi64(b0, b4),
        _mm_unpackhi_epi64(b0, b4),
        _mm_unpacklo_epi64(b1, b5),
        _mm_unpackhi_epi64(b1, b5),
        _mm_unpacklo_epi64(b2, b6),
        _mm_unpackhi_epi64(b2, b6),
        _mm_unpacklo_epi64(b3, b7),
        _mm_unpackhi_epi64(b3, b7),
    ]
}

/// Transposes an 8x8 `u16` tile from `src` (stride `src_stride`) into `dst` (stride `dst_stride`).
///
/// # Safety
/// Caller must confirm `src` is valid for 8 rows of 8 `u16` reads at `src_stride` and `dst` is
/// valid for 8 rows of 8 `u16` writes at `dst_stride`.
#[inline]
#[target_feature(enable = "avx2")]
unsafe fn transpose_8x8_block<S: RefSample, D: RefSample>(
    src: *const S,
    src_stride: usize,
    dst: *mut D,
    dst_stride: usize,
) {
    let rows: [__m128i; 8] = core::array::from_fn(|r| load8(src.add(r * src_stride)));
    for (r, &v) in transpose8x8_u16(&rows).iter().enumerate() {
        store8(dst.add(r * dst_stride), v);
    }
}

/// Shared vertical-edge transpose driver: transposes the needed tap window (`p3..q3` or
/// `p7..q7`), calls [`loop_filter_horiz8_core`], and skips transposing/storing back when
/// nothing was modified (or stores only the inner `p3..q3` 8 columns when `wide16` did not fire).
///
/// # Safety
/// Caller must confirm `avx2_enabled()` and that columns `x0-4..=x0+3` (or `x0-8..=x0+7` when
/// `wide == true`) across rows `y0..=y0+7` lie within `plane_base` (stride `plane_width`).
#[inline(never)]
#[target_feature(enable = "avx2")]
#[allow(clippy::too_many_arguments)]
unsafe fn loop_filter_vert8_core<T: RefSample>(
    plane_base: *mut T,
    plane_width: usize,
    x0: usize,
    y0: usize,
    wide: bool,
    eligible_v: __m256i,
    is_tx8_v: __m256i,
    is_tx16_v: __m256i,
    limit_v: __m256i,
    blimit_v: __m256i,
    thresh_v: __m256i,
    bit_depth: u8,
) {
    let center = if wide { 8usize } else { 4usize };
    let col0 = x0 - center;
    let base = plane_base;
    let mut scratch = [0u16; 16 * 8];

    transpose_8x8_block(
        base.add(y0 * plane_width + col0),
        plane_width,
        scratch.as_mut_ptr(),
        8,
    );
    if wide {
        transpose_8x8_block(
            base.add(y0 * plane_width + col0 + 8),
            plane_width,
            scratch.as_mut_ptr().add(64),
            8,
        );
    }

    let scope = loop_filter_horiz8_core(
        scratch.as_mut_ptr(),
        8,
        0,
        center,
        eligible_v,
        is_tx8_v,
        is_tx16_v,
        limit_v,
        blimit_v,
        thresh_v,
        bit_depth,
    );

    match scope {
        FilterWriteScope::None => {}
        FilterWriteScope::NarrowOrWide8 => {
            transpose_8x8_block(
                scratch.as_ptr().add((center - 4) * 8),
                8,
                base.add(y0 * plane_width + (x0 - 4)),
                plane_width,
            );
        }
        FilterWriteScope::Wide16 => {
            transpose_8x8_block(
                scratch.as_ptr(),
                8,
                base.add(y0 * plane_width + col0),
                plane_width,
            );
            transpose_8x8_block(
                scratch.as_ptr().add(64),
                8,
                base.add(y0 * plane_width + col0 + 8),
                plane_width,
            );
        }
    }
}

/// Uniform-parameter vertical 8-lane AVX2 loop filter: single MI lookup feeds all active lanes
/// (`active_lanes` is 4 or 8).
///
/// # Safety
/// Caller must confirm `avx2_enabled()`, `active_lanes` is 4 or 8, `filter_size` is `TX_4X4` (0),
/// `TX_8X8` (1), or `TX_16X16` (2), and columns `x0-4..=x0+3` (or `x0-8..=x0+7` when
/// `filter_size == 2`) across rows `y0..=y0+7` lie within `plane_base` (stride `plane_width`).
#[target_feature(enable = "avx2")]
#[allow(clippy::too_many_arguments)]
pub unsafe fn loop_filter_vert8_uniform_avx2<T: RefSample>(
    plane_base: *mut T,
    plane_width: usize,
    x0: usize,
    y0: usize,
    active_lanes: u32,
    filter_size: u8,
    limit: i32,
    blimit: i32,
    thresh: i32,
    bit_depth: u8,
) {
    let (eligible_v, is_tx8_v, is_tx16_v) = uniform_lane_masks(active_lanes, filter_size);
    loop_filter_vert8_core(
        plane_base,
        plane_width,
        x0,
        y0,
        filter_size == 2,
        eligible_v,
        is_tx8_v,
        is_tx16_v,
        _mm256_set1_epi32(limit),
        _mm256_set1_epi32(blimit),
        _mm256_set1_epi32(thresh),
        bit_depth,
    );
}
