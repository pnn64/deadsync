//! Phase 3d-6: Inter prediction — motion compensation with sub-pixel interpolation.
//!
//! Implements HEVC spec 8.5.3.2 (luma and chroma sample interpolation).
//! - Luma uses an 8-tap filter at quarter-pel resolution (4 positions).
//! - Chroma uses a 4-tap filter at eighth-pel resolution (8 positions).
//! - Boundary samples are handled by clamping reference coordinates.
//! - Bi-prediction averages L0 and L1 predictions: `(L0 + L1 + 1) >> 1`.
//!
//! Low-level MC functions (`mc_luma`, `mc_chroma`, `mc_luma_i16`, `mc_chroma_i16`,
//! `ref_sample`) are generic over `P: Pixel` for multi-bit-depth support.

use std::sync::Arc;

use crate::shared::SharedTable;

use crate::cu_tree::PictureState;
use crate::dpb::DecodedPicture;
use crate::pixel::Pixel;

/// HEVC 8-tap luma interpolation filter coefficients (spec table 8-12).
/// Index 0 = integer position (identity / copy), 1..3 = quarter-pel.
const QPEL_FILTER: [[i16; 8]; 4] = [
    [0, 0, 0, 64, 0, 0, 0, 0],
    [-1, 4, -10, 58, 17, -5, 1, 0],
    [-1, 4, -11, 40, 40, -11, 4, -1],
    [0, 1, -5, 17, 58, -10, 4, -1],
];

/// HEVC 4-tap chroma interpolation filter coefficients (spec table 8-13).
/// Index 0 = integer position (identity / copy), 1..7 = eighth-pel.
const EPEL_FILTER: [[i16; 4]; 8] = [
    [0, 64, 0, 0],
    [-2, 58, 10, -2],
    [-4, 54, 16, -2],
    [-6, 46, 28, -4],
    [-4, 36, 36, -4],
    [-4, 28, 46, -6],
    [-2, 16, 54, -4],
    [-2, 10, 58, -2],
];

/// Number of extra reference rows/columns needed on each side for luma.
const QPEL_EXTRA_BEFORE: i32 = 3;
const QPEL_EXTRA_AFTER: i32 = 4;

/// Number of extra reference rows/columns needed on each side for chroma.
const EPEL_EXTRA_BEFORE: i32 = 1;
#[allow(dead_code)]
const EPEL_EXTRA_AFTER: i32 = 2;

/// Maximum PB size (used for intermediate buffer stride).
const MAX_PB_SIZE: usize = 64;

/// Max rows in the 2D separable luma filter intermediate buffer:
/// n_pb_h (max 64) + QPEL_EXTRA_BEFORE (3) + QPEL_EXTRA_AFTER (4) = 71.
const QPEL_TMP_ROWS: usize = MAX_PB_SIZE + 3 + 4;

/// Max rows in the 2D separable chroma filter intermediate buffer:
/// n_pb_h_c (max 64 for 4:4:4) + EPEL_EXTRA_BEFORE (1) + EPEL_EXTRA_AFTER (2).
const EPEL_TMP_ROWS: usize = MAX_PB_SIZE + 1 + 2;

/// Max luma PU area (64×64).
const MAX_PB_LUMA: usize = MAX_PB_SIZE * MAX_PB_SIZE;

/// Max chroma PU area: 64×64 for 4:4:4 (32×32 would suffice for 4:2:0).
const MAX_PB_CHROMA: usize = MAX_PB_SIZE * MAX_PB_SIZE;

/// Length of the separable-filter intermediate buffer: enough rows for a
/// 64-row block plus the 8-tap luma filter margin (the 4-tap chroma margin
/// is smaller), at a fixed `MAX_PB_SIZE` stride.
pub(crate) const MC_TMP_LEN: usize = QPEL_TMP_ROWS * MAX_PB_SIZE;

/// Reusable motion compensation scratch space. `PictureState` owns one and
/// lends it to every prediction block, so no per-block buffer is
/// zero-filled. That zero-filling used to dominate the cost of small
/// blocks: a bi-predicted block cleared six 16 KB stack arrays plus a filter
/// temporary on every call. Every region is fully written before it is
/// read, so stale contents left by earlier blocks are never observed.
pub struct McScratch<P: Pixel> {
    /// L0 luma intermediate (bi-pred), or the weighted uni-pred luma.
    luma0: Vec<i16>,
    /// L1 luma intermediate (bi-pred).
    luma1: Vec<i16>,
    /// L0 Cb / Cr intermediates (bi-pred), or the weighted uni-pred chroma.
    cb0: Vec<i16>,
    cr0: Vec<i16>,
    /// L1 Cb / Cr intermediates (bi-pred).
    cb1: Vec<i16>,
    cr1: Vec<i16>,
    /// Separable 2-D filter intermediate, shared by luma and chroma.
    tmp: Vec<i16>,
    /// Padded copy of the reference window for blocks near the picture
    /// edge (see `ref_window`).
    edge: Vec<P>,
}

impl<P: Pixel> Default for McScratch<P> {
    fn default() -> Self {
        Self {
            luma0: Vec::new(),
            luma1: Vec::new(),
            cb0: Vec::new(),
            cr0: Vec::new(),
            cb1: Vec::new(),
            cr1: Vec::new(),
            tmp: Vec::new(),
            edge: Vec::new(),
        }
    }
}

impl<P: Pixel> McScratch<P> {
    /// Allocate buffers sized for the largest prediction block (64×64, and
    /// 64×64 chroma for 4:4:4). `Default` gives empty buffers instead, which
    /// is only used as the placeholder while the scratch is lent out.
    pub fn new() -> Self {
        Self {
            luma0: vec![0; MAX_PB_LUMA],
            luma1: vec![0; MAX_PB_LUMA],
            cb0: vec![0; MAX_PB_CHROMA],
            cr0: vec![0; MAX_PB_CHROMA],
            cb1: vec![0; MAX_PB_CHROMA],
            cr1: vec![0; MAX_PB_CHROMA],
            tmp: vec![0; MC_TMP_LEN],
            edge: vec![P::zero(); MC_EDGE_LEN],
        }
    }
}

/// Chroma motion vector component (spec 8.5.3.2.10): `mvLX * 2 / SubWidthC`
/// (or `SubHeightC`), in units of 1/8 chroma sample. `shift` is
/// `log2(SubWidthC)` — 1 for 4:2:0 (chroma MV equals the luma MV, now in
/// eighth-chroma-sample units) or 0 for 4:4:4 (twice the luma MV, so only
/// even eighth-sample fractions occur). Widened to i32 since `mv * 2` can
/// exceed the i16 range.
#[inline]
fn chroma_mv(mv: i16, shift: u32) -> i32 {
    (mv as i32 * 2) >> shift
}

/// Fetch a reference sample with boundary clamping (spec 8.5.3.2.1).
/// Generic over `P: Pixel` for multi-bit-depth support.
#[inline]
fn ref_sample<P: Pixel>(plane: &[P], stride: usize, x: i32, y: i32, w: i32, h: i32) -> i32 {
    if w <= 0 || h <= 0 {
        return 0;
    }
    let cx = x.clamp(0, w - 1) as usize;
    let cy = y.clamp(0, h - 1) as usize;
    let idx = cy * stride + cx;
    // `plane` ends at the rows `ref_rows` asked for; a clamped in-picture
    // sample past its end means that row count was too small.
    #[cfg(test)]
    assert!(
        idx < plane.len(),
        "MC reads reference row {cy}, past the rows it waited for"
    );
    if idx >= plane.len() {
        return 0;
    }
    plane[idx].to_i32()
}

/// Run `f(row, j)` on each destination row `j` of a `w × h` block whose
/// top-left is `plane[off]`. A row that would fall outside `plane` ends the
/// loop instead of panicking (malformed-stream safety); every row of a
/// well-formed block lies inside the CTU-aligned plane.
#[inline(always)]
fn put_rows<P: Pixel>(
    plane: &mut SharedTable<P>,
    off: usize,
    stride: usize,
    w: usize,
    h: usize,
    mut f: impl FnMut(&mut [P], usize),
) {
    // One reference per output row, covering exactly the block's samples:
    // with wavefront decoding other threads use the rest of the plane.
    for j in 0..h {
        let start = off + j * stride;
        if start + w > plane.len() {
            break;
        }
        f(&mut plane[start..start + w], j);
    }
}

/// Explicit weighted uni-prediction (spec 8.5.3.3.4.3) of one block:
/// `dst = clip(((pred * weight + round) >> shift) + offset)`. `wt` is
/// `(weight, round, shift, offset)`.
#[inline(always)]
fn put_weighted<P: Pixel>(
    plane: &mut SharedTable<P>,
    off: usize,
    stride: usize,
    pred: &[i16],
    (w, h): (usize, usize),
    (weight, round, shift, offset): (i32, i32, i32, i32),
    bit_depth: u8,
) {
    put_rows(plane, off, stride, w, h, |d, j| {
        for (o, &v) in d.iter_mut().zip(&pred[j * w..][..w]) {
            let v = v as i32;
            *o = P::from_i32_clamped(((v * weight + round) >> shift) + offset, bit_depth);
        }
    });
}

/// Motion compensation core shared by the luma (`N = 8`, quarter-sample)
/// and chroma (`N = 4`, eighth-sample) entry points.
///
/// Locates the reference window with `ref_window` (the block plus
/// `N/2 - 1` samples before and `N/2` after on each axis with a fractional
/// phase), interpolates the block into the spec's 14-bit intermediate
/// domain (`predSamplesLX`, spec 8.5.3.3.3), one row at a time through
/// `sink`. `pos` is `(x_int, y_int, x_frac, y_frac)`.
#[allow(clippy::too_many_arguments)]
#[inline(always)]
fn mc_block<P: Pixel, const N: usize>(
    filters: &[[i16; N]],
    ref_plane: &[P],
    ref_stride: usize,
    ref_w: i32,
    ref_h: i32,
    pos: (i32, i32, usize, usize),
    w: usize,
    h: usize,
    bit_depth: u8,
    tmp: &mut [i16],
    edge: &mut [P],
    sink: &mut impl RowSink,
) {
    let (x_int, y_int, x_frac, y_frac) = pos;
    let (before, after) = ((N / 2 - 1) as i32, (N / 2) as i32);
    let (bx, ax) = if x_frac != 0 { (before, after) } else { (0, 0) };
    let (by, ay) = if y_frac != 0 { (before, after) } else { (0, 0) };
    let (src, src_stride, org) = ref_window(
        ref_plane,
        ref_stride,
        ref_w,
        ref_h,
        x_int - bx,
        y_int - by,
        w + (bx + ax) as usize,
        h + (by + ay) as usize,
        edge,
        bit_depth,
    );
    let hf = (x_frac != 0).then(|| &filters[x_frac]);
    let vf = (y_frac != 0).then(|| &filters[y_frac]);
    interp_block::<P, N>(src, src_stride, org, w, h, hf, vf, bit_depth, tmp, sink);
}

/// Interpolate a `w × h` block into 14-bit intermediate samples, one row at
/// a time. `src[org + r * src_stride + c]` is sample `(c, r)` of the
/// reference window. `hf` / `vf` are the horizontal / vertical filters for
/// a fractional phase on that axis, `None` for an integer position.
///
/// Every inner loop is tap-major over contiguous row slices — a plain
/// multiply-accumulate of one coefficient against a run of samples — which
/// LLVM turns into SIMD code. The arithmetic matches spec 8.5.3.3.3.1 /
/// 8.5.3.3.3.2: single-axis results and the horizontal pass of a 2-D
/// filter are shifted by `bit_depth - 8`, the vertical pass of a 2-D filter
/// by 6, and integer positions are scaled by `14 - bit_depth`.
#[allow(clippy::too_many_arguments)]
#[inline(always)]
fn interp_block<P: Pixel, const N: usize>(
    src: &[P],
    src_stride: usize,
    org: usize,
    w: usize,
    h: usize,
    hf: Option<&[i16; N]>,
    vf: Option<&[i16; N]>,
    bit_depth: u8,
    tmp: &mut [i16],
    sink: &mut impl RowSink,
) {
    let shift1 = (bit_depth as u32).saturating_sub(8);
    match (hf, vf) {
        (None, None) => {
            let shift3 = crate::pixel::mc_shift(bit_depth) as u32;
            for j in 0..h {
                let s = &src[org + j * src_stride..][..w];
                for (o, &p) in sink.row(j, w).iter_mut().zip(s) {
                    *o = (p.to_i32() << shift3) as i16;
                }
                sink.done(j);
            }
        }
        (Some(f), None) => {
            for j in 0..h {
                let row = sink.row(j, w);
                filter_h(row, &src[org + j * src_stride..][..w + N - 1], f, shift1);
                sink.done(j);
            }
        }
        (None, Some(f)) => {
            for j in 0..h {
                let row = sink.row(j, w);
                filter_v_px(row, src, org + j * src_stride, src_stride, f, shift1);
                sink.done(j);
            }
        }
        (Some(fh), Some(fv)) => {
            debug_assert!(tmp.len() >= MC_TMP_LEN);
            for r in 0..h + N - 1 {
                filter_h(
                    &mut tmp[r * MAX_PB_SIZE..][..w],
                    &src[org + r * src_stride..][..w + N - 1],
                    fh,
                    shift1,
                );
            }
            for j in 0..h {
                let row = sink.row(j, w);
                filter_v_i16(row, tmp, j * MAX_PB_SIZE, MAX_PB_SIZE, fv, 6);
                sink.done(j);
            }
        }
    }
}

/// Largest `Σ |coefficient|` over all luma and chroma sub-pel filters
/// (the half-sample luma filter: 1+4+11+40+40+11+4+1). With 8-bit samples
/// one filter pass is bounded by 112 × 255 = 28,560, so it fits in i16.
const MAX_FILTER_ABS_SUM: i32 = 112;
const _: () = assert!(MAX_FILTER_ABS_SUM * 255 <= i16::MAX as i32);

/// N-tap filter of one row of pixels: `out[i] = (Σ f[k] * src(k)[i]) >> shift`,
/// where `src(k)` is the `w`-sample row slice for tap `k`. 8-bit samples
/// accumulate in i16 (exact, see `MAX_FILTER_ABS_SUM`), which doubles the
/// SIMD lanes per instruction; 16-bit samples accumulate in i32.
#[inline(always)]
fn filter_px<'a, P: Pixel, const N: usize>(
    out: &mut [i16],
    f: &[i16; N],
    shift: u32,
    src: impl Fn(usize) -> &'a [P],
) {
    let w = out.len();
    let rows: [&[P]; N] = std::array::from_fn(|k| &src(k)[..w]);
    // Tap-major over fixed 8-lane chunks: the accumulators stay in
    // registers (no per-row array to initialise) and each tap is one
    // multiply-add across the chunk; the tail handles narrow chroma
    // blocks (2, 4 or 6 wide).
    let chunks = w / 8;
    if P::BITS == 8 {
        for c in 0..chunks {
            let mut acc = [0i16; 8];
            for (k, row) in rows.iter().enumerate() {
                let r: &[P; 8] = row[c * 8..c * 8 + 8].try_into().unwrap();
                for l in 0..8 {
                    acc[l] += f[k] * r[l].to_i32() as i16;
                }
            }
            let o: &mut [i16; 8] = (&mut out[c * 8..c * 8 + 8]).try_into().unwrap();
            for l in 0..8 {
                o[l] = acc[l] >> shift;
            }
        }
        for i in chunks * 8..w {
            let mut a = 0i16;
            for k in 0..N {
                a += f[k] * rows[k][i].to_i32() as i16;
            }
            out[i] = a >> shift;
        }
    } else {
        // Up to 12-bit samples: the sum needs i32, the shifted result
        // (shift = BitDepth - 8) fits i16.
        for c in 0..chunks {
            let mut acc = [0i32; 8];
            for (k, row) in rows.iter().enumerate() {
                let coef = f[k] as i32;
                let r: &[P; 8] = row[c * 8..c * 8 + 8].try_into().unwrap();
                for l in 0..8 {
                    acc[l] += coef * r[l].to_i32();
                }
            }
            let o: &mut [i16; 8] = (&mut out[c * 8..c * 8 + 8]).try_into().unwrap();
            for l in 0..8 {
                o[l] = (acc[l] >> shift) as i16;
            }
        }
        for i in chunks * 8..w {
            let mut a = 0i32;
            for k in 0..N {
                a += f[k] as i32 * rows[k][i].to_i32();
            }
            out[i] = (a >> shift) as i16;
        }
    }
}

/// Horizontal N-tap filter of one row: `out[i] = (Σ f[k] * src[i + k]) >> shift`.
#[inline(always)]
fn filter_h<P: Pixel, const N: usize>(out: &mut [i16], src: &[P], f: &[i16; N], shift: u32) {
    let w = out.len();
    filter_px(out, f, shift, |k| &src[k..k + w]);
}

/// Vertical N-tap filter over pixel rows starting at `src[base]`:
/// `out[i] = (Σ f[k] * src[base + k * stride + i]) >> shift`.
#[inline(always)]
fn filter_v_px<P: Pixel, const N: usize>(
    out: &mut [i16],
    src: &[P],
    base: usize,
    stride: usize,
    f: &[i16; N],
    shift: u32,
) {
    let w = out.len();
    filter_px(out, f, shift, |k| &src[base + k * stride..][..w]);
}

/// Vertical N-tap filter over 16-bit intermediate rows (second pass of a
/// 2-D filter): `out[i] = (Σ f[k] * src[base + k * stride + i]) >> shift`,
/// accumulated in i32 and stored as 16 bits, as HM and FFmpeg store it.
#[inline(always)]
fn filter_v_i16<const N: usize>(
    out: &mut [i16],
    src: &[i16],
    base: usize,
    stride: usize,
    f: &[i16; N],
    shift: u32,
) {
    let w = out.len();
    let rows: [&[i16]; N] = std::array::from_fn(|k| &src[base + k * stride..][..w]);
    // Tap-major over fixed 8-lane chunks: the i32 accumulators stay in
    // registers (no per-row array to initialise) and each tap is one
    // widening multiply-add across the chunk.
    let chunks = w / 8;
    for c in 0..chunks {
        let mut acc = [0i32; 8];
        for (k, row) in rows.iter().enumerate() {
            let coef = f[k] as i32;
            let r: &[i16; 8] = row[c * 8..c * 8 + 8].try_into().unwrap();
            for l in 0..8 {
                acc[l] += coef * r[l] as i32;
            }
        }
        let o: &mut [i16; 8] = (&mut out[c * 8..c * 8 + 8]).try_into().unwrap();
        for l in 0..8 {
            o[l] = (acc[l] >> shift) as i16;
        }
    }
    // Narrow chroma blocks (2, 4 or 6 wide) and any tail.
    for i in chunks * 8..w {
        let mut a = 0i32;
        for k in 0..N {
            a += f[k] as i32 * rows[k][i] as i32;
        }
        out[i] = (a >> shift) as i16;
    }
}

/// Destination for the rows `interp_block` produces. `row(j, w)` gives the
/// buffer to write row `j` into; `done(j)` is called once it is complete.
trait RowSink {
    fn row(&mut self, j: usize, w: usize) -> &mut [i16];
    fn done(&mut self, j: usize);
}

/// Writes 14-bit intermediates straight into a block of i16s
/// (bi-prediction and weighted-prediction inputs), with no extra copy.
struct I16Sink<'a> {
    dst: &'a mut [i16],
    stride: usize,
}

impl RowSink for I16Sink<'_> {
    #[inline(always)]
    fn row(&mut self, j: usize, w: usize) -> &mut [i16] {
        &mut self.dst[j * self.stride..][..w]
    }
    #[inline(always)]
    fn done(&mut self, _j: usize) {}
}

/// Rounds each row of 14-bit intermediates to pixels:
/// `dst[j * stride + i] = clip((row[i] + offset) >> shift)`. A row that
/// would fall outside `dst` is skipped rather than panicking on malformed
/// input.
struct PixelSink<'a, P: Pixel> {
    dst: &'a mut [P],
    stride: usize,
    offset: i32,
    shift: i32,
    bit_depth: u8,
    buf: [i16; MAX_PB_SIZE],
    w: usize,
}

impl<'a, P: Pixel> PixelSink<'a, P> {
    #[inline(always)]
    fn new(dst: &'a mut [P], stride: usize, offset: i32, shift: i32, bit_depth: u8) -> Self {
        Self {
            dst,
            stride,
            offset,
            shift,
            bit_depth,
            buf: [0; MAX_PB_SIZE],
            w: 0,
        }
    }
}

impl<P: Pixel> RowSink for PixelSink<'_, P> {
    #[inline(always)]
    fn row(&mut self, _j: usize, w: usize) -> &mut [i16] {
        self.w = w;
        &mut self.buf[..w]
    }
    #[inline(always)]
    fn done(&mut self, j: usize) {
        let start = j * self.stride;
        let (offset, shift, bd) = (self.offset, self.shift, self.bit_depth);
        if let Some(d) = self.dst.get_mut(start..start + self.w) {
            for (o, &v) in d.iter_mut().zip(&self.buf[..self.w]) {
                *o = P::from_i32_clamped((v as i32 + offset) >> shift, bd);
            }
        }
    }
}

/// Default bi-prediction (spec 8.5.3.3.4.2):
/// `dst[i] = clip((a[i] + b[i] + offset) >> shift)`, in 16-bit lanes with
/// saturating adds. Exact after the clip: `max << shift` is at most
/// 255 << 7 = 32,640 (8-bit), 1023 << 5 (10-bit) or 4095 << 3 (12-bit),
/// within i16, so a sum that saturates rounds to the same clipped sample as
/// the true one. Twice the lanes per instruction of the i32 form.
#[inline(always)]
fn bi_average_row<P: Pixel>(
    dst: &mut [P],
    a: &[i16],
    b: &[i16],
    offset: i16,
    shift: u32,
    bit_depth: u8,
) {
    let max = ((1i32 << bit_depth) - 1) as i16;
    for ((o, &x), &y) in dst.iter_mut().zip(a).zip(b) {
        let v = x.saturating_add(y).saturating_add(offset) >> shift;
        *o = P::from_i32_clamped(v.clamp(0, max) as i32, bit_depth);
    }
}

/// Luma rows of a reference that motion compensation of a block at
/// `y0` of height `h` with vertical MV `mv_y` (quarter samples) may read,
/// for luma and chroma: the block, its MV, and the interpolation taps
/// below it (4 luma rows; 2 chroma rows, i.e. 4 luma rows in 4:2:0), with
/// margin. Clamped to the picture by `ref_plane`.
#[inline(always)]
fn ref_rows(y0: u32, h: usize, mv_y: i16) -> usize {
    (y0 as i64 + h as i64 + (mv_y as i64 >> 2) + 8).max(1) as usize
}

/// Plane `c` (0 = luma, 1 = Cb, 2 = Cr) of a reference picture, up to luma
/// row `rows` (see `ref_rows`). With frame threading the reference may
/// still be decoding: this waits until those rows are final (deblocked
/// and SAO-filtered, never written again) and returns a slice ending
/// there, so the block never sees a row still changing. A block whose MV
/// reaches past the picture's bottom waits for the whole picture.
#[inline(always)]
fn ref_plane<P: Pixel>(pic: &DecodedPicture, c: usize, rows: usize) -> &[P] {
    let rows = rows.clamp(1, pic.height as usize);
    pic.progress.wait_final_rows(rows);
    let plane = &P::picture_planes(&pic.planes)[c];
    let (stride, rows) = if c == 0 {
        (pic.y_stride, rows)
    } else {
        (pic.uv_stride, rows.div_ceil(1 << pic.chroma_shift_h))
    };
    &plane[0..(rows * stride).min(plane.len())]
}

/// Row stride of the padded edge buffer: a 64-wide block plus the 7-sample
/// luma filter margin.
pub(crate) const MC_EDGE_STRIDE: usize = MAX_PB_SIZE + 7;
/// Length of the padded edge buffer: 71 rows of `MC_EDGE_STRIDE`.
pub(crate) const MC_EDGE_LEN: usize = MC_EDGE_STRIDE * (MAX_PB_SIZE + 7);

/// Locate the reference samples a block's filter needs: the `ww × wh`
/// window whose top-left is `(wx0, wy0)` in the reference plane.
///
/// Returns `(source, stride, origin)` such that reference sample
/// `(wx0 + dx, wy0 + dy)` is `source[origin + dy * stride + dx]`.
///
/// When the window lies entirely inside the picture (the common case) the
/// source is the reference plane itself, so the filter loops read memory
/// directly with no per-tap clamping. Otherwise the window is copied into
/// `edge` through `ref_sample`, which applies the spec 8.5.3.2.1 boundary
/// clamp, and the filters read the padded copy. Either way every tap sees
/// exactly the value `ref_sample` would return.
#[allow(clippy::too_many_arguments)]
#[inline]
fn ref_window<'a, P: Pixel>(
    ref_plane: &'a [P],
    ref_stride: usize,
    ref_w: i32,
    ref_h: i32,
    wx0: i32,
    wy0: i32,
    ww: usize,
    wh: usize,
    edge: &'a mut [P],
    bit_depth: u8,
) -> (&'a [P], usize, usize) {
    let inside = ww > 0
        && wh > 0
        && wx0 >= 0
        && wy0 >= 0
        && wx0 as i64 + ww as i64 <= ref_w as i64
        && wy0 as i64 + wh as i64 <= ref_h as i64
        && (wy0 as usize + wh - 1) * ref_stride + wx0 as usize + ww - 1 < ref_plane.len();
    if inside {
        return (
            ref_plane,
            ref_stride,
            wy0 as usize * ref_stride + wx0 as usize,
        );
    }
    debug_assert!(ww <= MC_EDGE_STRIDE && wh * MC_EDGE_STRIDE <= edge.len());
    for r in 0..wh {
        for c in 0..ww {
            // `ref_sample` returns an in-range sample (or 0), so this
            // conversion never actually clamps.
            edge[r * MC_EDGE_STRIDE + c] = P::from_i32_clamped(
                ref_sample(
                    ref_plane,
                    ref_stride,
                    wx0 + c as i32,
                    wy0 + r as i32,
                    ref_w,
                    ref_h,
                ),
                bit_depth,
            );
        }
    }
    (edge, MC_EDGE_STRIDE, 0)
}

/// Uni-directional luma motion compensation (8-tap filter).
///
/// Writes `nPbW * nPbH` prediction samples into `dst` at stride `dst_stride`.
/// `x0, y0` are the PU's top-left luma coordinates in the current picture.
/// `mv` is the quarter-pel motion vector. `bit_depth` is the luma bit depth.
///
/// Generic over `P: Pixel` for multi-bit-depth support.
#[allow(clippy::too_many_arguments)]
pub fn mc_luma<P: Pixel>(
    dst: &mut [P],
    dst_stride: usize,
    ref_plane: &[P],
    ref_stride: usize,
    ref_w: i32,
    ref_h: i32,
    x0: i32,
    y0: i32,
    n_pb_w: usize,
    n_pb_h: usize,
    mv_x: i16,
    mv_y: i16,
    bit_depth: u8,
    tmp: &mut [i16],
    edge: &mut [P],
) {
    let x_frac = (mv_x & 3) as usize;
    let y_frac = (mv_y & 3) as usize;
    let x_int = x0 + (mv_x >> 2) as i32;
    let y_int = y0 + (mv_y >> 2) as i32;
    let shift = crate::pixel::mc_shift(bit_depth) as i32;
    let offset = crate::pixel::mc_offset(bit_depth);
    mc_block::<P, 8>(
        &QPEL_FILTER,
        ref_plane,
        ref_stride,
        ref_w,
        ref_h,
        (x_int, y_int, x_frac, y_frac),
        n_pb_w,
        n_pb_h,
        bit_depth,
        tmp,
        edge,
        &mut PixelSink::new(dst, dst_stride, offset, shift, bit_depth),
    );
}

/// Luma MC producing the spec's 14-bit intermediates (`predSamplesLX`,
/// spec 8.5.3.3.3) for bi-prediction / weighted prediction, without the
/// final rounding and clip. Bi-prediction combines two of them as
/// `clip((L0 + L1 + bipred_offset) >> bipred_shift)`.
///
/// Stored as i16, as HM and FFmpeg store them: a first filter pass is
/// shifted right by `BitDepth - 8`, so it stays below 112 * 255 (28,560)
/// for every bit depth up to 12, and integer positions shifted left by
/// `14 - BitDepth` fit as well. The second pass of a 2-D filter sums in i32
/// before its shift. Half the memory traffic of i32 and twice the SIMD
/// lanes in the combine loops.
#[allow(clippy::too_many_arguments)]
pub fn mc_luma_i16<P: Pixel>(
    dst: &mut [i16],
    dst_stride: usize,
    ref_plane: &[P],
    ref_stride: usize,
    ref_w: i32,
    ref_h: i32,
    x0: i32,
    y0: i32,
    n_pb_w: usize,
    n_pb_h: usize,
    mv_x: i16,
    mv_y: i16,
    bit_depth: u8,
    tmp: &mut [i16],
    edge: &mut [P],
) {
    let x_frac = (mv_x & 3) as usize;
    let y_frac = (mv_y & 3) as usize;
    let x_int = x0 + (mv_x >> 2) as i32;
    let y_int = y0 + (mv_y >> 2) as i32;
    mc_block::<P, 8>(
        &QPEL_FILTER,
        ref_plane,
        ref_stride,
        ref_w,
        ref_h,
        (x_int, y_int, x_frac, y_frac),
        n_pb_w,
        n_pb_h,
        bit_depth,
        tmp,
        edge,
        &mut I16Sink {
            dst,
            stride: dst_stride,
        },
    );
}

/// Chroma MC producing 14-bit i16 intermediates for bi-prediction /
/// weighted prediction (see `mc_luma_i16`).
#[allow(clippy::too_many_arguments)]
pub fn mc_chroma_i16<P: Pixel>(
    dst: &mut [i16],
    dst_stride: usize,
    ref_plane: &[P],
    ref_stride: usize,
    ref_w: i32,
    ref_h: i32,
    x0: i32,
    y0: i32,
    n_pb_w: usize,
    n_pb_h: usize,
    mv_x: i32,
    mv_y: i32,
    bit_depth: u8,
    tmp: &mut [i16],
    edge: &mut [P],
) {
    // Chroma MV in eighth-chroma-sample units (see `chroma_mv`).
    let x_frac = (mv_x & 7) as usize;
    let y_frac = (mv_y & 7) as usize;
    let x_int = x0 + (mv_x >> 3);
    let y_int = y0 + (mv_y >> 3);
    mc_block::<P, 4>(
        &EPEL_FILTER,
        ref_plane,
        ref_stride,
        ref_w,
        ref_h,
        (x_int, y_int, x_frac, y_frac),
        n_pb_w,
        n_pb_h,
        bit_depth,
        tmp,
        edge,
        &mut I16Sink {
            dst,
            stride: dst_stride,
        },
    );
}

/// Uni-directional chroma motion compensation (4-tap filter).
///
/// `x0_c, y0_c` are chroma-plane coordinates (i.e. already halved for 4:2:0).
/// `mv_x_c, mv_y_c` are the halved MV components (still in eighth-pel units).
/// `bit_depth` is the chroma bit depth.
///
/// Generic over `P: Pixel` for multi-bit-depth support.
#[allow(clippy::too_many_arguments)]
pub fn mc_chroma<P: Pixel>(
    dst: &mut [P],
    dst_stride: usize,
    ref_plane: &[P],
    ref_stride: usize,
    ref_w: i32,
    ref_h: i32,
    x0_c: i32,
    y0_c: i32,
    n_pb_w_c: usize,
    n_pb_h_c: usize,
    mv_x_c: i32,
    mv_y_c: i32,
    bit_depth: u8,
    tmp: &mut [i16],
    edge: &mut [P],
) {
    // Chroma MV in eighth-chroma-sample units (see `chroma_mv`).
    let x_frac = (mv_x_c & 7) as usize;
    let y_frac = (mv_y_c & 7) as usize;
    let x_int = x0_c + (mv_x_c >> 3);
    let y_int = y0_c + (mv_y_c >> 3);
    let shift = crate::pixel::mc_shift(bit_depth) as i32;
    let offset = crate::pixel::mc_offset(bit_depth);
    mc_block::<P, 4>(
        &EPEL_FILTER,
        ref_plane,
        ref_stride,
        ref_w,
        ref_h,
        (x_int, y_int, x_frac, y_frac),
        n_pb_w_c,
        n_pb_h_c,
        bit_depth,
        tmp,
        edge,
        &mut PixelSink::new(dst, dst_stride, offset, shift, bit_depth),
    );
}

/// Perform motion compensation for a single PU in an inter CU.
///
/// Reads the MV field from `tab_mvf` at position `(x0, y0)` and writes
/// the prediction into `state.y_plane`, `state.u_plane`, `state.v_plane`.
///
/// For uni-prediction (L0 or L1 only), the prediction is written directly.
/// For bi-prediction (both L0 and L1), the two predictions are averaged:
/// `dst = (predL0 + predL1 + 1) >> 1`.
#[allow(clippy::too_many_arguments)]
pub fn motion_compensation_pu<P: Pixel>(
    state: &mut PictureState<P>,
    scratch: &mut McScratch<P>,
    ref_frames_l0: &[Arc<DecodedPicture>],
    ref_frames_l1: &[Arc<DecodedPicture>],
    x0: u32,
    y0: u32,
    n_pb_w: u32,
    n_pb_h: u32,
    weighted_pred_flag: bool,
    pred_weight_table: &crate::slice::PredWeightTable,
) {
    // Read the MV field from the top-left min-PU of this PU.
    let x_pu = (x0 >> state.log2_min_pu_size) as usize;
    let y_pu = (y0 >> state.log2_min_pu_size) as usize;
    let mvf = state.tab_mvf[y_pu * state.min_pu_width + x_pu];
    let McScratch {
        luma0,
        luma1,
        cb0,
        cr0,
        cb1,
        cr1,
        tmp,
        edge,
    } = scratch;

    if mvf.pred_flag == 0 {
        // Intra PU in an inter slice — MC is not applicable. The intra
        // prediction path has already written the prediction.
        return;
    }

    let y_stride = state.y_stride;
    let uv_stride = state.uv_stride;
    let w = n_pb_w as usize;
    let h = n_pb_h as usize;
    // Chroma geometry: log2(SubWidthC) / log2(SubHeightC). 1 for 4:2:0
    // (half-size chroma blocks), 0 for 4:4:4 (chroma matches luma).
    let csw = state.chroma_shift_w;
    let csh = state.chroma_shift_h;
    let w_c = w >> csw;
    let h_c = h >> csh;
    let x0_c = (x0 >> csw) as i32;
    let y0_c = (y0 >> csh) as i32;
    let c_off = ((y0 >> csh) as usize) * uv_stride + ((x0 >> csw) as usize);

    let is_l0 = mvf.pred_flag & 1 != 0;
    let is_l1 = mvf.pred_flag & 2 != 0;
    let is_bi = is_l0 && is_l1;

    // Luma and chroma may have different bit depths (BitDepthY, BitDepthC).
    let bit_depth = state.bit_depth;
    let bit_depth_c = state.bit_depth_c;

    if is_bi {
        // Bi-prediction: compute the 14-bit L0 and L1 intermediates (i16,
        // before the final clip), then combine as specified by the HEVC spec:
        //   output = clip((L0 + L1 + bipred_offset) >> bipred_shift)
        // For 8-bit: shift = 7, offset = 64. For 10-bit: shift = 5, offset = 16.
        let pred_l0_y = &mut luma0[..];
        let pred_l1_y = &mut luma1[..];
        let pred_l0_u = &mut cb0[..];
        let pred_l0_v = &mut cr0[..];
        let pred_l1_u = &mut cb1[..];
        let pred_l1_v = &mut cr1[..];

        // L0
        let l0_idx = mvf.ref_idx[0] as usize;
        if l0_idx >= ref_frames_l0.len() {
            return;
        }
        let ref_pic_l0 = &ref_frames_l0[l0_idx];
        {
            let rw = ref_pic_l0.width as i32;
            let rh = ref_pic_l0.height as i32;
            let rs = ref_pic_l0.y_stride;
            mc_luma_i16::<P>(
                pred_l0_y,
                w,
                ref_plane::<P>(ref_pic_l0, 0, ref_rows(y0, h, mvf.mv[0].y)),
                rs,
                rw,
                rh,
                x0 as i32,
                y0 as i32,
                w,
                h,
                mvf.mv[0].x,
                mvf.mv[0].y,
                bit_depth,
                &mut tmp[..],
                &mut edge[..],
            );
            let rw_c = (ref_pic_l0.width >> csw) as i32;
            let rh_c = (ref_pic_l0.height >> csh) as i32;
            let rs_c = ref_pic_l0.uv_stride;
            let mv_x_c = chroma_mv(mvf.mv[0].x, csw);
            let mv_y_c = chroma_mv(mvf.mv[0].y, csh);
            mc_chroma_i16::<P>(
                pred_l0_u,
                w_c,
                ref_plane::<P>(ref_pic_l0, 1, ref_rows(y0, h, mvf.mv[0].y)),
                rs_c,
                rw_c,
                rh_c,
                x0_c,
                y0_c,
                w_c,
                h_c,
                mv_x_c,
                mv_y_c,
                bit_depth_c,
                &mut tmp[..],
                &mut edge[..],
            );
            mc_chroma_i16::<P>(
                pred_l0_v,
                w_c,
                ref_plane::<P>(ref_pic_l0, 2, ref_rows(y0, h, mvf.mv[0].y)),
                rs_c,
                rw_c,
                rh_c,
                x0_c,
                y0_c,
                w_c,
                h_c,
                mv_x_c,
                mv_y_c,
                bit_depth_c,
                &mut tmp[..],
                &mut edge[..],
            );
        }

        // L1
        let l1_idx = mvf.ref_idx[1] as usize;
        if l1_idx >= ref_frames_l1.len() {
            return;
        }
        let ref_pic_l1 = &ref_frames_l1[l1_idx];
        {
            let rw = ref_pic_l1.width as i32;
            let rh = ref_pic_l1.height as i32;
            let rs = ref_pic_l1.y_stride;
            mc_luma_i16::<P>(
                pred_l1_y,
                w,
                ref_plane::<P>(ref_pic_l1, 0, ref_rows(y0, h, mvf.mv[1].y)),
                rs,
                rw,
                rh,
                x0 as i32,
                y0 as i32,
                w,
                h,
                mvf.mv[1].x,
                mvf.mv[1].y,
                bit_depth,
                &mut tmp[..],
                &mut edge[..],
            );
            let rw_c = (ref_pic_l1.width >> csw) as i32;
            let rh_c = (ref_pic_l1.height >> csh) as i32;
            let rs_c = ref_pic_l1.uv_stride;
            let mv_x_c = chroma_mv(mvf.mv[1].x, csw);
            let mv_y_c = chroma_mv(mvf.mv[1].y, csh);
            mc_chroma_i16::<P>(
                pred_l1_u,
                w_c,
                ref_plane::<P>(ref_pic_l1, 1, ref_rows(y0, h, mvf.mv[1].y)),
                rs_c,
                rw_c,
                rh_c,
                x0_c,
                y0_c,
                w_c,
                h_c,
                mv_x_c,
                mv_y_c,
                bit_depth_c,
                &mut tmp[..],
                &mut edge[..],
            );
            mc_chroma_i16::<P>(
                pred_l1_v,
                w_c,
                ref_plane::<P>(ref_pic_l1, 2, ref_rows(y0, h, mvf.mv[1].y)),
                rs_c,
                rw_c,
                rh_c,
                x0_c,
                y0_c,
                w_c,
                h_c,
                mv_x_c,
                mv_y_c,
                bit_depth_c,
                &mut tmp[..],
                &mut edge[..],
            );
        }

        // Combine in i32: (L0 + L1 + bipred_offset) >> bipred_shift, then clip.
        let y_off = (y0 as usize) * y_stride + (x0 as usize);
        // Explicit weighted bi-prediction (spec 8.5.3.3.4.3) uses per-list
        // weights and offsets for each plane; otherwise the default average.
        let wt = pred_weight_table;
        let plane_weights = |c: usize| -> (i32, i32, i32, i32, i32) {
            let bd = if c == 0 { bit_depth } else { bit_depth_c };
            let ofs = if wt.high_precision_offsets {
                1
            } else {
                1i32 << (bd as i32 - 8)
            };
            let mc_sh = crate::pixel::mc_shift(bd) as i32;
            if c == 0 {
                (
                    wt.luma_weight_l0[l0_idx] as i32,
                    wt.luma_weight_l1[l1_idx] as i32,
                    wt.luma_offset_l0[l0_idx] as i32 * ofs,
                    wt.luma_offset_l1[l1_idx] as i32 * ofs,
                    wt.luma_log2_weight_denom as i32 + mc_sh,
                )
            } else {
                let k = c - 1;
                (
                    wt.chroma_weight_l0[l0_idx][k] as i32,
                    wt.chroma_weight_l1[l1_idx][k] as i32,
                    wt.chroma_offset_l0[l0_idx][k] as i32 * ofs,
                    wt.chroma_offset_l1[l1_idx][k] as i32 * ofs,
                    wt.chroma_log2_weight_denom as i32 + mc_sh,
                )
            }
        };
        let bi = |plane: &mut SharedTable<P>, c: usize, off, stride, a: &[i16], b: &[i16], w, h| {
            let bit_depth = if c == 0 { bit_depth } else { bit_depth_c };
            let bi_shift = crate::pixel::bipred_shift(bit_depth) as i32;
            let bi_offset = crate::pixel::bipred_offset(bit_depth);
            if weighted_pred_flag {
                let (w0, w1, o0, o1, log2wd) = plane_weights(c);
                let round = (o0 + o1 + 1) << log2wd;
                put_rows(plane, off, stride, w, h, |d, j| {
                    let (a, b) = (&a[j * w..][..w], &b[j * w..][..w]);
                    for ((o, &x), &y) in d.iter_mut().zip(a).zip(b) {
                        let (x, y) = (x as i32, y as i32);
                        *o = P::from_i32_clamped(
                            (x * w0 + y * w1 + round) >> (log2wd + 1),
                            bit_depth,
                        );
                    }
                })
            } else {
                put_rows(plane, off, stride, w, h, |d, j| {
                    bi_average_row(
                        d,
                        &a[j * w..][..w],
                        &b[j * w..][..w],
                        bi_offset as i16,
                        bi_shift as u32,
                        bit_depth,
                    );
                })
            }
        };
        bi(
            &mut state.y_plane,
            0,
            y_off,
            y_stride,
            pred_l0_y,
            pred_l1_y,
            w,
            h,
        );
        bi(
            &mut state.u_plane,
            1,
            c_off,
            uv_stride,
            pred_l0_u,
            pred_l1_u,
            w_c,
            h_c,
        );
        bi(
            &mut state.v_plane,
            2,
            c_off,
            uv_stride,
            pred_l0_v,
            pred_l1_v,
            w_c,
            h_c,
        );
    } else {
        // Uni-prediction: write directly into the picture planes.
        let (ref_list, mv, ref_idx, use_l0) = if is_l0 {
            let idx = mvf.ref_idx[0] as usize;
            if idx >= ref_frames_l0.len() {
                return; // malformed stream — ref index out of bounds
            }
            (&ref_frames_l0[idx], mvf.mv[0], idx, true)
        } else {
            let idx = mvf.ref_idx[1] as usize;
            if idx >= ref_frames_l1.len() {
                return;
            }
            (
                &ref_frames_l1[idx],
                mvf.mv[1],
                mvf.ref_idx[1] as usize,
                false,
            )
        };

        if !weighted_pred_flag {
            // Non-weighted uni-prediction: MC directly into the picture planes
            // (through `with_block`, which a wavefront worker needs).
            let row_exact = state.row_exact;
            let ref_y = ref_plane::<P>(ref_list, 0, ref_rows(y0, h, mv.y));
            let ref_u = ref_plane::<P>(ref_list, 1, ref_rows(y0, h, mv.y));
            let ref_v = ref_plane::<P>(ref_list, 2, ref_rows(y0, h, mv.y));
            let y_off = (y0 as usize) * y_stride + (x0 as usize);
            crate::cu_tree::with_block(
                &mut state.y_plane,
                &mut state.block_buf,
                row_exact,
                y_off,
                y_stride,
                (w, h),
                |dst, dst_stride| {
                    mc_luma::<P>(
                        dst,
                        dst_stride,
                        ref_y,
                        ref_list.y_stride,
                        ref_list.width as i32,
                        ref_list.height as i32,
                        x0 as i32,
                        y0 as i32,
                        w,
                        h,
                        mv.x,
                        mv.y,
                        bit_depth,
                        &mut tmp[..],
                        &mut edge[..],
                    )
                },
            );

            let ref_w_c = (ref_list.width >> csw) as i32;
            let ref_h_c = (ref_list.height >> csh) as i32;
            let ref_uv_stride = ref_list.uv_stride;
            let mv_x_c = chroma_mv(mv.x, csw);
            let mv_y_c = chroma_mv(mv.y, csh);

            crate::cu_tree::with_block(
                &mut state.u_plane,
                &mut state.block_buf,
                row_exact,
                c_off,
                uv_stride,
                (w_c, h_c),
                |dst, dst_stride| {
                    mc_chroma::<P>(
                        dst,
                        dst_stride,
                        ref_u,
                        ref_uv_stride,
                        ref_w_c,
                        ref_h_c,
                        x0_c,
                        y0_c,
                        w_c,
                        h_c,
                        mv_x_c,
                        mv_y_c,
                        bit_depth_c,
                        &mut tmp[..],
                        &mut edge[..],
                    )
                },
            );
            crate::cu_tree::with_block(
                &mut state.v_plane,
                &mut state.block_buf,
                row_exact,
                c_off,
                uv_stride,
                (w_c, h_c),
                |dst, dst_stride| {
                    mc_chroma::<P>(
                        dst,
                        dst_stride,
                        ref_v,
                        ref_uv_stride,
                        ref_w_c,
                        ref_h_c,
                        x0_c,
                        y0_c,
                        w_c,
                        h_c,
                        mv_x_c,
                        mv_y_c,
                        bit_depth_c,
                        &mut tmp[..],
                        &mut edge[..],
                    )
                },
            );
        } else {
            // Weighted uni-prediction: compute MC at i16 precision, then apply
            // weight/offset per HEVC spec 8.5.3.3.4.1 / FFmpeg put_hevc_qpel_uni_w.
            let wt = pred_weight_table;
            // Offsets are coded at 8-bit precision and scaled by
            // 1 << (BitDepth - 8) (spec 7.4.7.3, high_precision_offsets off).
            // Offsets are coded at 8-bit precision and scaled up, unless
            // high_precision_offsets_enabled_flag codes them at full depth.
            let scale = |bd: u8| {
                if wt.high_precision_offsets {
                    1
                } else {
                    1i32 << (bd as i32 - 8)
                }
            };
            let (ofs, ofs_c) = (scale(bit_depth), scale(bit_depth_c));
            let (luma_w, luma_o, chroma_w, chroma_o, luma_denom, chroma_denom) = if use_l0 {
                (
                    wt.luma_weight_l0[ref_idx] as i32,
                    wt.luma_offset_l0[ref_idx] as i32,
                    wt.chroma_weight_l0[ref_idx],
                    wt.chroma_offset_l0[ref_idx],
                    wt.luma_log2_weight_denom,
                    wt.chroma_log2_weight_denom,
                )
            } else {
                (
                    wt.luma_weight_l1[ref_idx] as i32,
                    wt.luma_offset_l1[ref_idx] as i32,
                    wt.chroma_weight_l1[ref_idx],
                    wt.chroma_offset_l1[ref_idx],
                    wt.luma_log2_weight_denom,
                    wt.chroma_log2_weight_denom,
                )
            };

            // Luma: MC into the i16 intermediate, then apply weight.
            let pred_y = &mut luma0[..];
            mc_luma_i16::<P>(
                pred_y,
                w,
                ref_plane::<P>(ref_list, 0, ref_rows(y0, h, mv.y)),
                ref_list.y_stride,
                ref_list.width as i32,
                ref_list.height as i32,
                x0 as i32,
                y0 as i32,
                w,
                h,
                mv.x,
                mv.y,
                bit_depth,
                &mut tmp[..],
                &mut edge[..],
            );
            // Apply weighted prediction: spec 8.5.3.3.4.1
            // For uni-pred weighted: log2WD = luma_log2_weight_denom + (bit_depth - 8)
            // For 8-bit: log2WD = luma_log2_weight_denom
            // FFmpeg's put_hevc_qpel_uni_w uses:
            //   (((val >> (14 - 8)) * weight + (1 << (log2WD + 14 - 8 - 1))) >> (log2WD + 14 - 8)) + offset
            // The intermediate from mc_luma_i16 is at (14 - bit_depth)
            // bits of extra precision. Weighted pred: clip((pred * weight +
            // round) >> (mc_shift + denom)) + offset.
            let mc_sh = crate::pixel::mc_shift(bit_depth) as i32;
            let shift = mc_sh + luma_denom as i32;
            let round = if shift > 0 { 1i32 << (shift - 1) } else { 0 };
            let y_off = (y0 as usize) * y_stride + (x0 as usize);
            put_weighted(
                &mut state.y_plane,
                y_off,
                y_stride,
                pred_y,
                (w, h),
                (luma_w, round, shift, luma_o * ofs),
                bit_depth,
            );

            // Chroma: similar weighted path.
            let ref_w_c = (ref_list.width >> csw) as i32;
            let ref_h_c = (ref_list.height >> csh) as i32;
            let ref_uv_stride = ref_list.uv_stride;
            let mv_x_c = chroma_mv(mv.x, csw);
            let mv_y_c = chroma_mv(mv.y, csh);
            let c_shift = crate::pixel::mc_shift(bit_depth_c) as i32 + chroma_denom as i32;
            let c_round = if c_shift > 0 {
                1i32 << (c_shift - 1)
            } else {
                0
            };

            let pred_u = &mut cb0[..];
            let pred_v = &mut cr0[..];
            mc_chroma_i16::<P>(
                pred_u,
                w_c,
                ref_plane::<P>(ref_list, 1, ref_rows(y0, h, mv.y)),
                ref_uv_stride,
                ref_w_c,
                ref_h_c,
                x0_c,
                y0_c,
                w_c,
                h_c,
                mv_x_c,
                mv_y_c,
                bit_depth_c,
                &mut tmp[..],
                &mut edge[..],
            );
            mc_chroma_i16::<P>(
                pred_v,
                w_c,
                ref_plane::<P>(ref_list, 2, ref_rows(y0, h, mv.y)),
                ref_uv_stride,
                ref_w_c,
                ref_h_c,
                x0_c,
                y0_c,
                w_c,
                h_c,
                mv_x_c,
                mv_y_c,
                bit_depth_c,
                &mut tmp[..],
                &mut edge[..],
            );

            for (plane, pred, c) in [
                (&mut state.u_plane, &*pred_u, 0),
                (&mut state.v_plane, &*pred_v, 1),
            ] {
                put_weighted(
                    plane,
                    c_off,
                    uv_stride,
                    pred,
                    (w_c, h_c),
                    (
                        chroma_w[c] as i32,
                        c_round,
                        c_shift,
                        chroma_o[c] as i32 * ofs_c,
                    ),
                    bit_depth_c,
                );
            }
        }
    }
}

/// Perform motion compensation for an entire inter CU, dispatching to each PU
/// based on the partition mode. This replaces `write_placeholder_prediction`.
#[allow(clippy::too_many_arguments)]
pub fn motion_compensation_cu<P: Pixel>(
    state: &mut PictureState<P>,
    ref_frames_l0: &[Arc<DecodedPicture>],
    ref_frames_l1: &[Arc<DecodedPicture>],
    x0: u32,
    y0: u32,
    cb_size: u32,
    part_mode: crate::cu_tree::PartMode,
    weighted_pred_flag: bool,
    pred_weight_table: &crate::slice::PredWeightTable,
) {
    use crate::cu_tree::PartMode;

    // Lend the picture's scratch buffers to every PU of this CU. The empty
    // placeholder left behind is never used: no other code touches
    // `mc_scratch` while it is out, and it is restored below.
    let mut scratch = std::mem::take(&mut state.mc_scratch);

    // Local helper to reduce repetition when threading weight params.
    macro_rules! mc_pu {
        ($x:expr, $y:expr, $w:expr, $h:expr) => {
            motion_compensation_pu(
                state,
                &mut scratch,
                ref_frames_l0,
                ref_frames_l1,
                $x,
                $y,
                $w,
                $h,
                weighted_pred_flag,
                pred_weight_table,
            )
        };
    }

    match part_mode {
        PartMode::Part2Nx2N => {
            mc_pu!(x0, y0, cb_size, cb_size);
        }
        PartMode::Part2NxN => {
            let half = cb_size / 2;
            mc_pu!(x0, y0, cb_size, half);
            mc_pu!(x0, y0 + half, cb_size, half);
        }
        PartMode::PartNx2N => {
            let half = cb_size / 2;
            mc_pu!(x0, y0, half, cb_size);
            mc_pu!(x0 + half, y0, half, cb_size);
        }
        PartMode::Part2NxnU => {
            let q = cb_size / 4;
            mc_pu!(x0, y0, cb_size, q);
            mc_pu!(x0, y0 + q, cb_size, cb_size - q);
        }
        PartMode::Part2NxnD => {
            let tq = cb_size * 3 / 4;
            mc_pu!(x0, y0, cb_size, tq);
            mc_pu!(x0, y0 + tq, cb_size, cb_size - tq);
        }
        PartMode::PartnLx2N => {
            let q = cb_size / 4;
            mc_pu!(x0, y0, q, cb_size);
            mc_pu!(x0 + q, y0, cb_size - q, cb_size);
        }
        PartMode::PartnRx2N => {
            let tq = cb_size * 3 / 4;
            mc_pu!(x0, y0, tq, cb_size);
            mc_pu!(x0 + tq, y0, cb_size - tq, cb_size);
        }
        PartMode::PartNxN => {
            let half = cb_size / 2;
            for pi in 0..2u32 {
                for pj in 0..2u32 {
                    mc_pu!(x0 + pj * half, y0 + pi * half, half, half);
                }
            }
        }
    }
    state.mc_scratch = scratch;
}

#[cfg(test)]
mod tests {
    /// The saturating i16 bi-prediction average equals the exact i32
    /// formula after clipping, for every bit depth up to 12 and inputs
    /// spanning (and beyond) the 14-bit intermediate range, including sums
    /// that overflow i16.
    #[test]
    fn bi_average_row_matches_exact_formula() {
        let samples: Vec<i16> = (-20000i32..=32767).step_by(97).map(|v| v as i16).collect();
        for bd in [8u8, 10, 12] {
            let shift = crate::pixel::bipred_shift(bd) as u32;
            let offset = crate::pixel::bipred_offset(bd);
            let max = (1i32 << bd) - 1;
            for &x in samples.iter().step_by(7) {
                let a = vec![x; samples.len()];
                let mut out = vec![0u16; samples.len()];
                super::bi_average_row(&mut out, &a, &samples, offset as i16, shift, bd);
                for (i, &y) in samples.iter().enumerate() {
                    let exact = ((x as i32 + y as i32 + offset) >> shift).clamp(0, max);
                    assert_eq!(out[i] as i32, exact, "bd {bd} x {x} y {y}");
                }
            }
        }
    }

    /// The i16 accumulation in `filter_px` is only exact if no filter's
    /// coefficient magnitudes sum to more than `MAX_FILTER_ABS_SUM`.
    #[test]
    fn filter_abs_sum_fits_i16_bound() {
        let abs_sum = |f: &[i16]| f.iter().map(|&c| (c as i32).abs()).sum::<i32>();
        let max = QPEL_FILTER
            .iter()
            .map(|f| abs_sum(f))
            .chain(EPEL_FILTER.iter().map(|f| abs_sum(f)))
            .max()
            .unwrap();
        assert_eq!(max, MAX_FILTER_ABS_SUM);
    }

    use super::*;

    /// Integer-pel copy should reproduce the reference exactly.
    #[test]
    fn mc_luma_integer_pel_copy() {
        let ref_w = 16usize;
        let ref_h = 16usize;
        let mut ref_plane = vec![0u8; ref_w * ref_h];
        // Fill with a gradient.
        for j in 0..ref_h {
            for i in 0..ref_w {
                ref_plane[j * ref_w + i] = (j * ref_w + i) as u8;
            }
        }

        let w = 4;
        let h = 4;
        let mut dst = vec![0u8; w * h];

        // Zero MV: copy from (2,3).
        mc_luma(
            &mut dst,
            w,
            &ref_plane,
            ref_w,
            ref_w as i32,
            ref_h as i32,
            2,
            3,
            w,
            h,
            0,
            0,
            8,
            &mut vec![0i16; MC_TMP_LEN],
            &mut vec![0u8; MC_EDGE_LEN],
        );

        for j in 0..h {
            for i in 0..w {
                assert_eq!(dst[j * w + i], ref_plane[(3 + j) * ref_w + (2 + i)]);
            }
        }
    }

    /// Test that horizontal-only sub-pel filtering produces reasonable results.
    #[test]
    fn mc_luma_half_pel_horizontal() {
        let ref_w = 16usize;
        let ref_h = 16usize;
        let ref_plane = vec![128u8; ref_w * ref_h];

        let mut dst = vec![0u8; 4 * 4];
        // MV = (2, 0) = half-pel horizontal.
        mc_luma(
            &mut dst,
            4,
            &ref_plane,
            ref_w,
            ref_w as i32,
            ref_h as i32,
            4,
            4,
            4,
            4,
            2,
            0,
            8,
            &mut vec![0i16; MC_TMP_LEN],
            &mut vec![0u8; MC_EDGE_LEN],
        );

        // With uniform reference, all outputs should be 128.
        for &v in &dst {
            assert_eq!(v, 128);
        }
    }

    /// Test that 2D sub-pel produces reasonable results on a uniform block.
    #[test]
    fn mc_luma_quarter_pel_hv_uniform() {
        let ref_w = 16usize;
        let ref_h = 16usize;
        let ref_plane = vec![200u8; ref_w * ref_h];

        let mut dst = vec![0u8; 4 * 4];
        mc_luma(
            &mut dst,
            4,
            &ref_plane,
            ref_w,
            ref_w as i32,
            ref_h as i32,
            4,
            4,
            4,
            4,
            1,
            3,
            8,
            &mut vec![0i16; MC_TMP_LEN],
            &mut vec![0u8; MC_EDGE_LEN],
        );

        for &v in &dst {
            assert_eq!(v, 200);
        }
    }

    /// Chroma integer-pel copy.
    #[test]
    fn mc_chroma_integer_pel_copy() {
        let ref_w = 8usize;
        let ref_h = 8usize;
        let mut ref_plane = vec![0u8; ref_w * ref_h];
        for j in 0..ref_h {
            for i in 0..ref_w {
                ref_plane[j * ref_w + i] = ((j + i) * 30) as u8;
            }
        }

        let w = 4;
        let h = 4;
        let mut dst = vec![0u8; w * h];

        mc_chroma(
            &mut dst,
            w,
            &ref_plane,
            ref_w,
            ref_w as i32,
            ref_h as i32,
            1,
            1,
            w,
            h,
            0,
            0,
            8,
            &mut vec![0i16; MC_TMP_LEN],
            &mut vec![0u8; MC_EDGE_LEN],
        );

        for j in 0..h {
            for i in 0..w {
                assert_eq!(dst[j * w + i], ref_plane[(1 + j) * ref_w + (1 + i)]);
            }
        }
    }

    /// Chroma sub-pel on uniform block should give the same value.
    #[test]
    fn mc_chroma_sub_pel_uniform() {
        let ref_w = 8usize;
        let ref_h = 8usize;
        let ref_plane = vec![100u8; ref_w * ref_h];

        let mut dst = vec![0u8; 4 * 4];
        mc_chroma(
            &mut dst,
            4,
            &ref_plane,
            ref_w,
            ref_w as i32,
            ref_h as i32,
            2,
            2,
            4,
            4,
            3,
            5,
            8,
            &mut vec![0i16; MC_TMP_LEN],
            &mut vec![0u8; MC_EDGE_LEN],
        );

        for &v in &dst {
            assert_eq!(v, 100);
        }
    }
}
