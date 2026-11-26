//! Scalar reference motion-compensation kernels.

use crate::inter_pred::avg;
use crate::inter_pred::{clip_u8, fir6, fir6_i32};
use crate::pixel::Pixel;

/// Row-based horizontal half-pel filter for in-bounds blocks.
/// Reads `w` output pixels from row at `src` (which must have `w + 5` accessible bytes).
#[inline(always)]
pub fn row_half_pel_h(src: &[u8], out: &mut [u8], w: usize) {
    let mut i = 0;
    while i < w {
        out[i] = clip_u8((fir6(src, i) + 16) >> 5);
        i += 1;
    }
}

/// Row-based vertical half-pel filter for in-bounds blocks.
/// `rows` contains 6 row slices (y-2..y+3), each at least `w` bytes.
#[inline(always)]
pub fn row_half_pel_v(rows: [&[u8]; 6], out: &mut [u8], w: usize) {
    for i in 0..w {
        let val = rows[0][i] as i32 - 5 * rows[1][i] as i32
            + 20 * rows[2][i] as i32
            + 20 * rows[3][i] as i32
            - 5 * rows[4][i] as i32
            + rows[5][i] as i32;
        out[i] = clip_u8((val + 16) >> 5);
    }
}

/// Row-based diagonal half-pel (hv) for in-bounds blocks.
/// `rows` contains 6 row slices (y-2..y+3), each with `w + 5` accessible bytes.
#[allow(clippy::needless_range_loop)]
#[inline(always)]
pub fn row_half_pel_hv(rows: [&[u8]; 6], out: &mut [u8], w: usize) {
    // First pass: horizontal filter on each of 6 rows → i32 intermediates
    // We need w intermediate values per row
    for i in 0..w {
        let h0 = fir6(rows[0], i);
        let h1 = fir6(rows[1], i);
        let h2 = fir6(rows[2], i);
        let h3 = fir6(rows[3], i);
        let h4 = fir6(rows[4], i);
        let h5 = fir6(rows[5], i);
        let val = fir6_i32(h0, h1, h2, h3, h4, h5);
        out[i] = clip_u8((val + 512) >> 10);
    }
}

/// Average an integer row with a vertical half-pel row.
pub fn row_avg_int_v(int_row: &[u8], rows: [&[u8]; 6], out: &mut [u8], w: usize) {
    for i in 0..w {
        let val = rows[0][i] as i32 - 5 * rows[1][i] as i32
            + 20 * rows[2][i] as i32
            + 20 * rows[3][i] as i32
            - 5 * rows[4][i] as i32
            + rows[5][i] as i32;
        let hp = clip_u8((val + 16) >> 5);
        out[i] = avg(int_row[i], hp);
    }
}

/// Average horizontal and vertical half-pel rows.
pub fn row_avg_h_v(src_h: &[u8], rows_v: [&[u8]; 6], out: &mut [u8], w: usize) {
    for i in 0..w {
        let h_val = clip_u8((fir6(src_h, i) + 16) >> 5);
        let v_val = clip_u8(
            (rows_v[0][i] as i32 - 5 * rows_v[1][i] as i32
                + 20 * rows_v[2][i] as i32
                + 20 * rows_v[3][i] as i32
                - 5 * rows_v[4][i] as i32
                + rows_v[5][i] as i32
                + 16)
                >> 5,
        );
        out[i] = avg(h_val, v_val);
    }
}

/// Quarter-pel `avg(integer_row, h_half_pel)` (arm (1,0)).
pub fn row_avg_int_h(int_row: &[u8], src_h: &[u8], out: &mut [u8], w: usize) {
    for i in 0..w {
        let hp = clip_u8((fir6(src_h, i) + 16) >> 5);
        out[i] = avg(int_row[i], hp);
    }
}

/// Quarter-pel `avg(h_half_pel, integer_row)` (arm (3,0), integer row at +1).
pub fn row_avg_h_int(src_h: &[u8], int_row: &[u8], out: &mut [u8], w: usize) {
    for i in 0..w {
        let hp = clip_u8((fir6(src_h, i) + 16) >> 5);
        out[i] = avg(hp, int_row[i]);
    }
}

/// Diagonal half-pel sample at column `i`: horizontal 6-tap on each of the
/// six rows (each with `w + 5` bytes), then vertical 6-tap, `(v + 512) >> 10`.
#[inline(always)]
pub(crate) fn hv_sample(rows_hv: &[&[u8]; 6], i: usize) -> u8 {
    let v = fir6_i32(
        fir6(rows_hv[0], i),
        fir6(rows_hv[1], i),
        fir6(rows_hv[2], i),
        fir6(rows_hv[3], i),
        fir6(rows_hv[4], i),
        fir6(rows_hv[5], i),
    );
    clip_u8((v + 512) >> 10)
}

/// Vertical half-pel sample at column `i` from six rows of `w` bytes.
#[inline(always)]
pub(crate) fn v_sample(rows: &[&[u8]; 6], i: usize) -> u8 {
    let val =
        rows[0][i] as i32 - 5 * rows[1][i] as i32 + 20 * rows[2][i] as i32 + 20 * rows[3][i] as i32
            - 5 * rows[4][i] as i32
            + rows[5][i] as i32;
    clip_u8((val + 16) >> 5)
}

/// Quarter-pel `avg(h_half_pel, hv_half_pel)` (arms (2,1) and (2,3); avg is
/// symmetric so operand order does not matter).
pub fn row_avg_h_hv(src_h: &[u8], rows_hv: [&[u8]; 6], out: &mut [u8], w: usize) {
    for (i, o) in out.iter_mut().enumerate().take(w) {
        let hp = clip_u8((fir6(src_h, i) + 16) >> 5);
        *o = avg(hp, hv_sample(&rows_hv, i));
    }
}

/// Quarter-pel `avg(v_half_pel, hv_half_pel)` (arms (1,2) and (3,2)).
pub fn row_avg_v_hv(rows_v: [&[u8]; 6], rows_hv: [&[u8]; 6], out: &mut [u8], w: usize) {
    for (i, o) in out.iter_mut().enumerate().take(w) {
        *o = avg(v_sample(&rows_v, i), hv_sample(&rows_hv, i));
    }
}

/// Chroma bilinear interpolation for an in-bounds block.
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
    for row in 0..block_h {
        let row_top = top_off + row * ref_width;
        let row_bot = row_top + ref_width;
        for i in 0..block_w {
            let val = c00 as i32 * ref_plane[row_top + i] as i32
                + c01 as i32 * ref_plane[row_top + i + 1] as i32
                + c10 as i32 * ref_plane[row_bot + i] as i32
                + c11 as i32 * ref_plane[row_bot + i + 1] as i32;
            output[row * block_w + i] = ((val + 32) >> 6) as u8;
        }
    }
}

/// Bi-prediction averaging (spec 8.4.2.3.2).
/// `output[i] = (pred_l0[i] + pred_l1[i] + 1) >> 1` for each pixel.
pub fn bi_pred_avg(pred_l0: &[u8], pred_l1: &[u8], output: &mut [u8]) {
    for (o, (&a, &b)) in output.iter_mut().zip(pred_l0.iter().zip(pred_l1.iter())) {
        *o = ((a as u16 + b as u16 + 1) >> 1) as u8;
    }
}

/// Apply explicit weighted prediction to uni-directional MC output (spec 8.4.2.3.1).
/// `output[i] = clip((pred[i] * weight + (1 << (log2_denom - 1))) >> log2_denom + offset)`
/// When log2_denom == 0, the rounding term is 0.
pub fn weighted_uni(pred: &mut [u8], log2_denom: u32, weight: i32, offset: i32) {
    if log2_denom == 0 {
        for p in pred.iter_mut() {
            *p = ((*p as i32 * weight + offset).clamp(0, 255)) as u8;
        }
    } else {
        let round = 1i32 << (log2_denom - 1);
        for p in pred.iter_mut() {
            *p = ((*p as i32 * weight + round) >> log2_denom)
                .wrapping_add(offset)
                .clamp(0, 255) as u8;
        }
    }
}

/// Apply explicit weighted bi-prediction (spec 8.4.2.3.2).
/// Formula: `clip((p0*w0 + p1*w1 + round) >> (denom+1) + (o0+o1+1)>>1)`
#[allow(clippy::too_many_arguments)]
pub fn weighted_bi(
    pred_l0: &[u8],
    pred_l1: &[u8],
    output: &mut [u8],
    log2_denom: u32,
    w0: i32,
    o0: i32,
    w1: i32,
    o1: i32,
) {
    let round = 1i32 << log2_denom;
    let offset = (o0 + o1 + 1) >> 1;
    let shift = log2_denom + 1;
    for (o, (&a, &b)) in output.iter_mut().zip(pred_l0.iter().zip(pred_l1.iter())) {
        *o = ((a as i32 * w0 + b as i32 * w1 + round) >> shift)
            .wrapping_add(offset)
            .clamp(0, 255) as u8;
    }
}

/// Apply implicit weighted bi-prediction for B-slices (spec 8.4.2.3.2).
/// Uses POC-distance-derived weights with fixed log2_denom=5.
pub fn weighted_bi_implicit(pred_l0: &[u8], pred_l1: &[u8], output: &mut [u8], w0: i32, w1: i32) {
    let round = 1i32 << 5; // 1 << log2_denom where log2_denom=5
    for (o, (&a, &b)) in output.iter_mut().zip(pred_l0.iter().zip(pred_l1.iter())) {
        *o = ((a as i32 * w0 + b as i32 * w1 + round) >> 6).clamp(0, 255) as u8;
    }
}

/// Luma deblocking of 16 lines across one edge (spec 8.7.2.3-4). `s[k][i]`
/// is sample k of line i: p3, p2, p1, p0, q0, q1, q2, q3. Line i is
/// filtered when `bs[i] > 0` and the sample test passes, with the strong
/// filter for bS 4, else tC0 `tc0[i]`.
#[allow(clippy::needless_range_loop)]
pub fn deblock_luma_lines<P: Pixel>(
    s: &mut [[P; 16]; 8],
    bs: &[i8; 16],
    tc0: &[i8; 16],
    alpha: i32,
    beta: i32,
) {
    for i in 0..16 {
        let [p3, p2, p1, p0, q0, q1, q2, q3] =
            core::array::from_fn::<i32, 8, _>(|k| s[k][i].to_i32());
        let (b, t0) = (bs[i], tc0[i] as i32);
        if b <= 0 || (p0 - q0).abs() >= alpha || (p1 - p0).abs() >= beta || (q1 - q0).abs() >= beta
        {
            continue;
        }
        let ap = (p2 - p0).abs() < beta;
        let aq = (q2 - q0).abs() < beta;
        if b == 4 {
            let gap = (p0 - q0).abs() < (alpha >> 2) + 2;
            if gap && ap {
                s[3][i] = P::from_i32((p2 + 2 * p1 + 2 * p0 + 2 * q0 + q1 + 4) >> 3);
                s[2][i] = P::from_i32((p2 + p1 + p0 + q0 + 2) >> 2);
                s[1][i] = P::from_i32((2 * p3 + 3 * p2 + p1 + p0 + q0 + 4) >> 3);
            } else {
                s[3][i] = P::from_i32((2 * p1 + p0 + q1 + 2) >> 2);
            }
            if gap && aq {
                s[4][i] = P::from_i32((q2 + 2 * q1 + 2 * q0 + 2 * p0 + p1 + 4) >> 3);
                s[5][i] = P::from_i32((q2 + q1 + q0 + p0 + 2) >> 2);
                s[6][i] = P::from_i32((2 * q3 + 3 * q2 + q1 + q0 + p0 + 4) >> 3);
            } else {
                s[4][i] = P::from_i32((2 * q1 + q0 + p1 + 2) >> 2);
            }
        } else {
            let tc = t0 + ap as i32 + aq as i32;
            let avg = (p0 + q0 + 1) >> 1;
            if ap {
                s[2][i] = P::from_i32(p1 + (((p2 + avg) >> 1) - p1).clamp(-t0, t0));
            }
            if aq {
                s[5][i] = P::from_i32(q1 + (((q2 + avg) >> 1) - q1).clamp(-t0, t0));
            }
            let delta = ((((q0 - p0) << 2) + (p1 - q1) + 4) >> 3).clamp(-tc, tc);
            s[3][i] = P::clip(p0 + delta);
            s[4][i] = P::clip(q0 - delta);
        }
    }
}

/// Chroma deblocking of 16 lines across one edge (spec 8.7.2.3-4): only
/// p0 and q0 change. Lines as in `deblock_luma_lines` (p3 and q3 unused);
/// bS 4 is the strong filter, else tC = tC0 + 1.
#[allow(clippy::needless_range_loop)]
pub fn deblock_chroma_lines<P: Pixel>(
    s: &mut [[P; 16]; 8],
    bs: &[i8; 16],
    tc0: &[i8; 16],
    alpha: i32,
    beta: i32,
) {
    for i in 0..16 {
        let (p1, p0, q0, q1) = (
            s[2][i].to_i32(),
            s[3][i].to_i32(),
            s[4][i].to_i32(),
            s[5][i].to_i32(),
        );
        if bs[i] <= 0
            || (p0 - q0).abs() >= alpha
            || (p1 - p0).abs() >= beta
            || (q1 - q0).abs() >= beta
        {
            continue;
        }
        if bs[i] == 4 {
            s[3][i] = P::from_i32((2 * p1 + p0 + q1 + 2) >> 2);
            s[4][i] = P::from_i32((2 * q1 + q0 + p1 + 2) >> 2);
        } else {
            let tc = tc0[i] as i32 + 1;
            let delta = ((((q0 - p0) << 2) + (p1 - q1) + 4) >> 3).clamp(-tc, tc);
            s[3][i] = P::clip(p0 + delta);
            s[4][i] = P::clip(q0 - delta);
        }
    }
}

/// The 8 samples `plane[base + i * stride..][..8]` of 16 lines `i`, as
/// lanes: `s[k][i]` is sample k of line i (the samples across a vertical
/// edge, x - 4 .. x + 4, of 16 rows).
#[allow(clippy::needless_range_loop)]
pub fn load_edge_lines_v<P: Pixel>(plane: &[u8], base: usize, stride: usize) -> [[P; 16]; 8] {
    let mut s = [[P::default(); 16]; 8];
    for i in 0..16 {
        for k in 0..8 {
            s[k][i] = P::load(plane, base + i * stride + k);
        }
    }
    s
}

/// Store lanes back as `load_edge_lines_v` read them.
#[allow(clippy::needless_range_loop)]
pub fn store_edge_lines_v<P: Pixel>(
    plane: &mut [u8],
    base: usize,
    stride: usize,
    s: &[[P; 16]; 8],
) {
    for i in 0..16 {
        for k in 0..8 {
            P::store(plane, base + i * stride + k, s[k][i]);
        }
    }
}
