//! Pure preparation of sync-analysis heatmaps and curve geometry.

use deadlib_render_core::MeshVertex;
use deadsync_config::null_or_die::GraphOrigin;
use image::RgbaImage;
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct SyncGraphCols {
    pub(super) total: usize,
    pub(super) first: usize,
    pub(super) end: usize,
}

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

pub(super) fn update_sync_curve_mesh(
    mesh: &mut Option<Arc<Vec<MeshVertex>>>,
    values: &[f64],
    edge_discard: usize,
    cols: SyncGraphCols,
    graph_w: f32,
    graph_h: f32,
    orientation: GraphOrientation,
    color: [f32; 4],
) {
    if values.len() < 2
        || cols.total != values.len()
        || cols.end > values.len()
        || cols.end.saturating_sub(cols.first) < 2
        || graph_w <= 0.0
        || graph_h <= 0.0
    {
        *mesh = None;
        return;
    }
    let edge = edge_discard.min(values.len() / 2);
    let core = &values[edge..values.len().saturating_sub(edge)];
    if core.is_empty() {
        *mesh = None;
        return;
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
    let storage = mesh.get_or_insert_with(|| Arc::new(Vec::new()));
    if Arc::get_mut(storage).is_none() {
        // A previously emitted actor still owns this version of the geometry.
        // Start fresh so that actor's vertices remain immutable.
        *storage = Arc::new(Vec::new());
    }
    let out = Arc::get_mut(storage).expect("curve storage has one owner");
    out.clear();
    out.reserve(visible_len.saturating_sub(1) * 6);
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
        push_line_segment(out, x0, y0, x1, y1, 1.5, color);
    }
    if out.is_empty() {
        *mesh = None;
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

pub(super) fn sync_percentile_pair(values: &[f64], lo_pct: f64, hi_pct: f64) -> (f64, f64) {
    match values.len() {
        0 => return (0.0, 0.0),
        1 => return (values[0], values[0]),
        _ => {}
    }
    let ordered_pair = |slice: &[f64], reverse: bool| {
        let at = |index| {
            slice[if reverse {
                slice.len() - 1 - index
            } else {
                index
            }]
        };
        let percentile = |pct: f64| {
            let rank = (pct / 100.0) * (slice.len() - 1) as f64;
            let lo = rank.floor() as usize;
            let hi = rank.ceil() as usize;
            if lo == hi {
                at(lo)
            } else {
                sync_lerp(at(lo), at(hi), rank - lo as f64)
            }
        };
        (percentile(lo_pct), percentile(hi_pct))
    };
    if values.is_sorted_by(|a, b| a.total_cmp(b).is_le()) {
        return ordered_pair(values, false);
    }
    if values.is_sorted_by(|a, b| a.total_cmp(b).is_ge()) {
        return ordered_pair(values, true);
    }
    // Small matrices need at most 256 bytes of local scratch.
    if values.len() <= 32 {
        let mut scratch = [0.0; 32];
        let slice = &mut scratch[..values.len()];
        slice.copy_from_slice(values);
        slice.sort_unstable_by(f64::total_cmp);
        return ordered_pair(slice, false);
    }
    let ranks = [lo_pct, hi_pct].map(|pct| (pct / 100.0) * (values.len() - 1) as f64);
    let mut requested = [
        (ranks[0].floor() as usize, 0),
        (ranks[0].ceil() as usize, 1),
        (ranks[1].floor() as usize, 2),
        (ranks[1].ceil() as usize, 3),
    ];
    requested.sort_unstable_by_key(|&(index, _)| index);
    let mut scratch = values.to_vec();
    let mut remaining = scratch.as_mut_slice();
    let mut offset = 0;
    let mut selected = [0.0; 4];
    let mut previous = None;
    // Select only the four interpolation endpoints, retaining each partition
    // for the next rank. Equal ranks share their already selected value.
    for (index, slot) in requested {
        let value = if let Some((previous_index, value)) = previous
            && previous_index == index
        {
            value
        } else {
            let (_, value, tail) = remaining.select_nth_unstable_by(index - offset, f64::total_cmp);
            let value = *value;
            remaining = tail;
            offset = index + 1;
            previous = Some((index, value));
            value
        };
        selected[slot] = value;
    }
    let interpolate = |rank: f64, lo: f64, hi: f64| {
        if rank.floor() as usize == rank.ceil() as usize {
            lo
        } else {
            sync_lerp(lo, hi, rank - rank.floor() as usize as f64)
        }
    };
    (
        interpolate(ranks[0], selected[0], selected[1]),
        interpolate(ranks[1], selected[2], selected[3]),
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
    let width = image_w as usize;
    let height = image_h as usize;
    let raw: &mut [u8] = image.as_mut();
    let pixels = raw.as_chunks_mut::<4>().0;
    let mut previous_axis = None;
    for py in 0..height {
        let axis = match orientation {
            GraphOrientation::Vertical => {
                let row_px = match origin {
                    GraphOrigin::Bottom => height - 1 - py,
                    GraphOrigin::Top => py,
                };
                ((row_px * total_rows) / height).min(total_rows.saturating_sub(1))
            }
            GraphOrientation::Horizontal => {
                cols.first
                    + (((height - 1 - py) * visible_cols) / height)
                        .min(visible_cols.saturating_sub(1))
            }
        };
        if previous_axis == Some(axis) {
            // Nearest-neighbor expansion repeats an entire completed pixel row.
            pixels.copy_within((py - 1) * width..py * width, py * width);
            continue;
        }
        previous_axis = Some(axis);
        let row = &mut pixels[py * width..(py + 1) * width];
        match orientation {
            GraphOrientation::Vertical if axis < data_rows => {
                fill_heat_row(row, visible_cols, |col| {
                    sync_heat_rgba(matrix[axis * cols.total + cols.first + col], lo, hi)
                });
            }
            GraphOrientation::Vertical => {} // Newly allocated pixels are transparent.
            GraphOrientation::Horizontal => {
                fill_heat_row(row, total_rows.max(1), |source_row| {
                    if source_row < data_rows {
                        sync_heat_rgba(matrix[source_row * cols.total + axis], lo, hi)
                    } else {
                        [0; 4]
                    }
                });
            }
        }
    }
    Some(image)
}

fn sync_heat_rgba(value: f64, lo: f64, hi: f64) -> [u8; 4] {
    let color = sync_viridis(sync_heat_norm01(value, lo, hi));
    color.map(|channel| (channel * 255.0).round().clamp(0.0, 255.0) as u8)
}

fn fill_heat_row(pixels: &mut [[u8; 4]], samples: usize, mut color: impl FnMut(usize) -> [u8; 4]) {
    let width = pixels.len();
    if samples >= width {
        for (px, pixel) in pixels.iter_mut().enumerate() {
            *pixel = color(px * samples / width);
        }
    } else {
        // Convert each sampled matrix value once and fill its destination run.
        let mut start = 0;
        for sample in 0..samples {
            let end = ((sample + 1) * width).div_ceil(samples);
            pixels[start..end].fill(color(sample));
            start = end;
        }
    }
}
