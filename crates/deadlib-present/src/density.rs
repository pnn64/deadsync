use deadlib_render::MeshVertex;
use std::sync::Arc;

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

#[derive(Clone, Copy, Debug)]
struct HistWindow {
    left: f32,
    right: f32,
    li: usize,
    ri: usize,
    point_count: usize,
    full_range: bool,
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

#[inline(always)]
fn write_hist_segment(
    dst: &mut [MeshVertex],
    written: usize,
    a: HistCol,
    b: HistCol,
    left: f32,
    bottom_y: f32,
    bottom_color: [f32; 4],
) -> usize {
    let ax = a.x - left;
    let bx = b.x - left;

    let verts = [
        MeshVertex {
            pos: [ax, bottom_y],
            color: bottom_color,
        },
        MeshVertex {
            pos: [ax, a.top_y],
            color: a.top_color,
        },
        MeshVertex {
            pos: [bx, bottom_y],
            color: bottom_color,
        },
        MeshVertex {
            pos: [ax, a.top_y],
            color: a.top_color,
        },
        MeshVertex {
            pos: [bx, b.top_y],
            color: b.top_color,
        },
        MeshVertex {
            pos: [bx, bottom_y],
            color: bottom_color,
        },
    ];
    dst[written..written + verts.len()].copy_from_slice(&verts);
    written + verts.len()
}

impl DensityHistCache {
    fn visible_window(&self, offset: f32, visible_width: f32) -> Option<HistWindow> {
        let visible_width = visible_width.max(0.0);
        if visible_width <= 0.0 || self.scaled_width <= 0.0 || self.height <= 0.0 {
            return None;
        }

        let left = offset.clamp(0.0, self.scaled_width);
        let right = (left + visible_width).clamp(0.0, self.scaled_width);
        if self.cols.is_empty() || left >= right {
            return None;
        }

        let cols = &self.cols;
        let full_range = left <= cols[0].x && right >= cols[cols.len() - 1].x;
        if full_range {
            if cols.len() < 2 {
                return None;
            }
            return Some(HistWindow {
                left,
                right,
                li: 0,
                ri: cols.len(),
                point_count: cols.len(),
                full_range: true,
            });
        }

        let li = cols.partition_point(|p| p.x < left);
        if li >= cols.len() {
            return None;
        }
        let ri = cols.partition_point(|p| p.x <= right);
        let point_count =
            ri.saturating_sub(li) + usize::from(li > 0) + usize::from(ri < cols.len() && ri > 0);
        if point_count < 2 {
            return None;
        }

        Some(HistWindow {
            left,
            right,
            li,
            ri,
            point_count,
            full_range: false,
        })
    }

    fn visit_window_points(&self, window: HistWindow, mut push: impl FnMut(HistCol)) {
        let cols = &self.cols;
        if window.full_range {
            for &point in cols.iter() {
                push(point);
            }
            return;
        }

        if window.li > 0 {
            push(interp_hist_col(
                cols[window.li - 1],
                cols[window.li],
                window.left,
            ));
        }
        for &point in &cols[window.li..window.ri] {
            push(point);
        }
        if window.ri < cols.len() && window.ri > 0 {
            push(interp_hist_col(
                cols[window.ri - 1],
                cols[window.ri],
                window.right,
            ));
        }
    }

    fn fill_mesh_vertices(&self, dst: &mut [MeshVertex], window: HistWindow) -> usize {
        let mut prev: Option<HistCol> = None;
        let mut written = 0usize;
        self.visit_window_points(window, |point| {
            if let Some(last) = prev {
                written = write_hist_segment(
                    dst,
                    written,
                    last,
                    point,
                    window.left,
                    self.height,
                    self.bottom_color,
                );
            }
            prev = Some(point);
        });
        written
    }

    pub fn mesh(&self, offset: f32, visible_width: f32) -> Vec<MeshVertex> {
        let Some(window) = self.visible_window(offset, visible_width) else {
            return Vec::new();
        };

        let mut out = Vec::with_capacity((window.point_count - 1) * 6);
        let mut prev: Option<HistCol> = None;

        let push_point = |point: HistCol| {
            if let Some(last) = prev {
                push_hist_segment(
                    &mut out,
                    last,
                    point,
                    window.left,
                    self.height,
                    self.bottom_color,
                );
            }
            prev = Some(point);
        };

        self.visit_window_points(window, push_point);
        out
    }
}

pub fn update_density_hist_mesh(
    mesh: &mut Option<Arc<[MeshVertex]>>,
    cache: Option<&DensityHistCache>,
    offset: f32,
    visible_width: f32,
) {
    let Some(cache) = cache else {
        *mesh = None;
        return;
    };
    let Some(window) = cache.visible_window(offset, visible_width) else {
        *mesh = None;
        return;
    };

    let len = (window.point_count - 1) * 6;
    if let Some(existing) = mesh.as_mut().and_then(Arc::get_mut)
        && existing.len() == len
    {
        let written = cache.fill_mesh_vertices(existing, window);
        debug_assert_eq!(written, len);
        return;
    }

    let mut verts = vec![MeshVertex::default(); len];
    let written = cache.fill_mesh_vertices(&mut verts, window);
    debug_assert_eq!(written, len);
    *mesh = Some(Arc::from(verts.into_boxed_slice()));
}

pub fn update_density_hist_mesh_reusable(
    mesh: &mut Option<Arc<Vec<MeshVertex>>>,
    cache: Option<&DensityHistCache>,
    offset: f32,
    visible_width: f32,
) {
    let Some(cache) = cache else {
        *mesh = None;
        return;
    };
    let Some(window) = cache.visible_window(offset, visible_width) else {
        *mesh = None;
        return;
    };

    let len = (window.point_count - 1) * 6;
    let can_reuse = mesh
        .as_mut()
        .is_some_and(|vertices| Arc::get_mut(vertices).is_some());
    if !can_reuse {
        *mesh = Some(Arc::new(Vec::with_capacity(len)));
    }

    let vertices = Arc::get_mut(mesh.as_mut().expect("mesh initialized above"))
        .expect("mesh has no other owners");
    vertices.resize(len, MeshVertex::default());
    let written = cache.fill_mesh_vertices(vertices, window);
    debug_assert_eq!(written, len);
}

const DENSITY_LIFE_MIN_LEN_SQ: f32 = 0.000_000_01_f32;
const DENSITY_LIFE_SEGMENT_VERTS: usize = 6;

#[derive(Clone, Copy, Debug)]
struct LifeWindow {
    left: f32,
    right: f32,
    start: usize,
    end: usize,
    point_count: usize,
    clip_left: bool,
}

#[inline(always)]
fn interp_life_point(a: [f32; 2], b: [f32; 2], x: f32) -> [f32; 2] {
    let dx = (b[0] - a[0]).max(0.000_001_f32);
    let t = ((x - a[0]) / dx).clamp(0.0, 1.0);
    [x, lerp(a[1], b[1], t)]
}

#[inline(always)]
fn density_life_window_point(points: &[[f32; 2]], window: LifeWindow, index: usize) -> [f32; 2] {
    if window.clip_left && index == 0 {
        return interp_life_point(points[window.start - 1], points[window.start], window.left);
    }
    let source_index = index - usize::from(window.clip_left) + window.start;
    if source_index < window.end {
        return points[source_index];
    }
    interp_life_point(points[window.end - 1], points[window.end], window.right)
}

#[inline(always)]
fn density_life_normal(a: [f32; 2], b: [f32; 2], half: f32) -> [f32; 2] {
    let dx = b[0] - a[0];
    let dy = b[1] - a[1];
    let len = dx.hypot(dy);
    if len <= f32::EPSILON {
        return [0.0, 0.0];
    }
    [-dy / len * half, dx / len * half]
}

#[inline(always)]
fn density_life_join(prev: [f32; 2], current: [f32; 2], next: [f32; 2], half: f32) -> [f32; 2] {
    let prev_normal = density_life_normal(prev, current, 1.0);
    let next_normal = density_life_normal(current, next, 1.0);
    let miter = [
        prev_normal[0] + next_normal[0],
        prev_normal[1] + next_normal[1],
    ];
    let miter_len = miter[0].hypot(miter[1]);
    if miter_len <= f32::EPSILON {
        return [prev_normal[0] * half, prev_normal[1] * half];
    }
    let miter = [miter[0] / miter_len, miter[1] / miter_len];
    let denom = miter[0] * prev_normal[0] + miter[1] * prev_normal[1];
    if denom.abs() <= 0.1 {
        return [next_normal[0] * half, next_normal[1] * half];
    }
    let scale = (half / denom).clamp(-half * 4.0, half * 4.0);
    [miter[0] * scale, miter[1] * scale]
}

#[inline(always)]
fn density_life_offset(
    points: &[[f32; 2]],
    window: LifeWindow,
    index: usize,
    half: f32,
) -> [f32; 2] {
    let current = density_life_window_point(points, window, index);
    if index == 0 {
        return density_life_normal(current, density_life_window_point(points, window, 1), half);
    }
    if index + 1 == window.point_count {
        return density_life_normal(
            density_life_window_point(points, window, index - 1),
            current,
            half,
        );
    }
    density_life_join(
        density_life_window_point(points, window, index - 1),
        current,
        density_life_window_point(points, window, index + 1),
        half,
    )
}

#[inline(always)]
fn write_density_life_segment(
    dst: &mut [MeshVertex],
    written: usize,
    a: [f32; 2],
    b: [f32; 2],
    a_offset: [f32; 2],
    b_offset: [f32; 2],
    color: [f32; 4],
) -> usize {
    let l0 = [a[0] + a_offset[0], a[1] + a_offset[1]];
    let r0 = [a[0] - a_offset[0], a[1] - a_offset[1]];
    let l1 = [b[0] + b_offset[0], b[1] + b_offset[1]];
    let r1 = [b[0] - b_offset[0], b[1] - b_offset[1]];

    let verts = [
        MeshVertex { pos: l0, color },
        MeshVertex { pos: r0, color },
        MeshVertex { pos: l1, color },
        MeshVertex { pos: r0, color },
        MeshVertex { pos: r1, color },
        MeshVertex { pos: l1, color },
    ];
    dst[written..written + verts.len()].copy_from_slice(&verts);
    written + verts.len()
}

#[inline(always)]
fn fill_density_life_vertices(
    dst: &mut [MeshVertex],
    points: &[[f32; 2]],
    window: LifeWindow,
    half: f32,
    color: [f32; 4],
) -> usize {
    let mut written = 0usize;
    for index in 0..window.point_count - 1 {
        let mut a = density_life_window_point(points, window, index);
        let mut b = density_life_window_point(points, window, index + 1);
        let dx = b[0] - a[0];
        let dy = b[1] - a[1];
        if dx.mul_add(dx, dy * dy) <= DENSITY_LIFE_MIN_LEN_SQ {
            continue;
        }
        a[0] -= window.left;
        b[0] -= window.left;
        written = write_density_life_segment(
            dst,
            written,
            a,
            b,
            density_life_offset(points, window, index, half),
            density_life_offset(points, window, index + 1, half),
            color,
        );
    }
    written
}

#[inline(always)]
fn density_life_mesh_window(
    points: &[[f32; 2]],
    offset: f32,
    width: f32,
    thickness: f32,
) -> Option<(LifeWindow, usize, f32)> {
    if points.len() < 2 || width <= 0.0_f32 || thickness <= 0.0_f32 {
        return None;
    }

    let left = offset.max(0.0);
    let right = left + width;
    let start = points.partition_point(|p| p[0] < left);
    let end = points.partition_point(|p| p[0] <= right);
    let clip_left = start > 0 && start < points.len() && points[start][0] > left;
    let clip_right = end > 0 && end < points.len() && points[end - 1][0] < right;
    let point_count = end.saturating_sub(start) + usize::from(clip_left) + usize::from(clip_right);
    if point_count < 2 {
        return None;
    }

    let window = LifeWindow {
        left,
        right,
        start,
        end,
        point_count,
        clip_left,
    };
    let segment_count = (0..point_count - 1)
        .filter(|&index| {
            let a = density_life_window_point(points, window, index);
            let b = density_life_window_point(points, window, index + 1);
            let dx = b[0] - a[0];
            let dy = b[1] - a[1];
            dx.mul_add(dx, dy * dy) > DENSITY_LIFE_MIN_LEN_SQ
        })
        .count();
    (segment_count != 0).then_some((
        window,
        segment_count * DENSITY_LIFE_SEGMENT_VERTS,
        thickness * 0.5_f32,
    ))
}

pub fn update_density_life_mesh(
    mesh: &mut Option<Arc<[MeshVertex]>>,
    points: &[[f32; 2]],
    offset: f32,
    width: f32,
    thickness: f32,
    color: [f32; 4],
) {
    let Some((window, len, half)) = density_life_mesh_window(points, offset, width, thickness)
    else {
        *mesh = None;
        return;
    };
    if let Some(existing) = mesh.as_mut().and_then(Arc::get_mut)
        && existing.len() == len
    {
        let written = fill_density_life_vertices(existing, points, window, half, color);
        debug_assert_eq!(written, len);
        return;
    }

    let mut verts = vec![MeshVertex::default(); len];
    let written = fill_density_life_vertices(&mut verts, points, window, half, color);
    debug_assert_eq!(written, len);
    *mesh = Some(Arc::from(verts.into_boxed_slice()));
}

/// Update a dynamic life graph while retaining its growable vertex allocation.
///
/// The buffer is mutated only while uniquely owned. If a renderer still holds
/// the preceding frame, a replacement is allocated and that frame remains
/// immutable.
pub fn update_density_life_mesh_reusable(
    mesh: &mut Option<Arc<Vec<MeshVertex>>>,
    points: &[[f32; 2]],
    offset: f32,
    width: f32,
    thickness: f32,
    color: [f32; 4],
) {
    let Some((window, len, half)) = density_life_mesh_window(points, offset, width, thickness)
    else {
        *mesh = None;
        return;
    };

    if mesh.as_mut().and_then(Arc::get_mut).is_none() {
        *mesh = Some(Arc::new(Vec::with_capacity(len)));
    }
    let vertices = mesh
        .as_mut()
        .and_then(Arc::get_mut)
        .expect("replacement density life mesh must be uniquely owned");
    vertices.resize(len, MeshVertex::default());
    let written = fill_density_life_vertices(vertices, points, window, half, color);
    debug_assert_eq!(written, len);
}

/// Build the 2px-style connected graph used by ITGmania's `GraphLine` actor.
///
/// Each pair of points is an independent quad and every sample gets a
/// four-sided radius cap. This intentionally differs from a native line strip:
/// evaluation lifelines rely on GraphDisplay's capped joins and endpoints.
pub fn build_graph_line_mesh(
    points: &[[f32; 2]],
    thickness: f32,
    color: [f32; 4],
) -> Vec<MeshVertex> {
    const CAP_SIDES: usize = 4;
    const SEGMENT_VERTS: usize = 6;
    const CAP_VERTS: usize = CAP_SIDES * 3;

    if points.len() < 2 || !thickness.is_finite() || thickness <= 0.0 {
        return Vec::new();
    }

    let half = thickness * 0.5;
    let segment_count = points
        .windows(2)
        .filter(|pair| {
            let dx = pair[1][0] - pair[0][0];
            let dy = pair[1][1] - pair[0][1];
            dx.mul_add(dx, dy * dy) > DENSITY_LIFE_MIN_LEN_SQ
        })
        .count();
    let mut out = Vec::with_capacity(segment_count * SEGMENT_VERTS + points.len() * CAP_VERTS);

    for pair in points.windows(2) {
        let a = pair[0];
        let b = pair[1];
        let dx = b[0] - a[0];
        let dy = b[1] - a[1];
        let len = dx.hypot(dy);
        if len * len <= DENSITY_LIFE_MIN_LEN_SQ {
            continue;
        }

        // GraphLine uses (dy, -dx) as its segment normal.
        let offset = [dy / len * half, -dx / len * half];
        let v0 = [a[0] + offset[0], a[1] + offset[1]];
        let v1 = [a[0] - offset[0], a[1] - offset[1]];
        let v2 = [b[0] - offset[0], b[1] - offset[1]];
        let v3 = [b[0] + offset[0], b[1] + offset[1]];
        out.extend_from_slice(&[
            MeshVertex { pos: v0, color },
            MeshVertex { pos: v1, color },
            MeshVertex { pos: v2, color },
            MeshVertex { pos: v0, color },
            MeshVertex { pos: v2, color },
            MeshVertex { pos: v3, color },
        ]);
    }

    // Four subdivisions are exactly what ITGmania uses. At a one-pixel radius
    // this diamond-shaped fan reads as a round cap while keeping the reference
    // geometry and coverage.
    const CAP_DIRS: [[f32; 2]; CAP_SIDES + 1] =
        [[1.0, 0.0], [0.0, -1.0], [-1.0, 0.0], [0.0, 1.0], [1.0, 0.0]];
    for &point in points {
        for side in 0..CAP_SIDES {
            let a = CAP_DIRS[side];
            let b = CAP_DIRS[side + 1];
            out.extend_from_slice(&[
                MeshVertex { pos: point, color },
                MeshVertex {
                    pos: [point[0] + a[0] * half, point[1] + a[1] * half],
                    color,
                },
                MeshVertex {
                    pos: [point[0] + b[0] * half, point[1] + b[1] * half],
                    color,
                },
            ]);
        }
    }

    out
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

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_cache() -> DensityHistCache {
        build_density_histogram_cache(
            &[0.0, 0.0, 2.0, 5.0, 3.0, 4.0, 1.0],
            5.0,
            &[0.0, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0],
            0.0,
            6.0,
            240.0,
            64.0,
            None,
            1.0,
        )
        .expect("sample cache")
    }

    fn assert_mesh_matches(actual: &[MeshVertex], expected: &[MeshVertex]) {
        assert_eq!(actual.len(), expected.len());
        for (index, (actual, expected)) in actual.iter().zip(expected.iter()).enumerate() {
            assert_eq!(actual.pos, expected.pos, "pos mismatch at {index}");
            assert_eq!(actual.color, expected.color, "color mismatch at {index}");
        }
    }

    #[test]
    fn update_density_hist_mesh_reuses_existing_buffer_when_vertex_count_matches() {
        let cache = sample_cache();
        let mut mesh = None;

        update_density_hist_mesh(&mut mesh, Some(&cache), 48.0, 120.0);
        let expected = cache.mesh(48.0, 120.0);
        let first_ptr = mesh.as_ref().expect("mesh").as_ptr();
        assert_mesh_matches(mesh.as_ref().expect("mesh"), &expected);

        update_density_hist_mesh(&mut mesh, Some(&cache), 48.0, 120.0);
        let second_ptr = mesh.as_ref().expect("mesh").as_ptr();

        assert_eq!(first_ptr, second_ptr);
        assert_mesh_matches(mesh.as_ref().expect("mesh"), &expected);
    }

    #[test]
    fn update_density_hist_mesh_clears_mesh_without_cache() {
        let cache = sample_cache();
        let mut mesh = None;

        update_density_hist_mesh(&mut mesh, Some(&cache), 0.0, 120.0);
        assert!(mesh.is_some());

        update_density_hist_mesh(&mut mesh, None, 0.0, 120.0);
        assert!(mesh.is_none());
    }

    #[test]
    fn reusable_density_hist_mesh_matches_shared_mesh_and_reuses_changed_lengths() {
        let cache = sample_cache();
        let mut shared = None;
        let mut reusable = None;

        update_density_hist_mesh(&mut shared, Some(&cache), 0.0, 240.0);
        update_density_hist_mesh_reusable(&mut reusable, Some(&cache), 0.0, 240.0);
        assert_mesh_matches(
            reusable.as_deref().expect("reusable mesh"),
            shared.as_deref().expect("shared mesh"),
        );
        let first_ptr = reusable.as_ref().expect("reusable mesh").as_ptr();
        let first_len = reusable.as_ref().expect("reusable mesh").len();

        update_density_hist_mesh(&mut shared, Some(&cache), 48.0, 120.0);
        update_density_hist_mesh_reusable(&mut reusable, Some(&cache), 48.0, 120.0);
        assert_mesh_matches(
            reusable.as_deref().expect("reusable mesh"),
            shared.as_deref().expect("shared mesh"),
        );

        assert_ne!(reusable.as_ref().expect("reusable mesh").len(), first_len);
        assert_eq!(
            reusable.as_ref().expect("reusable mesh").as_ptr(),
            first_ptr
        );
    }

    #[test]
    fn reusable_density_hist_mesh_preserves_a_shared_previous_frame() {
        let cache = sample_cache();
        let mut mesh = None;

        update_density_hist_mesh_reusable(&mut mesh, Some(&cache), 0.0, 240.0);
        let previous = Arc::clone(mesh.as_ref().expect("reusable mesh"));
        let previous_vertices = previous.to_vec();

        update_density_hist_mesh_reusable(&mut mesh, Some(&cache), 48.0, 120.0);

        assert_mesh_matches(previous.as_slice(), &previous_vertices);
        assert!(!Arc::ptr_eq(
            &previous,
            mesh.as_ref().expect("replacement mesh")
        ));
    }

    #[test]
    fn build_density_histogram_mesh_preserves_subpixel_bursts() {
        let mesh = build_density_histogram_mesh(
            &[1.0, 10.0, 1.0],
            10.0,
            &[0.0, 0.25, 0.5],
            0.0,
            1.0,
            1.0,
            10.0,
            0.0,
            1.0,
            None,
            1.0,
        );

        assert_eq!(mesh.len(), 18);
        assert!(mesh.iter().any(|v| v.pos == [0.25, 0.0]));
    }

    #[test]
    fn update_density_life_mesh_matches_line_strip_joins() {
        let mut mesh = None;
        let points = [[0.0, 0.0], [10.0, 0.0], [10.0, 10.0]];

        update_density_life_mesh(&mut mesh, &points, 0.0, 32.0, 4.0, [1.0; 4]);

        let mesh = mesh.expect("life mesh");
        assert_eq!(mesh.len(), 2 * DENSITY_LIFE_SEGMENT_VERTS);
        let has_pos = |expected: [f32; 2]| {
            mesh.iter().any(|vertex| {
                (vertex.pos[0] - expected[0]).abs() < 0.000_1
                    && (vertex.pos[1] - expected[1]).abs() < 0.000_1
            })
        };
        assert!(has_pos([8.0, 2.0]));
        assert!(has_pos([12.0, -2.0]));
        assert!(!mesh.iter().any(|vertex| points.contains(&vertex.pos)));
    }

    #[test]
    fn graph_line_mesh_matches_graph_display_quads_and_caps() {
        let points = [[4.0, 8.0], [14.0, 8.0], [14.0, 18.0]];
        let color = [1.0, 1.0, 1.0, 1.0];

        let mesh = build_graph_line_mesh(&points, 2.0, color);

        assert_eq!(mesh.len(), 2 * 6 + 3 * 12);
        assert!(mesh.iter().all(|vertex| vertex.color == color));
        assert!(mesh.iter().any(|vertex| vertex.pos == [3.0, 8.0]));
        assert!(mesh.iter().any(|vertex| vertex.pos == [15.0, 18.0]));
        assert!(mesh.iter().any(|vertex| vertex.pos == [4.0, 7.0]));
    }

    #[test]
    fn update_density_life_mesh_clips_fractional_window_edges() {
        let mut mesh = None;
        let points = [[0.0, 8.0], [12.0, 8.0], [24.0, 8.0]];

        update_density_life_mesh(&mut mesh, &points, 0.25, 10.0, 2.0, [1.0; 4]);

        let mesh = mesh.expect("life mesh");
        assert_eq!(mesh.len(), DENSITY_LIFE_SEGMENT_VERTS);
        assert!(
            mesh.iter()
                .all(|vertex| vertex.pos[0] == 0.0 || vertex.pos[0] == 10.0)
        );
        assert!(
            mesh.iter()
                .all(|vertex| vertex.pos[1] == 7.0 || vertex.pos[1] == 9.0)
        );
    }

    #[test]
    fn update_density_life_mesh_reuses_existing_buffer_when_vertex_count_matches() {
        let mut mesh = None;
        let points = [[0.0, 8.0], [12.0, 8.0], [24.0, 20.0]];

        update_density_life_mesh(&mut mesh, &points, 0.0, 32.0, 2.0, [1.0, 1.0, 1.0, 1.0]);
        let expected = mesh.as_ref().expect("life mesh").to_vec();
        let first_ptr = mesh.as_ref().expect("life mesh").as_ptr();

        update_density_life_mesh(&mut mesh, &points, 0.0, 32.0, 2.0, [1.0, 1.0, 1.0, 1.0]);
        let second_ptr = mesh.as_ref().expect("life mesh").as_ptr();

        assert_eq!(first_ptr, second_ptr);
        assert_mesh_matches(mesh.as_ref().expect("life mesh"), &expected);
    }

    #[test]
    fn reusable_density_life_mesh_matches_shared_mesh_and_reuses_changed_lengths() {
        let points = [[0.0, 8.0], [8.0, 12.0], [16.0, 6.0], [24.0, 20.0]];
        let mut shared = None;
        let mut reusable = None;

        update_density_life_mesh(&mut shared, &points, 0.0, 32.0, 2.0, [0.5, 0.75, 1.0, 1.0]);
        update_density_life_mesh_reusable(
            &mut reusable,
            &points,
            0.0,
            32.0,
            2.0,
            [0.5, 0.75, 1.0, 1.0],
        );
        assert_mesh_matches(
            reusable.as_ref().expect("reusable life mesh"),
            shared.as_ref().expect("shared life mesh"),
        );
        let allocation = reusable.as_ref().expect("reusable life mesh").as_ptr();

        update_density_life_mesh(
            &mut shared,
            &points[..3],
            0.0,
            32.0,
            2.0,
            [0.5, 0.75, 1.0, 1.0],
        );
        update_density_life_mesh_reusable(
            &mut reusable,
            &points[..3],
            0.0,
            32.0,
            2.0,
            [0.5, 0.75, 1.0, 1.0],
        );

        assert_eq!(
            reusable.as_ref().expect("reusable life mesh").as_ptr(),
            allocation
        );
        assert_mesh_matches(
            reusable.as_ref().expect("reusable life mesh"),
            shared.as_ref().expect("shared life mesh"),
        );
    }

    #[test]
    fn reusable_density_life_mesh_keeps_a_busy_previous_frame_immutable() {
        let points = [[0.0, 8.0], [12.0, 8.0], [24.0, 20.0]];
        let mut mesh = None;
        update_density_life_mesh_reusable(&mut mesh, &points, 0.0, 32.0, 2.0, [1.0; 4]);
        let previous = Arc::clone(mesh.as_ref().expect("life mesh"));
        let previous_vertices = previous.as_slice().to_vec();

        update_density_life_mesh_reusable(&mut mesh, &points[..2], 0.0, 32.0, 2.0, [0.5; 4]);

        assert!(!Arc::ptr_eq(
            mesh.as_ref().expect("replacement mesh"),
            &previous
        ));
        assert_mesh_matches(previous.as_slice(), &previous_vertices);
    }
}
