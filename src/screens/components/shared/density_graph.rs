use crate::core::gfx::MeshVertex;
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DensityGraphSlot {
    SelectMusicP1,
    SelectMusicP2,
}

#[derive(Debug, Clone)]
pub struct DensityGraphSource {
    pub max_nps: f64,
    pub measure_nps_vec: Vec<f64>,
    pub measure_seconds_vec: Vec<f32>,
    pub first_second: f32,
    pub last_second: f32,
}

#[inline(always)]
fn lerp(a: f32, b: f32, t: f32) -> f32 {
    (b - a).mul_add(t, a)
}

#[inline(always)]
fn lerp_color(t: f32, a: [f32; 4], b: [f32; 4]) -> [f32; 4] {
    [
        lerp(a[0], b[0], t),
        lerp(a[1], b[1], t),
        lerp(a[2], b[2], t),
        lerp(a[3], b[3], t),
    ]
}

#[inline(always)]
fn desaturate_rgb(mut c: [f32; 4], desat: f32) -> [f32; 4] {
    let d = desat.clamp(0.0, 1.0);
    if d <= 0.0 {
        return c;
    }
    let luma = (0.3 * c[0]).mul_add(1.0, (0.59 * c[1]).mul_add(1.0, 0.11 * c[2]));
    c[0] = c[0] + d * (luma - c[0]);
    c[1] = c[1] + d * (luma - c[1]);
    c[2] = c[2] + d * (luma - c[2]);
    c
}

#[inline(always)]
fn sl_hist_colors(desaturation: Option<f32>, alpha: f32) -> ([f32; 4], [f32; 4]) {
    let a = alpha.clamp(0.0, 1.0);
    let mut blue = [0.0, 0.678, 0.753, a];
    let mut purple = [0.51, 0.0, 0.631, a];
    if let Some(d) = desaturation {
        blue = desaturate_rgb(blue, d);
        purple = desaturate_rgb(purple, d);
    }
    (blue, purple)
}

#[derive(Clone, Copy, Debug)]
struct HistCol {
    x: f32,
    top_y: f32,
    top_color: [f32; 4],
}

pub struct DensityHistCache {
    cols: Arc<[HistCol]>,
    bottom_color: [f32; 4],
    height: f32,
    scaled_width: f32,
}

fn build_hist_cols(
    measure_nps: &[f64],
    peak_nps: f64,
    measure_seconds: &[f32],
    first_second: f32,
    last_second: f32,
    width: f32,
    height: f32,
    desaturation: Option<f32>,
    alpha: f32,
) -> (Vec<HistCol>, [f32; 4]) {
    let (blue, purple) = sl_hist_colors(desaturation, alpha);
    let denom_t = last_second - first_second;
    if width <= 0.0 || height <= 0.0 || !denom_t.is_finite() || denom_t <= 0.0 {
        return (Vec::new(), blue);
    }
    let peak = (peak_nps as f32).max(0.000_001);
    if measure_nps.len() <= 1 || !peak.is_finite() {
        return (Vec::new(), blue);
    }

    let mut cols: Vec<HistCol> = Vec::with_capacity(measure_nps.len().saturating_add(1));
    let mut first_step_has_occurred = false;

    for (i, &nps_f64) in measure_nps.iter().enumerate() {
        let nps = nps_f64 as f32;
        if nps > 0.0 {
            first_step_has_occurred = true;
        }
        if !first_step_has_occurred {
            continue;
        }

        let Some(&t) = measure_seconds.get(i) else {
            continue;
        };
        let x = ((t - first_second) / denom_t) * width;
        let bar_h = ((nps / peak) * height).round();
        let top_y = height - bar_h;
        let frac = (bar_h / height).abs();
        let top_color = lerp_color(frac, blue, purple);

        if cols.len() >= 2 {
            let a = cols[cols.len() - 1];
            let b = cols[cols.len() - 2];
            if a.top_y == top_y && b.top_y == top_y {
                let last_ix = cols.len() - 1;
                cols[last_ix].x = x;
                continue;
            }
        }

        cols.push(HistCol {
            x,
            top_y,
            top_color,
        });
    }

    if first_step_has_occurred && measure_nps.last().is_some_and(|&n| n != 0.0) {
        cols.push(HistCol {
            x: width,
            top_y: height,
            top_color: blue,
        });
    }

    (cols, blue)
}

pub fn build_density_histogram_cache(
    measure_nps: &[f64],
    peak_nps: f64,
    measure_seconds: &[f32],
    first_second: f32,
    last_second: f32,
    scaled_width: f32,
    height: f32,
    desaturation: Option<f32>,
    alpha: f32,
) -> Option<DensityHistCache> {
    let scaled_width = scaled_width.max(0.0);
    let height = height.max(0.0);
    if scaled_width <= 0.0 || height <= 0.0 {
        return None;
    }
    let (cols, bottom_color) = build_hist_cols(
        measure_nps,
        peak_nps,
        measure_seconds,
        first_second,
        last_second,
        scaled_width,
        height,
        desaturation,
        alpha,
    );
    if cols.len() < 2 {
        return None;
    }
    Some(DensityHistCache {
        cols: Arc::from(cols.into_boxed_slice()),
        bottom_color,
        height,
        scaled_width,
    })
}

#[inline(always)]
fn interp_hist_col(a: HistCol, b: HistCol, x: f32) -> HistCol {
    let dx = (b.x - a.x).max(0.000_001);
    let t = ((x - a.x) / dx).clamp(0.0, 1.0);
    HistCol {
        x,
        top_y: lerp(a.top_y, b.top_y, t),
        top_color: lerp_color(t, a.top_color, b.top_color),
    }
}

#[inline(always)]
fn push_hist_segment(
    out: &mut Vec<MeshVertex>,
    a: HistCol,
    b: HistCol,
    left: f32,
    bottom_y: f32,
    bottom_color: [f32; 4],
) {
    let ax = a.x - left;
    let bx = b.x - left;

    out.push(MeshVertex {
        pos: [ax, bottom_y],
        color: bottom_color,
    });
    out.push(MeshVertex {
        pos: [ax, a.top_y],
        color: a.top_color,
    });
    out.push(MeshVertex {
        pos: [bx, bottom_y],
        color: bottom_color,
    });

    out.push(MeshVertex {
        pos: [ax, a.top_y],
        color: a.top_color,
    });
    out.push(MeshVertex {
        pos: [bx, b.top_y],
        color: b.top_color,
    });
    out.push(MeshVertex {
        pos: [bx, bottom_y],
        color: bottom_color,
    });
}

impl DensityHistCache {
    pub fn mesh(&self, offset: f32, visible_width: f32) -> Vec<MeshVertex> {
        let visible_width = visible_width.max(0.0);
        if visible_width <= 0.0 || self.scaled_width <= 0.0 || self.height <= 0.0 {
            return Vec::new();
        }

        let left = offset.clamp(0.0, self.scaled_width);
        let right = (left + visible_width).clamp(0.0, self.scaled_width);
        if self.cols.is_empty() || left >= right {
            return Vec::new();
        }

        let cols = &self.cols;
        let point_count = if left <= cols[0].x && right >= cols[cols.len() - 1].x {
            cols.len()
        } else {
            let li = cols.partition_point(|p| p.x < left);
            let ri = cols.partition_point(|p| p.x <= right);
            ri.saturating_sub(li) + usize::from(li > 0) + usize::from(ri < cols.len() && ri > 0)
        };
        if point_count < 2 {
            return Vec::new();
        }

        let mut out = Vec::with_capacity((point_count - 1) * 6);
        let mut prev: Option<HistCol> = None;

        let mut push_point = |point: HistCol| {
            if let Some(last) = prev {
                push_hist_segment(&mut out, last, point, left, self.height, self.bottom_color);
            }
            prev = Some(point);
        };

        if left <= cols[0].x && right >= cols[cols.len() - 1].x {
            for &point in cols.iter() {
                push_point(point);
            }
            return out;
        }

        let li = cols.partition_point(|p| p.x < left);
        if li >= cols.len() {
            return Vec::new();
        }
        let ri = cols.partition_point(|p| p.x <= right);

        if li > 0 {
            push_point(interp_hist_col(cols[li - 1], cols[li], left));
        }
        for &point in &cols[li..ri] {
            push_point(point);
        }
        if ri < cols.len() && ri > 0 {
            push_point(interp_hist_col(cols[ri - 1], cols[ri], right));
        }

        out
    }
}

#[inline(always)]
fn density_life_vertex_count(points: &[[f32; 2]], start: usize, end: usize) -> usize {
    const MIN_LEN_SQ: f32 = 0.000_000_01_f32;

    let mut prev: Option<[f32; 2]> = None;
    let mut count = 0usize;
    for &point in &points[start..end] {
        if let Some(a) = prev {
            let dx = point[0] - a[0];
            let dy = point[1] - a[1];
            let len_sq = dx.mul_add(dx, dy * dy);
            if len_sq > MIN_LEN_SQ {
                count += 6;
            }
        }
        prev = Some(point);
    }
    count
}

#[inline(always)]
fn fill_density_life_vertices(
    dst: &mut [MeshVertex],
    points: &[[f32; 2]],
    start: usize,
    end: usize,
    offset: f32,
    half: f32,
    color: [f32; 4],
) -> usize {
    const MIN_LEN_SQ: f32 = 0.000_000_01_f32;

    let mut prev: Option<[f32; 2]> = None;
    let mut written = 0usize;
    for &point in &points[start..end] {
        let p = [point[0] - offset, point[1]];
        let Some(a) = prev else {
            prev = Some(p);
            continue;
        };
        let dx = p[0] - a[0];
        let dy = p[1] - a[1];
        let len_sq = dx.mul_add(dx, dy * dy);
        if len_sq <= MIN_LEN_SQ {
            continue;
        }
        let inv_len = len_sq.sqrt().recip();
        let nx = -dy * inv_len * half;
        let ny = dx * inv_len * half;
        let l0 = [a[0] + nx, a[1] + ny];
        let r0 = [a[0] - nx, a[1] - ny];
        let l1 = [p[0] + nx, p[1] + ny];
        let r1 = [p[0] - nx, p[1] - ny];

        let verts = [
            MeshVertex { pos: l0, color },
            MeshVertex { pos: r0, color },
            MeshVertex { pos: l1, color },
            MeshVertex { pos: r0, color },
            MeshVertex { pos: r1, color },
            MeshVertex { pos: l1, color },
        ];
        dst[written..written + verts.len()].copy_from_slice(&verts);
        written += 6;
        prev = Some(p);
    }
    written
}

pub(crate) fn update_density_life_mesh(
    mesh: &mut Option<Arc<[MeshVertex]>>,
    points: &[[f32; 2]],
    offset: f32,
    width: f32,
    thickness: f32,
    color: [f32; 4],
) {
    if points.len() < 2 || width <= 0.0_f32 || thickness <= 0.0_f32 {
        *mesh = None;
        return;
    }

    let right = offset + width;
    let start = points.partition_point(|p| p[0] < offset);
    let end = points.partition_point(|p| p[0] <= right);
    if end.saturating_sub(start) < 2 {
        *mesh = None;
        return;
    }

    let len = density_life_vertex_count(points, start, end);
    if len == 0 {
        *mesh = None;
        return;
    }

    let half = thickness * 0.5_f32;
    if let Some(existing) = mesh.as_mut().and_then(Arc::get_mut)
        && existing.len() == len
    {
        let written = fill_density_life_vertices(existing, points, start, end, offset, half, color);
        debug_assert_eq!(written, len);
        return;
    }

    let mut verts = vec![MeshVertex::default(); len];
    let written = fill_density_life_vertices(&mut verts, points, start, end, offset, half, color);
    debug_assert_eq!(written, len);
    *mesh = Some(Arc::from(verts.into_boxed_slice()));
}

pub fn build_density_histogram_mesh(
    measure_nps: &[f64],
    peak_nps: f64,
    measure_seconds: &[f32],
    first_second: f32,
    last_second: f32,
    scaled_width: f32,
    height: f32,
    offset: f32,
    visible_width: f32,
    desaturation: Option<f32>,
    alpha: f32,
) -> Vec<MeshVertex> {
    let scaled_width = scaled_width.max(0.0);
    let height = height.max(0.0);
    let visible_width = visible_width.max(0.0);
    if scaled_width <= 0.0 || height <= 0.0 || visible_width <= 0.0 {
        return Vec::new();
    }

    let Some(cache) = build_density_histogram_cache(
        measure_nps,
        peak_nps,
        measure_seconds,
        first_second,
        last_second,
        scaled_width,
        height,
        desaturation,
        alpha,
    ) else {
        return Vec::new();
    };
    cache.mesh(offset, visible_width)
}
