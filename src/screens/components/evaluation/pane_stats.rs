use std::collections::HashMap;
use std::sync::LazyLock;

use crate::act;
use crate::assets::AssetManager;
use crate::core::space::screen_center_y;
use crate::game::judgment::JudgeGrade;
use crate::game::profile;
use crate::screens::evaluation::{EvalPane, ScoreInfo};
use crate::ui::actors::Actor;
use crate::ui::color;
use crate::ui::font;

use super::utils::pane_origin_x;

// Simply Love metrics.ini [RollingNumbersEvaluation]: ApproachSeconds=1
const ROLLING_NUMBERS_APPROACH_SECONDS: f32 = 1.0;

static JUDGMENT_ORDER: [JudgeGrade; 6] = [
    JudgeGrade::Fantastic,
    JudgeGrade::Excellent,
    JudgeGrade::Great,
    JudgeGrade::Decent,
    JudgeGrade::WayOff,
    JudgeGrade::Miss,
];

struct JudgmentDisplayInfo {
    label: &'static str,
    color: [f32; 4],
}

static JUDGMENT_INFO: LazyLock<HashMap<JudgeGrade, JudgmentDisplayInfo>> = LazyLock::new(|| {
    HashMap::from([
        (
            JudgeGrade::Fantastic,
            JudgmentDisplayInfo {
                label: "FANTASTIC",
                color: color::JUDGMENT_RGBA[0],
            },
        ),
        (
            JudgeGrade::Excellent,
            JudgmentDisplayInfo {
                label: "EXCELLENT",
                color: color::JUDGMENT_RGBA[1],
            },
        ),
        (
            JudgeGrade::Great,
            JudgmentDisplayInfo {
                label: "GREAT",
                color: color::JUDGMENT_RGBA[2],
            },
        ),
        (
            JudgeGrade::Decent,
            JudgmentDisplayInfo {
                label: "DECENT",
                color: color::JUDGMENT_RGBA[3],
            },
        ),
        (
            JudgeGrade::WayOff,
            JudgmentDisplayInfo {
                label: "WAY OFF",
                color: color::JUDGMENT_RGBA[4],
            },
        ),
        (
            JudgeGrade::Miss,
            JudgmentDisplayInfo {
                label: "MISS",
                color: color::JUDGMENT_RGBA[5],
            },
        ),
    ])
});

#[inline(always)]
fn rolling_number_value(target: u32, elapsed_s: f32) -> u32 {
    if target == 0 {
        return 0;
    }
    let approach_s = ROLLING_NUMBERS_APPROACH_SECONDS;
    if approach_s <= 0.0 || elapsed_s >= approach_s {
        return target;
    }
    let velocity = target as f32 / approach_s;
    let current = (velocity * elapsed_s).clamp(0.0, target as f32);
    current.round() as u32
}

/// Builds a 300px evaluation pane for a given controller side, including judgment and radar counts.
pub fn build_stats_pane(
    score_info: &ScoreInfo,
    pane: EvalPane,
    controller: profile::PlayerSide,
    asset_manager: &AssetManager,
    elapsed_s: f32,
) -> Vec<Actor> {
    let mut actors = Vec::new();
    let cy = screen_center_y();

    let pane_origin_x = pane_origin_x(controller);
    let side_sign = if controller == profile::PlayerSide::P1 {
        1.0_f32
    } else {
        -1.0_f32
    };

    // Active evaluation pane is chosen at runtime; the profile toggle
    // only selects which pane is shown first.
    let show_fa_plus_pane = matches!(pane, EvalPane::FaPlus | EvalPane::HardEx);
    let show_10ms_blue = matches!(pane, EvalPane::HardEx);
    let wc = if show_10ms_blue {
        score_info.window_counts_10ms
    } else {
        score_info.window_counts
    };

    // --- Calculate label shift for large numbers ---
    let max_judgment_count = if !show_fa_plus_pane {
        JUDGMENT_ORDER
            .iter()
            .map(|grade| score_info.judgment_counts.get(grade).copied().unwrap_or(0))
            .max()
            .unwrap_or(0)
    } else {
        *[wc.w0, wc.w1, wc.w2, wc.w3, wc.w4, wc.w5, wc.miss]
            .iter()
            .max()
            .unwrap_or(&0)
    };

    let (label_shift_x, label_zoom, sublabel_zoom) = if max_judgment_count > 9999 {
        let length = (max_judgment_count as f32).log10().floor() as i32 + 1;
        (
            -11.0 * (length - 4) as f32,
            0.1f32.mul_add(-((length - 4) as f32), 0.833),
            0.1f32.mul_add(-((length - 4) as f32), 0.6),
        )
    } else {
        (0.0, 0.833, 0.6)
    };

    let digits_needed = if max_judgment_count == 0 {
        1
    } else {
        (max_judgment_count as f32).log10().floor() as usize + 1
    };
    let digits_to_fmt = digits_needed.max(4);

    asset_manager.with_fonts(|all_fonts| asset_manager.with_font("wendy_screenevaluation", |metrics_font| {
        let numbers_frame_zoom: f32 = 0.8;
        let final_numbers_zoom = numbers_frame_zoom * 0.5;
        let digit_width = font::measure_line_width_logical(metrics_font, "0", all_fonts) as f32 * final_numbers_zoom;
        if digit_width <= 0.0 { return; }

        // --- Judgment Labels & Numbers ---
        let labels_frame_origin_x = (50.0 * side_sign).mul_add(1.0, pane_origin_x);
        let numbers_frame_origin_x = (90.0 * side_sign).mul_add(1.0, pane_origin_x);
        let frame_origin_y = cy - 24.0;
        let number_local_x = if controller == profile::PlayerSide::P1 {
            64.0
        } else {
            94.0
        };

        if !show_fa_plus_pane {
            for (i, grade) in JUDGMENT_ORDER.iter().enumerate() {
                let info = JUDGMENT_INFO.get(grade).unwrap();
                let target_count = score_info.judgment_counts.get(grade).copied().unwrap_or(0);
                let count = rolling_number_value(target_count, elapsed_s);

                // Label
                let label_local_x = (28.0f32).mul_add(1.0, label_shift_x * side_sign) * side_sign;
                let label_local_y = (i as f32).mul_add(28.0, -16.0);
                actors.push(act!(text: font("miso"): settext(info.label):
                    align(1.0, 0.5): xy(labels_frame_origin_x + label_local_x, frame_origin_y + label_local_y):
                    maxwidth(76.0): zoom(label_zoom): horizalign(right):
                    diffuse(info.color[0], info.color[1], info.color[2], info.color[3]): z(101)
                ));

                // Number (digit by digit for dimming)
                let bright_color = info.color;
                let dim_color = color::JUDGMENT_DIM_EVAL_RGBA[i];
                let number_str = format!("{count:0digits_to_fmt$}");
                let first_nonzero = number_str.find(|c: char| c != '0').unwrap_or(number_str.len());

                let number_local_y = (i as f32).mul_add(35.0, -20.0);
                let number_final_y = frame_origin_y + (number_local_y * numbers_frame_zoom);
                let number_base_x = numbers_frame_origin_x + (number_local_x * numbers_frame_zoom);

                for (char_idx, ch) in number_str.chars().enumerate() {
                    let is_dim = if count == 0 { char_idx < digits_to_fmt - 1 } else { char_idx < first_nonzero };
                    let color = if is_dim { dim_color } else { bright_color };
                    let index_from_right = digits_to_fmt - 1 - char_idx;
                    let cell_right_x = (index_from_right as f32).mul_add(-digit_width, number_base_x);

                    actors.push(act!(text: font("wendy_screenevaluation"): settext(ch.to_string()):
                        align(1.0, 0.5): xy(cell_right_x, number_final_y): zoom(final_numbers_zoom):
                        diffuse(color[0], color[1], color[2], color[3]): z(101)
                    ));
                }
            }
        } else {
            let fantastic_color = JUDGMENT_INFO
                .get(&JudgeGrade::Fantastic).map_or_else(|| color::JUDGMENT_RGBA[0], |info| info.color);
            let excellent_color = JUDGMENT_INFO
                .get(&JudgeGrade::Excellent).map_or_else(|| color::JUDGMENT_RGBA[1], |info| info.color);
            let great_color = JUDGMENT_INFO
                .get(&JudgeGrade::Great).map_or_else(|| color::JUDGMENT_RGBA[2], |info| info.color);
            let decent_color = JUDGMENT_INFO
                .get(&JudgeGrade::Decent).map_or_else(|| color::JUDGMENT_RGBA[3], |info| info.color);
            let wayoff_color = JUDGMENT_INFO
                .get(&JudgeGrade::WayOff).map_or_else(|| color::JUDGMENT_RGBA[4], |info| info.color);
            let miss_color = JUDGMENT_INFO
                .get(&JudgeGrade::Miss).map_or_else(|| color::JUDGMENT_RGBA[5], |info| info.color);

            // Dim colors: reuse the standard evaluation dim palette for blue Fantastic
            // through Miss, and use a dedicated dim color for the white FA+ row.
            let dim_fantastic = color::JUDGMENT_DIM_EVAL_RGBA[0];
            let dim_excellent = color::JUDGMENT_DIM_EVAL_RGBA[1];
            let dim_great = color::JUDGMENT_DIM_EVAL_RGBA[2];
            let dim_decent = color::JUDGMENT_DIM_EVAL_RGBA[3];
            let dim_wayoff = color::JUDGMENT_DIM_EVAL_RGBA[4];
            let dim_miss = color::JUDGMENT_DIM_EVAL_RGBA[5];
            // White Fantastic (FA+ outer window) bright/dim colors.
            let white_fa_color = color::JUDGMENT_FA_PLUS_WHITE_RGBA;
            let dim_white_fa = color::JUDGMENT_FA_PLUS_WHITE_EVAL_DIM_RGBA;

            let rows: [(&str, [f32; 4], [f32; 4], u32); 7] = [
                ("FANTASTIC", fantastic_color, dim_fantastic, wc.w0),
                ("FANTASTIC",       white_fa_color, dim_white_fa, wc.w1),
                ("EXCELLENT", excellent_color, dim_excellent, wc.w2),
                ("GREAT", great_color, dim_great, wc.w3),
                ("DECENT", decent_color, dim_decent, wc.w4),
                ("WAY OFF", wayoff_color, dim_wayoff, wc.w5),
                ("MISS", miss_color, dim_miss, wc.miss),
            ];

            for (i, (label, bright_color, dim_color, count)) in rows.iter().enumerate() {
                let count = rolling_number_value(*count, elapsed_s);
                // Label: match Simply Love Pane2 labels using 26px spacing.
                // Original Lua uses 1-based indexing: y = i*26 - 46.
                // Our rows are 0-based, so use (i+1) here.
                let label_local_x = (28.0f32).mul_add(1.0, label_shift_x * side_sign) * side_sign;
                let label_local_y = (i as f32 + 1.0).mul_add(26.0, -46.0);
                actors.push(act!(text: font("miso"): settext(label.to_string()):
                    align(1.0, 0.5): xy(labels_frame_origin_x + label_local_x, frame_origin_y + label_local_y):
                    maxwidth(76.0): zoom(label_zoom): horizalign(right):
                    diffuse(bright_color[0], bright_color[1], bright_color[2], bright_color[3]): z(101)
                ));
                if show_10ms_blue && i == 0 {
                    actors.push(act!(text: font("miso"): settext("(10ms)".to_string()):
                        align(1.0, 0.5):
                        xy(labels_frame_origin_x + label_local_x, frame_origin_y + label_local_y + 10.0):
                        maxwidth(76.0): zoom(sublabel_zoom): horizalign(right):
                        diffuse(bright_color[0], bright_color[1], bright_color[2], bright_color[3]): z(101)
                    ));
                }

                // Number
                let number_str = format!("{count:0digits_to_fmt$}");
                let first_nonzero = number_str.find(|c: char| c != '0').unwrap_or(number_str.len());

                // Numbers: match Simply Love Pane2 numbers using 32px spacing.
                let number_local_y = (i as f32).mul_add(32.0, -24.0);
                let number_final_y = frame_origin_y + (number_local_y * numbers_frame_zoom);
                let number_base_x = numbers_frame_origin_x + (number_local_x * numbers_frame_zoom);

                for (char_idx, ch) in number_str.chars().enumerate() {
                    let is_dim = if count == 0 { char_idx < digits_to_fmt - 1 } else { char_idx < first_nonzero };
                    let color = if is_dim { *dim_color } else { *bright_color };
                    let index_from_right = digits_to_fmt - 1 - char_idx;
                    let cell_right_x = (index_from_right as f32).mul_add(-digit_width, number_base_x);

                    actors.push(act!(text: font("wendy_screenevaluation"): settext(ch.to_string()):
                        align(1.0, 0.5): xy(cell_right_x, number_final_y): zoom(final_numbers_zoom):
                        diffuse(color[0], color[1], color[2], color[3]): z(101)
                    ));
                }
            }
        }

        // --- RADAR LABELS & NUMBERS ---
        let radar_categories = [
            ("hands", score_info.hands_achieved, score_info.chart.stats.hands),
            ("holds", score_info.holds_held, score_info.holds_total),
            ("mines", score_info.mines_avoided, score_info.mines_total),
            ("rolls", score_info.rolls_held, score_info.rolls_total),
        ];

        const GRAY_POSSIBLE: [f32; 4] = color::rgba_hex("#5A6166");
        const GRAY_ACHIEVED: [f32; 4] = color::rgba_hex("#444444");
        let white_color = [1.0, 1.0, 1.0, 1.0];

        for (i, (label, achieved, possible)) in radar_categories.iter().copied().enumerate() {
            let label_local_x = if controller == profile::PlayerSide::P1 {
                -160.0
            } else {
                90.0
            };
            let label_local_y = (i as f32).mul_add(28.0, 41.0);
            actors.push(act!(text: font("miso"): settext(label.to_string()):
                align(1.0, 0.5): xy(labels_frame_origin_x + label_local_x, frame_origin_y + label_local_y): horizalign(right): zoom(0.833): z(101)
            ));

            let possible_clamped = possible.min(999);
            let achieved_clamped = achieved.min(999);
            let achieved_rolling = rolling_number_value(achieved_clamped, elapsed_s);

            let number_local_y = (i as f32).mul_add(35.0, 53.0);
            let number_final_y = frame_origin_y + (number_local_y * numbers_frame_zoom);

            // --- Group 1: "Achieved" Numbers (Anchored at -180, separated from Slash) ---
            // Matches Lua: x = { P1=-180 }, aligned right.
            let achieved_anchor_x = (if controller == profile::PlayerSide::P1 {
                -180.0_f32
            } else {
                218.0_f32
            })
            .mul_add(numbers_frame_zoom, numbers_frame_origin_x);

            let achieved_str = format!("{achieved_rolling:03}");
            let first_nonzero_achieved = achieved_str.find(|c: char| c != '0').unwrap_or(achieved_str.len());

            for (char_idx_from_right, ch) in achieved_str.chars().rev().enumerate() {
                let is_dim = if achieved_rolling == 0 {
                    char_idx_from_right > 0
                } else {
                    let idx_from_left = 2 - char_idx_from_right;
                    idx_from_left < first_nonzero_achieved
                };
                let color = if is_dim { GRAY_ACHIEVED } else { white_color };
                let x_pos = (char_idx_from_right as f32).mul_add(-digit_width, achieved_anchor_x);

                actors.push(act!(text: font("wendy_screenevaluation"): settext(ch.to_string()):
                    align(1.0, 0.5): xy(x_pos, number_final_y): zoom(final_numbers_zoom):
                    diffuse(color[0], color[1], color[2], color[3]): z(101)
                ));
            }

            // --- Group 2: "Slash + Possible" Numbers (Anchored at -114) ---
            // Matches Lua: x = { P1=-114 }, aligned right.
            let possible_anchor_x = (if controller == profile::PlayerSide::P1 {
                -114.0_f32
            } else {
                286.0_f32
            })
            .mul_add(numbers_frame_zoom, numbers_frame_origin_x);
            let mut cursor_x = possible_anchor_x;

            // 1. Draw "possible" number (right-most part)
            let possible_str = format!("{possible_clamped:03}");
            let first_nonzero_possible = possible_str.find(|c: char| c != '0').unwrap_or(possible_str.len());

            for (char_idx_from_right, ch) in possible_str.chars().rev().enumerate() {
                let is_dim = if possible_clamped == 0 {
                    char_idx_from_right > 0
                } else {
                    let idx_from_left = 2 - char_idx_from_right;
                    idx_from_left < first_nonzero_possible
                };
                let color = if is_dim { GRAY_POSSIBLE } else { white_color };

                actors.push(act!(text: font("wendy_screenevaluation"): settext(ch.to_string()):
                    align(1.0, 0.5): xy(cursor_x, number_final_y): zoom(final_numbers_zoom):
                    diffuse(color[0], color[1], color[2], color[3]): z(101)
                ));
                cursor_x -= digit_width;
            }

            // 2. Draw slash
            // Moved 1px to the right for visual parity
            actors.push(act!(text: font("wendy_screenevaluation"): settext("/"):
                align(1.0, 0.5): xy(cursor_x + 0.5, number_final_y): zoom(final_numbers_zoom):
                diffuse(GRAY_POSSIBLE[0], GRAY_POSSIBLE[1], GRAY_POSSIBLE[2], GRAY_POSSIBLE[3]): z(101)
            ));
        }
    }));

    actors
}
