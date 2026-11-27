//! HEVC intra prediction (spec 8.4.4).
//!
//! Phase 2c-5 scope: reference sample derivation/substitution + PLANAR (mode
//! 0) + DC (mode 1) prediction. Mirrors FFmpeg `pred_template.c`'s
//! `intra_pred`, `pred_planar`, and `pred_dc` for the simplified case our
//! fixture exercises (no neighbors → reference samples filled with the
//! default value, no constrained_intra_pred). Angular modes (2..34) and
//! reference sample smoothing for non-trivial neighbor configurations are
//! deferred until later phases need them.
//!
//! Layout convention: `top` and `left` are slices of length `2 * size + 1`,
//! where index 0 corresponds to the corner sample `p[-1][-1]` (top-left) and
//! indices 1..=size cover the immediate neighbors. The "extended" right and
//! bottom samples occupy indices size+1..=2*size.

use crate::pixel::Pixel;

/// Default reference sample value when nothing is available
/// (`1 << (BitDepth - 1)` per HEVC spec 8.4.4.2.2).
pub fn default_ref_sample(bit_depth: u8) -> u8 {
    1u8 << (bit_depth - 1)
}

/// Reference sample availability flags for a TU. For our Phase 2c-5 fixture
/// (single CTU, no neighbors), all of these are `false`.
#[derive(Debug, Clone, Copy, Default)]
pub struct ReferenceAvailability {
    pub up_left: bool,
    pub up: bool,
    pub up_right: bool,
    pub left: bool,
    pub bottom_left: bool,
}

/// Reference samples per side for the largest intra block (32x32): the
/// corner plus 2 * 32 samples. Fixed-size, so building references for a
/// transform block does not allocate; only the first `2 * size + 1`
/// entries are meaningful.
pub const MAX_REF_LEN: usize = 2 * 32 + 1;
/// A `top` or `left` reference array (see `MAX_REF_LEN`).
pub type RefSamples<P> = [P; MAX_REF_LEN];
/// Extended reference row of angular prediction: `3 * size + 4` at 32x32.
const ANGULAR_REF_LEN: usize = 3 * 32 + 4;

/// Build the `top` and `left` reference sample arrays for a TU at `(x0, y0)`
/// of size `size = 1 << log2_size`. Returns `(top, left)` each of length
/// `2 * size + 1`, where index 0 is the corner.
///
/// `plane` is the picture plane (luma or chroma) the TU lives in, indexed
/// in raster order with row stride `stride`. `pic_w` / `pic_h` are the
/// dimensions of `plane`.
///
/// `avail` reflects which directional neighbors have been **decoded already**
/// (per spec 8.4.4.2.2). For each unavailable group, the substitution rule
/// fills it from the nearest available sample.
#[allow(clippy::too_many_arguments)]
pub fn build_reference_samples<P: Pixel, S: std::ops::Index<usize, Output = P> + ?Sized>(
    plane: &S,
    stride: usize,
    pic_w: usize,
    pic_h: usize,
    x0: usize,
    y0: usize,
    log2_size: u8,
    bit_depth: u8,
    avail: ReferenceAvailability,
    intra_only: Option<(&[bool], &[bool])>,
) -> (RefSamples<P>, RefSamples<P>) {
    let size = 1usize << log2_size;
    let mut top = [P::zero(); MAX_REF_LEN];
    let mut left = [P::zero(); MAX_REF_LEN];

    // Track per-sample availability so we can substitute the unavailable
    // ones afterwards. `top_avail[i]` covers `top[i]` (i = 0 is the corner).
    let mut top_avail = [false; MAX_REF_LEN];
    let mut left_avail = [false; MAX_REF_LEN];

    // Corner (p[-1][-1]).
    if avail.up_left && x0 > 0 && y0 > 0 {
        let v = plane[(y0 - 1) * stride + (x0 - 1)];
        top[0] = v;
        left[0] = v;
        top_avail[0] = true;
        left_avail[0] = true;
    }

    // Top row (p[0..size-1][-1]).
    if avail.up && y0 > 0 {
        for i in 0..size {
            top[1 + i] = plane[(y0 - 1) * stride + (x0 + i)];
            top_avail[1 + i] = true;
        }
    }

    // Top-right extension (p[size..2*size-1][-1]).
    if avail.up_right && y0 > 0 {
        let avail_x = pic_w.saturating_sub(x0 + size);
        let count = avail_x.min(size);
        for i in 0..count {
            top[1 + size + i] = plane[(y0 - 1) * stride + (x0 + size + i)];
            top_avail[1 + size + i] = true;
        }
    }

    // Left column (p[-1][0..size-1]).
    if avail.left && x0 > 0 {
        for i in 0..size {
            left[1 + i] = plane[(y0 + i) * stride + (x0 - 1)];
            left_avail[1 + i] = true;
        }
    }

    // Bottom-left extension (p[-1][size..2*size-1]).
    if avail.bottom_left && x0 > 0 {
        let avail_y = pic_h.saturating_sub(y0 + size);
        let count = avail_y.min(size);
        for i in 0..count {
            left[1 + size + i] = plane[(y0 + size + i) * stride + (x0 - 1)];
            left_avail[1 + size + i] = true;
        }
    }

    // constrained_intra_pred: samples of non-intra blocks are unavailable.
    if let Some((top_intra, left_intra)) = intra_only {
        for (a, &ok) in top_avail.iter_mut().zip(top_intra) {
            *a &= ok;
        }
        for (a, &ok) in left_avail.iter_mut().zip(left_intra) {
            *a &= ok;
        }
    }

    // ---- Substitution per spec 8.4.4.2.2 ----
    // If everything is unavailable, fill with the default value.
    let any_top_avail = top_avail.iter().any(|&a| a);
    let any_left_avail = left_avail.iter().any(|&a| a);
    if !any_top_avail && !any_left_avail {
        let fill = P::from_i32_clamped(crate::pixel::default_ref_sample(bit_depth), bit_depth);
        for v in top.iter_mut() {
            *v = fill;
        }
        for v in left.iter_mut() {
            *v = fill;
        }
        return (top, left);
    }

    // Concatenate top and left into a single 4*size+1 array, scan from the
    // bottom-left corner upward to find the first available sample, then
    // fill leftward (along the left column upward) and rightward (along the
    // top row) using the nearest available value. This matches the spec's
    // unified substitution loop.
    //
    // Index layout in the unified array:
    //   [0..2*size]       = left[2*size..0]  (bottom-left to top-left, reversed)
    //   [2*size]          = corner
    //   [2*size+1..4*size+1] = top[1..2*size+1]
    let total = 4 * size + 1;
    let mut ref_array = [P::zero(); 2 * MAX_REF_LEN - 1];
    let mut ref_avail = [false; 2 * MAX_REF_LEN - 1];
    // left, reversed: index 0 = left[2*size], index 2*size-1 = left[1]
    for i in 0..2 * size {
        ref_array[i] = left[2 * size - i];
        ref_avail[i] = left_avail[2 * size - i];
    }
    // corner
    ref_array[2 * size] = top[0];
    ref_avail[2 * size] = top_avail[0];
    // top: index 2*size+1..4*size+1 = top[1..2*size+1]
    for i in 0..2 * size {
        ref_array[2 * size + 1 + i] = top[1 + i];
        ref_avail[2 * size + 1 + i] = top_avail[1 + i];
    }

    // Find the first available sample (lowest index).
    let first = ref_avail.iter().position(|&a| a).unwrap();
    let first_val = ref_array[first];
    // Fill everything before `first` with first_val.
    for v in ref_array.iter_mut().take(first) {
        *v = first_val;
    }
    for a in ref_avail.iter_mut().take(first) {
        *a = true;
    }
    // Fill forward: any unavailable sample takes the value of the previous one.
    for i in (first + 1)..total {
        if !ref_avail[i] {
            ref_array[i] = ref_array[i - 1];
            ref_avail[i] = true;
        }
    }

    // Unpack back into top/left.
    for i in 0..2 * size {
        left[2 * size - i] = ref_array[i];
    }
    top[0] = ref_array[2 * size];
    left[0] = ref_array[2 * size];
    for i in 0..2 * size {
        top[1 + i] = ref_array[2 * size + 1 + i];
    }

    (top, left)
}

/// PLANAR intra prediction (spec 8.4.4.2.5 / FFmpeg `pred_planar`).
///
/// `top` is indexed `[0..=2*size]` with `top[0] = p[-1][-1]` (corner) and
/// `top[i+1] = p[i][-1]` for `i = 0..2*size-1`. Same for `left` along the
/// vertical axis.
///
/// `dst` is the destination buffer of `size * size` samples in row-major
/// order with row stride `dst_stride`.
pub fn predict_planar<P: Pixel>(
    dst: &mut [P],
    dst_stride: usize,
    top: &[P],
    left: &[P],
    log2_size: u8,
    bit_depth: u8,
) {
    let size = 1usize << log2_size;
    let top_p = &top[1..];
    let left_p = &left[1..];

    let shift = log2_size as u32 + 1;
    for y in 0..size {
        for x in 0..size {
            let pred = (size - 1 - x) as i32 * left_p[y].to_i32()
                + (x + 1) as i32 * top_p[size].to_i32()
                + (size - 1 - y) as i32 * top_p[x].to_i32()
                + (y + 1) as i32 * left_p[size].to_i32()
                + size as i32;
            dst[y * dst_stride + x] = P::from_i32_clamped(pred >> shift, bit_depth);
        }
    }
}

/// DC intra prediction (spec 8.4.4.2.4 / FFmpeg `pred_dc`).
///
/// `apply_luma_filter` is true for luma TUs with `size < 32`, in which case
/// the top row, left column, and top-left corner get a simple smoothing
/// filter applied (spec eq. 8-23).
pub fn predict_dc<P: Pixel>(
    dst: &mut [P],
    dst_stride: usize,
    top: &[P],
    left: &[P],
    log2_size: u8,
    apply_luma_filter: bool,
    bit_depth: u8,
) {
    let size = 1usize << log2_size;
    let top_p = &top[1..];
    let left_p = &left[1..];
    let mut dc_sum: i32 = size as i32;
    for i in 0..size {
        dc_sum += left_p[i].to_i32() + top_p[i].to_i32();
    }
    let dc_val = dc_sum >> (log2_size as u32 + 1);
    let dc = P::from_i32_clamped(dc_val, bit_depth);

    for y in 0..size {
        for x in 0..size {
            dst[y * dst_stride + x] = dc;
        }
    }

    if apply_luma_filter && size < 32 {
        // Top-left corner: (left[0] + 2*dc + top[0] + 2) >> 2
        dst[0] = P::from_i32_clamped(
            ((left_p[0].to_i32()) + 2 * dc_val + (top_p[0].to_i32()) + 2) >> 2,
            bit_depth,
        );
        // Top row x = 1..size: (top[x] + 3*dc + 2) >> 2
        for x in 1..size {
            dst[x] = P::from_i32_clamped(((top_p[x].to_i32()) + 3 * dc_val + 2) >> 2, bit_depth);
        }
        // Left column y = 1..size: (left[y] + 3*dc + 2) >> 2
        for y in 1..size {
            dst[y * dst_stride] =
                P::from_i32_clamped(((left_p[y].to_i32()) + 3 * dc_val + 2) >> 2, bit_depth);
        }
    }
}

/// Angular intra prediction (modes 2..34) (spec 8.4.4.2.6 / FFmpeg
/// `pred_angular`).
///
/// `top` / `left` use the same layout as `predict_planar`: index 0 = corner
/// sample p[-1][-1], indices 1..=2*size = neighbors / extensions.
///
/// `c_idx` is 0 for luma (needed for the boundary smoothing of pure
/// horizontal/vertical modes).
#[allow(clippy::too_many_arguments)]
pub fn predict_angular<P: Pixel>(
    dst: &mut [P],
    dst_stride: usize,
    top: &[P],
    left: &[P],
    log2_size: u8,
    mode: u8,
    c_idx: u8,
    bit_depth: u8,
) {
    debug_assert!((2..=34).contains(&mode));

    let size = 1usize << log2_size;

    // Angle tables — indexed by (mode - 2).
    static INTRA_PRED_ANGLE: [i32; 33] = [
        32, 26, 21, 17, 13, 9, 5, 2, 0, -2, -5, -9, -13, -17, -21, -26, -32, -26, -21, -17, -13,
        -9, -5, -2, 0, 2, 5, 9, 13, 17, 21, 26, 32,
    ];
    // Inverse angle table — indexed by (mode - 11) for modes 11..25 (the 15
    // modes with negative angles).
    static INV_ANGLE: [i32; 15] = [
        -4096, -1638, -910, -630, -482, -390, -315, -256, -315, -390, -482, -630, -910, -1638,
        -4096,
    ];

    let angle = INTRA_PRED_ANGLE[(mode - 2) as usize];
    let last = ((size as i32) * angle) >> 5;

    let top_p = &top[1..];
    let left_p = &left[1..];
    let corner = top[0];

    if mode >= 18 {
        // ---- Horizontal-like: iterate y (rows), index into top ref ----
        let mut ref_buf = [P::zero(); ANGULAR_REF_LEN];
        let ref_origin = size;

        ref_buf[ref_origin] = corner;
        for i in 0..2 * size {
            ref_buf[ref_origin + 1 + i] = top_p[i];
        }

        if angle < 0 && last < -1 {
            for x in last..=-1 {
                let left_idx = -1 + ((x * INV_ANGLE[(mode - 11) as usize] + 128) >> 8);
                ref_buf[(ref_origin as i32 + x) as usize] = left_p[left_idx as usize];
            }
        }

        for y in 0..size {
            let idx = (((y + 1) as i32) * angle) >> 5;
            let fact = (((y + 1) as i32) * angle) & 31;
            if fact != 0 {
                for x in 0..size {
                    let ri = (ref_origin as i32 + x as i32 + idx + 1) as usize;
                    dst[y * dst_stride + x] = P::from_i32_clamped(
                        ((32 - fact) * ref_buf[ri].to_i32() + fact * ref_buf[ri + 1].to_i32() + 16)
                            >> 5,
                        bit_depth,
                    );
                }
            } else {
                for x in 0..size {
                    let ri = (ref_origin as i32 + x as i32 + idx + 1) as usize;
                    dst[y * dst_stride + x] = ref_buf[ri];
                }
            }
        }

        // Mode 26 (pure vertical) luma boundary filter.
        if mode == 26 && c_idx == 0 && size < 32 {
            for y in 0..size {
                let val = top_p[0].to_i32() + ((left_p[y].to_i32() - corner.to_i32()) >> 1);
                dst[y * dst_stride] = P::from_i32_clamped(val, bit_depth);
            }
        }
    } else {
        // ---- Vertical-like (modes 2..17): iterate x (cols), index into left ref ----
        let mut ref_buf = [P::zero(); ANGULAR_REF_LEN];
        let ref_origin = size;

        ref_buf[ref_origin] = corner;
        for i in 0..2 * size {
            ref_buf[ref_origin + 1 + i] = left_p[i];
        }

        if angle < 0 && last < -1 {
            for x in last..=-1 {
                let top_idx = -1 + ((x * INV_ANGLE[(mode - 11) as usize] + 128) >> 8);
                ref_buf[(ref_origin as i32 + x) as usize] = top_p[top_idx as usize];
            }
        }

        for x in 0..size {
            let idx = (((x + 1) as i32) * angle) >> 5;
            let fact = (((x + 1) as i32) * angle) & 31;
            if fact != 0 {
                for y in 0..size {
                    let ri = (ref_origin as i32 + y as i32 + idx + 1) as usize;
                    dst[y * dst_stride + x] = P::from_i32_clamped(
                        ((32 - fact) * ref_buf[ri].to_i32() + fact * ref_buf[ri + 1].to_i32() + 16)
                            >> 5,
                        bit_depth,
                    );
                }
            } else {
                for y in 0..size {
                    let ri = (ref_origin as i32 + y as i32 + idx + 1) as usize;
                    dst[y * dst_stride + x] = ref_buf[ri];
                }
            }
        }

        // Mode 10 (pure horizontal) luma boundary filter.
        if mode == 10 && c_idx == 0 && size < 32 {
            for x in 0..size {
                let val = left_p[0].to_i32() + ((top_p[x].to_i32() - corner.to_i32()) >> 1);
                dst[x] = P::from_i32_clamped(val, bit_depth);
            }
        }
    }
}

/// Reference sample filtering for angular modes (spec 8.4.4.2.3 /
/// FFmpeg `intra_pred` lines 291-329).
///
/// Applies the [1,2,1]/4 smoothing filter (or strong intra smoothing for
/// 32x32 luma) to `top` and `left` **in place** when the mode / size /
/// distance criteria are met. Must be called BEFORE the prediction function
/// for modes 2..34. Not needed for PLANAR (mode 0) or DC (mode 1).
///
/// `c_idx` = 0 for luma. `strong_intra_smoothing_enabled` comes from SPS.
/// `chroma_format_idc` comes from SPS (1 = 4:2:0, 3 = 4:4:4).
///
/// Note: intra_smoothing_disabled is always false for our supported streams
/// (it's not even exposed in the SPS we parse).
#[allow(clippy::too_many_arguments)]
pub fn filter_reference_samples<P: Pixel>(
    top: &mut RefSamples<P>,
    left: &mut RefSamples<P>,
    log2_size: u8,
    mode: u8,
    strong_intra_smoothing_enabled: bool,
    c_idx: u8,
    chroma_format_idc: u32,
    bit_depth: u8,
) {
    let size = 1usize << log2_size;

    // Only filter for non-DC modes and sizes > 4.
    if mode == 1 || size == 4 {
        return;
    }

    // Only filter luma (c_idx == 0) or 4:4:4 chroma (chroma_format_idc == 3).
    if c_idx != 0 && chroma_format_idc != 3 {
        return;
    }

    // Distance threshold check.
    static INTRA_HOR_VER_DIST_THRESH: [i32; 3] = [7, 1, 0];
    let thresh_idx = (log2_size as usize).saturating_sub(3);
    if thresh_idx >= INTRA_HOR_VER_DIST_THRESH.len() {
        return; // shouldn't happen for valid log2_size
    }
    let min_dist_vert_hor = ((mode as i32) - 26).abs().min(((mode as i32) - 10).abs());
    if min_dist_vert_hor <= INTRA_HOR_VER_DIST_THRESH[thresh_idx] {
        return;
    }

    // Strong intra smoothing for 32x32 luma.
    if strong_intra_smoothing_enabled && c_idx == 0 && log2_size == 5 {
        // threshold = 1 << (BitDepth - 5)
        let threshold = 1i32 << (bit_depth as i32 - 5);
        let top_smooth =
            (top[0].to_i32() + top[2 * size].to_i32() - 2 * top[size].to_i32()).abs() < threshold;
        let left_smooth = (left[0].to_i32() + left[2 * size].to_i32() - 2 * left[size].to_i32())
            .abs()
            < threshold;
        if top_smooth && left_smooth {
            // Strong smoothing: linear interpolation between corner and edge.
            let mut filtered_top = [P::zero(); MAX_REF_LEN];
            filtered_top[0] = top[0]; // corner
            filtered_top[2 * size] = top[2 * size]; // far end
            for i in 0..(2 * size - 1) {
                filtered_top[i + 1] = P::from_i32_clamped(
                    ((64 - (i + 1) as i32) * top[0].to_i32()
                        + (i + 1) as i32 * top[2 * size].to_i32()
                        + 32)
                        >> 6,
                    bit_depth,
                );
            }
            let left_corner = left[0];
            let left_end = left[2 * size];
            for i in 0..(2 * size - 1) {
                left[i + 1] = P::from_i32_clamped(
                    ((64 - (i + 1) as i32) * left_corner.to_i32()
                        + (i + 1) as i32 * left_end.to_i32()
                        + 32)
                        >> 6,
                    bit_depth,
                );
            }
            *top = filtered_top;
            return;
        }
    }

    // Normal [1,2,1]/4 smoothing.
    let mut filtered_top = [P::zero(); MAX_REF_LEN];
    let mut filtered_left = [P::zero(); MAX_REF_LEN];

    // Last element stays unchanged.
    filtered_top[2 * size] = top[2 * size];
    filtered_left[2 * size] = left[2 * size];

    for k in (1..2 * size).rev() {
        filtered_top[k] = P::from_i32_clamped(
            (top[k + 1].to_i32() + 2 * top[k].to_i32() + top[k - 1].to_i32() + 2) >> 2,
            bit_depth,
        );
        filtered_left[k] = P::from_i32_clamped(
            (left[k + 1].to_i32() + 2 * left[k].to_i32() + left[k - 1].to_i32() + 2) >> 2,
            bit_depth,
        );
    }

    // Corner: (left[1] + 2*corner + top[1] + 2) >> 2
    let new_corner = P::from_i32_clamped(
        (left[1].to_i32() + 2 * left[0].to_i32() + top[1].to_i32() + 2) >> 2,
        bit_depth,
    );
    filtered_top[0] = new_corner;
    filtered_left[0] = new_corner;

    *top = filtered_top;
    *left = filtered_left;
}

/// Add a residual block to a prediction in place, clipping to the valid
/// pixel range. `residual` and `dst` are the same shape (`size * size`);
/// `dst_stride` is the row stride of `dst`. Used by callers to combine the
/// intra prediction with the inverse-transformed residual.
pub fn add_residual<P: Pixel>(
    dst: &mut [P],
    dst_stride: usize,
    residual: &[i16],
    log2_size: u8,
    bit_depth: u8,
) {
    let size = 1usize << log2_size;
    for y in 0..size {
        for x in 0..size {
            let pixel = dst[y * dst_stride + x].to_i32() + residual[y * size + x] as i32;
            dst[y * dst_stride + x] = P::from_i32_clamped(pixel, bit_depth);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// With no available neighbors, reference samples must be filled with
    /// the bit-depth midpoint (128 for 8-bit).
    #[test]
    fn test_no_neighbors_fills_with_midpoint() {
        let plane = vec![0u8; 0];
        let avail = ReferenceAvailability::default();
        let (top, left) = build_reference_samples(&plane, 0, 0, 0, 0, 0, 4, 8, avail, None);
        // Fixed-size arrays; the first 2 * 16 + 1 entries are meaningful.
        assert_eq!(top.len(), MAX_REF_LEN);
        assert_eq!(left.len(), MAX_REF_LEN);
        assert!(top.iter().all(|&p| p == 128));
        assert!(left.iter().all(|&p| p == 128));
    }

    /// With only the left column available (e.g., CTU 2 of a 32×32 picture
    /// with --ctu 16, where CTU 1 has been decoded), the substitution rule
    /// extends the bottom-most available left sample upward into the
    /// corner and across the top row.
    #[test]
    fn test_left_only_substitution() {
        // 32-wide picture with CTU 1 (cols 0..15) decoded as all 0x7E.
        let stride = 32;
        let plane: Vec<u8> = (0..32 * 16).map(|_| 0x7E).collect();
        let avail = ReferenceAvailability {
            up_left: false,
            up: false,
            up_right: false,
            left: true,
            bottom_left: false,
        };
        // Building refs for CTU 2 at (16, 0), size 16x16. The left column
        // pulls from plane[(0..15) * stride + 15], all 0x7E.
        let (top, left) = build_reference_samples(&plane, stride, 32, 16, 16, 0, 4, 8, avail, None);
        // The 16 immediate left neighbors should be 0x7E.
        for i in 0..16 {
            assert_eq!(left[1 + i], 0x7E, "left[{}]", 1 + i);
        }
        // Substitution: corner and top row inherit the topmost available
        // left value (which is 0x7E since the entire left column is 0x7E).
        assert_eq!(top[0], 0x7E, "corner");
        for i in 0..32 {
            assert_eq!(top[1 + i], 0x7E, "top[{}]", 1 + i);
        }
        // Bottom-left extension: filled by forward propagation from the
        // last available left sample (still 0x7E).
        for i in 16..32 {
            assert_eq!(left[1 + i], 0x7E, "left ext[{}]", 1 + i);
        }
    }

    /// PLANAR with all-128 reference samples must produce all-128 prediction.
    #[test]
    fn test_planar_uniform_neighbors() {
        let top = vec![128u8; 33];
        let left = vec![128u8; 33];
        let mut dst = vec![0u8; 256];
        predict_planar(&mut dst, 16, &top, &left, 4, 8);
        assert!(dst.iter().all(|&p| p == 128), "first row: {:?}", &dst[..16]);
    }

    /// DC with all-128 reference samples must produce all-128 prediction —
    /// the corner-and-edge filter on a uniform input is also a no-op.
    #[test]
    fn test_dc_uniform_neighbors_with_filter() {
        let top = vec![128u8; 33];
        let left = vec![128u8; 33];
        let mut dst = vec![0u8; 256];
        predict_dc(&mut dst, 16, &top, &left, 4, true, 8);
        assert!(dst.iter().all(|&p| p == 128));
    }

    /// `add_residual` clamps to [0, 255] and adds element-by-element.
    #[test]
    fn test_add_residual_basic() {
        let mut dst = vec![128u8; 16];
        let mut residual = vec![0i16; 16];
        residual[0] = 10;
        residual[1] = -200;
        residual[2] = 200;
        add_residual(&mut dst, 4, &residual, 2, 8);
        assert_eq!(dst[0], 138);
        assert_eq!(dst[1], 0); // clamped from -72
        assert_eq!(dst[2], 255); // clamped from 328
        assert_eq!(dst[3], 128);
    }

    /// **End-to-end check for the fixture's luma block**: PLANAR with no
    /// neighbors gives all-128, plus the IDCT residual of all -2, equals
    /// all 0x7E (the reference YUV).
    #[test]
    fn test_fixture_luma_reconstruction() {
        let plane = vec![0u8; 0];
        let avail = ReferenceAvailability::default();
        let (top, left) = build_reference_samples(&plane, 0, 0, 0, 0, 0, 4, 8, avail, None);
        let mut block = vec![0u8; 256];
        predict_planar(&mut block, 16, &top, &left, 4, 8);
        let residual = vec![-2i16; 256];
        add_residual(&mut block, 16, &residual, 4, 8);
        assert!(
            block.iter().all(|&p| p == 0x7E),
            "expected all 0x7E (= 126), got first row: {:?}",
            &block[..16]
        );
    }

    /// **End-to-end check for the fixture's chroma blocks**: PLANAR 8×8 with
    /// no neighbors gives all-128, no chroma residual → all 0x80.
    #[test]
    fn test_fixture_chroma_reconstruction() {
        let plane = vec![0u8; 0];
        let avail = ReferenceAvailability::default();
        let (top, left) = build_reference_samples(&plane, 0, 0, 0, 0, 0, 3, 8, avail, None);
        let mut block = vec![0u8; 64];
        predict_planar(&mut block, 8, &top, &left, 3, 8);
        // No chroma residual in our fixture (cbf_cb = cbf_cr = 0).
        assert!(
            block.iter().all(|&p| p == 0x80),
            "expected all 0x80 (= 128), got first row: {:?}",
            &block[..8]
        );
    }
}
