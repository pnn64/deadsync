//! Sample types: the decoder is generic over the type of a sample, `u8`
//! for 8-bit streams and `u16` for 10-bit ones, and is compiled once for
//! each (as FFmpeg's per-bit-depth templates), so the 8-bit path carries
//! no bit-depth checks.
//!
//! Planes are stored as bytes either way: one byte per 8-bit sample, two
//! little-endian bytes per 10-bit sample (FFmpeg's `yuv420p10le` layout).
//! Sample-processing code indexes planes in samples and goes through the
//! accessors here. Hot per-row operations are methods of [`Pixel`] so that
//! the 8-bit implementation is the existing (SIMD) code.

use crate::dpb::{ChromaRef, DecodedPicture};
use crate::inter_pred;

/// A sample of one bit depth.
pub(crate) trait Pixel:
    Copy + Default + PartialEq + core::fmt::Debug + Send + Sync + 'static
{
    /// Bits per sample (BitDepthY = BitDepthC).
    const BIT_DEPTH: u32;
    /// Largest sample value, (1 << BitDepth) - 1.
    const MAX: i32 = (1 << Self::BIT_DEPTH) - 1;
    /// Mid-grey, 1 << (BitDepth - 1): the default intra prediction value.
    const MID: i32 = 1 << (Self::BIT_DEPTH - 1);
    /// QpBdOffset = 6 * (BitDepth - 8) (spec 7.4.2.1.1).
    const QP_BD_OFFSET: i32 = 6 * (Self::BIT_DEPTH as i32 - 8);
    /// Bytes per sample in a plane.
    const BYTES: usize;

    fn to_i32(self) -> i32;
    /// A value already within 0..=MAX.
    fn from_i32(v: i32) -> Self;
    /// Clip to 0..=MAX.
    #[inline(always)]
    fn clip(v: i32) -> Self {
        Self::from_i32(v.clamp(0, Self::MAX))
    }

    /// Sample `i` of a byte plane.
    fn load(plane: &[u8], i: usize) -> Self;
    /// Set sample `i` of a byte plane.
    fn store(plane: &mut [u8], i: usize, v: Self);
    /// Samples `i..i + out.len()` of a byte plane.
    fn load_row(plane: &[u8], i: usize, out: &mut [Self]);
    /// Set samples `i..i + row.len()` of a byte plane.
    fn store_row(plane: &mut [u8], i: usize, row: &[Self]);

    /// `plane[i + k] = clip(plane[i + k] + res[k])`.
    fn add_residual_row(plane: &mut [u8], i: usize, res: &[i32]);
    /// `plane[i + k] = clip(pred[k] + res[k])`.
    fn put_pred_residual_row(plane: &mut [u8], i: usize, pred: &[Self], res: &[i32]);

    /// Luma motion compensation (see `inter_pred::luma_mc_stride`).
    #[allow(clippy::too_many_arguments)]
    fn luma_mc(
        ref_pic: &DecodedPicture,
        x: i32,
        y: i32,
        dx: i32,
        dy: i32,
        block_w: usize,
        block_h: usize,
        output: &mut [Self],
        ref_stride: usize,
        ref_y_offset: usize,
    );
    /// Chroma motion compensation (see `inter_pred::chroma_mc`).
    #[allow(clippy::too_many_arguments)]
    fn chroma_mc(
        ref_plane: ChromaRef<'_>,
        ref_width: usize,
        ref_height: usize,
        x: i32,
        y: i32,
        dx: i32,
        dy: i32,
        block_w: usize,
        block_h: usize,
        output: &mut [Self],
    );
    fn bi_pred_avg(a: &[Self], b: &[Self], out: &mut [Self]);
    fn weighted_uni(pred: &mut [Self], log2_denom: u32, weight: i32, offset: i32);
    #[allow(clippy::too_many_arguments)]
    fn weighted_bi(
        a: &[Self],
        b: &[Self],
        out: &mut [Self],
        log2_denom: u32,
        w0: i32,
        o0: i32,
        w1: i32,
        o1: i32,
    );
    fn weighted_bi_implicit(a: &[Self], b: &[Self], out: &mut [Self], w0: i32, w1: i32);

    /// 16 lines of 8 samples across a vertical edge, as lanes (see
    /// `simd::scalar::load_edge_lines_v`); offsets in samples.
    fn load_edge_lines_v(plane: &[u8], base: usize, stride: usize) -> [[Self; 16]; 8];
    fn store_edge_lines_v(plane: &mut [u8], base: usize, stride: usize, s: &[[Self; 16]; 8]);
    /// Deblocking of 16 lines across an edge; `alpha`, `beta` and `tc0`
    /// already scaled to the bit depth.
    fn deblock_luma_lines(
        s: &mut [[Self; 16]; 8],
        bs: &[i8; 16],
        tc0: &[i8; 16],
        alpha: i32,
        beta: i32,
    );
    fn deblock_chroma_lines(
        s: &mut [[Self; 16]; 8],
        bs: &[i8; 16],
        tc0: &[i8; 16],
        alpha: i32,
        beta: i32,
    );
}

impl Pixel for u8 {
    const BIT_DEPTH: u32 = 8;
    const BYTES: usize = 1;

    #[inline(always)]
    fn to_i32(self) -> i32 {
        self as i32
    }
    #[inline(always)]
    fn from_i32(v: i32) -> Self {
        v as u8
    }
    #[inline(always)]
    fn load(plane: &[u8], i: usize) -> Self {
        plane[i]
    }
    #[inline(always)]
    fn store(plane: &mut [u8], i: usize, v: Self) {
        plane[i] = v;
    }
    #[inline(always)]
    fn load_row(plane: &[u8], i: usize, out: &mut [Self]) {
        crate::inter_pred::copy_row(out, &plane[i..i + out.len()]);
    }
    #[inline(always)]
    fn store_row(plane: &mut [u8], i: usize, row: &[Self]) {
        crate::inter_pred::copy_row(&mut plane[i..i + row.len()], row);
    }
    #[inline(always)]
    fn add_residual_row(plane: &mut [u8], i: usize, res: &[i32]) {
        for (px, &v) in plane[i..i + res.len()].iter_mut().zip(res) {
            *px = (*px as i32 + v).clamp(0, 255) as u8;
        }
    }
    #[inline(always)]
    fn put_pred_residual_row(plane: &mut [u8], i: usize, pred: &[Self], res: &[i32]) {
        for ((px, &a), &v) in plane[i..i + pred.len()].iter_mut().zip(pred).zip(res) {
            *px = (a as i32 + v).clamp(0, 255) as u8;
        }
    }
    #[inline(always)]
    fn luma_mc(
        ref_pic: &DecodedPicture,
        x: i32,
        y: i32,
        dx: i32,
        dy: i32,
        block_w: usize,
        block_h: usize,
        output: &mut [Self],
        ref_stride: usize,
        ref_y_offset: usize,
    ) {
        inter_pred::luma_mc_stride(
            ref_pic,
            x,
            y,
            dx,
            dy,
            block_w,
            block_h,
            output,
            ref_stride,
            ref_y_offset,
        );
    }
    #[inline(always)]
    fn chroma_mc(
        ref_plane: ChromaRef<'_>,
        ref_width: usize,
        ref_height: usize,
        x: i32,
        y: i32,
        dx: i32,
        dy: i32,
        block_w: usize,
        block_h: usize,
        output: &mut [Self],
    ) {
        inter_pred::chroma_mc(
            ref_plane, ref_width, ref_height, x, y, dx, dy, block_w, block_h, output,
        );
    }
    #[inline(always)]
    fn bi_pred_avg(a: &[Self], b: &[Self], out: &mut [Self]) {
        inter_pred::bi_pred_avg(a, b, out);
    }
    #[inline(always)]
    fn weighted_uni(pred: &mut [Self], log2_denom: u32, weight: i32, offset: i32) {
        inter_pred::weighted_uni(pred, log2_denom, weight, offset);
    }
    #[inline(always)]
    fn weighted_bi(
        a: &[Self],
        b: &[Self],
        out: &mut [Self],
        log2_denom: u32,
        w0: i32,
        o0: i32,
        w1: i32,
        o1: i32,
    ) {
        inter_pred::weighted_bi(a, b, out, log2_denom, w0, o0, w1, o1);
    }
    #[inline(always)]
    fn weighted_bi_implicit(a: &[Self], b: &[Self], out: &mut [Self], w0: i32, w1: i32) {
        inter_pred::weighted_bi_implicit(a, b, out, w0, w1);
    }
    #[inline(always)]
    fn load_edge_lines_v(plane: &[u8], base: usize, stride: usize) -> [[Self; 16]; 8] {
        crate::simd::load_edge_lines_v(plane, base, stride)
    }
    #[inline(always)]
    fn store_edge_lines_v(plane: &mut [u8], base: usize, stride: usize, s: &[[Self; 16]; 8]) {
        crate::simd::store_edge_lines_v(plane, base, stride, s);
    }
    #[inline(always)]
    fn deblock_luma_lines(
        s: &mut [[Self; 16]; 8],
        bs: &[i8; 16],
        tc0: &[i8; 16],
        alpha: i32,
        beta: i32,
    ) {
        crate::simd::deblock_luma_lines(s, bs, tc0, alpha, beta);
    }
    #[inline(always)]
    fn deblock_chroma_lines(
        s: &mut [[Self; 16]; 8],
        bs: &[i8; 16],
        tc0: &[i8; 16],
        alpha: i32,
        beta: i32,
    ) {
        crate::simd::deblock_chroma_lines(s, bs, tc0, alpha, beta);
    }
}

/// 10-bit samples.
impl Pixel for u16 {
    const BIT_DEPTH: u32 = 10;
    const BYTES: usize = 2;

    #[inline(always)]
    fn to_i32(self) -> i32 {
        self as i32
    }
    #[inline(always)]
    fn from_i32(v: i32) -> Self {
        v as u16
    }
    #[inline(always)]
    fn load(plane: &[u8], i: usize) -> Self {
        u16::from_le_bytes([plane[2 * i], plane[2 * i + 1]])
    }
    #[inline(always)]
    fn store(plane: &mut [u8], i: usize, v: Self) {
        plane[2 * i..2 * i + 2].copy_from_slice(&v.to_le_bytes());
    }
    #[inline]
    fn load_row(plane: &[u8], i: usize, out: &mut [Self]) {
        let bytes = &plane[2 * i..2 * (i + out.len())];
        for (o, b) in out.iter_mut().zip(bytes.as_chunks::<2>().0) {
            *o = u16::from_le_bytes(*b);
        }
    }
    #[inline]
    fn store_row(plane: &mut [u8], i: usize, row: &[Self]) {
        let bytes = &mut plane[2 * i..2 * (i + row.len())];
        for (b, v) in bytes.as_chunks_mut::<2>().0.iter_mut().zip(row) {
            *b = v.to_le_bytes();
        }
    }
    fn add_residual_row(plane: &mut [u8], i: usize, res: &[i32]) {
        for (k, &v) in res.iter().enumerate() {
            let cur = Self::load(plane, i + k) as i32;
            Self::store(plane, i + k, Self::clip(cur + v));
        }
    }
    fn put_pred_residual_row(plane: &mut [u8], i: usize, pred: &[Self], res: &[i32]) {
        for (k, (&a, &v)) in pred.iter().zip(res).enumerate() {
            Self::store(plane, i + k, Self::clip(a as i32 + v));
        }
    }
    fn luma_mc(
        ref_pic: &DecodedPicture,
        x: i32,
        y: i32,
        dx: i32,
        dy: i32,
        block_w: usize,
        block_h: usize,
        output: &mut [Self],
        ref_stride: usize,
        ref_y_offset: usize,
    ) {
        inter_pred::luma_mc_px(
            ref_pic,
            x,
            y,
            dx,
            dy,
            block_w,
            block_h,
            output,
            ref_stride,
            ref_y_offset,
        );
    }
    fn chroma_mc(
        ref_plane: ChromaRef<'_>,
        ref_width: usize,
        ref_height: usize,
        x: i32,
        y: i32,
        dx: i32,
        dy: i32,
        block_w: usize,
        block_h: usize,
        output: &mut [Self],
    ) {
        inter_pred::chroma_mc_px(
            ref_plane, ref_width, ref_height, x, y, dx, dy, block_w, block_h, output,
        );
    }
    fn bi_pred_avg(a: &[Self], b: &[Self], out: &mut [Self]) {
        inter_pred::bi_pred_avg_px(a, b, out);
    }
    fn weighted_uni(pred: &mut [Self], log2_denom: u32, weight: i32, offset: i32) {
        inter_pred::weighted_uni_px(pred, log2_denom, weight, offset);
    }
    fn weighted_bi(
        a: &[Self],
        b: &[Self],
        out: &mut [Self],
        log2_denom: u32,
        w0: i32,
        o0: i32,
        w1: i32,
        o1: i32,
    ) {
        inter_pred::weighted_bi_px(a, b, out, log2_denom, w0, o0, w1, o1);
    }
    fn weighted_bi_implicit(a: &[Self], b: &[Self], out: &mut [Self], w0: i32, w1: i32) {
        inter_pred::weighted_bi_implicit_px(a, b, out, w0, w1);
    }
    fn load_edge_lines_v(plane: &[u8], base: usize, stride: usize) -> [[Self; 16]; 8] {
        crate::simd::scalar::load_edge_lines_v(plane, base, stride)
    }
    fn store_edge_lines_v(plane: &mut [u8], base: usize, stride: usize, s: &[[Self; 16]; 8]) {
        crate::simd::scalar::store_edge_lines_v(plane, base, stride, s);
    }
    fn deblock_luma_lines(
        s: &mut [[Self; 16]; 8],
        bs: &[i8; 16],
        tc0: &[i8; 16],
        alpha: i32,
        beta: i32,
    ) {
        crate::simd::scalar::deblock_luma_lines(s, bs, tc0, alpha, beta);
    }
    fn deblock_chroma_lines(
        s: &mut [[Self; 16]; 8],
        bs: &[i8; 16],
        tc0: &[i8; 16],
        alpha: i32,
        beta: i32,
    ) {
        crate::simd::scalar::deblock_chroma_lines(s, bs, tc0, alpha, beta);
    }
}
