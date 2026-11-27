//! Loop-filter frame parameter tables and scalar sample-filtering kernels (spec §8.8.1, §8.8.3-§8.8.5):
//! `LvlLookup`, `StrengthTable`, `filter_size_process`, `compute_filter_mask`, `narrow_filter`,
//! `wide_filter`, and `sample_filtering`.

use crate::common::{clip3, round2, INTRA_FRAME, MAX_SEGMENTS};
use crate::framebuffer::Plane;
use crate::header::{LoopFilterParams, SegmentationParams, SEG_LVL_ALT_L};
use crate::prob_tables::{TX_16X16, TX_4X4, TX_8X8};

const MAX_REF_FRAMES: usize = 4;
const MAX_MODE_LF_DELTAS: usize = 2;
const MAX_LOOP_FILTER: i32 = 63;

/// `LvlLookup[ segmentId ][ ref ][ mode ]` (spec §8.8.1).
pub(super) type LvlLookup = [[[i32; MAX_MODE_LF_DELTAS]; MAX_REF_FRAMES]; MAX_SEGMENTS];

/// Precomputed `(limit, blimit, thresh)` table indexed by `lvl` (`0..=63`), built once per
/// frame from `(sharpness, bit_depth)` so edge filtering avoids recomputing strength per block.
pub(super) type StrengthTable = [(i32, i32, i32); 64];

/// Pixel access for the loop-filter kernels.
pub(super) trait PlaneAccess {
    /// Reads pixel `(x, y)` without release bounds checks.
    ///
    /// # Safety
    /// Caller must ensure `(x, y)` lies inside the plane's bounds.
    unsafe fn lf_get(&self, x: usize, y: usize) -> u16;
    /// Writes pixel `(x, y)` without release bounds checks.
    ///
    /// # Safety
    /// Caller must ensure `(x, y)` lies inside the plane's bounds.
    unsafe fn lf_set(&mut self, x: usize, y: usize, v: u16);
    #[cfg(all(target_arch = "x86_64", target_feature = "sse2"))]
    fn lf_base_ptr(&mut self) -> Option<*mut u16>;
    #[cfg(all(target_arch = "x86_64", target_feature = "sse2"))]
    fn lf_byte_ptr(&mut self) -> Option<*mut u8>;
    #[cfg(all(target_arch = "x86_64", target_feature = "sse2"))]
    fn lf_stride(&self) -> usize;
}

impl PlaneAccess for Plane {
    #[inline]
    unsafe fn lf_get(&self, x: usize, y: usize) -> u16 {
        // SAFETY: caller upholds `PlaneAccess::lf_get` bounds contract.
        self.get_unchecked(x, y)
    }
    #[inline]
    unsafe fn lf_set(&mut self, x: usize, y: usize, v: u16) {
        // SAFETY: caller upholds `PlaneAccess::lf_set` bounds contract.
        self.set_unchecked(x, y, v);
    }
    #[cfg(all(target_arch = "x86_64", target_feature = "sse2"))]
    #[inline]
    fn lf_base_ptr(&mut self) -> Option<*mut u16> {
        self.word_ptr()
    }
    #[cfg(all(target_arch = "x86_64", target_feature = "sse2"))]
    fn lf_byte_ptr(&mut self) -> Option<*mut u8> {
        self.byte_ptr()
    }
    #[cfg(all(target_arch = "x86_64", target_feature = "sse2"))]
    #[inline]
    fn lf_stride(&self) -> usize {
        self.stride()
    }
}

pub(super) fn build_strength_table(sharpness: u8, bit_depth: u8) -> StrengthTable {
    core::array::from_fn(|lvl| adaptive_filter_strength(lvl as i32, sharpness, bit_depth))
}

/// Spec §8.8.1 "Loop filter frame init process".
pub(super) fn build_lvl_lookup(lf: &LoopFilterParams, seg: &SegmentationParams) -> LvlLookup {
    let n_shift = (lf.level as i32) >> 5;
    let mut table: LvlLookup = [[[0; MAX_MODE_LF_DELTAS]; MAX_REF_FRAMES]; MAX_SEGMENTS];

    for (segment_id, seg_table) in table.iter_mut().enumerate() {
        let mut lvl_seg = lf.level as i32;
        if seg.enabled && seg.feature_enabled[segment_id][SEG_LVL_ALT_L] {
            let data = seg.feature_data[segment_id][SEG_LVL_ALT_L];
            lvl_seg = if seg.abs_or_delta_update {
                data
            } else {
                data + lf.level as i32
            };
            lvl_seg = clip3(0, MAX_LOOP_FILTER, lvl_seg);
        }

        if !lf.delta_enabled {
            for r in seg_table.iter_mut() {
                for m in r.iter_mut() {
                    *m = lvl_seg;
                }
            }
        } else {
            let intra_lvl = lvl_seg + (lf.ref_deltas[INTRA_FRAME as usize] as i32) * (1 << n_shift);
            seg_table[INTRA_FRAME as usize][0] = clip3(0, MAX_LOOP_FILTER, intra_lvl);
            for (r, ref_delta) in lf.ref_deltas.iter().enumerate().skip(1) {
                for (m, mode_delta) in lf.mode_deltas.iter().enumerate() {
                    let inter_lvl = lvl_seg
                        + (*ref_delta as i32) * (1 << n_shift)
                        + (*mode_delta as i32) * (1 << n_shift);
                    seg_table[r][m] = clip3(0, MAX_LOOP_FILTER, inter_lvl);
                }
            }
        }
    }
    table
}

/// Spec §8.8.4 "Adaptive filter strength process", scaled by `<< (bit_depth - 8)`.
pub(super) fn adaptive_filter_strength(lvl: i32, sharpness: u8, bit_depth: u8) -> (i32, i32, i32) {
    let shift = if sharpness > 4 {
        2
    } else if sharpness > 0 {
        1
    } else {
        0
    };
    let limit = if sharpness > 0 {
        clip3(1, 9 - sharpness as i32, lvl >> shift)
    } else {
        (lvl >> shift).max(1)
    };
    let blimit = 2 * (lvl + 2) + limit;
    let thresh = lvl >> 4;
    let depth_shift = (bit_depth - 8) as u32;
    (
        limit << depth_shift,
        blimit << depth_shift,
        thresh << depth_shift,
    )
}

/// Direction of a loop-filter pass (spec §8.8): vertical edges first, then horizontal edges.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum FilterPass {
    /// `pass == 0`: vertical block/transform edges (`dx = 1, dy = 0`).
    Vertical,
    /// `pass == 1`: horizontal block/transform edges (`dx = 0, dy = 1`).
    Horizontal,
}

impl FilterPass {
    #[inline]
    pub(super) fn step(self) -> (i64, i64) {
        match self {
            FilterPass::Vertical => (1, 0),
            FilterPass::Horizontal => (0, 1),
        }
    }
}

/// Parameters shared across [`compute_filter_mask`] and [`sample_filtering`] for one edge.
#[derive(Clone, Copy)]
pub(super) struct EdgeFilterParams {
    pub(super) dx: i64,
    pub(super) dy: i64,
    pub(super) limit: i32,
    pub(super) blimit: i32,
    pub(super) thresh: i32,
    pub(super) filter_size: u8,
    pub(super) bit_depth: u8,
}

/// Spec §8.8.3 "Filter size process".
#[allow(clippy::too_many_arguments)]
pub(super) fn filter_size_process(
    tx_sz: u8,
    is_32_edge: bool,
    pass: FilterPass,
    x: u32,
    y: u32,
    sub_x: u32,
    sub_y: u32,
    mi_cols: u32,
    mi_rows: u32,
) -> u8 {
    let base_size = if tx_sz == TX_4X4 && is_32_edge {
        TX_8X8
    } else {
        tx_sz.min(TX_16X16)
    };

    let luma_boundary = match pass {
        FilterPass::Vertical => sub_x == 1 && (x >> 3) == mi_cols - 1,
        FilterPass::Horizontal => sub_y == 1 && (y >> 3) == mi_rows - 1,
    };
    if base_size == TX_16X16 && luma_boundary {
        TX_8X8
    } else {
        base_size
    }
}

#[inline]
fn get_off<P: PlaneAccess>(plane: &P, x: usize, y: usize, dx: i64, dy: i64, k: i64) -> i32 {
    let px = (x as i64 + dx * k) as usize;
    let py = (y as i64 + dy * k) as usize;
    // SAFETY: `superblock_loop_filter` skips `edge_coord == 0`, caps `filter_size` at chroma
    // boundaries (`filter_size_process`), and operates on superblock-padded planes, so every tap
    // `p7..q7` stays inside the plane bounds.
    unsafe { plane.lf_get(px, py) as i32 }
}

#[inline]
fn set_off<P: PlaneAccess>(plane: &mut P, x: usize, y: usize, dx: i64, dy: i64, k: i64, v: i32) {
    let px = (x as i64 + dx * k) as usize;
    let py = (y as i64 + dy * k) as usize;
    // SAFETY: same plane bounds as `get_off`.
    unsafe {
        plane.lf_set(px, py, v as u16);
    }
}

/// Spec §8.8.5.1 "Filter mask process". Returns `(hevMask, filterMask, flatMask, flatMask2)`.
fn compute_filter_mask<P: PlaneAccess>(
    plane: &P,
    x: usize,
    y: usize,
    p: EdgeFilterParams,
) -> (bool, bool, bool, bool) {
    let g = |k: i64| get_off(plane, x, y, p.dx, p.dy, k);
    let q: [i32; 4] = core::array::from_fn(|k| g(k as i64));
    let pv: [i32; 4] = core::array::from_fn(|k| g(-(k as i64) - 1));

    let hev_mask = (pv[1] - pv[0]).abs() > p.thresh || (q[1] - q[0]).abs() > p.thresh;

    let mut mask = (pv[0] - q[0]).abs() * 2 + (pv[1] - q[1]).abs() / 2 > p.blimit;
    for k in 0..3 {
        mask |= (pv[k + 1] - pv[k]).abs() > p.limit || (q[k + 1] - q[k]).abs() > p.limit;
    }
    let filter_mask = !mask;

    let threshold = 1i32 << (p.bit_depth - 8);
    let flat_mask = p.filter_size >= TX_8X8
        && !(1..=3).any(|k| (pv[k] - pv[0]).abs() > threshold || (q[k] - q[0]).abs() > threshold);
    let flat_mask2 = p.filter_size >= TX_16X16
        && !(4..=7).any(|k| {
            (g(-(k as i64) - 1) - pv[0]).abs() > threshold || (g(k as i64) - q[0]).abs() > threshold
        });

    (hev_mask, filter_mask, flat_mask, flat_mask2)
}

/// Spec §8.8.5.2 "Narrow filter process" (`filter4`).
pub(super) fn narrow_filter<P: PlaneAccess>(
    plane: &mut P,
    x: usize,
    y: usize,
    dx: i64,
    dy: i64,
    hev_mask: bool,
    bit_depth: u8,
) {
    let half = 1i32 << (bit_depth - 1);
    let clamp_hi = (128i32 << (bit_depth - 8)) - 1;
    let clamp_lo = -(clamp_hi + 1);
    let clamp4 = |v: i32| clip3(clamp_lo, clamp_hi, v);

    let q0 = get_off(plane, x, y, dx, dy, 0);
    let q1 = get_off(plane, x, y, dx, dy, 1);
    let p0 = get_off(plane, x, y, dx, dy, -1);
    let p1 = get_off(plane, x, y, dx, dy, -2);

    let ps1 = p1 - half;
    let ps0 = p0 - half;
    let qs0 = q0 - half;
    let qs1 = q1 - half;

    let mut filter = if hev_mask { clamp4(ps1 - qs1) } else { 0 };
    filter = clamp4(filter + 3 * (qs0 - ps0));
    let filter1 = clamp4(filter + 4) >> 3;
    let filter2 = clamp4(filter + 3) >> 3;

    let oq0 = clamp4(qs0 - filter1) + half;
    let op0 = clamp4(ps0 + filter2) + half;
    set_off(plane, x, y, dx, dy, 0, oq0);
    set_off(plane, x, y, dx, dy, -1, op0);

    if !hev_mask {
        let filter = round2(filter1, 1);
        let oq1 = clamp4(qs1 - filter) + half;
        let op1 = clamp4(ps1 + filter) + half;
        set_off(plane, x, y, dx, dy, 1, oq1);
        set_off(plane, x, y, dx, dy, -2, op1);
    }
}

/// Spec §8.8.5.3 "Wide filter process". `log2_size` is 3 (8-tap) or 4 (16-tap).
fn wide_filter<P: PlaneAccess>(
    plane: &mut P,
    x: usize,
    y: usize,
    dx: i64,
    dy: i64,
    log2_size: u32,
) {
    let n: i64 = (1i64 << (log2_size - 1)) - 1;
    let mut f = [0i32; 16];

    let mut i = -n;
    while i < n {
        let mut t = get_off(plane, x, y, dx, dy, i);
        let mut j = -n;
        while j <= n {
            let p = clip3(-((n + 1) as i32), n as i32, (i + j) as i32) as i64;
            t += get_off(plane, x, y, dx, dy, p);
            j += 1;
        }
        f[(i + n) as usize] = round2(t, log2_size);
        i += 1;
    }

    let mut i = -n;
    while i < n {
        set_off(plane, x, y, dx, dy, i, f[(i + n) as usize]);
        i += 1;
    }
}

/// Spec §8.8.5 "Sample filtering process".
pub(super) fn sample_filtering<P: PlaneAccess>(
    plane: &mut P,
    x: usize,
    y: usize,
    p: EdgeFilterParams,
) {
    let (hev_mask, filter_mask, flat_mask, flat_mask2) = compute_filter_mask(plane, x, y, p);

    if !filter_mask {
        return;
    }
    if p.filter_size == TX_4X4 || !flat_mask {
        narrow_filter(plane, x, y, p.dx, p.dy, hev_mask, p.bit_depth);
    } else if p.filter_size == TX_8X8 || !flat_mask2 {
        wide_filter(plane, x, y, p.dx, p.dy, 3);
    } else {
        wide_filter(plane, x, y, p.dx, p.dy, 4);
    }
}
