use std::sync::Arc;

use crate::act;
use crate::core::gfx::{BlendMode, MeshMode, MeshVertex};
use crate::game::profile;
use crate::screens::components::eval_graphs::TimingHistogramScale;
use crate::screens::evaluation::ScoreInfo;
use crate::ui::actors::{Actor, SizeSpec};
use crate::ui::color;

use super::utils::pane_origin_x;

#[derive(Clone, Copy)]
struct TimingBand {
    label: &'static str,
    start_ms: f32,
    end_ms: f32,
    color: [f32; 4],
}

const EMPTY_BAND: TimingBand = TimingBand {
    label: "",
    start_ms: 0.0,
    end_ms: 0.0,
    color: [0.0, 0.0, 0.0, 0.0],
};

#[inline(always)]
const fn band(label: &'static str, start_ms: f32, end_ms: f32, color: [f32; 4]) -> TimingBand {
    TimingBand {
        label,
        start_ms,
        end_ms,
        color,
    }
}

#[inline(always)]
fn timing_bands_itg(timing_windows: [f32; 5]) -> ([TimingBand; 7], usize) {
    let blue = color::JUDGMENT_RGBA[0];
    let excellent = color::JUDGMENT_RGBA[1];
    let great = color::JUDGMENT_RGBA[2];
    let decent = color::JUDGMENT_RGBA[3];
    let wayoff = color::JUDGMENT_RGBA[4];
    let w1 = timing_windows[0];
    let w2 = timing_windows[1];
    let w3 = timing_windows[2];
    let w4 = timing_windows[3];
    let w5 = timing_windows[4];

    (
        [
            band("Fan", 0.0, w1, blue),
            band("Ex", w1, w2, excellent),
            band("Gr", w2, w3, great),
            band("Dec", w3, w4, decent),
            band("WO", w4, w5, wayoff),
            EMPTY_BAND,
            EMPTY_BAND,
        ],
        5,
    )
}

#[inline(always)]
fn timing_bands_ex(timing_windows: [f32; 5]) -> ([TimingBand; 7], usize) {
    let blue = color::JUDGMENT_RGBA[0];
    let excellent = color::JUDGMENT_RGBA[1];
    let great = color::JUDGMENT_RGBA[2];
    let decent = color::JUDGMENT_RGBA[3];
    let wayoff = color::JUDGMENT_RGBA[4];
    let white = color::JUDGMENT_FA_PLUS_WHITE_RGBA;
    let w0 = crate::game::timing::FA_PLUS_W0_MS;
    let w1 = timing_windows[0];
    let w2 = timing_windows[1];
    let w3 = timing_windows[2];
    let w4 = timing_windows[3];
    let w5 = timing_windows[4];

    (
        [
            band("Fan", 0.0, w0, blue),
            band("Fan", w0, w1, white),
            band("Ex", w1, w2, excellent),
            band("Gr", w2, w3, great),
            band("Dec", w3, w4, decent),
            band("WO", w4, w5, wayoff),
            EMPTY_BAND,
        ],
        6,
    )
}

#[inline(always)]
fn timing_bands_hard_ex(timing_windows: [f32; 5]) -> ([TimingBand; 7], usize) {
    let pink = color::HARD_EX_SCORE_RGBA;
    let blue = color::JUDGMENT_RGBA[0];
    let excellent = color::JUDGMENT_RGBA[1];
    let great = color::JUDGMENT_RGBA[2];
    let decent = color::JUDGMENT_RGBA[3];
    let wayoff = color::JUDGMENT_RGBA[4];
    let white = color::JUDGMENT_FA_PLUS_WHITE_RGBA;
    let w010 = crate::game::timing::FA_PLUS_W010_MS;
    let w0 = crate::game::timing::FA_PLUS_W0_MS;
    let w1 = timing_windows[0];
    let w2 = timing_windows[1];
    let w3 = timing_windows[2];
    let w4 = timing_windows[3];
    let w5 = timing_windows[4];

    (
        [
            band("Fan", 0.0, w010, pink),
            band("Fan", w010, w0, blue),
            band("Fan", w0, w1, white),
            band("Ex", w1, w2, excellent),
            band("Gr", w2, w3, great),
            band("Dec", w3, w4, decent),
            band("WO", w4, w5, wayoff),
        ],
        7,
    )
}

#[inline(always)]
fn timing_bands_ms(
    scale: TimingHistogramScale,
    timing_windows: [f32; 5],
) -> ([TimingBand; 7], usize) {
    match scale {
        TimingHistogramScale::Itg => timing_bands_itg(timing_windows),
        TimingHistogramScale::Ex => timing_bands_ex(timing_windows),
        TimingHistogramScale::HardEx => timing_bands_hard_ex(timing_windows),
    }
}

/// Builds the timing statistics pane (Simply Love Pane5), shown inside a 300px evaluation pane.
pub fn build_timing_pane(
    score_info: &ScoreInfo,
    timing_hist_mesh: Option<&Arc<[MeshVertex]>>,
    controller: profile::PlayerSide,
    scale: TimingHistogramScale,
) -> Vec<Actor> {
    let pane_width: f32 = 300.0;
    let pane_height: f32 = 180.0;
    let topbar_height: f32 = 26.0;
    let bottombar_height: f32 = 13.0;

    let pane_origin_x = pane_origin_x(controller);
    let frame_x = pane_origin_x - pane_width * 0.5;
    let frame_y = crate::core::space::screen_center_y() - 56.0;

    let mut children = Vec::new();
    const BAR_BG_COLOR: [f32; 4] = color::rgba_hex("#101519");

    // Top and Bottom bars
    children.push(act!(quad:
        align(0.0, 0.0): xy(0.0, 0.0):
        setsize(pane_width, topbar_height):
        diffuse(BAR_BG_COLOR[0], BAR_BG_COLOR[1], BAR_BG_COLOR[2], 1.0)
    ));
    children.push(act!(quad:
        align(0.0, 1.0): xy(0.0, pane_height):
        setsize(pane_width, bottombar_height):
        diffuse(BAR_BG_COLOR[0], BAR_BG_COLOR[1], BAR_BG_COLOR[2], 1.0)
    ));

    // Center line of graph area
    children.push(act!(quad:
        align(0.5, 0.0): xy(pane_width / 2.0_f32, topbar_height):
        setsize(1.0, pane_height - topbar_height - bottombar_height):
        diffuse(1.0, 1.0, 1.0, 0.666)
    ));

    // Early/Late text
    let early_late_y = topbar_height + 11.0;
    children.push(act!(text: font("wendy"): settext("Early"):
        align(0.0, 0.0): xy(10.0, early_late_y):
        zoom(0.3):
    ));
    children.push(act!(text: font("wendy"): settext("Late"):
        align(1.0, 0.0): xy(pane_width - 10.0, early_late_y):
        zoom(0.3): horizalign(right)
    ));

    // Bottom bar judgment labels
    let bottom_bar_center_y = pane_height - (bottombar_height / 2.0_f32);
    let timing_windows: [f32; 5] = crate::game::timing::effective_windows_ms(); // ms, with +1.5ms
    let (judgment_bands, band_count) = timing_bands_ms(scale, timing_windows);
    let legend_span_ms = score_info.histogram.worst_window_ms.max(1.0);

    for (i, band) in judgment_bands.iter().take(band_count).enumerate() {
        if band.start_ms >= legend_span_ms {
            continue;
        }
        let clamped_end_ms = band.end_ms.min(legend_span_ms);
        if clamped_end_ms <= band.start_ms {
            continue;
        }
        let mid_point_ms = f32::midpoint(band.start_ms, clamped_end_ms);

        // Scale position from ms to pane coordinates
        let x_offset = (mid_point_ms / legend_span_ms) * (pane_width / 2.0_f32);

        if i == 0 {
            // "Fan" is centered
            children.push(act!(text: font("miso"): settext(band.label):
                align(0.5, 0.5): xy(pane_width / 2.0_f32, bottom_bar_center_y):
                zoom(0.65): diffuse(band.color[0], band.color[1], band.color[2], band.color[3])
            ));
        } else {
            // Others are symmetric
            children.push(act!(text: font("miso"): settext(band.label):
                align(0.5, 0.5): xy(pane_width / 2.0_f32 - x_offset, bottom_bar_center_y):
                zoom(0.65): diffuse(band.color[0], band.color[1], band.color[2], band.color[3])
            ));
            children.push(act!(text: font("miso"): settext(band.label):
                align(0.5, 0.5): xy(pane_width / 2.0_f32 + x_offset, bottom_bar_center_y):
                zoom(0.65): diffuse(band.color[0], band.color[1], band.color[2], band.color[3])
            ));
        }
    }

    // Histogram (aggregate timing offsets) — Simply Love uses an ActorMultiVertex (QuadStrip).
    if let Some(mesh) = timing_hist_mesh
        && !mesh.is_empty()
    {
        let graph_area_height = (pane_height - topbar_height - bottombar_height).max(0.0);
        children.push(Actor::Mesh {
            align: [0.0, 0.0],
            offset: [0.0, topbar_height],
            size: [SizeSpec::Px(pane_width), SizeSpec::Px(graph_area_height)],
            vertices: mesh.clone(),
            mode: MeshMode::Triangles,
            visible: true,
            blend: BlendMode::Alpha,
            z: 0,
        });
    }

    // Top bar stats
    let top_label_y = 2.0;
    let top_value_y = 13.0;
    let label_zoom = 0.575;
    let value_zoom = 0.8;

    let max_error_text = format!("{:.1}ms", score_info.timing.max_abs_ms);
    let mean_abs_text = format!("{:.1}ms", score_info.timing.mean_abs_ms);
    let mean_text = format!("{:.1}ms", score_info.timing.mean_ms);
    let stddev3_text = format!("{:.1}ms", score_info.timing.stddev_ms * 3.0);

    let labels_and_values = [
        ("mean abs error", 40.0, mean_abs_text),
        ("mean", 40.0 + (pane_width - 80.0_f32) / 3.0_f32, mean_text),
        (
            "std dev * 3",
            ((pane_width - 80.0_f32) / 3.0_f32).mul_add(2.0_f32, 40.0),
            stddev3_text,
        ),
        ("max error", pane_width - 40.0, max_error_text),
    ];

    for (label, x, value) in labels_and_values {
        children.push(act!(text: font("miso"): settext(label):
            align(0.5, 0.0): xy(x, top_label_y):
            zoom(label_zoom)
        ));
        children.push(act!(text: font("miso"): settext(value):
            align(0.5, 0.0): xy(x, top_value_y):
            zoom(value_zoom)
        ));
    }

    vec![Actor::Frame {
        align: [0.0, 0.0],
        offset: [frame_x, frame_y],
        size: [SizeSpec::Px(pane_width), SizeSpec::Px(pane_height)],
        children,
        background: None,
        z: 101,
    }]
}
