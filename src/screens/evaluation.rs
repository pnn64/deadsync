use crate::act;
use crate::core::space::*;
use crate::screens::Screen;
use crate::ui::actors::{Actor, SizeSpec};
use crate::ui::color;
use crate::ui::components::{heart_bg, pad_display, screen_bar};
use crate::ui::components::screen_bar::{AvatarParams, ScreenBarParams, ScreenBarPosition, ScreenBarTitlePlacement};
use crate::core::space::widescale;

use crate::game::judgment::{self, JudgeGrade};
use crate::screens::gameplay;
use crate::game::song::SongData;
use crate::game::chart::ChartData;
use crate::game::scores;
use std::collections::HashMap;
use std::sync::{Arc, LazyLock};
use crate::game::scroll::ScrollSpeedSetting;
use crate::game::timing as timing_stats;
use crate::assets::AssetManager;
use crate::ui::font;

use crate::game::profile;
use crate::core::input::{VirtualAction, InputEvent};
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
    // Prepared histogram in 1ms bins
    pub histogram: timing_stats::HistogramMs,
    // Time range used to scale scatter/NPS graph (FirstSecond..LastSecond)
    pub graph_first_second: f32,
    pub graph_last_second: f32,
    pub music_rate: f32,
}

pub struct State {
    pub active_color_index: i32,
    bg: heart_bg::State,
    pub session_elapsed: f32, // To display the timer
    pub score_info: Option<ScoreInfo>,
    pub density_graph_texture_key: String,
}

pub fn init(gameplay_results: Option<gameplay::State>) -> State {
    let score_info = gameplay_results.map(|gs| {
        // Compute timing statistics across all non-miss tap judgments
        let stats = timing_stats::compute_note_timing_stats(&gs.notes);
        // Prepare scatter points and histogram bins
        let scatter = timing_stats::build_scatter_points(&gs.notes, &gs.note_time_cache);
        let histogram = timing_stats::build_histogram_ms(&gs.notes);
        let graph_first_second = 0.0_f32.min(gs.timing.get_time_for_beat(0.0));
        // Pad right bound slightly (0.05s) to match SL visual alignment.
        let graph_last_second = gs.song.total_length_seconds as f32 + 0.05;

        let score_percent = judgment::calculate_itg_score_percent(
            &gs.scoring_counts,
            gs.holds_held_for_score,
            gs.rolls_held_for_score,
            gs.mines_hit_for_score,
            gs.possible_grade_points,
        );

        let grade = if gs.is_failing || !gs.song_completed_naturally {
            scores::Grade::Failed
        } else {
            scores::score_to_grade(score_percent * 10000.0)
        };

        ScoreInfo {
            song: gs.song.clone(),
            chart: gs.chart.clone(),
            judgment_counts: gs.judgment_counts.clone(),
            score_percent,
            grade,
            speed_mod: gs.scroll_speed,
            hands_achieved: gs.hands_achieved,
            holds_held: gs.holds_held,
            holds_total: gs.holds_total,
            rolls_held: gs.rolls_held,
            rolls_total: gs.rolls_total,
            mines_avoided: gs.mines_avoided,
            mines_total: gs.mines_total,
            timing: stats,
            scatter,
            histogram,
            graph_first_second,
            graph_last_second,
            music_rate: if gs.music_rate.is_finite() && gs.music_rate > 0.0 { gs.music_rate } else { 1.0 },
        }
    });

    State {
        active_color_index: color::DEFAULT_COLOR_INDEX, // This will be overwritten by app.rs
        bg: heart_bg::State::new(),
        session_elapsed: 0.0,
        score_info,
        density_graph_texture_key: "__white".to_string(),
    }
}

// Keyboard input is handled centrally via the virtual dispatcher in app.rs

// This screen doesn't have any dynamic state updates yet, but we keep the function for consistency.
pub fn update(_state: &mut State, _dt: f32) {
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
        format!("{}:{:02}:{:02}", hours, minutes, seconds)
    } else {
        format!("{:02}:{:02}", minutes, seconds)
    }
}

pub fn handle_input(_state: &mut State, ev: &InputEvent) -> ScreenAction {
    if ev.pressed {
        match ev.action {
            VirtualAction::P1_Back | VirtualAction::P1_Start => return ScreenAction::Navigate(Screen::SelectMusic),
            _ => {}
        }
    }
    ScreenAction::None
}

// --- Statics and helper function for the P1 stats pane ---

static JUDGMENT_ORDER: [JudgeGrade; 6] = [
    JudgeGrade::Fantastic, JudgeGrade::Excellent, JudgeGrade::Great,
    JudgeGrade::Decent, JudgeGrade::WayOff, JudgeGrade::Miss,
];

struct JudgmentDisplayInfo {
    label: &'static str,
    color: [f32; 4],
}

static JUDGMENT_INFO: LazyLock<HashMap<JudgeGrade, JudgmentDisplayInfo>> = LazyLock::new(|| {
    HashMap::from([
        (JudgeGrade::Fantastic, JudgmentDisplayInfo { label: "FANTASTIC", color: color::rgba_hex(color::JUDGMENT_HEX[0]) }),
        (JudgeGrade::Excellent, JudgmentDisplayInfo { label: "EXCELLENT", color: color::rgba_hex(color::JUDGMENT_HEX[1]) }),
        (JudgeGrade::Great,     JudgmentDisplayInfo { label: "GREAT",     color: color::rgba_hex(color::JUDGMENT_HEX[2]) }),
        (JudgeGrade::Decent,    JudgmentDisplayInfo { label: "DECENT",    color: color::rgba_hex(color::JUDGMENT_HEX[3]) }),
        (JudgeGrade::WayOff,    JudgmentDisplayInfo { label: "WAY OFF",   color: color::rgba_hex(color::JUDGMENT_HEX[4]) }),
        (JudgeGrade::Miss,      JudgmentDisplayInfo { label: "MISS",      color: color::rgba_hex(color::JUDGMENT_HEX[5]) }),
    ])
});

/// Builds the entire P1 (left side) stats pane including judgments and radar counts.
fn build_p1_stats_pane(state: &State, asset_manager: &AssetManager) -> Vec<Actor> {
    let Some(score_info) = &state.score_info else { return vec![]; };
    let mut actors = Vec::new();
    let cy = screen_center_y();

    // The base offset for all P1 panes from the screen center.
    let p1_side_offset = screen_center_x() - 155.0;

    // --- Calculate label shift for large numbers ---
    let max_judgment_count = JUDGMENT_ORDER.iter()
        .map(|grade| score_info.judgment_counts.get(grade).cloned().unwrap_or(0))
        .max().unwrap_or(0);
    
    let (label_shift_x, label_zoom) = if max_judgment_count > 9999 {
        let length = (max_judgment_count as f32).log10().floor() as i32 + 1;
        (-11.0 * (length - 4) as f32, 0.833 - 0.1 * (length - 4) as f32)
    } else {
        (0.0, 0.833)
    };

    let digits_needed = if max_judgment_count == 0 { 1 } else { (max_judgment_count as f32).log10().floor() as usize + 1 };
    let digits_to_fmt = digits_needed.max(4);

    asset_manager.with_fonts(|all_fonts| asset_manager.with_font("wendy_screenevaluation", |metrics_font| {
        let numbers_frame_zoom = 0.8;
        let final_numbers_zoom = numbers_frame_zoom * 0.5;
        let digit_width = font::measure_line_width_logical(metrics_font, "0", all_fonts) as f32 * final_numbers_zoom;
        let slash_width = font::measure_line_width_logical(metrics_font, "/", all_fonts) as f32 * final_numbers_zoom;
        if digit_width <= 0.0 { return; }
        let slash_width = if slash_width > 0.0 { slash_width } else { digit_width };

        // --- Judgment Labels & Numbers ---
        let labels_frame_origin_x = p1_side_offset + 50.0;
        let numbers_frame_origin_x = p1_side_offset + 90.0;
        let frame_origin_y = cy - 24.0;

        for (i, grade) in JUDGMENT_ORDER.iter().enumerate() {
            let info = JUDGMENT_INFO.get(grade).unwrap();
            let count = score_info.judgment_counts.get(grade).cloned().unwrap_or(0);
            
            // Label
            let label_local_x = 28.0 + label_shift_x;
            let label_local_y = (i as f32 * 28.0) - 16.0;
            actors.push(act!(text: font("miso"): settext(info.label):
                align(1.0, 0.5): xy(labels_frame_origin_x + label_local_x, frame_origin_y + label_local_y):
                maxwidth(76.0): zoom(label_zoom): horizalign(right):
                diffuse(info.color[0], info.color[1], info.color[2], info.color[3]): z(101)
            ));

            // Number (digit by digit for dimming)
            let bright_color = info.color;
            let dim_color = color::rgba_hex(color::JUDGMENT_DIM_EVAL_HEX[i]);
            let number_str = format!("{:0width$}", count, width = digits_to_fmt);
            let first_nonzero = number_str.find(|c: char| c != '0').unwrap_or(number_str.len());
            
            let number_local_x = 64.0;
            let number_local_y = (i as f32 * 35.0) - 20.0;
            let number_final_y = frame_origin_y + (number_local_y * numbers_frame_zoom);
            let number_base_x = numbers_frame_origin_x + (number_local_x * numbers_frame_zoom);
            
            for (char_idx, ch) in number_str.chars().enumerate() {
                let is_dim = if count == 0 { char_idx < digits_to_fmt - 1 } else { char_idx < first_nonzero };
                let color = if is_dim { dim_color } else { bright_color };
                let index_from_right = digits_to_fmt - 1 - char_idx;
                let cell_right_x = number_base_x - (index_from_right as f32 * digit_width);
                
                actors.push(act!(text: font("wendy_screenevaluation"): settext(ch.to_string()):
                    align(1.0, 0.5): xy(cell_right_x, number_final_y): zoom(final_numbers_zoom):
                    diffuse(color[0], color[1], color[2], color[3]): z(101)
                ));
            }
        }
        
        // --- RADAR LABELS & NUMBERS ---
        let radar_categories = [
            ("hands", score_info.hands_achieved, score_info.chart.stats.hands),
            ("holds", score_info.holds_held, score_info.holds_total),
            ("mines", score_info.mines_avoided, score_info.mines_total),
            ("rolls", score_info.rolls_held, score_info.rolls_total),
        ];

        let gray_color_possible = color::rgba_hex("#5A6166");
        let gray_color_achieved = color::rgba_hex("#444444");
        let white_color = [1.0, 1.0, 1.0, 1.0];

        for (i, (label, achieved, possible)) in radar_categories.iter().cloned().enumerate() {
            let label_local_x = -160.0;
            let label_local_y = (i as f32 * 28.0) + 41.0;
            actors.push(act!(text: font("miso"): settext(label.to_string()):
                align(1.0, 0.5): xy(labels_frame_origin_x + label_local_x, frame_origin_y + label_local_y): horizalign(right): zoom(0.833): z(101)
            ));

            let possible_clamped = possible.min(999);
            let achieved_clamped = achieved.min(999);
            
            let number_local_y = (i as f32 * 35.0) + 53.0;
            let number_final_y = frame_origin_y + (number_local_y * numbers_frame_zoom);
            
            // --- Actor Group: "Achieved / Possible" (Right-aligned at local x = -114) ---
            let right_anchor_x = numbers_frame_origin_x + (-114.0 * numbers_frame_zoom);
            let mut cursor_x = right_anchor_x; // Start drawing from the right edge.

            // 1. Draw "possible" number (right-most part)
            let possible_str = format!("{:03}", possible_clamped);
            let first_nonzero_possible = possible_str.find(|c: char| c != '0').unwrap_or(possible_str.len());

            for (char_idx_from_right, ch) in possible_str.chars().rev().enumerate() {
                let is_dim = if possible_clamped == 0 { 
                    char_idx_from_right > 0 
                } else { 
                    let idx_from_left = 2 - char_idx_from_right;
                    idx_from_left < first_nonzero_possible
                };
                let color = if is_dim { gray_color_possible } else { white_color };
                
                actors.push(act!(text: font("wendy_screenevaluation"): settext(ch.to_string()):
                    align(1.0, 0.5): xy(cursor_x, number_final_y): zoom(final_numbers_zoom):
                    diffuse(color[0], color[1], color[2], color[3]): z(101)
                ));
                cursor_x -= digit_width;
            }

            // 2. Draw slash
            actors.push(act!(text: font("wendy_screenevaluation"): settext("/"):
                align(1.0, 0.5): xy(cursor_x, number_final_y): zoom(final_numbers_zoom):
                diffuse(gray_color_possible[0], gray_color_possible[1], gray_color_possible[2], gray_color_possible[3]): z(101)
            ));
            cursor_x -= slash_width;

            // 3. Draw "achieved" number (left-most part)
            let achieved_str = format!("{:03}", achieved_clamped);
            let first_nonzero_achieved = achieved_str.find(|c: char| c != '0').unwrap_or(achieved_str.len());

            // The 'achieved' block must have its own right-anchor for alignment within the group.
            let achieved_block_right_x = cursor_x;

            for (char_idx_from_right, ch) in achieved_str.chars().rev().enumerate() {
                 let is_dim = if achieved == 0 { 
                    char_idx_from_right > 0
                } else { 
                    let idx_from_left = 2 - char_idx_from_right;
                    idx_from_left < first_nonzero_achieved 
                };
                let color = if is_dim { gray_color_achieved } else { white_color };
                let x_pos = achieved_block_right_x - (char_idx_from_right as f32 * digit_width);

                actors.push(act!(text: font("wendy_screenevaluation"): settext(ch.to_string()):
                    align(1.0, 0.5): xy(x_pos, number_final_y): zoom(final_numbers_zoom):
                    diffuse(color[0], color[1], color[2], color[3]): z(101)
                ));
            }
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
    let bar_bg_color = color::rgba_hex("#101519");

    // Top and Bottom bars
    children.push(act!(quad:
        align(0.0, 0.0): xy(0.0, 0.0):
        setsize(pane_width, topbar_height):
        diffuse(bar_bg_color[0], bar_bg_color[1], bar_bg_color[2], 1.0)
    ));
    children.push(act!(quad:
        align(0.0, 1.0): xy(0.0, pane_height):
        setsize(pane_width, bottombar_height):
        diffuse(bar_bg_color[0], bar_bg_color[1], bar_bg_color[2], 1.0)
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
        let color = color::rgba_hex(color::JUDGMENT_HEX[*grade_idx]);
        let window_ms = if i > 0 { timing_windows[i-1] } else { 0.0 };
        let next_window_ms = timing_windows[i];
        let mid_point_ms = (window_ms + next_window_ms) / 2.0_f32;
        
        // Scale position from ms to pane coordinates
        let x_offset = (mid_point_ms / worst_window) * (pane_width / 2.0_f32);

        if i == 0 { // "Fan" is centered
            children.push(act!(text: font("miso"): settext(*label):
                align(0.5, 0.5): xy(pane_width / 2.0_f32, bottom_bar_center_y):
                zoom(0.65): diffuse(color[0], color[1], color[2], color[3])
            ));
        } else { // Others are symmetric
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

    // Histogram bars (aggregate timing offsets)
    if let Some(score_info) = &state.score_info {
        let graph_area_height = (pane_height - topbar_height - bottombar_height).max(1.0_f32);
        let y_bottom = pane_height - bottombar_height;
        let worst_bin = (score_info.histogram.worst_window_ms / 1.0_f32).round() as i32; // 1ms bins
        let total_bins = (worst_bin * 2 + 1).max(1) as i32;
        let bar_w = pane_width / (total_bins as f32);

        let use_smoothing = crate::config::get().smooth_histogram;
        use std::collections::HashMap;
        let worst_observed = score_info.histogram.worst_observed_ms.max(0.0);
        // Choose source (smoothed vs raw)
        let (height_scale_peak, get_value_at_bin): (f32, Box<dyn Fn(i32) -> f32>) = if use_smoothing {
            let mut smooth_map: HashMap<i32, f32> = HashMap::with_capacity(score_info.histogram.smoothed.len());
            for (bin, val) in &score_info.histogram.smoothed { smooth_map.insert(*bin, *val); }
            let raw_peak = score_info.histogram.max_count.max(1) as f32;
            (raw_peak, Box::new(move |b| *smooth_map.get(&b).unwrap_or(&0.0)))
        } else {
            let mut count_map: HashMap<i32, u32> = HashMap::with_capacity(score_info.histogram.bins.len());
            for (bin, cnt) in &score_info.histogram.bins { count_map.insert(*bin, *cnt); }
            let max_count = score_info.histogram.max_count.max(1) as f32;
            (max_count, Box::new(move |b| *count_map.get(&b).unwrap_or(&0) as f32))
        };

        let color_for_abs_ms = |abs_ms: f32| -> [f32; 4] {
            if abs_ms <= timing_windows[0] { color::rgba_hex(color::JUDGMENT_HEX[0]) }
            else if abs_ms <= timing_windows[1] { color::rgba_hex(color::JUDGMENT_HEX[1]) }
            else if abs_ms <= timing_windows[2] { color::rgba_hex(color::JUDGMENT_HEX[2]) }
            else if abs_ms <= timing_windows[3] { color::rgba_hex(color::JUDGMENT_HEX[3]) }
            else { color::rgba_hex(color::JUDGMENT_HEX[4]) }
        };

        let mut draw_bin = |bin_idx: i32, y_val: f32| {
            if y_val <= 0.0 { return; }
            let x = (bin_idx - (-worst_bin)) as f32 * bar_w;
            let abs_ms = (bin_idx as f32).abs();
            // Don't draw beyond the worst observed offset, matching SL's behavior
            if abs_ms > worst_observed { return; }
            let col = color_for_abs_ms(abs_ms);
            // Scale value by peak to 75% of graph area
            let h = (y_val / height_scale_peak) * (graph_area_height * 0.75);
            if h <= 0.0 { return; }
            children.push(act!(quad:
                align(0.0, 1.0): xy(x, y_bottom):
                setsize(bar_w, h):
                diffuse(col[0], col[1], col[2], 1.0)
            ));
        };

        for bin_idx in (-worst_bin)..=(worst_bin) {
            let y = get_value_at_bin(bin_idx);
            draw_bin(bin_idx, y);
        }
    }

    // Top bar stats
    let top_label_y = 2.0;
    let top_value_y = 13.0;
    let label_zoom = 0.575;
    let value_zoom = 0.8;

    let max_error_text = state
        .score_info
        .as_ref()
        .map(|s| format!("{:.1}ms", s.timing.max_abs_ms))
        .unwrap_or_else(|| "0.0ms".to_string());

    let stats = state.score_info.as_ref();
    let mean_abs_text = stats.map(|s| format!("{:.1}ms", s.timing.mean_abs_ms)).unwrap_or_else(|| "0.0ms".to_string());
    let mean_text = stats.map(|s| format!("{:.1}ms", s.timing.mean_ms)).unwrap_or_else(|| "0.0ms".to_string());
    let stddev3_text = stats.map(|s| format!("{:.1}ms", s.timing.stddev_ms * 3.0)).unwrap_or_else(|| "0.0ms".to_string());

    let labels_and_values = [
        ("mean abs error", 40.0, mean_abs_text),
        (
            "mean",
            40.0 + (pane_width - 80.0_f32) / 3.0_f32,
            mean_text,
        ),
        (
            "std dev * 3",
            40.0 + (pane_width - 80.0_f32) / 3.0_f32 * 2.0_f32,
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

    // Get the speed mod from state.score_info
    let speed_mod_text = state.score_info.as_ref().unwrap().speed_mod.to_string();
    let final_text = format!("{}, Overhead", speed_mod_text);

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
    let profile = profile::get();

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
        left_text: None, center_text: None, right_text: None,
        left_avatar: None,
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
        let pane_width = (300.0 * 2.0) + 10.0;
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
        let banner_key = score_info.song.banner_path.as_ref()
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_else(|| {
                let banner_num = state.active_color_index.rem_euclid(12) + 1;
                format!("banner{}.png", banner_num)
            });

        let full_title = if score_info.song.subtitle.trim().is_empty() {
            score_info.song.title.clone()
        } else {
            format!("{} {}", score_info.song.title, score_info.song.subtitle)
        };

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
            let rate_f64 = score_info.music_rate as f64;
            let min = (score_info.song.min_bpm * rate_f64).round() as i32;
            let max = (score_info.song.max_bpm * rate_f64).round() as i32;
            let base = if (score_info.song.min_bpm - score_info.song.max_bpm).abs() < 1e-6 {
                format!("{} bpm", min)
            } else {
                format!("{} - {} bpm", min, max)
            };
            if (score_info.music_rate - 1.0).abs() > 0.001 { format!("{} ({}x Music Rate)", base, format!("{:.2}", score_info.music_rate)) } else { base }
        };

        let length_text = {
            let seconds = score_info.song.total_length_seconds;
            if seconds < 0 { "".to_string() }
            else if seconds >= 3600 { format!("{}:{:02}:{:02}", seconds / 3600, (seconds % 3600) / 60, seconds % 60) }
            else { format!("{}:{:02}", seconds / 60, seconds % 60) }
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

    // Letter Grade (0.4 for parity with individual pngs)
    actors.push(act!(sprite("grades/grades 1x19.png"): align(0.5, 0.5): xy(p1_frame_x - 70.0, cy - 134.0): zoom(1.0): z(101): setstate(score_info.grade.to_sprite_state()) ));

    // Difficulty Text and Meter Block
    {
        // Find the index of the current difficulty to look up the display name.
        let difficulty_index = color::FILE_DIFFICULTY_NAMES.iter().position(|&n| n.eq_ignore_ascii_case(&score_info.chart.difficulty)).unwrap_or(2);
        let difficulty_display_name = color::DISPLAY_DIFFICULTY_NAMES[difficulty_index];

        let difficulty_color = color::difficulty_rgba(&score_info.chart.difficulty, state.active_color_index);
        let difficulty_text = format!("Single / {}", difficulty_display_name);
        actors.push(act!(text: font("miso"): settext(difficulty_text): align(0.0, 0.5): xy(p1_frame_x - 115.0, cy - 65.0): zoom(0.7): z(101): diffuse(1.0, 1.0, 1.0, 1.0) ));
        actors.push(act!(quad: align(0.5, 0.5): xy(p1_frame_x - 134.5, cy - 71.0): zoomto(30.0, 30.0): z(101): diffuse(difficulty_color[0], difficulty_color[1], difficulty_color[2], 1.0) ));
        actors.push(act!(text: font("wendy"): settext(score_info.chart.meter.to_string()): align(0.5, 0.5): xy(p1_frame_x - 134.5, cy - 71.0): zoom(0.4): z(102): diffuse(0.0, 0.0, 0.0, 1.0) ));
    }

    // Step Artist
    actors.push(act!(text: font("miso"): settext(score_info.chart.step_artist.clone()): align(0.0, 0.5): xy(p1_frame_x - 115.0, cy - 81.0): zoom(0.7): z(101): diffuse(1.0, 1.0, 1.0, 1.0) ));

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
                        let logical_width = font::measure_line_width_logical(miso_font, text, all_fonts) as f32;
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
        let score_bg_color = color::rgba_hex("#101519");

        let score_display_frame = Actor::Frame {
            align: [0.5, 0.5],
            offset: [p1_frame_x, score_frame_y],
            size: [SizeSpec::Px(0.0), SizeSpec::Px(0.0)],
            background: None,
            z: 101,
            children: vec![
                act!(quad: align(0.0, 0.5): xy(-150.0, 0.0): setsize(158.5, 60.0): diffuse(score_bg_color[0], score_bg_color[1], score_bg_color[2], 1.0) ),
                act!(text: font("wendy_white"): settext(percent_text): align(1.0, 0.5): xy(1.5, 0.0): zoom(0.585): horizalign(right)),
            ],
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
                // The NPS histogram is positioned with its origin at the bottom-left of the frame,
                // and then shifted to be centered horizontally.
                // Lua: `addx(-GraphWidth/2):addy(GraphHeight)`
                // This is equivalent to `align(0.0, 1.0)` (bottom-left) and `xy` at the center of the frame.
                act!(sprite(state.density_graph_texture_key.clone()):
                    align(0.0, 1.0): // bottom-left
                    xy(0.0, GRAPH_HEIGHT): // position at the bottom-left of the frame
                    setsize(GRAPH_WIDTH, GRAPH_HEIGHT): z(1)
                ),
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
                    // Build actors for scatter lazily here
                    let mut scatter_children: Vec<Actor> = Vec::new();
                    if let Some(si) = &state.score_info {
                        let first = si.graph_first_second;
                        let last = si.graph_last_second.max(first + 0.001_f32);
                        let dur = (last - first).max(0.001_f32);
                        let worst = si.histogram.worst_window_ms.max(1.0_f32);

                        let color_for_abs_ms = |abs_ms: f32| -> [f32; 4] {
                            let tw = crate::game::timing::effective_windows_ms();
                            if abs_ms <= tw[0] { color::rgba_hex(color::JUDGMENT_HEX[0]) }
                            else if abs_ms <= tw[1] { color::rgba_hex(color::JUDGMENT_HEX[1]) }
                            else if abs_ms <= tw[2] { color::rgba_hex(color::JUDGMENT_HEX[2]) }
                            else if abs_ms <= tw[3] { color::rgba_hex(color::JUDGMENT_HEX[3]) }
                            else { color::rgba_hex(color::JUDGMENT_HEX[4]) }
                        };

                        for sp in &si.scatter {
                            // For non-miss offsets, shift x by the offset amount (align to actual tap time).
                            // For misses, shift by the current worst window to mimic SL.
                            let x_time = match sp.offset_ms {
                                Some(off_ms) => sp.time_sec - (off_ms / 1000.0),
                                None => sp.time_sec - (worst / 1000.0),
                            };
                            // SL pads the right bound slightly by +0.05 seconds.
                            let x = ((x_time - first) / (dur + 0.05)).clamp(0.0, 1.0) * GRAPH_WIDTH;
                            match sp.offset_ms {
                                Some(off_ms) => {
                                    // Map offset to vertical position; center is at GRAPH_HEIGHT/2
                                    let t = ((worst - off_ms) / (2.0 * worst)).clamp(0.0, 1.0);
                                    let y = t * GRAPH_HEIGHT;
                                    let c = color_for_abs_ms(off_ms.abs());
                                    scatter_children.push(act!(quad:
                                        align(0.0, 0.0): xy(x, y):
                                        setsize(1.5, 1.5):
                                        diffuse(c[0], c[1], c[2], 0.666):
                                        z(3)
                                    ));
                                }
                                None => {
                                    // Miss: draw a thin red column
                                    scatter_children.push(act!(quad:
                                        align(0.0, 0.0): xy(x, 0.0):
                                        setsize(1.0, GRAPH_HEIGHT):
                                        diffuse(1.0, 0.0, 0.0, 0.47):
                                        z(3)
                                    ));
                                }
                            }
                        }
                    }
                    Actor::Frame {
                        align: [0.0, 0.0],
                        offset: [0.0, 0.0],
                        size: [SizeSpec::Px(GRAPH_WIDTH), SizeSpec::Px(GRAPH_HEIGHT)],
                        background: None,
                        z: 3,
                        children: scatter_children,
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
        actors.push(pad_display::build(pad_display::PadDisplayParams { center_x: screen_width() - widescale(35.0, 41.0), center_y: widescale(22.0, 23.5), zoom: final_pad_zoom, z: 121, is_active: true, }));
        actors.push(pad_display::build(pad_display::PadDisplayParams { center_x: screen_width() - widescale(15.0, 17.0), center_y: widescale(22.0, 23.5), zoom: final_pad_zoom, z: 121, is_active: false, }));
    }

    // 3. Bottom Bar
    let footer_avatar = profile
        .avatar_texture_key
        .as_deref()
        .map(|texture_key| AvatarParams { texture_key });
    actors.push(screen_bar::build(ScreenBarParams {
        title: "",
        title_placement: screen_bar::ScreenBarTitlePlacement::Center,
        position: screen_bar::ScreenBarPosition::Bottom,
        transparent: true,
        fg_color: [1.0; 4],
        left_text: Some(&profile.display_name), center_text: None, right_text: None,
        left_avatar: footer_avatar,
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
