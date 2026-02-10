use crate::act;
use crate::core::gfx::{BlendMode, MeshMode, MeshVertex};
use crate::core::space::widescale;
use crate::core::space::{screen_center_x, screen_center_y, screen_height, screen_width};
use crate::screens::Screen;
use crate::screens::components::screen_bar::{
    AvatarParams, ScreenBarParams, ScreenBarPosition, ScreenBarTitlePlacement,
};
use crate::screens::components::{eval_grades, heart_bg, pad_display, qr_code, screen_bar};
use crate::ui::actors::{Actor, SizeSpec};
use crate::ui::color;

use crate::assets::AssetManager;
use crate::game::chart::ChartData;
use crate::game::gameplay::MAX_PLAYERS;
use crate::game::judgment::{self, JudgeGrade};
use crate::game::note::NoteType;
use crate::game::parsing::noteskin::{NUM_QUANTIZATIONS, Noteskin, Quantization};
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
// Simply Love metrics.ini [RollingNumbersEvaluation]: ApproachSeconds=1
const ROLLING_NUMBERS_APPROACH_SECONDS: f32 = 1.0;
// Simply Love ScreenEvaluationStage in/default.lua (non-SRPG9 branch)
const EVAL_STAGE_IN_BLACK_DELAY_SECONDS: f32 = 0.2;
const EVAL_STAGE_IN_BLACK_FADE_SECONDS: f32 = 0.5;
const EVAL_STAGE_IN_TEXT_FADE_IN_SECONDS: f32 = 0.4;
const EVAL_STAGE_IN_TEXT_HOLD_SECONDS: f32 = 0.6;
const EVAL_STAGE_IN_TEXT_FADE_OUT_SECONDS: f32 = 0.4;
const EVAL_STAGE_IN_TOTAL_SECONDS: f32 = EVAL_STAGE_IN_TEXT_FADE_IN_SECONDS
    + EVAL_STAGE_IN_TEXT_HOLD_SECONDS
    + EVAL_STAGE_IN_TEXT_FADE_OUT_SECONDS;
const MACHINE_RECORD_ROWS: usize = 10;
const MACHINE_RECORD_SPLIT_MACHINE_ROWS: usize = 8;
const MACHINE_RECORD_SPLIT_PERSONAL_ROWS: usize = 2;
const MACHINE_RECORD_DEFAULT_ROW_HEIGHT: f32 = 22.0;
const MACHINE_RECORD_SPLIT_ROW_HEIGHT: f32 = 20.25;
const MACHINE_RECORD_SPLIT_SEPARATOR_Y_ROWS: f32 = 9.0;
const MACHINE_RECORD_HIGHLIGHT_PERIOD_SECONDS: f32 = 4.0 / 3.0;
const GS_RECORD_ROWS: usize = 10;
const GS_LOADING_TEXT: &str = "Loading ...";
const GS_NO_SCORES_TEXT: &str = "No Scores";
const GS_ERROR_TIMEOUT: &str = "Timed Out";
const GS_ERROR_FAILED: &str = "Failed to Load 😞";
const GS_ERROR_DISABLED: &str = "Disabled";
const GS_ROW_PLACEHOLDER_RANK: &str = "---";
const GS_ROW_PLACEHOLDER_NAME: &str = "----";
const GS_ROW_PLACEHOLDER_SCORE: &str = "------";
const GS_ROW_PLACEHOLDER_DATE: &str = "----------";
const GS_QR_URL: &str = "https://www.groovestats.com";
const GS_QR_TITLE: &str = "GrooveStats QR";
const GS_QR_HELP_TEXT: &str =
    "Scan with your phone\nto upload this score\nto your GrooveStats\naccount.";
const GS_QR_FALLBACK_TEXT: &str = "QR Unavailable";
const GS_RIVAL_COLOR: [f32; 4] = color::rgba_hex("#BD94FF");
const GS_SELF_COLOR: [f32; 4] = color::rgba_hex("#A1FF94");
const MONTH_ABBR: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

// A struct to hold a snapshot of the final score data from the gameplay screen.
#[derive(Clone)]
pub struct ScoreInfo {
    pub song: Arc<SongData>,
    pub chart: Arc<ChartData>,
    pub profile_name: String,
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
    // Like window_counts, but with the Fantastic split at 10ms (Arrow Cloud: "SmallerWhite").
    pub window_counts_10ms: timing_stats::WindowCounts,
    // FA+ style EX score percentage (0.00–100.00), using the same semantics
    // as ScreenGameplay's EX HUD (Simply Love's CalculateExScore).
    pub ex_score_percent: f64,
    // Arrow Cloud style "H.EX" score percentage (0.00–100.00).
    pub hard_ex_score_percent: f64,
    // Per-column tap note judgment breakdown (Pane3 in Simply Love).
    pub column_judgments: Vec<ColumnJudgments>,
    // Noteskin used during gameplay, for Pane3 column previews.
    pub noteskin: Option<Arc<Noteskin>>,
    pub show_fa_plus_window: bool,
    pub show_ex_score: bool,
    pub show_hard_ex_score: bool,
    pub show_fa_plus_pane: bool,
    pub machine_records: Vec<scores::LeaderboardEntry>,
    pub machine_record_highlight_rank: Option<u32>,
    pub personal_records: Vec<scores::LeaderboardEntry>,
    pub personal_record_highlight_rank: Option<u32>,
    pub show_machine_personal_split: bool,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct ColumnJudgments {
    pub w0: u32,
    pub w1: u32,
    pub w2: u32,
    pub w3: u32,
    pub w4: u32,
    pub w5: u32,
    pub miss: u32,
    pub early_w4: u32,
    pub early_w5: u32,
    pub held_miss: u32,
}

fn compute_column_judgments(
    notes: &[crate::game::note::Note],
    cols_per_player: usize,
    col_offset: usize,
) -> Vec<ColumnJudgments> {
    let cols = cols_per_player.max(0);
    let mut out = vec![ColumnJudgments::default(); cols];
    if cols == 0 {
        return out;
    }

    for note in notes {
        if note.is_fake || !note.can_be_judged || matches!(note.note_type, NoteType::Mine) {
            continue;
        }
        let Some(j) = note.result.as_ref() else {
            continue;
        };
        let col = note.column.saturating_sub(col_offset);
        if col >= out.len() {
            continue;
        }
        let slot = &mut out[col];

        match j.grade {
            JudgeGrade::Fantastic => match j.window {
                Some(crate::game::judgment::TimingWindow::W0) => {
                    slot.w0 = slot.w0.saturating_add(1)
                }
                _ => slot.w1 = slot.w1.saturating_add(1),
            },
            JudgeGrade::Excellent => slot.w2 = slot.w2.saturating_add(1),
            JudgeGrade::Great => slot.w3 = slot.w3.saturating_add(1),
            JudgeGrade::Decent => {
                slot.w4 = slot.w4.saturating_add(1);
                if j.time_error_ms < 0.0 {
                    slot.early_w4 = slot.early_w4.saturating_add(1);
                }
            }
            JudgeGrade::WayOff => {
                slot.w5 = slot.w5.saturating_add(1);
                if j.time_error_ms < 0.0 {
                    slot.early_w5 = slot.early_w5.saturating_add(1);
                }
            }
            JudgeGrade::Miss => {
                slot.miss = slot.miss.saturating_add(1);
                if j.miss_because_held {
                    slot.held_miss = slot.held_miss.saturating_add(1);
                }
            }
        }
    }

    out
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EvalPane {
    Standard,
    FaPlus,
    HardEx,
    Column,
    MachineRecords,
    QrCode,
    GrooveStats,
    Timing,
}

impl EvalPane {
    #[inline(always)]
    const fn default_for(show_fa_plus_pane: bool) -> Self {
        if show_fa_plus_pane {
            Self::FaPlus
        } else {
            Self::Standard
        }
    }

    #[inline(always)]
    fn next(self, has_hard_ex: bool, has_gs: bool) -> Self {
        // Order (per user parity request):
        // ITG -> EX -> H.EX -> Arrow breakdown -> Machine -> QR -> GS -> Timing -> ITG
        match (self, has_hard_ex, has_gs) {
            (Self::Standard, _, _) => Self::FaPlus,
            (Self::FaPlus, true, _) => Self::HardEx,
            (Self::FaPlus, false, _) => Self::Column,
            (Self::HardEx, true, _) => Self::Column,
            (Self::HardEx, false, _) => Self::Standard,
            (Self::Column, _, _) => Self::MachineRecords,
            (Self::MachineRecords, _, _) => Self::QrCode,
            (Self::QrCode, _, true) => Self::GrooveStats,
            (Self::QrCode, _, false) => Self::Timing,
            (Self::GrooveStats, _, _) => Self::Timing,
            (Self::Timing, _, _) => Self::Standard,
        }
    }

    #[inline(always)]
    fn prev(self, has_hard_ex: bool, has_gs: bool) -> Self {
        match (self, has_hard_ex, has_gs) {
            (Self::Standard, _, _) => Self::Timing,
            (Self::Timing, _, true) => Self::GrooveStats,
            (Self::Timing, _, false) => Self::QrCode,
            (Self::GrooveStats, _, _) => Self::QrCode,
            (Self::QrCode, _, _) => Self::MachineRecords,
            (Self::MachineRecords, _, _) => Self::Column,
            (Self::Column, true, _) => Self::HardEx,
            (Self::Column, false, _) => Self::FaPlus,
            (Self::HardEx, true, _) => Self::FaPlus,
            (Self::HardEx, false, _) => Self::Standard,
            (Self::FaPlus, _, _) => Self::Standard,
        }
    }
}

pub struct State {
    pub active_color_index: i32,
    bg: heart_bg::State,
    pub screen_elapsed: f32,
    pub session_elapsed: f32, // To display the timer
    pub stage_duration_seconds: f32,
    pub score_info: [Option<ScoreInfo>; MAX_PLAYERS],
    pub density_graph_mesh: [Option<Arc<[MeshVertex]>>; MAX_PLAYERS],
    pub timing_hist_mesh: [Option<Arc<[MeshVertex]>>; MAX_PLAYERS],
    pub scatter_mesh: [Option<Arc<[MeshVertex]>>; MAX_PLAYERS],
    pub density_graph_texture_key: String,
    active_pane: [EvalPane; MAX_PLAYERS],
}

pub fn init(gameplay_results: Option<gameplay::State>) -> State {
    let mut score_info: [Option<ScoreInfo>; MAX_PLAYERS] = std::array::from_fn(|_| None);
    let mut density_graph_mesh: [Option<Arc<[MeshVertex]>>; MAX_PLAYERS] =
        std::array::from_fn(|_| None);
    let mut timing_hist_mesh: [Option<Arc<[MeshVertex]>>; MAX_PLAYERS] =
        std::array::from_fn(|_| None);
    let mut scatter_mesh: [Option<Arc<[MeshVertex]>>; MAX_PLAYERS] = std::array::from_fn(|_| None);
    let mut active_pane: [EvalPane; MAX_PLAYERS] = [EvalPane::Standard; MAX_PLAYERS];
    let mut stage_duration_seconds: f32 = 0.0;
    let mut machine_records_by_hash: HashMap<String, Vec<scores::LeaderboardEntry>> =
        HashMap::new();

    if let Some(mut gs) = gameplay_results {
        stage_duration_seconds = gs.total_elapsed_in_screen;

        // Persist one score file per play (per local profile), including fails and replay lane
        // input, unless gameplay was disqualified (e.g., autoplay used).
        scores::save_local_scores_from_gameplay(&gs);

        let cols_per_player = gs.cols_per_player;
        for player_idx in 0..gs.num_players.min(MAX_PLAYERS) {
            let noteskin = gs.noteskin[player_idx].take().map(Arc::new);
            let (start, end) = gs.note_ranges[player_idx];
            let notes = &gs.notes[start..end];
            let note_times = &gs.note_time_cache[start..end];
            let hold_end_times = &gs.hold_end_time_cache[start..end];
            let p = &gs.players[player_idx];
            let prof = &gs.player_profiles[player_idx];

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
            let side = if gs.num_players >= 2 {
                if player_idx == 0 {
                    profile::PlayerSide::P1
                } else {
                    profile::PlayerSide::P2
                }
            } else {
                profile::get_session_player_side()
            };
            let machine_records = if let Some(records) =
                machine_records_by_hash.get(&gs.charts[player_idx].short_hash)
            {
                records.clone()
            } else {
                let records = scores::get_machine_leaderboard_local(
                    &gs.charts[player_idx].short_hash,
                    usize::MAX,
                );
                machine_records_by_hash
                    .insert(gs.charts[player_idx].short_hash.clone(), records.clone());
                records
            };
            let machine_record_highlight_rank = find_machine_record_highlight_rank(
                machine_records.as_slice(),
                &prof.player_initials,
                score_percent,
            );
            let personal_records = scores::get_personal_leaderboard_local_for_side(
                &gs.charts[player_idx].short_hash,
                side,
                usize::MAX,
            );
            let personal_record_highlight_rank = find_machine_record_highlight_rank(
                personal_records.as_slice(),
                &prof.player_initials,
                score_percent,
            );
            let earned_machine_record = machine_record_highlight_rank
                .is_some_and(|rank| rank <= MACHINE_RECORD_ROWS as u32);
            let earned_top2_personal = personal_record_highlight_rank.is_some_and(|rank| rank <= 2);
            let show_machine_personal_split = !earned_machine_record && earned_top2_personal;

            let mut grade = if p.is_failing || !gs.song_completed_naturally {
                scores::Grade::Failed
            } else {
                scores::score_to_grade(score_percent * 10000.0)
            };

            // Per-window counts for the FA+ pane should always reflect all tap
            // judgments that occurred (including after failure), matching the
            // standard pane's judgment_counts semantics.
            let window_counts = timing_stats::compute_window_counts(notes);
            let window_counts_10ms = timing_stats::compute_window_counts_10ms_blue(notes);

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
            let hard_ex_score_percent = judgment::calculate_hard_ex_score_from_notes(
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

            let w0_enabled =
                (prof.show_fa_plus_window && prof.show_fa_plus_pane) || prof.show_ex_score;

            // Simply Love: show Quint (Grade_Tier00) if EX score is exactly 100.00
            // and we're in a mode that actually tracks/displays W0 (FA+/EX score).
            if w0_enabled && grade != scores::Grade::Failed && ex_score_percent >= 100.0 {
                grade = scores::Grade::Quint;
            }

            let col_offset = player_idx.saturating_mul(cols_per_player);
            let column_judgments = compute_column_judgments(notes, cols_per_player, col_offset);

            score_info[player_idx] = Some(ScoreInfo {
                song: gs.song.clone(),
                chart: gs.charts[player_idx].clone(),
                profile_name: prof.display_name.clone(),
                judgment_counts: p.judgment_counts.clone(),
                score_percent,
                grade,
                speed_mod: gs.scroll_speed[player_idx],
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
                scroll_option: prof.scroll_option,
                life_history: p.life_history.clone(),
                fail_time: p.fail_time,
                window_counts,
                window_counts_10ms,
                ex_score_percent,
                hard_ex_score_percent,
                column_judgments,
                noteskin,
                show_fa_plus_window: prof.show_fa_plus_window,
                show_ex_score: prof.show_ex_score,
                show_hard_ex_score: prof.show_hard_ex_score,
                show_fa_plus_pane: prof.show_fa_plus_pane,
                machine_records,
                machine_record_highlight_rank,
                personal_records,
                personal_record_highlight_rank,
                show_machine_personal_split,
            });
        }

        let play_style = profile::get_session_play_style();
        let graph_width: f32 = if play_style == profile::PlayStyle::Versus {
            300.0
        } else {
            610.0
        };

        for player_idx in 0..MAX_PLAYERS {
            let Some(si) = score_info[player_idx].as_ref() else {
                continue;
            };

            density_graph_mesh[player_idx] = {
                const GRAPH_H: f32 = 64.0;
                let last_second = si.song.total_length_seconds.max(0) as f32;
                let verts = crate::screens::components::density_graph::build_density_histogram_mesh(
                    &si.chart.measure_nps_vec,
                    si.chart.max_nps,
                    &si.chart.timing,
                    si.graph_first_second,
                    last_second,
                    graph_width,
                    GRAPH_H,
                    0.0,
                    graph_width,
                    Some(0.5),
                    0.5,
                );
                (!verts.is_empty()).then(|| Arc::from(verts.into_boxed_slice()))
            };

            scatter_mesh[player_idx] = {
                const GRAPH_H: f32 = 64.0;
                let verts = crate::screens::components::eval_graphs::build_scatter_mesh(
                    &si.scatter,
                    si.graph_first_second,
                    si.graph_last_second,
                    graph_width,
                    GRAPH_H,
                    si.scatter_worst_window_ms,
                );
                (!verts.is_empty()).then(|| Arc::from(verts.into_boxed_slice()))
            };

            timing_hist_mesh[player_idx] = {
                const PANE_W: f32 = 300.0;
                const PANE_H: f32 = 180.0;
                const TOP_H: f32 = 26.0;
                const BOT_H: f32 = 13.0;

                let graph_h = (PANE_H - TOP_H - BOT_H).max(0.0);
                let verts = crate::screens::components::eval_graphs::build_offset_histogram_mesh(
                    &si.histogram,
                    PANE_W,
                    graph_h,
                    PANE_H,
                    crate::config::get().smooth_histogram,
                );
                (!verts.is_empty()).then(|| Arc::from(verts.into_boxed_slice()))
            };
        }

        match play_style {
            profile::PlayStyle::Versus => {
                active_pane[0] = score_info[0].as_ref().map_or(EvalPane::Standard, |si| {
                    EvalPane::default_for(si.show_fa_plus_pane)
                });
                active_pane[1] = score_info[1].as_ref().map_or(EvalPane::Standard, |si| {
                    EvalPane::default_for(si.show_fa_plus_pane)
                });
            }
            profile::PlayStyle::Single | profile::PlayStyle::Double => {
                let joined = profile::get_session_player_side();
                let primary = score_info[0].as_ref().map_or(EvalPane::Standard, |si| {
                    EvalPane::default_for(si.show_fa_plus_pane)
                });
                let secondary = EvalPane::Timing;
                active_pane = match joined {
                    profile::PlayerSide::P1 => [primary, secondary],
                    profile::PlayerSide::P2 => [secondary, primary],
                };
            }
        }
    }

    State {
        active_color_index: color::DEFAULT_COLOR_INDEX, // This will be overwritten by app.rs
        bg: heart_bg::State::new(),
        screen_elapsed: 0.0,
        session_elapsed: 0.0,
        stage_duration_seconds,
        score_info,
        density_graph_mesh,
        timing_hist_mesh,
        scatter_mesh,
        density_graph_texture_key: "__white".to_string(),
        active_pane,
    }
}

// Keyboard input is handled centrally via the virtual dispatcher in app.rs

pub fn update(state: &mut State, dt: f32) {
    if dt > 0.0 {
        state.screen_elapsed += dt;
    }
}

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
fn eval_player_color_rgba(side: profile::PlayerSide, active_color_index: i32) -> [f32; 4] {
    match side {
        profile::PlayerSide::P1 => color::simply_love_rgba(active_color_index),
        profile::PlayerSide::P2 => color::simply_love_rgba(active_color_index - 2),
    }
}

#[inline(always)]
fn stage_score_10000(score_percent: f64) -> f64 {
    (score_percent * 10000.0).round()
}

#[inline(always)]
fn machine_record_rank_window(highlight_rank: Option<u32>) -> (u32, u32) {
    let mut lower: u32 = 1;
    let mut upper: u32 = MACHINE_RECORD_ROWS as u32;
    if let Some(rank) = highlight_rank
        && rank > upper
    {
        lower = lower.saturating_add(rank - upper);
        upper = rank;
    }
    (lower, upper)
}

#[inline(always)]
fn find_machine_record_highlight_rank(
    entries: &[scores::LeaderboardEntry],
    initials: &str,
    score_percent: f64,
) -> Option<u32> {
    if initials.trim().is_empty() {
        return None;
    }
    let target = stage_score_10000(score_percent);
    for entry in entries {
        if entry.name != initials {
            continue;
        }
        if (entry.score - target).abs() > 0.5 {
            continue;
        }
        return Some(entry.rank.max(1));
    }
    None
}

#[inline(always)]
fn format_machine_record_score(score_10000: f64) -> String {
    format!("{:.2}%", (score_10000 / 100.0).clamp(0.0, 100.0))
}

fn format_machine_record_date(date: &str) -> String {
    let trimmed = date.trim();
    if trimmed.is_empty() {
        return "----------".to_string();
    }

    let ymd = trimmed.split_once(' ').map_or(trimmed, |(d, _)| d);
    let mut parts = ymd.split('-');
    let (Some(year), Some(month), Some(day)) = (parts.next(), parts.next(), parts.next()) else {
        return trimmed.to_string();
    };

    let Some(month_idx) = month
        .parse::<usize>()
        .ok()
        .and_then(|m| m.checked_sub(1))
        .filter(|m| *m < MONTH_ABBR.len())
    else {
        return trimmed.to_string();
    };
    let Some(day_num) = day.parse::<u32>().ok().filter(|d| *d > 0) else {
        return trimmed.to_string();
    };

    format!("{} {}, {}", MONTH_ABBR[month_idx], day_num, year)
}

#[inline(always)]
fn machine_record_highlight_color(
    side: profile::PlayerSide,
    active_color_index: i32,
    elapsed_s: f32,
) -> [f32; 4] {
    let base = eval_player_color_rgba(side, active_color_index);
    let phase =
        ((elapsed_s / MACHINE_RECORD_HIGHLIGHT_PERIOD_SECONDS) * std::f32::consts::TAU).sin() * 0.5
            + 0.5;
    let inv = 1.0 - phase;
    [
        base[0] * inv + phase,
        base[1] * inv + phase,
        base[2] * inv + phase,
        1.0,
    ]
}

fn all_joined_players_failed(state: &State) -> bool {
    let play_style = profile::get_session_play_style();
    let side_to_idx = |side: profile::PlayerSide| match (play_style, side) {
        (profile::PlayStyle::Versus, profile::PlayerSide::P1) => 0,
        (profile::PlayStyle::Versus, profile::PlayerSide::P2) => 1,
        _ => 0,
    };

    let mut found_player = false;
    for side in [profile::PlayerSide::P1, profile::PlayerSide::P2] {
        if !profile::is_session_side_joined(side) {
            continue;
        }
        let idx = side_to_idx(side);
        let Some(score) = state.score_info.get(idx).and_then(|s| s.as_ref()) else {
            continue;
        };
        found_player = true;
        if score.grade != scores::Grade::Failed {
            return false;
        }
    }

    if found_player {
        return true;
    }

    // Fallback if join-state bookkeeping is unavailable: mirror the same
    // "any pass means CLEARED" semantics over available score entries.
    let mut any = false;
    for score in state.score_info.iter().flatten() {
        any = true;
        if score.grade != scores::Grade::Failed {
            return false;
        }
    }
    any
}

fn build_stage_in_stinger(state: &State) -> Vec<Actor> {
    if state.screen_elapsed > EVAL_STAGE_IN_TOTAL_SECONDS {
        return vec![];
    }

    let failed = all_joined_players_failed(state);
    let texture_key = if failed {
        "evaluation/failed.png"
    } else {
        "evaluation/cleared.png"
    };

    vec![
        act!(quad:
            align(0.0, 0.0): xy(0.0, 0.0):
            zoomto(screen_width(), screen_height()):
            diffuse(0.0, 0.0, 0.0, 1.0): z(1250):
            sleep(EVAL_STAGE_IN_BLACK_DELAY_SECONDS):
            linear(EVAL_STAGE_IN_BLACK_FADE_SECONDS): alpha(0.0):
            linear(0.0): visible(false)
        ),
        act!(sprite(texture_key):
            align(0.5, 0.5):
            xy(screen_center_x(), screen_center_y()):
            zoom(0.8):
            z(1251):
            alpha(0.0):
            accelerate(EVAL_STAGE_IN_TEXT_FADE_IN_SECONDS): alpha(1.0):
            sleep(EVAL_STAGE_IN_TEXT_HOLD_SECONDS):
            decelerate(EVAL_STAGE_IN_TEXT_FADE_OUT_SECONDS): alpha(0.0):
            linear(0.0): visible(false)
        ),
    ]
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

    let play_style = profile::get_session_play_style();
    let side_idx = |side: profile::PlayerSide| match side {
        profile::PlayerSide::P1 => 0,
        profile::PlayerSide::P2 => 1,
    };
    let player_idx_for_controller = |controller: profile::PlayerSide| {
        if play_style == profile::PlayStyle::Versus {
            side_idx(controller)
        } else {
            0
        }
    };
    let mut shift_pane_for = |controller: profile::PlayerSide, dir: i32| {
        let controller_idx = side_idx(controller);
        let player_idx = player_idx_for_controller(controller);
        let Some(si) = state.score_info.get(player_idx).and_then(|s| s.as_ref()) else {
            return;
        };
        let has_hard_ex = si.show_hard_ex_score;
        let gs_side = if play_style == profile::PlayStyle::Versus {
            controller
        } else {
            profile::get_session_player_side()
        };
        let has_gs = scores::get_or_fetch_player_leaderboards_for_side(
            &si.chart.short_hash,
            gs_side,
            GS_RECORD_ROWS,
        )
        .is_some();

        state.active_pane[controller_idx] = if dir >= 0 {
            state.active_pane[controller_idx].next(has_hard_ex, has_gs)
        } else {
            state.active_pane[controller_idx].prev(has_hard_ex, has_gs)
        };

        // Don't allow duplicate panes in single/double.
        if play_style != profile::PlayStyle::Versus {
            let other_idx = 1 - controller_idx;
            if state.active_pane[controller_idx] == state.active_pane[other_idx] {
                state.active_pane[controller_idx] = if dir >= 0 {
                    state.active_pane[controller_idx].next(has_hard_ex, has_gs)
                } else {
                    state.active_pane[controller_idx].prev(has_hard_ex, has_gs)
                };
            }
        }
    };

    match ev.action {
        VirtualAction::p1_back
        | VirtualAction::p1_start
        | VirtualAction::p2_back
        | VirtualAction::p2_start => ScreenAction::Navigate(Screen::SelectMusic),
        VirtualAction::p1_right | VirtualAction::p1_menu_right => {
            shift_pane_for(profile::PlayerSide::P1, 1);
            ScreenAction::None
        }
        VirtualAction::p1_left | VirtualAction::p1_menu_left => {
            shift_pane_for(profile::PlayerSide::P1, -1);
            ScreenAction::None
        }
        VirtualAction::p2_right | VirtualAction::p2_menu_right => {
            shift_pane_for(profile::PlayerSide::P2, 1);
            ScreenAction::None
        }
        VirtualAction::p2_left | VirtualAction::p2_menu_left => {
            shift_pane_for(profile::PlayerSide::P2, -1);
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

/// Builds a 300px evaluation pane for a given controller side, including judgment and radar counts.
fn build_stats_pane(
    score_info: &ScoreInfo,
    pane: EvalPane,
    controller: profile::PlayerSide,
    asset_manager: &AssetManager,
    elapsed_s: f32,
) -> Vec<Actor> {
    let mut actors = Vec::new();
    let cy = screen_center_y();

    let (pane_origin_x, side_sign) = match controller {
        profile::PlayerSide::P1 => (screen_center_x() - 155.0, 1.0_f32),
        profile::PlayerSide::P2 => (screen_center_x() + 155.0, -1.0_f32),
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

fn build_pane_percentage_display(
    score_info: &ScoreInfo,
    pane: EvalPane,
    controller: profile::PlayerSide,
) -> Vec<Actor> {
    if matches!(
        pane,
        EvalPane::Timing | EvalPane::MachineRecords | EvalPane::QrCode | EvalPane::GrooveStats
    ) {
        return vec![];
    }

    let pane_origin_x = match controller {
        profile::PlayerSide::P1 => screen_center_x() - 155.0,
        profile::PlayerSide::P2 => screen_center_x() + 155.0,
    };
    let cy = screen_center_y();

    let percent_text = format!("{:.2}", score_info.score_percent * 100.0);
    let ex_percent_text = format!("{:.2}", score_info.ex_score_percent.max(0.0));
    let hard_ex_percent_text = format!("{:.2}", score_info.hard_ex_score_percent.max(0.0));
    let score_bg_color = color::rgba_hex("#101519");

    let (bg_align_x, bg_x, percent_x) = if controller == profile::PlayerSide::P1 {
        (0.0, -150.0, 1.5)
    } else {
        (1.0, 150.0, 141.0)
    };

    let mut frame_x = pane_origin_x;
    let mut frame_y = cy - 26.0;
    let mut children: Vec<Actor> = Vec::new();

    match pane {
        EvalPane::Timing => {}
        EvalPane::MachineRecords => {}
        EvalPane::QrCode => {}
        EvalPane::GrooveStats => {}
        EvalPane::Column => {
            // Pane3 percentage container: small and not mirrored.
            frame_x = pane_origin_x - 115.0;
            frame_y = cy - 40.0;
            children.push(act!(quad:
                align(0.5, 0.5):
                xy(0.0, -2.0):
                setsize(70.0, 28.0):
                diffuse(score_bg_color[0], score_bg_color[1], score_bg_color[2], 1.0)
            ));
            children.push(act!(text:
                font("wendy_white"):
                settext(percent_text):
                align(1.0, 0.5):
                xy(30.0, -2.0):
                zoom(0.25):
                horizalign(right)
            ));
        }
        EvalPane::FaPlus => {
            children.push(act!(quad:
                align(bg_align_x, 0.5):
                xy(bg_x, 14.0):
                setsize(158.5, 88.0):
                diffuse(score_bg_color[0], score_bg_color[1], score_bg_color[2], 1.0)
            ));
            children.push(act!(text:
                font("wendy_white"):
                settext(percent_text):
                align(1.0, 0.5):
                xy(percent_x, 0.0):
                zoom(0.585):
                horizalign(right)
            ));

            let ex_color = color::JUDGMENT_RGBA[0];
            let bottom_value_x = if controller == profile::PlayerSide::P1 {
                0.0
            } else {
                percent_x
            };
            let bottom_label_x = bottom_value_x - 108.0;
            children.push(act!(text:
                font("wendy_white"):
                settext("EX"):
                align(1.0, 0.5):
                xy(bottom_label_x, 40.0):
                zoom(0.31):
                horizalign(right):
                diffuse(ex_color[0], ex_color[1], ex_color[2], ex_color[3])
            ));
            children.push(act!(text:
                font("wendy_white"):
                settext(ex_percent_text):
                align(1.0, 0.5):
                xy(bottom_value_x, 40.0):
                zoom(0.31):
                horizalign(right):
                diffuse(ex_color[0], ex_color[1], ex_color[2], ex_color[3])
            ));
        }
        EvalPane::HardEx => {
            children.push(act!(quad:
                align(bg_align_x, 0.5):
                xy(bg_x, 14.0):
                setsize(158.5, 88.0):
                diffuse(score_bg_color[0], score_bg_color[1], score_bg_color[2], 1.0)
            ));

            let ex_color = color::JUDGMENT_RGBA[0];
            let hex_color = color::HARD_EX_SCORE_RGBA;
            children.push(act!(text:
                font("wendy_white"):
                settext(ex_percent_text):
                align(1.0, 0.5):
                xy(percent_x, 0.0):
                zoom(0.585):
                horizalign(right):
                diffuse(ex_color[0], ex_color[1], ex_color[2], ex_color[3])
            ));

            let bottom_value_x = if controller == profile::PlayerSide::P1 {
                0.0
            } else {
                percent_x
            };
            let bottom_label_x = bottom_value_x - 92.0;
            children.push(act!(text:
                font("wendy_white"):
                settext("H.EX"):
                align(1.0, 0.5):
                xy(bottom_label_x, 40.0):
                zoom(0.31):
                horizalign(right):
                diffuse(hex_color[0], hex_color[1], hex_color[2], hex_color[3])
            ));
            children.push(act!(text:
                font("wendy_white"):
                settext(hard_ex_percent_text):
                align(1.0, 0.5):
                xy(bottom_value_x, 40.0):
                zoom(0.31):
                horizalign(right):
                diffuse(hex_color[0], hex_color[1], hex_color[2], hex_color[3])
            ));
        }
        EvalPane::Standard => {
            children.push(act!(quad:
                align(bg_align_x, 0.5):
                xy(bg_x, 0.0):
                setsize(158.5, 60.0):
                diffuse(score_bg_color[0], score_bg_color[1], score_bg_color[2], 1.0)
            ));
            children.push(act!(text:
                font("wendy_white"):
                settext(percent_text):
                align(1.0, 0.5):
                xy(percent_x, 0.0):
                zoom(0.585):
                horizalign(right)
            ));
        }
    }

    vec![Actor::Frame {
        align: [0.5, 0.5],
        offset: [frame_x, frame_y],
        size: [SizeSpec::Px(0.0), SizeSpec::Px(0.0)],
        background: None,
        z: 102,
        children,
    }]
}

fn build_machine_records_pane(
    score_info: &ScoreInfo,
    controller: profile::PlayerSide,
    active_color_index: i32,
    elapsed_s: f32,
) -> Vec<Actor> {
    let pane_origin_x = match controller {
        profile::PlayerSide::P1 => screen_center_x() - 155.0,
        profile::PlayerSide::P2 => screen_center_x() + 155.0,
    };
    let pane_origin_y = screen_center_y() - 62.0;
    let pane_zoom = 0.8_f32;
    let rank_x = -120.0 * pane_zoom;
    let name_x = -110.0 * pane_zoom;
    let score_x = -24.0 * pane_zoom;
    let date_x = 50.0 * pane_zoom;
    let text_zoom = pane_zoom;
    let hl = machine_record_highlight_color(controller, active_color_index, elapsed_s);

    let mut children = Vec::with_capacity(MACHINE_RECORD_ROWS * 4 + 1);

    if score_info.show_machine_personal_split {
        let row_height = MACHINE_RECORD_SPLIT_ROW_HEIGHT * pane_zoom;
        let first_row_y = row_height;
        for i in 0..MACHINE_RECORD_SPLIT_MACHINE_ROWS {
            let rank = (i as u32).saturating_add(1);
            push_machine_record_row(
                &mut children,
                score_info.machine_records.get(i),
                rank,
                first_row_y + i as f32 * row_height,
                rank_x,
                name_x,
                score_x,
                date_x,
                text_zoom,
                [1.0, 1.0, 1.0, 1.0],
            );
        }

        let split_y = first_row_y
            + MACHINE_RECORD_SPLIT_SEPARATOR_Y_ROWS * MACHINE_RECORD_SPLIT_ROW_HEIGHT * pane_zoom;
        children.push(act!(quad:
            align(0.5, 0.5):
            xy(0.0, split_y):
            setsize(100.0 * pane_zoom, 1.0 * pane_zoom):
            diffuse(1.0, 1.0, 1.0, 0.33):
            z(101)
        ));

        for i in 0..MACHINE_RECORD_SPLIT_PERSONAL_ROWS {
            let rank = (i as u32).saturating_add(1);
            let col = if score_info.personal_record_highlight_rank == Some(rank) {
                hl
            } else {
                [1.0, 1.0, 1.0, 1.0]
            };
            push_machine_record_row(
                &mut children,
                score_info.personal_records.get(i),
                rank,
                split_y + i as f32 * row_height,
                rank_x,
                name_x,
                score_x,
                date_x,
                text_zoom,
                col,
            );
        }
    } else {
        let row_height = MACHINE_RECORD_DEFAULT_ROW_HEIGHT * pane_zoom;
        let first_row_y = row_height;
        let (lower, upper) = machine_record_rank_window(score_info.machine_record_highlight_rank);
        for (row_idx, rank) in (lower..=upper).enumerate() {
            let col = if score_info.machine_record_highlight_rank == Some(rank) {
                hl
            } else {
                [1.0, 1.0, 1.0, 1.0]
            };
            push_machine_record_row(
                &mut children,
                score_info
                    .machine_records
                    .get(rank.saturating_sub(1) as usize),
                rank,
                first_row_y + row_idx as f32 * row_height,
                rank_x,
                name_x,
                score_x,
                date_x,
                text_zoom,
                col,
            );
        }
    }

    vec![Actor::Frame {
        align: [0.5, 0.5],
        offset: [pane_origin_x, pane_origin_y],
        size: [SizeSpec::Px(0.0), SizeSpec::Px(0.0)],
        background: None,
        z: 101,
        children,
    }]
}

fn push_machine_record_row(
    children: &mut Vec<Actor>,
    entry: Option<&scores::LeaderboardEntry>,
    rank: u32,
    y: f32,
    rank_x: f32,
    name_x: f32,
    score_x: f32,
    date_x: f32,
    text_zoom: f32,
    col: [f32; 4],
) {
    let (name, score, date) = if let Some(entry) = entry {
        let name = if entry.name.trim().is_empty() {
            "----".to_string()
        } else {
            entry.name.clone()
        };
        (
            name,
            format_machine_record_score(entry.score),
            format_machine_record_date(&entry.date),
        )
    } else {
        (
            "----".to_string(),
            "------".to_string(),
            "----------".to_string(),
        )
    };

    children.push(act!(text:
        font("miso"):
        settext(format!("{rank}.")):
        align(1.0, 0.5):
        xy(rank_x, y):
        zoom(text_zoom):
        z(101):
        diffuse(col[0], col[1], col[2], col[3]):
        horizalign(right)
    ));
    children.push(act!(text:
        font("miso"):
        settext(name):
        align(0.0, 0.5):
        xy(name_x, y):
        zoom(text_zoom):
        z(101):
        diffuse(col[0], col[1], col[2], col[3]):
        horizalign(left)
    ));
    children.push(act!(text:
        font("miso"):
        settext(score):
        align(0.0, 0.5):
        xy(score_x, y):
        zoom(text_zoom):
        z(101):
        diffuse(col[0], col[1], col[2], col[3]):
        horizalign(left)
    ));
    children.push(act!(text:
        font("miso"):
        settext(date):
        align(0.0, 0.5):
        xy(date_x, y):
        zoom(text_zoom):
        z(101):
        diffuse(col[0], col[1], col[2], col[3]):
        horizalign(left)
    ));
}

fn format_gs_error_text(error: &str) -> String {
    if error.eq_ignore_ascii_case("disabled") {
        return GS_ERROR_DISABLED.to_string();
    }
    let lower = error.to_ascii_lowercase();
    if lower.contains("timed out") || lower.contains("timeout") {
        GS_ERROR_TIMEOUT.to_string()
    } else {
        GS_ERROR_FAILED.to_string()
    }
}

fn gs_machine_tag(entry: &scores::LeaderboardEntry) -> String {
    if let Some(tag) = entry.machine_tag.as_deref() {
        let trimmed = tag.trim();
        if !trimmed.is_empty() {
            return trimmed
                .chars()
                .take(4)
                .collect::<String>()
                .to_ascii_uppercase();
        }
    }
    let trimmed_name = entry.name.trim();
    if trimmed_name.is_empty() {
        return GS_ROW_PLACEHOLDER_NAME.to_string();
    }
    trimmed_name
        .chars()
        .take(4)
        .collect::<String>()
        .to_ascii_uppercase()
}

fn build_gs_records_pane(
    controller: profile::PlayerSide,
    snapshot: Option<&scores::CachedPlayerLeaderboardData>,
) -> Vec<Actor> {
    let pane_origin_x = match controller {
        profile::PlayerSide::P1 => screen_center_x() - 155.0,
        profile::PlayerSide::P2 => screen_center_x() + 155.0,
    };
    let pane_origin_y = screen_center_y() - 62.0;
    let pane_zoom = 0.8_f32;
    let row_height = 22.0 * pane_zoom;
    let first_row_y = row_height;
    let rank_x = -120.0 * pane_zoom;
    let name_x = -110.0 * pane_zoom;
    let score_x = -24.0 * pane_zoom;
    let date_x = 50.0 * pane_zoom;
    let text_zoom = pane_zoom;

    let mut rows: Vec<(String, String, String, String, [f32; 4], [f32; 4])> =
        Vec::with_capacity(GS_RECORD_ROWS);

    match snapshot {
        None => {
            rows.push((
                String::new(),
                GS_ERROR_DISABLED.to_string(),
                String::new(),
                String::new(),
                [1.0, 1.0, 1.0, 1.0],
                [1.0, 1.0, 1.0, 1.0],
            ));
        }
        Some(snapshot) if snapshot.loading => {
            rows.push((
                String::new(),
                GS_LOADING_TEXT.to_string(),
                String::new(),
                String::new(),
                [1.0, 1.0, 1.0, 1.0],
                [1.0, 1.0, 1.0, 1.0],
            ));
        }
        Some(snapshot) if snapshot.error.is_some() => {
            rows.push((
                String::new(),
                format_gs_error_text(snapshot.error.as_deref().unwrap_or_default()),
                String::new(),
                String::new(),
                [1.0, 1.0, 1.0, 1.0],
                [1.0, 1.0, 1.0, 1.0],
            ));
        }
        Some(snapshot) => {
            let gs_pane = snapshot.data.as_ref().and_then(|data| {
                data.panes
                    .iter()
                    .find(|pane| pane.name.eq_ignore_ascii_case("GrooveStats"))
            });
            if let Some(pane) = gs_pane {
                if pane.entries.is_empty() {
                    rows.push((
                        String::new(),
                        GS_NO_SCORES_TEXT.to_string(),
                        String::new(),
                        String::new(),
                        [1.0, 1.0, 1.0, 1.0],
                        [1.0, 1.0, 1.0, 1.0],
                    ));
                } else {
                    for entry in pane.entries.iter().take(GS_RECORD_ROWS) {
                        let base_col = if entry.is_rival {
                            GS_RIVAL_COLOR
                        } else if entry.is_self {
                            GS_SELF_COLOR
                        } else {
                            [1.0, 1.0, 1.0, 1.0]
                        };
                        let mut score_col = if pane.is_ex {
                            color::JUDGMENT_RGBA[0]
                        } else {
                            base_col
                        };
                        if entry.is_fail {
                            score_col = [1.0, 0.0, 0.0, 1.0];
                        }
                        rows.push((
                            format!("{}.", entry.rank),
                            gs_machine_tag(entry),
                            format!("{:.2}%", entry.score / 100.0),
                            format_machine_record_date(&entry.date),
                            base_col,
                            score_col,
                        ));
                    }
                }
            } else {
                rows.push((
                    String::new(),
                    GS_NO_SCORES_TEXT.to_string(),
                    String::new(),
                    String::new(),
                    [1.0, 1.0, 1.0, 1.0],
                    [1.0, 1.0, 1.0, 1.0],
                ));
            }
        }
    }

    while rows.len() < GS_RECORD_ROWS {
        rows.push((
            GS_ROW_PLACEHOLDER_RANK.to_string(),
            GS_ROW_PLACEHOLDER_NAME.to_string(),
            GS_ROW_PLACEHOLDER_SCORE.to_string(),
            GS_ROW_PLACEHOLDER_DATE.to_string(),
            [1.0, 1.0, 1.0, 1.0],
            [1.0, 1.0, 1.0, 1.0],
        ));
    }

    let mut children = Vec::with_capacity(GS_RECORD_ROWS * 4 + 1);
    for (i, (rank, name, score, date, row_col, score_col)) in rows.into_iter().enumerate() {
        let y = first_row_y + i as f32 * row_height;
        children.push(act!(text:
            font("miso"):
            settext(rank):
            align(1.0, 0.5):
            xy(rank_x, y):
            zoom(text_zoom):
            z(101):
            diffuse(row_col[0], row_col[1], row_col[2], row_col[3]):
            horizalign(right)
        ));
        children.push(act!(text:
            font("miso"):
            settext(name):
            align(0.0, 0.5):
            xy(name_x, y):
            zoom(text_zoom):
            z(101):
            diffuse(row_col[0], row_col[1], row_col[2], row_col[3]):
            horizalign(left)
        ));
        children.push(act!(text:
            font("miso"):
            settext(score):
            align(0.0, 0.5):
            xy(score_x, y):
            zoom(text_zoom):
            z(101):
            diffuse(score_col[0], score_col[1], score_col[2], score_col[3]):
            horizalign(left)
        ));
        children.push(act!(text:
            font("miso"):
            settext(date):
            align(0.0, 0.5):
            xy(date_x, y):
            zoom(text_zoom):
            z(101):
            diffuse(row_col[0], row_col[1], row_col[2], row_col[3]):
            horizalign(left)
        ));
    }

    children.push(act!(sprite("GrooveStats.png"):
        align(0.5, 0.5):
        xy(165.0 * pane_zoom, 25.0 * pane_zoom):
        zoom(0.3 * pane_zoom):
        z(102)
    ));

    vec![Actor::Frame {
        align: [0.5, 0.5],
        offset: [pane_origin_x, pane_origin_y],
        size: [SizeSpec::Px(0.0), SizeSpec::Px(0.0)],
        background: None,
        z: 101,
        children,
    }]
}

fn build_gs_qr_pane(score_info: &ScoreInfo, controller: profile::PlayerSide) -> Vec<Actor> {
    let pane_origin_x = match controller {
        profile::PlayerSide::P1 => screen_center_x() - 155.0,
        profile::PlayerSide::P2 => screen_center_x() + 155.0,
    };
    let pane_origin_y = screen_center_y() - 62.0;
    let top_y = MACHINE_RECORD_DEFAULT_ROW_HEIGHT * 0.8;
    let score_w = 70.0;
    let score_h = 28.0;
    let score_bg = color::rgba_hex("#101519");
    let score_text = format!("{:.2}", score_info.score_percent * 100.0);

    // SL Pane7: keep a fixed left text column and dedicate the right side to the QR.
    let qr_size = 168.0;
    let qr_left = -26.0;
    let qr_top_y = top_y - 6.0;
    let qr_center_x = qr_left + qr_size * 0.5;
    let qr_center_y = qr_top_y + qr_size * 0.5;
    // SL parity: keep QR fixed and shift the full left info column as a unit.
    let left_col_x = -150.0;
    let score_y = qr_top_y - 6.0;

    let mut children = Vec::with_capacity(8);

    children.push(act!(quad:
        align(0.0, 0.0):
        xy(left_col_x, score_y):
        setsize(score_w, score_h):
        z(101):
        diffuse(score_bg[0], score_bg[1], score_bg[2], 1.0)
    ));
    children.push(act!(text:
        font("wendy_white"):
        settext(score_text):
        align(1.0, 0.5):
        xy(left_col_x + 60.0, score_y + 12.0):
        zoom(0.25):
        z(102):
        diffuse(1.0, 1.0, 1.0, 1.0):
        horizalign(right)
    ));

    let title_y = top_y + 36.0;
    children.push(act!(text:
        font("miso"):
        settext(GS_QR_TITLE):
        align(0.0, 0.0):
        xy(left_col_x + 4.0, title_y + 1.0):
        zoom(1.0):
        z(101):
        diffuse(1.0, 1.0, 1.0, 1.0)
    ));

    children.push(act!(quad:
        align(0.0, 0.0):
        xy(left_col_x + 4.0, title_y + 23.0):
        setsize(96.0, 1.0):
        z(101):
        diffuse(1.0, 1.0, 1.0, 0.33)
    ));

    children.push(act!(text:
        font("miso"):
        settext(GS_QR_HELP_TEXT):
        align(0.0, 0.0):
        xy(left_col_x + 1.0, title_y + 31.0):
        zoom(0.80):
        z(101):
        diffuse(1.0, 1.0, 1.0, 1.0)
    ));

    let qr_actors = qr_code::build(qr_code::QrCodeParams {
        content: GS_QR_URL,
        center_x: qr_center_x,
        center_y: qr_center_y,
        size: qr_size,
        border_modules: 1,
        z: 0,
    });
    if qr_actors.is_empty() {
        children.push(act!(text:
            font("miso"):
            settext(GS_QR_FALLBACK_TEXT):
            align(0.5, 0.5):
            xy(qr_center_x, qr_center_y):
            zoom(0.8):
            z(101):
            diffuse(1.0, 0.3, 0.3, 1.0):
            horizalign(center)
        ));
    } else {
        children.extend(qr_actors);
    }

    vec![Actor::Frame {
        align: [0.5, 0.5],
        offset: [pane_origin_x, pane_origin_y],
        size: [SizeSpec::Px(0.0), SizeSpec::Px(0.0)],
        background: None,
        z: 101,
        children,
    }]
}

fn build_column_judgments_pane(
    score_info: &ScoreInfo,
    controller: profile::PlayerSide,
    player_side: profile::PlayerSide,
    asset_manager: &AssetManager,
) -> Vec<Actor> {
    let num_cols = score_info.column_judgments.len();
    if num_cols == 0 {
        return vec![];
    }

    #[derive(Clone, Copy)]
    enum RowKind {
        FanCombined,
        FanW0,
        FanW1,
        Ex,
        Gr,
        Dec,
        Wo,
        Miss,
    }

    #[derive(Clone, Copy)]
    struct RowInfo {
        kind: RowKind,
        label: &'static str,
        color: [f32; 4],
    }

    let show_fa_plus_rows = score_info.show_fa_plus_window && score_info.show_fa_plus_pane;
    let rows: Vec<RowInfo> = if show_fa_plus_rows {
        vec![
            RowInfo {
                kind: RowKind::FanW0,
                label: "FANTASTIC",
                color: color::JUDGMENT_RGBA[0],
            },
            RowInfo {
                kind: RowKind::FanW1,
                label: "FANTASTIC",
                color: color::JUDGMENT_FA_PLUS_WHITE_RGBA,
            },
            RowInfo {
                kind: RowKind::Ex,
                label: "EXCELLENT",
                color: color::JUDGMENT_RGBA[1],
            },
            RowInfo {
                kind: RowKind::Gr,
                label: "GREAT",
                color: color::JUDGMENT_RGBA[2],
            },
            RowInfo {
                kind: RowKind::Dec,
                label: "DECENT",
                color: color::JUDGMENT_RGBA[3],
            },
            RowInfo {
                kind: RowKind::Wo,
                label: "WAY OFF",
                color: color::JUDGMENT_RGBA[4],
            },
            RowInfo {
                kind: RowKind::Miss,
                label: "MISS",
                color: color::JUDGMENT_RGBA[5],
            },
        ]
    } else {
        vec![
            RowInfo {
                kind: RowKind::FanCombined,
                label: "FANTASTIC",
                color: color::JUDGMENT_RGBA[0],
            },
            RowInfo {
                kind: RowKind::Ex,
                label: "EXCELLENT",
                color: color::JUDGMENT_RGBA[1],
            },
            RowInfo {
                kind: RowKind::Gr,
                label: "GREAT",
                color: color::JUDGMENT_RGBA[2],
            },
            RowInfo {
                kind: RowKind::Dec,
                label: "DECENT",
                color: color::JUDGMENT_RGBA[3],
            },
            RowInfo {
                kind: RowKind::Wo,
                label: "WAY OFF",
                color: color::JUDGMENT_RGBA[4],
            },
            RowInfo {
                kind: RowKind::Miss,
                label: "MISS",
                color: color::JUDGMENT_RGBA[5],
            },
        ]
    };

    let cy = screen_center_y();
    let pane_origin_x = match controller {
        profile::PlayerSide::P1 => screen_center_x() - 155.0,
        profile::PlayerSide::P2 => screen_center_x() + 155.0,
    };

    // Pane3 geometry (Simply Love): 230x146 box, anchored near (-104, cy-40) within the P1 pane.
    let box_width: f32 = 230.0;
    let box_height: f32 = 146.0;
    let col_width = box_width / num_cols as f32;
    let row_height = box_height / rows.len() as f32;
    let base_x = pane_origin_x - 104.0;
    let base_y = cy - 40.0;

    // Judgment label column (Simply Love): frame at (50, cy-36), labels at x=-130 for P1 and -28 for P2.
    let labels_frame_x = (if player_side == profile::PlayerSide::P1 {
        50.0_f32
    } else {
        -50.0_f32
    })
    .mul_add(1.0_f32, pane_origin_x);
    let labels_frame_y = cy - 36.0;
    let labels_right_x = labels_frame_x
        + if player_side == profile::PlayerSide::P1 {
            -130.0
        } else {
            -28.0
        };

    let mut actors = Vec::new();

    let count_for = |cj: ColumnJudgments, kind: RowKind| -> (u32, Option<u32>) {
        match kind {
            RowKind::FanCombined => (cj.w0.saturating_add(cj.w1), None),
            RowKind::FanW0 => (cj.w0, None),
            RowKind::FanW1 => (cj.w1, None),
            RowKind::Ex => (cj.w2, None),
            RowKind::Gr => (cj.w3, None),
            RowKind::Dec => (cj.w4, Some(cj.early_w4)),
            RowKind::Wo => (cj.w5, Some(cj.early_w5)),
            RowKind::Miss => (cj.miss, None),
        }
    };

    asset_manager.with_fonts(|all_fonts| {
        asset_manager.with_font("miso", |miso_font| {
            let label_zoom: f32 = 0.8;
            let number_zoom: f32 = 0.9;
            let small_zoom: f32 = 0.65;
            let held_label_zoom: f32 = 0.6;

            // Row labels
            for (row_idx, row) in rows.iter().enumerate() {
                let y = labels_frame_y + (row_idx as f32 + 1.0).mul_add(row_height, 0.0);
                actors.push(act!(text: font("miso"): settext(row.label.to_string()):
                    align(1.0, 0.5):
                    xy(labels_right_x, y):
                    zoom(label_zoom):
                    maxwidth(65.0 / label_zoom):
                    horizalign(right):
                    diffuse(row.color[0], row.color[1], row.color[2], row.color[3]):
                    z(101)
                ));
            }

            // "HELD" label at the bottom, aligned relative to the MISS label width.
            let miss_label_width =
                font::measure_line_width_logical(miso_font, "MISS", all_fonts) as f32 * label_zoom;
            let held_label_x = labels_right_x - miss_label_width / 1.15;
            let held_y = labels_frame_y + 140.0;
            let miss_color = color::JUDGMENT_RGBA[5];
            actors.push(act!(text: font("miso"): settext("HELD".to_string()):
                align(1.0, 0.5):
                xy(held_label_x, held_y):
                zoom(held_label_zoom):
                horizalign(right):
                diffuse(miss_color[0], miss_color[1], miss_color[2], miss_color[3]):
                z(101)
            ));

            // Columns: arrows + per-row counts
            for col_idx in 0..num_cols {
                let cj = score_info.column_judgments[col_idx];
                let col_center_x = (col_idx as f32 + 1.0).mul_add(col_width, base_x);

                // Measure Miss number width for this column for alignment of early/held counts.
                let miss_str = cj.miss.to_string();
                let miss_width = font::measure_line_width_logical(miso_font, &miss_str, all_fonts)
                    as f32
                    * number_zoom;
                let right_edge_x = col_center_x - 1.0 - miss_width * 0.5;

                // Noteskin preview arrow (Tap Note, Q4th) above the column.
                if let Some(ns) = score_info.noteskin.as_ref() {
                    let note_idx = col_idx
                        .saturating_mul(NUM_QUANTIZATIONS)
                        .saturating_add(Quantization::Q4th as usize);
                    if let Some(slot) = ns.notes.get(note_idx) {
                        let uv = slot.uv_for_frame(0);
                        let size = slot.size();
                        let w = size[0].max(0) as f32;
                        let h = size[1].max(0) as f32;
                        if w > 0.0 && h > 0.0 {
                            // Match gameplay arrow sizing (target 64px tall), then apply Pane3 zoom(0.4).
                            const TARGET_ARROW_PX: f32 = 64.0;
                            let (w_scaled, h_scaled) = if h > 0.0 {
                                let s = TARGET_ARROW_PX / h;
                                (w * s, TARGET_ARROW_PX)
                            } else {
                                (w, h)
                            };

                            actors.push(act!(sprite(slot.texture_key().to_string()):
                                align(0.5, 0.5):
                                xy(col_center_x, base_y):
                                setsize(w_scaled, h_scaled):
                                zoom(0.4):
                                rotationz(-slot.def.rotation_deg as f32):
                                customtexturerect(uv[0], uv[1], uv[2], uv[3]):
                                z(101)
                            ));
                        }
                    }
                }

                for (row_idx, row) in rows.iter().enumerate() {
                    let (count, early_opt) = count_for(cj, row.kind);
                    let y = labels_frame_y + (row_idx as f32 + 1.0).mul_add(row_height, 0.0);
                    actors.push(act!(text: font("miso"): settext(count.to_string()):
                        align(0.5, 0.5):
                        xy(col_center_x, y):
                        zoom(number_zoom):
                        horizalign(center):
                        z(101)
                    ));

                    if let Some(early) = early_opt {
                        let early_y = y - 10.0;
                        actors.push(act!(text: font("miso"): settext(early.to_string()):
                            align(1.0, 0.5):
                            xy(right_edge_x, early_y):
                            zoom(small_zoom):
                            horizalign(right):
                            z(101)
                        ));
                    }
                }

                // Held-miss count per column (MissBecauseHeld) at y=144, aligned like early counts.
                let held_str = cj.held_miss.to_string();
                actors.push(act!(text: font("miso"): settext(held_str):
                    align(1.0, 0.5):
                    xy(right_edge_x, base_y + 144.0):
                    zoom(small_zoom):
                    horizalign(right):
                    z(101)
                ));
            }
        })
    });

    actors
}

/// Builds the timing statistics pane (Simply Love Pane5), shown inside a 300px evaluation pane.
fn build_timing_pane(
    score_info: &ScoreInfo,
    timing_hist_mesh: Option<&Arc<[MeshVertex]>>,
    controller: profile::PlayerSide,
) -> Vec<Actor> {
    let pane_width: f32 = 300.0;
    let pane_height: f32 = 180.0;
    let topbar_height: f32 = 26.0;
    let bottombar_height: f32 = 13.0;

    let pane_origin_x = match controller {
        profile::PlayerSide::P1 => screen_center_x() - 155.0,
        profile::PlayerSide::P2 => screen_center_x() + 155.0,
    };
    let frame_x = pane_origin_x - pane_width * 0.5;
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

fn build_modifiers_pane(score_info: &ScoreInfo, bar_center_x: f32, bar_width: f32) -> Vec<Actor> {
    let frame_center_y = screen_center_y() + 200.5;
    let font_zoom = 0.7;

    // Simply Love places the modifiers text 10px from the bar's left edge.
    // (For a 300px bar this is equivalent to `center_x - 140`.)
    let text_x = bar_center_x - (bar_width * 0.5) + 10.0;
    let text_y = frame_center_y - 5.0;

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

    let bg = color::rgba_hex("#1E282F");
    vec![
        act!(quad:
            align(0.5, 0.5):
            xy(bar_center_x, frame_center_y):
            zoomto(bar_width, 26.0):
            diffuse(bg[0], bg[1], bg[2], 1.0):
            z(101)
        ),
        act!(text:
            font("miso"):
            settext(final_text):
            align(0.0, 0.0):
            xy(text_x, text_y):
            zoom(font_zoom):
            z(102):
            diffuse(1.0, 1.0, 1.0, 1.0)
        ),
    ]
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

    let play_style = profile::get_session_play_style();
    let player_side = profile::get_session_player_side();

    let Some(score_info) = state.score_info.iter().find_map(|s| s.as_ref()) else {
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
        let pane_y_top = screen_center_y() - 56.0;
        let pane_y_bottom = (screen_center_y() + 34.0) + 180.0;
        let pane_height = pane_y_bottom - pane_y_top;
        let pane_bg_color = color::rgba_hex("#1E282F");

        let pane_x_left = screen_center_x() - 305.0;
        if play_style == profile::PlayStyle::Versus {
            let pane_w = 300.0;
            let pane_x_right = screen_center_x() + 5.0;
            for x in [pane_x_left, pane_x_right] {
                actors.push(act!(quad:
                    align(0.0, 0.0):
                    xy(x, pane_y_top):
                    zoomto(pane_w, pane_height):
                    diffuse(pane_bg_color[0], pane_bg_color[1], pane_bg_color[2], 1.0):
                    z(100)
                ));
            }
        } else {
            let pane_w = 300.0_f32.mul_add(2.0, 10.0);
            actors.push(act!(quad:
                align(0.0, 0.0):
                xy(pane_x_left, pane_y_top):
                zoomto(pane_w, pane_height):
                diffuse(pane_bg_color[0], pane_bg_color[1], pane_bg_color[2], 1.0):
                z(100)
            ));
        }
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

    // --- Upper Content (Simply Love PerPlayer/Upper) ---
    {
        let style_label = match play_style {
            profile::PlayStyle::Double => "Double",
            profile::PlayStyle::Single | profile::PlayStyle::Versus => "Single",
        };

        let upper_single = [(0, player_side)];
        let upper_vs = [(0, profile::PlayerSide::P1), (1, profile::PlayerSide::P2)];
        let upper_players: &[(usize, profile::PlayerSide)] =
            if play_style == profile::PlayStyle::Versus {
                &upper_vs
            } else {
                &upper_single
            };

        for &(player_idx, side) in upper_players {
            let Some(si) = state.score_info.get(player_idx).and_then(|s| s.as_ref()) else {
                continue;
            };

            let upper_origin_x = match side {
                profile::PlayerSide::P1 => screen_center_x() - 155.0,
                profile::PlayerSide::P2 => screen_center_x() + 155.0,
            };
            let dir = if side == profile::PlayerSide::P1 {
                -1.0
            } else {
                1.0
            };

            // Letter Grade
            actors.extend(eval_grades::actors(
                si.grade,
                eval_grades::EvalGradeParams {
                    x: upper_origin_x + 70.0 * dir,
                    y: cy - 134.0,
                    z: 101,
                    zoom: 0.4,
                    elapsed: state.session_elapsed,
                },
            ));

            // Difficulty Text and Meter Block
            {
                let difficulty_display_name = if si.chart.difficulty.eq_ignore_ascii_case("edit") {
                    "Edit"
                } else {
                    let difficulty_index = color::FILE_DIFFICULTY_NAMES
                        .iter()
                        .position(|&n| n.eq_ignore_ascii_case(&si.chart.difficulty))
                        .unwrap_or(2);
                    color::DISPLAY_DIFFICULTY_NAMES[difficulty_index]
                };

                let difficulty_color =
                    color::difficulty_rgba(&si.chart.difficulty, state.active_color_index);
                let difficulty_text = format!("{style_label} / {difficulty_display_name}");
                let text_x = upper_origin_x + 115.0 * dir;
                let box_x = upper_origin_x + 134.5 * dir;
                let align_x = if side == profile::PlayerSide::P1 {
                    0.0
                } else {
                    1.0
                };

                if side == profile::PlayerSide::P1 {
                    actors.push(act!(text: font("miso"): settext(difficulty_text):
                        align(align_x, 0.5): xy(text_x, cy - 65.0): zoom(0.7): z(101):
                        diffuse(1.0, 1.0, 1.0, 1.0)
                    ));
                } else {
                    actors.push(act!(text: font("miso"): settext(difficulty_text):
                        align(align_x, 0.5): xy(text_x, cy - 65.0): zoom(0.7): z(101):
                        diffuse(1.0, 1.0, 1.0, 1.0): horizalign(right)
                    ));
                }

                actors.push(act!(quad:
                    align(0.5, 0.5):
                    xy(box_x, cy - 71.0):
                    zoomto(30.0, 30.0):
                    z(101):
                    diffuse(difficulty_color[0], difficulty_color[1], difficulty_color[2], 1.0)
                ));
                actors.push(act!(text:
                    font("wendy"):
                    settext(si.chart.meter.to_string()):
                    align(0.5, 0.5):
                    xy(box_x, cy - 71.0):
                    zoom(0.4):
                    z(102):
                    diffuse(0.0, 0.0, 0.0, 1.0)
                ));
            }

            // Step Artist (or Edit description)
            let step_artist_text = if si.chart.difficulty.eq_ignore_ascii_case("edit")
                && !si.chart.description.trim().is_empty()
            {
                si.chart.description.clone()
            } else {
                si.chart.step_artist.clone()
            };
            {
                let x = upper_origin_x + 115.0 * dir;
                let align_x = if side == profile::PlayerSide::P1 {
                    0.0
                } else {
                    1.0
                };
                if side == profile::PlayerSide::P1 {
                    actors.push(act!(text: font("miso"): settext(step_artist_text):
                        align(align_x, 0.5): xy(x, cy - 81.0): zoom(0.7): z(101):
                        diffuse(1.0, 1.0, 1.0, 1.0)
                    ));
                } else {
                    actors.push(act!(text: font("miso"): settext(step_artist_text):
                        align(align_x, 0.5): xy(x, cy - 81.0): zoom(0.7): z(101):
                        diffuse(1.0, 1.0, 1.0, 1.0): horizalign(right)
                    ));
                }
            }

            // Breakdown Text (under grade)
            let breakdown_text = {
                let chart = &si.chart;
                asset_manager
                    .with_fonts(|all_fonts| {
                        asset_manager.with_font("miso", |miso_font| -> Option<String> {
                            let width_constraint = 155.0;
                            let text_zoom = 0.7;
                            let max_allowed_logical_width = width_constraint / text_zoom;

                            let fits = |text: &str| {
                                let logical_width =
                                    font::measure_line_width_logical(miso_font, text, all_fonts)
                                        as f32;
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
                    .unwrap_or_else(|| chart.simple_breakdown.clone())
            };

            {
                let x = upper_origin_x + 150.0 * dir;
                let align_x = if side == profile::PlayerSide::P1 {
                    0.0
                } else {
                    1.0
                };
                if side == profile::PlayerSide::P1 {
                    actors.push(act!(text: font("miso"): settext(breakdown_text):
                        align(align_x, 0.5): xy(x, cy - 95.0): zoom(0.7):
                        maxwidth(155.0): horizalign(left): z(101):
                        diffuse(1.0, 1.0, 1.0, 1.0)
                    ));
                } else {
                    actors.push(act!(text: font("miso"): settext(breakdown_text):
                        align(align_x, 0.5): xy(x, cy - 95.0): zoom(0.7):
                        maxwidth(155.0): horizalign(right): z(101):
                        diffuse(1.0, 1.0, 1.0, 1.0)
                    ));
                }
            }
        }
    }

    // --- Panes (Simply Love ScreenEvaluation common/Panes) ---
    {
        for controller in [profile::PlayerSide::P1, profile::PlayerSide::P2] {
            let controller_idx = if controller == profile::PlayerSide::P1 {
                0
            } else {
                1
            };
            let player_idx = if play_style == profile::PlayStyle::Versus {
                controller_idx
            } else {
                0
            };
            let Some(si) = state.score_info.get(player_idx).and_then(|s| s.as_ref()) else {
                continue;
            };
            let pane = state.active_pane[controller_idx];
            let gs_side = if play_style == profile::PlayStyle::Versus {
                controller
            } else {
                player_side
            };

            actors.extend(build_pane_percentage_display(si, pane, controller));

            match pane {
                EvalPane::Timing => actors.extend(build_timing_pane(
                    si,
                    state.timing_hist_mesh[player_idx].as_ref(),
                    controller,
                )),
                EvalPane::QrCode => actors.extend(build_gs_qr_pane(si, controller)),
                EvalPane::GrooveStats => actors.extend(build_gs_records_pane(
                    controller,
                    scores::get_or_fetch_player_leaderboards_for_side(
                        &si.chart.short_hash,
                        gs_side,
                        GS_RECORD_ROWS,
                    )
                    .as_ref(),
                )),
                EvalPane::MachineRecords => actors.extend(build_machine_records_pane(
                    si,
                    controller,
                    state.active_color_index,
                    state.screen_elapsed,
                )),
                EvalPane::Column => {
                    let pane3_player_side = if play_style == profile::PlayStyle::Versus {
                        controller
                    } else {
                        player_side
                    };
                    actors.extend(build_column_judgments_pane(
                        si,
                        controller,
                        pane3_player_side,
                        asset_manager,
                    ));
                }
                EvalPane::Standard | EvalPane::FaPlus | EvalPane::HardEx => {
                    actors.extend(build_stats_pane(
                        si,
                        pane,
                        controller,
                        asset_manager,
                        state.screen_elapsed,
                    ));
                }
            }
        }
    }

    // --- Player Modifiers Bar (Simply Love PerPlayer/Lower/PlayerModifiers) ---
    {
        let graph_width = if play_style == profile::PlayStyle::Versus {
            300.0
        } else {
            610.0
        };

        if play_style == profile::PlayStyle::Versus {
            for (player_idx, center_x) in [
                (0, screen_center_x() - 155.0),
                (1, screen_center_x() + 155.0),
            ] {
                if let Some(si) = state.score_info.get(player_idx).and_then(|s| s.as_ref()) {
                    actors.extend(build_modifiers_pane(si, center_x, graph_width));
                }
            }
        } else if let Some(si) = state.score_info.get(0).and_then(|s| s.as_ref()) {
            actors.extend(build_modifiers_pane(si, screen_center_x(), graph_width));
        }
    }

    // --- Graphs (density + scatter + life) ---
    {
        let graph_width = if play_style == profile::PlayStyle::Versus {
            300.0
        } else {
            610.0
        };
        let graph_height = 64.0_f32;
        let frame_center_y = screen_center_y() + 124.0;

        let cx = screen_center_x();
        let graph_single = [(0, cx)];
        let graph_vs = [(0, cx - 155.0), (1, cx + 155.0)];
        let graph_players: &[(usize, f32)] = if play_style == profile::PlayStyle::Versus {
            &graph_vs
        } else {
            &graph_single
        };

        for &(player_idx, frame_center_x) in graph_players {
            let Some(si) = state.score_info.get(player_idx).and_then(|s| s.as_ref()) else {
                continue;
            };

            let density_mesh = state.density_graph_mesh[player_idx].as_ref();
            let scatter_mesh = state.scatter_mesh[player_idx].as_ref();

            let graph_frame = Actor::Frame {
                align: [0.5, 0.0],
                offset: [frame_center_x, frame_center_y],
                size: [SizeSpec::Px(graph_width), SizeSpec::Px(graph_height)],
                z: 101,
                background: None,
                children: vec![
                    act!(quad:
                        align(0.0, 0.0):
                        xy(0.0, 0.0):
                        setsize(graph_width, graph_height):
                        diffuse(16.0/255.0, 21.0/255.0, 25.0/255.0, 1.0):
                        z(0)
                    ),
                    {
                        if let Some(mesh) = density_mesh
                            && !mesh.is_empty()
                        {
                            Actor::Mesh {
                                align: [0.0, 1.0],
                                offset: [0.0, graph_height],
                                size: [SizeSpec::Px(graph_width), SizeSpec::Px(graph_height)],
                                vertices: mesh.clone(),
                                mode: MeshMode::Triangles,
                                visible: true,
                                blend: BlendMode::Alpha,
                                z: 1,
                            }
                        } else if state.density_graph_texture_key != "__white" {
                            act!(sprite(state.density_graph_texture_key.clone()):
                                align(0.0, 1.0):
                                xy(0.0, graph_height):
                                setsize(graph_width, graph_height): z(1)
                            )
                        } else {
                            act!(sprite("__white"): visible(false))
                        }
                    },
                    act!(quad:
                        align(0.5, 0.5):
                        xy(graph_width / 2.0_f32, graph_height / 2.0_f32):
                        setsize(graph_width, 1.0):
                        diffusealpha(0.1):
                        z(2)
                    ),
                    {
                        if let Some(mesh) = scatter_mesh
                            && !mesh.is_empty()
                        {
                            Actor::Mesh {
                                align: [0.0, 0.0],
                                offset: [0.0, 0.0],
                                size: [SizeSpec::Px(graph_width), SizeSpec::Px(graph_height)],
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
                                size: [SizeSpec::Px(graph_width), SizeSpec::Px(graph_height)],
                                background: None,
                                z: 3,
                                children: Vec::new(),
                            }
                        }
                    },
                    {
                        let mut life_children: Vec<Actor> = Vec::new();
                        let first = si.graph_first_second;
                        let last = si.graph_last_second.max(first + 0.001_f32);
                        let dur = (last - first).max(0.001_f32);
                        let padding = 0.05;

                        let mut last_x = -999.0_f32;
                        let mut last_y = -999.0_f32;

                        for &(t, life) in &si.life_history {
                            let x = ((t - first) / (dur + padding)).clamp(0.0, 1.0) * graph_width;
                            let y = (1.0 - life).clamp(0.0, 1.0) * graph_height;

                            if (x - last_x).abs() < 0.5 && (y - last_y).abs() < 0.5 {
                                continue;
                            }

                            if last_x > -900.0 {
                                let w = (x - last_x).max(0.0);
                                if w > 0.5 {
                                    life_children.push(act!(quad:
                                        align(0.0, 0.5): xy(last_x, last_y):
                                        setsize(w, 2.0):
                                        diffuse(1.0, 1.0, 1.0, 0.8):
                                        z(4)
                                    ));
                                }

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

                        if let Some(fail_time) = si.fail_time {
                            let x = ((fail_time - first) / (dur + padding)).clamp(0.0, 1.0)
                                * graph_width;

                            life_children.push(act!(quad:
                                align(0.5, 0.0): xy(x, 0.0):
                                setsize(1.5, graph_height):
                                diffuse(1.0, 0.0, 0.0, 0.8):
                                z(5)
                            ));

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

                            let flag_w = 40.0;
                            let flag_h = 14.0;
                            life_children.push(act!(quad:
                                align(1.0, 1.0): xy(x, graph_height):
                                setsize(flag_w, flag_h):
                                diffuse(1.0, 0.0, 0.0, 1.0):
                                z(5)
                            ));
                            life_children.push(act!(quad:
                                align(1.0, 1.0): xy(x - 1.0, graph_height - 1.0):
                                setsize(flag_w - 2.0, flag_h - 2.0):
                                diffuse(0.0, 0.0, 0.0, 0.8):
                                z(6)
                            ));
                            life_children.push(act!(text:
                                font("miso"): settext(remaining_str):
                                align(1.0, 1.0): xy(x - 4.0, graph_height - 1.5):
                                zoom(0.5):
                                diffuse(1.0, 0.3, 0.3, 1.0):
                                z(7)
                            ));
                        }

                        Actor::Frame {
                            align: [0.0, 0.0],
                            offset: [0.0, 0.0],
                            size: [SizeSpec::Px(graph_width), SizeSpec::Px(graph_height)],
                            background: None,
                            z: 4,
                            children: life_children,
                        }
                    },
                ],
            };
            actors.push(graph_frame);
        }
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

    // ScreenEvaluationStage in stinger (standard Simply Love visual style).
    actors.extend(build_stage_in_stinger(state));

    actors
}
