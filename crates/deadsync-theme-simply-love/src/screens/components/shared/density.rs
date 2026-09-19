//! Simply Love measure-density histograms and reusable mesh updates.

use deadlib_present::color::{desaturate_rgb, lerp, lerp_color};
use deadlib_render_core::MeshVertex;
use std::sync::Arc;

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

/// Immutable per-chart histogram columns, built when a graph is loaded or rescaled.
///
/// The owning screen reads this on the game thread; there is no interior mutation.
/// Storage is capped by the source measure count plus one closing column and is
/// released with the graph at transitions. There are no misses, insertions or
/// evictions during drawing. A window update does two binary searches and writes
/// six vertices per visible segment. Mesh owners retain their reusable buffers;
/// shared previous-frame buffers require replacement. No counters are maintained.
#[derive(Debug)]
pub struct DensityHistCache {
    cols: Arc<[HistCol]>,
    bottom_color: [f32; 4],
    height: f32,
    scaled_width: f32,
}

// Mesh construction only reads columns. One-shot callers can borrow their
// temporary columns; long-lived caches keep their shared ownership unchanged.
#[derive(Clone, Copy)]
struct HistView<'a> {
    cols: &'a [HistCol],
    bottom_color: [f32; 4],
    height: f32,
    scaled_width: f32,
}

#[inline]
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
    let mut cols = Vec::new();
    let bottom_color = build_hist_cols_into(
        &mut cols,
        measure_nps,
        peak_nps,
        measure_seconds,
        first_second,
        last_second,
        width,
        height,
        desaturation,
        alpha,
    );
    (cols, bottom_color)
}

#[inline]
fn build_hist_cols_into(
    cols: &mut Vec<HistCol>,
    measure_nps: &[f64],
    peak_nps: f64,
    measure_seconds: &[f32],
    first_second: f32,
    last_second: f32,
    width: f32,
    height: f32,
    desaturation: Option<f32>,
    alpha: f32,
) -> [f32; 4] {
    cols.clear();
    let (blue, purple) = sl_hist_colors(desaturation, alpha);
    let denom_t = last_second - first_second;
    if width <= 0.0 || height <= 0.0 || !denom_t.is_finite() || denom_t <= 0.0 {
        return blue;
    }
    let peak = (peak_nps as f32).max(0.000_001);
    if measure_nps.len() <= 1 || !peak.is_finite() {
        return blue;
    }

    cols.reserve_exact(measure_nps.len().saturating_add(1));
    let mut first_step_has_occurred = false;
    // The first sampled column must have positive density, so a NaN key
    // cannot hit before a height has been computed.
    let mut previous_nps_bits = f32::NAN.to_bits();
    let mut previous_bar_h = f32::NAN;

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
        // Repeated density values share their rounded height. Compare bits so
        // signed zero and nonfinite input keep the same arithmetic inputs.
        if previous_nps_bits != nps.to_bits() {
            previous_bar_h = ((nps / peak) * height).round();
            previous_nps_bits = nps.to_bits();
        }
        let bar_h = previous_bar_h;
        let top_y = height - bar_h;
        if cols.len() >= 2 {
            let a = cols[cols.len() - 1];
            let b = cols[cols.len() - 2];
            if a.top_y == top_y && b.top_y == top_y {
                let last_ix = cols.len() - 1;
                cols[last_ix].x = x;
                continue;
            }
        }

        // A collapsed plateau keeps the preceding column's color. Compute a
        // new color only for columns that will actually be stored.
        let frac = (bar_h / height).abs();
        let top_color = lerp_color(frac, blue, purple);
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

    blue
}

/// Caller-owned column storage for assembling several histograms into one mesh.
///
/// Each append replaces the columns while retaining capacity for the largest
/// source seen. Course graph construction owns this scratch locally and drops
/// it when the combined mesh is complete; no chart data or cache is shared.
#[derive(Debug, Default)]
pub struct DensityHistScratch {
    cols: Vec<HistCol>,
}

impl DensityHistScratch {
    /// Append a full histogram translated by `x`, preserving existing vertices.
    /// Invalid or empty histograms append nothing. Both column and destination
    /// buffers reuse capacity; growing either buffer may allocate.
    pub fn append_mesh(
        &mut self,
        out: &mut Vec<MeshVertex>,
        measure_nps: &[f64],
        peak_nps: f64,
        measure_seconds: &[f32],
        first_second: f32,
        last_second: f32,
        scaled_width: f32,
        height: f32,
        x: f32,
        desaturation: Option<f32>,
        alpha: f32,
    ) {
        let scaled_width = scaled_width.max(0.0);
        let height = height.max(0.0);
        if measure_nps.len() <= 1 || scaled_width <= 0.0 || height <= 0.0 {
            self.cols.clear();
            return;
        }
        let bottom_color = build_hist_cols_into(
            &mut self.cols,
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
        if self.cols.len() < 2 {
            return;
        }
        let view = HistView {
            cols: &self.cols,
            bottom_color,
            height,
            scaled_width,
        };
        let Some(window) = view.visible_window(0.0, scaled_width) else {
            return;
        };
        out.reserve((window.point_count - 1) * 6);
        let mut prev = None;
        view.visit_window_points(window, |point| {
            if let Some(last) = prev {
                let mut segment =
                    hist_segment_vertices(last, point, window.left, height, bottom_color);
                // Keep the original subtraction followed by translation, and
                // apply it before writing directly to the destination buffer.
                for vertex in &mut segment {
                    vertex.pos[0] += x;
                }
                out.extend_from_slice(&segment);
            }
            prev = Some(point);
        });
    }
}

#[must_use]
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
    if measure_nps.len() <= 1 {
        return None;
    }
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
    out.extend_from_slice(&hist_segment_vertices(a, b, left, bottom_y, bottom_color));
}

#[inline(always)]
fn hist_segment_vertices(
    a: HistCol,
    b: HistCol,
    left: f32,
    bottom_y: f32,
    bottom_color: [f32; 4],
) -> [MeshVertex; 6] {
    let ax = a.x - left;
    let bx = b.x - left;

    [
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
    ]
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
    let verts = hist_segment_vertices(a, b, left, bottom_y, bottom_color);
    dst[written..written + verts.len()].copy_from_slice(&verts);
    written + verts.len()
}

impl DensityHistCache {
    fn view(&self) -> HistView<'_> {
        HistView {
            cols: &self.cols,
            bottom_color: self.bottom_color,
            height: self.height,
            scaled_width: self.scaled_width,
        }
    }

    #[must_use]
    pub fn mesh(&self, offset: f32, visible_width: f32) -> Vec<MeshVertex> {
        self.view().mesh(offset, visible_width)
    }
}

impl HistView<'_> {
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

    #[must_use]
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
    let cache = cache.view();
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

/// # Panics
///
/// Panics if an internal state invariant is violated.
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
    let cache = cache.view();
    let Some(window) = cache.visible_window(offset, visible_width) else {
        *mesh = None;
        return;
    };

    let len = (window.point_count - 1) * 6;
    let vertices = match mesh.as_mut().and_then(Arc::get_mut) {
        Some(vertices) => vertices,
        None => Arc::get_mut(mesh.insert(Arc::new(Vec::with_capacity(len))))
            .expect("mesh has no other owners"),
    };
    if vertices.len() >= len {
        let written = cache.fill_mesh_vertices(vertices, window);
        vertices.truncate(written);
        debug_assert_eq!(written, len);
        return;
    }
    vertices.clear();
    vertices.reserve(len);
    let mut prev = None;
    let mut written = 0;
    // Rebuild growing meshes in the retained allocation without zero-filling
    // vertices that would immediately be overwritten.
    cache.visit_window_points(window, |point| {
        if let Some(last) = prev {
            push_hist_segment(
                vertices,
                last,
                point,
                window.left,
                cache.height,
                cache.bottom_color,
            );
            written += 6;
        }
        prev = Some(point);
    });
    debug_assert_eq!(written, len);
}

#[must_use]
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
    if measure_nps.len() <= 1 {
        return Vec::new();
    }
    let scaled_width = scaled_width.max(0.0);
    let height = height.max(0.0);
    let visible_width = visible_width.max(0.0);
    if scaled_width <= 0.0 || height <= 0.0 || visible_width <= 0.0 {
        return Vec::new();
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
        return Vec::new();
    }
    // Release capacity discarded by plateau compression before allocating the
    // mesh, while avoiding the cache's additional Arc allocation and copy.
    let cols = cols.into_boxed_slice();
    HistView {
        cols: &cols,
        bottom_color,
        height,
        scaled_width,
    }
    .mesh(offset, visible_width)
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
}
