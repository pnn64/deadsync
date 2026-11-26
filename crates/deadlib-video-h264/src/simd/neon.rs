//! Hand-written NEON (aarch64) motion-compensation kernels.
//!
//! Covers the luma half-pel and quarter-pel filters, chroma bilinear
//! interpolation, bi-prediction averaging, and the deblocking filters
//! with the transposes for vertical edges. Kernels not implemented here
//! (explicit and implicit weighting) re-export the scalar reference. Luma
//! kernels process 8 pixels per iteration with a scalar tail. All are
//! bit-exact with `scalar.rs` (checked against it on random input in
//! `mod.rs`, and by the golden corpus digest test).

use core::arch::aarch64::*;

use crate::inter_pred::{avg, clip_u8, fir6};

use super::scalar::hv_sample;
pub use super::scalar::{weighted_bi, weighted_bi_implicit, weighted_uni};

/// 6-tap FIR of six u8x8 vectors, rounded and clipped to u8x8:
/// `clip((s0 - 5*s1 + 20*s2 + 20*s3 - 5*s4 + s5 + 16) >> 5)`.
#[inline(always)]
unsafe fn fir6_round_clip(
    s0: uint8x8_t,
    s1: uint8x8_t,
    s2: uint8x8_t,
    s3: uint8x8_t,
    s4: uint8x8_t,
    s5: uint8x8_t,
) -> uint8x8_t {
    let sum_pos1 = vaddl_u8(s0, s5);
    let sum_20 = vaddl_u8(s2, s3);
    let sum_neg5 = vaddl_u8(s1, s4);
    let mut acc = vreinterpretq_s16_u16(sum_pos1);
    acc = vmlaq_n_s16(acc, vreinterpretq_s16_u16(sum_20), 20);
    acc = vmlsq_n_s16(acc, vreinterpretq_s16_u16(sum_neg5), 5);
    acc = vaddq_s16(acc, vdupq_n_s16(16));
    vqmovun_s16(vshrq_n_s16(acc, 5))
}

/// Horizontal half-pel for 8 pixels starting at `src[i]` (`src` must have
/// `i + 13` accessible bytes).
#[inline(always)]
unsafe fn hfir6_8(src: &[u8], i: usize) -> uint8x8_t {
    let p = src.as_ptr().add(i);
    fir6_round_clip(
        vld1_u8(p),
        vld1_u8(p.add(1)),
        vld1_u8(p.add(2)),
        vld1_u8(p.add(3)),
        vld1_u8(p.add(4)),
        vld1_u8(p.add(5)),
    )
}

/// Vertical half-pel for 8 pixels at column `i` across six rows.
#[inline(always)]
unsafe fn vfir6_8(rows: &[&[u8]; 6], i: usize) -> uint8x8_t {
    fir6_round_clip(
        vld1_u8(rows[0].as_ptr().add(i)),
        vld1_u8(rows[1].as_ptr().add(i)),
        vld1_u8(rows[2].as_ptr().add(i)),
        vld1_u8(rows[3].as_ptr().add(i)),
        vld1_u8(rows[4].as_ptr().add(i)),
        vld1_u8(rows[5].as_ptr().add(i)),
    )
}

#[inline(always)]
fn scalar_vfir6(rows: &[&[u8]; 6], i: usize) -> u8 {
    let val =
        rows[0][i] as i32 - 5 * rows[1][i] as i32 + 20 * rows[2][i] as i32 + 20 * rows[3][i] as i32
            - 5 * rows[4][i] as i32
            + rows[5][i] as i32;
    clip_u8((val + 16) >> 5)
}

/// Row-based horizontal half-pel filter for in-bounds blocks.
/// Reads `w` output pixels from row at `src` (which must have `w + 5` accessible bytes).
#[inline(never)]
pub fn row_half_pel_h(src: &[u8], out: &mut [u8], w: usize) {
    debug_assert!(src.len() >= w + 5 && out.len() >= w);
    let mut i = 0;
    while i + 8 <= w {
        // SAFETY: src has w + 5 bytes, so src[i..i + 13] is in bounds for
        // i + 8 <= w; out has at least w bytes.
        unsafe {
            vst1_u8(out.as_mut_ptr().add(i), hfir6_8(src, i));
        }
        i += 8;
    }
    while i < w {
        out[i] = clip_u8((fir6(src, i) + 16) >> 5);
        i += 1;
    }
}

/// Row-based vertical half-pel filter for in-bounds blocks.
/// `rows` contains 6 row slices (y-2..y+3), each at least `w` bytes.
#[inline(never)]
pub fn row_half_pel_v(rows: [&[u8]; 6], out: &mut [u8], w: usize) {
    debug_assert!(rows.iter().all(|r| r.len() >= w) && out.len() >= w);
    let mut i = 0;
    while i + 8 <= w {
        // SAFETY: every row has at least w bytes, so rows[k][i..i + 8] is in
        // bounds for i + 8 <= w; out has at least w bytes.
        unsafe {
            vst1_u8(out.as_mut_ptr().add(i), vfir6_8(&rows, i));
        }
        i += 8;
    }
    while i < w {
        out[i] = scalar_vfir6(&rows, i);
        i += 1;
    }
}

/// Average an integer row with a vertical half-pel row.
#[inline(never)]
pub fn row_avg_int_v(int_row: &[u8], rows: [&[u8]; 6], out: &mut [u8], w: usize) {
    debug_assert!(int_row.len() >= w && rows.iter().all(|r| r.len() >= w) && out.len() >= w);
    let mut i = 0;
    while i + 8 <= w {
        // SAFETY: as in row_half_pel_v, plus int_row has at least w bytes.
        unsafe {
            let int_val = vld1_u8(int_row.as_ptr().add(i));
            let hp = vfir6_8(&rows, i);
            vst1_u8(out.as_mut_ptr().add(i), vrhadd_u8(int_val, hp));
        }
        i += 8;
    }
    while i < w {
        out[i] = avg(int_row[i], scalar_vfir6(&rows, i));
        i += 1;
    }
}

/// Average horizontal and vertical half-pel rows.
#[inline(never)]
pub fn row_avg_h_v(src_h: &[u8], rows_v: [&[u8]; 6], out: &mut [u8], w: usize) {
    debug_assert!(src_h.len() >= w + 5 && rows_v.iter().all(|r| r.len() >= w) && out.len() >= w);
    let mut i = 0;
    while i + 8 <= w {
        // SAFETY: src_h has w + 5 bytes (see row_half_pel_h) and every row
        // in rows_v has at least w bytes (see row_half_pel_v).
        unsafe {
            let h_val = hfir6_8(src_h, i);
            let v_val = vfir6_8(&rows_v, i);
            vst1_u8(out.as_mut_ptr().add(i), vrhadd_u8(h_val, v_val));
        }
        i += 8;
    }
    while i < w {
        let h_val = clip_u8((fir6(src_h, i) + 16) >> 5);
        out[i] = avg(h_val, scalar_vfir6(&rows_v, i));
        i += 1;
    }
}

/// Unrounded horizontal 6-tap of 8 pixels at `src[i]` as i16 (the sum fits:
/// -2550..=10710).
#[inline(always)]
unsafe fn hfir6_raw_8(src: &[u8], i: usize) -> int16x8_t {
    let p = src.as_ptr().add(i);
    let sum_pos1 = vaddl_u8(vld1_u8(p), vld1_u8(p.add(5)));
    let sum_20 = vaddl_u8(vld1_u8(p.add(2)), vld1_u8(p.add(3)));
    let sum_neg5 = vaddl_u8(vld1_u8(p.add(1)), vld1_u8(p.add(4)));
    let mut acc = vreinterpretq_s16_u16(sum_pos1);
    acc = vmlaq_n_s16(acc, vreinterpretq_s16_u16(sum_20), 20);
    vmlsq_n_s16(acc, vreinterpretq_s16_u16(sum_neg5), 5)
}

/// Diagonal half-pel for 8 pixels at column `i`: horizontal 6-tap of each
/// of the six rows (each with `i + 13` accessible bytes), then vertical
/// 6-tap in i32, `clip((v + 512) >> 10)`.
#[inline(always)]
unsafe fn hvfir6_8(rows: &[&[u8]; 6], i: usize) -> uint8x8_t {
    let h: [int16x8_t; 6] = core::array::from_fn(|k| hfir6_raw_8(rows[k], i));
    let half = |lo: bool| {
        let w = |v: int16x8_t| {
            if lo {
                vmovl_s16(vget_low_s16(v))
            } else {
                vmovl_s16(vget_high_s16(v))
            }
        };
        let mut acc = vaddq_s32(w(h[0]), w(h[5]));
        acc = vmlaq_n_s32(acc, vaddq_s32(w(h[2]), w(h[3])), 20);
        acc = vmlsq_n_s32(acc, vaddq_s32(w(h[1]), w(h[4])), 5);
        // (v + 512) >> 10, saturated to 0..=65535
        vqmovun_s32(vrshrq_n_s32(acc, 10))
    };
    vqmovn_u16(vcombine_u16(half(true), half(false)))
}

/// Row-based diagonal half-pel (hv) for in-bounds blocks.
/// `rows` contains 6 row slices (y-2..y+3), each with `w + 5` accessible bytes.
#[inline(never)]
pub fn row_half_pel_hv(rows: [&[u8]; 6], out: &mut [u8], w: usize) {
    debug_assert!(rows.iter().all(|r| r.len() >= w + 5) && out.len() >= w);
    let mut i = 0;
    while i + 8 <= w {
        // SAFETY: every row has w + 5 bytes, so rows[k][i..i + 13] is in
        // bounds for i + 8 <= w; out has at least w bytes.
        unsafe {
            vst1_u8(out.as_mut_ptr().add(i), hvfir6_8(&rows, i));
        }
        i += 8;
    }
    while i < w {
        out[i] = hv_sample(&rows, i);
        i += 1;
    }
}

/// Quarter-pel `avg(integer_row, h_half_pel)` (arm (1,0)).
#[inline(never)]
pub fn row_avg_int_h(int_row: &[u8], src_h: &[u8], out: &mut [u8], w: usize) {
    debug_assert!(int_row.len() >= w && src_h.len() >= w + 5 && out.len() >= w);
    let mut i = 0;
    while i + 8 <= w {
        // SAFETY: as in row_half_pel_h, plus int_row has at least w bytes.
        unsafe {
            let int_val = vld1_u8(int_row.as_ptr().add(i));
            vst1_u8(
                out.as_mut_ptr().add(i),
                vrhadd_u8(int_val, hfir6_8(src_h, i)),
            );
        }
        i += 8;
    }
    while i < w {
        out[i] = avg(int_row[i], clip_u8((fir6(src_h, i) + 16) >> 5));
        i += 1;
    }
}

/// Quarter-pel `avg(h_half_pel, integer_row)` (arm (3,0), integer row at +1).
#[inline(never)]
pub fn row_avg_h_int(src_h: &[u8], int_row: &[u8], out: &mut [u8], w: usize) {
    row_avg_int_h(int_row, src_h, out, w);
}

/// Quarter-pel `avg(h_half_pel, hv_half_pel)` (arms (2,1) and (2,3)).
#[inline(never)]
pub fn row_avg_h_hv(src_h: &[u8], rows_hv: [&[u8]; 6], out: &mut [u8], w: usize) {
    debug_assert!(src_h.len() >= w + 5 && rows_hv.iter().all(|r| r.len() >= w + 5));
    let mut i = 0;
    while i + 8 <= w {
        // SAFETY: src_h and every row of rows_hv have w + 5 bytes (see
        // row_half_pel_h and row_half_pel_hv); out has at least w bytes.
        unsafe {
            let hv = hvfir6_8(&rows_hv, i);
            vst1_u8(out.as_mut_ptr().add(i), vrhadd_u8(hfir6_8(src_h, i), hv));
        }
        i += 8;
    }
    while i < w {
        out[i] = avg(clip_u8((fir6(src_h, i) + 16) >> 5), hv_sample(&rows_hv, i));
        i += 1;
    }
}

/// Quarter-pel `avg(v_half_pel, hv_half_pel)` (arms (1,2) and (3,2)).
#[inline(never)]
pub fn row_avg_v_hv(rows_v: [&[u8]; 6], rows_hv: [&[u8]; 6], out: &mut [u8], w: usize) {
    debug_assert!(rows_v.iter().all(|r| r.len() >= w) && rows_hv.iter().all(|r| r.len() >= w + 5));
    let mut i = 0;
    while i + 8 <= w {
        // SAFETY: rows_v have w bytes (see row_half_pel_v), rows_hv w + 5
        // (see row_half_pel_hv); out has at least w bytes.
        unsafe {
            let hv = hvfir6_8(&rows_hv, i);
            vst1_u8(out.as_mut_ptr().add(i), vrhadd_u8(vfir6_8(&rows_v, i), hv));
        }
        i += 8;
    }
    while i < w {
        out[i] = avg(scalar_vfir6(&rows_v, i), hv_sample(&rows_hv, i));
        i += 1;
    }
}

/// Chroma bilinear interpolation for an in-bounds block: each output
/// sample reads its own and the next column of its row and the row below.
#[allow(clippy::too_many_arguments)]
#[inline(never)]
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
    if block_w != 8 && block_w != 4 {
        super::scalar::chroma_bilinear_block(
            ref_plane, top_off, ref_width, block_w, block_h, output, c00, c01, c10, c11,
        );
        return;
    }
    if block_h == 0 {
        return;
    }
    // The last row reads columns up to block_w of the row below it, as the
    // scalar kernel does
    assert!(top_off + block_h * ref_width + block_w < ref_plane.len());
    assert!(output.len() >= block_w * block_h);
    // SAFETY: every load below reads `block_w` bytes at offsets up to
    // `row + 1` for rows top_off .. top_off + block_h * ref_width (the row
    // below the last included), within ref_plane per the assert above;
    // stores write block_w bytes per row within output.
    unsafe {
        let (k00, k01, k10, k11) = (
            vdup_n_u8(c00),
            vdup_n_u8(c01),
            vdup_n_u8(c10),
            vdup_n_u8(c11),
        );
        let base = ref_plane.as_ptr().add(top_off);
        let load = |p: *const u8| -> uint8x8_t {
            if block_w == 8 {
                vld1_u8(p)
            } else {
                vcreate_u8(
                    u32::from_le_bytes(core::ptr::read_unaligned(p as *const [u8; 4])) as u64,
                )
            }
        };
        let mut top = load(base);
        let mut top1 = load(base.add(1));
        for row in 0..block_h {
            let below = base.add((row + 1) * ref_width);
            let (bot, bot1) = (load(below), load(below.add(1)));
            let mut acc = vmull_u8(top, k00);
            acc = vmlal_u8(acc, top1, k01);
            acc = vmlal_u8(acc, bot, k10);
            acc = vmlal_u8(acc, bot1, k11);
            // (val + 32) >> 6; the weights sum to 64, so it fits in u8
            let px = vrshrn_n_u16(acc, 6);
            let dst = output.as_mut_ptr().add(row * block_w);
            if block_w == 8 {
                vst1_u8(dst, px);
            } else {
                vst1_lane_u32::<0>(dst as *mut u32, vreinterpret_u32_u8(px));
            }
            (top, top1) = (bot, bot1);
        }
    }
}

/// Bi-prediction averaging (spec 8.4.2.3.2): `(a + b + 1) >> 1`.
#[inline(never)]
pub fn bi_pred_avg(pred_l0: &[u8], pred_l1: &[u8], output: &mut [u8]) {
    let n = output.len().min(pred_l0.len()).min(pred_l1.len());
    let mut i = 0;
    while i + 16 <= n {
        // SAFETY: all three slices have at least n bytes and i + 16 <= n.
        unsafe {
            let a = vld1q_u8(pred_l0.as_ptr().add(i));
            let b = vld1q_u8(pred_l1.as_ptr().add(i));
            vst1q_u8(output.as_mut_ptr().add(i), vrhaddq_u8(a, b));
        }
        i += 16;
    }
    while i < n {
        output[i] = ((pred_l0[i] as u16 + pred_l1[i] as u16 + 1) >> 1) as u8;
        i += 1;
    }
}

/// Luma deblocking of 16 lines across one edge; see
/// `scalar::deblock_luma_lines`. All 16 lines are processed at once, the
/// per-line decisions applied as lane masks (as FFmpeg's NEON filter).
#[inline(never)]
pub fn deblock_luma_lines(
    s: &mut [[u8; 16]; 8],
    bs: &[i8; 16],
    tc0: &[i8; 16],
    alpha: i32,
    beta: i32,
) {
    // SAFETY: every load and store is 16 bytes within one of the fixed-size
    // rows of `s`, `bs` or `tc0`.
    unsafe {
        let ld = |k: usize| vld1q_u8(s[k].as_ptr());
        let (p3, p2, p1, p0) = (ld(0), ld(1), ld(2), ld(3));
        let (q0, q1, q2, q3) = (ld(4), ld(5), ld(6), ld(7));
        let bsv = vreinterpretq_u8_s8(vld1q_s8(bs.as_ptr()));
        let tc0v = vreinterpretq_u8_s8(vld1q_s8(tc0.as_ptr()));
        let alpha_v = vdupq_n_u8(alpha.clamp(0, 255) as u8);
        let beta_v = vdupq_n_u8(beta.clamp(0, 255) as u8);

        // Line test: bS > 0, |p0 - q0| < alpha, |p1 - p0| < beta, |q1 - q0| < beta
        let mut filt = vcgtq_u8(bsv, vdupq_n_u8(0));
        filt = vandq_u8(filt, vcltq_u8(vabdq_u8(p0, q0), alpha_v));
        filt = vandq_u8(filt, vcltq_u8(vabdq_u8(p1, p0), beta_v));
        filt = vandq_u8(filt, vcltq_u8(vabdq_u8(q1, q0), beta_v));
        let ap = vcltq_u8(vabdq_u8(p2, p0), beta_v);
        let aq = vcltq_u8(vabdq_u8(q2, q0), beta_v);
        let strong = vandq_u8(filt, vceqq_u8(bsv, vdupq_n_u8(4)));
        let normal = vbicq_u8(filt, strong);

        // --- Normal filter (bS 1..3) ---
        // p1' = clamp((p2 + avg) >> 1, p1 - tc0, p1 + tc0), avg = (p0 + q0 + 1) >> 1
        let avg = vrhaddq_u8(p0, q0);
        let np1 = vminq_u8(
            vmaxq_u8(vhaddq_u8(p2, avg), vqsubq_u8(p1, tc0v)),
            vqaddq_u8(p1, tc0v),
        );
        let nq1 = vminq_u8(
            vmaxq_u8(vhaddq_u8(q2, avg), vqsubq_u8(q1, tc0v)),
            vqaddq_u8(q1, tc0v),
        );
        // tc = tc0 + ap + aq
        let one = vdupq_n_u8(1);
        let tc = vaddq_u8(vaddq_u8(tc0v, vandq_u8(ap, one)), vandq_u8(aq, one));
        // delta = clamp((((q0 - p0) << 2) + (p1 - q1) + 4) >> 3, -tc, tc), in i16
        let half = |lo: bool| {
            let w = |v: uint8x16_t| -> int16x8_t {
                if lo {
                    vreinterpretq_s16_u16(vmovl_u8(vget_low_u8(v)))
                } else {
                    vreinterpretq_s16_u16(vmovl_high_u8(v))
                }
            };
            let (wp0, wq0, wp1, wq1, wtc) = (w(p0), w(q0), w(p1), w(q1), w(tc));
            let d = vaddq_s16(vshlq_n_s16(vsubq_s16(wq0, wp0), 2), vsubq_s16(wp1, wq1));
            let d = vminq_s16(vmaxq_s16(vrshrq_n_s16(d, 3), vnegq_s16(wtc)), wtc);
            (
                vqmovun_s16(vaddq_s16(wp0, d)),
                vqmovun_s16(vsubq_s16(wq0, d)),
            )
        };
        let (lo_p0, lo_q0) = half(true);
        let (hi_p0, hi_q0) = half(false);
        let np0 = vcombine_u8(lo_p0, hi_p0);
        let nq0 = vcombine_u8(lo_q0, hi_q0);

        // --- Strong filter (bS 4), sums in u16 ---
        let small = vdupq_n_u8(((alpha >> 2) + 2).clamp(0, 255) as u8);
        let gap = vcltq_u8(vabdq_u8(p0, q0), small);
        let sp = vandq_u8(gap, ap);
        let sq = vandq_u8(gap, aq);
        let strong_side = |a3: uint8x16_t,
                           a2: uint8x16_t,
                           a1: uint8x16_t,
                           a0: uint8x16_t,
                           b0: uint8x16_t,
                           b1: uint8x16_t|
         -> [uint8x16_t; 4] {
            let half = |lo: bool| {
                let w = |v: uint8x16_t| -> uint16x8_t {
                    if lo {
                        vmovl_u8(vget_low_u8(v))
                    } else {
                        vmovl_high_u8(v)
                    }
                };
                let (a3, a2, a1, a0, b0, b1) = (w(a3), w(a2), w(a1), w(a0), w(b0), w(b1));
                let a1a0b0 = vaddq_u16(vaddq_u16(a1, a0), b0);
                // (a2 + 2a1 + 2a0 + 2b0 + b1 + 4) >> 3
                let x0 = vaddq_u16(vaddq_u16(a2, vshlq_n_u16(a1a0b0, 1)), b1);
                // (a2 + a1 + a0 + b0 + 2) >> 2
                let x1 = vaddq_u16(a2, a1a0b0);
                // (2a3 + 3a2 + a1 + a0 + b0 + 4) >> 3
                let x2 = vaddq_u16(vaddq_u16(vshlq_n_u16(a3, 1), vmulq_n_u16(a2, 3)), a1a0b0);
                // (2a1 + a0 + b1 + 2) >> 2
                let w0 = vaddq_u16(vaddq_u16(vshlq_n_u16(a1, 1), a0), b1);
                [
                    vrshrn_n_u16(x0, 3),
                    vrshrn_n_u16(x1, 2),
                    vrshrn_n_u16(x2, 3),
                    vrshrn_n_u16(w0, 2),
                ]
            };
            let (l, h) = (half(true), half(false));
            core::array::from_fn(|k| vcombine_u8(l[k], h[k]))
        };
        let [sp0, sp1, sp2, wp0] = strong_side(p3, p2, p1, p0, q0, q1);
        let [sq0, sq1, sq2, wq0] = strong_side(q3, q2, q1, q0, p0, p1);

        // --- Select ---
        let sel = |m: uint8x16_t, a: uint8x16_t, b: uint8x16_t| vbslq_u8(m, a, b);
        let out_p0 = sel(normal, np0, sel(strong, sel(sp, sp0, wp0), p0));
        let out_q0 = sel(normal, nq0, sel(strong, sel(sq, sq0, wq0), q0));
        let out_p1 = sel(
            vandq_u8(normal, ap),
            np1,
            sel(vandq_u8(strong, sp), sp1, p1),
        );
        let out_q1 = sel(
            vandq_u8(normal, aq),
            nq1,
            sel(vandq_u8(strong, sq), sq1, q1),
        );
        let out_p2 = sel(vandq_u8(strong, sp), sp2, p2);
        let out_q2 = sel(vandq_u8(strong, sq), sq2, q2);
        for (k, v) in [
            (1, out_p2),
            (2, out_p1),
            (3, out_p0),
            (4, out_q0),
            (5, out_q1),
            (6, out_q2),
        ] {
            vst1q_u8(s[k].as_mut_ptr(), v);
        }
    }
}

/// Chroma deblocking of 16 lines across one edge; see
/// `scalar::deblock_chroma_lines`.
#[inline(never)]
pub fn deblock_chroma_lines(
    s: &mut [[u8; 16]; 8],
    bs: &[i8; 16],
    tc0: &[i8; 16],
    alpha: i32,
    beta: i32,
) {
    // SAFETY: every load and store is 16 bytes within one of the fixed-size
    // rows of `s`, `bs` or `tc0`.
    unsafe {
        let (p1, p0) = (vld1q_u8(s[2].as_ptr()), vld1q_u8(s[3].as_ptr()));
        let (q0, q1) = (vld1q_u8(s[4].as_ptr()), vld1q_u8(s[5].as_ptr()));
        let bsv = vreinterpretq_u8_s8(vld1q_s8(bs.as_ptr()));
        let tc = vaddq_u8(vreinterpretq_u8_s8(vld1q_s8(tc0.as_ptr())), vdupq_n_u8(1));
        let alpha_v = vdupq_n_u8(alpha.clamp(0, 255) as u8);
        let beta_v = vdupq_n_u8(beta.clamp(0, 255) as u8);
        let mut filt = vcgtq_u8(bsv, vdupq_n_u8(0));
        filt = vandq_u8(filt, vcltq_u8(vabdq_u8(p0, q0), alpha_v));
        filt = vandq_u8(filt, vcltq_u8(vabdq_u8(p1, p0), beta_v));
        filt = vandq_u8(filt, vcltq_u8(vabdq_u8(q1, q0), beta_v));
        let strong = vandq_u8(filt, vceqq_u8(bsv, vdupq_n_u8(4)));
        let half = |lo: bool| {
            let w = |v: uint8x16_t| -> int16x8_t {
                if lo {
                    vreinterpretq_s16_u16(vmovl_u8(vget_low_u8(v)))
                } else {
                    vreinterpretq_s16_u16(vmovl_high_u8(v))
                }
            };
            let (wp1, wp0, wq0, wq1, wtc) = (w(p1), w(p0), w(q0), w(q1), w(tc));
            // Normal: delta = clamp((((q0 - p0) << 2) + (p1 - q1) + 4) >> 3, -tc, tc)
            let d = vaddq_s16(vshlq_n_s16(vsubq_s16(wq0, wp0), 2), vsubq_s16(wp1, wq1));
            let d = vminq_s16(vmaxq_s16(vrshrq_n_s16(d, 3), vnegq_s16(wtc)), wtc);
            let np0 = vqmovun_s16(vaddq_s16(wp0, d));
            let nq0 = vqmovun_s16(vsubq_s16(wq0, d));
            // Strong: (2p1 + p0 + q1 + 2) >> 2 and (2q1 + q0 + p1 + 2) >> 2
            let sp0 = vqrshrun_n_s16(vaddq_s16(vaddq_s16(vshlq_n_s16(wp1, 1), wp0), wq1), 2);
            let sq0 = vqrshrun_n_s16(vaddq_s16(vaddq_s16(vshlq_n_s16(wq1, 1), wq0), wp1), 2);
            (np0, nq0, sp0, sq0)
        };
        let (a, b, c, d) = half(true);
        let (e, f, g, h) = half(false);
        let (np0, nq0, sp0, sq0) = (
            vcombine_u8(a, e),
            vcombine_u8(b, f),
            vcombine_u8(c, g),
            vcombine_u8(d, h),
        );
        let out_p0 = vbslq_u8(filt, vbslq_u8(strong, sp0, np0), p0);
        let out_q0 = vbslq_u8(filt, vbslq_u8(strong, sq0, nq0), q0);
        vst1q_u8(s[3].as_mut_ptr(), out_p0);
        vst1q_u8(s[4].as_mut_ptr(), out_q0);
    }
}

/// Transpose 8 rows of 8 bytes into 8 columns.
#[inline(always)]
unsafe fn transpose_8x8(r: [uint8x8_t; 8]) -> [uint8x8_t; 8] {
    let t0 = vtrn_u8(r[0], r[1]);
    let t1 = vtrn_u8(r[2], r[3]);
    let t2 = vtrn_u8(r[4], r[5]);
    let t3 = vtrn_u8(r[6], r[7]);
    let u16x = vreinterpret_u16_u8;
    let u0 = vtrn_u16(u16x(t0.0), u16x(t1.0));
    let u1 = vtrn_u16(u16x(t0.1), u16x(t1.1));
    let u2 = vtrn_u16(u16x(t2.0), u16x(t3.0));
    let u3 = vtrn_u16(u16x(t2.1), u16x(t3.1));
    let u32x = vreinterpret_u32_u16;
    let v0 = vtrn_u32(u32x(u0.0), u32x(u2.0));
    let v1 = vtrn_u32(u32x(u1.0), u32x(u3.0));
    let v2 = vtrn_u32(u32x(u0.1), u32x(u2.1));
    let v3 = vtrn_u32(u32x(u1.1), u32x(u3.1));
    let b = vreinterpret_u8_u32;
    [
        b(v0.0),
        b(v1.0),
        b(v2.0),
        b(v3.0),
        b(v0.1),
        b(v1.1),
        b(v2.1),
        b(v3.1),
    ]
}

/// See `scalar::load_edge_lines_v`: 16 rows of 8 bytes, transposed.
#[inline(never)]
pub fn load_edge_lines_v(plane: &[u8], base: usize, stride: usize) -> [[u8; 16]; 8] {
    assert!(base + 15 * stride + 8 <= plane.len());
    let mut s = [[0u8; 16]; 8];
    // SAFETY: the assert covers the 8 bytes read from each of the 16 rows;
    // stores are 16 bytes into the rows of `s`.
    unsafe {
        let row = |i: usize| vld1_u8(plane.as_ptr().add(base + i * stride));
        let lo = transpose_8x8(core::array::from_fn(row));
        let hi = transpose_8x8(core::array::from_fn(|i| row(8 + i)));
        for k in 0..8 {
            vst1q_u8(s[k].as_mut_ptr(), vcombine_u8(lo[k], hi[k]));
        }
    }
    s
}

/// See `scalar::store_edge_lines_v`.
#[inline(never)]
pub fn store_edge_lines_v(plane: &mut [u8], base: usize, stride: usize, s: &[[u8; 16]; 8]) {
    assert!(base + 15 * stride + 8 <= plane.len());
    // SAFETY: loads are 16 bytes from the rows of `s`; the assert covers
    // the 8 bytes written to each of the 16 rows.
    unsafe {
        let k = |k: usize| vld1q_u8(s[k].as_ptr());
        let cols: [uint8x16_t; 8] = core::array::from_fn(k);
        let lo = transpose_8x8(core::array::from_fn(|c| vget_low_u8(cols[c])));
        let hi = transpose_8x8(core::array::from_fn(|c| vget_high_u8(cols[c])));
        for i in 0..8 {
            vst1_u8(plane.as_mut_ptr().add(base + i * stride), lo[i]);
            vst1_u8(plane.as_mut_ptr().add(base + (8 + i) * stride), hi[i]);
        }
    }
}
