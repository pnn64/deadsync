use crate::engine::gfx::MeshVertex;
use crate::engine::present::color;
use crate::game::timing::{HistogramMs, ScatterPoint};

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

#[inline(always)]
fn hard_ex_display_window_ms(worst_window_ms: f32) -> f32 {
    worst_window_ms.min(crate::game::timing::effective_windows_ms()[1])
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
    let worst = match scale {
        ScatterPlotScale::HardEx => hard_ex_display_window_ms(worst_window_ms),
        _ => worst_window_ms,
    }
    .max(1.0);
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
fn hist_worst_observed_bin(histogram: &HistogramMs, worst_bin: i32) -> i32 {
    let mut worst_observed = 0;
    for &(bin, _) in &histogram.bins {
        if bin < -worst_bin {
            continue;
        }
        if bin > worst_bin {
            break;
        }
        worst_observed = worst_observed.max(bin.abs());
    }
    worst_observed
}

#[inline(always)]
fn hist_raw_y(bins: &[(i32, u32)], raw_ix: &mut usize, bin: i32) -> f32 {
    while *raw_ix < bins.len() && bins[*raw_ix].0 < bin {
        *raw_ix += 1;
    }
    if *raw_ix < bins.len() && bins[*raw_ix].0 == bin {
        bins[*raw_ix].1 as f32
    } else {
        0.0
    }
}

#[inline(always)]
fn push_hist_segment(
    out: &mut Vec<MeshVertex>,
    ax: f32,
    atop: f32,
    bx: f32,
    btop: f32,
    bottom_y: f32,
    color: [f32; 4],
) {
    out.push(MeshVertex {
        pos: [ax, bottom_y],
        color,
    });
    out.push(MeshVertex {
        pos: [ax, atop],
        color,
    });
    out.push(MeshVertex {
        pos: [bx, bottom_y],
        color,
    });
    out.push(MeshVertex {
        pos: [ax, atop],
        color,
    });
    out.push(MeshVertex {
        pos: [bx, btop],
        color,
    });
    out.push(MeshVertex {
        pos: [bx, bottom_y],
        color,
    });
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
    let worst_observed = hist_worst_observed_bin(histogram, worst_bin);
    if worst_observed <= 0 {
        return Vec::new();
    }
    let peak = histogram.max_count.max(1) as f32;

    let timing_windows_ms = crate::game::timing::effective_windows_ms();
    let height_max = ph * 0.75;
    let bottom_y = gh;
    let x_for = |bin: i32| (bin + worst_bin + 1) as f32 * w;
    let top_y_for = |y: f32| (gh - (y / peak) * height_max).max(0.0);
    let first_bin = -worst_observed;
    let mut out: Vec<MeshVertex> = Vec::with_capacity((worst_observed as usize).saturating_mul(12));

    if use_smoothing {
        let smoothed_zero = (histogram.smoothed.len() / 2) as i32;
        let mut prev_x = x_for(first_bin);
        let mut prev_top = top_y_for(histogram.smoothed[(first_bin + smoothed_zero) as usize].1);
        let mut prev_color = color_for_abs_ms(hist_bin_abs_ms(first_bin), timing_windows_ms, scale);

        for bin in (first_bin + 1)..=worst_observed {
            let x = x_for(bin);
            let top = top_y_for(histogram.smoothed[(bin + smoothed_zero) as usize].1);
            push_hist_segment(&mut out, prev_x, prev_top, x, top, bottom_y, prev_color);
            prev_x = x;
            prev_top = top;
            prev_color = color_for_abs_ms(hist_bin_abs_ms(bin), timing_windows_ms, scale);
        }
    } else {
        let mut raw_ix = 0usize;
        while raw_ix < histogram.bins.len() && histogram.bins[raw_ix].0 < first_bin {
            raw_ix += 1;
        }

        let mut prev_x = x_for(first_bin);
        let mut prev_top = top_y_for(hist_raw_y(&histogram.bins, &mut raw_ix, first_bin));
        let mut prev_color = color_for_abs_ms(hist_bin_abs_ms(first_bin), timing_windows_ms, scale);

        for bin in (first_bin + 1)..=worst_observed {
            let x = x_for(bin);
            let top = top_y_for(hist_raw_y(&histogram.bins, &mut raw_ix, bin));
            push_hist_segment(&mut out, prev_x, prev_top, x, top, bottom_y, prev_color);
            prev_x = x;
            prev_top = top;
            prev_color = color_for_abs_ms(hist_bin_abs_ms(bin), timing_windows_ms, scale);
        }
    }

    out
}
