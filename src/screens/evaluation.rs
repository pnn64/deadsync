use crate::act;
use crate::core::gfx::{BlendMode, MeshMode, MeshVertex};
use crate::core::space::widescale;
use crate::core::space::{screen_center_x, screen_center_y, screen_height, screen_width};
use crate::screens::Screen;
use crate::screens::components::screen_bar::{
    AvatarParams, ScreenBarParams, ScreenBarPosition, ScreenBarTitlePlacement,
};
use crate::screens::components::{
    eval_grades, evaluation as eval_panes, heart_bg, pad_display, screen_bar, select_shared,
};
use crate::ui::actors::{Actor, SizeSpec};
use crate::ui::color;

use crate::assets::AssetManager;
use crate::game::chart::ChartData;
use crate::game::gameplay::MAX_PLAYERS;
use crate::game::judgment::{self, JudgeGrade};
use crate::game::note::NoteType;
use crate::game::parsing::noteskin::Noteskin;
use crate::game::scores;
use crate::game::scroll::ScrollSpeedSetting;
use crate::game::song::SongData;
use crate::game::timing as timing_stats;
use crate::screens::gameplay;
use crate::ui::font;
use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Arc;
use std::thread::LocalKey;

use crate::core::input::{InputEvent, VirtualAction};
use crate::game::profile;
use crate::screens::ScreenAction;
// Keyboard handling is centralized in app.rs via virtual actions
use chrono::Local;

/* ---------------------------- transitions ---------------------------- */
const TRANSITION_IN_DURATION: f32 = 0.4;
const TRANSITION_OUT_DURATION: f32 = 0.4;
// Simply Love ScreenEvaluationStage in/default.lua (non-SRPG9 branch)
const EVAL_STAGE_IN_BLACK_DELAY_SECONDS: f32 = 0.2;
const EVAL_STAGE_IN_BLACK_FADE_SECONDS: f32 = 0.5;
const EVAL_STAGE_IN_TEXT_FADE_IN_SECONDS: f32 = 0.4;
const EVAL_STAGE_IN_TEXT_HOLD_SECONDS: f32 = 0.6;
const EVAL_STAGE_IN_TEXT_FADE_OUT_SECONDS: f32 = 0.4;
const EVAL_STAGE_IN_TOTAL_SECONDS: f32 = EVAL_STAGE_IN_TEXT_FADE_IN_SECONDS
    + EVAL_STAGE_IN_TEXT_HOLD_SECONDS
    + EVAL_STAGE_IN_TEXT_FADE_OUT_SECONDS;
const GRAPH_BARELY_SAMPLE_COUNT: usize = 100;
const GRAPH_BARELY_LIFE_MAX: f32 = 0.1;
const GRAPH_BARELY_ANIM_DELAY_SECONDS: f32 = 2.0;
const GRAPH_BARELY_ANIM_SEG_SECONDS: f32 = 0.2;
const GRAPH_BARELY_ARROW_PULSE_DELAY_SECONDS: f32 = 0.5;
const MACHINE_RECORD_ROWS: usize = 10;
const GS_RECORD_ROWS: usize = 10;
const TEXT_CACHE_LIMIT: usize = 8192;
const BANNER_FALLBACK_KEYS: [&str; 12] = [
    "banner1.png",
    "banner2.png",
    "banner3.png",
    "banner4.png",
    "banner5.png",
    "banner6.png",
    "banner7.png",
    "banner8.png",
    "banner9.png",
    "banner10.png",
    "banner11.png",
    "banner12.png",
];

type TextCache<K> = HashMap<K, Arc<str>>;

thread_local! {
    static SESSION_TIME_CACHE: RefCell<TextCache<u32>> = RefCell::new(HashMap::with_capacity(2048));
    static BPM_TEXT_CACHE: RefCell<TextCache<(i32, i32, u32)>> = RefCell::new(HashMap::with_capacity(1024));
    static SONG_LENGTH_CACHE: RefCell<TextCache<i32>> = RefCell::new(HashMap::with_capacity(2048));
    static RECORD_TEXT_CACHE: RefCell<TextCache<(u32, u8)>> = RefCell::new(HashMap::with_capacity(256));
    static DIFFICULTY_TEXT_CACHE: RefCell<TextCache<(&'static str, &'static str)>> = RefCell::new(HashMap::with_capacity(64));
    static REMAINING_TIME_CACHE: RefCell<TextCache<u32>> = RefCell::new(HashMap::with_capacity(2048));
    static TOTAL_LABEL_CACHE: RefCell<TextCache<u32>> = RefCell::new(HashMap::with_capacity(512));
    static STR_REF_CACHE: RefCell<TextCache<(usize, usize)>> = RefCell::new(HashMap::with_capacity(4096));
}

#[inline(always)]
fn cached_text<K, F>(cache: &'static LocalKey<RefCell<TextCache<K>>>, key: K, build: F) -> Arc<str>
where
    K: Copy + Eq + std::hash::Hash,
    F: FnOnce() -> String,
{
    cache.with(|cache| {
        let mut cache = cache.borrow_mut();
        if let Some(text) = cache.get(&key) {
            return text.clone();
        }
        let text: Arc<str> = Arc::<str>::from(build());
        if cache.len() < TEXT_CACHE_LIMIT {
            cache.insert(key, text.clone());
        }
        text
    })
}

#[inline(always)]
fn cached_bpm_text(min_bpm: f64, max_bpm: f64, music_rate: f32) -> Arc<str> {
    let rate = if music_rate.is_finite() {
        music_rate
    } else {
        1.0
    };
    let rate_f64 = f64::from(rate);
    let min = (min_bpm * rate_f64).round() as i32;
    let max = (max_bpm * rate_f64).round() as i32;
    cached_text(&BPM_TEXT_CACHE, (min, max, rate.to_bits()), || {
        let base = if min == max {
            format!("{min} bpm")
        } else {
            format!("{min} - {max} bpm")
        };
        if (rate - 1.0).abs() > 0.001 {
            format!("{base} ({rate:.2}x Music Rate)")
        } else {
            base
        }
    })
}

#[inline(always)]
fn cached_song_length_text(seconds: i32) -> Arc<str> {
    let key = seconds.max(0);
    cached_text(&SONG_LENGTH_CACHE, key, || {
        if key >= 3600 {
            format!("{}:{:02}:{:02}", key / 3600, (key % 3600) / 60, key % 60)
        } else {
            format!("{}:{:02}", key / 60, key % 60)
        }
    })
}

#[inline(always)]
fn cached_record_text(is_machine: bool, rank: u32) -> Arc<str> {
    cached_text(
        &RECORD_TEXT_CACHE,
        (rank, if is_machine { 0 } else { 1 }),
        || {
            if is_machine {
                format!("Machine Record {rank}")
            } else {
                format!("Personal Record {rank}")
            }
        },
    )
}

#[inline(always)]
fn cached_difficulty_text(style_label: &'static str, difficulty: &'static str) -> Arc<str> {
    cached_text(&DIFFICULTY_TEXT_CACHE, (style_label, difficulty), || {
        format!("{style_label} / {difficulty}")
    })
}

#[inline(always)]
fn cached_total_label_text(total: u32) -> Arc<str> {
    cached_text(&TOTAL_LABEL_CACHE, total, || {
        let mut s = total.to_string();
        s.push_str(" Total");
        s
    })
}

#[inline(always)]
fn cached_str_ref(text: &str) -> Arc<str> {
    let key = (text.as_ptr() as usize, text.len());
    cached_text(&STR_REF_CACHE, key, || text.to_owned())
}

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
pub(crate) enum EvalPane {
    Standard,
    FaPlus,
    HardEx,
    Column,
    MachineRecords,
    QrCode,
    GrooveStats,
    Timing,
    TimingEx,
    TimingHardEx,
}

#[inline(always)]
const fn eval_pane_default_for(show_fa_plus_pane: bool) -> EvalPane {
    if show_fa_plus_pane {
        EvalPane::FaPlus
    } else {
        EvalPane::Standard
    }
}

#[inline(always)]
fn eval_pane_next(
    pane: EvalPane,
    has_hard_ex: bool,
    has_online_panes: bool,
    has_gs: bool,
) -> EvalPane {
    // Order (per user parity request):
    // ITG -> EX -> H.EX -> Arrow breakdown -> Machine -> QR -> GS -> Timing -> Timing EX -> Timing H.EX -> ITG
    match (pane, has_hard_ex, has_online_panes, has_gs) {
        (EvalPane::Standard, _, _, _) => EvalPane::FaPlus,
        (EvalPane::FaPlus, true, _, _) => EvalPane::HardEx,
        (EvalPane::FaPlus, false, _, _) => EvalPane::Column,
        (EvalPane::HardEx, true, _, _) => EvalPane::Column,
        (EvalPane::HardEx, false, _, _) => EvalPane::Standard,
        (EvalPane::Column, _, _, _) => EvalPane::MachineRecords,
        (EvalPane::MachineRecords, _, true, _) => EvalPane::QrCode,
        (EvalPane::MachineRecords, _, false, _) => EvalPane::Timing,
        (EvalPane::QrCode, _, true, true) => EvalPane::GrooveStats,
        (EvalPane::QrCode, _, true, false) => EvalPane::Timing,
        (EvalPane::QrCode, _, false, _) => EvalPane::Timing,
        (EvalPane::GrooveStats, _, _, _) => EvalPane::Timing,
        (EvalPane::Timing, _, _, _) => EvalPane::TimingEx,
        (EvalPane::TimingEx, true, _, _) => EvalPane::TimingHardEx,
        (EvalPane::TimingEx, false, _, _) => EvalPane::Standard,
        (EvalPane::TimingHardEx, _, _, _) => EvalPane::Standard,
    }
}

#[inline(always)]
fn eval_pane_prev(
    pane: EvalPane,
    has_hard_ex: bool,
    has_online_panes: bool,
    has_gs: bool,
) -> EvalPane {
    match (pane, has_hard_ex, has_online_panes, has_gs) {
        (EvalPane::Standard, true, _, _) => EvalPane::TimingHardEx,
        (EvalPane::Standard, false, _, _) => EvalPane::TimingEx,
        (EvalPane::TimingHardEx, _, _, _) => EvalPane::TimingEx,
        (EvalPane::TimingEx, _, _, _) => EvalPane::Timing,
        (EvalPane::Timing, _, true, true) => EvalPane::GrooveStats,
        (EvalPane::Timing, _, true, false) => EvalPane::QrCode,
        (EvalPane::Timing, _, false, _) => EvalPane::MachineRecords,
        (EvalPane::GrooveStats, _, true, _) => EvalPane::QrCode,
        (EvalPane::GrooveStats, _, false, _) => EvalPane::MachineRecords,
        (EvalPane::QrCode, _, _, _) => EvalPane::MachineRecords,
        (EvalPane::MachineRecords, _, _, _) => EvalPane::Column,
        (EvalPane::Column, true, _, _) => EvalPane::HardEx,
        (EvalPane::Column, false, _, _) => EvalPane::FaPlus,
        (EvalPane::HardEx, true, _, _) => EvalPane::FaPlus,
        (EvalPane::HardEx, false, _, _) => EvalPane::Standard,
        (EvalPane::FaPlus, _, _, _) => EvalPane::Standard,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EvalGraphPane {
    Itg,
    Ex,
    HardEx,
    Arrow,
    Foot,
}

#[inline(always)]
const fn eval_graph_next(pane: EvalGraphPane) -> EvalGraphPane {
    match pane {
        EvalGraphPane::Itg => EvalGraphPane::Ex,
        EvalGraphPane::Ex => EvalGraphPane::HardEx,
        EvalGraphPane::HardEx => EvalGraphPane::Arrow,
        EvalGraphPane::Arrow => EvalGraphPane::Foot,
        EvalGraphPane::Foot => EvalGraphPane::Itg,
    }
}

#[inline(always)]
const fn eval_graph_prev(pane: EvalGraphPane) -> EvalGraphPane {
    match pane {
        EvalGraphPane::Itg => EvalGraphPane::Foot,
        EvalGraphPane::Ex => EvalGraphPane::Itg,
        EvalGraphPane::HardEx => EvalGraphPane::Ex,
        EvalGraphPane::Arrow => EvalGraphPane::HardEx,
        EvalGraphPane::Foot => EvalGraphPane::Arrow,
    }
}

#[derive(Clone)]
pub struct State {
    pub active_color_index: i32,
    bg: heart_bg::State,
    pub screen_elapsed: f32,
    pub session_elapsed: f32, // To display the timer
    pub gameplay_elapsed: f32,
    pub stage_duration_seconds: f32,
    pub score_info: [Option<ScoreInfo>; MAX_PLAYERS],
    pub density_graph_mesh: [Option<Arc<[MeshVertex]>>; MAX_PLAYERS],
    pub timing_hist_mesh: [Option<Arc<[MeshVertex]>>; MAX_PLAYERS],
    pub timing_hist_mesh_ex: [Option<Arc<[MeshVertex]>>; MAX_PLAYERS],
    pub timing_hist_mesh_hard_ex: [Option<Arc<[MeshVertex]>>; MAX_PLAYERS],
    pub scatter_mesh_itg: [Option<Arc<[MeshVertex]>>; MAX_PLAYERS],
    pub scatter_mesh_ex: [Option<Arc<[MeshVertex]>>; MAX_PLAYERS],
    pub scatter_mesh_hard_ex: [Option<Arc<[MeshVertex]>>; MAX_PLAYERS],
    pub scatter_mesh_arrow: [Option<Arc<[MeshVertex]>>; MAX_PLAYERS],
    pub scatter_mesh_foot: [Option<Arc<[MeshVertex]>>; MAX_PLAYERS],
    pub density_graph_texture_key: String,
    pub return_to_course: bool,
    pub auto_advance_seconds: Option<f32>,
    pub allow_online_panes: bool,
    active_pane: [EvalPane; MAX_PLAYERS],
    active_graph: [EvalGraphPane; MAX_PLAYERS],
}

pub fn init(gameplay_results: Option<gameplay::State>) -> State {
    let mut score_info: [Option<ScoreInfo>; MAX_PLAYERS] = std::array::from_fn(|_| None);
    let mut density_graph_mesh: [Option<Arc<[MeshVertex]>>; MAX_PLAYERS] =
        std::array::from_fn(|_| None);
    let mut timing_hist_mesh: [Option<Arc<[MeshVertex]>>; MAX_PLAYERS] =
        std::array::from_fn(|_| None);
    let mut timing_hist_mesh_ex: [Option<Arc<[MeshVertex]>>; MAX_PLAYERS] =
        std::array::from_fn(|_| None);
    let mut timing_hist_mesh_hard_ex: [Option<Arc<[MeshVertex]>>; MAX_PLAYERS] =
        std::array::from_fn(|_| None);
    let mut scatter_mesh_itg: [Option<Arc<[MeshVertex]>>; MAX_PLAYERS] =
        std::array::from_fn(|_| None);
    let mut scatter_mesh_ex: [Option<Arc<[MeshVertex]>>; MAX_PLAYERS] =
        std::array::from_fn(|_| None);
    let mut scatter_mesh_hard_ex: [Option<Arc<[MeshVertex]>>; MAX_PLAYERS] =
        std::array::from_fn(|_| None);
    let mut scatter_mesh_arrow: [Option<Arc<[MeshVertex]>>; MAX_PLAYERS] =
        std::array::from_fn(|_| None);
    let mut scatter_mesh_foot: [Option<Arc<[MeshVertex]>>; MAX_PLAYERS] =
        std::array::from_fn(|_| None);
    let mut active_pane: [EvalPane; MAX_PLAYERS] = [EvalPane::Standard; MAX_PLAYERS];
    let active_graph: [EvalGraphPane; MAX_PLAYERS] = [EvalGraphPane::Itg; MAX_PLAYERS];
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
            let col_offset = player_idx.saturating_mul(cols_per_player);

            // Compute timing statistics across all non-miss tap judgments
            let stats = timing_stats::compute_note_timing_stats(notes);
            // Prepare scatter points and histogram bins
            let scatter = timing_stats::build_scatter_points(
                notes,
                note_times,
                col_offset,
                cols_per_player,
                &gs.mini_indicator_stream_segments[player_idx],
            );
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

            let score_percent = judgment::calculate_itg_score_percent_from_counts(
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

            let column_judgments = compute_column_judgments(notes, cols_per_player, col_offset);

            score_info[player_idx] = Some(ScoreInfo {
                song: gs.song.clone(),
                chart: gs.charts[player_idx].clone(),
                profile_name: prof.display_name.clone(),
                judgment_counts: HashMap::from([
                    (
                        JudgeGrade::Fantastic,
                        p.judgment_counts[judgment::judge_grade_ix(JudgeGrade::Fantastic)],
                    ),
                    (
                        JudgeGrade::Excellent,
                        p.judgment_counts[judgment::judge_grade_ix(JudgeGrade::Excellent)],
                    ),
                    (
                        JudgeGrade::Great,
                        p.judgment_counts[judgment::judge_grade_ix(JudgeGrade::Great)],
                    ),
                    (
                        JudgeGrade::Decent,
                        p.judgment_counts[judgment::judge_grade_ix(JudgeGrade::Decent)],
                    ),
                    (
                        JudgeGrade::WayOff,
                        p.judgment_counts[judgment::judge_grade_ix(JudgeGrade::WayOff)],
                    ),
                    (
                        JudgeGrade::Miss,
                        p.judgment_counts[judgment::judge_grade_ix(JudgeGrade::Miss)],
                    ),
                ]),
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

            scatter_mesh_itg[player_idx] = {
                const GRAPH_H: f32 = 64.0;
                let verts = crate::screens::components::eval_graphs::build_scatter_mesh(
                    &si.scatter,
                    si.graph_first_second,
                    si.graph_last_second,
                    graph_width,
                    GRAPH_H,
                    si.scatter_worst_window_ms,
                    crate::screens::components::eval_graphs::ScatterPlotScale::Itg,
                );
                (!verts.is_empty()).then(|| Arc::from(verts.into_boxed_slice()))
            };

            scatter_mesh_ex[player_idx] = {
                const GRAPH_H: f32 = 64.0;
                let verts = crate::screens::components::eval_graphs::build_scatter_mesh(
                    &si.scatter,
                    si.graph_first_second,
                    si.graph_last_second,
                    graph_width,
                    GRAPH_H,
                    si.scatter_worst_window_ms,
                    crate::screens::components::eval_graphs::ScatterPlotScale::Ex,
                );
                (!verts.is_empty()).then(|| Arc::from(verts.into_boxed_slice()))
            };

            scatter_mesh_hard_ex[player_idx] = {
                const GRAPH_H: f32 = 64.0;
                let verts = crate::screens::components::eval_graphs::build_scatter_mesh(
                    &si.scatter,
                    si.graph_first_second,
                    si.graph_last_second,
                    graph_width,
                    GRAPH_H,
                    si.scatter_worst_window_ms,
                    crate::screens::components::eval_graphs::ScatterPlotScale::HardEx,
                );
                (!verts.is_empty()).then(|| Arc::from(verts.into_boxed_slice()))
            };

            scatter_mesh_arrow[player_idx] = {
                const GRAPH_H: f32 = 64.0;
                let verts = crate::screens::components::eval_graphs::build_scatter_mesh(
                    &si.scatter,
                    si.graph_first_second,
                    si.graph_last_second,
                    graph_width,
                    GRAPH_H,
                    si.scatter_worst_window_ms,
                    crate::screens::components::eval_graphs::ScatterPlotScale::Arrow,
                );
                (!verts.is_empty()).then(|| Arc::from(verts.into_boxed_slice()))
            };

            scatter_mesh_foot[player_idx] = {
                const GRAPH_H: f32 = 64.0;
                let verts = crate::screens::components::eval_graphs::build_scatter_mesh(
                    &si.scatter,
                    si.graph_first_second,
                    si.graph_last_second,
                    graph_width,
                    GRAPH_H,
                    si.scatter_worst_window_ms,
                    crate::screens::components::eval_graphs::ScatterPlotScale::Foot,
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
                    crate::screens::components::eval_graphs::TimingHistogramScale::Itg,
                    crate::config::get().smooth_histogram,
                );
                (!verts.is_empty()).then(|| Arc::from(verts.into_boxed_slice()))
            };

            timing_hist_mesh_ex[player_idx] = {
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
                    crate::screens::components::eval_graphs::TimingHistogramScale::Ex,
                    crate::config::get().smooth_histogram,
                );
                (!verts.is_empty()).then(|| Arc::from(verts.into_boxed_slice()))
            };

            timing_hist_mesh_hard_ex[player_idx] = {
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
                    crate::screens::components::eval_graphs::TimingHistogramScale::HardEx,
                    crate::config::get().smooth_histogram,
                );
                (!verts.is_empty()).then(|| Arc::from(verts.into_boxed_slice()))
            };
        }

        match play_style {
            profile::PlayStyle::Versus => {
                active_pane[0] = score_info[0]
                    .as_ref()
                    .map_or(EvalPane::Standard, |si| eval_pane_default_for(si.show_fa_plus_pane));
                active_pane[1] = score_info[1]
                    .as_ref()
                    .map_or(EvalPane::Standard, |si| eval_pane_default_for(si.show_fa_plus_pane));
            }
            profile::PlayStyle::Single | profile::PlayStyle::Double => {
                let joined = profile::get_session_player_side();
                let primary = score_info[0]
                    .as_ref()
                    .map_or(EvalPane::Standard, |si| eval_pane_default_for(si.show_fa_plus_pane));
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
        gameplay_elapsed: 0.0,
        stage_duration_seconds,
        score_info,
        density_graph_mesh,
        timing_hist_mesh,
        timing_hist_mesh_ex,
        timing_hist_mesh_hard_ex,
        scatter_mesh_itg,
        scatter_mesh_ex,
        scatter_mesh_hard_ex,
        scatter_mesh_arrow,
        scatter_mesh_foot,
        density_graph_texture_key: "__white".to_string(),
        return_to_course: false,
        auto_advance_seconds: None,
        allow_online_panes: true,
        active_pane,
        active_graph,
    }
}

pub fn init_from_score_info(
    score_info: [Option<ScoreInfo>; MAX_PLAYERS],
    stage_duration_seconds: f32,
) -> State {
    let mut active_pane: [EvalPane; MAX_PLAYERS] = [EvalPane::Standard; MAX_PLAYERS];
    let active_graph: [EvalGraphPane; MAX_PLAYERS] = [EvalGraphPane::Itg; MAX_PLAYERS];
    let play_style = profile::get_session_play_style();
    match play_style {
        profile::PlayStyle::Versus => {
            active_pane[0] = score_info[0]
                .as_ref()
                .map_or(EvalPane::Standard, |si| eval_pane_default_for(si.show_fa_plus_pane));
            active_pane[1] = score_info[1]
                .as_ref()
                .map_or(EvalPane::Standard, |si| eval_pane_default_for(si.show_fa_plus_pane));
        }
        profile::PlayStyle::Single | profile::PlayStyle::Double => {
            let joined = profile::get_session_player_side();
            let primary = score_info[0]
                .as_ref()
                .map_or(EvalPane::Standard, |si| eval_pane_default_for(si.show_fa_plus_pane));
            let secondary = EvalPane::Timing;
            active_pane = match joined {
                profile::PlayerSide::P1 => [primary, secondary],
                profile::PlayerSide::P2 => [secondary, primary],
            };
        }
    }

    State {
        active_color_index: color::DEFAULT_COLOR_INDEX,
        bg: heart_bg::State::new(),
        screen_elapsed: 0.0,
        session_elapsed: 0.0,
        gameplay_elapsed: 0.0,
        stage_duration_seconds,
        score_info,
        density_graph_mesh: std::array::from_fn(|_| None),
        timing_hist_mesh: std::array::from_fn(|_| None),
        timing_hist_mesh_ex: std::array::from_fn(|_| None),
        timing_hist_mesh_hard_ex: std::array::from_fn(|_| None),
        scatter_mesh_itg: std::array::from_fn(|_| None),
        scatter_mesh_ex: std::array::from_fn(|_| None),
        scatter_mesh_hard_ex: std::array::from_fn(|_| None),
        scatter_mesh_arrow: std::array::from_fn(|_| None),
        scatter_mesh_foot: std::array::from_fn(|_| None),
        density_graph_texture_key: "__white".to_string(),
        return_to_course: false,
        auto_advance_seconds: None,
        allow_online_panes: true,
        active_pane,
        active_graph,
    }
}

// Keyboard input is handled centrally via the virtual dispatcher in app.rs

pub fn update(state: &mut State, dt: f32) {
    if dt > 0.0 {
        state.screen_elapsed += dt;
    }
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

fn format_session_time(seconds_total: f32) -> Arc<str> {
    let seconds_total = if !seconds_total.is_finite() || seconds_total < 0.0 {
        0_u64
    } else {
        seconds_total as u64
    };
    let key = seconds_total.min(u32::MAX as u64) as u32;
    cached_text(&SESSION_TIME_CACHE, key, || {
        let hours = seconds_total / 3600;
        let minutes = (seconds_total % 3600) / 60;
        let seconds = seconds_total % 60;
        if seconds_total < 3600 {
            format!("{minutes:02}:{seconds:02}")
        } else if seconds_total < 36000 {
            format!("{hours}:{minutes:02}:{seconds:02}")
        } else {
            format!("{hours:02}:{minutes:02}:{seconds:02}")
        }
    })
}

#[inline(always)]
fn cached_remaining_time_text(seconds_total: f32) -> Arc<str> {
    let seconds_total = if !seconds_total.is_finite() || seconds_total < 0.0 {
        0_u64
    } else {
        seconds_total as u64
    };
    let key = seconds_total.min(u32::MAX as u64) as u32;
    cached_text(&REMAINING_TIME_CACHE, key, || {
        if seconds_total >= 3600 {
            format!(
                "{}:{:02}:{:02}",
                seconds_total / 3600,
                (seconds_total % 3600) / 60,
                seconds_total % 60
            )
        } else {
            format!("{}:{:02}", seconds_total / 60, seconds_total % 60)
        }
    })
}

#[inline(always)]
fn life_record_lerp_at(life_history: &[(f32, f32)], sample_time: f32) -> f32 {
    let Some(&(_, first_life)) = life_history.first() else {
        return 0.0;
    };
    if life_history.len() == 1 {
        return first_life.clamp(0.0, 1.0);
    }

    // Match ITGmania's PlayerStageStats::GetLifeRecordLerpAt() upper_bound behavior:
    // choose the first key > sample_time, then lerp from the previous sample.
    let later_ix = life_history.partition_point(|&(t, _)| t <= sample_time);
    let earlier_ix = later_ix.saturating_sub(1).min(life_history.len() - 1);
    let (earlier_t, earlier_life) = life_history[earlier_ix];

    if later_ix >= life_history.len() {
        return earlier_life.clamp(0.0, 1.0);
    }

    let (later_t, later_life) = life_history[later_ix];
    let dt = later_t - earlier_t;
    if dt.abs() <= f32::EPSILON {
        return earlier_life.clamp(0.0, 1.0);
    }

    let alpha = ((sample_time - earlier_t) / dt).clamp(0.0, 1.0);
    (earlier_life + (later_life - earlier_life) * alpha).clamp(0.0, 1.0)
}

#[inline(always)]
fn barely_marker_sample(si: &ScoreInfo) -> Option<(f32, f32)> {
    // ITGmania GraphDisplay only shows "Barely" if the chart was cleared.
    if si.grade == scores::Grade::Failed || si.fail_time.is_some() || si.life_history.is_empty() {
        return None;
    }

    let sample_end = si.graph_last_second.max(0.0);
    if !sample_end.is_finite() || sample_end <= 0.0 {
        return None;
    }

    let mut min_life = 1.0_f32;
    let mut min_ix = 0usize;
    let inv_samples = 1.0_f32 / GRAPH_BARELY_SAMPLE_COUNT as f32;
    for i in 0..GRAPH_BARELY_SAMPLE_COUNT {
        let t = (i as f32) * inv_samples * sample_end;
        let life = life_record_lerp_at(&si.life_history, t);
        if life < min_life {
            min_life = life;
            min_ix = i;
        }
    }

    if min_life <= 0.0 || min_life >= GRAPH_BARELY_LIFE_MAX {
        return None;
    }

    let t = (min_ix as f32) * inv_samples * sample_end;
    Some((t, min_life))
}

pub fn handle_input(state: &mut State, ev: &InputEvent) -> ScreenAction {
    if !ev.pressed {
        return ScreenAction::None;
    }
    if state.auto_advance_seconds.is_some() {
        return ScreenAction::None;
    }
    let return_target = if state.return_to_course {
        Screen::SelectCourse
    } else {
        Screen::SelectMusic
    };

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
        let has_online_panes = state.allow_online_panes;
        let gs_side = if play_style == profile::PlayStyle::Versus {
            controller
        } else {
            profile::get_session_player_side()
        };
        let has_gs = has_online_panes
            && scores::get_or_fetch_player_leaderboards_for_side(
                &si.chart.short_hash,
                gs_side,
                GS_RECORD_ROWS,
            )
            .is_some();

        state.active_pane[controller_idx] = if dir >= 0 {
            eval_pane_next(
                state.active_pane[controller_idx],
                has_hard_ex,
                has_online_panes,
                has_gs,
            )
        } else {
            eval_pane_prev(
                state.active_pane[controller_idx],
                has_hard_ex,
                has_online_panes,
                has_gs,
            )
        };

        // Don't allow duplicate panes in single/double.
        if play_style != profile::PlayStyle::Versus {
            let other_idx = 1 - controller_idx;
            if state.active_pane[controller_idx] == state.active_pane[other_idx] {
                state.active_pane[controller_idx] = if dir >= 0 {
                    eval_pane_next(
                        state.active_pane[controller_idx],
                        has_hard_ex,
                        has_online_panes,
                        has_gs,
                    )
                } else {
                    eval_pane_prev(
                        state.active_pane[controller_idx],
                        has_hard_ex,
                        has_online_panes,
                        has_gs,
                    )
                };
            }
        }
    };
    let mut shift_graph_for = |controller: profile::PlayerSide, dir: i32| {
        let controller_idx = side_idx(controller);
        let player_idx = player_idx_for_controller(controller);
        if state
            .score_info
            .get(player_idx)
            .and_then(|s| s.as_ref())
            .is_none()
        {
            return;
        }

        state.active_graph[controller_idx] = if dir >= 0 {
            eval_graph_next(state.active_graph[controller_idx])
        } else {
            eval_graph_prev(state.active_graph[controller_idx])
        };

        // Single/double have one lower graph; keep both controller slots in sync.
        if play_style != profile::PlayStyle::Versus {
            let other_idx = 1 - controller_idx;
            state.active_graph[other_idx] = state.active_graph[controller_idx];
        }
    };

    match ev.action {
        VirtualAction::p1_back
        | VirtualAction::p1_start
        | VirtualAction::p2_back
        | VirtualAction::p2_start => ScreenAction::Navigate(return_target),
        VirtualAction::p1_right | VirtualAction::p1_menu_right => {
            shift_pane_for(profile::PlayerSide::P1, 1);
            ScreenAction::None
        }
        VirtualAction::p1_left | VirtualAction::p1_menu_left => {
            shift_pane_for(profile::PlayerSide::P1, -1);
            ScreenAction::None
        }
        VirtualAction::p1_up | VirtualAction::p1_menu_up => {
            shift_graph_for(profile::PlayerSide::P1, -1);
            ScreenAction::None
        }
        VirtualAction::p1_down | VirtualAction::p1_menu_down => {
            shift_graph_for(profile::PlayerSide::P1, 1);
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
        VirtualAction::p2_up | VirtualAction::p2_menu_up => {
            shift_graph_for(profile::PlayerSide::P2, -1);
            ScreenAction::None
        }
        VirtualAction::p2_down | VirtualAction::p2_menu_down => {
            shift_graph_for(profile::PlayerSide::P2, 1);
            ScreenAction::None
        }
        _ => ScreenAction::None,
    }
}

pub fn get_actors(state: &State, asset_manager: &AssetManager) -> Vec<Actor> {
    let cfg = crate::config::get();
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

    // Header timers (zmod parity): session timer + optional cumulative gameplay timer.
    actors.push(select_shared::build_session_timer(format_session_time(
        state.session_elapsed,
    )));
    if cfg.show_select_music_gameplay_timer {
        actors.push(select_shared::build_gameplay_timer(format_session_time(
            state.gameplay_elapsed,
        )));
    }

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
                BANNER_FALLBACK_KEYS[state.active_color_index.rem_euclid(12) as usize].to_string()
            });

        let full_title = score_info.song.display_full_title(cfg.translated_titles);

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
        let bpm_text = cached_bpm_text(
            score_info.song.min_bpm,
            score_info.song.max_bpm,
            score_info.music_rate,
        );

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
                Arc::<str>::from("")
            } else {
                cached_song_length_text(seconds)
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

            // Record Texts (Simply Love PerPlayer/Upper/RecordTexts.lua)
            let has_recordable_score = si.score_percent >= 0.01;
            let machine_record_rank = if has_recordable_score {
                si.machine_record_highlight_rank.filter(|rank| *rank > 0)
            } else {
                None
            };
            let personal_record_rank = if has_recordable_score {
                si.personal_record_highlight_rank.filter(|rank| *rank > 0)
            } else {
                None
            };
            if machine_record_rank.is_some() || personal_record_rank.is_some() {
                let record_color = eval_player_color_rgba(side, state.active_color_index);
                // Simply Love/zmod:
                // RecordTexts frame @ x(-45|95), y(54), zoom(0.225)
                // MachineRecord child @ xy(-110,-18), PersonalRecord @ xy(-110,24)
                // Final world pos = frame + child * frame_zoom.
                let record_frame_x = if side == profile::PlayerSide::P1 {
                    upper_origin_x - 45.0
                } else {
                    upper_origin_x + 95.0
                };
                let record_frame_y = 54.0_f32;
                let record_frame_zoom = 0.225_f32;
                let record_x = record_frame_x - 110.0 * record_frame_zoom;

                if let Some(rank) = machine_record_rank {
                    actors.push(act!(text: font("wendy"):
                        settext(cached_record_text(true, rank)):
                        align(0.5, 0.5):
                        xy(record_x, record_frame_y - 18.0 * record_frame_zoom):
                        zoom(record_frame_zoom): z(101):
                        diffuse(record_color[0], record_color[1], record_color[2], 1.0)
                    ));
                }

                if let Some(rank) = personal_record_rank {
                    actors.push(act!(text: font("wendy"):
                        settext(cached_record_text(false, rank)):
                        align(0.5, 0.5):
                        xy(record_x, record_frame_y + 24.0 * record_frame_zoom):
                        zoom(record_frame_zoom): z(101):
                        diffuse(record_color[0], record_color[1], record_color[2], 1.0)
                    ));
                }
            }

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
                let difficulty_color =
                    color::difficulty_rgba(&si.chart.difficulty, state.active_color_index);
                if cfg.zmod_rating_box_text {
                    let difficulty_display_name = color::difficulty_display_name_for_song(
                        &si.chart.difficulty,
                        &si.song.title,
                        true,
                    );
                    let box_x = upper_origin_x + 129.5 * dir;
                    actors.push(act!(quad:
                        align(0.5, 0.5):
                        xy(box_x, cy - 76.0):
                        zoomto(40.0, 40.0):
                        z(101):
                        diffuse(difficulty_color[0], difficulty_color[1], difficulty_color[2], 1.0)
                    ));
                    actors.push(act!(text:
                        font("wendy"):
                        settext(si.chart.meter.to_string()):
                        align(0.5, 0.5):
                        xy(box_x, cy - 76.0):
                        zoom(0.55):
                        z(102):
                        diffuse(0.0, 0.0, 0.0, 1.0)
                    ));
                    actors.push(act!(text:
                        font("miso"):
                        settext(style_label):
                        align(0.5, 0.5):
                        xy(box_x, cy - 92.0):
                        zoom(0.5):
                        z(102):
                        diffuse(0.0, 0.0, 0.0, 1.0)
                    ));
                    actors.push(act!(text:
                        font("miso"):
                        settext(difficulty_display_name):
                        align(0.5, 0.5):
                        xy(box_x, cy - 61.0):
                        zoom(0.5):
                        z(102):
                        diffuse(0.0, 0.0, 0.0, 1.0)
                    ));
                } else {
                    let difficulty_display_name =
                        color::difficulty_display_name(&si.chart.difficulty, false);
                    let difficulty_text =
                        cached_difficulty_text(style_label, difficulty_display_name);
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
            }

            // Step artist / description:
            // SL-style source list is [AuthorCredit, Description] (if distinct).
            let mut step_artist_lines: Vec<String> = Vec::with_capacity(2);
            let author = si.chart.step_artist.trim();
            if !author.is_empty() {
                step_artist_lines.push(author.to_owned());
            }
            let description = si.chart.description.trim();
            if !description.is_empty() && step_artist_lines.iter().all(|line| line != description) {
                step_artist_lines.push(description.to_owned());
            }

            if cfg.zmod_rating_box_text {
                let step_artist_text = step_artist_lines.join("\n");
                if !step_artist_text.is_empty() {
                    let line_count = step_artist_lines.len().max(1);
                    let zmod_diff_box_x = upper_origin_x + 129.5 * dir;
                    let x = zmod_diff_box_x - 21.5 * dir;
                    let y_base = if line_count > 2 { cy - 62.0 } else { cy - 59.0 };
                    let align_x = if side == profile::PlayerSide::P1 {
                        0.0
                    } else {
                        1.0
                    };
                    let (text_zoom, y_nudge, bg_w, text_h_px) = asset_manager
                        .with_fonts(|all_fonts| {
                            asset_manager.with_font("miso", |miso_font| {
                                let mut max_w = 0.0_f32;
                                for line in step_artist_text.lines() {
                                    let line_w = font::measure_line_width_logical(
                                        miso_font, line, all_fonts,
                                    ) as f32;
                                    if line_w > max_w {
                                        max_w = line_w;
                                    }
                                }

                                let line_spacing = miso_font.line_spacing.max(1) as f32;
                                let text_h = line_spacing * line_count as f32;

                                let mut zoom = if line_count > 2 { 0.6_f32 } else { 0.7_f32 };
                                let mut nudge = 0.0_f32;
                                while max_w * zoom > 120.0 && zoom > 0.45 {
                                    zoom -= 0.05;
                                    nudge -= 1.0;
                                }
                                let bg_w = (max_w + 20.0).max(10.0) * zoom;
                                let text_h_px = text_h * zoom;
                                (zoom, nudge, bg_w, text_h_px)
                            })
                        })
                        .unwrap_or((0.7, 0.0, 24.0, 8.0));
                    let y = y_base + y_nudge;

                    let bg_x = zmod_diff_box_x - 19.5 * dir;
                    let bg_y = cy - 56.0;
                    let bg_h = (bg_y - y + text_h_px - 3.0).max(1.0);
                    let (fadeleft, faderight) = if side == profile::PlayerSide::P1 {
                        (0.0, 0.1)
                    } else {
                        (0.1, 0.0)
                    };
                    actors.push(act!(quad:
                        align(align_x, 1.0): xy(bg_x, bg_y):
                        zoomto(bg_w, bg_h):
                        diffuse(0.0, 0.0, 0.0, 0.7):
                        fadeleft(fadeleft): faderight(faderight):
                        z(102)
                    ));

                    if side == profile::PlayerSide::P1 {
                        actors.push(act!(text: font("miso"): settext(step_artist_text):
                            align(align_x, 1.0): xy(x, y): zoom(text_zoom): z(103):
                            diffuse(1.0, 1.0, 1.0, 1.0)
                        ));
                    } else {
                        actors.push(act!(text: font("miso"): settext(step_artist_text):
                            align(align_x, 1.0): xy(x, y): zoom(text_zoom): z(103):
                            diffuse(1.0, 1.0, 1.0, 1.0): horizalign(right)
                        ));
                    }
                }
            } else {
                let step_artist_text = if step_artist_lines.is_empty() {
                    String::new()
                } else {
                    // Simply Love StepArtist.lua marquee cadence: 2s per entry.
                    let cycle_idx = ((state.screen_elapsed.max(0.0) / 2.0).floor() as usize)
                        % step_artist_lines.len();
                    step_artist_lines[cycle_idx].clone()
                };
                if !step_artist_text.is_empty() {
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
            }

            // Breakdown Text (under grade)
            let breakdown_width = if cfg.zmod_rating_box_text {
                screen_width() * 0.26
            } else {
                155.0
            };
            let breakdown_text = {
                let chart = &si.chart;
                let (detailed, partial, simple) = match cfg.select_music_breakdown_style {
                    crate::config::BreakdownStyle::Sn => (
                        &chart.sn_detailed_breakdown,
                        &chart.sn_partial_breakdown,
                        &chart.sn_simple_breakdown,
                    ),
                    crate::config::BreakdownStyle::Sl => (
                        &chart.detailed_breakdown,
                        &chart.partial_breakdown,
                        &chart.simple_breakdown,
                    ),
                };
                asset_manager
                    .with_fonts(|all_fonts| {
                        asset_manager.with_font("miso", |miso_font| -> Option<Arc<str>> {
                            let width_constraint = breakdown_width;
                            let text_zoom = 0.7;
                            let max_allowed_logical_width = width_constraint / text_zoom;

                            let fits = |text: &str| {
                                let logical_width =
                                    font::measure_line_width_logical(miso_font, text, all_fonts)
                                        as f32;
                                logical_width <= max_allowed_logical_width
                            };

                            if fits(detailed) {
                                Some(cached_str_ref(detailed))
                            } else if fits(partial) {
                                Some(cached_str_ref(partial))
                            } else if fits(simple) {
                                Some(cached_str_ref(simple))
                            } else {
                                Some(cached_total_label_text(chart.total_streams))
                            }
                        })
                    })
                    .flatten()
                    .unwrap_or_else(|| cached_str_ref(simple))
            };

            {
                let x = if cfg.zmod_rating_box_text {
                    upper_origin_x + 148.0 * dir
                } else {
                    upper_origin_x + 150.0 * dir
                };
                let y = if cfg.zmod_rating_box_text {
                    cy - 97.0
                } else {
                    cy - 95.0
                };
                let align_x = if side == profile::PlayerSide::P1 {
                    0.0
                } else {
                    1.0
                };
                let align_y = if cfg.zmod_rating_box_text { 1.0 } else { 0.5 };
                if cfg.zmod_rating_box_text {
                    let (bg_w, bg_h) = asset_manager
                        .with_fonts(|all_fonts| {
                            asset_manager.with_font("miso", |miso_font| {
                                let text_w = font::measure_line_width_logical(
                                    miso_font,
                                    &breakdown_text,
                                    all_fonts,
                                ) as f32;
                                let line_h = miso_font.height.max(1) as f32;
                                let bg_w = (text_w + 10.0).min(breakdown_width).max(10.0) * 0.7;
                                let bg_h = (line_h + 4.0).max(4.0) * 0.7;
                                (bg_w, bg_h)
                            })
                        })
                        .unwrap_or((breakdown_width * 0.7, 14.0));
                    let bg_x = upper_origin_x + 150.0 * dir;
                    let bg_y = cy - 95.5;
                    let (fadeleft, faderight) = if side == profile::PlayerSide::P1 {
                        (0.0, 0.1)
                    } else {
                        (0.1, 0.0)
                    };
                    actors.push(act!(quad:
                        align(align_x, 1.0): xy(bg_x, bg_y):
                        zoomto(bg_w, bg_h):
                        diffuse(0.0, 0.0, 0.0, 0.7):
                        fadeleft(fadeleft): faderight(faderight):
                        z(102)
                    ));
                }
                let text_z = if cfg.zmod_rating_box_text { 103 } else { 101 };
                if side == profile::PlayerSide::P1 {
                    actors.push(act!(text: font("miso"): settext(breakdown_text):
                        align(align_x, align_y): xy(x, y): zoom(0.7):
                        maxwidth(breakdown_width): horizalign(left): z(text_z):
                        diffuse(1.0, 1.0, 1.0, 1.0)
                    ));
                } else {
                    actors.push(act!(text: font("miso"): settext(breakdown_text):
                        align(align_x, align_y): xy(x, y): zoom(0.7):
                        maxwidth(breakdown_width): horizalign(right): z(text_z):
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

            actors.extend(eval_panes::build_pane_percentage_display(
                si, pane, controller,
            ));

            match pane {
                EvalPane::Timing => actors.extend(eval_panes::build_timing_pane(
                    si,
                    state.timing_hist_mesh[player_idx].as_ref(),
                    controller,
                    crate::screens::components::eval_graphs::TimingHistogramScale::Itg,
                )),
                EvalPane::TimingEx => actors.extend(eval_panes::build_timing_pane(
                    si,
                    state.timing_hist_mesh_ex[player_idx].as_ref(),
                    controller,
                    crate::screens::components::eval_graphs::TimingHistogramScale::Ex,
                )),
                EvalPane::TimingHardEx => actors.extend(eval_panes::build_timing_pane(
                    si,
                    state.timing_hist_mesh_hard_ex[player_idx].as_ref(),
                    controller,
                    crate::screens::components::eval_graphs::TimingHistogramScale::HardEx,
                )),
                EvalPane::QrCode => actors.extend(eval_panes::build_gs_qr_pane(si, controller)),
                EvalPane::GrooveStats => actors.extend(eval_panes::build_gs_records_pane(
                    controller,
                    scores::get_or_fetch_player_leaderboards_for_side(
                        &si.chart.short_hash,
                        gs_side,
                        GS_RECORD_ROWS,
                    )
                    .as_ref(),
                )),
                EvalPane::MachineRecords => actors.extend(eval_panes::build_machine_records_pane(
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
                    actors.extend(eval_panes::build_column_judgments_pane(
                        si,
                        controller,
                        pane3_player_side,
                        asset_manager,
                    ));
                }
                EvalPane::Standard | EvalPane::FaPlus | EvalPane::HardEx => {
                    actors.extend(eval_panes::build_stats_pane(
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
                    actors.extend(eval_panes::build_modifiers_pane(si, center_x, graph_width));
                }
            }
        } else if let Some(si) = state.score_info.get(0).and_then(|s| s.as_ref()) {
            actors.extend(eval_panes::build_modifiers_pane(
                si,
                screen_center_x(),
                graph_width,
            ));
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

            let graph_controller_idx = if play_style == profile::PlayStyle::Versus {
                player_idx
            } else if player_side == profile::PlayerSide::P1 {
                0
            } else {
                1
            };
            let graph_mode = state.active_graph[graph_controller_idx];
            let density_mesh = state.density_graph_mesh[player_idx].as_ref();
            let scatter_mesh = match graph_mode {
                EvalGraphPane::Itg => state.scatter_mesh_itg[player_idx].as_ref(),
                EvalGraphPane::Ex => state.scatter_mesh_ex[player_idx].as_ref(),
                EvalGraphPane::HardEx => state.scatter_mesh_hard_ex[player_idx].as_ref(),
                EvalGraphPane::Arrow => state.scatter_mesh_arrow[player_idx].as_ref(),
                EvalGraphPane::Foot => state.scatter_mesh_foot[player_idx].as_ref(),
            };
            let show_feet_overlay = graph_mode == EvalGraphPane::Foot;

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
                        if show_feet_overlay {
                            act!(sprite("feet-diagram.png"):
                                align(0.5, 0.5):
                                xy(graph_width / 2.0_f32, graph_height / 2.0_f32):
                                zoom(0.45):
                                diffusealpha(0.2):
                                z(3)
                            )
                        } else {
                            act!(sprite("__white"): visible(false))
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
                            // Simply Love nudges GraphDisplay's white life line down by 1px
                            // (`self:GetChild("Line"):addy(1)`), so keep a matching inset.
                            let y = ((1.0 - life).clamp(0.0, 1.0) * graph_height + 1.0)
                                .clamp(1.0, (graph_height - 1.0).max(1.0));

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

                        // Life history only stores change points; once life stops changing
                        // (e.g. capped at full), continue the final segment to graph end.
                        let end_x =
                            ((last - first) / (dur + padding)).clamp(0.0, 1.0) * graph_width;
                        if last_x > -900.0 {
                            let w = (end_x - last_x).max(0.0);
                            if w > 0.5 {
                                life_children.push(act!(quad:
                                    align(0.0, 0.5): xy(last_x, last_y):
                                    setsize(w, 2.0):
                                    diffuse(1.0, 1.0, 1.0, 0.8):
                                    z(4)
                                ));
                            }
                        }

                        if let Some((barely_time, barely_life)) = barely_marker_sample(si) {
                            let x = ((barely_time - first) / (dur + padding)).clamp(0.0, 1.0)
                                * graph_width;
                            let y = ((1.0 - barely_life).clamp(0.0, 1.0) * graph_height + 1.0)
                                .clamp(1.0, (graph_height - 1.0).max(1.0));
                            // Keep a tiny marker on the life line, then animate the label/arrow
                            // in the same timing pattern as Simply Love GraphDisplay Barely.
                            life_children.push(act!(quad:
                                align(0.5, 0.5): xy(x, y):
                                setsize(3.0, 3.0):
                                diffuse(1.0, 1.0, 1.0, 0.95):
                                z(6)
                            ));

                            let anchor_y = (y - 12.0).clamp(18.0, graph_height - 24.0);
                            let text_start_y = anchor_y - 20.0;
                            let text_mid_y = anchor_y - 5.0;
                            let text_end_y = anchor_y + 10.0;
                            let arrow_start_y = anchor_y - 10.0;
                            let arrow_mid_y = anchor_y + 5.0;
                            let arrow_end_y = anchor_y + 20.0;

                            life_children.push(act!(text:
                                font("miso"): settext("Barely!"):
                                align(0.5, 0.5): xy(x, text_start_y):
                                zoom(0.75):
                                diffuse(1.0, 1.0, 1.0, 1.0): alpha(0.0):
                                sleep(GRAPH_BARELY_ANIM_DELAY_SECONDS):
                                accelerate(GRAPH_BARELY_ANIM_SEG_SECONDS): alpha(1.0): y(text_end_y):
                                decelerate(GRAPH_BARELY_ANIM_SEG_SECONDS): y(text_mid_y):
                                accelerate(GRAPH_BARELY_ANIM_SEG_SECONDS): y(text_end_y):
                                z(8)
                            ));
                            life_children.push(act!(sprite("meter_arrow.png"):
                                align(0.5, 0.5): xy(x, arrow_start_y):
                                // SL uses rotationz(90); deadsync's current z-rotation sign
                                // is opposite in screen space, so -90 is the visual parity.
                                rotationz(-90.0): zoom(0.50):
                                diffuse(1.0, 1.0, 1.0, 1.0): alpha(0.0):
                                sleep(GRAPH_BARELY_ANIM_DELAY_SECONDS):
                                accelerate(GRAPH_BARELY_ANIM_SEG_SECONDS): alpha(1.0): y(arrow_end_y):
                                decelerate(GRAPH_BARELY_ANIM_SEG_SECONDS): y(arrow_mid_y):
                                accelerate(GRAPH_BARELY_ANIM_SEG_SECONDS): y(arrow_end_y):
                                sleep(GRAPH_BARELY_ARROW_PULSE_DELAY_SECONDS):
                                diffuseshift():
                                effectcolor1(1.0, 1.0, 1.0, 1.0):
                                effectcolor2(1.0, 1.0, 1.0, 0.2):
                                z(8)
                            ));
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
                            let remaining_str = cached_remaining_time_text(remaining);
                            // SL/zmod/Arrow Cloud Graphs.lua:
                            // width = text_width * 0.65, addx = max(width * 0.8, 10), parent zoom=1.25.
                            let fail_box_scale = 1.25_f32;
                            let (inner_w, outer_w, inner_h, outer_h, addx) = asset_manager
                                .with_fonts(|all_fonts| {
                                    asset_manager.with_font("miso", |miso_font| {
                                        let text_w = font::measure_line_width_logical(
                                            miso_font,
                                            &remaining_str,
                                            all_fonts,
                                        ) as f32;
                                        let base_w = (text_w * 0.65).max(1.0);
                                        let base_addx = (base_w * 0.8).max(10.0);
                                        (
                                            base_w * fail_box_scale,
                                            (base_w + 1.0) * fail_box_scale,
                                            10.0 * fail_box_scale,
                                            11.0 * fail_box_scale,
                                            base_addx * fail_box_scale,
                                        )
                                    })
                                })
                                .unwrap_or((30.0, 31.25, 12.5, 13.75, 12.5));

                            // SL/zmod/Arrow Cloud place this at GraphHeight-10 (inside the graph),
                            // not below the panel.
                            let box_center_y = graph_height - 10.0;
                            let box_center_x = x + addx;

                            life_children.push(act!(quad:
                                align(0.5, 0.5): xy(box_center_x, box_center_y):
                                setsize(outer_w, outer_h):
                                diffuse(1.0, 0.0, 0.0, 1.0):
                                z(6)
                            ));
                            life_children.push(act!(quad:
                                align(0.5, 0.5): xy(box_center_x, box_center_y):
                                setsize(inner_w, inner_h):
                                diffuse(0.0, 0.0, 0.0, 1.0):
                                z(7)
                            ));
                            life_children.push(act!(text:
                                font("miso"): settext(remaining_str):
                                align(0.5, 0.5): xy(box_center_x, box_center_y):
                                zoom(0.625):
                                diffuse(1.0, 0.0, 0.0, 1.0):
                                z(8)
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
