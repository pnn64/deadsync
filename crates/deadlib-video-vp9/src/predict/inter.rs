//! Inter prediction / motion compensation process (spec §8.5.2): MV selection, edge clamping,
//! reference-frame scaling (§8.5.2.1-§8.5.2.3), two-pass 8-tap subpel interpolation (§8.5.2.4,
//! with direct-to-u16 AVX2 fast paths), and compound averaging.

use crate::common::{clip3, round2};
use crate::framebuffer::{Plane, RefPlane, RefSlice};
use crate::prob_tables::{
    BLOCK_8X8, INTRA_FRAME, NUM_8X8_BLOCKS_HIGH_LOOKUP, NUM_8X8_BLOCKS_WIDE_LOOKUP,
};
use crate::subpel::{
    INTERP_EXTEND, MAX_BLOCK_DIM, MAX_INTERMEDIATE_HEIGHT, REF_SCALE_SHIFT, SUBPEL_BITS,
    SUBPEL_FILTERS, SUBPEL_MASK, SUBPEL_SHIFTS,
};
use crate::tile::Mv;

/// Reference pixel sample type (`u8` for 8-bit DPB planes, `u16` for 10/12-bit DPB planes),
/// allowing hot AVX2 unscaled inter-prediction kernels to share one implementation.
#[cfg(all(target_arch = "x86_64", target_feature = "sse2"))]
pub(crate) trait RefSample: Copy {
    const IS_U8: bool;
}

#[cfg(all(target_arch = "x86_64", target_feature = "sse2"))]
impl RefSample for u8 {
    const IS_U8: bool = true;
}

#[cfg(all(target_arch = "x86_64", target_feature = "sse2"))]
impl RefSample for u16 {
    const IS_U8: bool = false;
}

/// A view over one plane of a reference frame (equivalent to the spec's
/// `FrameStore[ refIdx ][ plane ]` + `RefFrameWidth`/`RefFrameHeight`).
/// `width`/`height` are the overall frame size based on luma (plane 0) -- the
/// `RefFrameWidth[refIdx]` used in spec §8.5.2.3's scaling computation. `plane` is the
/// reference pixel data cropped to the relevant plane (`RefPlane::width`/`height` are `lastX+1`/`lastY+1`).
pub struct RefPlaneView<'a> {
    pub plane: &'a RefPlane,
    pub width: u32,
    pub height: u32,
}

#[inline]
fn round_mv_comp_q2(value: i32) -> i32 {
    if value < 0 {
        (value - 1) / 2
    } else {
        (value + 1) / 2
    }
}

#[inline]
fn round_mv_comp_q4(value: i32) -> i32 {
    if value < 0 {
        (value - 2) / 4
    } else {
        (value + 2) / 4
    }
}

/// `clip1(x)` = `Clip3( 0, (1<<BitDepth)-1, x )` (spec §4).
#[inline]
fn clip1(x: i32, bit_depth: u8) -> i32 {
    clip3(0, (1i32 << bit_depth) - 1, x)
}

/// Per-block-invariant parameters for [`predict_inter`] (spec §8.5.2) and its helpers.
pub struct InterPredictParams<'a> {
    pub ref_frame: [u8; 2],
    pub block_mvs: &'a [[Mv; 4]; 2],
    pub interp_filter: u8,
    pub mi_row: u32,
    pub mi_col: u32,
    pub mi_size: u8,
    pub mi_rows: u32,
    pub mi_cols: u32,
    pub subsampling_x: u32,
    pub subsampling_y: u32,
    pub frame_width: u32,
    pub frame_height: u32,
    pub bit_depth: u8,
    pub refs: [Option<&'a RefPlaneView<'a>>; 2],
}

/// Motion vector selection process (spec §8.5.2.1).
fn select_mv(plane: usize, ref_list: usize, block_idx: usize, p: &InterPredictParams) -> Mv {
    let bm = &p.block_mvs[ref_list];
    if plane == 0 || p.mi_size >= BLOCK_8X8 {
        return bm[block_idx];
    }
    match (p.subsampling_x, p.subsampling_y) {
        (0, 0) => bm[block_idx],
        (0, 1) => [
            round_mv_comp_q2(bm[block_idx][0] + bm[block_idx + 2][0]),
            round_mv_comp_q2(bm[block_idx][1] + bm[block_idx + 2][1]),
        ],
        (1, 0) => [
            round_mv_comp_q2(bm[block_idx][0] + bm[block_idx + 1][0]),
            round_mv_comp_q2(bm[block_idx][1] + bm[block_idx + 1][1]),
        ],
        _ => [
            round_mv_comp_q4(bm[0][0] + bm[1][0] + bm[2][0] + bm[3][0]),
            round_mv_comp_q4(bm[0][1] + bm[1][1] + bm[2][1] + bm[3][1]),
        ],
    }
}

/// Motion vector clamping process (spec §8.5.2.2).
fn clamp_mv_for_plane(plane: usize, mv: Mv, p: &InterPredictParams) -> Mv {
    let (sx, sy) = if plane == 0 {
        (0i32, 0i32)
    } else {
        (p.subsampling_x as i32, p.subsampling_y as i32)
    };
    let bh = NUM_8X8_BLOCKS_HIGH_LOOKUP[p.mi_size as usize] as i32;
    let bw = NUM_8X8_BLOCKS_WIDE_LOOKUP[p.mi_size as usize] as i32;
    let mi_row = p.mi_row as i32;
    let mi_col = p.mi_col as i32;
    let mi_rows = p.mi_rows as i32;
    let mi_cols = p.mi_cols as i32;

    let mb_to_top_edge = -((mi_row * 8) * 16) >> sy;
    let mb_to_bottom_edge = ((mi_rows - bh - mi_row) * 8 * 16) >> sy;
    let mb_to_left_edge = -((mi_col * 8) * 16) >> sx;
    let mb_to_right_edge = ((mi_cols - bw - mi_col) * 8 * 16) >> sx;

    let spel_left = (INTERP_EXTEND + ((bw * 8) >> sx)) << SUBPEL_BITS;
    let spel_right = spel_left - SUBPEL_SHIFTS;
    let spel_top = (INTERP_EXTEND + ((bh * 8) >> sy)) << SUBPEL_BITS;
    let spel_bottom = spel_top - SUBPEL_SHIFTS;

    [
        clip3(
            mb_to_top_edge - spel_top,
            mb_to_bottom_edge + spel_bottom,
            (2 * mv[0]) >> sy,
        ),
        clip3(
            mb_to_left_edge - spel_left,
            mb_to_right_edge + spel_right,
            (2 * mv[1]) >> sx,
        ),
    ]
}

/// Motion vector scaling process (spec §8.5.2.3). Returns `(startX, startY, stepX, stepY)`
/// in 1/16-pel units.
fn scale_mv_for_plane(
    plane: usize,
    x: usize,
    y: usize,
    clamped_mv: Mv,
    ref_width: u32,
    ref_height: u32,
    p: &InterPredictParams,
) -> (i64, i64, i64, i64) {
    if ref_width == p.frame_width && ref_height == p.frame_height {
        return (
            ((x as i64) << SUBPEL_BITS) + clamped_mv[1] as i64,
            ((y as i64) << SUBPEL_BITS) + clamped_mv[0] as i64,
            16,
            16,
        );
    }
    let x_scale = ((ref_width as i64) << REF_SCALE_SHIFT) / p.frame_width as i64;
    let y_scale = ((ref_height as i64) << REF_SCALE_SHIFT) / p.frame_height as i64;
    let base_x = ((x as i64) * x_scale) >> REF_SCALE_SHIFT;
    let base_y = ((y as i64) * y_scale) >> REF_SCALE_SHIFT;
    let luma_x = if plane > 0 {
        (x as u32) << p.subsampling_x
    } else {
        x as u32
    } as i64;
    let luma_y = if plane > 0 {
        (y as u32) << p.subsampling_y
    } else {
        y as u32
    } as i64;
    let frac_x = ((16 * luma_x * x_scale) >> REF_SCALE_SHIFT) & SUBPEL_MASK as i64;
    let frac_y = ((16 * luma_y * y_scale) >> REF_SCALE_SHIFT) & SUBPEL_MASK as i64;
    let d_x = ((clamped_mv[1] as i64 * x_scale) >> REF_SCALE_SHIFT) + frac_x;
    let d_y = ((clamped_mv[0] as i64 * y_scale) >> REF_SCALE_SHIFT) + frac_y;
    let step_x = (16 * x_scale) >> REF_SCALE_SHIFT;
    let step_y = (16 * y_scale) >> REF_SCALE_SHIFT;
    let start_x = (base_x << SUBPEL_BITS) + d_x;
    let start_y = (base_y << SUBPEL_BITS) + d_y;
    (start_x, start_y, step_x, step_y)
}

#[allow(clippy::too_many_arguments)]
fn block_inter_predict_scalar_ref(
    ref_data: RefSlice<'_>,
    ref_w: usize,
    ref_stride: usize,
    ref_h: usize,
    x: i64,
    y: i64,
    x_step: i64,
    y_step: i64,
    w: usize,
    h: usize,
    interp_filter: u8,
    bit_depth: u8,
    pred: &mut [i32],
) {
    debug_assert!(w <= MAX_BLOCK_DIM && h <= MAX_BLOCK_DIM);
    debug_assert_eq!(pred.len(), w * h);
    let last_x = ref_w as i64 - 1;
    let last_y = ref_h as i64 - 1;
    let intermediate_height = (((h as i64 - 1) * y_step + 15) >> 4) + 8;
    debug_assert!(intermediate_height as usize <= MAX_INTERMEDIATE_HEIGHT);

    let filters = &SUBPEL_FILTERS[interp_filter as usize];
    let px = |row_off: usize, col: usize| ref_data.get_i32(row_off + col);

    // Fast unscaled scalar paths: avoid running zero taps when fx == 0 or fy == 0, and avoid
    // 34 KB stack zeroing on every small block.
    if x_step == 16 && y_step == 16 {
        let fx = (x & 15) as usize;
        let fy = (y & 15) as usize;
        let x0 = x >> 4;
        let y0 = y >> 4;
        let max_val = (1i32 << bit_depth) - 1;

        if fx == 0 && fy == 0 {
            let in_x = x0 >= 0 && x0 + w as i64 - 1 <= last_x;
            for r in 0..h {
                let row_off = (y0 + r as i64).clamp(0, last_y) as usize * ref_stride;
                for c in 0..w {
                    let sx = if in_x {
                        (x0 as usize) + c
                    } else {
                        (x0 + c as i64).clamp(0, last_x) as usize
                    };
                    pred[r * w + c] = px(row_off, sx);
                }
            }
            return;
        }
        if fy == 0 {
            let hcoeffs = &filters[fx];
            let in_x = x0 - 3 >= 0 && x0 + w as i64 + 3 <= last_x;
            for r in 0..h {
                let row_off = (y0 + r as i64).clamp(0, last_y) as usize * ref_stride;
                for c in 0..w {
                    let mut s = 0i32;
                    for (t, &coeff) in hcoeffs.iter().enumerate() {
                        let sx = if in_x {
                            (x0 - 3) as usize + c + t
                        } else {
                            (x0 + c as i64 + t as i64 - 3).clamp(0, last_x) as usize
                        };
                        s += coeff * px(row_off, sx);
                    }
                    pred[r * w + c] = clip3(0, max_val, round2(s, 7));
                }
            }
            return;
        }
        if fx == 0
            && x0 >= 0
            && x0 + w as i64 - 1 <= last_x
            && y0 - 3 >= 0
            && y0 + h as i64 + 3 <= last_y
        {
            let vcoeffs = &filters[fy];
            let sx = x0 as usize;
            for r in 0..h {
                let ybase = (y0 - 3) as usize + r;
                for c in 0..w {
                    let mut s = 0i32;
                    for (t, &coeff) in vcoeffs.iter().enumerate() {
                        s += coeff * px((ybase + t) * ref_stride, sx + c);
                    }
                    pred[r * w + c] = clip3(0, max_val, round2(s, 7));
                }
            }
            return;
        }
    }

    // General scalar path (scaled or 2D unscaled).
    let mut intermediate = [0i32; MAX_INTERMEDIATE_HEIGHT * MAX_BLOCK_DIM];
    for r in 0..intermediate_height {
        let row_off = ((y >> 4) + r - 3).clamp(0, last_y) as usize * ref_stride;
        for c in 0..w {
            let p = x + x_step * (c as i64);
            let coeffs = &filters[(p & 15) as usize];
            let mut s = 0i32;
            for (t, &coeff) in coeffs.iter().enumerate() {
                let ref_x = ((p >> 4) + t as i64 - 3).clamp(0, last_x) as usize;
                s += coeff * px(row_off, ref_x);
            }
            intermediate[(r as usize) * w + c] = clip1(round2(s, 7), bit_depth);
        }
    }

    for r in 0..h {
        let p = (y & 15) + y_step * (r as i64);
        let coeffs = &filters[(p & 15) as usize];
        let base_row = (p >> 4) as usize;
        for c in 0..w {
            let mut s = 0i32;
            for (t, &coeff) in coeffs.iter().enumerate() {
                s += coeff * intermediate[(base_row + t) * w + c];
            }
            pred[r * w + c] = clip1(round2(s, 7), bit_depth);
        }
    }
}

/// Writes one reference list's inter prediction directly into `dst` (or averages it with `dst`
/// when `compound` is true). Dispatches to direct-to-u16 AVX2 kernels on `x86_64`.
#[allow(clippy::too_many_arguments)]
fn block_inter_predict_into_plane(
    ref_plane: &RefPlane,
    dst: &mut Plane,
    dst_x: usize,
    dst_y: usize,
    x: i64,
    y: i64,
    x_step: i64,
    y_step: i64,
    w: usize,
    h: usize,
    interp_filter: u8,
    bit_depth: u8,
    compound: bool,
    scratch: &mut [u16],
) {
    debug_assert!(w <= MAX_BLOCK_DIM && h <= MAX_BLOCK_DIM);
    debug_assert!(dst_x >= dst.x0 && dst_x - dst.x0 + w <= dst.width);
    debug_assert!(dst_y + h <= dst.height);
    let ref_slice = ref_plane.as_slice_ref();

    if let RefSlice::U8(src) = ref_slice {
        let (sx, sy) = (x >> 4, y >> 4);
        if x_step == 16
            && y_step == 16
            && (x | y) & 15 == 0
            && sx >= 0
            && sy >= 0
            && sx as usize + w <= ref_plane.width
            && sy as usize + h <= ref_plane.height
        {
            let stride = ref_plane.stride();
            let start = sy as usize * stride + sx as usize;
            if dst.copy_byte_block([dst_x, dst_y], [w, h], &src[start..], stride, compound) {
                return;
            }
        }
    }

    let _ = &scratch;
    #[cfg(all(target_arch = "x86_64", target_feature = "sse2"))]
    if (w == 4 || w.is_multiple_of(8))
        && crate::simd::avx2_enabled()
        && dst.with_word_block(
            [dst_x, dst_y],
            [w, h],
            compound,
            scratch,
            |base, stride, col, row| {
                try_inter_predict_avx2(
                    ref_slice,
                    ref_plane.width,
                    ref_plane.stride(),
                    ref_plane.height,
                    base,
                    stride,
                    col,
                    row,
                    x,
                    y,
                    x_step,
                    y_step,
                    w,
                    h,
                    interp_filter,
                    bit_depth,
                    compound,
                )
            },
        )
    {
        return;
    }

    // Scalar fallback (non-x86_64 or `VP9DEC_NO_SIMD=1`).
    let mut pred = [0i32; MAX_BLOCK_DIM * MAX_BLOCK_DIM];
    block_inter_predict_scalar_ref(
        ref_slice,
        ref_plane.width,
        ref_plane.stride(),
        ref_plane.height,
        x,
        y,
        x_step,
        y_step,
        w,
        h,
        interp_filter,
        bit_depth,
        &mut pred[..w * h],
    );
    for row in 0..h {
        let src = &pred[row * w..][..w];
        dst.update_row(dst_x, dst_y + row, w, |i, sample| {
            if compound {
                round2(i32::from(sample) + src[i], 1) as u16
            } else {
                src[i] as u16
            }
        });
    }
}

#[cfg(all(target_arch = "x86_64", target_feature = "sse2"))]
#[allow(clippy::too_many_arguments)]
fn try_inter_predict_avx2(
    ref_data: RefSlice<'_>,
    ref_width: usize,
    ref_stride: usize,
    ref_height: usize,
    dst_ptr: *mut u16,
    dst_stride: usize,
    local_x: usize,
    dst_y: usize,
    x: i64,
    y: i64,
    x_step: i64,
    y_step: i64,
    w: usize,
    h: usize,
    interp_filter: u8,
    bit_depth: u8,
    compound: bool,
) -> bool {
    let max_val = (1i32 << bit_depth) - 1;
    let last_x = ref_width as i64 - 1;
    let last_y = ref_height as i64 - 1;

    if x_step == 16 && y_step == 16 {
        let (x0, y0) = (x >> 4, y >> 4);
        let (fx, fy) = ((x & 15) as usize, (y & 15) as usize);
        let x_ok = if fx == 0 {
            x0 >= 0 && x0 + (w as i64 - 1) <= last_x
        } else {
            x0 - 3 >= 0 && x0 + (w as i64 - 1) + 4 <= last_x
        };
        let y_ok = if fy == 0 {
            y0 >= 0 && y0 + (h as i64 - 1) <= last_y
        } else {
            y0 - 3 >= 0 && y0 + (h as i64 - 1) + 4 <= last_y
        };
        if x_ok && y_ok {
            // SAFETY: `avx2_enabled()` confirmed by caller; `x_ok && y_ok` proves every source
            // pixel read lies in `ref_data`, and `dst` bounds hold within this strip.
            unsafe {
                match ref_data {
                    RefSlice::U8(s) => crate::simd::block_inter_predict_u16_avx2::<u8>(
                        s,
                        ref_stride,
                        dst_ptr,
                        dst_stride,
                        local_x,
                        dst_y,
                        x0,
                        y0,
                        fx,
                        fy,
                        w,
                        h,
                        interp_filter,
                        max_val,
                        compound,
                    ),
                    RefSlice::U16(s) => crate::simd::block_inter_predict_u16_avx2::<u16>(
                        s,
                        ref_stride,
                        dst_ptr,
                        dst_stride,
                        local_x,
                        dst_y,
                        x0,
                        y0,
                        fx,
                        fy,
                        w,
                        h,
                        interp_filter,
                        max_val,
                        compound,
                    ),
                }
            }
            return true;
        }
    }

    let intermediate_height = (((h as i64 - 1) * y_step + 15) >> 4) + 8;
    let span = (((x & 15) + x_step * (w as i64 - 1)) >> 4) + 8;
    if intermediate_height as usize <= MAX_INTERMEDIATE_HEIGHT
        && span as usize <= MAX_INTERMEDIATE_HEIGHT
    {
        // SAFETY: `avx2_enabled()` confirmed; `ref_data` has `ref_stride * ref_height` samples,
        // every source read is clamped inside `ref_width x ref_height`, scratch bounds were checked,
        // and `dst` bounds hold within this strip.
        unsafe {
            crate::simd::block_inter_predict_scaled_u16_avx2(
                ref_data,
                ref_width,
                ref_stride,
                ref_height,
                dst_ptr,
                dst_stride,
                local_x,
                dst_y,
                x,
                y,
                x_step,
                y_step,
                w,
                h,
                intermediate_height as usize,
                interp_filter,
                max_val,
                compound,
            );
        }
        return true;
    }
    false
}

/// `predict_inter()` (spec §8.5.2 "Inter prediction process").
#[allow(clippy::too_many_arguments)]
pub fn predict_inter(
    dst: &mut Plane,
    plane: usize,
    x: usize,
    y: usize,
    w: usize,
    h: usize,
    block_idx: usize,
    p: &InterPredictParams,
    scratch: &mut [u16],
) {
    let is_compound = p.ref_frame[1] > INTRA_FRAME;
    let n_refs = 1 + is_compound as usize;

    for ref_list in 0..n_refs {
        let mv = select_mv(plane, ref_list, block_idx, p);
        let clamped = clamp_mv_for_plane(plane, mv, p);
        let view = p.refs[ref_list].expect("reference frame slot is missing (DPB not initialized)");
        let (start_x, start_y, step_x, step_y) =
            scale_mv_for_plane(plane, x, y, clamped, view.width, view.height, p);
        block_inter_predict_into_plane(
            view.plane,
            dst,
            x,
            y,
            start_x,
            start_y,
            step_x,
            step_y,
            w,
            h,
            p.interp_filter,
            p.bit_depth,
            ref_list == 1,
            scratch,
        );
    }
}
