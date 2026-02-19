use std::sync::Arc;

use crate::act;
use crate::core::gfx::{BlendMode, MeshMode, MeshVertex};
use crate::game::profile;
use crate::screens::evaluation::ScoreInfo;
use crate::ui::actors::{Actor, SizeSpec};
use crate::ui::color;

use super::utils::pane_origin_x;

/// Builds the timing statistics pane (Simply Love Pane5), shown inside a 300px evaluation pane.
pub fn build_timing_pane(
    score_info: &ScoreInfo,
    timing_hist_mesh: Option<&Arc<[MeshVertex]>>,
    controller: profile::PlayerSide,
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
    let judgment_labels = [("Fan", 0), ("Ex", 1), ("Gr", 2), ("Dec", 3), ("WO", 4)];
    let timing_windows: [f32; 5] = crate::game::timing::effective_windows_ms(); // ms, with +1.5ms
    let worst_window = timing_windows[timing_windows.len() - 1];

    for (i, (label, grade_idx)) in judgment_labels.iter().enumerate() {
        let color = color::JUDGMENT_RGBA[*grade_idx];
        let window_ms = if i > 0 { timing_windows[i - 1] } else { 0.0 };
        let next_window_ms = timing_windows[i];
        let mid_point_ms = f32::midpoint(window_ms, next_window_ms);

        // Scale position from ms to pane coordinates
        let x_offset = (mid_point_ms / worst_window) * (pane_width / 2.0_f32);

        if i == 0 {
            // "Fan" is centered
            children.push(act!(text: font("miso"): settext(*label):
                align(0.5, 0.5): xy(pane_width / 2.0_f32, bottom_bar_center_y):
                zoom(0.65): diffuse(color[0], color[1], color[2], color[3])
            ));
        } else {
            // Others are symmetric
            children.push(act!(text: font("miso"): settext(*label):
                align(0.5, 0.5): xy(pane_width / 2.0_f32 - x_offset, bottom_bar_center_y):
                zoom(0.65): diffuse(color[0], color[1], color[2], color[3])
            ));
            children.push(act!(text: font("miso"): settext(*label):
                align(0.5, 0.5): xy(pane_width / 2.0_f32 + x_offset, bottom_bar_center_y):
                zoom(0.65): diffuse(color[0], color[1], color[2], color[3])
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
