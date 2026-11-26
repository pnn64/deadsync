//! x86-64 SIMD motion-compensation kernels.
//!
//! Adapted from PR #1 by codepaws. All kernels are bit-exact with the scalar
//! reference in `scalar.rs`:
//! - the 6-tap half-pel FIR accumulates in i16 (max |acc| is 42·255 =
//!   10,710; +16 rounding still fits),
//! - the diagonal filter stages its horizontal pass in i16 and finishes the
//!   vertical pass in i32 lanes (±449,820),
//! - quarter-pel averaging saturates each half-pel result to u8 *before*
//!   `pavgb` ((a+b+1)>>1), matching the scalar clip-then-average order,
//! - chroma bilinear weights sum to 64, so `(sum + 32) >> 6` is always in
//!   0..=255 and needs no saturation clamp.
//!
//! Loads are sized so SIMD chunks never read past the slice lengths callers
//! guarantee (`w + 5` bytes for horizontal-filter sources, `w` for vertical
//! row sets, `w + 1` per chroma row); tails below the chunk width run the
//! scalar reference. Luma blocks are at most 16 wide, so kernels process
//! 16-wide chunks, then one 8-wide chunk, then the scalar tail.
//!
//! SSE2 is part of the x86-64 baseline and needs no runtime check. Kernels
//! using newer instructions (`pmaddubsw` in chroma, 256/512-bit `pavgb` in
//! bi-pred) gate on a level detected once at runtime, which requires the
//! `std` feature; without it those kernels stay on the SSE2/scalar path.

// Whether an SSE2 intrinsic call needs an `unsafe` block depends on the
// toolchain's target_feature rules; keep the blocks and silence the lint
// where they turn out to be redundant.
#![allow(unused_unsafe)]

use core::arch::x86_64::*;

use crate::inter_pred::{avg, clip_u8, fir6};
use crate::once::OnceLock;

use super::scalar::{self, hv_sample, v_sample};

#[path = "x86_deblock.rs"]
mod deblock;
#[path = "x86_weight.rs"]
mod weight;
pub use super::scalar::weighted_bi;
pub use deblock::{
    deblock_chroma_lines, deblock_luma_lines, load_edge_lines_v, store_edge_lines_v,
};
pub(crate) use weight::block_bi_weighted_16;
pub use weight::{weighted_bi_implicit, weighted_uni};

/// Detected CPU capability level, in ascending order. `Scalar` is only
/// produced by the test-only force-scalar switch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) enum SimdLevel {
    Scalar,
    Sse2,
    Ssse3,
    #[cfg(feature = "std")]
    Sse41,
    Avx2,
    Avx512,
}

static LEVEL: OnceLock<SimdLevel> = OnceLock::new();

#[cfg(test)]
static FORCE_SCALAR: core::sync::atomic::AtomicBool = core::sync::atomic::AtomicBool::new(false);

/// Force (or re-enable) scalar code paths process-wide. Test-only: the
/// differential tests run every kernel both ways and compare.
#[cfg(test)]
pub(crate) fn set_force_scalar(force: bool) {
    FORCE_SCALAR.store(force, core::sync::atomic::Ordering::Release);
}

#[cfg(feature = "std")]
fn detect_level() -> SimdLevel {
    // AVX-512 needs BW (byte/word ops) + VL (128/256-bit forms) + F to be
    // useful for integer pixel work.
    if std::arch::is_x86_feature_detected!("avx512bw")
        && std::arch::is_x86_feature_detected!("avx512vl")
        && std::arch::is_x86_feature_detected!("avx512f")
    {
        SimdLevel::Avx512
    } else if std::arch::is_x86_feature_detected!("avx2") {
        SimdLevel::Avx2
    } else if std::arch::is_x86_feature_detected!("sse4.1") {
        SimdLevel::Sse41
    } else if std::arch::is_x86_feature_detected!("ssse3") {
        SimdLevel::Ssse3
    } else {
        SimdLevel::Sse2
    }
}

#[cfg(not(feature = "std"))]
fn detect_level() -> SimdLevel {
    // No runtime feature detection without std; SSE2 is guaranteed on x86-64.
    SimdLevel::Sse2
}

/// Effective SIMD level: detected level, or Scalar when forced (tests).
#[inline]
pub(crate) fn level() -> SimdLevel {
    #[cfg(test)]
    if FORCE_SCALAR.load(core::sync::atomic::Ordering::Acquire) {
        return SimdLevel::Scalar;
    }
    *LEVEL.get_or_init(detect_level)
}

// ---------------------------------------------------------------------------
// Shared building blocks
// ---------------------------------------------------------------------------

/// Widen the low 8 bytes of `x` to i16 lanes.
#[inline(always)]
fn widen8(x: __m128i) -> __m128i {
    unsafe { _mm_unpacklo_epi8(x, _mm_setzero_si128()) }
}

/// Widen the high 8 bytes of `x` to i16 lanes.
#[inline(always)]
fn widen8_hi(x: __m128i) -> __m128i {
    unsafe { _mm_unpackhi_epi8(x, _mm_setzero_si128()) }
}

/// 6-tap FIR accumulate over six already-widened i16x8 vectors:
/// `s0 - 5·s1 + 20·s2 + 20·s3 - 5·s4 + s5`, in i16 without rounding.
#[inline(always)]
unsafe fn fir6_acc_w(w: [__m128i; 6]) -> __m128i {
    let t05 = _mm_add_epi16(w[0], w[5]);
    let t14 = _mm_add_epi16(w[1], w[4]);
    let t23 = _mm_add_epi16(w[2], w[3]);
    let c20 = _mm_set1_epi16(20);
    let c5 = _mm_set1_epi16(5);
    _mm_add_epi16(
        t05,
        _mm_sub_epi16(_mm_mullo_epi16(t23, c20), _mm_mullo_epi16(t14, c5)),
    )
}

/// Round `(acc + 16) >> 5` and saturate an i16x8 accumulator to u8x8
/// (result in the low 8 bytes).
#[inline(always)]
unsafe fn round_pack5(acc: __m128i) -> __m128i {
    let r = _mm_srai_epi16(_mm_add_epi16(acc, _mm_set1_epi16(16)), 5);
    _mm_packus_epi16(r, r)
}

/// Round and saturate two i16x8 accumulators (low and high halves of a
/// 16-wide chunk) into one u8x16 vector.
#[inline(always)]
unsafe fn round_pack5_x2(lo: __m128i, hi: __m128i) -> __m128i {
    _mm_packus_epi16(
        _mm_srai_epi16(_mm_add_epi16(lo, _mm_set1_epi16(16)), 5),
        _mm_srai_epi16(_mm_add_epi16(hi, _mm_set1_epi16(16)), 5),
    )
}

/// Six overlapping 16-byte loads from `src[i..]` for the horizontal FIR.
/// Caller guarantees `i + 16 + 5 <= src.len()`.
#[inline(always)]
unsafe fn load_h16(src: &[u8], i: usize) -> [__m128i; 6] {
    let p = src.as_ptr().add(i);
    [
        _mm_loadu_si128(p as *const __m128i),
        _mm_loadu_si128(p.add(1) as *const __m128i),
        _mm_loadu_si128(p.add(2) as *const __m128i),
        _mm_loadu_si128(p.add(3) as *const __m128i),
        _mm_loadu_si128(p.add(4) as *const __m128i),
        _mm_loadu_si128(p.add(5) as *const __m128i),
    ]
}

/// Six 16-byte loads at column `i` of six rows for the vertical FIR.
/// Caller guarantees `i + 16 <= rows[k].len()`.
#[inline(always)]
unsafe fn load_v16(rows: [&[u8]; 6], i: usize) -> [__m128i; 6] {
    rows.map(|r| _mm_loadu_si128(r.as_ptr().add(i) as *const __m128i))
}

/// 6-tap horizontal FIR of one 8-wide chunk from `src[i..]`.
/// Caller guarantees `i + 8 + 5 <= src.len()`.
#[inline(always)]
unsafe fn fir6_h8(src: &[u8], i: usize) -> __m128i {
    let p = src.as_ptr().add(i);
    fir6_acc_w([
        widen8(_mm_loadl_epi64(p as *const __m128i)),
        widen8(_mm_loadl_epi64(p.add(1) as *const __m128i)),
        widen8(_mm_loadl_epi64(p.add(2) as *const __m128i)),
        widen8(_mm_loadl_epi64(p.add(3) as *const __m128i)),
        widen8(_mm_loadl_epi64(p.add(4) as *const __m128i)),
        widen8(_mm_loadl_epi64(p.add(5) as *const __m128i)),
    ])
}

/// 6-tap vertical FIR of one 8-wide chunk from six row slices at offset `i`.
/// Caller guarantees `i + 8 <= rows[k].len()`.
#[inline(always)]
unsafe fn fir6_v8(rows: [&[u8]; 6], i: usize) -> __m128i {
    fir6_acc_w(rows.map(|r| widen8(_mm_loadl_epi64(r.as_ptr().add(i) as *const __m128i))))
}

/// Load 8 bytes from `src[i..]` into the low half of a vector.
#[inline(always)]
unsafe fn load8(src: &[u8], i: usize) -> __m128i {
    _mm_loadl_epi64(src.as_ptr().add(i) as *const __m128i)
}

/// Store the low 8 bytes of `x` to `out[i..i+8]`.
#[inline(always)]
unsafe fn store8(out: &mut [u8], i: usize, x: __m128i) {
    _mm_storel_epi64(out.as_mut_ptr().add(i) as *mut __m128i, x);
}

/// Store 16 bytes of `x` to `out[i..i+16]`.
#[inline(always)]
unsafe fn store16(out: &mut [u8], i: usize, x: __m128i) {
    _mm_storeu_si128(out.as_mut_ptr().add(i) as *mut __m128i, x);
}

/// Stage-1 horizontal FIR (unrounded i16) of six rows at chunk offset `i`.
/// Each row must have `i + 8 + 5` accessible bytes.
#[inline(always)]
unsafe fn hv_stage1(rows_hv: &[&[u8]; 6], i: usize) -> [__m128i; 6] {
    [
        fir6_h8(rows_hv[0], i),
        fir6_h8(rows_hv[1], i),
        fir6_h8(rows_hv[2], i),
        fir6_h8(rows_hv[3], i),
        fir6_h8(rows_hv[4], i),
        fir6_h8(rows_hv[5], i),
    ]
}

/// Vertical FIR over six staged i16x8 rows, rounds `(v + 512) >> 10` and
/// clips to u8. Result in the low 8 bytes.
///
/// `madd` turns 8 i16 lanes into 4 i32 lanes, so the low and high halves of
/// each row pair are accumulated separately (outputs 0..3 and 4..7) and
/// concatenated by the final `packs_epi32`.
#[inline(always)]
unsafe fn hv_vertical(h: &[__m128i; 6]) -> __m128i {
    // madd weights as i16 pairs packed into i32 lanes: [1, -5], [20, 20], [-5, 1].
    let w01 = _mm_set1_epi32(1i32 | ((-5i32) << 16));
    let w23 = _mm_set1_epi32(20 | (20i32 << 16));
    let w45 = _mm_set1_epi32((-5i32 & 0xFFFF) | (1i32 << 16));
    let mut sum_lo = _mm_madd_epi16(_mm_unpacklo_epi16(h[0], h[1]), w01);
    let mut sum_hi = _mm_madd_epi16(_mm_unpackhi_epi16(h[0], h[1]), w01);
    sum_lo = _mm_add_epi32(sum_lo, _mm_madd_epi16(_mm_unpacklo_epi16(h[2], h[3]), w23));
    sum_hi = _mm_add_epi32(sum_hi, _mm_madd_epi16(_mm_unpackhi_epi16(h[2], h[3]), w23));
    sum_lo = _mm_add_epi32(sum_lo, _mm_madd_epi16(_mm_unpacklo_epi16(h[4], h[5]), w45));
    sum_hi = _mm_add_epi32(sum_hi, _mm_madd_epi16(_mm_unpackhi_epi16(h[4], h[5]), w45));
    let round = |s: __m128i| _mm_srai_epi32(_mm_add_epi32(s, _mm_set1_epi32(512)), 10);
    _mm_packus_epi16(
        _mm_packs_epi32(round(sum_lo), round(sum_hi)),
        _mm_setzero_si128(),
    )
}

/// Diagonal half-pel and its quarter-pel neighbors. `src` starts two rows
/// above and two columns to the left of the first interpolated sample.
#[inline(never)]
pub(crate) fn block_qpel_diagonal(
    src: &[u8],
    stride: usize,
    w: usize,
    h: usize,
    frac_x: i32,
    frac_y: i32,
    out: &mut [u8],
) -> bool {
    if !matches!(w, 4 | 8 | 16) || h > 16 || level() == SimdLevel::Scalar {
        return false;
    }
    assert!(src.len() >= (h + 4) * stride + w + 5 && out.len() >= w * h);

    // SAFETY: each load is within the (w + 5) by (h + 5) sample window
    // checked above. Four-wide partitions use exact four-byte accesses.
    unsafe {
        let load = |row: &[u8], col: usize| {
            if w == 4 {
                let word = row.as_ptr().add(col).cast::<i32>().read_unaligned();
                _mm_cvtsi32_si128(word)
            } else {
                load8(row, col)
            }
        };
        for col in (0..w).step_by(8) {
            let mut horizontal = [_mm_setzero_si128(); 21];
            for (r, acc) in horizontal[..h + 5].iter_mut().enumerate() {
                let row = &src[r * stride..];
                *acc = fir6_acc_w(core::array::from_fn(|tap| widen8(load(row, col + tap))));
            }
            for (r, rows) in horizontal[..h + 5].windows(6).enumerate() {
                let mut result = hv_vertical(rows.try_into().unwrap());
                if frac_x != 2 {
                    let x = col + if frac_x == 3 { 3 } else { 2 };
                    let samples =
                        core::array::from_fn(|i| widen8(load(&src[(r + i) * stride..], x)));
                    result = _mm_avg_epu8(result, round_pack5(fir6_acc_w(samples)));
                } else if frac_y != 2 {
                    let y = if frac_y == 3 { 3 } else { 2 };
                    result = _mm_avg_epu8(result, round_pack5(rows[y]));
                }
                if w == 4 {
                    out.as_mut_ptr()
                        .add(r * w)
                        .cast::<i32>()
                        .write_unaligned(_mm_cvtsi128_si32(result));
                } else {
                    store8(out, r * w + col, result);
                }
            }
        }
    }
    true
}

// ---------------------------------------------------------------------------
// Kernels. Each dispatches on `level()`; SSE2 paths need no feature check.
// ---------------------------------------------------------------------------

/// Horizontal half-pel FIR for a row of `w` pixels; `src` has `w + 5` bytes.
#[inline(never)]
pub fn row_half_pel_h(src: &[u8], out: &mut [u8], w: usize) {
    if level() == SimdLevel::Scalar {
        return scalar::row_half_pel_h(src, out, w);
    }
    debug_assert!(src.len() >= w + 5 && out.len() >= w);
    let mut i = 0;
    // SAFETY: src has w + 5 bytes and out has w bytes; every chunk below
    // stays within i + chunk + 5 <= w + 5.
    unsafe {
        while i + 16 <= w {
            let l = load_h16(src, i);
            store16(
                out,
                i,
                round_pack5_x2(fir6_acc_w(l.map(widen8)), fir6_acc_w(l.map(widen8_hi))),
            );
            i += 16;
        }
        if i + 8 <= w {
            store8(out, i, round_pack5(fir6_h8(src, i)));
            i += 8;
        }
    }
    while i < w {
        out[i] = clip_u8((fir6(src, i) + 16) >> 5);
        i += 1;
    }
}

/// Vertical half-pel FIR for a row of `w` pixels; each of `rows` has `w` bytes.
#[inline(never)]
pub fn row_half_pel_v(rows: [&[u8]; 6], out: &mut [u8], w: usize) {
    if level() == SimdLevel::Scalar {
        return scalar::row_half_pel_v(rows, out, w);
    }
    debug_assert!(rows.iter().all(|r| r.len() >= w) && out.len() >= w);
    let mut i = 0;
    // SAFETY: every row has w bytes and out has w bytes.
    unsafe {
        while i + 16 <= w {
            let l = load_v16(rows, i);
            store16(
                out,
                i,
                round_pack5_x2(fir6_acc_w(l.map(widen8)), fir6_acc_w(l.map(widen8_hi))),
            );
            i += 16;
        }
        if i + 8 <= w {
            store8(out, i, round_pack5(fir6_v8(rows, i)));
            i += 8;
        }
    }
    while i < w {
        out[i] = v_sample(&rows, i);
        i += 1;
    }
}

/// Diagonal half-pel FIR for a row of `w` pixels; each of `rows` has
/// `w + 5` bytes. Two-stage: horizontal into i16, vertical in i32.
#[inline(never)]
pub fn row_half_pel_hv(rows: [&[u8]; 6], out: &mut [u8], w: usize) {
    if level() == SimdLevel::Scalar {
        return scalar::row_half_pel_hv(rows, out, w);
    }
    debug_assert!(rows.iter().all(|r| r.len() >= w + 5) && out.len() >= w);
    let mut i = 0;
    // SAFETY: every row has w + 5 bytes and out has w bytes.
    unsafe {
        while i + 8 <= w {
            store8(out, i, hv_vertical(&hv_stage1(&rows, i)));
            i += 8;
        }
    }
    while i < w {
        out[i] = hv_sample(&rows, i);
        i += 1;
    }
}

/// Quarter-pel `avg(integer_row, h_half_pel)` (arm (1,0)).
#[inline(never)]
pub fn row_avg_int_h(int_row: &[u8], src_h: &[u8], out: &mut [u8], w: usize) {
    if level() == SimdLevel::Scalar {
        return scalar::row_avg_int_h(int_row, src_h, out, w);
    }
    debug_assert!(int_row.len() >= w && src_h.len() >= w + 5 && out.len() >= w);
    let mut i = 0;
    // SAFETY: int_row has w bytes, src_h has w + 5 bytes, out has w bytes.
    unsafe {
        while i + 8 <= w {
            let hp = round_pack5(fir6_h8(src_h, i));
            store8(out, i, _mm_avg_epu8(load8(int_row, i), hp));
            i += 8;
        }
    }
    while i < w {
        let hp = clip_u8((fir6(src_h, i) + 16) >> 5);
        out[i] = avg(int_row[i], hp);
        i += 1;
    }
}

/// Quarter-pel `avg(h_half_pel, integer_row)` (arm (3,0), integer row at +1).
#[inline(never)]
pub fn row_avg_h_int(src_h: &[u8], int_row: &[u8], out: &mut [u8], w: usize) {
    // avg is symmetric, so this is the same computation with operands swapped.
    row_avg_int_h(int_row, src_h, out, w);
}

/// Quarter-pel `avg(integer_row, v_half_pel)` (arms (0,1) and (0,3)).
#[inline(never)]
pub fn row_avg_int_v(int_row: &[u8], rows: [&[u8]; 6], out: &mut [u8], w: usize) {
    if level() == SimdLevel::Scalar {
        return scalar::row_avg_int_v(int_row, rows, out, w);
    }
    debug_assert!(int_row.len() >= w && rows.iter().all(|r| r.len() >= w) && out.len() >= w);
    let mut i = 0;
    // SAFETY: int_row and every row have w bytes, out has w bytes.
    unsafe {
        while i + 8 <= w {
            let vp = round_pack5(fir6_v8(rows, i));
            store8(out, i, _mm_avg_epu8(load8(int_row, i), vp));
            i += 8;
        }
    }
    while i < w {
        out[i] = avg(int_row[i], v_sample(&rows, i));
        i += 1;
    }
}

/// Quarter-pel `avg(h_half_pel, v_half_pel)` (arms (1,1), (3,1), (1,3), (3,3)).
#[inline(never)]
pub fn row_avg_h_v(src_h: &[u8], rows_v: [&[u8]; 6], out: &mut [u8], w: usize) {
    if level() == SimdLevel::Scalar {
        return scalar::row_avg_h_v(src_h, rows_v, out, w);
    }
    debug_assert!(src_h.len() >= w + 5 && rows_v.iter().all(|r| r.len() >= w) && out.len() >= w);
    let mut i = 0;
    // SAFETY: src_h has w + 5 bytes, every row has w bytes, out has w bytes.
    unsafe {
        while i + 8 <= w {
            let hp = round_pack5(fir6_h8(src_h, i));
            let vp = round_pack5(fir6_v8(rows_v, i));
            store8(out, i, _mm_avg_epu8(hp, vp));
            i += 8;
        }
    }
    while i < w {
        let hp = clip_u8((fir6(src_h, i) + 16) >> 5);
        out[i] = avg(hp, v_sample(&rows_v, i));
        i += 1;
    }
}

/// Quarter-pel `avg(h_half_pel, hv_half_pel)` (arms (2,1) and (2,3)).
#[inline(never)]
pub fn row_avg_h_hv(src_h: &[u8], rows_hv: [&[u8]; 6], out: &mut [u8], w: usize) {
    if level() == SimdLevel::Scalar {
        return scalar::row_avg_h_hv(src_h, rows_hv, out, w);
    }
    debug_assert!(
        src_h.len() >= w + 5 && rows_hv.iter().all(|r| r.len() >= w + 5) && out.len() >= w
    );
    let mut i = 0;
    // SAFETY: src_h and every row have w + 5 bytes, out has w bytes.
    unsafe {
        while i + 8 <= w {
            let hp = round_pack5(fir6_h8(src_h, i));
            let hvp = hv_vertical(&hv_stage1(&rows_hv, i));
            store8(out, i, _mm_avg_epu8(hp, hvp));
            i += 8;
        }
    }
    while i < w {
        let hp = clip_u8((fir6(src_h, i) + 16) >> 5);
        out[i] = avg(hp, hv_sample(&rows_hv, i));
        i += 1;
    }
}

/// Quarter-pel `avg(v_half_pel, hv_half_pel)` (arms (1,2) and (3,2)).
#[inline(never)]
pub fn row_avg_v_hv(rows_v: [&[u8]; 6], rows_hv: [&[u8]; 6], out: &mut [u8], w: usize) {
    if level() == SimdLevel::Scalar {
        return scalar::row_avg_v_hv(rows_v, rows_hv, out, w);
    }
    debug_assert!(
        rows_v.iter().all(|r| r.len() >= w)
            && rows_hv.iter().all(|r| r.len() >= w + 5)
            && out.len() >= w
    );
    let mut i = 0;
    // SAFETY: rows_v have w bytes, rows_hv have w + 5 bytes, out has w bytes.
    unsafe {
        while i + 8 <= w {
            let vp = round_pack5(fir6_v8(rows_v, i));
            let hvp = hv_vertical(&hv_stage1(&rows_hv, i));
            store8(out, i, _mm_avg_epu8(vp, hvp));
            i += 8;
        }
    }
    while i < w {
        out[i] = avg(v_sample(&rows_v, i), hv_sample(&rows_hv, i));
        i += 1;
    }
}

/// Chroma bilinear interpolation block (in-bounds path). Reads
/// `block_w + 1` bytes from each of `block_h + 1` rows starting at
/// `top_off` in `ref_plane` (row stride `ref_width`). Weights sum to 64.
///
/// The SIMD path uses `pmaddubsw` (SSSE3) and runs only when SSSE3 was
/// detected at runtime; otherwise the scalar reference is used.
#[allow(clippy::too_many_arguments)]
pub fn chroma_bilinear_block(
    ref_plane: &[u8],
    top_off: usize,
    ref_width: usize,
    block_w: usize,
    block_h: usize,
    output: &mut [u8],
    c00: u8,
    c01: u8,
    c10: u8,
    c11: u8,
) {
    if level() >= SimdLevel::Ssse3 {
        debug_assert!(output.len() >= block_w * block_h);
        debug_assert!(ref_plane.len() > top_off + block_h * ref_width + block_w);
        // SAFETY: SSSE3 was detected; the caller guarantees block_h + 1 rows
        // of block_w + 1 bytes are readable from top_off (checked above in
        // debug builds).
        unsafe {
            ssse3_chroma_bilinear_block(
                ref_plane, top_off, ref_width, block_w, block_h, output, c00, c01, c10, c11,
            )
        }
    } else {
        scalar::chroma_bilinear_block(
            ref_plane, top_off, ref_width, block_w, block_h, output, c00, c01, c10, c11,
        );
    }
}

#[allow(clippy::too_many_arguments)]
#[target_feature(enable = "ssse3")]
#[inline(never)]
unsafe fn ssse3_chroma_bilinear_block(
    ref_plane: &[u8],
    top_off: usize,
    ref_width: usize,
    block_w: usize,
    block_h: usize,
    output: &mut [u8],
    c00: u8,
    c01: u8,
    c10: u8,
    c11: u8,
) {
    // Weight vectors as u8 pairs for maddubs: [c00, c01] and [c10, c11].
    let wa = _mm_set1_epi16(((c01 as i16) << 8) | c00 as i16);
    let wb = _mm_set1_epi16(((c11 as i16) << 8) | c10 as i16);
    for row in 0..block_h {
        let top_p = ref_plane.as_ptr().add(top_off + row * ref_width);
        let bot_p = top_p.add(ref_width);
        let out_p = output.as_mut_ptr().add(row * block_w);
        let mut i = 0;
        while i + 8 <= block_w {
            // Interleave (top[i], top[i+1]) pairs so lane k holds pixel i+k.
            let t = _mm_loadl_epi64(top_p.add(i) as *const __m128i);
            let t1 = _mm_loadl_epi64(top_p.add(i + 1) as *const __m128i);
            let ta = _mm_unpacklo_epi8(t, t1);
            let b = _mm_loadl_epi64(bot_p.add(i) as *const __m128i);
            let b1 = _mm_loadl_epi64(bot_p.add(i + 1) as *const __m128i);
            let bb = _mm_unpacklo_epi8(b, b1);
            // (c00·t + c01·t1) + (c10·b + c11·b1), max 64·255 = 16320 (i16-safe)
            let acc = _mm_add_epi16(_mm_maddubs_epi16(ta, wa), _mm_maddubs_epi16(bb, wb));
            let res = _mm_srai_epi16(_mm_add_epi16(acc, _mm_set1_epi16(32)), 6);
            _mm_storel_epi64(out_p.add(i) as *mut __m128i, _mm_packus_epi16(res, res));
            i += 8;
        }
        while i < block_w {
            let val = c00 as i32 * *top_p.add(i) as i32
                + c01 as i32 * *top_p.add(i + 1) as i32
                + c10 as i32 * *bot_p.add(i) as i32
                + c11 as i32 * *bot_p.add(i + 1) as i32;
            *out_p.add(i) = ((val + 32) >> 6) as u8;
            i += 1;
        }
    }
}

/// Bi-prediction averaging (spec 8.4.2.3.2): `out[k] = (a[k] + b[k] + 1) >> 1`,
/// exactly `pavgb`. Uses the widest available variant with a scalar tail.
pub fn bi_pred_avg(pred_l0: &[u8], pred_l1: &[u8], output: &mut [u8]) {
    let n = output.len().min(pred_l0.len()).min(pred_l1.len());
    let lvl = level();
    // SAFETY: each variant only runs when its feature level was detected,
    // and all loads/stores stay below n.
    unsafe {
        if lvl >= SimdLevel::Avx512 {
            avx512_avg_bytes(pred_l0, pred_l1, output, n)
        } else if lvl >= SimdLevel::Avx2 {
            avx2_avg_bytes(pred_l0, pred_l1, output, n)
        } else if lvl >= SimdLevel::Sse2 {
            sse2_avg_bytes(pred_l0, pred_l1, output, n)
        } else {
            scalar::bi_pred_avg(pred_l0, pred_l1, output)
        }
    }
}

#[inline(always)]
unsafe fn sse_avg_tail(a: &[u8], b: &[u8], out: &mut [u8], mut i: usize, n: usize) {
    while i + 16 <= n {
        let va = _mm_loadu_si128(a.as_ptr().add(i) as *const __m128i);
        let vb = _mm_loadu_si128(b.as_ptr().add(i) as *const __m128i);
        store16(out, i, _mm_avg_epu8(va, vb));
        i += 16;
    }
    while i < n {
        out[i] = avg(a[i], b[i]);
        i += 1;
    }
}

#[target_feature(enable = "sse2")]
#[inline(never)]
unsafe fn sse2_avg_bytes(a: &[u8], b: &[u8], out: &mut [u8], n: usize) {
    sse_avg_tail(a, b, out, 0, n);
}

#[target_feature(enable = "avx2")]
#[inline(never)]
unsafe fn avx2_avg_bytes(a: &[u8], b: &[u8], out: &mut [u8], n: usize) {
    let mut i = 0;
    while i + 32 <= n {
        let va = _mm256_loadu_si256(a.as_ptr().add(i) as *const __m256i);
        let vb = _mm256_loadu_si256(b.as_ptr().add(i) as *const __m256i);
        _mm256_storeu_si256(
            out.as_mut_ptr().add(i) as *mut __m256i,
            _mm256_avg_epu8(va, vb),
        );
        i += 32;
    }
    sse_avg_tail(a, b, out, i, n);
}

#[target_feature(enable = "avx512bw,avx512vl,avx512f")]
#[inline(never)]
unsafe fn avx512_avg_bytes(a: &[u8], b: &[u8], out: &mut [u8], n: usize) {
    let mut i = 0;
    while i + 64 <= n {
        let va = _mm512_loadu_si512(a.as_ptr().add(i) as *const __m512i);
        let vb = _mm512_loadu_si512(b.as_ptr().add(i) as *const __m512i);
        _mm512_storeu_si512(
            out.as_mut_ptr().add(i) as *mut __m512i,
            _mm512_avg_epu8(va, vb),
        );
        i += 64;
    }
    // Buffers are at most 16x16 (256 bytes); finish with SSE-wide chunks.
    sse_avg_tail(a, b, out, i, n);
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec::Vec;

    /// Deterministic xorshift PRNG for differential tests.
    struct Rng(u64);
    impl Rng {
        fn next_u32(&mut self) -> u32 {
            let mut x = self.0;
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            self.0 = x;
            x as u32
        }
        fn byte(&mut self) -> u8 {
            (self.next_u32() >> 24) as u8
        }
        /// Biased towards 0 and 255 to exercise saturation paths.
        fn byte_biased(&mut self) -> u8 {
            match self.next_u32() % 4 {
                0 => 0,
                1 => 255,
                _ => self.byte(),
            }
        }
        fn bytes(&mut self, n: usize) -> Vec<u8> {
            (0..n).map(|_| self.byte_biased()).collect()
        }
    }

    /// Serialize tests that toggle the global force-scalar switch.
    static PATH_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn with_both_paths<T>(f: impl Fn() -> T) -> (T, T) {
        let _guard = PATH_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        set_force_scalar(true);
        let scalar = f();
        set_force_scalar(false);
        let simd = f();
        (scalar, simd)
    }

    fn cmp(name: &str, w: usize, a: &[u8], b: &[u8]) {
        assert_eq!(a.len(), b.len(), "{name} length mismatch at w={w}");
        for (i, (x, y)) in a.iter().zip(b.iter()).enumerate() {
            assert_eq!(x, y, "{name} mismatch at w={w} index {i}: {x} != {y}");
        }
    }

    fn rows6(bufs: &[Vec<u8>]) -> [&[u8]; 6] {
        [&bufs[0], &bufs[1], &bufs[2], &bufs[3], &bufs[4], &bufs[5]]
    }

    #[test]
    fn differential_row_half_pel_h() {
        let mut rng = Rng(0x5eed_1234);
        for w in 1..=33usize {
            let src = rng.bytes(w + 5);
            let (a, b) = with_both_paths(|| {
                let mut out = vec![0u8; w];
                row_half_pel_h(&src, &mut out, w);
                out
            });
            cmp("row_half_pel_h", w, &a, &b);
        }
    }

    #[test]
    fn differential_row_half_pel_v() {
        let mut rng = Rng(0x5eed_5678);
        for w in 1..=33usize {
            let bufs: Vec<Vec<u8>> = (0..6).map(|_| rng.bytes(w)).collect();
            let rows = rows6(&bufs);
            let (a, b) = with_both_paths(|| {
                let mut out = vec![0u8; w];
                row_half_pel_v(rows, &mut out, w);
                out
            });
            cmp("row_half_pel_v", w, &a, &b);
        }
    }

    #[test]
    fn differential_row_half_pel_hv() {
        let mut rng = Rng(0x5eed_9abc);
        for w in 1..=33usize {
            let bufs: Vec<Vec<u8>> = (0..6).map(|_| rng.bytes(w + 5)).collect();
            let rows = rows6(&bufs);
            let (a, b) = with_both_paths(|| {
                let mut out = vec![0u8; w];
                row_half_pel_hv(rows, &mut out, w);
                out
            });
            cmp("row_half_pel_hv", w, &a, &b);
        }
    }

    #[test]
    fn diagonal_block_matches_scalar_rows() {
        let mut rng = Rng(0x5eed_7654);
        let _guard = PATH_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        for w in [4, 8, 16] {
            for h in [4, 8, 16] {
                for stride in [w + 5, 48] {
                    let src = rng.bytes((h + 4) * stride + w + 5);
                    for (fx, fy) in [(2, 2), (2, 1), (2, 3), (1, 2), (3, 2)] {
                        let mut out = [0u8; 16 * 16];
                        assert!(block_qpel_diagonal(&src, stride, w, h, fx, fy, &mut out));
                        let mut expected = [0u8; 16 * 16];
                        for (r, out_row) in expected[..w * h].chunks_exact_mut(w).enumerate() {
                            let rows = core::array::from_fn(|i| &src[(r + i) * stride..]);
                            match (fx, fy) {
                                (2, 2) => scalar::row_half_pel_hv(rows, out_row, w),
                                (2, 1) => scalar::row_avg_h_hv(rows[2], rows, out_row, w),
                                (2, 3) => scalar::row_avg_h_hv(rows[3], rows, out_row, w),
                                (1, 2) => {
                                    scalar::row_avg_v_hv(rows.map(|r| &r[2..]), rows, out_row, w)
                                }
                                (3, 2) => {
                                    scalar::row_avg_v_hv(rows.map(|r| &r[3..]), rows, out_row, w)
                                }
                                _ => unreachable!(),
                            }
                        }
                        assert_eq!(out, expected, "{w}x{h} frac ({fx},{fy})");
                    }
                }
            }
        }
    }

    #[test]
    fn differential_qpel_avg_kernels() {
        let mut rng = Rng(0x5eed_def0);
        for w in 1..=33usize {
            let int_row = rng.bytes(w);
            let src_h = rng.bytes(w + 5);
            let rv: Vec<Vec<u8>> = (0..6).map(|_| rng.bytes(w)).collect();
            let rhv: Vec<Vec<u8>> = (0..6).map(|_| rng.bytes(w + 5)).collect();
            let rows_v = rows6(&rv);
            let rows_hv = rows6(&rhv);

            let run = |name: &str, f: &dyn Fn(&mut [u8])| {
                let (a, b) = with_both_paths(|| {
                    let mut out = vec![0u8; w];
                    f(&mut out);
                    out
                });
                cmp(name, w, &a, &b);
            };
            run("row_avg_int_h", &|o| row_avg_int_h(&int_row, &src_h, o, w));
            run("row_avg_h_int", &|o| row_avg_h_int(&src_h, &int_row, o, w));
            run("row_avg_int_v", &|o| row_avg_int_v(&int_row, rows_v, o, w));
            run("row_avg_h_v", &|o| row_avg_h_v(&src_h, rows_v, o, w));
            run("row_avg_h_hv", &|o| row_avg_h_hv(&src_h, rows_hv, o, w));
            run("row_avg_v_hv", &|o| row_avg_v_hv(rows_v, rows_hv, o, w));
        }
    }

    #[test]
    fn differential_chroma_bilinear_block() {
        let mut rng = Rng(0x5eed_1111);
        for frac_x in 0..8i32 {
            for frac_y in 0..8i32 {
                if frac_x == 0 && frac_y == 0 {
                    continue;
                }
                let c00 = ((8 - frac_x) * (8 - frac_y)) as u8;
                let c01 = (frac_x * (8 - frac_y)) as u8;
                let c10 = ((8 - frac_x) * frac_y) as u8;
                let c11 = (frac_x * frac_y) as u8;
                let (bw, bh, w) = (8usize, 8usize, 64usize);
                let plane: Vec<u8> = (0..(bh + 2) * w).map(|_| rng.byte()).collect();
                let top_off = w + 3; // deliberately not row-aligned
                let (a, b) = with_both_paths(|| {
                    let mut out = vec![0u8; bw * bh];
                    chroma_bilinear_block(&plane, top_off, w, bw, bh, &mut out, c00, c01, c10, c11);
                    out
                });
                cmp(
                    "chroma_bilinear_block",
                    frac_x as usize * 8 + frac_y as usize,
                    &a,
                    &b,
                );
            }
        }
    }

    #[test]
    fn differential_bi_pred_avg() {
        let mut rng = Rng(0x5eed_2222);
        for n in [1usize, 2, 7, 15, 16, 17, 31, 32, 33, 255, 256] {
            let a = rng.bytes(n);
            let b = rng.bytes(n);
            let (x, y) = with_both_paths(|| {
                let mut out = vec![0u8; n];
                bi_pred_avg(&a, &b, &mut out);
                out
            });
            cmp("bi_pred_avg", n, &x, &y);
        }
    }

    #[test]
    fn saturation_extremes() {
        fn mk6(row: &[u8]) -> [&[u8]; 6] {
            [row, row, row, row, row, row]
        }
        for w in [4usize, 8, 16] {
            let zeros_h = vec![0u8; w + 5];
            let maxs_h = vec![255u8; w + 5];
            let zeros_r = vec![0u8; w];
            let maxs_r = vec![255u8; w];
            for src in [&zeros_h, &maxs_h] {
                let (a, b) = with_both_paths(|| {
                    let mut out = vec![0u8; w];
                    row_half_pel_h(src, &mut out, w);
                    out
                });
                cmp("h extreme", w, &a, &b);
            }
            for rows in [mk6(&zeros_r), mk6(&maxs_r)] {
                let (a, b) = with_both_paths(|| {
                    let mut out = vec![0u8; w];
                    row_half_pel_v(rows, &mut out, w);
                    out
                });
                cmp("v extreme", w, &a, &b);
            }
            for rows in [mk6(&zeros_h), mk6(&maxs_h)] {
                let (a, b) = with_both_paths(|| {
                    let mut out = vec![0u8; w];
                    row_half_pel_hv(rows, &mut out, w);
                    out
                });
                cmp("hv extreme", w, &a, &b);
            }
        }
    }
}
