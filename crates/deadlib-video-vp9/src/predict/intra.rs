//! Intra prediction process (spec §8.5.1): the 10 VP9 intra modes (`DC_PRED` through `TM_PRED`),
//! with runtime-detected AVX2 dispatch for the high-frequency modes (`V_PRED`, `H_PRED`,
//! `DC_PRED`, `TM_PRED`) across all transform sizes (4/8/16/32) and bit depths (8/10/12).

use crate::common::{clip3, round2};
use crate::framebuffer::Plane;
use crate::prob_tables::{
    D117_PRED, D135_PRED, D153_PRED, D207_PRED, D45_PRED, D63_PRED, DC_PRED, H_PRED, TM_PRED,
    TX_4X4, V_PRED,
};

#[cfg(all(target_arch = "x86_64", target_feature = "sse2"))]
use super::RefSample;

#[cfg(all(target_arch = "x86_64", target_feature = "sse2"))]
#[inline]
fn load_clamped_above(plane: &Plane, x: usize, y: usize, max_x: usize, dst: &mut [u16]) {
    for (i, slot) in dst.iter_mut().enumerate() {
        // SAFETY: `y >= 1` (`have_above`), `x >= plane.x0`, and `min(x + i, max_x)` stays inside
        // this tile/strip's horizontal bounds.
        *slot = unsafe { plane.get_unchecked((x + i).min(max_x), y - 1) };
    }
}

#[cfg(all(target_arch = "x86_64", target_feature = "sse2"))]
#[inline]
fn load_clamped_left(plane: &Plane, x: usize, y: usize, max_y: usize, dst: &mut [u16]) {
    for (i, slot) in dst.iter_mut().enumerate() {
        // SAFETY: `x - 1 >= plane.x0` (`have_left` is gated at tile start) and
        // `min(y + i, max_y) < plane.height`.
        *slot = unsafe { plane.get_unchecked(x - 1, (y + i).min(max_y)) };
    }
}

/// Spec §8.5.1's `predict_intra` process.
///
/// `x`/`y` are absolute pixel coordinates within `plane`; `max_x`/`max_y` are
/// the spec's `maxX`/`maxY` (= the maximum coordinates that may be
/// referenced within the plane).
///
/// Dispatches `V_PRED`, `H_PRED`, `DC_PRED`, and `TM_PRED` to AVX2 when available; falls back
/// to `predict_intra_scalar` for the six directional modes and on CPUs without AVX2.
#[allow(clippy::too_many_arguments)]
pub fn predict_intra(
    plane: &mut Plane,
    x: usize,
    y: usize,
    have_left: bool,
    have_above: bool,
    not_on_right: bool,
    tx_size: u8,
    mode: u8,
    max_x: usize,
    max_y: usize,
    bit_depth: u8,
) {
    #[cfg(all(target_arch = "x86_64", target_feature = "sse2"))]
    if matches!(mode, V_PRED | H_PRED | DC_PRED | TM_PRED) && crate::simd::avx2_enabled() {
        if let Some(dst) = plane.byte_ptr() {
            predict_intra_avx2(
                plane, dst, x, y, have_left, have_above, tx_size, mode, max_x, max_y, bit_depth,
            );
        } else {
            let dst = plane.word_ptr().unwrap();
            predict_intra_avx2(
                plane, dst, x, y, have_left, have_above, tx_size, mode, max_x, max_y, bit_depth,
            );
        }
        return;
    }

    predict_intra_scalar(
        plane,
        x,
        y,
        have_left,
        have_above,
        not_on_right,
        tx_size,
        mode,
        max_x,
        max_y,
        bit_depth,
    );
}

/// Borrows a contiguous wide row when possible and gathers byte or clamped neighbors otherwise.
#[cfg(all(target_arch = "x86_64", target_feature = "sse2"))]
#[inline]
#[allow(clippy::too_many_arguments)]
fn with_above_row<T: RefSample>(
    plane: &Plane,
    dst: *mut T,
    x: usize,
    y: usize,
    size: usize,
    max_x: usize,
    have_above: bool,
    f: impl FnOnce(Option<&[u16]>),
) {
    let pw = plane.stride();
    let local_x = x - plane.x0;
    if have_above && !T::IS_U8 && x + size - 1 <= max_x {
        // SAFETY: `y >= 1` (`have_above`), `local_x + size <= plane.width`, and row `y - 1` is
        // strictly disjoint from destination rows `y..y + size` written by `f` through `base`.
        let above = unsafe {
            core::slice::from_raw_parts(dst.cast::<u16>().add((y - 1) * pw + local_x), size)
        };
        f(Some(above));
    } else if have_above {
        let mut above_buf = [0u16; 32];
        load_clamped_above(plane, x, y, max_x, &mut above_buf[..size]);
        f(Some(&above_buf[..size]));
    } else {
        f(None);
    }
}

#[cfg(all(target_arch = "x86_64", target_feature = "sse2"))]
#[allow(clippy::too_many_arguments)]
fn predict_intra_avx2<T: RefSample>(
    plane: &Plane,
    dst: *mut T,
    x: usize,
    y: usize,
    have_left: bool,
    have_above: bool,
    tx_size: u8,
    mode: u8,
    max_x: usize,
    max_y: usize,
    bit_depth: u8,
) {
    let log2_size = (tx_size as u32) + 2;
    let size = 1usize << log2_size;
    let pw = plane.stride();
    debug_assert!(
        x >= plane.x0 && x - plane.x0 + size <= plane.width && y + size <= plane.height,
        "predict_intra block outside its plane bounds"
    );
    let local_x = x - plane.x0;
    let base = 1u16 << (bit_depth - 1);

    match mode {
        V_PRED => {
            if !have_above {
                // SAFETY: avx2_enabled() confirmed by caller; block bounds checked above.
                unsafe {
                    crate::simd::intra_pred_fill_avx2(dst, pw, local_x, y, base - 1, size);
                }
            } else {
                with_above_row(plane, dst, x, y, size, max_x, true, |above| {
                    // SAFETY: avx2_enabled() confirmed; `above` has len `size` and bounds hold.
                    unsafe {
                        crate::simd::intra_pred_v_avx2(dst, pw, local_x, y, above.unwrap(), size);
                    }
                });
            }
        }
        H_PRED => {
            if !have_left {
                // SAFETY: avx2_enabled() confirmed; block bounds checked above.
                unsafe {
                    crate::simd::intra_pred_fill_avx2(dst, pw, local_x, y, base + 1, size);
                }
            } else {
                let mut left_buf = [0u16; 32];
                load_clamped_left(plane, x, y, max_y, &mut left_buf[..size]);
                // SAFETY: avx2_enabled() confirmed; left_buf[..size] and block bounds hold.
                unsafe {
                    crate::simd::intra_pred_h_avx2(dst, pw, local_x, y, &left_buf[..size], size);
                }
            }
        }
        DC_PRED => {
            let mut left_buf = [0u16; 32];
            let left_opt = have_left.then(|| {
                load_clamped_left(plane, x, y, max_y, &mut left_buf[..size]);
                &left_buf[..size]
            });
            with_above_row(plane, dst, x, y, size, max_x, have_above, |above_opt| {
                // SAFETY: avx2_enabled() confirmed; neighbor slices and block bounds hold.
                unsafe {
                    crate::simd::intra_pred_dc_avx2(
                        dst, pw, local_x, y, above_opt, left_opt, log2_size, bit_depth,
                    );
                }
            });
        }
        TM_PRED => {
            let top_left = if have_above && have_left {
                // SAFETY: `have_above && have_left` guarantees `(x - 1, y - 1)` is inside the strip.
                unsafe { plane.get_unchecked(x.saturating_sub(1).min(max_x), y - 1) }
            } else if have_above {
                base + 1
            } else {
                base - 1
            };
            let mut left_buf = [base + 1; 32];
            if have_left {
                load_clamped_left(plane, x, y, max_y, &mut left_buf[..size]);
            }
            let fallback_above = [base - 1; 32];
            with_above_row(plane, dst, x, y, size, max_x, have_above, |above_opt| {
                let above = above_opt.unwrap_or(&fallback_above[..size]);
                // SAFETY: avx2_enabled() confirmed; neighbor slices and block bounds hold.
                unsafe {
                    crate::simd::intra_pred_tm_avx2(
                        dst,
                        pw,
                        local_x,
                        y,
                        above,
                        &left_buf[..size],
                        top_left,
                        size,
                        bit_depth,
                    );
                }
            });
        }
        _ => unreachable!(),
    }
}

/// Scalar fallback for the six directional modes and for CPUs without AVX2 (spec §8.5.1).
#[allow(clippy::too_many_arguments)]
pub(crate) fn predict_intra_scalar(
    plane: &mut Plane,
    x: usize,
    y: usize,
    have_left: bool,
    have_above: bool,
    not_on_right: bool,
    tx_size: u8,
    mode: u8,
    max_x: usize,
    max_y: usize,
    bit_depth: u8,
) {
    let log2_size = (tx_size as u32) + 2;
    let size = 1usize << log2_size;
    let base = 1i32 << (bit_depth - 1);

    // above_row holds spec indices -1..=2*size-1 offset by +1 (max length 2*32 + 1 == 65).
    let mut above_row_buf = [0i32; 65];
    let above_row = &mut above_row_buf[..2 * size + 1];
    for i in 0..size {
        above_row[i + 1] = if have_above {
            // SAFETY: `have_above` guarantees `y >= 1` and `(x + i).min(max_x)` lies in strip bounds.
            unsafe { plane.get_unchecked((x + i).min(max_x), y - 1) as i32 }
        } else {
            base - 1
        };
    }
    for i in size..2 * size {
        above_row[i + 1] = if have_above && not_on_right && tx_size == TX_4X4 {
            // SAFETY: `not_on_right` keeps the above-right 4x4 within the current tile/strip.
            unsafe { plane.get_unchecked((x + i).min(max_x), y - 1) as i32 }
        } else {
            above_row[size]
        };
    }
    above_row[0] = if have_above && have_left {
        // SAFETY: `have_above && have_left` guarantees `(x - 1, y - 1)` lies in strip bounds.
        unsafe { plane.get_unchecked(x.saturating_sub(1).min(max_x), y - 1) as i32 }
    } else if have_above {
        base + 1
    } else {
        base - 1
    };
    let above = |i: i32| -> i32 { above_row[(i + 1) as usize] };

    let mut left_col_buf = [0i32; 32];
    let left_col = &mut left_col_buf[..size];
    for (i, slot) in left_col.iter_mut().enumerate() {
        *slot = if have_left {
            // SAFETY: `have_left` guarantees `x - 1 >= plane.x0` and `(y + i).min(max_y) < plane.height`.
            unsafe { plane.get_unchecked(x - 1, (y + i).min(max_y)) as i32 }
        } else {
            base + 1
        };
    }

    let mut pred_buf = [0i32; 1024];
    let pred = &mut pred_buf[..size * size];
    let at = |i: usize, j: usize| i * size + j;
    let avg2 = |a: i32, b: i32| round2(a + b, 1);
    let avg3 = |a: i32, b: i32, c: i32| round2(a + 2 * b + c, 2);

    match mode {
        V_PRED => {
            for i in 0..size {
                for j in 0..size {
                    pred[at(i, j)] = above(j as i32);
                }
            }
        }
        H_PRED => {
            for (i, &lv) in left_col.iter().enumerate() {
                pred[i * size..(i + 1) * size].fill(lv);
            }
        }
        D207_PRED => {
            pred[(size - 1) * size..size * size].fill(left_col[size - 1]);
            for i in 0..size.saturating_sub(1) {
                pred[at(i, 0)] = avg2(left_col[i], left_col[i + 1]);
            }
            for i in 0..size.saturating_sub(2) {
                pred[at(i, 1)] = avg3(left_col[i], left_col[i + 1], left_col[i + 2]);
            }
            if size >= 2 {
                pred[at(size - 2, 1)] =
                    avg3(left_col[size - 2], left_col[size - 1], left_col[size - 1]);
            }
            for i in (0..size.saturating_sub(1)).rev() {
                for j in 2..size {
                    pred[at(i, j)] = pred[at(i + 1, j - 2)];
                }
            }
        }
        D45_PRED => {
            for i in 0..size {
                for j in 0..size {
                    let s = (i + j) as i32;
                    pred[at(i, j)] = if s + 2 < (size as i32) * 2 {
                        avg3(above(s), above(s + 1), above(s + 2))
                    } else {
                        above(2 * size as i32 - 1)
                    };
                }
            }
        }
        D63_PRED => {
            for i in 0..size {
                for j in 0..size {
                    let h = (i / 2) as i32 + j as i32;
                    pred[at(i, j)] = if i & 1 == 1 {
                        avg3(above(h), above(h + 1), above(h + 2))
                    } else {
                        avg2(above(h), above(h + 1))
                    };
                }
            }
        }
        D117_PRED => {
            for j in 0..size {
                pred[at(0, j)] = avg2(above(j as i32 - 1), above(j as i32));
            }
            pred[at(1, 0)] = avg3(left_col[0], above(-1), above(0));
            for j in 1..size {
                pred[at(1, j)] = avg3(above(j as i32 - 2), above(j as i32 - 1), above(j as i32));
            }
            if size >= 3 {
                pred[at(2, 0)] = avg3(above(-1), left_col[0], left_col[1]);
            }
            for i in 3..size {
                pred[at(i, 0)] = avg3(left_col[i - 3], left_col[i - 2], left_col[i - 1]);
            }
            for i in 2..size {
                for j in 1..size {
                    pred[at(i, j)] = pred[at(i - 2, j - 1)];
                }
            }
        }
        D135_PRED => {
            pred[at(0, 0)] = avg3(left_col[0], above(-1), above(0));
            for j in 1..size {
                pred[at(0, j)] = avg3(above(j as i32 - 2), above(j as i32 - 1), above(j as i32));
            }
            if size >= 2 {
                pred[at(1, 0)] = avg3(above(-1), left_col[0], left_col[1]);
            }
            for i in 2..size {
                pred[at(i, 0)] = avg3(left_col[i - 2], left_col[i - 1], left_col[i]);
            }
            for i in 1..size {
                for j in 1..size {
                    pred[at(i, j)] = pred[at(i - 1, j - 1)];
                }
            }
        }
        D153_PRED => {
            pred[at(0, 0)] = avg2(left_col[0], above(-1));
            for i in 1..size {
                pred[at(i, 0)] = avg2(left_col[i - 1], left_col[i]);
            }
            if size >= 2 {
                pred[at(0, 1)] = avg3(left_col[0], above(-1), above(0));
                pred[at(1, 1)] = avg3(above(-1), left_col[0], left_col[1]);
            }
            for i in 2..size {
                pred[at(i, 1)] = avg3(left_col[i - 2], left_col[i - 1], left_col[i]);
            }
            for j in 2..size {
                pred[at(0, j)] = avg3(
                    above(j as i32 - 3),
                    above(j as i32 - 2),
                    above(j as i32 - 1),
                );
            }
            for i in 1..size {
                for j in 2..size {
                    pred[at(i, j)] = pred[at(i - 1, j - 2)];
                }
            }
        }
        TM_PRED => {
            let top_left = above(-1);
            let max_v = (1 << bit_depth) - 1;
            for (i, &lv) in left_col.iter().enumerate() {
                for j in 0..size {
                    pred[at(i, j)] = clip3(0, max_v, above(j as i32) + lv - top_left);
                }
            }
        }
        DC_PRED => {
            let value = match (have_left, have_above) {
                (true, true) => {
                    let sum: i32 = left_col
                        .iter()
                        .enumerate()
                        .map(|(k, &l)| l + above(k as i32))
                        .sum();
                    (sum + size as i32) >> (log2_size + 1)
                }
                (true, false) => {
                    (left_col.iter().sum::<i32>() + (1 << (log2_size - 1))) >> log2_size
                }
                (false, true) => {
                    ((0..size).map(|k| above(k as i32)).sum::<i32>() + (1 << (log2_size - 1)))
                        >> log2_size
                }
                (false, false) => base,
            };
            pred.fill(value);
        }
        _ => unreachable!("predict_intra: unknown intra prediction mode {mode}"),
    }

    for i in 0..size {
        for j in 0..size {
            // SAFETY: caller's block `[x..x + size) x [y..y + size)` lies within `plane`'s strip.
            unsafe {
                plane.set_unchecked(x + j, y + i, pred[at(i, j)] as u16);
            }
        }
    }
}
