use crate::core::gfx::MeshVertex;
use crate::game::timing::TimingData;
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

pub struct DensityHistCache {
    cols: Arc<[HistCol]>,
    bottom_color: [f32; 4],
    height: f32,
    scaled_width: f32,
}

fn build_hist_cols(
    measure_nps: &[f64],
    peak_nps: f64,
    timing: &TimingData,
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

        let t = timing.get_time_for_beat(i as f32 * 4.0);
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
    timing: &TimingData,
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
        timing,
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

fn clip_hist_cols(cols: &[HistCol], left: f32, right: f32) -> Vec<HistCol> {
    if cols.is_empty() || left >= right {
        return Vec::new();
    }
    if left <= cols[0].x && right >= cols[cols.len() - 1].x {
        return cols.to_vec();
    }

    let mut li = 0usize;
    while li < cols.len() && cols[li].x < left {
        li += 1;
    }
    if li >= cols.len() {
        return Vec::new();
    }

    let mut ri = li;
    while ri < cols.len() && cols[ri].x <= right {
        ri += 1;
    }

    let mut out: Vec<HistCol> = Vec::with_capacity(ri.saturating_sub(li) + 2);
    out.extend_from_slice(&cols[li..ri]);

    if li > 0 {
        out.insert(0, interp_hist_col(cols[li - 1], cols[li], left));
    }
    if ri < cols.len() && ri > 0 {
        out.push(interp_hist_col(cols[ri - 1], cols[ri], right));
    }

    out
}

fn hist_cols_to_tris(cols: &[HistCol], bottom_y: f32, bottom_color: [f32; 4]) -> Vec<MeshVertex> {
    if cols.len() < 2 {
        return Vec::new();
    }
    let mut out: Vec<MeshVertex> = Vec::with_capacity(cols.len().saturating_sub(1) * 6);
    for w in cols.windows(2) {
        let a = w[0];
        let b = w[1];

        out.push(MeshVertex {
            pos: [a.x, bottom_y],
            color: bottom_color,
        });
        out.push(MeshVertex {
            pos: [a.x, a.top_y],
            color: a.top_color,
        });
        out.push(MeshVertex {
            pos: [b.x, bottom_y],
            color: bottom_color,
        });

        out.push(MeshVertex {
            pos: [a.x, a.top_y],
            color: a.top_color,
        });
        out.push(MeshVertex {
            pos: [b.x, b.top_y],
            color: b.top_color,
        });
        out.push(MeshVertex {
            pos: [b.x, bottom_y],
            color: bottom_color,
        });
    }
    out
}

impl DensityHistCache {
    pub fn mesh(&self, offset: f32, visible_width: f32) -> Vec<MeshVertex> {
        let visible_width = visible_width.max(0.0);
        if visible_width <= 0.0 || self.scaled_width <= 0.0 || self.height <= 0.0 {
            return Vec::new();
        }

        let left = offset.clamp(0.0, self.scaled_width);
        let right = (left + visible_width).clamp(0.0, self.scaled_width);
        let mut clipped = clip_hist_cols(&self.cols, left, right);
        for c in &mut clipped {
            c.x -= left;
        }
        hist_cols_to_tris(&clipped, self.height, self.bottom_color)
    }
}

pub fn build_density_histogram_mesh(
    measure_nps: &[f64],
    peak_nps: f64,
    timing: &TimingData,
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
        timing,
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
