use crate::act;
use crate::assets::AssetManager;
use crate::assets::i18n::{LookupKey, lookup_key};
use crate::engine::present::actors::Actor;
use crate::engine::present::color;
use crate::engine::present::font;
use crate::engine::space::screen_center_y;
use crate::game::judgment::JudgeGrade;
use crate::game::profile;
use crate::screens::evaluation::{EvalPane, ScoreInfo};
use std::sync::{Arc, LazyLock};

use super::utils::pane_origin_x;

// Simply Love metrics.ini [RollingNumbersEvaluation]: ApproachSeconds=1
const ROLLING_NUMBERS_APPROACH_SECONDS: f32 = 1.0;

#[inline(always)]
pub(crate) const fn rolling_numbers_approach_seconds() -> f32 {
    ROLLING_NUMBERS_APPROACH_SECONDS
}

static JUDGMENT_ORDER: [JudgeGrade; 6] = [
    JudgeGrade::Fantastic,
    JudgeGrade::Excellent,
    JudgeGrade::Great,
    JudgeGrade::Decent,
    JudgeGrade::WayOff,
    JudgeGrade::Miss,
];

#[derive(Clone, Copy)]
struct LabeledColor {
    label: LookupKey,
    color: [f32; 4],
}

const JUDGMENT_INFO: [LabeledColor; 6] = [
    LabeledColor {
        label: lookup_key("Gameplay", "JudgmentFantastic"),
        color: color::JUDGMENT_RGBA[0],
    },
    LabeledColor {
        label: lookup_key("Gameplay", "JudgmentExcellent"),
        color: color::JUDGMENT_RGBA[1],
    },
    LabeledColor {
        label: lookup_key("Gameplay", "JudgmentGreat"),
        color: color::JUDGMENT_RGBA[2],
    },
    LabeledColor {
        label: lookup_key("Gameplay", "JudgmentDecent"),
        color: color::JUDGMENT_RGBA[3],
    },
    LabeledColor {
        label: lookup_key("Gameplay", "JudgmentWayOff"),
        color: color::JUDGMENT_RGBA[4],
    },
    LabeledColor {
        label: lookup_key("Gameplay", "JudgmentMiss"),
        color: color::JUDGMENT_RGBA[5],
    },
];

const RADAR_LABELS: [LookupKey; 4] = [
    lookup_key("Gameplay", "HandsLabel"),
    lookup_key("Gameplay", "HoldsLabel"),
    lookup_key("Gameplay", "MinesLabel"),
    lookup_key("Gameplay", "RollsLabel"),
];

static DIGIT_TEXT: LazyLock<[Arc<str>; 10]> =
    LazyLock::new(|| ["0", "1", "2", "3", "4", "5", "6", "7", "8", "9"].map(Arc::<str>::from));
static TEN_MS_TEXT: LazyLock<Arc<str>> = LazyLock::new(|| Arc::<str>::from("(10ms)"));
static SLASH_TEXT: LazyLock<Arc<str>> = LazyLock::new(|| Arc::<str>::from("/"));

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

#[inline(always)]
fn digit_text(digit: u8) -> Arc<str> {
    DIGIT_TEXT[digit.min(9) as usize].clone()
}

#[inline(always)]
fn judgment_label_text(index: usize) -> Arc<str> {
    JUDGMENT_INFO
        .get(index)
        .map(|info| info.label.get())
        .unwrap_or_else(|| Arc::from(""))
}

#[inline(always)]
fn radar_label_text(index: usize) -> Arc<str> {
    RADAR_LABELS
        .get(index)
        .map(LookupKey::get)
        .unwrap_or_else(|| Arc::from(""))
}

#[inline(always)]
const fn decimal_digits(value: u32) -> usize {
    if value >= 1_000_000_000 {
        10
    } else if value >= 100_000_000 {
        9
    } else if value >= 10_000_000 {
        8
    } else if value >= 1_000_000 {
        7
    } else if value >= 100_000 {
        6
    } else if value >= 10_000 {
        5
    } else if value >= 1_000 {
        4
    } else if value >= 100 {
        3
    } else if value >= 10 {
        2
    } else {
        1
    }
}

#[inline(always)]
fn fill_padded_digits(mut value: u32, width: usize, out: &mut [u8; 10]) -> usize {
    let width = width.min(out.len());
    let mut idx = width;
    while idx > 0 {
        idx -= 1;
        out[idx] = (value % 10) as u8;
        value /= 10;
    }
    let mut first_nonzero = 0usize;
    while first_nonzero < width && out[first_nonzero] == 0 {
        first_nonzero += 1;
    }
    first_nonzero
}

#[inline(always)]
fn max_window_count(wc: crate::game::timing::WindowCounts) -> u32 {
    wc.w0
        .max(wc.w1)
        .max(wc.w2)
        .max(wc.w3)
        .max(wc.w4)
        .max(wc.w5)
        .max(wc.miss)
}

#[inline(always)]
fn actor_capacity(show_fa_plus_pane: bool, show_10ms_blue: bool, digits_to_fmt: usize) -> usize {
    let judgment_rows = if show_fa_plus_pane { 7 } else { 6 };
    let judgment_labels = judgment_rows + usize::from(show_10ms_blue);
    let radar_rows = 4;
    judgment_labels + (judgment_rows * digits_to_fmt) + (radar_rows * 8)
}

/// Builds a 300px evaluation pane for a given controller side, including judgment and radar counts.
pub(crate) fn build_stats_pane(
    score_info: &ScoreInfo,
    pane: EvalPane,
    controller: profile::PlayerSide,
    asset_manager: &AssetManager,
    elapsed_s: f32,
) -> Vec<Actor> {
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
    let judgment_counts = JUDGMENT_ORDER.map(|grade| score_info.judgment_count(grade));
    let show_standard_judgments = !show_fa_plus_pane;

    // --- Calculate label shift for large numbers ---
    let max_judgment_count = if show_standard_judgments {
        *judgment_counts.iter().max().unwrap_or(&0)
    } else {
        max_window_count(wc)
    };

    let (label_shift_x, label_zoom, sublabel_zoom) = if max_judgment_count > 9999 {
        let length = decimal_digits(max_judgment_count) as i32;
        (
            -11.0 * (length - 4) as f32,
            0.1f32.mul_add(-((length - 4) as f32), 0.833),
            0.1f32.mul_add(-((length - 4) as f32), 0.6),
        )
    } else {
        (0.0, 0.833, 0.6)
    };

    let digits_needed = decimal_digits(max_judgment_count);
    let digits_to_fmt = digits_needed.max(4);
    let mut actors = Vec::with_capacity(actor_capacity(
        show_fa_plus_pane,
        show_10ms_blue,
        digits_to_fmt,
    ));

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
        let label_local_x = (28.0f32).mul_add(1.0, label_shift_x * side_sign) * side_sign;
        let number_base_x = numbers_frame_origin_x + (number_local_x * numbers_frame_zoom);
        let mut digits = [0u8; 10];

        if show_standard_judgments {
            for (i, info) in JUDGMENT_INFO.iter().enumerate() {
                let target_count = judgment_counts[i];
                let count = rolling_number_value(target_count, elapsed_s);

                // Label
                let label_local_y = (i as f32).mul_add(28.0, -16.0);
                actors.push(act!(text: font("miso"): settext(judgment_label_text(i)):
                    align(1.0, 0.5): xy(labels_frame_origin_x + label_local_x, frame_origin_y + label_local_y):
                    maxwidth(76.0): zoom(label_zoom): horizalign(right):
                    diffuse(info.color[0], info.color[1], info.color[2], info.color[3]): z(101)
                ));

                // Number (digit by digit for dimming)
                let bright_color = info.color;
                let dim_color = color::JUDGMENT_DIM_EVAL_RGBA[i];
                let first_nonzero = fill_padded_digits(count, digits_to_fmt, &mut digits);

                let number_local_y = (i as f32).mul_add(35.0, -20.0);
                let number_final_y = frame_origin_y + (number_local_y * numbers_frame_zoom);
                for (char_idx, digit) in digits.iter().take(digits_to_fmt).enumerate() {
                    let is_dim = if count == 0 { char_idx < digits_to_fmt - 1 } else { char_idx < first_nonzero };
                    let color = if is_dim { dim_color } else { bright_color };
                    let index_from_right = digits_to_fmt - 1 - char_idx;
                    let cell_right_x = (index_from_right as f32).mul_add(-digit_width, number_base_x);

                    actors.push(act!(text: font("wendy_screenevaluation"): settext(digit_text(*digit)):
                        align(1.0, 0.5): xy(cell_right_x, number_final_y): zoom(final_numbers_zoom):
                        diffuse(color[0], color[1], color[2], color[3]): z(101)
                    ));
                }
            }
        } else {
            // Dim colors: reuse the standard evaluation dim palette for blue Fantastic
            // through Miss, and use a dedicated dim color for the white FA+ row.
            // White Fantastic (FA+ outer window) bright/dim colors.
            let white_fa_color = color::JUDGMENT_FA_PLUS_WHITE_RGBA;
            let dim_white_fa = color::JUDGMENT_FA_PLUS_WHITE_EVAL_DIM_RGBA;

            let rows: [(usize, [f32; 4], [f32; 4], u32); 7] = [
                (0, JUDGMENT_INFO[0].color, color::JUDGMENT_DIM_EVAL_RGBA[0], wc.w0),
                (0, white_fa_color, dim_white_fa, wc.w1),
                (1, JUDGMENT_INFO[1].color, color::JUDGMENT_DIM_EVAL_RGBA[1], wc.w2),
                (2, JUDGMENT_INFO[2].color, color::JUDGMENT_DIM_EVAL_RGBA[2], wc.w3),
                (3, JUDGMENT_INFO[3].color, color::JUDGMENT_DIM_EVAL_RGBA[3], wc.w4),
                (4, JUDGMENT_INFO[4].color, color::JUDGMENT_DIM_EVAL_RGBA[4], wc.w5),
                (5, JUDGMENT_INFO[5].color, color::JUDGMENT_DIM_EVAL_RGBA[5], wc.miss),
            ];

            for (i, (label_idx, bright_color, dim_color, count)) in rows.iter().enumerate() {
                let count = rolling_number_value(*count, elapsed_s);
                // Label: match Simply Love Pane2 labels using 26px spacing.
                // Original Lua uses 1-based indexing: y = i*26 - 46.
                // Our rows are 0-based, so use (i+1) here.
                let label_local_y = (i as f32 + 1.0).mul_add(26.0, -46.0);
                actors.push(act!(text: font("miso"): settext(judgment_label_text(*label_idx)):
                    align(1.0, 0.5): xy(labels_frame_origin_x + label_local_x, frame_origin_y + label_local_y):
                    maxwidth(76.0): zoom(label_zoom): horizalign(right):
                    diffuse(bright_color[0], bright_color[1], bright_color[2], bright_color[3]): z(101)
                ));
                if show_10ms_blue && i == 0 {
                    actors.push(act!(text: font("miso"): settext(TEN_MS_TEXT.clone()):
                        align(1.0, 0.5):
                        xy(labels_frame_origin_x + label_local_x, frame_origin_y + label_local_y + 10.0):
                        maxwidth(76.0): zoom(sublabel_zoom): horizalign(right):
                        diffuse(bright_color[0], bright_color[1], bright_color[2], bright_color[3]): z(101)
                    ));
                }

                // Number
                let first_nonzero = fill_padded_digits(count, digits_to_fmt, &mut digits);

                // Numbers: match Simply Love Pane2 numbers using 32px spacing.
                let number_local_y = (i as f32).mul_add(32.0, -24.0);
                let number_final_y = frame_origin_y + (number_local_y * numbers_frame_zoom);
                for (char_idx, digit) in digits.iter().take(digits_to_fmt).enumerate() {
                    let is_dim = if count == 0 { char_idx < digits_to_fmt - 1 } else { char_idx < first_nonzero };
                    let color = if is_dim { *dim_color } else { *bright_color };
                    let index_from_right = digits_to_fmt - 1 - char_idx;
                    let cell_right_x = (index_from_right as f32).mul_add(-digit_width, number_base_x);

                    actors.push(act!(text: font("wendy_screenevaluation"): settext(digit_text(*digit)):
                        align(1.0, 0.5): xy(cell_right_x, number_final_y): zoom(final_numbers_zoom):
                        diffuse(color[0], color[1], color[2], color[3]): z(101)
                    ));
                }
            }
        }

        // --- RADAR LABELS & NUMBERS ---
        let radar_categories = [
            ("hands", score_info.hands_achieved, score_info.hands_total),
            ("holds", score_info.holds_held, score_info.holds_total),
            ("mines", score_info.mines_avoided, score_info.mines_total),
            ("rolls", score_info.rolls_held, score_info.rolls_total),
        ];

        const GRAY_POSSIBLE: [f32; 4] = color::rgba_hex("#5A6166");
        const GRAY_ACHIEVED: [f32; 4] = color::rgba_hex("#444444");
        let white_color = [1.0, 1.0, 1.0, 1.0];

        for (i, (_, achieved, possible)) in radar_categories.iter().copied().enumerate() {
            let label_local_x = if controller == profile::PlayerSide::P1 {
                -160.0
            } else {
                90.0
            };
            let label_local_y = (i as f32).mul_add(28.0, 41.0);
            actors.push(act!(text: font("miso"): settext(radar_label_text(i)):
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

            let first_nonzero_achieved = fill_padded_digits(achieved_rolling, 3, &mut digits);

            for char_idx_from_right in 0..3 {
                let is_dim = if achieved_rolling == 0 {
                    char_idx_from_right > 0
                } else {
                    let idx_from_left = 2 - char_idx_from_right;
                    idx_from_left < first_nonzero_achieved
                };
                let color = if is_dim { GRAY_ACHIEVED } else { white_color };
                let x_pos = (char_idx_from_right as f32).mul_add(-digit_width, achieved_anchor_x);
                let digit_idx = 2 - char_idx_from_right;

                actors.push(act!(text: font("wendy_screenevaluation"): settext(digit_text(digits[digit_idx])):
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
            let first_nonzero_possible = fill_padded_digits(possible_clamped, 3, &mut digits);

            for char_idx_from_right in 0..3 {
                let is_dim = if possible_clamped == 0 {
                    char_idx_from_right > 0
                } else {
                    let idx_from_left = 2 - char_idx_from_right;
                    idx_from_left < first_nonzero_possible
                };
                let color = if is_dim { GRAY_POSSIBLE } else { white_color };
                let digit_idx = 2 - char_idx_from_right;

                actors.push(act!(text: font("wendy_screenevaluation"): settext(digit_text(digits[digit_idx])):
                    align(1.0, 0.5): xy(cursor_x, number_final_y): zoom(final_numbers_zoom):
                    diffuse(color[0], color[1], color[2], color[3]): z(101)
                ));
                cursor_x -= digit_width;
            }

            // 2. Draw slash
            // Moved 1px to the right for visual parity
            actors.push(act!(text: font("wendy_screenevaluation"): settext(SLASH_TEXT.clone()):
                align(1.0, 0.5): xy(cursor_x + 0.5, number_final_y): zoom(final_numbers_zoom):
                diffuse(GRAY_POSSIBLE[0], GRAY_POSSIBLE[1], GRAY_POSSIBLE[2], GRAY_POSSIBLE[3]): z(101)
            ));
        }
    }));

    actors
}
