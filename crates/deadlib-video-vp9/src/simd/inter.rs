//! AVX2 SIMD mirror of `predict::block_inter_predict`'s scalar two-pass 8-tap subpel
//! convolution (spec §8.5.2.4): specialized kernels for the common UNSCALED case
//! (`x_step == y_step == 16`, i.e. reference frame same size as the current frame) plus a
//! general edge-clamping kernel ([`block_inter_predict_scaled_u16_avx2`]). The direct-to-plane
//! `*_u16_avx2` entries write `u16` samples straight into the destination plane (with optional
//! compound averaging via `VPAVGW` / `PAVGW`, matching `round2(a + b, 1)`), dispatch full-pel
//! copy and 1D horizontal/vertical fast paths when `fx == 0` or `fy == 0`, use `_mm256_madd_epi16`
//! for 2 taps per instruction across all bit depths (8/10/12-bit), and use uninitialized stack
//! scratch so small blocks never pay a 34 KB zero-fill. `predict.rs` owns the dispatch and the
//! scalar fallback; this module owns only the vector kernels.

use crate::framebuffer::RefSlice;
use crate::predict::RefSample;
use crate::prob_tables::BILINEAR;
use crate::subpel::{MAX_BLOCK_DIM, MAX_INTERMEDIATE_HEIGHT, SUBPEL_FILTERS};
use core::arch::x86_64::*;
use core::mem::MaybeUninit;

/// Loads 4 `u8` (zero-extended to `u16`) or 4 `u16` samples into the low 64 bits of an `__m128i`.
///
/// # Safety
/// Caller must confirm `avx2_enabled()` and `p` valid for 4 `S` reads.
#[inline]
#[target_feature(enable = "avx2")]
unsafe fn load4_u16<S: RefSample>(p: *const S) -> __m128i {
    if S::IS_U8 {
        _mm_cvtepu8_epi16(_mm_cvtsi32_si128((p as *const i32).read_unaligned()))
    } else {
        _mm_loadl_epi64(p as *const __m128i)
    }
}

/// Loads 8 `u8` (zero-extended to `u16`) or 8 `u16` samples into an `__m128i`.
///
/// # Safety
/// Caller must confirm `avx2_enabled()` and `p` valid for 8 `S` reads.
#[inline]
#[target_feature(enable = "avx2")]
unsafe fn load8_u16<S: RefSample>(p: *const S) -> __m128i {
    if S::IS_U8 {
        _mm_cvtepu8_epi16(_mm_loadl_epi64(p as *const __m128i))
    } else {
        _mm_loadu_si128(p as *const __m128i)
    }
}

/// Loads 16 `u8` (zero-extended to `u16`) or 16 `u16` samples into an `__m256i`.
///
/// # Safety
/// Caller must confirm `avx2_enabled()` and `p` valid for 16 `S` reads.
#[inline]
#[target_feature(enable = "avx2")]
unsafe fn load16_u16<S: RefSample>(p: *const S) -> __m256i {
    if S::IS_U8 {
        _mm256_cvtepu8_epi16(_mm_loadu_si128(p as *const __m128i))
    } else {
        _mm256_loadu_si256(p as *const __m256i)
    }
}

/// Packs two `i32` subpel filter coefficients (each in `-24..=128`) as repeated `(a, b)` `i16`
/// pairs in a 256-bit register for `_mm256_madd_epi16`.
///
/// # Safety
/// Caller must confirm `avx2_enabled()`.
#[inline]
#[target_feature(enable = "avx2")]
unsafe fn pair_coeff_256(a: i32, b: i32) -> __m256i {
    let lo = a as i16;
    let hi = b as i16;
    _mm256_setr_epi16(
        lo, hi, lo, hi, lo, hi, lo, hi, lo, hi, lo, hi, lo, hi, lo, hi,
    )
}

/// 128-bit counterpart of [`pair_coeff_256`] for `_mm_madd_epi16`.
///
/// # Safety
/// Caller must confirm `avx2_enabled()`.
#[inline]
#[target_feature(enable = "avx2")]
unsafe fn pair_coeff_128(a: i32, b: i32) -> __m128i {
    let lo = a as i16;
    let hi = b as i16;
    _mm_setr_epi16(lo, hi, lo, hi, lo, hi, lo, hi)
}

/// Rounds (`(x + 64) >> 7`) and clips two 8-lane `i32` accumulators (`acc_lo` holding lanes
/// `0..4, 8..12` and `acc_hi` holding lanes `4..8, 12..16`) and packs them into 16 contiguous
/// `u16` lanes in exact order `0..16` (in-lane `VPACKUSDW` needs no cross-lane permute).
///
/// # Safety
/// Caller must confirm `avx2_enabled()`.
#[inline]
#[target_feature(enable = "avx2")]
unsafe fn round_clip_pack_16(
    acc_lo: __m256i,
    acc_hi: __m256i,
    round_add: __m256i,
    zero: __m256i,
    max_clip: __m256i,
) -> __m256i {
    let r_lo = _mm256_srai_epi32(_mm256_add_epi32(acc_lo, round_add), 7);
    let r_hi = _mm256_srai_epi32(_mm256_add_epi32(acc_hi, round_add), 7);
    let c_lo = _mm256_min_epi32(_mm256_max_epi32(r_lo, zero), max_clip);
    let c_hi = _mm256_min_epi32(_mm256_max_epi32(r_hi, zero), max_clip);
    _mm256_packus_epi32(c_lo, c_hi)
}

/// Rounds (`(x + 64) >> 7`), clips, and packs two 4-lane `i32` accumulators into 8 contiguous
/// `u16` lanes.
///
/// # Safety
/// Caller must confirm `avx2_enabled()`.
#[inline]
#[target_feature(enable = "avx2")]
unsafe fn round_clip_pack_8(
    acc_lo: __m128i,
    acc_hi: __m128i,
    round_add: __m128i,
    zero: __m128i,
    max_clip: __m128i,
) -> __m128i {
    let r_lo = _mm_srai_epi32(_mm_add_epi32(acc_lo, round_add), 7);
    let r_hi = _mm_srai_epi32(_mm_add_epi32(acc_hi, round_add), 7);
    let c_lo = _mm_min_epi32(_mm_max_epi32(r_lo, zero), max_clip);
    let c_hi = _mm_min_epi32(_mm_max_epi32(r_hi, zero), max_clip);
    _mm_packus_epi32(c_lo, c_hi)
}

/// Stores 16 `u16` values into `dst`, or averages them with existing `dst` via `VPAVGW`
/// (`(dst + v + 1) >> 1 == round2(dst + v, 1)`) when `compound` is true.
///
/// # Safety
/// Caller must confirm `avx2_enabled()` and `dst` valid for 16 `u16` writes (and initialized
/// reads when `compound`).
#[inline]
#[target_feature(enable = "avx2")]
unsafe fn store_or_avg_16(dst: *mut u16, v: __m256i, compound: bool) {
    let out = if compound {
        _mm256_avg_epu16(_mm256_loadu_si256(dst as *const __m256i), v)
    } else {
        v
    };
    _mm256_storeu_si256(dst as *mut __m256i, out);
}

/// 8-sample counterpart of [`store_or_avg_16`].
///
/// # Safety
/// Caller must confirm `avx2_enabled()` and `dst` valid for 8 `u16` writes (and initialized
/// reads when `compound`).
#[inline]
#[target_feature(enable = "avx2")]
unsafe fn store_or_avg_8(dst: *mut u16, v: __m128i, compound: bool) {
    let out = if compound {
        _mm_avg_epu16(_mm_loadu_si128(dst as *const __m128i), v)
    } else {
        v
    };
    _mm_storeu_si128(dst as *mut __m128i, out);
}

/// 4-sample counterpart of [`store_or_avg_16`].
///
/// # Safety
/// Caller must confirm `avx2_enabled()` and `dst` valid for 4 `u16` writes (and initialized
/// reads when `compound`).
#[inline]
#[target_feature(enable = "avx2")]
unsafe fn store_or_avg_4(dst: *mut u16, v: __m128i, compound: bool) {
    let out = if compound {
        _mm_avg_epu16(_mm_loadl_epi64(dst as *const __m128i), v)
    } else {
        v
    };
    _mm_storel_epi64(dst as *mut __m128i, out);
}

/// Accumulates 4 adjacent tap pairs `(s0,s1)..(s6,s7)` against pre-packed `i16` pair coefficients
/// into low/high `i32` accumulators (`8` pixels, 128-bit SSE/AVX2).
///
/// # Safety
/// Caller must confirm `avx2_enabled()`.
#[inline]
#[target_feature(enable = "avx2")]
unsafe fn madd_8tap_128(s: [__m128i; 8], c: [__m128i; 4]) -> (__m128i, __m128i) {
    let mut lo = _mm_madd_epi16(_mm_unpacklo_epi16(s[0], s[1]), c[0]);
    let mut hi = _mm_madd_epi16(_mm_unpackhi_epi16(s[0], s[1]), c[0]);
    for i in 1..4 {
        lo = _mm_add_epi32(
            lo,
            _mm_madd_epi16(_mm_unpacklo_epi16(s[2 * i], s[2 * i + 1]), c[i]),
        );
        hi = _mm_add_epi32(
            hi,
            _mm_madd_epi16(_mm_unpackhi_epi16(s[2 * i], s[2 * i + 1]), c[i]),
        );
    }
    (lo, hi)
}

/// 256-bit counterpart of [`madd_8tap_128`] (`16` pixels per call).
///
/// # Safety
/// Caller must confirm `avx2_enabled()`.
#[inline]
#[target_feature(enable = "avx2")]
unsafe fn madd_8tap_256(s: [__m256i; 8], c: [__m256i; 4]) -> (__m256i, __m256i) {
    let mut lo = _mm256_madd_epi16(_mm256_unpacklo_epi16(s[0], s[1]), c[0]);
    let mut hi = _mm256_madd_epi16(_mm256_unpackhi_epi16(s[0], s[1]), c[0]);
    for i in 1..4 {
        lo = _mm256_add_epi32(
            lo,
            _mm256_madd_epi16(_mm256_unpacklo_epi16(s[2 * i], s[2 * i + 1]), c[i]),
        );
        hi = _mm256_add_epi32(
            hi,
            _mm256_madd_epi16(_mm256_unpackhi_epi16(s[2 * i], s[2 * i + 1]), c[i]),
        );
    }
    (lo, hi)
}

/// Unified 1D subpel filter (`tap_step == 1` for horizontal, `src_stride` for vertical), writing
/// `rows` rows of `w` (`4` or multiple of `8`) `u16` samples into `dst` (stride `dst_stride`),
/// with optional compound averaging. Keeping `tap_step` as a runtime register halves monomorphized
/// `.text` size without hurting throughput.
///
/// # Safety
/// Caller must confirm `avx2_enabled()`, `w` is 4 or a multiple of 8, `src` valid for all tap
/// reads across `rows` rows at `src_stride`/`tap_step`, and `dst` valid for `rows` rows of `w`
/// `u16` writes at `dst_stride` (initialized when `COMPOUND`).
#[inline(never)]
#[target_feature(enable = "avx2")]
#[allow(clippy::too_many_arguments)]
unsafe fn convolve_1d_u16_avx2<S: RefSample>(
    src: *const S,
    src_stride: usize,
    tap_step: usize,
    dst: *mut u16,
    dst_stride: usize,
    w: usize,
    rows: usize,
    coeffs: &[i32; 8],
    is_bilinear: bool,
    max_val: i32,
    compound: bool,
) {
    if w <= 8 {
        let round_add = _mm_set1_epi32(64);
        let zero = _mm_setzero_si128();
        let max_clip = _mm_set1_epi32(max_val);
        let c = [
            pair_coeff_128(coeffs[0], coeffs[1]),
            pair_coeff_128(coeffs[2], coeffs[3]),
            pair_coeff_128(coeffs[4], coeffs[5]),
            pair_coeff_128(coeffs[6], coeffs[7]),
        ];
        let c34 = pair_coeff_128(coeffs[3], coeffs[4]);
        let load = |p: *const S| if w == 4 { load4_u16(p) } else { load8_u16(p) };

        for r in 0..rows {
            let sp = src.add(r * src_stride);
            let dp = dst.add(r * dst_stride);
            let (acc_lo, acc_hi) = if is_bilinear {
                let s3 = load(sp.add(3 * tap_step));
                let s4 = load(sp.add(4 * tap_step));
                (
                    _mm_madd_epi16(_mm_unpacklo_epi16(s3, s4), c34),
                    if w == 8 {
                        _mm_madd_epi16(_mm_unpackhi_epi16(s3, s4), c34)
                    } else {
                        zero
                    },
                )
            } else {
                let s: [__m128i; 8] = core::array::from_fn(|t| load(sp.add(t * tap_step)));
                madd_8tap_128(s, c)
            };
            let packed = round_clip_pack_8(acc_lo, acc_hi, round_add, zero, max_clip);
            if w == 4 {
                store_or_avg_4(dp, packed, compound);
            } else {
                store_or_avg_8(dp, packed, compound);
            }
        }
        return;
    }

    // w is 16, 32, or 64 (multiple of 16): process 16 pixels per iteration with 256-bit AVX2.
    debug_assert_eq!(w % 16, 0);
    let round_add = _mm256_set1_epi32(64);
    let zero = _mm256_setzero_si256();
    let max_clip = _mm256_set1_epi32(max_val);
    if is_bilinear {
        let c34 = pair_coeff_256(coeffs[3], coeffs[4]);
        for r in 0..rows {
            let p3 = src.add(r * src_stride + 3 * tap_step);
            let p4 = src.add(r * src_stride + 4 * tap_step);
            let row_dst = dst.add(r * dst_stride);
            for col in (0..w).step_by(16) {
                let s3 = load16_u16(p3.add(col));
                let s4 = load16_u16(p4.add(col));
                let acc_lo = _mm256_madd_epi16(_mm256_unpacklo_epi16(s3, s4), c34);
                let acc_hi = _mm256_madd_epi16(_mm256_unpackhi_epi16(s3, s4), c34);
                let packed = round_clip_pack_16(acc_lo, acc_hi, round_add, zero, max_clip);
                store_or_avg_16(row_dst.add(col), packed, compound);
            }
        }
    } else {
        let c = [
            pair_coeff_256(coeffs[0], coeffs[1]),
            pair_coeff_256(coeffs[2], coeffs[3]),
            pair_coeff_256(coeffs[4], coeffs[5]),
            pair_coeff_256(coeffs[6], coeffs[7]),
        ];
        for r in 0..rows {
            let row_src = src.add(r * src_stride);
            let pt: [*const S; 8] = core::array::from_fn(|t| row_src.add(t * tap_step));
            let row_dst = dst.add(r * dst_stride);
            for col in (0..w).step_by(16) {
                let s: [__m256i; 8] = core::array::from_fn(|t| load16_u16(pt[t].add(col)));
                let (acc_lo, acc_hi) = madd_8tap_256(s, c);
                let packed = round_clip_pack_16(acc_lo, acc_hi, round_add, zero, max_clip);
                store_or_avg_16(row_dst.add(col), packed, compound);
            }
        }
    }
}

/// Full-pel copy (or compound average via `VPAVGW`/`PAVGW`) for `fx == 0 && fy == 0`.
///
/// # Safety
/// Caller must confirm `avx2_enabled()`, `w` is 4 or a multiple of 8, `src` valid for `h` rows
/// of `w` `S` reads at `src_stride`, and `dst` valid for `h` rows of `w` `u16` writes at
/// `dst_stride` (initialized when `compound`).
#[inline(never)]
#[target_feature(enable = "avx2")]
#[allow(clippy::too_many_arguments)]
unsafe fn copy_u16_avx2<S: RefSample>(
    src: *const S,
    src_stride: usize,
    dst: *mut u16,
    dst_stride: usize,
    w: usize,
    h: usize,
    compound: bool,
) {
    if w <= 8 {
        for r in 0..h {
            let sp = src.add(r * src_stride);
            let dp = dst.add(r * dst_stride);
            if w == 4 {
                store_or_avg_4(dp, load4_u16(sp), compound);
            } else {
                store_or_avg_8(dp, load8_u16(sp), compound);
            }
        }
    } else {
        debug_assert_eq!(w % 16, 0);
        for r in 0..h {
            let sp = src.add(r * src_stride);
            let dp = dst.add(r * dst_stride);
            for c in (0..w).step_by(16) {
                store_or_avg_16(dp.add(c), load16_u16(sp.add(c)), compound);
            }
        }
    }
}

/// Direct-to-plane unscaled AVX2 inter prediction (`x_step == y_step == 16`, `w` is 4 or a
/// multiple of 8). Writes `u16` output straight into `dst_base` at `(dst_x, dst_y)` (or averages
/// with existing `dst_base` via `VPAVGW`/`PAVGW` when `compound == true`).
///
/// # Safety
/// Caller must confirm `avx2_enabled()`, that `dst_base` holds rows `dst_y..dst_y + h` x cols
/// `dst_x..dst_x + w` (stride `dst_width`), and that `ref_data` (stride `ref_width`) covers every
/// source pixel read by the active `(fx, fy)` branch.
#[target_feature(enable = "avx2")]
#[allow(clippy::too_many_arguments)]
pub(crate) unsafe fn block_inter_predict_u16_avx2<S: RefSample>(
    ref_data: &[S],
    ref_width: usize,
    dst_base: *mut u16,
    dst_width: usize,
    dst_x: usize,
    dst_y: usize,
    x0: i64,
    y0: i64,
    fx: usize,
    fy: usize,
    w: usize,
    h: usize,
    interp_filter: u8,
    max_val: i32,
    compound: bool,
) {
    debug_assert!(dst_x + w <= dst_width);
    debug_assert!(w == 4 || w.is_multiple_of(8));
    debug_assert!(w <= MAX_BLOCK_DIM && h <= MAX_BLOCK_DIM);
    let dst = dst_base.add(dst_y * dst_width + dst_x);
    let is_bilinear = interp_filter == BILINEAR;
    let filters = &SUBPEL_FILTERS[interp_filter as usize];

    match (fx == 0, fy == 0) {
        (true, true) => {
            let src = ref_data
                .as_ptr()
                .add((y0 as usize) * ref_width + (x0 as usize));
            copy_u16_avx2::<S>(src, ref_width, dst, dst_width, w, h, compound);
        }
        (false, true) => {
            let src = ref_data
                .as_ptr()
                .add((y0 as usize) * ref_width + ((x0 - 3) as usize));
            convolve_1d_u16_avx2::<S>(
                src,
                ref_width,
                1,
                dst,
                dst_width,
                w,
                h,
                &filters[fx],
                is_bilinear,
                max_val,
                compound,
            );
        }
        (true, false) => {
            let src = ref_data
                .as_ptr()
                .add(((y0 - 3) as usize) * ref_width + (x0 as usize));
            convolve_1d_u16_avx2::<S>(
                src,
                ref_width,
                ref_width,
                dst,
                dst_width,
                w,
                h,
                &filters[fy],
                is_bilinear,
                max_val,
                compound,
            );
        }
        (false, false) => {
            let mut intermediate =
                [MaybeUninit::<u16>::uninit(); (MAX_BLOCK_DIM + 7) * MAX_BLOCK_DIM];
            let tmp_ptr = intermediate.as_mut_ptr() as *mut u16;
            let src = ref_data
                .as_ptr()
                .add(((y0 - 3) as usize) * ref_width + ((x0 - 3) as usize));
            convolve_1d_u16_avx2::<S>(
                src,
                ref_width,
                1,
                tmp_ptr,
                w,
                w,
                h + 7,
                &filters[fx],
                is_bilinear,
                max_val,
                false,
            );
            convolve_1d_u16_avx2::<u16>(
                tmp_ptr,
                w,
                w,
                dst,
                dst_width,
                w,
                h,
                &filters[fy],
                is_bilinear,
                max_val,
                compound,
            );
        }
    }
}

/// Direct-to-plane edge-clamping AVX2 inter prediction for unscaled border blocks
/// (`x_step == y_step == 16`) AND scaled references (`x_step`/`y_step` != 16, SVC / resize).
///
/// # Safety
/// The caller must have confirmed `avx2_enabled()`, and must guarantee:
/// `ref_stride >= ref_width`; `ref_data.len() >= ref_stride * ref_height`;
/// `intermediate_height <= MAX_INTERMEDIATE_HEIGHT`; horizontal source span
/// `<= MAX_INTERMEDIATE_HEIGHT`; and `dst_base` holds rows `dst_y..dst_y + h` x cols
/// `dst_x..dst_x + w` (stride `dst_width`).
#[inline(never)]
#[target_feature(enable = "avx2")]
#[allow(clippy::too_many_arguments)]
pub(crate) unsafe fn block_inter_predict_scaled_u16_avx2(
    ref_data: RefSlice<'_>,
    ref_width: usize,
    ref_stride: usize,
    ref_height: usize,
    dst_base: *mut u16,
    dst_width: usize,
    dst_x: usize,
    dst_y: usize,
    x: i64,
    y: i64,
    x_step: i64,
    y_step: i64,
    w: usize,
    h: usize,
    intermediate_height: usize,
    interp_filter: u8,
    max_val: i32,
    compound: bool,
) {
    debug_assert!(w == 4 || w.is_multiple_of(8));
    debug_assert!(w <= MAX_BLOCK_DIM && h <= MAX_BLOCK_DIM);
    debug_assert!(intermediate_height <= MAX_INTERMEDIATE_HEIGHT);
    debug_assert!(ref_stride >= ref_width);
    debug_assert!(ref_data.len() >= ref_stride * ref_height);
    debug_assert!(x_step >= 0 && y_step >= 0);

    let dst = dst_base.add(dst_y * dst_width + dst_x);
    let last_y = ref_height as i64 - 1;
    let filters = &SUBPEL_FILTERS[interp_filter as usize];

    if x_step == 16 && y_step == 16 {
        let (x0, y0) = (x >> 4, y >> 4);
        let (fx, fy) = ((x & 15) as usize, (y & 15) as usize);
        let is_bilinear = interp_filter == BILINEAR;

        let fill_clamped_row = |ref_y: usize, col_start: i64, len: usize, out: *mut u16| {
            let row_off = ref_y * ref_stride;
            let first = col_start.clamp(0, ref_width as i64) as usize;
            let left = (-col_start).clamp(0, len as i64) as usize;
            let middle = (len - left).min(ref_width - first);
            if left != 0 {
                let sample = ref_data.get_i32(row_off) as u16;
                for i in 0..left {
                    out.add(i).write(sample);
                }
            }
            match ref_data {
                RefSlice::U16(s) => {
                    core::ptr::copy_nonoverlapping(
                        s.as_ptr().add(row_off + first),
                        out.add(left),
                        middle,
                    );
                }
                RefSlice::U8(s) => {
                    let input = &s[row_off + first..][..middle];
                    for (i, &sample) in input.iter().enumerate() {
                        out.add(left + i).write(u16::from(sample));
                    }
                }
            }
            if left + middle < len {
                let sample = ref_data.get_i32(row_off + ref_width - 1) as u16;
                for i in left + middle..len {
                    out.add(i).write(sample);
                }
            }
        };

        let mut line = [MaybeUninit::<u16>::uninit(); MAX_BLOCK_DIM + 8];
        let line_ptr = line.as_mut_ptr() as *mut u16;

        if fy == 0 {
            for r in 0..h {
                let ref_y = (y0 + r as i64).clamp(0, last_y) as usize;
                let row_dst = dst.add(r * dst_width);
                if fx == 0 {
                    fill_clamped_row(ref_y, x0, w, line_ptr);
                    copy_u16_avx2::<u16>(line_ptr, w, row_dst, dst_width, w, 1, compound);
                } else {
                    fill_clamped_row(ref_y, x0 - 3, w + 7, line_ptr);
                    convolve_1d_u16_avx2::<u16>(
                        line_ptr,
                        w + 7,
                        1,
                        row_dst,
                        dst_width,
                        w,
                        1,
                        &filters[fx],
                        is_bilinear,
                        max_val,
                        compound,
                    );
                }
            }
        } else {
            let mut tmp = [MaybeUninit::<u16>::uninit(); (MAX_BLOCK_DIM + 7) * MAX_BLOCK_DIM];
            let tmp_ptr = tmp.as_mut_ptr() as *mut u16;
            for r in 0..(h + 7) {
                let ref_y = (y0 - 3 + r as i64).clamp(0, last_y) as usize;
                if fx == 0 {
                    fill_clamped_row(ref_y, x0, w, tmp_ptr.add(r * w));
                } else {
                    fill_clamped_row(ref_y, x0 - 3, w + 7, line_ptr);
                    convolve_1d_u16_avx2::<u16>(
                        line_ptr,
                        w + 7,
                        1,
                        tmp_ptr.add(r * w),
                        w,
                        w,
                        1,
                        &filters[fx],
                        is_bilinear,
                        max_val,
                        false,
                    );
                }
            }
            convolve_1d_u16_avx2::<u16>(
                tmp_ptr,
                w,
                w,
                dst,
                dst_width,
                w,
                h,
                &filters[fy],
                is_bilinear,
                max_val,
                compound,
            );
        }
        return;
    }

    let mut pred = [MaybeUninit::<i32>::uninit(); MAX_BLOCK_DIM * MAX_BLOCK_DIM];
    let pred_ptr = pred.as_mut_ptr().cast::<i32>();
    block_inter_predict_scaled_strided_avx2(
        ref_data,
        ref_width,
        ref_stride,
        ref_height,
        x,
        y,
        x_step,
        y_step,
        w,
        h,
        intermediate_height,
        interp_filter,
        max_val,
        pred_ptr,
    );
    for r in 0..h {
        let sp = pred_ptr.add(r * w);
        let dp = dst.add(r * dst_width);
        if w == 4 {
            let v = _mm_packus_epi32(_mm_loadu_si128(sp as *const __m128i), _mm_setzero_si128());
            store_or_avg_4(dp, v, compound);
        } else {
            for c in (0..w).step_by(8) {
                let lo = _mm_loadu_si128(sp.add(c) as *const __m128i);
                let hi = _mm_loadu_si128(sp.add(c + 4) as *const __m128i);
                store_or_avg_8(dp.add(c), _mm_packus_epi32(lo, hi), compound);
            }
        }
    }
}

/// Stride-aware general edge-clamping AVX2 inter-prediction core (`ref_stride >= ref_width`),
/// writing all `w * h` output samples to `pred_ptr` (allowing uninitialized stack scratch).
///
/// # Safety
/// Caller must confirm `avx2_enabled()`, `ref_stride >= ref_width`,
/// `ref_data.len() >= ref_stride * ref_height`, `intermediate_height <= MAX_INTERMEDIATE_HEIGHT`,
/// horizontal source span `<= MAX_INTERMEDIATE_HEIGHT`, and `pred_ptr` valid for writes of
/// `w * h` contiguous `i32` elements.
#[inline(never)]
#[target_feature(enable = "avx2")]
#[allow(clippy::too_many_arguments)]
unsafe fn block_inter_predict_scaled_strided_avx2(
    ref_data: RefSlice<'_>,
    ref_width: usize,
    ref_stride: usize,
    ref_height: usize,
    x: i64,
    y: i64,
    x_step: i64,
    y_step: i64,
    w: usize,
    h: usize,
    intermediate_height: usize,
    interp_filter: u8,
    max_val: i32,
    pred_ptr: *mut i32,
) {
    debug_assert!(w == 4 || w.is_multiple_of(8));
    debug_assert!(w <= MAX_BLOCK_DIM && h <= MAX_BLOCK_DIM);
    debug_assert!(intermediate_height <= MAX_INTERMEDIATE_HEIGHT);
    debug_assert!(ref_stride >= ref_width);
    debug_assert!(ref_data.len() >= ref_stride * ref_height);
    debug_assert!(x_step >= 0 && y_step >= 0);

    let filters = &SUBPEL_FILTERS[interp_filter as usize];
    let last_x = ref_width as i64 - 1;
    let last_y = ref_height as i64 - 1;

    let wp = if w == 4 { 8 } else { w };
    let groups = wp / 8;

    let col_min = (x >> 4) - 3;
    let span = ((((x & 15) + x_step * (w as i64 - 1)) >> 4) + 8) as usize;
    debug_assert!(span <= MAX_INTERMEDIATE_HEIGHT);
    let mut gidx = [[0i32; 8]; MAX_BLOCK_DIM / 8];
    let mut hcoef = [[[0i32; 8]; 8]; MAX_BLOCK_DIM / 8];
    for c in 0..wp {
        let p = x + x_step * (c.min(w - 1) as i64);
        gidx[c / 8][c % 8] = ((p >> 4) - 3 - col_min) as i32;
        for (t, &coeff) in filters[(p & 15) as usize].iter().enumerate() {
            hcoef[c / 8][t][c % 8] = coeff;
        }
    }

    let mut src_col = [0usize; MAX_INTERMEDIATE_HEIGHT];
    for (i, sc) in src_col[..span].iter_mut().enumerate() {
        *sc = (col_min + i as i64).clamp(0, last_x) as usize;
    }

    let round_add = _mm256_set1_epi32(64);
    let zero = _mm256_setzero_si256();
    let max_clip = _mm256_set1_epi32(max_val);

    let mut intermediate = [MaybeUninit::<i32>::uninit(); MAX_INTERMEDIATE_HEIGHT * MAX_BLOCK_DIM];
    let inter_ptr = intermediate.as_mut_ptr() as *mut i32;
    let mut scratch = [MaybeUninit::<i32>::uninit(); MAX_INTERMEDIATE_HEIGHT];
    let scratch_ptr = scratch.as_mut_ptr() as *mut i32;

    for r in 0..intermediate_height {
        let ref_y = ((y >> 4) + r as i64 - 3).clamp(0, last_y) as usize;
        let row_off = ref_y * ref_stride;
        for (i, &sc) in src_col[..span].iter().enumerate() {
            *scratch_ptr.add(i) = ref_data.get_i32(row_off + sc);
        }
        for (g, (idx_lanes, taps)) in gidx[..groups]
            .iter()
            .zip(hcoef[..groups].iter())
            .enumerate()
        {
            let idx = _mm256_loadu_si256(idx_lanes.as_ptr() as *const __m256i);
            let mut acc = zero;
            for (t, tap_lanes) in taps.iter().enumerate() {
                let vals = _mm256_i32gather_epi32::<4>(scratch_ptr.add(t), idx);
                let coeff = _mm256_loadu_si256(tap_lanes.as_ptr() as *const __m256i);
                acc = _mm256_add_epi32(acc, _mm256_mullo_epi32(vals, coeff));
            }
            acc = _mm256_srai_epi32(_mm256_add_epi32(acc, round_add), 7);
            acc = _mm256_min_epi32(_mm256_max_epi32(acc, zero), max_clip);
            _mm256_storeu_si256(inter_ptr.add(r * wp + g * 8) as *mut __m256i, acc);
        }
    }

    for r in 0..h {
        let p = (y & 15) + y_step * (r as i64);
        let vcoeffs = &filters[(p & 15) as usize];
        let base_row = (p >> 4) as usize;
        for g in 0..groups {
            let mut acc = zero;
            for (t, &tap) in vcoeffs.iter().enumerate() {
                let ptr = inter_ptr.add((base_row + t) * wp + g * 8);
                let vals = _mm256_loadu_si256(ptr as *const __m256i);
                acc = _mm256_add_epi32(acc, _mm256_mullo_epi32(vals, _mm256_set1_epi32(tap)));
            }
            acc = _mm256_srai_epi32(_mm256_add_epi32(acc, round_add), 7);
            acc = _mm256_min_epi32(_mm256_max_epi32(acc, zero), max_clip);
            if w == 4 {
                _mm_storeu_si128(
                    pred_ptr.add(r * 4) as *mut __m128i,
                    _mm256_castsi256_si128(acc),
                );
            } else {
                _mm256_storeu_si256(pred_ptr.add(r * w + g * 8) as *mut __m256i, acc);
            }
        }
    }
}
