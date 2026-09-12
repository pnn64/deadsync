//! Frozen graph preparation from 2230885b3 (0.5.1146).
//! Only visibility/imports differ; the column descriptor is shared unchanged.

//! Pure preparation of sync-analysis heatmaps and curve geometry.

use deadlib_render_core::MeshVertex;
use deadsync_config::null_or_die::GraphOrigin;
use image::{Rgba, RgbaImage};
use null_or_die::GraphOrientation;
use std::sync::Arc;

pub(super) const SYNC_HEAT_ALPHA: f32 = 1.0;

fn push_line_segment(
    out: &mut Vec<MeshVertex>,
    x0: f32,
    y0: f32,
    x1: f32,
    y1: f32,
    thickness: f32,
    color: [f32; 4],
) {
    let dx = x1 - x0;
    let dy = y1 - y0;
    let len = (dx.mul_add(dx, dy * dy)).sqrt();
    if len <= 0.000_1 {
        return;
    }
    let half = thickness * 0.5;
    let nx = -dy / len * half;
    let ny = dx / len * half;

    let a = [x0 + nx, y0 + ny];
    let b = [x0 - nx, y0 - ny];
    let c = [x1 + nx, y1 + ny];
    let d = [x1 - nx, y1 - ny];

    out.push(MeshVertex { pos: a, color });
    out.push(MeshVertex { pos: b, color });
    out.push(MeshVertex { pos: c, color });
    out.push(MeshVertex { pos: c, color });
    out.push(MeshVertex { pos: b, color });
    out.push(MeshVertex { pos: d, color });
}

// A 5x time zoom makes the typical 5-6% kernel-response band occupy roughly
// one quarter of the horizontal graph while leaving useful context around it.
const SYNC_HORIZONTAL_TIME_FRACTION: f32 = 0.2;

use super::sync_graph::SyncGraphCols;

pub(super) fn sync_graph_cols(
    total: usize,
    times_ms: &[f64],
    orientation: GraphOrientation,
    focus_ms: Option<f64>,
) -> SyncGraphCols {
    if total == 0 || orientation == GraphOrientation::Vertical {
        return SyncGraphCols {
            total,
            first: 0,
            end: total,
        };
    }

    let min_cols = total.min(8);
    let visible =
        ((total as f32 * SYNC_HORIZONTAL_TIME_FRACTION).round() as usize).clamp(min_cols, total);
    let focus = focus_ms.filter(|value| value.is_finite()).unwrap_or(0.0);
    let focus_col = if times_ms.len() == total {
        let next = times_ms.partition_point(|value| *value < focus);
        match (next.checked_sub(1), (next < total).then_some(next)) {
            (Some(before), Some(after)) => {
                if (times_ms[before] - focus).abs() <= (times_ms[after] - focus).abs() {
                    before
                } else {
                    after
                }
            }
            (Some(before), None) => before,
            (None, Some(after)) => after,
            (None, None) => total / 2,
        }
    } else {
        total / 2
    };
    let first = focus_col.saturating_sub(visible / 2).min(total - visible);
    SyncGraphCols {
        total,
        first,
        end: first + visible,
    }
}

pub(super) fn build_sync_curve_mesh(
    values: &[f64],
    edge_discard: usize,
    cols: SyncGraphCols,
    graph_w: f32,
    graph_h: f32,
    orientation: GraphOrientation,
    color: [f32; 4],
) -> Option<Arc<[MeshVertex]>> {
    if values.len() < 2
        || cols.total != values.len()
        || cols.end > values.len()
        || cols.end.saturating_sub(cols.first) < 2
        || graph_w <= 0.0
        || graph_h <= 0.0
    {
        return None;
    }
    let edge = edge_discard.min(values.len() / 2);
    let core = &values[edge..values.len().saturating_sub(edge)];
    if core.is_empty() {
        return None;
    }
    let mut min_value = f64::INFINITY;
    let mut max_value = f64::NEG_INFINITY;
    for &value in core {
        min_value = min_value.min(value);
        max_value = max_value.max(value);
    }
    let x_left = graph_w * 0.1;
    let x_right = graph_w * 0.9;
    let y_top = graph_h * 0.1;
    let y_bottom = graph_h * 0.9;
    let visible_len = cols.end - cols.first;
    let mut out = Vec::with_capacity(visible_len.saturating_sub(1) * 6);
    for i in cols.first..cols.end.saturating_sub(1) {
        let denom = visible_len.saturating_sub(1) as f32;
        let axis0 = (i - cols.first) as f32 / denom;
        let axis1 = (i + 1 - cols.first) as f32 / denom;
        let t0 = sync_heat_norm01(values[i], min_value, max_value) as f32;
        let t1 = sync_heat_norm01(values[i + 1], min_value, max_value) as f32;
        let (x0, y0, x1, y1) = match orientation {
            GraphOrientation::Vertical => (
                axis0 * (graph_w - 1.0).max(0.0),
                (y_top - y_bottom).mul_add(t0, y_bottom),
                axis1 * (graph_w - 1.0).max(0.0),
                (y_top - y_bottom).mul_add(t1, y_bottom),
            ),
            GraphOrientation::Horizontal => (
                (x_right - x_left).mul_add(t0, x_left),
                (1.0 - axis0) * (graph_h - 1.0).max(0.0),
                (x_right - x_left).mul_add(t1, x_left),
                (1.0 - axis1) * (graph_h - 1.0).max(0.0),
            ),
        };
        push_line_segment(&mut out, x0, y0, x1, y1, 1.5, color);
    }
    if out.is_empty() {
        None
    } else {
        Some(Arc::from(out.into_boxed_slice()))
    }
}

#[inline(always)]
fn sync_heat_norm01(v: f64, lo: f64, hi: f64) -> f64 {
    let span = hi - lo;
    if !span.is_finite() || span.abs() < f64::EPSILON {
        0.5
    } else {
        ((v - lo) / span).clamp(0.0, 1.0)
    }
}

#[inline(always)]
fn sync_lerp(a: f64, b: f64, t: f64) -> f64 {
    b.mul_add(t, a * (1.0 - t))
}

fn sync_percentile_from_sorted(sorted: &[f64], pct: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    if sorted.len() == 1 {
        return sorted[0];
    }
    let rank = (pct / 100.0) * (sorted.len() - 1) as f64;
    let lo = rank.floor() as usize;
    let hi = rank.ceil() as usize;
    if lo == hi {
        sorted[lo]
    } else {
        sync_lerp(sorted[lo], sorted[hi], rank - lo as f64)
    }
}

pub(super) fn sync_percentile_pair(values: &[f64], lo_pct: f64, hi_pct: f64) -> (f64, f64) {
    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    (
        sync_percentile_from_sorted(&sorted, lo_pct),
        sync_percentile_from_sorted(&sorted, hi_pct),
    )
}

#[inline(always)]
fn sync_viridis(t: f64) -> [f32; 4] {
    const STOPS: [[u8; 3]; 5] = [
        [68, 1, 84],
        [59, 82, 139],
        [33, 145, 140],
        [94, 201, 98],
        [253, 231, 37],
    ];
    let x = t.clamp(0.0, 1.0) * 4.0;
    let i = x.floor() as usize;
    let (a, b, frac) = if i >= 4 {
        (STOPS[4], STOPS[4], 0.0)
    } else {
        (STOPS[i], STOPS[i + 1], x - i as f64)
    };
    let mix =
        |aa: u8, bb: u8| f64::from(bb).mul_add(frac, f64::from(aa) * (1.0 - frac)) as f32 / 255.0;
    [
        mix(a[0], b[0]),
        mix(a[1], b[1]),
        mix(a[2], b[2]),
        SYNC_HEAT_ALPHA,
    ]
}

pub(super) fn sync_heat_value_range(
    values: &[f64],
    clim_pct: Option<(f64, f64)>,
) -> Option<(f64, f64)> {
    if values.is_empty() {
        return None;
    }
    if let Some((lo_pct, hi_pct)) = clim_pct {
        let (lo, hi) = sync_percentile_pair(values, lo_pct, hi_pct);
        if hi > lo {
            return Some((lo, hi));
        }
    }
    let lo = values.iter().copied().fold(f64::INFINITY, f64::min);
    let hi = values.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    if !lo.is_finite() || !hi.is_finite() {
        None
    } else if hi > lo {
        Some((lo, hi))
    } else {
        Some((lo - 1.0, hi + 1.0))
    }
}

pub(super) fn build_sync_heat_image(
    matrix: &[f64],
    total_rows: usize,
    data_rows: usize,
    cols: SyncGraphCols,
    graph_size: [f32; 2],
    orientation: GraphOrientation,
    origin: GraphOrigin,
    clim_pct: Option<(f64, f64)>,
) -> Option<RgbaImage> {
    let [graph_w, graph_h] = graph_size;
    let visible_cols = cols.end.saturating_sub(cols.first);
    if data_rows == 0
        || cols.total == 0
        || cols.end > cols.total
        || visible_cols == 0
        || graph_w <= 0.0
        || graph_h <= 0.0
    {
        return None;
    }
    let image_h = (graph_h.round() as u32).max(1);
    let image_w = (graph_w.round() as u32).max(1);
    let used = data_rows.saturating_mul(cols.total).min(matrix.len());
    let (lo, hi) = sync_heat_value_range(&matrix[..used], clim_pct)?;
    let mut image = RgbaImage::new(image_w, image_h);
    for py in 0..image_h as usize {
        for px in 0..image_w as usize {
            let (row, col) = match orientation {
                GraphOrientation::Vertical => {
                    let row_px = match origin {
                        GraphOrigin::Bottom => image_h as usize - 1 - py,
                        GraphOrigin::Top => py,
                    };
                    (
                        ((row_px * total_rows) / image_h as usize)
                            .min(total_rows.saturating_sub(1)),
                        cols.first
                            + (px * visible_cols / image_w as usize)
                                .min(visible_cols.saturating_sub(1)),
                    )
                }
                // Transpose the row axis so streamed rows fill left-to-right;
                // the fingerprint-time axis grows upward to match null-or-die.
                GraphOrientation::Horizontal => (
                    ((px * total_rows) / image_w as usize).min(total_rows.saturating_sub(1)),
                    cols.first
                        + (((image_h as usize - 1 - py) * visible_cols) / image_h as usize)
                            .min(visible_cols.saturating_sub(1)),
                ),
            };
            let rgba = if row < data_rows {
                let value = matrix[row * cols.total + col];
                let color = sync_viridis(sync_heat_norm01(value, lo, hi));
                Rgba([
                    (color[0] * 255.0).round().clamp(0.0, 255.0) as u8,
                    (color[1] * 255.0).round().clamp(0.0, 255.0) as u8,
                    (color[2] * 255.0).round().clamp(0.0, 255.0) as u8,
                    (color[3] * 255.0).round().clamp(0.0, 255.0) as u8,
                ])
            } else {
                Rgba([0, 0, 0, 0])
            };
            image.put_pixel(px as u32, py as u32, rgba);
        }
    }
    Some(image)
}
