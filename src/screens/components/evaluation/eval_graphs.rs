use crate::core::gfx::MeshVertex;
use crate::game::timing::{HistogramMs, ScatterPoint};
use crate::ui::color;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TimingHistogramScale {
    Itg,
    Ex,
    HardEx,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScatterPlotScale {
    Itg,
    Ex,
    HardEx,
    Arrow,
    Foot,
}

const HIST_BIN_MS: f32 = 1.0;
const GAUSS7: [f32; 7] = [0.045, 0.090, 0.180, 0.370, 0.180, 0.090, 0.045];

#[inline(always)]
fn hard_ex_display_window_ms(worst_window_ms: f32) -> f32 {
    worst_window_ms.min(crate::game::timing::effective_windows_ms()[1])
}

#[inline(always)]
fn scatter_display_window_ms(worst_window_ms: f32, scale: ScatterPlotScale) -> f32 {
    let display = match scale {
        ScatterPlotScale::HardEx => hard_ex_display_window_ms(worst_window_ms),
        ScatterPlotScale::Itg
        | ScatterPlotScale::Ex
        | ScatterPlotScale::Arrow
        | ScatterPlotScale::Foot => worst_window_ms,
    };
    display.max(1.0)
}

#[inline(always)]
pub(crate) fn timing_display_window_ms(worst_window_ms: f32, scale: TimingHistogramScale) -> f32 {
    let display = match scale {
        TimingHistogramScale::HardEx => hard_ex_display_window_ms(worst_window_ms),
        TimingHistogramScale::Itg | TimingHistogramScale::Ex => worst_window_ms,
    };
    display.max(1.0)
}

#[inline(always)]
fn color_for_abs_ms(
    abs_ms: f32,
    timing_windows_ms: [f32; 5],
    scale: TimingHistogramScale,
) -> [f32; 4] {
    let w1 = timing_windows_ms[0];
    let w2 = timing_windows_ms[1];
    let w3 = timing_windows_ms[2];
    let w4 = timing_windows_ms[3];
    let w0 = crate::game::timing::FA_PLUS_W0_MS;
    let w010 = crate::game::timing::FA_PLUS_W010_MS;

    match scale {
        TimingHistogramScale::Itg => {
            if abs_ms <= w1 {
                color::JUDGMENT_RGBA[0]
            } else if abs_ms <= w2 {
                color::JUDGMENT_RGBA[1]
            } else if abs_ms <= w3 {
                color::JUDGMENT_RGBA[2]
            } else if abs_ms <= w4 {
                color::JUDGMENT_RGBA[3]
            } else {
                color::JUDGMENT_RGBA[4]
            }
        }
        TimingHistogramScale::Ex => {
            if abs_ms <= w0 {
                color::JUDGMENT_RGBA[0]
            } else if abs_ms <= w1 {
                color::JUDGMENT_FA_PLUS_WHITE_RGBA
            } else if abs_ms <= w2 {
                color::JUDGMENT_RGBA[1]
            } else if abs_ms <= w3 {
                color::JUDGMENT_RGBA[2]
            } else if abs_ms <= w4 {
                color::JUDGMENT_RGBA[3]
            } else {
                color::JUDGMENT_RGBA[4]
            }
        }
        TimingHistogramScale::HardEx => {
            if abs_ms <= w010 {
                color::HARD_EX_SCORE_RGBA
            } else if abs_ms <= w0 {
                color::JUDGMENT_RGBA[0]
            } else if abs_ms <= w1 {
                color::JUDGMENT_FA_PLUS_WHITE_RGBA
            } else if abs_ms <= w2 {
                color::JUDGMENT_RGBA[1]
            } else if abs_ms <= w3 {
                color::JUDGMENT_RGBA[2]
            } else if abs_ms <= w4 {
                color::JUDGMENT_RGBA[3]
            } else {
                color::JUDGMENT_RGBA[4]
            }
        }
    }
}

#[inline(always)]
fn color_for_arrow(direction_code: u8) -> [f32; 4] {
    match direction_code {
        1 => [1.0, 0.0, 0.0, 1.0],
        2 => [0.0, 0.0, 1.0, 1.0],
        3 => [0.0, 1.0, 0.0, 1.0],
        4 => [1.0, 1.0, 0.0, 1.0],
        _ => [1.0, 1.0, 1.0, 1.0],
    }
}

#[inline(always)]
fn color_for_foot(is_stream: bool, is_left_foot: bool) -> [f32; 4] {
    if !is_stream {
        return [0.0, 0.0, 0.0, 1.0];
    }
    if is_left_foot {
        [1.0, 0.0, 0.0, 1.0]
    } else {
        [0.0, 0.0, 1.0, 1.0]
    }
}

#[inline(always)]
fn color_for_scatter(
    sp: &ScatterPoint,
    abs_ms: f32,
    timing_windows_ms: [f32; 5],
    scale: ScatterPlotScale,
) -> [f32; 4] {
    match scale {
        ScatterPlotScale::Itg => {
            color_for_abs_ms(abs_ms, timing_windows_ms, TimingHistogramScale::Itg)
        }
        ScatterPlotScale::Ex => {
            color_for_abs_ms(abs_ms, timing_windows_ms, TimingHistogramScale::Ex)
        }
        ScatterPlotScale::HardEx => {
            color_for_abs_ms(abs_ms, timing_windows_ms, TimingHistogramScale::HardEx)
        }
        ScatterPlotScale::Arrow => color_for_arrow(sp.direction_code),
        ScatterPlotScale::Foot => color_for_foot(sp.is_stream, sp.is_left_foot),
    }
}

#[inline(always)]
fn miss_color_for_scatter(sp: &ScatterPoint, scale: ScatterPlotScale) -> [f32; 4] {
    match scale {
        ScatterPlotScale::Itg | ScatterPlotScale::Ex | ScatterPlotScale::HardEx => {
            [1.0, 0.0, 0.0, 1.0]
        }
        ScatterPlotScale::Arrow => color_for_arrow(sp.direction_code),
        ScatterPlotScale::Foot => color_for_foot(sp.is_stream, sp.is_left_foot),
    }
}

#[inline(always)]
fn hist_bin_abs_ms(bin: i32) -> f32 {
    if bin < 0 {
        bin.unsigned_abs() as f32 - 0.5
    } else if bin > 0 {
        bin as f32 + 0.5
    } else {
        0.0
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
    scale: ScatterPlotScale,
) -> Vec<MeshVertex> {
    let w = graph_width.max(0.0);
    let h = graph_height.max(0.0);
    if scatter.is_empty() || w <= 0.0 || h <= 0.0 {
        return Vec::new();
    }

    let denom = ((last_second + 0.05) - first_second).max(0.001);
    let worst = scatter_display_window_ms(worst_window_ms, scale);
    let timing_windows_ms = crate::game::timing::effective_windows_ms();
    const POINT_W: f32 = 1.5;
    const POINT_H: f32 = 1.5;
    const MISS_W: f32 = 1.0;

    let mut out: Vec<MeshVertex> = Vec::with_capacity(scatter.len().saturating_mul(6));

    for sp in scatter {
        if let Some(off_ms) = sp.offset_ms
            && off_ms.abs() > worst
        {
            continue;
        }

        let x_time = match sp.offset_ms {
            Some(off_ms) => sp.time_sec - (off_ms / 1000.0),
            None => sp.time_sec - (worst / 1000.0),
        };
        let x = ((x_time - first_second) / denom).clamp(0.0, 1.0) * w;

        match sp.offset_ms {
            Some(off_ms) => {
                let t = ((worst - off_ms) / (2.0 * worst)).clamp(0.0, 1.0);
                let x = x.clamp(0.0, (w - POINT_W).max(0.0));
                let y = (t * (h - POINT_H).max(0.0)).clamp(0.0, (h - POINT_H).max(0.0));
                let base = color_for_scatter(sp, off_ms.abs(), timing_windows_ms, scale);
                let c = [base[0], base[1], base[2], 0.666];
                push_quad(&mut out, x, y, POINT_W, POINT_H, c);
            }
            None => {
                let x = x.clamp(0.0, (w - MISS_W).max(0.0));
                let base = miss_color_for_scatter(sp, scale);
                let miss_alpha = if matches!(
                    scale,
                    ScatterPlotScale::Itg | ScatterPlotScale::Ex | ScatterPlotScale::HardEx
                ) {
                    0.3
                } else {
                    0.333
                };
                let c = [base[0], base[1], base[2], miss_alpha];
                let h1 = if sp.miss_because_held { h * 0.5 } else { 0.0 };
                let h2 = if sp.miss_because_held { h } else { h * 0.5 };
                push_quad(&mut out, x, h1, MISS_W, (h2 - h1).max(0.0), c);
            }
        }
    }

    out
}

#[inline(always)]
fn hist_dense_counts(histogram: &HistogramMs, worst_bin: i32) -> (Vec<u32>, u32, i32) {
    let total_bins = (worst_bin * 2 + 1).max(1) as usize;
    let mut raw = vec![0; total_bins];
    let mut peak = 0u32;
    let mut worst_observed = 0i32;

    for &(bin, count) in &histogram.bins {
        if bin < -worst_bin || bin > worst_bin {
            continue;
        }
        raw[(bin + worst_bin) as usize] = count;
        peak = peak.max(count);
        worst_observed = worst_observed.max(bin.abs());
    }

    (raw, peak, worst_observed)
}

#[inline(always)]
fn smooth_hist_bin(raw: &[u32], worst_bin: i32, bin: i32) -> f32 {
    let mut y = 0.0_f32;
    for (offset, weight) in (-3..=3).zip(GAUSS7) {
        let sample = (bin + offset).clamp(-worst_bin, worst_bin);
        y += raw[(sample + worst_bin) as usize] as f32 * weight;
    }
    y
}

pub fn build_offset_histogram_mesh(
    histogram: &HistogramMs,
    pane_width: f32,
    graph_height: f32,
    pane_height: f32,
    scale: TimingHistogramScale,
    use_smoothing: bool,
) -> Vec<MeshVertex> {
    let pw = pane_width.max(0.0);
    let gh = graph_height.max(0.0);
    let ph = pane_height.max(0.0);
    if pw <= 0.0 || gh <= 0.0 || ph <= 0.0 {
        return Vec::new();
    }

    let display_window_ms = timing_display_window_ms(histogram.worst_window_ms, scale);
    let worst_bin = (display_window_ms / HIST_BIN_MS).round() as i32;
    if worst_bin <= 0 {
        return Vec::new();
    }
    let total_bins = (worst_bin * 2 + 1).max(1);
    let w = pw / (total_bins as f32);
    let (raw, peak_raw, worst_observed) = hist_dense_counts(histogram, worst_bin);
    if worst_observed <= 0 {
        return Vec::new();
    }
    let peak = peak_raw.max(1) as f32;

    let timing_windows_ms = crate::game::timing::effective_windows_ms();
    let height_max = ph * 0.75;

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
            smooth_hist_bin(&raw, worst_bin, bin)
        } else {
            raw[(bin + worst_bin) as usize] as f32
        };
        let bar_h = (y / peak) * height_max;
        let top_y = (gh - bar_h).max(0.0);
        let c = color_for_abs_ms(hist_bin_abs_ms(bin), timing_windows_ms, scale);
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
        let color = a.color;

        out.push(MeshVertex {
            pos: [a.x, bottom_y],
            color,
        });
        out.push(MeshVertex {
            pos: [a.x, a.top_y],
            color,
        });
        out.push(MeshVertex {
            pos: [b.x, bottom_y],
            color,
        });

        out.push(MeshVertex {
            pos: [a.x, a.top_y],
            color,
        });
        out.push(MeshVertex {
            pos: [b.x, b.top_y],
            color,
        });
        out.push(MeshVertex {
            pos: [b.x, bottom_y],
            color,
        });
    }

    out
}
