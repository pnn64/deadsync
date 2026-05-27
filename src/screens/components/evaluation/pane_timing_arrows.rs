use crate::act;
use crate::assets::{FontRole, current_machine_font_key_for_text};
use crate::engine::present::actors::{Actor, SizeSpec};
use crate::engine::present::color;
use crate::game::profile;
use crate::game::timing::ArrowTimingBucket;
use crate::screens::evaluation::ScoreInfo;

use super::pane_column::build_pane3_arrow_preview;
use super::utils::pane_origin_x;

const LEFT_FOOT_RGBA: [f32; 4] = color::rgba_hex("#FF3030");
const RIGHT_FOOT_RGBA: [f32; 4] = color::rgba_hex("#3070FF");
const LABEL_RGBA: [f32; 4] = color::rgba_hex("#A0A0A0");
const VALUE_RGBA: [f32; 4] = color::rgba_hex("#FFFFFF");

#[inline(always)]
fn fmt_int(v: u32) -> String {
    v.to_string()
}

#[inline(always)]
fn fmt_ms(v: f32) -> String {
    format!("{:.2}", v)
}

/// Builds the per-arrow timing pane: a small table that breaks down
/// `# Steps`, `Mean Abs`, `Mean`, `Stddev*3`, and `Max` for each of the four
/// arrow directions plus the player's left and right foot.
pub fn build_timing_arrows_pane(
    score_info: &ScoreInfo,
    controller: profile::PlayerSide,
    preview_elapsed: f32,
) -> Vec<Actor> {
    let arrows = &score_info.arrow_timing;
    // Singles-only: render nothing if the data isn't a 4-column chart.
    if arrows.per_column.len() != 4 {
        return Vec::new();
    }

    let pane_width: f32 = 300.0;
    let pane_height: f32 = 180.0;

    let pane_origin_x = pane_origin_x(controller);
    let frame_x = pane_origin_x - pane_width * 0.5;
    let frame_y = crate::engine::space::screen_center_y() - 56.0;

    let mut children = Vec::new();

    // Layout: 6 data columns + a row-label gutter.
    let label_col_width: f32 = 64.0;
    let data_area_left: f32 = label_col_width;
    let data_area_right: f32 = pane_width - 6.0;
    let data_area_width: f32 = data_area_right - data_area_left;
    let col_step: f32 = data_area_width / 6.0;
    let col_centers: [f32; 6] = [
        data_area_left + col_step * 0.5,
        data_area_left + col_step * 1.5,
        data_area_left + col_step * 2.5,
        data_area_left + col_step * 3.5,
        data_area_left + col_step * 4.5,
        data_area_left + col_step * 5.5,
    ];

    let header_y: f32 = 24.0;
    let row_start_y: f32 = 52.0;
    let row_step: f32 = 24.0;

    // Column headers: noteskin arrow previews for ←/↓/↑/→ (20% larger
    // than the column-judgments pane to give the table room to breathe).
    if let Some(ns) = score_info.noteskin.as_ref() {
        for col_idx in 0..4 {
            children.extend(build_pane3_arrow_preview(
                ns,
                col_idx,
                [col_centers[col_idx], header_y],
                None,
                preview_elapsed,
                1.2,
            ));
        }
    }

    // L/R column headers.
    let foot_labels: [(&str, [f32; 4]); 2] = [("L", LEFT_FOOT_RGBA), ("R", RIGHT_FOOT_RGBA)];
    for (i, (label, color_rgba)) in foot_labels.iter().enumerate() {
        let foot_header_font = current_machine_font_key_for_text(FontRole::Header, label);
        children.push(
            act!(text: font(foot_header_font): settext(label.to_string()):
                align(0.5, 0.5): xy(col_centers[4 + i], header_y):
                zoom(0.55):
                diffuse(color_rgba[0], color_rgba[1], color_rgba[2], color_rgba[3])
            ),
        );
    }

    let buckets: [&ArrowTimingBucket; 6] = [
        &arrows.per_column[0],
        &arrows.per_column[1],
        &arrows.per_column[2],
        &arrows.per_column[3],
        &arrows.left_foot,
        &arrows.right_foot,
    ];

    let row_labels: [&str; 5] = ["# Steps", "Mean Abs", "Mean", "Stddev*3", "Max"];
    let cell_value = |bucket: &ArrowTimingBucket, row_idx: usize| -> String {
        if bucket.count == 0 {
            return String::from("-");
        }
        match row_idx {
            0 => fmt_int(bucket.count),
            1 => fmt_ms(bucket.stats.mean_abs_ms),
            2 => fmt_ms(bucket.stats.mean_ms),
            3 => fmt_ms(bucket.stats.stddev_ms * 3.0),
            4 => fmt_ms(bucket.stats.max_abs_ms),
            _ => String::new(),
        }
    };

    for (row_idx, label) in row_labels.iter().enumerate() {
        let y = row_start_y + (row_idx as f32) * row_step;

        // Row label.
        children.push(act!(text: font("miso"): settext(label.to_string()):
            align(1.0, 0.5): xy(label_col_width - 6.0, y):
            zoom(0.65):
            horizalign(right):
            diffuse(LABEL_RGBA[0], LABEL_RGBA[1], LABEL_RGBA[2], LABEL_RGBA[3])
        ));

        // `# Steps` is 50% larger than the timing-stat rows.
        let value_zoom = if row_idx == 0 { 1.05 } else { 0.7 };

        for (col_idx, bucket) in buckets.iter().enumerate() {
            let color = match col_idx {
                4 => LEFT_FOOT_RGBA,
                5 => RIGHT_FOOT_RGBA,
                _ => VALUE_RGBA,
            };
            let value = cell_value(bucket, row_idx);
            children.push(act!(text: font("miso"): settext(value):
                align(0.5, 0.5): xy(col_centers[col_idx], y):
                zoom(value_zoom):
                diffuse(color[0], color[1], color[2], color[3])
            ));
        }
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
