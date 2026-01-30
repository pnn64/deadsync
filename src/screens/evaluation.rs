use crate::act;
use crate::core::gfx::{BlendMode, MeshMode, MeshVertex};
use crate::core::space::widescale;
use crate::core::space::{screen_center_x, screen_center_y, screen_height, screen_width};
use crate::screens::Screen;
use crate::ui::actors::{Actor, SizeSpec};
use crate::ui::color;
use crate::ui::components::screen_bar::{
    AvatarParams, ScreenBarParams, ScreenBarPosition, ScreenBarTitlePlacement,
};
use crate::ui::components::{eval_grades, heart_bg, pad_display, screen_bar};

use crate::assets::AssetManager;
use crate::game::chart::ChartData;
use crate::game::judgment::{self, JudgeGrade};
use crate::game::scores;
use crate::game::scroll::ScrollSpeedSetting;
use crate::game::song::SongData;
use crate::game::timing as timing_stats;
use crate::screens::gameplay;
use crate::ui::font;
use std::collections::HashMap;
use std::sync::{Arc, LazyLock};

use crate::core::input::{InputEvent, VirtualAction};
use crate::game::profile;
use crate::screens::ScreenAction;
// Keyboard handling is centralized in app.rs via virtual actions
use chrono::Local;

/* ---------------------------- transitions ---------------------------- */
const TRANSITION_IN_DURATION: f32 = 0.4;
const TRANSITION_OUT_DURATION: f32 = 0.4;

// A struct to hold a snapshot of the final score data from the gameplay screen.
#[derive(Clone)]
pub struct ScoreInfo {
    pub song: Arc<SongData>,
    pub chart: Arc<ChartData>,
    pub judgment_counts: HashMap<JudgeGrade, u32>,
    pub score_percent: f64,
    pub grade: scores::Grade,
    pub speed_mod: ScrollSpeedSetting,
    pub hands_achieved: u32,
    pub holds_held: u32,
    pub holds_total: u32,
    pub rolls_held: u32,
    pub rolls_total: u32,
    pub mines_avoided: u32,
    pub mines_total: u32,
    // Aggregate timing stats for non-miss tap judgments
    pub timing: timing_stats::TimingStats,
    // Prepared scatter plot points (time, offset), like Simply Love
    pub scatter: Vec<timing_stats::ScatterPoint>,
    // Worst window used to scale scatter (at least W2), like Simply Love ScatterPlot.lua
    pub scatter_worst_window_ms: f32,
    // Prepared histogram in 1ms bins
    pub histogram: timing_stats::HistogramMs,
    // Time range used to scale scatter/NPS graph (FirstSecond..LastSecond)
    pub graph_first_second: f32,
    pub graph_last_second: f32,
    pub music_rate: f32,
    pub scroll_option: crate::game::profile::ScrollOption,
    pub life_history: Vec<(f32, f32)>,
    pub fail_time: Option<f32>,
    // Per-window tap counts (including FA+ W0) for display purposes.
    pub window_counts: timing_stats::WindowCounts,
    // FA+ style EX score percentage (0.00–100.00), using the same semantics
    // as ScreenGameplay's EX HUD (Simply Love's CalculateExScore).
    pub ex_score_percent: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EvalPane {
    Standard,
    FaPlus,
}

impl EvalPane {
    #[inline(always)]
    fn default_from_profile() -> Self {
        if profile::get().show_fa_plus_pane {
            Self::FaPlus
        } else {
            Self::Standard
        }
    }

    #[inline(always)]
    const fn toggle(self) -> Self {
        match self {
            Self::Standard => Self::FaPlus,
            Self::FaPlus => Self::Standard,
        }
    }
}

pub struct State {
    pub active_color_index: i32,
    bg: heart_bg::State,
    pub session_elapsed: f32, // To display the timer
    pub score_info: Option<ScoreInfo>,
    pub density_graph_mesh: Option<Arc<[MeshVertex]>>,
    pub timing_hist_mesh: Option<Arc<[MeshVertex]>>,
    pub scatter_mesh: Option<Arc<[MeshVertex]>>,
    pub density_graph_texture_key: String,
    active_pane: EvalPane,
}

pub fn init(gameplay_results: Option<gameplay::State>) -> State {
    let score_info = gameplay_results.map(|gs| {
        let player_idx = 0;
        let (start, end) = gs.note_ranges[player_idx];
        let notes = &gs.notes[start..end];
        let note_times = &gs.note_time_cache[start..end];
        let hold_end_times = &gs.hold_end_time_cache[start..end];
        let p = &gs.players[player_idx];

        // Compute timing statistics across all non-miss tap judgments
        let stats = timing_stats::compute_note_timing_stats(notes);
        // Prepare scatter points and histogram bins
        let scatter = timing_stats::build_scatter_points(notes, note_times);
        let histogram = timing_stats::build_histogram_ms(notes);
        let scatter_worst_window_ms = {
            let tw = timing_stats::effective_windows_ms();
            let abs = histogram.worst_observed_ms.max(0.0);
            let mut idx: usize = if abs <= tw[0] {
                1
            } else if abs <= tw[1] {
                2
            } else if abs <= tw[2] {
                3
            } else if abs <= tw[3] {
                4
            } else {
                5
            };
            idx = idx.max(2);
            tw[idx - 1]
        };
        let graph_first_second = 0.0_f32.min(gs.timing.get_time_for_beat(0.0));
        let graph_last_second = gs.song.total_length_seconds as f32;

        let score_percent = judgment::calculate_itg_score_percent(
            &p.scoring_counts,
            p.holds_held_for_score,
            p.rolls_held_for_score,
            p.mines_hit_for_score,
            gs.possible_grade_points[player_idx],
        );

        let mut grade = if p.is_failing || !gs.song_completed_naturally {
            scores::Grade::Failed
        } else {
            scores::score_to_grade(score_percent * 10000.0)
        };

        // Per-window counts for the FA+ pane should always reflect all tap
        // judgments that occurred (including after failure), matching the
        // standard pane's judgment_counts semantics.
        let window_counts = timing_stats::compute_window_counts(notes);

        // NoMines handling is not wired yet, so treat mines as enabled.
        let mines_disabled = false;
        let ex_score_percent = judgment::calculate_ex_score_from_notes(
            notes,
            note_times,
            hold_end_times,
            gs.charts[player_idx].stats.total_steps,
            gs.holds_total[player_idx],
            gs.rolls_total[player_idx],
            gs.mines_total[player_idx],
            p.fail_time,
            mines_disabled,
        );

        // Simply Love: show Quint (Grade_Tier00) if EX score is exactly 100.00.
        if grade != scores::Grade::Failed && ex_score_percent >= 100.0 {
            grade = scores::Grade::Quint;
        }

        ScoreInfo {
            song: gs.song.clone(),
            chart: gs.charts[player_idx].clone(),
            judgment_counts: p.judgment_counts.clone(),
            score_percent,
            grade,
            speed_mod: gs.scroll_speed[0],
            hands_achieved: p.hands_achieved,
            holds_held: p.holds_held,
            holds_total: gs.holds_total[player_idx],
            rolls_held: p.rolls_held,
            rolls_total: gs.rolls_total[player_idx],
            mines_avoided: p.mines_avoided,
            mines_total: gs.mines_total[player_idx],
            timing: stats,
            scatter,
            scatter_worst_window_ms,
            histogram,
            graph_first_second,
            graph_last_second,
            music_rate: if gs.music_rate.is_finite() && gs.music_rate > 0.0 {
                gs.music_rate
            } else {
                1.0
            },
            scroll_option: profile::get().scroll_option,
            life_history: p.life_history.clone(),
            fail_time: p.fail_time,
            window_counts,
            ex_score_percent,
        }
    });

    let density_graph_mesh = score_info.as_ref().and_then(|si| {
        const GRAPH_W: f32 = 610.0;
        const GRAPH_H: f32 = 64.0;

        let last_second = si.song.total_length_seconds.max(0) as f32;
        let verts = crate::ui::density_graph::build_density_histogram_mesh(
            &si.chart.measure_nps_vec,
            si.chart.max_nps,
            &si.chart.timing,
            si.graph_first_second,
            last_second,
            GRAPH_W,
            GRAPH_H,
            0.0,
            GRAPH_W,
            Some(0.5),
            0.5,
        );
        if verts.is_empty() {
            None
        } else {
            Some(Arc::from(verts.into_boxed_slice()))
        }
    });

    let timing_hist_mesh = score_info.as_ref().and_then(|si| {
        const PANE_W: f32 = 300.0;
        const PANE_H: f32 = 180.0;
        const TOP_H: f32 = 26.0;
        const BOT_H: f32 = 13.0;

        let graph_h = (PANE_H - TOP_H - BOT_H).max(0.0);
        let verts = crate::ui::eval_graphs::build_offset_histogram_mesh(
            &si.histogram,
            PANE_W,
            graph_h,
            PANE_H,
            crate::config::get().smooth_histogram,
        );
        if verts.is_empty() {
            None
        } else {
            Some(Arc::from(verts.into_boxed_slice()))
        }
    });

    let scatter_mesh = score_info.as_ref().and_then(|si| {
        const GRAPH_W: f32 = 610.0;
        const GRAPH_H: f32 = 64.0;

        let verts = crate::ui::eval_graphs::build_scatter_mesh(
            &si.scatter,
            si.graph_first_second,
            si.graph_last_second,
            GRAPH_W,
            GRAPH_H,
            si.scatter_worst_window_ms,
        );
        if verts.is_empty() {
            None
        } else {
            Some(Arc::from(verts.into_boxed_slice()))
        }
    });

    State {
        active_color_index: color::DEFAULT_COLOR_INDEX, // This will be overwritten by app.rs
        bg: heart_bg::State::new(),
        session_elapsed: 0.0,
        score_info,
        density_graph_mesh,
        timing_hist_mesh,
        scatter_mesh,
        density_graph_texture_key: "__white".to_string(),
        active_pane: EvalPane::default_from_profile(),
    }
}

// Keyboard input is handled centrally via the virtual dispatcher in app.rs

// This screen doesn't have any dynamic state updates yet, but we keep the function for consistency.
pub const fn update(_state: &mut State, _dt: f32) {
    //
}

pub fn in_transition() -> (Vec<Actor>, f32) {
    let actor = act!(quad:
        align(0.0, 0.0): xy(0.0, 0.0):
        zoomto(screen_width(), screen_height()):
        diffuse(0.0, 0.0, 0.0, 1.0): z(1100):
        linear(TRANSITION_IN_DURATION): alpha(0.0):
        linear(0.0): visible(false)
    );
    (vec![actor], TRANSITION_IN_DURATION)
}

pub fn out_transition() -> (Vec<Actor>, f32) {
    let actor = act!(quad:
        align(0.0, 0.0): xy(0.0, 0.0):
        zoomto(screen_width(), screen_height()):
        diffuse(0.0, 0.0, 0.0, 0.0):
        z(1200):
        linear(TRANSITION_OUT_DURATION): alpha(1.0)
    );
    (vec![actor], TRANSITION_OUT_DURATION)
}

fn format_session_time(seconds_total: f32) -> String {
    if seconds_total < 0.0 {
        return "00:00".to_string();
    }
    let seconds_total = seconds_total as u64;

    let hours = seconds_total / 3600;
    let minutes = (seconds_total % 3600) / 60;
    let seconds = seconds_total % 60;

    if hours > 0 {
        format!("{hours}:{minutes:02}:{seconds:02}")
    } else {
        format!("{minutes:02}:{seconds:02}")
    }
}

pub fn handle_input(state: &mut State, ev: &InputEvent) -> ScreenAction {
    if !ev.pressed {
        return ScreenAction::None;
    }

    match ev.action {
        VirtualAction::p1_back | VirtualAction::p1_start => {
            ScreenAction::Navigate(Screen::SelectMusic)
        }
        VirtualAction::p1_left
        | VirtualAction::p1_menu_left
        | VirtualAction::p1_right
        | VirtualAction::p1_menu_right => {
            if state.score_info.is_some() {
                state.active_pane = state.active_pane.toggle();
            }
            ScreenAction::None
        }
        _ => ScreenAction::None,
    }
}

// --- Statics and helper function for the P1 stats pane ---

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

/// Builds the entire P1 (left side) stats pane including judgments and radar counts.
fn build_p1_stats_pane(state: &State, asset_manager: &AssetManager) -> Vec<Actor> {
    let Some(score_info) = &state.score_info else {
        return vec![];
    };
    let mut actors = Vec::new();
    let cy = screen_center_y();

    // The base offset for all P1 panes from the screen center.
    let p1_side_offset = screen_center_x() - 155.0;

    // Active evaluation pane is chosen at runtime; the profile toggle
    // only selects which pane is shown first.
    let show_fa_plus_pane = matches!(state.active_pane, EvalPane::FaPlus);

    // --- Calculate label shift for large numbers ---
    let max_judgment_count = if !show_fa_plus_pane {
        JUDGMENT_ORDER
            .iter()
            .map(|grade| score_info.judgment_counts.get(grade).copied().unwrap_or(0))
            .max()
            .unwrap_or(0)
    } else {
        let wc = score_info.window_counts;
        *[wc.w0, wc.w1, wc.w2, wc.w3, wc.w4, wc.w5, wc.miss]
            .iter()
            .max()
            .unwrap_or(&0)
    };

    let (label_shift_x, label_zoom) = if max_judgment_count > 9999 {
        let length = (max_judgment_count as f32).log10().floor() as i32 + 1;
        (
            -11.0 * (length - 4) as f32,
            0.1f32.mul_add(-((length - 4) as f32), 0.833),
        )
    } else {
        (0.0, 0.833)
    };

    let digits_needed = if max_judgment_count == 0 {
        1
    } else {
        (max_judgment_count as f32).log10().floor() as usize + 1
    };
    let digits_to_fmt = digits_needed.max(4);

    asset_manager.with_fonts(|all_fonts| asset_manager.with_font("wendy_screenevaluation", |metrics_font| {
        let numbers_frame_zoom = 0.8;
        let final_numbers_zoom = numbers_frame_zoom * 0.5;
        let digit_width = font::measure_line_width_logical(metrics_font, "0", all_fonts) as f32 * final_numbers_zoom;
        if digit_width <= 0.0 { return; }

        // --- Judgment Labels & Numbers ---
        let labels_frame_origin_x = p1_side_offset + 50.0;
        let numbers_frame_origin_x = p1_side_offset + 90.0;
        let frame_origin_y = cy - 24.0;

        if !show_fa_plus_pane {
            for (i, grade) in JUDGMENT_ORDER.iter().enumerate() {
                let info = JUDGMENT_INFO.get(grade).unwrap();
                let count = score_info.judgment_counts.get(grade).copied().unwrap_or(0);

                // Label
                let label_local_x = 28.0 + label_shift_x;
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

                let number_local_x = 64.0;
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
	            let wc = score_info.window_counts;
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
                // Label: match Simply Love Pane2 labels using 26px spacing.
                // Original Lua uses 1-based indexing: y = i*26 - 46.
                // Our rows are 0-based, so use (i+1) here.
                let label_local_x = 28.0 + label_shift_x;
                let label_local_y = (i as f32 + 1.0).mul_add(26.0, -46.0);
                actors.push(act!(text: font("miso"): settext(label.to_string()):
                    align(1.0, 0.5): xy(labels_frame_origin_x + label_local_x, frame_origin_y + label_local_y):
                    maxwidth(76.0): zoom(label_zoom): horizalign(right):
                    diffuse(bright_color[0], bright_color[1], bright_color[2], bright_color[3]): z(101)
                ));

                // Number
                let number_str = format!("{count:0digits_to_fmt$}");
                let first_nonzero = number_str.find(|c: char| c != '0').unwrap_or(number_str.len());

                // Numbers: match Simply Love Pane2 numbers using 32px spacing.
                let number_local_x = 64.0;
                let number_local_y = (i as f32).mul_add(32.0, -24.0);
                let number_final_y = frame_origin_y + (number_local_y * numbers_frame_zoom);
                let number_base_x = numbers_frame_origin_x + (number_local_x * numbers_frame_zoom);

                for (char_idx, ch) in number_str.chars().enumerate() {
                    let is_dim = if *count == 0 { char_idx < digits_to_fmt - 1 } else { char_idx < first_nonzero };
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
            let label_local_x = -160.0;
            let label_local_y = (i as f32).mul_add(28.0, 41.0);
            actors.push(act!(text: font("miso"): settext(label.to_string()):
                align(1.0, 0.5): xy(labels_frame_origin_x + label_local_x, frame_origin_y + label_local_y): horizalign(right): zoom(0.833): z(101)
            ));

            let possible_clamped = possible.min(999);
            let achieved_clamped = achieved.min(999);

            let number_local_y = (i as f32).mul_add(35.0, 53.0);
            let number_final_y = frame_origin_y + (number_local_y * numbers_frame_zoom);

            // --- Group 1: "Achieved" Numbers (Anchored at -180, separated from Slash) ---
            // Matches Lua: x = { P1=-180 }, aligned right.
            let achieved_anchor_x = (-180.0f32).mul_add(numbers_frame_zoom, numbers_frame_origin_x);

            let achieved_str = format!("{achieved_clamped:03}");
            let first_nonzero_achieved = achieved_str.find(|c: char| c != '0').unwrap_or(achieved_str.len());

            for (char_idx_from_right, ch) in achieved_str.chars().rev().enumerate() {
                let is_dim = if achieved == 0 {
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
            let possible_anchor_x = (-114.0f32).mul_add(numbers_frame_zoom, numbers_frame_origin_x);
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

/// Builds the timing statistics pane for P2 (or P1 in single player).
fn build_p2_timing_pane(state: &State) -> Vec<Actor> {
    let pane_width: f32 = 300.0;
    let pane_height: f32 = 180.0;
    let topbar_height: f32 = 26.0;
    let bottombar_height: f32 = 13.0;

    let frame_x = screen_center_x() + 5.0;
    let frame_y = screen_center_y() - 56.0;

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
    if let Some(mesh) = &state.timing_hist_mesh
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

    let max_error_text = state.score_info.as_ref().map_or_else(
        || "0.0ms".to_string(),
        |s| format!("{:.1}ms", s.timing.max_abs_ms),
    );

    let stats = state.score_info.as_ref();
    let mean_abs_text = stats.map_or_else(
        || "0.0ms".to_string(),
        |s| format!("{:.1}ms", s.timing.mean_abs_ms),
    );
    let mean_text = stats.map_or_else(
        || "0.0ms".to_string(),
        |s| format!("{:.1}ms", s.timing.mean_ms),
    );
    let stddev3_text = stats.map_or_else(
        || "0.0ms".to_string(),
        |s| format!("{:.1}ms", s.timing.stddev_ms * 3.0),
    );

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

/// Builds the modifiers display pane for P1.
fn build_modifiers_pane(state: &State) -> Vec<Actor> {
    // These positions are derived from the original ActorFrame layout to place
    // the text in the exact same world-space position without the frame.
    let p1_side_offset = screen_center_x() - 155.0;
    let frame_center_y = screen_center_y() + 200.5;
    let font_zoom = 0.7;

    // The text's top-left corner was positioned at xy(-140, -5) relative to the
    // frame's center. We now calculate that absolute position directly.
    let text_x = p1_side_offset - 140.0;
    let text_y = frame_center_y - 5.0;

    // The original large background pane is at z=100. This text needs to be on top.
    let text_z = 101;

    // Get the speed mod and scroll perspective from score info.
    let score_info = state.score_info.as_ref().unwrap();
    let speed_mod_text = score_info.speed_mod.to_string();
    let mut parts = Vec::new();
    parts.push(speed_mod_text);
    // Show active scroll modifiers in a fixed order, matching Simply Love's
    // preference for listing Reverse before the perspective.
    let scroll = score_info.scroll_option;
    if scroll.contains(profile::ScrollOption::Reverse) {
        parts.push("Reverse".to_string());
    }
    if scroll.contains(profile::ScrollOption::Split) {
        parts.push("Split".to_string());
    }
    if scroll.contains(profile::ScrollOption::Alternate) {
        parts.push("Alternate".to_string());
    }
    if scroll.contains(profile::ScrollOption::Cross) {
        parts.push("Cross".to_string());
    }
    if scroll.contains(profile::ScrollOption::Centered) {
        parts.push("Centered".to_string());
    }
    parts.push("Overhead".to_string());
    let final_text = parts.join(", ");

    let modifier_text = act!(text:
        font("miso"):
        settext(final_text):
        align(0.0, 0.0):
        xy(text_x, text_y):
        zoom(font_zoom):
        z(text_z):
        diffuse(1.0, 1.0, 1.0, 1.0)
    );

    vec![modifier_text]
}

pub fn get_actors(state: &State, asset_manager: &AssetManager) -> Vec<Actor> {
    let mut actors = Vec::with_capacity(20);

    // 1. Background
    actors.extend(state.bg.build(heart_bg::Params {
        active_color_index: state.active_color_index,
        backdrop_rgba: [0.0, 0.0, 0.0, 1.0],
        alpha_mul: 1.0,
    }));

    // 2. Top Bar
    actors.push(screen_bar::build(ScreenBarParams {
        title: "EVALUATION",
        title_placement: ScreenBarTitlePlacement::Left,
        position: ScreenBarPosition::Top,
        transparent: false,
        fg_color: [1.0; 4],
        left_text: None,
        center_text: None,
        right_text: None,
        left_avatar: None,
        right_avatar: None,
    }));

    // Session Timer
    let timer_text = format_session_time(state.session_elapsed);
    actors.push(act!(text:
        font("wendy_monospace_numbers"):
        settext(timer_text):
        align(0.5, 0.5):
        xy(screen_center_x(), 10.0):
        zoom(widescale(0.3, 0.36)):
        z(121):
        diffuse(1.0, 1.0, 1.0, 1.0):
        horizalign(center)
    ));

    let Some(score_info) = &state.score_info else {
        actors.push(act!(text:
            font("wendy"):
            settext("NO SCORE DATA AVAILABLE"):
            align(0.5, 0.5): xy(screen_center_x(), screen_center_y()):
            zoom(0.8): horizalign(center):
            z(100)
        ));
        return actors;
    };

    // --- Lower Stats Pane Background ---
    {
        let pane_width = 300.0f64.mul_add(2.0, 10.0);
        let pane_x_left = screen_center_x() - 305.0;
        let pane_y_top = screen_center_y() - 56.0;
        let pane_y_bottom = (screen_center_y() + 34.0) + 180.0;
        let pane_height = pane_y_bottom - pane_y_top;
        let pane_bg_color = color::rgba_hex("#1E282F");

        actors.push(act!(quad:
            align(0.0, 0.0):
            xy(pane_x_left, pane_y_top):
            zoomto(pane_width, pane_height):
            diffuse(pane_bg_color[0], pane_bg_color[1], pane_bg_color[2], 1.0):
            z(100)
        ));
    }

    let cy = screen_center_y();

    // --- Title, Banner, and Song Features (Center Column) ---
    {
        // --- TitleAndBanner Group ---
        let banner_key = score_info
            .song
            .banner_path
            .as_ref()
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_else(|| {
                let banner_num = state.active_color_index.rem_euclid(12) + 1;
                format!("banner{banner_num}.png")
            });

        let full_title = score_info
            .song
            .display_full_title(crate::config::get().translated_titles);

        let title_and_banner_frame = Actor::Frame {
            align: [0.5, 0.5],
            offset: [screen_center_x(), 46.0],
            size: [SizeSpec::Px(0.0), SizeSpec::Px(0.0)],
            children: vec![
                act!(sprite(banner_key): align(0.5, 0.5): xy(0.0, 66.0): setsize(418.0, 164.0): zoom(0.7): z(0)),
                act!(quad: align(0.5, 0.5): xy(0.0, 0.0): setsize(418.0, 25.0): zoom(0.7): diffuse(0.117, 0.157, 0.184, 1.0): z(1)),
                act!(text: font("miso"): settext(full_title): align(0.5, 0.5): xy(0.0, 0.0): maxwidth(418.0 * 0.7): z(2)),
            ],
            background: None,
            z: 50,
        };
        actors.push(title_and_banner_frame);

        // --- SongFeatures Group ---
        let bpm_text = {
            let rate_f64 = f64::from(score_info.music_rate);
            let min = (score_info.song.min_bpm * rate_f64).round() as i32;
            let max = (score_info.song.max_bpm * rate_f64).round() as i32;
            let base = if (score_info.song.min_bpm - score_info.song.max_bpm).abs() < 1e-6 {
                format!("{min} bpm")
            } else {
                format!("{min} - {max} bpm")
            };
            if (score_info.music_rate - 1.0).abs() > 0.001 {
                format!(
                    "{} ({}x Music Rate)",
                    base,
                    format!("{:.2}", score_info.music_rate)
                )
            } else {
                base
            }
        };

        let length_text = {
            // Simply Love uses Song:MusicLengthSeconds() divided by MusicRate
            // for this display, not the chart's last note time.
            let base_seconds = if score_info.song.music_length_seconds.is_finite()
                && score_info.song.music_length_seconds > 0.0
            {
                score_info.song.music_length_seconds
            } else {
                score_info.song.total_length_seconds.max(0) as f32
            };
            let rate = if score_info.music_rate.is_finite() && score_info.music_rate > 0.0 {
                score_info.music_rate
            } else {
                1.0
            };
            let adjusted = base_seconds / rate;
            let seconds = adjusted.round() as i32;
            if seconds < 0 {
                String::new()
            } else if seconds >= 3600 {
                format!(
                    "{}:{:02}:{:02}",
                    seconds / 3600,
                    (seconds % 3600) / 60,
                    seconds % 60
                )
            } else {
                format!("{}:{:02}", seconds / 60, seconds % 60)
            }
        };

        let song_features_frame = Actor::Frame {
            align: [0.5, 0.5],
            offset: [screen_center_x(), 175.0],
            size: [SizeSpec::Px(0.0), SizeSpec::Px(0.0)],
            children: vec![
                act!(quad: align(0.5, 0.5): xy(0.0, 0.0): setsize(418.0, 16.0): zoom(0.7): diffuse(0.117, 0.157, 0.184, 1.0): z(0) ),
                act!(text: font("miso"): settext(score_info.song.artist.clone()): align(0.0, 0.5): xy(-145.0, 0.0): zoom(0.6): maxwidth(418.0 / 3.5): z(1) ),
                act!(text: font("miso"): settext(bpm_text): align(0.5, 0.5): xy(0.0, 0.0): zoom(0.6): maxwidth(418.0 / 0.875): z(1) ),
                act!(text: font("miso"): settext(length_text): align(1.0, 0.5): xy(145.0, 0.0): zoom(0.6): z(1) ),
            ],
            background: None,
            z: 50,
        };
        actors.push(song_features_frame);
    }

    // --- Player 1 Upper Content Frame ---
    let p1_frame_x = screen_center_x() - 155.0;

    // Letter Grade (Simply Love parity)
    actors.extend(eval_grades::actors(
        score_info.grade,
        eval_grades::EvalGradeParams {
            x: p1_frame_x - 70.0,
            y: cy - 134.0,
            z: 101,
            zoom: 0.4,
            elapsed: state.session_elapsed,
        },
    ));

    // Difficulty Text and Meter Block
    {
        let difficulty_display_name = if score_info.chart.difficulty.eq_ignore_ascii_case("edit") {
            "Edit"
        } else {
            let difficulty_index = color::FILE_DIFFICULTY_NAMES
                .iter()
                .position(|&n| n.eq_ignore_ascii_case(&score_info.chart.difficulty))
                .unwrap_or(2);
            color::DISPLAY_DIFFICULTY_NAMES[difficulty_index]
        };

        let difficulty_color =
            color::difficulty_rgba(&score_info.chart.difficulty, state.active_color_index);
        let difficulty_text = format!("Single / {difficulty_display_name}");
        actors.push(act!(text: font("miso"): settext(difficulty_text): align(0.0, 0.5): xy(p1_frame_x - 115.0, cy - 65.0): zoom(0.7): z(101): diffuse(1.0, 1.0, 1.0, 1.0) ));
        actors.push(act!(quad: align(0.5, 0.5): xy(p1_frame_x - 134.5, cy - 71.0): zoomto(30.0, 30.0): z(101): diffuse(difficulty_color[0], difficulty_color[1], difficulty_color[2], 1.0) ));
        actors.push(act!(text: font("wendy"): settext(score_info.chart.meter.to_string()): align(0.5, 0.5): xy(p1_frame_x - 134.5, cy - 71.0): zoom(0.4): z(102): diffuse(0.0, 0.0, 0.0, 1.0) ));
    }

    // Step Artist (or Edit description)
    let step_artist_text = if score_info.chart.difficulty.eq_ignore_ascii_case("edit")
        && !score_info.chart.description.trim().is_empty()
    {
        score_info.chart.description.clone()
    } else {
        score_info.chart.step_artist.clone()
    };
    actors.push(act!(text: font("miso"): settext(step_artist_text): align(0.0, 0.5): xy(p1_frame_x - 115.0, cy - 81.0): zoom(0.7): z(101): diffuse(1.0, 1.0, 1.0, 1.0) ));

    // --- Breakdown Text (under grade) ---
    let breakdown_text = {
        let chart = &score_info.chart;
        // Match the Lua script by progressively minimizing the breakdown text until it fits.
        asset_manager
            .with_fonts(|all_fonts| {
                asset_manager.with_font("miso", |miso_font| -> Option<String> {
                    let width_constraint = 155.0;
                    let text_zoom = 0.7;
                    // Measure at logical width (zoom 1.0) and ensure it fits once scaled down.
                    let max_allowed_logical_width = width_constraint / text_zoom;

                    let fits = |text: &str| {
                        let logical_width =
                            font::measure_line_width_logical(miso_font, text, all_fonts) as f32;
                        logical_width <= max_allowed_logical_width
                    };

                    if fits(&chart.detailed_breakdown) {
                        Some(chart.detailed_breakdown.clone())
                    } else if fits(&chart.partial_breakdown) {
                        Some(chart.partial_breakdown.clone())
                    } else if fits(&chart.simple_breakdown) {
                        Some(chart.simple_breakdown.clone())
                    } else {
                        Some(format!("{} Total", chart.total_streams))
                    }
                })
            })
            .flatten()
            .unwrap_or_else(|| chart.simple_breakdown.clone()) // Fallback if font isn't found
    };

    // Position based on P1, left-aligned. The y-value is from the original theme.
    actors.push(act!(text: font("miso"): settext(breakdown_text):
        align(0.0, 0.5): xy(p1_frame_x - 150.0, cy - 95.0): zoom(0.7):
        maxwidth(155.0): horizalign(left): z(101): diffuse(1.0, 1.0, 1.0, 1.0)
    ));

    // --- Player 1 Score Percentage Display ---
    {
        let score_frame_y = screen_center_y() - 26.0;
        let percent_text = format!("{:.2}", score_info.score_percent * 100.0);
        let ex_percent_text = format!("{:.2}", score_info.ex_score_percent.max(0.0));
        let score_bg_color = color::rgba_hex("#101519");
        let show_fa_plus_pane = matches!(state.active_pane, EvalPane::FaPlus);

        let mut children = Vec::new();

        if show_fa_plus_pane {
            // FA+ pane: stretch the background down (height 88, y-offset 14)
            // to match Simply Love's Pane2 percentage container, and always
            // show EX score beneath the normal ITG percent (independent of the
            // in-game EX HUD option).
            children.push(act!(quad:
                align(0.0, 0.5):
                xy(-150.0, 14.0):
                setsize(158.5, 88.0):
                diffuse(score_bg_color[0], score_bg_color[1], score_bg_color[2], 1.0)
            ));

            // Normal ITG score (top line, white)
            children.push(act!(text:
                font("wendy_white"):
                settext(percent_text):
                align(1.0, 0.5):
                // Keep ITG percent in the same position regardless of FA+ pane.
                xy(1.5, 0.0):
                zoom(0.585):
                horizalign(right)
            ));

            // EX score (bottom line, Fantastic blue / turquoise), smaller than ITG score
            let ex_color = color::JUDGMENT_RGBA[0];
            // "EX" label to the left of the numeric EX score.
            children.push(act!(text:
                font("wendy_white"):
                settext("EX"):
                align(1.0, 0.5):
                // Near the left edge of the background box.
                xy(-108.0, 40.0):
                zoom(0.31):
                horizalign(right):
                diffuse(ex_color[0], ex_color[1], ex_color[2], ex_color[3])
            ));
            children.push(act!(text:
                font("wendy_white"):
                settext(ex_percent_text):
                align(1.0, 0.5):
                // EX numeric value aligned with label, further below ITG percent.
                xy(0, 40.0):
                zoom(0.31):
                horizalign(right):
                diffuse(ex_color[0], ex_color[1], ex_color[2], ex_color[3])
            ));
        } else {
            // Standard pane: original 60px-tall background and single ITG percent.
            children.push(act!(quad:
                align(0.0, 0.5):
                xy(-150.0, 0.0):
                setsize(158.5, 60.0):
                diffuse(score_bg_color[0], score_bg_color[1], score_bg_color[2], 1.0)
            ));
            children.push(act!(text:
                font("wendy_white"):
                settext(percent_text):
                align(1.0, 0.5):
                xy(1.5, 0.0):
                zoom(0.585):
                horizalign(right)
            ));
        }

        let score_display_frame = Actor::Frame {
            align: [0.5, 0.5],
            offset: [p1_frame_x, score_frame_y],
            size: [SizeSpec::Px(0.0), SizeSpec::Px(0.0)],
            background: None,
            // Draw above the judgment/radar pane (z≈101) so the stretched
            // background cleanly covers the top radar row when FA+ pane is used.
            z: 102,
            children,
        };
        actors.push(score_display_frame);
    }

    // --- P1 Stats Pane (Judgments & Radar) ---
    actors.extend(build_p1_stats_pane(state, asset_manager));

    // --- P2 Timing Pane (repurposed for single player) ---
    actors.extend(build_p2_timing_pane(state));

    // --- NEW: P1 Modifiers Pane ---
    actors.extend(build_modifiers_pane(state));

    // --- DENSITY GRAPH PANE (Corrected Layout) ---
    {
        const GRAPH_WIDTH: f32 = 610.0;
        const GRAPH_HEIGHT: f32 = 64.0;

        let frame_center_x = screen_center_x();
        let frame_center_y = screen_center_y() + 124.0;

        let graph_frame = Actor::Frame {
            align: [0.5, 0.0], // Center-Top alignment for the main frame
            offset: [frame_center_x, frame_center_y],
            size: [SizeSpec::Px(GRAPH_WIDTH), SizeSpec::Px(GRAPH_HEIGHT)],
            z: 101,
            background: None,
            children: vec![
                act!(quad:
                    align(0.0, 0.0):
                    xy(0.0, 0.0):
                    setsize(GRAPH_WIDTH, GRAPH_HEIGHT):
                    diffuse(16.0/255.0, 21.0/255.0, 25.0/255.0, 1.0):
                    z(0)
                ),
                // The NPS histogram is positioned with its origin at the bottom-left of the frame,
                // and then shifted to be centered horizontally.
                // Lua: `addx(-GraphWidth/2):addy(GraphHeight)`
                // This is equivalent to `align(0.0, 1.0)` (bottom-left) and `xy` at the center of the frame.
                {
                    if let Some(mesh) = &state.density_graph_mesh
                        && !mesh.is_empty()
                    {
                        Actor::Mesh {
                            align: [0.0, 1.0],
                            offset: [0.0, GRAPH_HEIGHT],
                            size: [SizeSpec::Px(GRAPH_WIDTH), SizeSpec::Px(GRAPH_HEIGHT)],
                            vertices: mesh.clone(),
                            mode: MeshMode::Triangles,
                            visible: true,
                            blend: BlendMode::Alpha,
                            z: 1,
                        }
                    } else if state.density_graph_texture_key != "__white" {
                        act!(sprite(state.density_graph_texture_key.clone()):
                            align(0.0, 1.0): // bottom-left
                            xy(0.0, GRAPH_HEIGHT): // position at the bottom-left of the frame
                            setsize(GRAPH_WIDTH, GRAPH_HEIGHT): z(1)
                        )
                    } else {
                        act!(sprite("__white"): visible(false))
                    }
                },
                // The horizontal zero-line, centered vertically in the panel.
                act!(quad:
                    align(0.5, 0.5):
                    xy(GRAPH_WIDTH / 2.0_f32, GRAPH_HEIGHT / 2.0_f32):
                    setsize(GRAPH_WIDTH, 1.0):
                    diffusealpha(0.1):
                    z(2)
                ),
                // Scatter plot overlay (judgment offsets over time)
                {
                    if let Some(mesh) = &state.scatter_mesh
                        && !mesh.is_empty()
                    {
                        Actor::Mesh {
                            align: [0.0, 0.0],
                            offset: [0.0, 0.0],
                            size: [SizeSpec::Px(GRAPH_WIDTH), SizeSpec::Px(GRAPH_HEIGHT)],
                            vertices: mesh.clone(),
                            mode: MeshMode::Triangles,
                            visible: true,
                            blend: BlendMode::Alpha,
                            z: 3,
                        }
                    } else {
                        Actor::Frame {
                            align: [0.0, 0.0],
                            offset: [0.0, 0.0],
                            size: [SizeSpec::Px(GRAPH_WIDTH), SizeSpec::Px(GRAPH_HEIGHT)],
                            background: None,
                            z: 3,
                            children: Vec::new(),
                        }
                    }
                },
                // Life Line Overlay (z=4)
                {
                    let mut life_children: Vec<Actor> = Vec::new();
                    if let Some(si) = &state.score_info {
                        let first = si.graph_first_second;
                        let last = si.graph_last_second.max(first + 0.001_f32);
                        let dur = (last - first).max(0.001_f32);
                        let padding = 0.05; // Same padding as scatter

                        let mut last_x = -999.0_f32;
                        let mut last_y = -999.0_f32;

                        for &(t, life) in &si.life_history {
                            let x = ((t - first) / (dur + padding)).clamp(0.0, 1.0) * GRAPH_WIDTH;
                            // Map life (0..1) to Y (GraphHeight..0)
                            // life 1.0 = top (y=0), life 0.0 = bottom (y=Height)
                            let y = (1.0 - life).clamp(0.0, 1.0) * GRAPH_HEIGHT;

                            // Skip if this point is identical to the last one in screen space
                            if (x - last_x).abs() < 0.5 && (y - last_y).abs() < 0.5 {
                                continue;
                            }

                            if last_x > -900.0 {
                                // Horizontal segment (if time passed)
                                let w = (x - last_x).max(0.0);
                                if w > 0.5 {
                                    life_children.push(act!(quad:
                                        align(0.0, 0.5): xy(last_x, last_y):
                                        setsize(w, 2.0): // 2px thick
                                        diffuse(1.0, 1.0, 1.0, 0.8):
                                        z(4)
                                    ));
                                }

                                // Vertical segment (drawdown/gain)
                                // This handles the "loss of life" vertical drop perfectly.
                                let h = (y - last_y).abs();
                                if h > 0.5 {
                                    let min_y = last_y.min(y);
                                    life_children.push(act!(quad:
                                        align(0.5, 0.0): xy(x, min_y):
                                        setsize(2.0, h):
                                        diffuse(1.0, 1.0, 1.0, 0.8):
                                        z(4)
                                    ));
                                }
                            } else {
                                // First point dot
                                life_children.push(act!(quad:
                                    align(0.5, 0.5): xy(x, y):
                                    setsize(2.0, 2.0):
                                    diffuse(1.0, 1.0, 1.0, 0.8):
                                    z(4)
                                ));
                            }

                            last_x = x;
                            last_y = y;
                        }

                        // Draw Fail Marker if present
                        if let Some(fail_time) = si.fail_time {
                            let x = ((fail_time - first) / (dur + padding)).clamp(0.0, 1.0)
                                * GRAPH_WIDTH;

                            // Red vertical line
                            life_children.push(act!(quad:
                                align(0.5, 0.0): xy(x, 0.0):
                                setsize(1.5, GRAPH_HEIGHT):
                                diffuse(1.0, 0.0, 0.0, 0.8):
                                z(5)
                            ));

                            // Time remaining text calculation
                            // Match Simply Love's TrackFailTime behavior:
                            // display remaining time using chart length divided by MusicRate.
                            let base_total = si.song.total_length_seconds.max(0) as f32;
                            let rate = if si.music_rate.is_finite() && si.music_rate > 0.0 {
                                si.music_rate
                            } else {
                                1.0
                            };
                            let total_display = if rate == 0.0 {
                                base_total
                            } else {
                                base_total / rate
                            };
                            let death_display = if rate == 0.0 {
                                fail_time.max(0.0)
                            } else {
                                fail_time.max(0.0) / rate
                            };
                            let remaining = (total_display - death_display).max(0.0);
                            let remaining_str = format!("-{}", format_session_time(remaining));

                            // Flag box background (Black with Red border)
                            // Using a small frame to group the flag elements
                            let flag_w = 40.0;
                            let flag_h = 14.0;

                            life_children.push(act!(quad:
                                align(1.0, 1.0): xy(x, GRAPH_HEIGHT):
                                setsize(flag_w, flag_h):
                                diffuse(1.0, 0.0, 0.0, 1.0): // Red border
                                z(5)
                            ));
                            life_children.push(act!(quad:
                                align(1.0, 1.0): xy(x - 1.0, GRAPH_HEIGHT - 1.0):
                                setsize(flag_w - 2.0, flag_h - 2.0):
                                diffuse(0.0, 0.0, 0.0, 0.8): // Black fill
                                z(6)
                            ));

                            // Flag Text
                            life_children.push(act!(text:
                                font("miso"): settext(remaining_str):
                                align(1.0, 1.0): xy(x - 4.0, GRAPH_HEIGHT - 1.5):
                                zoom(0.5):
                                diffuse(1.0, 0.3, 0.3, 1.0):
                                z(7)
                            ));
                        }
                    }

                    Actor::Frame {
                        align: [0.0, 0.0],
                        offset: [0.0, 0.0],
                        size: [SizeSpec::Px(GRAPH_WIDTH), SizeSpec::Px(GRAPH_HEIGHT)],
                        background: None,
                        z: 4,
                        children: life_children,
                    }
                },
            ],
        };
        actors.push(graph_frame);
    }

    // --- "ITG" text and Pads (top right) ---
    {
        let itg_text_x = screen_width() - widescale(55.0, 62.0);
        actors.push(act!(text: font("wendy"): settext("ITG"): align(1.0, 0.5): xy(itg_text_x, 15.0): zoom(widescale(0.5, 0.6)): z(121): diffuse(1.0, 1.0, 1.0, 1.0) ));
        let final_pad_zoom = 0.24 * widescale(0.435, 0.525);
        actors.push(pad_display::build(pad_display::PadDisplayParams {
            center_x: screen_width() - widescale(35.0, 41.0),
            center_y: widescale(22.0, 23.5),
            zoom: final_pad_zoom,
            z: 121,
            is_active: true,
        }));
        actors.push(pad_display::build(pad_display::PadDisplayParams {
            center_x: screen_width() - widescale(15.0, 17.0),
            center_y: widescale(22.0, 23.5),
            zoom: final_pad_zoom,
            z: 121,
            is_active: false,
        }));
    }

    // 3. Bottom Bar
    let play_style = profile::get_session_play_style();
    let player_side = profile::get_session_player_side();

    let p1_profile = profile::get_for_side(profile::PlayerSide::P1);
    let p2_profile = profile::get_for_side(profile::PlayerSide::P2);
    let p1_avatar = p1_profile
        .avatar_texture_key
        .as_deref()
        .map(|texture_key| AvatarParams { texture_key });
    let p2_avatar = p2_profile
        .avatar_texture_key
        .as_deref()
        .map(|texture_key| AvatarParams { texture_key });

    let p1_joined = profile::is_session_side_joined(profile::PlayerSide::P1);
    let p2_joined = profile::is_session_side_joined(profile::PlayerSide::P2);
    let p1_guest = profile::is_session_side_guest(profile::PlayerSide::P1);
    let p2_guest = profile::is_session_side_guest(profile::PlayerSide::P2);

    let (p1_footer_text, p1_footer_avatar) = if p1_joined {
        (
            Some(if p1_guest {
                "INSERT CARD"
            } else {
                p1_profile.display_name.as_str()
            }),
            if p1_guest { None } else { p1_avatar },
        )
    } else {
        (None, None)
    };
    let (p2_footer_text, p2_footer_avatar) = if p2_joined {
        (
            Some(if p2_guest {
                "INSERT CARD"
            } else {
                p2_profile.display_name.as_str()
            }),
            if p2_guest { None } else { p2_avatar },
        )
    } else {
        (None, None)
    };

    let (footer_left, footer_right, left_avatar, right_avatar) =
        if play_style == profile::PlayStyle::Versus {
            (
                p1_footer_text,
                p2_footer_text,
                p1_footer_avatar,
                p2_footer_avatar,
            )
        } else {
            match player_side {
                profile::PlayerSide::P1 => (p1_footer_text, None, p1_footer_avatar, None),
                profile::PlayerSide::P2 => (None, p2_footer_text, None, p2_footer_avatar),
            }
        };
    actors.push(screen_bar::build(ScreenBarParams {
        title: "",
        title_placement: screen_bar::ScreenBarTitlePlacement::Center,
        position: screen_bar::ScreenBarPosition::Bottom,
        transparent: true,
        fg_color: [1.0; 4],
        left_text: footer_left,
        center_text: None,
        right_text: footer_right,
        left_avatar,
        right_avatar,
    }));

    // --- Date/Time in footer (like ScreenEvaluation decorations) ---
    let now = Local::now();
    // The format matches YYYY/MM/DD HH:MM from the Lua script.
    let timestamp_text = now.format("%Y/%m/%d %H:%M").to_string();

    actors.push(act!(text:
        font("wendy_monospace_numbers"):
        settext(timestamp_text):
        align(0.5, 1.0): // align bottom-center of text block
        xy(screen_center_x(), screen_height() - 14.0):
        zoom(0.18):
        horizalign(center):
        z(121) // a bit above the screen bar (z=120)
    ));

    actors
}
