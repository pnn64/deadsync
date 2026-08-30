use crate::act;
use crate::assets::AssetManager;
use crate::assets::i18n::{LookupKey, lookup_key};
use crate::assets::{FontRole, machine_font_key};
use crate::config::MachineFont;
use crate::screens::evaluation::{EvalPane, ScoreInfo};
use deadlib_present::actors::{Actor, SizeSpec};
use deadlib_present::color;
use deadlib_present::color::{JudgmentColorRole as Role, JudgmentPalette};
use deadlib_present::font;
use deadlib_present::space::screen_center_y;
use deadsync_profile as profile_data;
use deadsync_rules::judgment::JudgeGrade;
use deadsync_rules::timing::WindowCounts;
use std::cell::RefCell;
use std::sync::{Arc, LazyLock};

use super::utils::pane_origin_x;

// Simply Love metrics.ini [RollingNumbersEvaluation]: ApproachSeconds=1
const ROLLING_NUMBERS_APPROACH_SECONDS: f32 = 1.0;
const DISABLED_WINDOW_RGBA: [f32; 4] = color::JUDGMENT_FA_PLUS_WHITE_EVAL_DIM_RGBA;

#[derive(Clone, Copy, PartialEq)]
struct StatsPaneCacheKey {
    controller: profile_data::PlayerSide,
    machine_font: MachineFont,
    palette: JudgmentPalette,
    font_address: usize,
    font_count: usize,
    language_revision: u64,
}

type SettledStatsPane = Option<(StatsPaneCacheKey, Arc<[Actor]>)>;

/// Immutable judgment/radar values and three bounded settled-pane actor caches.
#[derive(Clone)]
pub(crate) struct StatsPanePresentation {
    judgment_counts: [u32; 6],
    window_counts: WindowCounts,
    window_counts_10ms: WindowCounts,
    disabled_timing_windows: [bool; 5],
    radar: [(u32, u32); 4],
    settled: RefCell<[SettledStatsPane; 3]>,
}

impl StatsPanePresentation {
    pub(crate) fn new(score: &ScoreInfo) -> Self {
        Self {
            judgment_counts: JUDGMENT_ORDER.map(|grade| score.judgment_count(grade)),
            window_counts: score.window_counts,
            window_counts_10ms: score.window_counts_10ms,
            disabled_timing_windows: score.disabled_timing_windows,
            radar: [
                (score.hands_achieved, score.hands_total),
                (score.holds_held, score.holds_total),
                (score.mines_avoided, score.mines_total),
                (score.rolls_held, score.rolls_total),
            ],
            settled: RefCell::new(std::array::from_fn(|_| None)),
        }
    }

    fn cached_settled(
        &self,
        pane: EvalPane,
        controller: profile_data::PlayerSide,
        asset_manager: &AssetManager,
        machine_font: MachineFont,
        palette: JudgmentPalette,
    ) -> Arc<[Actor]> {
        let index = stats_pane_index(pane).expect("settled stats cache only accepts stats panes");
        let (font_address, font_count) = stats_font_key(asset_manager, machine_font);
        let key = StatsPaneCacheKey {
            controller,
            machine_font,
            palette,
            font_address,
            font_count,
            language_revision: crate::i18n::revision(),
        };
        if let Some((_, children)) = self.settled.borrow()[index]
            .as_ref()
            .filter(|(cached, _)| *cached == key)
        {
            return Arc::clone(children);
        }

        let mut children = Vec::new();
        push_stats_actors(
            &mut children,
            self,
            pane,
            controller,
            asset_manager,
            ROLLING_NUMBERS_APPROACH_SECONDS,
            machine_font,
            palette,
        );
        let children = Arc::from(children);
        self.settled.borrow_mut()[index] = Some((key, Arc::clone(&children)));
        children
    }
}

#[inline(always)]
const fn stats_pane_index(pane: EvalPane) -> Option<usize> {
    match pane {
        EvalPane::Standard => Some(0),
        EvalPane::FaPlus => Some(1),
        EvalPane::HardEx => Some(2),
        _ => None,
    }
}

fn stats_font_key(asset_manager: &AssetManager, machine_font: MachineFont) -> (usize, usize) {
    let address = asset_manager
        .with_font(
            machine_font_key(machine_font, FontRole::ScreenEval),
            |font| std::ptr::from_ref(font) as usize,
        )
        .unwrap_or(0);
    (address, asset_manager.fonts().len())
}

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

const JUDGMENT_LABELS: [LookupKey; 6] = [
    lookup_key("Gameplay", "JudgmentFantastic"),
    lookup_key("Gameplay", "JudgmentExcellent"),
    lookup_key("Gameplay", "JudgmentGreat"),
    lookup_key("Gameplay", "JudgmentDecent"),
    lookup_key("Gameplay", "JudgmentWayOff"),
    lookup_key("Gameplay", "JudgmentMiss"),
];

const STANDARD_ROLES: [Role; 6] = [
    Role::FantasticBlue,
    Role::Excellent,
    Role::Great,
    Role::Decent,
    Role::WayOff,
    Role::Miss,
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
    JUDGMENT_LABELS
        .get(index)
        .map(LookupKey::get)
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
fn max_window_count(wc: WindowCounts) -> u32 {
    wc.w0
        .max(wc.w1)
        .max(wc.w2)
        .max(wc.w3)
        .max(wc.w4)
        .max(wc.w5)
        .max(wc.miss)
}

#[inline(always)]
const fn standard_row_disabled(disabled_windows: [bool; 5], row: usize) -> bool {
    row < 5 && disabled_windows[row]
}

#[inline(always)]
const fn split_row_disabled(disabled_windows: [bool; 5], row: usize) -> bool {
    match row {
        0 | 1 => disabled_windows[0],
        2 => disabled_windows[1],
        3 => disabled_windows[2],
        4 => disabled_windows[3],
        5 => disabled_windows[4],
        _ => false,
    }
}

#[inline(always)]
fn actor_capacity(
    show_fa_plus_pane: bool,
    show_10ms_blue: bool,
    show_hands_row: bool,
    digits_to_fmt: usize,
) -> usize {
    let judgment_rows = if show_fa_plus_pane { 7 } else { 6 };
    let judgment_labels = judgment_rows + usize::from(show_10ms_blue);
    let radar_rows = radar_rows_for_pane(show_hands_row);
    judgment_labels + (judgment_rows * digits_to_fmt) + (radar_rows * 8)
}

#[inline(always)]
const fn show_hands_row_for_pane(pane: EvalPane) -> bool {
    matches!(pane, EvalPane::Standard)
}

#[inline(always)]
const fn radar_start_index(show_hands_row: bool) -> usize {
    if show_hands_row { 0 } else { 1 }
}

#[inline(always)]
const fn radar_rows_for_pane(show_hands_row: bool) -> usize {
    if show_hands_row { 4 } else { 3 }
}

#[inline(always)]
const fn radar_row_offset(show_hands_row: bool) -> f32 {
    if show_hands_row { 0.0 } else { 1.0 }
}

#[allow(clippy::too_many_arguments)]
fn push_stats_actors(
    out: &mut Vec<Actor>,
    presentation: &StatsPanePresentation,
    pane: EvalPane,
    controller: profile_data::PlayerSide,
    asset_manager: &AssetManager,
    elapsed_s: f32,
    machine_font: MachineFont,
    palette: JudgmentPalette,
) {
    let cy = screen_center_y();

    let pane_origin_x = pane_origin_x(controller);
    let side_sign = if controller == profile_data::PlayerSide::P1 {
        1.0_f32
    } else {
        -1.0_f32
    };

    // Active evaluation pane is chosen at runtime; the profile toggle
    // only selects which pane is shown first.
    let show_fa_plus_pane = matches!(pane, EvalPane::FaPlus | EvalPane::HardEx);
    let show_10ms_blue = matches!(pane, EvalPane::HardEx);
    let wc = if show_10ms_blue {
        presentation.window_counts_10ms
    } else {
        presentation.window_counts
    };
    let judgment_counts = presentation.judgment_counts;
    let show_standard_judgments = !show_fa_plus_pane;
    let show_hands_row = show_hands_row_for_pane(pane);

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
    out.reserve(actor_capacity(
        show_fa_plus_pane,
        show_10ms_blue,
        show_hands_row,
        digits_to_fmt,
    ));

    asset_manager.with_fonts(|all_fonts| asset_manager.with_font(machine_font_key(machine_font, FontRole::ScreenEval), |metrics_font| {
        let numbers_frame_zoom: f32 = 0.8;
        let final_numbers_zoom = numbers_frame_zoom * 0.5;
        let digit_width = font::measure_line_width_logical(metrics_font, "0", all_fonts) as f32 * final_numbers_zoom;
        if digit_width <= 0.0 { return; }

        // --- Judgment Labels & Numbers ---
        let labels_frame_origin_x = (50.0 * side_sign).mul_add(1.0, pane_origin_x);
        let numbers_frame_origin_x = (90.0 * side_sign).mul_add(1.0, pane_origin_x);
        let frame_origin_y = cy - 24.0;
        let number_local_x = if controller == profile_data::PlayerSide::P1 {
            64.0
        } else {
            94.0
        };
        let label_local_x = (28.0f32).mul_add(1.0, label_shift_x * side_sign) * side_sign;
        let number_base_x = numbers_frame_origin_x + (number_local_x * numbers_frame_zoom);
        let mut digits = [0u8; 10];

        if show_standard_judgments {
            for (i, role) in STANDARD_ROLES.iter().copied().enumerate() {
                let target_count = judgment_counts[i];
                let count = rolling_number_value(target_count, elapsed_s);
                let disabled = standard_row_disabled(presentation.disabled_timing_windows, i);
                let bright_color = if disabled {
                    DISABLED_WINDOW_RGBA
                } else {
                    palette.color(role)
                };
                let dim_color = if disabled {
                    DISABLED_WINDOW_RGBA
                } else {
                    palette.evaluation_dim_color(role)
                };

                // Label
                let label_local_y = (i as f32).mul_add(28.0, -16.0);
                out.push(act!(text: font("miso"): settext(judgment_label_text(i)):
                    align(1.0, 0.5): xy(labels_frame_origin_x + label_local_x, frame_origin_y + label_local_y):
                    maxwidth(76.0): zoom(label_zoom): horizalign(right):
                    diffuse(bright_color[0], bright_color[1], bright_color[2], bright_color[3]): z(101)
                ));

                // Number (digit by digit for dimming)
                let first_nonzero = fill_padded_digits(count, digits_to_fmt, &mut digits);

                let number_local_y = (i as f32).mul_add(35.0, -20.0);
                let number_final_y = frame_origin_y + (number_local_y * numbers_frame_zoom);
                for (char_idx, digit) in digits.iter().take(digits_to_fmt).enumerate() {
                    let is_dim = disabled
                        || if count == 0 { char_idx < digits_to_fmt - 1 } else { char_idx < first_nonzero };
                    let color = if is_dim { dim_color } else { bright_color };
                    let index_from_right = digits_to_fmt - 1 - char_idx;
                    let cell_right_x = (index_from_right as f32).mul_add(-digit_width, number_base_x);

                    out.push(act!(text: font(machine_font_key(machine_font, FontRole::ScreenEval)): settext(digit_text(*digit)):
                        align(1.0, 0.5): xy(cell_right_x, number_final_y): zoom(final_numbers_zoom):
                        diffuse(color[0], color[1], color[2], color[3]): z(101)
                    ));
                }
            }
        } else {
            // Dim colors: reuse the standard evaluation dim palette for blue Fantastic
            // through Miss, and use a dedicated dim color for the white FA+ row.
            // White Fantastic (FA+ outer window) bright/dim colors.
            let white_fa_color = palette.color(Role::FantasticWhite);
            let dim_white_fa = palette.evaluation_dim_color(Role::FantasticWhite);

            let rows: [(usize, [f32; 4], [f32; 4], u32); 7] = [
                (0, palette.color(Role::FantasticBlue), palette.evaluation_dim_color(Role::FantasticBlue), wc.w0),
                (0, white_fa_color, dim_white_fa, wc.w1),
                (1, palette.color(Role::Excellent), palette.evaluation_dim_color(Role::Excellent), wc.w2),
                (2, palette.color(Role::Great), palette.evaluation_dim_color(Role::Great), wc.w3),
                (3, palette.color(Role::Decent), palette.evaluation_dim_color(Role::Decent), wc.w4),
                (4, palette.color(Role::WayOff), palette.evaluation_dim_color(Role::WayOff), wc.w5),
                (5, palette.color(Role::Miss), palette.evaluation_dim_color(Role::Miss), wc.miss),
            ];

            for (i, (label_idx, bright_color, dim_color, count)) in rows.iter().enumerate() {
                let count = rolling_number_value(*count, elapsed_s);
                let disabled = split_row_disabled(presentation.disabled_timing_windows, i);
                let bright_color = if disabled {
                    DISABLED_WINDOW_RGBA
                } else {
                    *bright_color
                };
                let dim_color = if disabled {
                    DISABLED_WINDOW_RGBA
                } else {
                    *dim_color
                };
                // Label: match Simply Love Pane2 labels using 26px spacing.
                // Original Lua uses 1-based indexing: y = i*26 - 46.
                // Our rows are 0-based, so use (i+1) here.
                let label_local_y = (i as f32 + 1.0).mul_add(26.0, -46.0);
                out.push(act!(text: font("miso"): settext(judgment_label_text(*label_idx)):
                    align(1.0, 0.5): xy(labels_frame_origin_x + label_local_x, frame_origin_y + label_local_y):
                    maxwidth(76.0): zoom(label_zoom): horizalign(right):
                    diffuse(bright_color[0], bright_color[1], bright_color[2], bright_color[3]): z(101)
                ));
                if show_10ms_blue && i == 0 {
                    out.push(act!(text: font("miso"): settext(TEN_MS_TEXT.clone()):
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
                    let is_dim = disabled
                        || if count == 0 { char_idx < digits_to_fmt - 1 } else { char_idx < first_nonzero };
                    let color = if is_dim { dim_color } else { bright_color };
                    let index_from_right = digits_to_fmt - 1 - char_idx;
                    let cell_right_x = (index_from_right as f32).mul_add(-digit_width, number_base_x);

                    out.push(act!(text: font(machine_font_key(machine_font, FontRole::ScreenEval)): settext(digit_text(*digit)):
                        align(1.0, 0.5): xy(cell_right_x, number_final_y): zoom(final_numbers_zoom):
                        diffuse(color[0], color[1], color[2], color[3]): z(101)
                    ));
                }
            }
        }

        // --- RADAR LABELS & NUMBERS ---
        let radar_categories = [
            (0, presentation.radar[0].0, presentation.radar[0].1),
            (1, presentation.radar[1].0, presentation.radar[1].1),
            (2, presentation.radar[2].0, presentation.radar[2].1),
            (3, presentation.radar[3].0, presentation.radar[3].1),
        ];
        let radar_start_index = radar_start_index(show_hands_row);
        let radar_categories = &radar_categories[radar_start_index..];
        let radar_row_offset = radar_row_offset(show_hands_row);

        const GRAY_POSSIBLE: [f32; 4] = color::rgba_hex("#5A6166");
        const GRAY_ACHIEVED: [f32; 4] = color::rgba_hex("#444444");
        let white_color = [1.0, 1.0, 1.0, 1.0];

        for (i, (label_idx, achieved, possible)) in radar_categories.iter().copied().enumerate() {
            let sl_row = i as f32 + radar_row_offset;
            let label_local_x = if controller == profile_data::PlayerSide::P1 {
                -160.0
            } else {
                90.0
            };
            let label_local_y = sl_row.mul_add(28.0, 41.0);
            out.push(act!(text: font("miso"): settext(radar_label_text(label_idx)):
                align(1.0, 0.5): xy(labels_frame_origin_x + label_local_x, frame_origin_y + label_local_y): horizalign(right): zoom(0.833): z(101)
            ));

            let possible_clamped = possible.min(999);
            let achieved_clamped = achieved.min(999);
            let achieved_rolling = rolling_number_value(achieved_clamped, elapsed_s);

            let number_local_y = sl_row.mul_add(35.0, 53.0);
            let number_final_y = frame_origin_y + (number_local_y * numbers_frame_zoom);

            // --- Group 1: "Achieved" Numbers (Anchored at -180, separated from Slash) ---
            // Matches Lua: x = { P1=-180 }, aligned right.
            let achieved_anchor_x = (if controller == profile_data::PlayerSide::P1 {
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

                out.push(act!(text: font(machine_font_key(machine_font, FontRole::ScreenEval)): settext(digit_text(digits[digit_idx])):
                    align(1.0, 0.5): xy(x_pos, number_final_y): zoom(final_numbers_zoom):
                    diffuse(color[0], color[1], color[2], color[3]): z(101)
                ));
            }

            // --- Group 2: "Slash + Possible" Numbers (Anchored at -114) ---
            // Matches Lua: x = { P1=-114 }, aligned right.
            let possible_anchor_x = (if controller == profile_data::PlayerSide::P1 {
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

                out.push(act!(text: font(machine_font_key(machine_font, FontRole::ScreenEval)): settext(digit_text(digits[digit_idx])):
                    align(1.0, 0.5): xy(cursor_x, number_final_y): zoom(final_numbers_zoom):
                    diffuse(color[0], color[1], color[2], color[3]): z(101)
                ));
                cursor_x -= digit_width;
            }

            // 2. Draw slash
            // Moved 1px to the right for visual parity
            out.push(act!(text: font(machine_font_key(machine_font, FontRole::ScreenEval)): settext(SLASH_TEXT.clone()):
                align(1.0, 0.5): xy(cursor_x + 0.5, number_final_y): zoom(final_numbers_zoom):
                diffuse(GRAY_POSSIBLE[0], GRAY_POSSIBLE[1], GRAY_POSSIBLE[2], GRAY_POSSIBLE[3]): z(101)
            ));
        }
    }));
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn push_stats_pane_with_palette(
    out: &mut Vec<Actor>,
    presentation: &StatsPanePresentation,
    pane: EvalPane,
    controller: profile_data::PlayerSide,
    asset_manager: &AssetManager,
    elapsed_s: f32,
    machine_font: MachineFont,
    palette: JudgmentPalette,
) {
    if elapsed_s < ROLLING_NUMBERS_APPROACH_SECONDS {
        push_stats_actors(
            out,
            presentation,
            pane,
            controller,
            asset_manager,
            elapsed_s,
            machine_font,
            palette,
        );
        return;
    }

    out.push(Actor::SharedFrame {
        align: [0.0, 0.0],
        offset: [0.0, 0.0],
        size: [SizeSpec::Px(0.0), SizeSpec::Px(0.0)],
        children: presentation.cached_settled(
            pane,
            controller,
            asset_manager,
            machine_font,
            palette,
        ),
        background: None,
        z: 0,
        tint: [1.0; 4],
        blend: None,
    });
}

#[cfg(any(test, feature = "bench-support"))]
#[allow(clippy::too_many_arguments)]
fn build_stats_pane_legacy(
    presentation: &StatsPanePresentation,
    pane: EvalPane,
    controller: profile_data::PlayerSide,
    asset_manager: &AssetManager,
    elapsed_s: f32,
    machine_font: MachineFont,
    palette: JudgmentPalette,
) -> Vec<Actor> {
    let mut actors = Vec::new();
    push_stats_actors(
        &mut actors,
        presentation,
        pane,
        controller,
        asset_manager,
        elapsed_s,
        machine_font,
        palette,
    );
    actors
}

/// Stable old/new fixture for settled judgment/radar actor trees.
#[cfg(any(test, feature = "bench-support"))]
pub struct StatsPaneCacheBenchmark {
    presentation: StatsPanePresentation,
    assets: AssetManager,
}

#[cfg(any(test, feature = "bench-support"))]
impl StatsPaneCacheBenchmark {
    #[must_use]
    pub fn new() -> Self {
        let presentation = StatsPanePresentation {
            judgment_counts: [12_345, 2_345, 345, 45, 5, 1],
            window_counts: WindowCounts {
                w0: 10_000,
                w1: 2_000,
                w2: 300,
                w3: 40,
                w4: 5,
                w5: 1,
                miss: 2,
            },
            window_counts_10ms: WindowCounts {
                w0: 9_000,
                w1: 3_000,
                w2: 300,
                w3: 40,
                w4: 5,
                w5: 1,
                miss: 2,
            },
            disabled_timing_windows: [false; 5],
            radar: [(12, 14), (48, 50), (18, 20), (6, 8)],
            settled: RefCell::new(std::array::from_fn(|_| None)),
        };
        let assets = super::benchmark_asset_manager();
        let _ = presentation.cached_settled(
            EvalPane::HardEx,
            profile_data::PlayerSide::P1,
            &assets,
            MachineFont::Mega,
            JudgmentPalette::default(),
        );
        Self {
            presentation,
            assets,
        }
    }

    #[must_use]
    pub fn legacy_frame(&self, out: &mut Vec<Actor>) -> u64 {
        out.clear();
        out.extend(build_stats_pane_legacy(
            &self.presentation,
            EvalPane::HardEx,
            profile_data::PlayerSide::P1,
            &self.assets,
            ROLLING_NUMBERS_APPROACH_SECONDS,
            MachineFont::Mega,
            JudgmentPalette::default(),
        ));
        actor_tree_checksum(out)
    }

    #[must_use]
    pub fn retained_frame(&self, out: &mut Vec<Actor>) -> u64 {
        out.clear();
        push_stats_pane_with_palette(
            out,
            &self.presentation,
            EvalPane::HardEx,
            profile_data::PlayerSide::P1,
            &self.assets,
            ROLLING_NUMBERS_APPROACH_SECONDS,
            MachineFont::Mega,
            JudgmentPalette::default(),
        );
        actor_tree_checksum(out)
    }
}

#[cfg(any(test, feature = "bench-support"))]
impl Default for StatsPaneCacheBenchmark {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(any(test, feature = "bench-support"))]
fn actor_tree_checksum(actors: &[Actor]) -> u64 {
    let semantic_actors = match actors {
        [Actor::SharedFrame { children, .. }] => children.as_ref(),
        _ => actors,
    };
    let stats = deadlib_present::actors::actor_tree_stats(semantic_actors);
    (u64::from(stats.total) << 32) | u64::from(stats.text_chars)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn radar_hands_row_only_shows_for_single_score_pane() {
        assert!(show_hands_row_for_pane(EvalPane::Standard));
        assert!(!show_hands_row_for_pane(EvalPane::FaPlus));
        assert!(!show_hands_row_for_pane(EvalPane::HardEx));
        assert_eq!(radar_start_index(true), 0);
        assert_eq!(radar_start_index(false), 1);
        assert_eq!(radar_rows_for_pane(true), 4);
        assert_eq!(radar_rows_for_pane(false), 3);
        assert_eq!(radar_row_offset(true), 0.0);
        assert_eq!(radar_row_offset(false), 1.0);
    }

    #[test]
    fn rolling_stats_remain_direct_and_match_legacy() {
        let fixture = StatsPaneCacheBenchmark::new();
        let legacy = build_stats_pane_legacy(
            &fixture.presentation,
            EvalPane::Standard,
            profile_data::PlayerSide::P2,
            &fixture.assets,
            0.5,
            MachineFont::Mega,
            JudgmentPalette::default(),
        );
        let mut direct = Vec::new();
        push_stats_pane_with_palette(
            &mut direct,
            &fixture.presentation,
            EvalPane::Standard,
            profile_data::PlayerSide::P2,
            &fixture.assets,
            0.5,
            MachineFont::Mega,
            JudgmentPalette::default(),
        );
        assert_eq!(format!("{legacy:#?}"), format!("{direct:#?}"));
    }

    #[test]
    fn settled_stats_match_legacy_and_reuse_the_shared_slice() {
        let fixture = StatsPaneCacheBenchmark::new();
        let legacy = build_stats_pane_legacy(
            &fixture.presentation,
            EvalPane::HardEx,
            profile_data::PlayerSide::P1,
            &fixture.assets,
            ROLLING_NUMBERS_APPROACH_SECONDS,
            MachineFont::Mega,
            JudgmentPalette::default(),
        );
        let mut retained = Vec::new();
        push_stats_pane_with_palette(
            &mut retained,
            &fixture.presentation,
            EvalPane::HardEx,
            profile_data::PlayerSide::P1,
            &fixture.assets,
            ROLLING_NUMBERS_APPROACH_SECONDS,
            MachineFont::Mega,
            JudgmentPalette::default(),
        );
        let [
            Actor::SharedFrame {
                children,
                align,
                offset,
                tint,
                blend,
                ..
            },
        ] = retained.as_slice()
        else {
            panic!("expected a settled shared frame");
        };
        assert_eq!(format!("{legacy:#?}"), format!("{children:#?}"));
        assert_eq!(*align, [0.0, 0.0]);
        assert_eq!(*offset, [0.0, 0.0]);
        assert_eq!(*tint, [1.0; 4]);
        assert_eq!(*blend, None);

        let repeated = fixture.presentation.cached_settled(
            EvalPane::HardEx,
            profile_data::PlayerSide::P1,
            &fixture.assets,
            MachineFont::Mega,
            JudgmentPalette::default(),
        );
        assert!(Arc::ptr_eq(children, &repeated));
    }
}
