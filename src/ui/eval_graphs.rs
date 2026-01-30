use crate::core::gfx::MeshVertex;
use crate::game::timing::{HistogramMs, ScatterPoint};
use crate::ui::color;

#[inline(always)]
fn color_for_abs_ms(abs_ms: f32, timing_windows_ms: [f32; 5]) -> [f32; 4] {
    if abs_ms <= timing_windows_ms[0] {
        color::JUDGMENT_RGBA[0]
    } else if abs_ms <= timing_windows_ms[1] {
        color::JUDGMENT_RGBA[1]
    } else if abs_ms <= timing_windows_ms[2] {
        color::JUDGMENT_RGBA[2]
    } else if abs_ms <= timing_windows_ms[3] {
        color::JUDGMENT_RGBA[3]
    } else {
        color::JUDGMENT_RGBA[4]
    }
}

#[inline(always)]
fn push_quad(out: &mut Vec<MeshVertex>, x: f32, y: f32, w: f32, h: f32, color: [f32; 4]) {
    let x1 = x + w;
    let y1 = y + h;
    out.push(MeshVertex { pos: [x, y], color });
    out.push(MeshVertex {
        pos: [x1, y],
        color,
    });
    out.push(MeshVertex {
        pos: [x1, y1],
        color,
    });
    out.push(MeshVertex { pos: [x, y], color });
    out.push(MeshVertex {
        pos: [x1, y1],
        color,
    });
    out.push(MeshVertex {
        pos: [x, y1],
        color,
    });
}

pub fn build_scatter_mesh(
    scatter: &[ScatterPoint],
    first_second: f32,
    last_second: f32,
    graph_width: f32,
    graph_height: f32,
    worst_window_ms: f32,
) -> Vec<MeshVertex> {
    let w = graph_width.max(0.0);
    let h = graph_height.max(0.0);
    if scatter.is_empty() || w <= 0.0 || h <= 0.0 {
        return Vec::new();
    }

    let denom = ((last_second + 0.05) - first_second).max(0.001);
    let worst = worst_window_ms.max(1.0);
    let timing_windows_ms = crate::game::timing::effective_windows_ms();

    let mut out: Vec<MeshVertex> = Vec::with_capacity(scatter.len().saturating_mul(6));

    for sp in scatter {
        let x_time = match sp.offset_ms {
            Some(off_ms) => sp.time_sec - (off_ms / 1000.0),
            None => sp.time_sec - (worst / 1000.0),
        };
        let x = ((x_time - first_second) / denom).clamp(0.0, 1.0) * w;

        match sp.offset_ms {
            Some(off_ms) => {
                let t = ((worst - off_ms) / (2.0 * worst)).clamp(0.0, 1.0);
                let y = t * h;
                let base = color_for_abs_ms(off_ms.abs(), timing_windows_ms);
                let c = [base[0], base[1], base[2], 0.666];
                push_quad(&mut out, x, y, 1.5, 1.5, c);
            }
            None => {
                let c = [1.0, 0.0, 0.0, 0.47];
                push_quad(&mut out, x, 0.0, 1.0, h, c);
            }
        }
    }

    out
}

pub fn build_offset_histogram_mesh(
    histogram: &HistogramMs,
    pane_width: f32,
    graph_height: f32,
    pane_height: f32,
    use_smoothing: bool,
) -> Vec<MeshVertex> {
    let pw = pane_width.max(0.0);
    let gh = graph_height.max(0.0);
    let ph = pane_height.max(0.0);
    if pw <= 0.0 || gh <= 0.0 || ph <= 0.0 {
        return Vec::new();
    }

    let worst_bin = (histogram.worst_window_ms / 1.0).round() as i32;
    if worst_bin <= 0 {
        return Vec::new();
    }
    let total_bins = (worst_bin * 2 + 1).max(1);
    let w = pw / (total_bins as f32);
    let peak = histogram.max_count.max(1) as f32;
    let worst_observed = (histogram.worst_observed_ms / 1.0).round() as i32;
    if worst_observed <= 0 {
        return Vec::new();
    }

    let timing_windows_ms = crate::game::timing::effective_windows_ms();
    let height_max = ph * 0.75;

    let mut raw: Vec<u32> = Vec::new();
    if !use_smoothing {
        raw.resize(total_bins as usize, 0);
        for &(bin, cnt) in &histogram.bins {
            let idx = bin + worst_bin;
            if idx >= 0 && idx < total_bins {
                raw[idx as usize] = cnt;
            }
        }
    }

    #[derive(Clone, Copy)]
    struct Col {
        x: f32,
        top_y: f32,
        color: [f32; 4],
    }

    let mut cols: Vec<Col> = Vec::with_capacity((worst_observed * 2 + 1).max(0) as usize);
    for bin in -worst_bin..=worst_bin {
        if bin.abs() > worst_observed {
            continue;
        }
        let i = (bin - (-worst_bin) + 1) as f32;
        let x = i * w;
        let y = if use_smoothing {
            let idx = (bin + worst_bin) as usize;
            histogram.smoothed.get(idx).map_or(0.0, |(_, v)| *v)
        } else {
            raw[(bin + worst_bin) as usize] as f32
        };
        let bar_h = (y / peak) * height_max;
        let top_y = (gh - bar_h).max(0.0);
        let c = color_for_abs_ms(bin.abs() as f32, timing_windows_ms);
        cols.push(Col { x, top_y, color: c });
    }

    if cols.len() < 2 {
        return Vec::new();
    }

    let bottom_y = gh;
    let mut out: Vec<MeshVertex> = Vec::with_capacity((cols.len() - 1) * 6);
    for w in cols.windows(2) {
        let a = w[0];
        let b = w[1];

        out.push(MeshVertex {
            pos: [a.x, bottom_y],
            color: a.color,
        });
        out.push(MeshVertex {
            pos: [a.x, a.top_y],
            color: a.color,
        });
        out.push(MeshVertex {
            pos: [b.x, bottom_y],
            color: b.color,
        });

        out.push(MeshVertex {
            pos: [a.x, a.top_y],
            color: a.color,
        });
        out.push(MeshVertex {
            pos: [b.x, b.top_y],
            color: b.color,
        });
        out.push(MeshVertex {
            pos: [b.x, bottom_y],
            color: b.color,
        });
    }

    out
}
