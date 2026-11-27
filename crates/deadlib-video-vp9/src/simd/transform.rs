//! AVX2 SIMD inverse transforms (DCT, ADST, and lossless 4x4 WHT networks, spec §8.7)
//! fused with the reconstruction add+clip; `tile/residual.rs` owns the dispatch and the
//! scalar fallback.
//!
//! Submodules:
//! - [`dct`]: 8-lane `i32` IDCT butterfly networks (both 8-bit and HBD widened-product paths).
//! - [`adst`]: 8-bit 8-lane `i32` IADST4/8/16 networks.
//! - [`adst_hbd`]: 10/12-bit 4-lane `i64` IADST4/8/16 and mixed-axis IDCT networks.
//!
//! This hub holds the 2D separable pass drivers, fused add+clip reconstruction, DC-only constant
//! reconstruction, and the register-only lossless 4x4 IWHT kernel.

mod adst;
mod adst_hbd;
mod dct;

pub(super) use adst::iadst_simd;
use adst_hbd::xform_pass_i64;
pub(super) use dct::{idct_permute_simd, idct_simd};

use crate::{predict::RefSample, transform::TxType};
use core::arch::x86_64::*;
use core::mem::MaybeUninit;

/// Transposes a 4x4 `i32` matrix (`r[i]` = row i) so `out[j]` = column j.
///
/// # Safety
/// Pure SSE2 register shuffle; caller must be on x86_64 (SSE2 is baseline).
#[inline]
unsafe fn transpose4x4_i32(r: &[__m128i; 4]) -> [__m128i; 4] {
    let a0 = _mm_unpacklo_epi32(r[0], r[1]);
    let a1 = _mm_unpackhi_epi32(r[0], r[1]);
    let a2 = _mm_unpacklo_epi32(r[2], r[3]);
    let a3 = _mm_unpackhi_epi32(r[2], r[3]);
    [
        _mm_unpacklo_epi64(a0, a2),
        _mm_unpackhi_epi64(a0, a2),
        _mm_unpacklo_epi64(a1, a3),
        _mm_unpackhi_epi64(a1, a3),
    ]
}

/// Transposes an 8x8 `i32` matrix (`r[i]` = row i) so `out[j]` = column j.
///
/// # Safety
/// Caller must confirm `avx2_enabled()`.
#[inline]
#[target_feature(enable = "avx2")]
unsafe fn transpose8x8_i32(r: &[__m256i; 8]) -> [__m256i; 8] {
    let t0 = _mm256_unpacklo_epi32(r[0], r[1]);
    let t1 = _mm256_unpackhi_epi32(r[0], r[1]);
    let t2 = _mm256_unpacklo_epi32(r[2], r[3]);
    let t3 = _mm256_unpackhi_epi32(r[2], r[3]);
    let t4 = _mm256_unpacklo_epi32(r[4], r[5]);
    let t5 = _mm256_unpackhi_epi32(r[4], r[5]);
    let t6 = _mm256_unpacklo_epi32(r[6], r[7]);
    let t7 = _mm256_unpackhi_epi32(r[6], r[7]);
    let s0 = _mm256_unpacklo_epi64(t0, t2);
    let s1 = _mm256_unpackhi_epi64(t0, t2);
    let s2 = _mm256_unpacklo_epi64(t1, t3);
    let s3 = _mm256_unpackhi_epi64(t1, t3);
    let s4 = _mm256_unpacklo_epi64(t4, t6);
    let s5 = _mm256_unpackhi_epi64(t4, t6);
    let s6 = _mm256_unpacklo_epi64(t5, t7);
    let s7 = _mm256_unpackhi_epi64(t5, t7);
    [
        _mm256_permute2x128_si256(s0, s4, 0x20),
        _mm256_permute2x128_si256(s1, s5, 0x20),
        _mm256_permute2x128_si256(s2, s6, 0x20),
        _mm256_permute2x128_si256(s3, s7, 0x20),
        _mm256_permute2x128_si256(s0, s4, 0x31),
        _mm256_permute2x128_si256(s1, s5, 0x31),
        _mm256_permute2x128_si256(s2, s6, 0x31),
        _mm256_permute2x128_si256(s3, s7, 0x31),
    ]
}

/// `round2(x, shift)` with a runtime shift (the 2D column pass's `min(n+2, 6)`), lane-wise.
///
/// # Safety
/// Caller must confirm `avx2_enabled()` and `shift >= 1`.
#[inline]
#[target_feature(enable = "avx2")]
unsafe fn round2_var(x: __m256i, shift: u32) -> __m256i {
    let add = _mm256_set1_epi32(1 << (shift - 1));
    _mm256_sra_epi32(_mm256_add_epi32(x, add), _mm_cvtsi32_si128(shift as i32))
}

#[inline]
#[target_feature(enable = "avx2")]
unsafe fn apply_1d_xform<const HBD: bool, const ADST: bool>(t: &mut [__m256i; 32], n: u32) {
    if ADST {
        iadst_simd(t, n);
    } else {
        idct_permute_simd(t, n);
        idct_simd::<HBD>(t, n);
    }
}

/// One separable pass of the 2D inverse transform: transforms every "row" of `input` (n0 x n0,
/// row-major i32) with the inverse DCT (`ADST == false`) or inverse ADST (`ADST == true`) and
/// writes the result TRANSPOSED into `output`, so running this twice (row then column) lands
/// the 2D result back in row-major. `max_row` is the highest row index in `input` that may be
/// non-zero (`n0 - 1` when unknown): any 8-row chunk with `cs > max_row` is skipped.
///
/// # Safety
/// Caller must confirm `avx2_enabled()`, `2 <= n <= 5` (`<= 4` when `ADST`), `input` valid for
/// `n0 * n0` initialized `i32` reads, and `output` valid for `n0 * n0` non-overlapping `i32`
/// writes.
#[inline(never)]
#[target_feature(enable = "avx2")]
unsafe fn xform_pass<const HBD: bool, const ADST: bool>(
    input: *const i32,
    output: *mut i32,
    n: u32,
    round_shift: Option<u32>,
    max_row: usize,
) {
    let n0 = 1usize << n;
    let zero = _mm256_setzero_si256();

    if n0 == 4 {
        let rows4: [__m128i; 4] =
            core::array::from_fn(|r| _mm_loadu_si128(input.add(r * 4) as *const __m128i));
        let cols = transpose4x4_i32(&rows4);
        let mut t = [zero; 32];
        for (c, &col) in cols.iter().enumerate() {
            // The upper 128 bits of `_mm256_castsi128_si256` are formally UNDEFINED; safe because
            // everything downstream on n0 == 4 is lane-wise and stores only the low 128 bits.
            t[c] = _mm256_castsi128_si256(col);
        }
        apply_1d_xform::<HBD, ADST>(&mut t, 2);
        for (k, &tk) in t[..4].iter().enumerate() {
            let v = round_shift.map_or(tk, |sh| round2_var(tk, sh));
            _mm_storeu_si128(output.add(k * 4) as *mut __m128i, _mm256_castsi256_si128(v));
        }
        return;
    }

    let mut cs = 0usize;
    while cs < n0 {
        if cs > max_row {
            for k in 0..n0 {
                _mm256_storeu_si256(output.add(k * n0 + cs) as *mut __m256i, zero);
            }
            cs += 8;
            continue;
        }
        let mut t = [zero; 32];
        let mut g = 0usize;
        while g < n0 {
            let rows8: [__m256i; 8] = core::array::from_fn(|r| {
                _mm256_loadu_si256(input.add((cs + r) * n0 + g) as *const __m256i)
            });
            t[g..g + 8].copy_from_slice(&transpose8x8_i32(&rows8));
            g += 8;
        }
        apply_1d_xform::<HBD, ADST>(&mut t, n);
        for (k, &tk) in t[..n0].iter().enumerate() {
            let v = round_shift.map_or(tk, |sh| round2_var(tk, sh));
            _mm256_storeu_si256(output.add(k * n0 + cs) as *mut __m256i, v);
        }
        cs += 8;
    }
}

/// The two separable passes of the AVX2 DCT_DCT 2D inverse transform (spec §8.7.2).
///
/// # Safety
/// Caller must confirm `avx2_enabled()`, `2 <= n <= 5`, `dequant.len() >= n0 * n0`, and `out`
/// valid for `n0 * n0` `i32` writes.
#[target_feature(enable = "avx2")]
unsafe fn idct_2d_dct<const HBD: bool>(dequant: &[i64], n: u32, max_row: usize, out: *mut i32) {
    let n0 = 1usize << n;
    let count = n0 * n0;
    let mut buf = [MaybeUninit::<i32>::uninit(); 1024];
    let mut buf2 = [MaybeUninit::<i32>::uninit(); 1024];
    let buf_ptr = buf.as_mut_ptr() as *mut i32;
    let buf2_ptr = buf2.as_mut_ptr() as *mut i32;
    for (i, &src) in dequant[..count].iter().enumerate() {
        *buf_ptr.add(i) = src as i32;
    }
    xform_pass::<HBD, false>(buf_ptr, buf2_ptr, n, None, max_row);
    xform_pass::<HBD, false>(buf2_ptr, out, n, Some((n + 2).min(6)), n0 - 1);
}

/// Unified AVX2 fused inverse transform + reconstruction entry point covering lossless 4x4 WHT,
/// 8-bit & HBD ADST-containing transforms (`n = 2..=4`), and 8-bit & HBD `DCT_DCT` (`n = 2..=5`)
/// with known non-zero row bound `max_row`.
///
/// # Safety
/// `avx2_enabled()` must hold; `dequant.len() == (1 << n)^2` with `2 <= n <= 5` (`n == 2` when
/// `lossless`, `n <= 4` when `tx_type != DctDct`); `bit_depth` must be 8, 10, or 12; the block's
/// `n0 x n0` region at `(start_x, start_y)` must lie in bounds for `plane_base` (stride
/// `plane_width`); and every row strictly below `max_row` in `dequant` must be zero.
#[allow(clippy::too_many_arguments)]
#[target_feature(enable = "avx2")]
pub unsafe fn inverse_transform_reconstruct_avx2<T: RefSample>(
    plane_base: *mut T,
    plane_width: usize,
    start_x: usize,
    start_y: usize,
    dequant: &[i64],
    n: u32,
    tx_type: TxType,
    max_row: usize,
    lossless: bool,
    bit_depth: u8,
) {
    if lossless {
        debug_assert_eq!(n, 2, "lossless transform (WHT) only supports 4x4");
        inverse_transform_wht_reconstruct_avx2(
            plane_base,
            plane_width,
            start_x,
            start_y,
            dequant,
            bit_depth,
        );
    } else if tx_type != TxType::DctDct {
        if bit_depth == 8 {
            inverse_transform_adst_reconstruct_avx2(
                plane_base,
                plane_width,
                start_x,
                start_y,
                dequant,
                n,
                tx_type,
            );
        } else {
            inverse_transform_adst_reconstruct_hbd_avx2(
                plane_base,
                plane_width,
                start_x,
                start_y,
                dequant,
                n,
                tx_type,
                bit_depth,
            );
        }
    } else if bit_depth == 8 {
        idct_dct_reconstruct::<false, T>(
            plane_base,
            plane_width,
            start_x,
            start_y,
            dequant,
            n,
            max_row,
            255,
        );
    } else {
        let max_val = (1i32 << bit_depth) - 1;
        idct_dct_reconstruct::<true, T>(
            plane_base,
            plane_width,
            start_x,
            start_y,
            dequant,
            n,
            max_row,
            max_val,
        );
    }
}

/// AVX2 8-bit inverse transform + reconstruction for the ADST-containing transform types
/// (`AdstDct` / `DctAdst` / `AdstAdst`, spec §8.7.1.4-§8.7.1.9 + §8.7.2).
///
/// # Safety
/// Same contract as [`inverse_transform_dct_dct_reconstruct_eob_avx2`] (with `2 <= n <= 4`);
/// `tx_type` must not be `DctDct` and the stream must be 8-bit.
#[target_feature(enable = "avx2")]
unsafe fn inverse_transform_adst_reconstruct_avx2<T: RefSample>(
    plane_base: *mut T,
    plane_width: usize,
    start_x: usize,
    start_y: usize,
    dequant: &[i64],
    n: u32,
    tx_type: TxType,
) {
    debug_assert!((2..=4).contains(&n));
    debug_assert!(tx_type != TxType::DctDct);
    let n0 = 1usize << n;
    let count = n0 * n0;
    let mut buf = [MaybeUninit::<i32>::uninit(); 256];
    let mut buf2 = [MaybeUninit::<i32>::uninit(); 256];
    let mut o = [MaybeUninit::<i32>::uninit(); 256];
    let buf_ptr = buf.as_mut_ptr() as *mut i32;
    let buf2_ptr = buf2.as_mut_ptr() as *mut i32;
    let o_ptr = o.as_mut_ptr() as *mut i32;
    for (i, &src) in dequant[..count].iter().enumerate() {
        *buf_ptr.add(i) = src as i32;
    }
    let shift = (n + 2).min(6);
    match tx_type {
        TxType::DctAdst | TxType::AdstAdst => {
            xform_pass::<false, true>(buf_ptr, buf2_ptr, n, None, n0 - 1)
        }
        _ => xform_pass::<false, false>(buf_ptr, buf2_ptr, n, None, n0 - 1),
    }
    match tx_type {
        TxType::AdstDct | TxType::AdstAdst => {
            xform_pass::<false, true>(buf2_ptr, o_ptr, n, Some(shift), n0 - 1)
        }
        _ => xform_pass::<false, false>(buf2_ptr, o_ptr, n, Some(shift), n0 - 1),
    }
    reconstruct_add_clip(plane_base, plane_width, start_x, start_y, o_ptr, n0, 255);
}

/// High-bit-depth (10/12-bit) companion to [`inverse_transform_adst_reconstruct_avx2`].
///
/// # Safety
/// Same contract as [`inverse_transform_adst_reconstruct_avx2`] (`2 <= n <= 4`,
/// `tx_type != DctDct`); `bit_depth` must be 10 or 12.
#[allow(clippy::too_many_arguments)]
#[target_feature(enable = "avx2")]
unsafe fn inverse_transform_adst_reconstruct_hbd_avx2<T: RefSample>(
    plane_base: *mut T,
    plane_width: usize,
    start_x: usize,
    start_y: usize,
    dequant: &[i64],
    n: u32,
    tx_type: TxType,
    bit_depth: u8,
) {
    assert!((2..=4).contains(&n));
    debug_assert!(tx_type != TxType::DctDct);
    let n0 = 1usize << n;
    let count = n0 * n0;
    let mut buf = [0i64; 256];
    let mut buf2 = [0i64; 256];
    buf[..count].copy_from_slice(&dequant[..count]);
    let shift = (n + 2).min(6);
    match tx_type {
        TxType::DctAdst | TxType::AdstAdst => xform_pass_i64::<true>(&buf, &mut buf2, n, None),
        _ => xform_pass_i64::<false>(&buf, &mut buf2, n, None),
    }
    match tx_type {
        TxType::AdstDct | TxType::AdstAdst => {
            xform_pass_i64::<true>(&buf2, &mut buf, n, Some(shift))
        }
        _ => xform_pass_i64::<false>(&buf2, &mut buf, n, Some(shift)),
    }
    let mut o = [MaybeUninit::<i32>::uninit(); 256];
    let o_ptr = o.as_mut_ptr() as *mut i32;
    for (i, &src) in buf[..count].iter().enumerate() {
        *o_ptr.add(i) = src as i32;
    }
    let max_val = (1i32 << bit_depth) - 1;
    reconstruct_add_clip(
        plane_base,
        plane_width,
        start_x,
        start_y,
        o_ptr,
        n0,
        max_val,
    );
}

/// Shared body of the fused DCT_DCT entries above.
///
/// # Safety
/// Caller must confirm `avx2_enabled()`, `2 <= n <= 5`, `dequant.len() >= n0 * n0`, and the
/// `n0 x n0` block at `(start_x, start_y)` is in bounds for `plane_base` (stride `plane_width`).
#[allow(clippy::too_many_arguments)]
#[target_feature(enable = "avx2")]
unsafe fn idct_dct_reconstruct<const HBD: bool, T: RefSample>(
    plane_base: *mut T,
    plane_width: usize,
    start_x: usize,
    start_y: usize,
    dequant: &[i64],
    n: u32,
    max_row: usize,
    max_val: i32,
) {
    let n0 = 1usize << n;
    let mut o = [MaybeUninit::<i32>::uninit(); 1024];
    let o_ptr = o.as_mut_ptr() as *mut i32;
    idct_2d_dct::<HBD>(dequant, n, max_row, o_ptr);
    reconstruct_add_clip(
        plane_base,
        plane_width,
        start_x,
        start_y,
        o_ptr,
        n0,
        max_val,
    );
}

/// Adds a constant residual `delta` (clamped to `[-max_val, max_val]`, so `pred + delta` fits
/// signed `i16` for all 8/10/12-bit samples) to an `n0 x n0` block in `plane_base` and clips to
/// `0..=max_val`. Used for DC-only `DCT_DCT` transform blocks (`eob == 1`).
///
/// # Safety
/// `avx2_enabled()` must hold; `n0` must be `4, 8, 16, or 32`; `delta` must be in
/// `-max_val..=max_val` with `max_val <= 4095`; and `start_y..start_y+n0` x `start_x..start_x+n0`
/// must be in bounds for `plane_base` (stride `plane_width`).
#[target_feature(enable = "avx2")]
pub unsafe fn reconstruct_add_const_avx2<T: RefSample>(
    plane_base: *mut T,
    plane_width: usize,
    start_x: usize,
    start_y: usize,
    delta: i16,
    n0: usize,
    max_val: i16,
) {
    debug_assert!(matches!(n0, 4 | 8 | 16 | 32));
    debug_assert!(start_x + n0 <= plane_width);

    if T::IS_U8 {
        let base = plane_base.cast::<u8>();
        let add = _mm_set1_epi8(delta.max(0) as i8);
        let sub = _mm_set1_epi8((-delta).max(0) as i8);
        match n0 {
            4 => add_dc_u8::<4>(base, plane_width, start_x, start_y, add, sub),
            8 => add_dc_u8::<8>(base, plane_width, start_x, start_y, add, sub),
            16 => add_dc_u8::<16>(base, plane_width, start_x, start_y, add, sub),
            _ => add_dc_u8::<32>(base, plane_width, start_x, start_y, add, sub),
        }
        return;
    }

    let base = plane_base;
    if n0 <= 8 {
        let zero = _mm_setzero_si128();
        let max_v = _mm_set1_epi16(max_val);
        let d = _mm_set1_epi16(delta);
        for i in 0..n0 {
            let row = base.add((start_y + i) * plane_width + start_x);
            let pred = if n0 == 4 {
                _mm_loadl_epi64(row as *const __m128i)
            } else {
                _mm_loadu_si128(row as *const __m128i)
            };
            let clipped = _mm_min_epi16(_mm_max_epi16(_mm_add_epi16(pred, d), zero), max_v);
            if n0 == 4 {
                _mm_storel_epi64(row as *mut __m128i, clipped);
            } else {
                _mm_storeu_si128(row as *mut __m128i, clipped);
            }
        }
    } else {
        let zero = _mm256_setzero_si256();
        let max_v = _mm256_set1_epi16(max_val);
        let d = _mm256_set1_epi16(delta);
        for i in 0..n0 {
            let row = base.add((start_y + i) * plane_width + start_x);
            let mut j = 0usize;
            while j < n0 {
                let p = _mm256_loadu_si256(row.add(j) as *const __m256i);
                let c = _mm256_min_epi16(_mm256_max_epi16(_mm256_add_epi16(p, d), zero), max_v);
                _mm256_storeu_si256(row.add(j) as *mut __m256i, c);
                j += 16;
            }
        }
    }
}

#[target_feature(enable = "avx2")]
unsafe fn add_dc_u8<const N: usize>(
    base: *mut u8,
    stride: usize,
    x: usize,
    y: usize,
    add: __m128i,
    sub: __m128i,
) {
    for i in 0..N {
        let row = base.add((y + i) * stride + x);
        if N == 4 {
            let pred = _mm_cvtsi32_si128(row.cast::<i32>().read_unaligned());
            let result = _mm_subs_epu8(_mm_adds_epu8(pred, add), sub);
            row.cast::<i32>().write_unaligned(_mm_cvtsi128_si32(result));
        } else if N == 8 {
            let pred = _mm_loadl_epi64(row.cast());
            let result = _mm_subs_epu8(_mm_adds_epu8(pred, add), sub);
            _mm_storel_epi64(row.cast(), result);
        } else {
            for j in (0..N).step_by(16) {
                let ptr = row.add(j).cast::<__m128i>();
                let pred = _mm_loadu_si128(ptr);
                let result = _mm_subs_epu8(_mm_adds_epu8(pred, add), sub);
                _mm_storeu_si128(ptr, result);
            }
        }
    }
}

#[inline]
#[target_feature(enable = "avx2")]
unsafe fn add_residual4<T: RefSample>(row: *mut T, resid: __m128i, max: __m128i) {
    let pred = if T::IS_U8 {
        _mm_cvtepu8_epi32(_mm_cvtsi32_si128(row.cast::<i32>().read_unaligned()))
    } else {
        _mm_cvtepu16_epi32(_mm_loadl_epi64(row.cast()))
    };
    let clipped = _mm_min_epi32(
        _mm_max_epi32(_mm_add_epi32(pred, resid), _mm_setzero_si128()),
        max,
    );
    let packed = _mm_packus_epi32(clipped, clipped);
    if T::IS_U8 {
        row.cast::<i32>()
            .write_unaligned(_mm_cvtsi128_si32(_mm_packus_epi16(packed, packed)));
    } else {
        _mm_storel_epi64(row.cast(), packed);
    }
}

/// Per-pixel `clip(pred + residual, 0, max_val)` of the n0 x n0 residual block `o` (row-major)
/// straight into the reconstruction plane.
///
/// # Safety
/// Caller must confirm `avx2_enabled()`, `n0` is `4, 8, 16, or 32`, `o` is valid for `n0 * n0`
/// initialized `i32` reads, and `(start_x..start_x + n0, start_y..start_y + n0)` lies in
/// `plane_data` (stride `plane_width`).
#[inline(never)]
#[target_feature(enable = "avx2")]
unsafe fn reconstruct_add_clip<T: RefSample>(
    plane_base: *mut T,
    plane_width: usize,
    start_x: usize,
    start_y: usize,
    o: *const i32,
    n0: usize,
    max_val: i32,
) {
    let base = plane_base;
    let zero = _mm256_setzero_si256();
    let max = _mm256_set1_epi32(max_val);

    if n0 == 4 {
        for i in 0..4 {
            let row = base.add((start_y + i) * plane_width + start_x);
            let resid = _mm_loadu_si128(o.add(i * 4) as *const __m128i);
            add_residual4(row, resid, _mm256_castsi256_si128(max));
        }
        return;
    }

    for i in 0..n0 {
        let mut j = 0usize;
        while j < n0 {
            let row = base.add((start_y + i) * plane_width + start_x + j);
            let pred = if T::IS_U8 {
                _mm256_cvtepu8_epi32(_mm_loadl_epi64(row.cast()))
            } else {
                _mm256_cvtepu16_epi32(_mm_loadu_si128(row.cast()))
            };
            let resid = _mm256_loadu_si256(o.add(i * n0 + j) as *const __m256i);
            let sum = _mm256_add_epi32(pred, resid);
            let clipped = _mm256_min_epi32(_mm256_max_epi32(sum, zero), max);
            let packed = _mm_packus_epi32(
                _mm256_castsi256_si128(clipped),
                _mm256_extracti128_si256(clipped, 1),
            );
            if T::IS_U8 {
                _mm_storel_epi64(row.cast(), _mm_packus_epi16(packed, packed));
            } else {
                _mm_storeu_si128(row.cast(), packed);
            }
            j += 8;
        }
    }
}

/// Vector 1D inverse Walsh-Hadamard transform (`transform.rs::iwht4`, spec §8.7.1.10) on 4
/// rows/columns in parallel (`t[k]` holds element `k` across 4 lanes).
///
/// # Safety
/// Caller must confirm `avx2_enabled()`.
#[inline]
#[target_feature(enable = "avx2")]
pub(super) unsafe fn iwht4_simd<const ROW_SHIFT: bool>(t: &mut [__m128i; 4]) {
    let (mut a, mut c, mut d, mut b) = if ROW_SHIFT {
        (
            _mm_srai_epi32(t[0], 2),
            _mm_srai_epi32(t[1], 2),
            _mm_srai_epi32(t[2], 2),
            _mm_srai_epi32(t[3], 2),
        )
    } else {
        (t[0], t[1], t[2], t[3])
    };
    a = _mm_add_epi32(a, c);
    d = _mm_sub_epi32(d, b);
    let e = _mm_srai_epi32(_mm_sub_epi32(a, d), 1);
    b = _mm_sub_epi32(e, b);
    c = _mm_sub_epi32(e, c);
    a = _mm_sub_epi32(a, b);
    d = _mm_add_epi32(d, c);
    t[0] = a;
    t[1] = b;
    t[2] = c;
    t[3] = d;
}

/// Loads 4 `i64` coefficients and narrows their low 32 bits into a single `__m128i` of 4 `i32`s.
///
/// # Safety
/// Caller must confirm `avx2_enabled()` and that `ptr` is valid for reading 4 contiguous `i64`s.
#[inline]
#[target_feature(enable = "avx2")]
unsafe fn load_4_i64_as_i32(ptr: *const i64) -> __m128i {
    let v = _mm256_loadu_si256(ptr as *const __m256i);
    let lo_dwords = _mm256_shuffle_epi32(v, 0b10_00_10_00);
    let packed = _mm256_permute4x64_epi64(lo_dwords, 0b11_01_10_00);
    _mm256_castsi256_si128(packed)
}

/// AVX2 lossless 4x4 inverse Walsh-Hadamard transform + fused reconstruction (spec §8.7.1.10
/// and §8.7.2), exact across all VP9 bit depths (`8/10/12`).
///
/// # Safety
/// `avx2_enabled()` must hold; `dequant.len()` must be 16; `bit_depth` must be 8, 10, or 12;
/// and the 4x4 block at `(start_x, start_y)` (stride `plane_width`) must be in bounds.
#[target_feature(enable = "avx2")]
unsafe fn inverse_transform_wht_reconstruct_avx2<T: RefSample>(
    plane_base: *mut T,
    plane_width: usize,
    start_x: usize,
    start_y: usize,
    dequant: &[i64],
    bit_depth: u8,
) {
    debug_assert_eq!(dequant.len(), 16);
    debug_assert!(start_x + 4 <= plane_width);

    let dq_ptr = dequant.as_ptr();
    let rows = [
        load_4_i64_as_i32(dq_ptr),
        load_4_i64_as_i32(dq_ptr.add(4)),
        load_4_i64_as_i32(dq_ptr.add(8)),
        load_4_i64_as_i32(dq_ptr.add(12)),
    ];

    let mut t = transpose4x4_i32(&rows);
    iwht4_simd::<true>(&mut t);
    let mut col_t = transpose4x4_i32(&t);
    iwht4_simd::<false>(&mut col_t);

    let base = plane_base;
    let max = _mm_set1_epi32((1i32 << bit_depth) - 1);
    for (i, &resid) in col_t.iter().enumerate() {
        let row = base.add((start_y + i) * plane_width + start_x);
        add_residual4(row, resid, max);
    }
}
