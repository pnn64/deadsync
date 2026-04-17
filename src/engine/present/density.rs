use crate::engine::gfx::MeshVertex;
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
    let mut last_bucket: Option<i32> = None;

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
        // Cap histogram complexity to the rendered pixel width. Long charts can
        // have far more measures than horizontal pixels, especially for the top
        // gameplay NPS graph; keeping one column per pixel bucket preserves the
        // visible shape while preventing mesh size from scaling with chart length.
        let bucket = x.floor().clamp(i32::MIN as f32, i32::MAX as f32) as i32;

        if let Some(last) = cols.last_mut()
            && last_bucket == Some(bucket)
        {
            last.x = x;
            if top_y < last.top_y {
                last.top_y = top_y;
                last.top_color = top_color;
            }
            continue;
        }

        if cols.len() >= 2 {
            let a = cols[cols.len() - 1];
            let b = cols[cols.len() - 2];
            if a.top_y == top_y && b.top_y == top_y {
                let last_ix = cols.len() - 1;
                cols[last_ix].x = x;
                last_bucket = Some(bucket);
                continue;
            }
        }

        cols.push(HistCol {
            x,
            top_y,
            top_color,
        });
        last_bucket = Some(bucket);
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

const DENSITY_LIFE_MIN_LEN_SQ: f32 = 0.000_000_01_f32;
const DENSITY_LIFE_SEGMENT_VERTS: usize = 6;
const DENSITY_LIFE_CAP_SUBDIVISIONS: usize = 4;
const DENSITY_LIFE_CAP_VERTS: usize = DENSITY_LIFE_CAP_SUBDIVISIONS * 3;
// Match StepMania's low-cost fallback polyline joins/caps: a quad per segment plus
// a tiny fan at each vertex so sharp turns do not sprout stretched miters.
const DENSITY_LIFE_CAP_DIRS: [[f32; 2]; DENSITY_LIFE_CAP_SUBDIVISIONS + 1] =
    [[1.0, 0.0], [0.0, -1.0], [-1.0, 0.0], [0.0, 1.0], [1.0, 0.0]];

#[inline(always)]
fn density_life_segment_count(points: &[[f32; 2]], start: usize, end: usize) -> usize {
    let mut prev: Option<[f32; 2]> = None;
    let mut count = 0usize;
    for &point in &points[start..end] {
        if let Some(a) = prev {
            let dx = point[0] - a[0];
            let dy = point[1] - a[1];
            let len_sq = dx.mul_add(dx, dy * dy);
            if len_sq > DENSITY_LIFE_MIN_LEN_SQ {
                count += 1;
            }
        }
        prev = Some(point);
    }
    count
}

#[inline(always)]
fn density_life_vertex_count(points: &[[f32; 2]], start: usize, end: usize) -> usize {
    let point_count = end.saturating_sub(start);
    if point_count < 2 {
        return 0;
    }
    let segment_count = density_life_segment_count(points, start, end);
    if segment_count == 0 {
        return 0;
    }
    segment_count * DENSITY_LIFE_SEGMENT_VERTS + point_count * DENSITY_LIFE_CAP_VERTS
}

#[inline(always)]
fn write_density_life_segment(
    dst: &mut [MeshVertex],
    written: usize,
    a: [f32; 2],
    b: [f32; 2],
    half: f32,
    color: [f32; 4],
) -> usize {
    let dx = b[0] - a[0];
    let dy = b[1] - a[1];
    let inv_len = dx.mul_add(dx, dy * dy).sqrt().recip();
    let nx = -dy * inv_len * half;
    let ny = dx * inv_len * half;
    let l0 = [a[0] + nx, a[1] + ny];
    let r0 = [a[0] - nx, a[1] - ny];
    let l1 = [b[0] + nx, b[1] + ny];
    let r1 = [b[0] - nx, b[1] - ny];

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
fn write_density_life_cap(
    dst: &mut [MeshVertex],
    written: usize,
    center: [f32; 2],
    radius: f32,
    color: [f32; 4],
) -> usize {
    let mut written = written;
    for dirs in DENSITY_LIFE_CAP_DIRS.windows(2) {
        let p0 = [
            center[0] + dirs[0][0] * radius,
            center[1] + dirs[0][1] * radius,
        ];
        let p1 = [
            center[0] + dirs[1][0] * radius,
            center[1] + dirs[1][1] * radius,
        ];
        let verts = [
            MeshVertex { pos: center, color },
            MeshVertex { pos: p0, color },
            MeshVertex { pos: p1, color },
        ];
        dst[written..written + verts.len()].copy_from_slice(&verts);
        written += verts.len();
    }
    written
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
    let mut prev: Option<[f32; 2]> = None;
    let mut written = 0usize;
    for &point in &points[start..end] {
        let p = [point[0] - offset, point[1]];
        if let Some(a) = prev {
            let dx = p[0] - a[0];
            let dy = p[1] - a[1];
            let len_sq = dx.mul_add(dx, dy * dy);
            if len_sq > DENSITY_LIFE_MIN_LEN_SQ {
                written = write_density_life_segment(dst, written, a, p, half, color);
            }
        }
        prev = Some(p);
    }
    for &point in &points[start..end] {
        let p = [point[0] - offset, point[1]];
        written = write_density_life_cap(dst, written, p, half, color);
    }
    written
}

pub fn update_density_life_mesh(
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
    fn build_density_histogram_mesh_caps_columns_to_pixel_width() {
        let measure_nps: Vec<f64> = (0..4096usize)
            .map(|i| {
                if i < 32 {
                    0.0
                } else {
                    1.0 + ((i % 11) as f64) + ((i % 7) as f64 * 0.5)
                }
            })
            .collect();
        let measure_seconds: Vec<f32> = (0..measure_nps.len()).map(|i| i as f32).collect();
        let width = 32.0;
        let mesh = build_density_histogram_mesh(
            &measure_nps,
            16.0,
            &measure_seconds,
            0.0,
            measure_nps.len() as f32,
            width,
            24.0,
            0.0,
            width,
            None,
            1.0,
        );

        // One segment per horizontal pixel bucket, plus the trailing drop to zero.
        assert!(mesh.len() <= ((width as usize) + 1) * 6);
    }

    #[test]
    fn update_density_life_mesh_adds_caps_for_polyline_joins() {
        let mut mesh = None;
        let points = [[0.0, 8.0], [12.0, 8.0], [24.0, 20.0]];

        update_density_life_mesh(&mut mesh, &points, 0.0, 32.0, 2.0, [1.0, 1.0, 1.0, 1.0]);

        let mesh = mesh.expect("life mesh");
        assert_eq!(
            mesh.len(),
            2 * DENSITY_LIFE_SEGMENT_VERTS + 3 * DENSITY_LIFE_CAP_VERTS
        );
        let first_center_count = mesh.iter().filter(|v| v.pos == [0.0, 8.0]).count();
        let mid_center_count = mesh.iter().filter(|v| v.pos == [12.0, 8.0]).count();
        let last_center_count = mesh.iter().filter(|v| v.pos == [24.0, 20.0]).count();
        assert_eq!(first_center_count, DENSITY_LIFE_CAP_SUBDIVISIONS);
        assert_eq!(mid_center_count, DENSITY_LIFE_CAP_SUBDIVISIONS);
        assert_eq!(last_center_count, DENSITY_LIFE_CAP_SUBDIVISIONS);
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
}
