//! VP9 loop filter (spec §8.8).

mod edge_filter;

use crate::common::{get_uv_tx_size, INTRA_FRAME};
use crate::framebuffer::Plane;
use crate::header::{LoopFilterParams, SegmentationParams};
use crate::prob_tables::{
    BLOCK_16X16, NEARESTMV, NEARMV, NEWMV, NUM_8X8_BLOCKS_HIGH_LOOKUP, NUM_8X8_BLOCKS_WIDE_LOOKUP,
};
use crate::tile::MiGrid;
use edge_filter::{
    build_lvl_lookup, build_strength_table, filter_size_process, sample_filtering,
    EdgeFilterParams, FilterPass, LvlLookup, PlaneAccess, StrengthTable,
};

/// Frame-invariant parameters for the loop-filter traversal.
struct LoopFilterContext<'a> {
    mi_grid: &'a MiGrid,
    mi_cols: u32,
    mi_rows: u32,
    subsampling_x: u32,
    subsampling_y: u32,
    lvl_lookup: LvlLookup,
    strength_table: StrengthTable,
    bit_depth: u8,
}

/// Active filter specification for an 8-lane edge batch when filtering is enabled.
#[derive(Clone, Copy)]
struct EdgeFilterSpec {
    lvl: i32,
    filter_size: u8,
}

/// Per-8-lane-group filter gating/strength along one superblock edge.
#[derive(Clone, Copy)]
struct EdgeGroup {
    x: u32,
    y: u32,
    active_lanes: u32,
    filter: Option<EdgeFilterSpec>,
}

/// Per-8-lane-group filter gating/strength.
///
/// Because `row`, `col`, and `i` are all multiples of 8, every 8-lane batch (`i..i+8`) along an
/// edge falls inside a single `(loop_row, loop_col)` MI unit for ALL VP9 chroma subsamplings
/// (`4:2:0`, `4:2:2`, `4:4:0`, `4:4:4`). Therefore `mi`, `lvl`, `filter_size`, `is_block_edge`,
/// and `is_tx_edge` are uniform across the active lanes; only the last superblock of an
/// odd-MI-dimension chroma plane can truncate `active_lanes` from 8 down to 4 (or 0).
#[inline]
#[allow(clippy::too_many_arguments)]
fn edge_group_params(
    ctx: &LoopFilterContext<'_>,
    plane_idx: usize,
    pass: FilterPass,
    row: u32,
    col: u32,
    sub_x: u32,
    sub_y: u32,
    sub: u32,
    edge: u32,
    i: u32,
) -> EdgeGroup {
    let (x, y, active_lanes): (u32, u32, u32) = match pass {
        FilterPass::Vertical => {
            let x = col * 8 + edge * (4 << sub_x);
            let y = row * 8 + (i << sub_y);
            let max_y = 8 * ctx.mi_rows;
            if y >= max_y {
                return EdgeGroup {
                    x,
                    y,
                    active_lanes: 0,
                    filter: None,
                };
            }
            let lanes = if y + (7 << sub_y) < max_y { 8 } else { 4 };
            (x, y, lanes)
        }
        FilterPass::Horizontal => {
            let x = col * 8 + (i << sub_x);
            let y = row * 8 + edge * (4 << sub_y);
            let max_x = 8 * ctx.mi_cols;
            if x >= max_x {
                return EdgeGroup {
                    x,
                    y,
                    active_lanes: 0,
                    filter: None,
                };
            }
            let lanes = if x + (7 << sub_x) < max_x { 8 } else { 4 };
            (x, y, lanes)
        }
    };

    let loop_col = ((x >> 3) >> sub_x) << sub_x;
    let loop_row = ((y >> 3) >> sub_y) << sub_y;
    // SAFETY: `x < 8 * mi_cols` and `y < 8 * mi_rows` (checked above), so `loop_col < mi_cols`
    // and `loop_row < mi_rows` lie strictly inside the whole-frame `mi_grid`.
    let mi = unsafe { ctx.mi_grid.get_unchecked(loop_row, loop_col) };

    let mi_size = mi.mi_size();
    let tx_size = mi.tx_size();
    let tx_sz = if plane_idx > 0 {
        get_uv_tx_size(mi_size, tx_size, ctx.subsampling_x, ctx.subsampling_y)
    } else {
        tx_size
    };
    let sb_size = if sub == 0 {
        mi_size
    } else {
        mi_size.max(BLOCK_16X16)
    };
    let skip = mi.skip();
    let ref_frame = mi.ref_frame0();
    let is_intra = ref_frame == INTRA_FRAME;

    // Block widths/heights in pixels (`8 * NUM_8X8_BLOCKS_*`) are powers of 2 -> bitmask test.
    let is_block_edge = match pass {
        FilterPass::Vertical => {
            (x & (8 * NUM_8X8_BLOCKS_WIDE_LOOKUP[sb_size as usize] as u32 - 1)) == 0
        }
        FilterPass::Horizontal => {
            (y & (8 * NUM_8X8_BLOCKS_HIGH_LOOKUP[sb_size as usize] as u32 - 1)) == 0
        }
    };

    // For Horizontal pass, sub_x == 1, odd mi_cols, and the last 4 active lanes (`x + 8 >= 8 * mi_cols`
    // holds for all 4 active lanes simultaneously because `x` is a multiple of 16 and `8 * mi_cols`
    // is `16k + 8`, so `x + 8 == 8 * mi_cols` on lane 0 and `> 8 * mi_cols` on lanes 1..3).
    let is_tx_edge = if pass == FilterPass::Horizontal
        && sub_x == 1
        && (ctx.mi_cols & 1) == 1
        && (edge & 1) == 1
        && (x + 8) >= ctx.mi_cols * 8
    {
        false
    } else {
        (edge & ((1u32 << tx_sz) - 1)) == 0
    };

    let apply_filter = is_block_edge || (is_tx_edge && (is_intra || !skip));
    if !apply_filter {
        return EdgeGroup {
            x,
            y,
            active_lanes,
            filter: None,
        };
    }

    let mode_type = matches!(mi.y_mode(), NEARESTMV | NEARMV | NEWMV) as usize;
    let lvl = ctx.lvl_lookup[mi.segment_id() as usize][ref_frame as usize][mode_type];
    if lvl <= 0 {
        return EdgeGroup {
            x,
            y,
            active_lanes,
            filter: None,
        };
    }

    let is_32_edge = (edge & 7) == 0;
    let filter_size = filter_size_process(
        tx_sz,
        is_32_edge,
        pass,
        x,
        y,
        sub_x,
        sub_y,
        ctx.mi_cols,
        ctx.mi_rows,
    );

    EdgeGroup {
        x,
        y,
        active_lanes,
        filter: Some(EdgeFilterSpec { lvl, filter_size }),
    }
}

/// Spec §8.8.2 "Superblock loop filter process".
fn superblock_loop_filter<P: PlaneAccess>(
    plane: &mut P,
    ctx: &LoopFilterContext<'_>,
    plane_idx: usize,
    pass: FilterPass,
    row: u32,
    col: u32,
) {
    let (sub_x, sub_y) = if plane_idx == 0 {
        (0u32, 0u32)
    } else {
        (ctx.subsampling_x, ctx.subsampling_y)
    };

    let (dx, dy) = pass.step();
    let (sub, edge_len) = match pass {
        FilterPass::Vertical => (sub_x, 64 >> sub_y),
        FilterPass::Horizontal => (sub_y, 64 >> sub_x),
    };

    #[cfg(all(target_arch = "x86_64", target_feature = "sse2"))]
    let use_avx2 = crate::simd::avx2_enabled();
    #[cfg(not(all(target_arch = "x86_64", target_feature = "sse2")))]
    let use_avx2 = false;

    let num_edges = 16u32 >> sub;
    for edge in 0..num_edges {
        // Edge-level screen boundary check: `x` (Vertical) or `y` (Horizontal) is constant along the
        // entire edge, so skip edge 0 at frame origin and break past the frame's far edge.
        let (edge_coord, edge_limit) = match pass {
            FilterPass::Vertical => (col * 8 + edge * (4 << sub_x), 8 * ctx.mi_cols),
            FilterPass::Horizontal => (row * 8 + edge * (4 << sub_y), 8 * ctx.mi_rows),
        };
        if edge_coord == 0 {
            continue;
        }
        if edge_coord >= edge_limit {
            break;
        }

        let mut i = 0u32;
        while i < edge_len {
            let group =
                edge_group_params(ctx, plane_idx, pass, row, col, sub_x, sub_y, sub, edge, i);
            if group.active_lanes == 0 {
                break;
            }
            if let Some(spec) = group.filter {
                let (limit, blimit, thresh) = ctx.strength_table[spec.lvl as usize];
                let px0 = (group.x >> sub_x) as usize;
                let py0 = (group.y >> sub_y) as usize;
                #[cfg(all(target_arch = "x86_64", target_feature = "sse2"))]
                if use_avx2 {
                    let lf_fn = match pass {
                        FilterPass::Horizontal => crate::simd::loop_filter_horiz8_uniform_avx2,
                        FilterPass::Vertical => crate::simd::loop_filter_vert8_uniform_avx2,
                    };
                    // SAFETY: `avx2_enabled()` confirmed; active lanes lie within the
                    // superblock-padded plane, and `filter_size == TX_16X16` guarantees the
                    // `p7..q7` window is in bounds (same proof as scalar `sample_filtering`).
                    unsafe {
                        lf_fn(
                            plane.lf_base_ptr(),
                            plane.lf_stride(),
                            px0,
                            py0,
                            group.active_lanes,
                            spec.filter_size,
                            limit,
                            blimit,
                            thresh,
                            ctx.bit_depth,
                        );
                    }
                    i += 8;
                    continue;
                }

                let _ = use_avx2;
                let edge_params = EdgeFilterParams {
                    dx,
                    dy,
                    limit,
                    blimit,
                    thresh,
                    filter_size: spec.filter_size,
                    bit_depth: ctx.bit_depth,
                };
                for lane in 0..group.active_lanes as usize {
                    let (sx, sy) = match pass {
                        FilterPass::Vertical => (px0, py0 + lane),
                        FilterPass::Horizontal => (px0 + lane, py0),
                    };
                    sample_filtering(plane, sx, sy, edge_params);
                }
            }
            i += 8;
        }
    }
}

/// Filters one plane in superblock raster order, vertical pass first.
fn loop_filter_plane(plane: &mut Plane, plane_idx: usize, ctx: &LoopFilterContext<'_>) {
    let mut row = 0u32;
    while row < ctx.mi_rows {
        let mut col = 0u32;
        while col < ctx.mi_cols {
            for pass in [FilterPass::Vertical, FilterPass::Horizontal] {
                superblock_loop_filter(plane, ctx, plane_idx, pass, row, col);
            }
            col += 8;
        }
        row += 8;
    }
}

/// Frame-wide entry point for spec §8.8 "Loop filter process".
///
/// `planes` is `CurrFrame` (equivalent to `TileDecoder::planes`, a buffer allocated out to
/// superblock boundaries); `mi_grid` is the mode info grid; `mi_cols`/`mi_rows` is the
/// non-padded, mi-unit frame size obtained from `compute_image_size()`.
#[allow(clippy::too_many_arguments)]
pub(crate) fn loop_filter_frame(
    planes: &mut [Plane; 3],
    mi_grid: &MiGrid,
    mi_cols: u32,
    mi_rows: u32,
    subsampling_x: u32,
    subsampling_y: u32,
    lf: &LoopFilterParams,
    seg: &SegmentationParams,
    bit_depth: u8,
) {
    let ctx = LoopFilterContext {
        mi_grid,
        mi_cols,
        mi_rows,
        subsampling_x,
        subsampling_y,
        lvl_lookup: build_lvl_lookup(lf, seg),
        strength_table: build_strength_table(lf.sharpness, bit_depth),
        bit_depth,
    };

    for (plane_idx, plane) in planes.iter_mut().enumerate() {
        loop_filter_plane(plane, plane_idx, &ctx);
    }
}
