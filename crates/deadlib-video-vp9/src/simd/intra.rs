//! AVX2 SIMD mirrors of `predict::predict_intra`'s high-frequency modes (spec §8.5.1):
//! `V_PRED`, `H_PRED`, `DC_PRED` (all four neighbor-availability variants), and `TM_PRED`,
//! across every transform block size (`4x4`, `8x8`, `16x16`, `32x32`) and all VP9 bit depths
//! (`8`, `10`, `12`). These kernels write into the plane's native sample type.

use core::arch::x86_64::*;

use crate::predict::RefSample;

/// Stores 4 or 8 samples from `v` into `row`.
///
/// # Safety
/// Caller must confirm `avx2_enabled()` and `row` valid for 4 (if `is_4`) or 8 sample writes.
#[inline]
#[target_feature(enable = "avx2")]
unsafe fn store_row_128<T: RefSample>(row: *mut T, v: __m128i, is_4: bool) {
    if T::IS_U8 {
        let bytes = _mm_packus_epi16(v, v);
        if is_4 {
            row.cast::<i32>().write_unaligned(_mm_cvtsi128_si32(bytes));
        } else {
            _mm_storel_epi64(row.cast(), bytes);
        }
    } else if is_4 {
        _mm_storel_epi64(row as *mut __m128i, v);
    } else {
        _mm_storeu_si128(row as *mut __m128i, v);
    }
}

/// Loads 4 or 8 `u16` lanes from `src`.
///
/// # Safety
/// Caller must confirm `avx2_enabled()` and `src` valid for 4 (if `is_4`) or 8 `u16` reads.
#[inline]
#[target_feature(enable = "avx2")]
unsafe fn load_row_128(src: *const u16, is_4: bool) -> __m128i {
    if is_4 {
        _mm_loadl_epi64(src as *const __m128i)
    } else {
        _mm_loadu_si128(src as *const __m128i)
    }
}

/// Stores 16 (`v0`) or 32 (`v0`, `v1`) samples into `row`.
///
/// # Safety
/// Caller must confirm `avx2_enabled()` and `row` valid for 16 (or 32 if `is_32`) sample writes.
#[inline]
#[target_feature(enable = "avx2")]
unsafe fn store_row_256<T: RefSample>(row: *mut T, v0: __m256i, v1: __m256i, is_32: bool) {
    for (i, v) in [v0, v1]
        .into_iter()
        .take(if is_32 { 2 } else { 1 })
        .enumerate()
    {
        let dst = row.add(i * 16);
        if T::IS_U8 {
            let bytes = _mm_packus_epi16(_mm256_castsi256_si128(v), _mm256_extracti128_si256(v, 1));
            _mm_storeu_si128(dst.cast(), bytes);
        } else {
            _mm256_storeu_si256(dst.cast(), v);
        }
    }
}

/// Fills a `size x size` block at `(start_x, start_y)` in `plane_data` (row-major samples,
/// stride `plane_width`) with the constant `val`. Shared by `DC_PRED`, unavailable-above
/// `V_PRED`, and unavailable-left `H_PRED`.
///
/// # Safety
/// `avx2_enabled()` must hold; `size` must be one of `4, 8, 16, 32`; and rows
/// `start_y..start_y + size` x columns `start_x..start_x + size` must be in bounds for
/// `plane_data`.
#[target_feature(enable = "avx2")]
pub unsafe fn intra_pred_fill_avx2<T: RefSample>(
    plane_base: *mut T,
    plane_width: usize,
    start_x: usize,
    start_y: usize,
    val: u16,
    size: usize,
) {
    debug_assert!(matches!(size, 4 | 8 | 16 | 32));
    debug_assert!(start_x + size <= plane_width);

    if size <= 8 {
        let v = _mm_set1_epi16(val as i16);
        for i in 0..size {
            store_row_128(
                plane_base.add((start_y + i) * plane_width + start_x),
                v,
                size == 4,
            );
        }
    } else {
        let v = _mm256_set1_epi16(val as i16);
        for i in 0..size {
            store_row_256(
                plane_base.add((start_y + i) * plane_width + start_x),
                v,
                v,
                size == 32,
            );
        }
    }
}

/// AVX2 `V_PRED` (vertical prediction, spec §8.5.1): copies `above[0..size]` into every
/// row of the `size x size` destination block at `(start_x, start_y)`.
///
/// # Safety
/// `avx2_enabled()` must hold; `size` must be one of `4, 8, 16, 32`; `above.len() >= size`;
/// and rows `start_y..start_y + size` x columns `start_x..start_x + size` must be in bounds
/// for `plane_data`.
#[target_feature(enable = "avx2")]
pub unsafe fn intra_pred_v_avx2<T: RefSample>(
    plane_base: *mut T,
    plane_width: usize,
    start_x: usize,
    start_y: usize,
    above: &[u16],
    size: usize,
) {
    debug_assert!(matches!(size, 4 | 8 | 16 | 32));
    debug_assert!(above.len() >= size);
    debug_assert!(start_x + size <= plane_width);

    let src = above.as_ptr();
    if size <= 8 {
        let v = load_row_128(src, size == 4);
        for i in 0..size {
            store_row_128(
                plane_base.add((start_y + i) * plane_width + start_x),
                v,
                size == 4,
            );
        }
    } else {
        let v0 = _mm256_loadu_si256(src as *const __m256i);
        let v1 = if size == 32 {
            _mm256_loadu_si256(src.add(16) as *const __m256i)
        } else {
            v0
        };
        for i in 0..size {
            store_row_256(
                plane_base.add((start_y + i) * plane_width + start_x),
                v0,
                v1,
                size == 32,
            );
        }
    }
}

/// AVX2 `H_PRED` (horizontal prediction, spec §8.5.1): broadcasts `left[i]` across row `i`
/// of the `size x size` destination block at `(start_x, start_y)`.
///
/// # Safety
/// `avx2_enabled()` must hold; `size` must be one of `4, 8, 16, 32`; `left.len() >= size`;
/// and rows `start_y..start_y + size` x columns `start_x..start_x + size` must be in bounds
/// for `plane_data`.
#[target_feature(enable = "avx2")]
pub unsafe fn intra_pred_h_avx2<T: RefSample>(
    plane_base: *mut T,
    plane_width: usize,
    start_x: usize,
    start_y: usize,
    left: &[u16],
    size: usize,
) {
    debug_assert!(matches!(size, 4 | 8 | 16 | 32));
    debug_assert!(left.len() >= size);
    debug_assert!(start_x + size <= plane_width);

    if size <= 8 {
        for (i, &lv) in left[..size].iter().enumerate() {
            store_row_128(
                plane_base.add((start_y + i) * plane_width + start_x),
                _mm_set1_epi16(lv as i16),
                size == 4,
            );
        }
    } else {
        for (i, &lv) in left[..size].iter().enumerate() {
            let v = _mm256_set1_epi16(lv as i16);
            store_row_256(
                plane_base.add((start_y + i) * plane_width + start_x),
                v,
                v,
                size == 32,
            );
        }
    }
}

/// Horizontal sum of `size` (`4, 8, 16, 32`) `u16` VP9 samples (each `<= 4095 < 32768`, so
/// non-negative as `i16`) via pairwise `madd_epi16(., 1)` -> `i32` reduction. Exact for
/// every 8/10/12-bit VP9 sample (max sum `32 * 4095 = 131_040`, far below `i32::MAX`).
///
/// # Safety
/// Caller must confirm `avx2_enabled()`, `size` is one of `4, 8, 16, 32`, and
/// `samples.len() >= size`.
#[inline]
#[target_feature(enable = "avx2")]
unsafe fn sum_u16_avx2(samples: &[u16], size: usize) -> i32 {
    debug_assert!(samples.len() >= size);
    debug_assert!(samples[..size].iter().all(|&v| v <= 4095));

    let ptr = samples.as_ptr();
    if size == 4 {
        let pairs = _mm_madd_epi16(_mm_loadl_epi64(ptr as *const __m128i), _mm_set1_epi16(1));
        return _mm_cvtsi128_si32(_mm_add_epi32(pairs, _mm_srli_si128(pairs, 4)));
    }
    let s128 = if size == 8 {
        _mm_madd_epi16(_mm_loadu_si128(ptr as *const __m128i), _mm_set1_epi16(1))
    } else {
        let ones = _mm256_set1_epi16(1);
        let mut pairs = _mm256_madd_epi16(_mm256_loadu_si256(ptr as *const __m256i), ones);
        if size == 32 {
            pairs = _mm256_add_epi32(
                pairs,
                _mm256_madd_epi16(_mm256_loadu_si256(ptr.add(16) as *const __m256i), ones),
            );
        }
        _mm_add_epi32(
            _mm256_castsi256_si128(pairs),
            _mm256_extracti128_si256(pairs, 1),
        )
    };
    let s2 = _mm_add_epi32(s128, _mm_srli_si128(s128, 8));
    _mm_cvtsi128_si32(_mm_add_epi32(s2, _mm_srli_si128(s2, 4)))
}

/// AVX2 `DC_PRED` (spec §8.5.1): handles all four neighbor-availability variants
/// (`above: Some/None` x `left: Some/None`) across sizes `4/8/16/32` (`log2_size` in
/// `2..=5`) and all VP9 bit depths (`8/10/12`), reducing the available neighbors in `i32`
/// and filling the destination block via [`intra_pred_fill_avx2`].
///
/// # Safety
/// `avx2_enabled()` must hold; `log2_size` must be in `2..=5` (`size = 1 << log2_size`);
/// any `Some(slice)` must have length `>= size` with samples in `0..=4095`; and rows
/// `start_y..start_y + size` x columns `start_x..start_x + size` must be in bounds for
/// `plane_data`.
#[allow(clippy::too_many_arguments)]
#[target_feature(enable = "avx2")]
pub unsafe fn intra_pred_dc_avx2<T: RefSample>(
    plane_base: *mut T,
    plane_width: usize,
    start_x: usize,
    start_y: usize,
    above: Option<&[u16]>,
    left: Option<&[u16]>,
    log2_size: u32,
    bit_depth: u8,
) {
    debug_assert!((2..=5).contains(&log2_size));
    let size = 1usize << log2_size;
    let dc = match (above, left) {
        (Some(a), Some(l)) => {
            (sum_u16_avx2(a, size) + sum_u16_avx2(l, size) + size as i32) >> (log2_size + 1)
        }
        (None, Some(s)) | (Some(s), None) => {
            (sum_u16_avx2(s, size) + (1 << (log2_size - 1))) >> log2_size
        }
        (None, None) => 1i32 << (bit_depth - 1),
    };
    intra_pred_fill_avx2(plane_base, plane_width, start_x, start_y, dc as u16, size);
}

/// AVX2 `TM_PRED` (TrueMotion prediction, spec §8.5.1): writes
/// `clip(above[j] + left[i] - top_left, 0, (1 << bit_depth) - 1)` directly into the
/// `size x size` block at `(start_x, start_y)`.
///
/// Exact in signed `i16` lanes for ALL VP9 bit depths (`8/10/12`): every input (`above[j]`,
/// `left[i]`, `top_left`) is in `0..=4095`, so `above[j] - top_left` (precomputed once per
/// block before the row loop) lies in `-4095..=4095`, and adding `left[i]` stays in
/// `-4095..=8190` — strictly inside `i16` (`-32768..=32767`) with zero wrap. Signed
/// `max(0)` / `min(max_val)` then leaves a non-negative value `0..=max_val` directly
/// storable as `u16`.
///
/// # Safety
/// `avx2_enabled()` must hold; `size` must be one of `4, 8, 16, 32`; `above.len() >= size`
/// and `left.len() >= size` with all samples (and `top_left`) in `0..=(1 << bit_depth) - 1`
/// (at most `4095`); and rows `start_y..start_y + size` x columns
/// `start_x..start_x + size` must be in bounds for `plane_data`.
#[allow(clippy::too_many_arguments)]
#[target_feature(enable = "avx2")]
pub unsafe fn intra_pred_tm_avx2<T: RefSample>(
    plane_base: *mut T,
    plane_width: usize,
    start_x: usize,
    start_y: usize,
    above: &[u16],
    left: &[u16],
    top_left: u16,
    size: usize,
    bit_depth: u8,
) {
    debug_assert!(matches!(size, 4 | 8 | 16 | 32));
    debug_assert!(above.len() >= size && left.len() >= size);
    debug_assert!(start_x + size <= plane_width);
    debug_assert!(top_left <= 4095);
    debug_assert!(above[..size].iter().all(|&v| v <= 4095));
    debug_assert!(left[..size].iter().all(|&v| v <= 4095));

    let max_val = ((1i32 << bit_depth) - 1) as i16;
    if size <= 8 {
        let zero = _mm_setzero_si128();
        let max_v = _mm_set1_epi16(max_val);
        let diff = _mm_sub_epi16(
            load_row_128(above.as_ptr(), size == 4),
            _mm_set1_epi16(top_left as i16),
        );
        for (i, &lv) in left[..size].iter().enumerate() {
            let sum = _mm_add_epi16(diff, _mm_set1_epi16(lv as i16));
            let clipped = _mm_min_epi16(_mm_max_epi16(sum, zero), max_v);
            store_row_128(
                plane_base.add((start_y + i) * plane_width + start_x),
                clipped,
                size == 4,
            );
        }
    } else {
        let zero = _mm256_setzero_si256();
        let max_v = _mm256_set1_epi16(max_val);
        let tl = _mm256_set1_epi16(top_left as i16);
        let diff0 = _mm256_sub_epi16(_mm256_loadu_si256(above.as_ptr() as *const __m256i), tl);
        let diff1 = if size == 32 {
            _mm256_sub_epi16(
                _mm256_loadu_si256(above.as_ptr().add(16) as *const __m256i),
                tl,
            )
        } else {
            diff0
        };
        for (i, &lv) in left[..size].iter().enumerate() {
            let lvec = _mm256_set1_epi16(lv as i16);
            let c0 = _mm256_min_epi16(_mm256_max_epi16(_mm256_add_epi16(diff0, lvec), zero), max_v);
            let c1 = if size == 32 {
                _mm256_min_epi16(_mm256_max_epi16(_mm256_add_epi16(diff1, lvec), zero), max_v)
            } else {
                c0
            };
            store_row_256(
                plane_base.add((start_y + i) * plane_width + start_x),
                c0,
                c1,
                size == 32,
            );
        }
    }
}
