use crate::act;
use crate::assets::{FontRole, machine_font_key_for_text};
use crate::config::MachineFont;
use crate::screens::evaluation::ScoreInfo;
use deadlib_present::actors::{Actor, SizeSpec, TextContent};
use deadlib_present::color;
use deadsync_profile as profile_data;
use deadsync_rules::timing::{ArrowTimingBucket, ArrowTimingStats};

use super::pane_column::build_pane3_arrow_preview;
use super::utils::pane_origin_x;

const LEFT_FOOT_RGBA: [f32; 4] = color::rgba_hex("#FF3030");
const RIGHT_FOOT_RGBA: [f32; 4] = color::rgba_hex("#3070FF");
const LABEL_RGBA: [f32; 4] = color::rgba_hex("#A0A0A0");
const VALUE_RGBA: [f32; 4] = color::rgba_hex("#FFFFFF");

#[derive(Clone)]
pub(crate) struct TimingArrowsText {
    cells: [[TextContent; 6]; 5],
}

impl TimingArrowsText {
    pub(crate) fn new(arrows: &ArrowTimingStats) -> Option<Self> {
        if arrows.per_column.len() != 4 {
            return None;
        }
        let buckets = [
            &arrows.per_column[0],
            &arrows.per_column[1],
            &arrows.per_column[2],
            &arrows.per_column[3],
            &arrows.left_foot,
            &arrows.right_foot,
        ];
        Some(Self {
            cells: std::array::from_fn(|row| {
                std::array::from_fn(|column| cell_text(buckets[column], row))
            }),
        })
    }
}

#[inline]
fn fmt_ms(value: f32) -> TextContent {
    super::retained_text(format_args!("{value:.2}"))
}

#[inline]
fn cell_text(bucket: &ArrowTimingBucket, row: usize) -> TextContent {
    if bucket.count == 0 {
        return TextContent::Static("-");
    }
    match row {
        0 => TextContent::inline_u32(bucket.count),
        1 => fmt_ms(bucket.stats.mean_abs_ms),
        2 => fmt_ms(bucket.stats.mean_ms),
        3 => fmt_ms(bucket.stats.stddev_ms * 3.0),
        4 => fmt_ms(bucket.stats.max_abs_ms),
        _ => TextContent::Static(""),
    }
}

/// Builds the per-arrow timing pane: a small table that breaks down
/// `# Steps`, `Mean Abs`, `Mean`, `Stddev*3`, and `Max` for each of the four
/// arrow directions plus the player's left and right foot.
pub(crate) fn build_timing_arrows_pane(
    score_info: &ScoreInfo,
    text: &TimingArrowsText,
    controller: profile_data::PlayerSide,
    preview_elapsed: f32,
    machine_font: MachineFont,
) -> Vec<Actor> {
    let pane_width: f32 = 300.0;
    let pane_height: f32 = 180.0;

    let pane_origin_x = pane_origin_x(controller);
    let frame_x = pane_width.mul_add(-0.5, pane_origin_x);
    let frame_y = deadlib_present::space::screen_center_y() - 56.0;

    let mut children = Vec::new();

    // Layout: 6 data columns + a row-label gutter.
    let label_col_width: f32 = 64.0;
    let data_area_left: f32 = label_col_width;
    let data_area_right: f32 = pane_width - 6.0;
    let data_area_width: f32 = data_area_right - data_area_left;
    let col_step: f32 = data_area_width / 6.0;
    let col_centers: [f32; 6] = [
        col_step.mul_add(0.5, data_area_left),
        col_step.mul_add(1.5, data_area_left),
        col_step.mul_add(2.5, data_area_left),
        col_step.mul_add(3.5, data_area_left),
        col_step.mul_add(4.5, data_area_left),
        col_step.mul_add(5.5, data_area_left),
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
    let foot_labels: [(&'static str, [f32; 4]); 2] =
        [("L", LEFT_FOOT_RGBA), ("R", RIGHT_FOOT_RGBA)];
    for (i, &(label, color_rgba)) in foot_labels.iter().enumerate() {
        let foot_header_font = machine_font_key_for_text(machine_font, FontRole::Header, label);
        children.push(act!(text: font(foot_header_font): settext(label):
            align(0.5, 0.5): xy(col_centers[4 + i], header_y):
            zoom(0.55):
            diffuse(color_rgba[0], color_rgba[1], color_rgba[2], color_rgba[3])
        ));
    }

    let row_labels: [&'static str; 5] = ["# Steps", "Mean Abs", "Mean", "Stddev*3", "Max"];
    for (row_idx, &label) in row_labels.iter().enumerate() {
        let y = (row_idx as f32).mul_add(row_step, row_start_y);

        // Row label.
        children.push(act!(text: font("miso"): settext(label):
            align(1.0, 0.5): xy(label_col_width - 6.0, y):
            zoom(0.65):
            horizalign(right):
            diffuse(LABEL_RGBA[0], LABEL_RGBA[1], LABEL_RGBA[2], LABEL_RGBA[3])
        ));

        // `# Steps` is 50% larger than the timing-stat rows.
        let value_zoom = if row_idx == 0 { 1.05 } else { 0.7 };

        for col_idx in 0..6 {
            let color = match col_idx {
                4 => LEFT_FOOT_RGBA,
                5 => RIGHT_FOOT_RGBA,
                _ => VALUE_RGBA,
            };
            let value = text.cells[row_idx][col_idx].clone();
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

#[cfg(test)]
mod tests {
    use super::*;
    use deadsync_rules::timing::TimingStats;

    fn arrows(bucket: ArrowTimingBucket) -> ArrowTimingStats {
        ArrowTimingStats {
            per_column: vec![bucket; 4],
            left_foot: bucket,
            right_foot: bucket,
        }
    }

    #[test]
    fn timing_arrow_text_compiles_normal_cells_inline() {
        let text = TimingArrowsText::new(&arrows(ArrowTimingBucket {
            count: 123,
            stats: TimingStats {
                mean_abs_ms: 12.345,
                mean_ms: -3.5,
                stddev_ms: 2.25,
                max_abs_ms: 20.0,
            },
        }))
        .expect("four columns have timing-arrow presentation");

        assert!(matches!(text.cells[0][0], TextContent::InlineU32(_)));
        assert_eq!(text.cells[0][0].as_str(), "123");
        assert_eq!(text.cells[1][0].as_str(), "12.35");
        assert_eq!(text.cells[2][0].as_str(), "-3.50");
        assert_eq!(text.cells[3][0].as_str(), "6.75");
        assert_eq!(text.cells[4][0].as_str(), "20.00");
        assert!(
            text.cells[1..]
                .iter()
                .flatten()
                .all(|cell| matches!(cell, TextContent::Inline(_)))
        );
    }

    #[test]
    fn timing_arrow_text_keeps_empty_buckets_static() {
        let text = TimingArrowsText::new(&arrows(ArrowTimingBucket::default()))
            .expect("four columns have timing-arrow presentation");

        assert!(
            text.cells
                .iter()
                .flatten()
                .all(|cell| matches!(cell, TextContent::Static("-")))
        );
    }

    #[test]
    fn timing_arrow_text_shares_oversized_float_fallback() {
        let text = TimingArrowsText::new(&arrows(ArrowTimingBucket {
            count: u32::MAX,
            stats: TimingStats {
                mean_abs_ms: f32::MAX,
                mean_ms: f32::MIN,
                stddev_ms: f32::MAX,
                max_abs_ms: f32::MAX,
            },
        }))
        .expect("four columns have timing-arrow presentation");

        assert_eq!(text.cells[0][0].as_str(), u32::MAX.to_string());
        assert_eq!(text.cells[1][0].as_str(), format!("{:.2}", f32::MAX));
        assert!(matches!(text.cells[1][0], TextContent::Shared(_)));
        assert!(TimingArrowsText::new(&ArrowTimingStats::default()).is_none());
    }
}
