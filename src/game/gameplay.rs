// ===== PROJECT: deadsync FILE: src/game/gameplay.rs =====
use crate::core::audio;
use crate::core::input::{lane_from_keycode, InputEdge, InputSource, Lane};
use crate::core::space::*;
use crate::game::chart::ChartData;
use crate::game::judgment::{self, JudgeGrade, Judgment};
use crate::game::note::{HoldData, HoldResult, MineResult, Note, NoteType};
use crate::game::parsing::notes as note_parser;
use crate::game::parsing::noteskin::{self, Noteskin, Style};
use crate::game::song::SongData;
use crate::game::timing::TimingData;
use crate::game::{
    life::{LifeChange, REGEN_COMBO_AFTER_MISS},
    profile,
    scroll::ScrollSpeedSetting,
};
use crate::screens::{Screen, ScreenAction};
use crate::ui::color;
use log::{info, warn};
use std::collections::{HashMap, VecDeque};
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};
use winit::event::{ElementState, KeyEvent};
use winit::keyboard::{KeyCode, PhysicalKey};

pub const TRANSITION_IN_DURATION: f32 = 0.4;
pub const TRANSITION_OUT_DURATION: f32 = 0.4;

const MIN_SECONDS_TO_STEP: f32 = 6.0;
const MIN_SECONDS_TO_MUSIC: f32 = 2.0;
const M_MOD_HIGH_CAP: f32 = 600.0;

// Timing windows now sourced from game::timing

pub const RECEPTOR_Y_OFFSET_FROM_CENTER: f32 = -125.0;
pub const DRAW_DISTANCE_BEFORE_TARGETS_MULTIPLIER: f32 = 1.5;
pub const DRAW_DISTANCE_AFTER_TARGETS: f32 = 130.0;
pub const MINE_EXPLOSION_DURATION: f32 = 0.6;
pub const HOLD_JUDGMENT_TOTAL_DURATION: f32 = 0.8;
pub const RECEPTOR_GLOW_DURATION: f32 = 0.2;
pub const COMBO_HUNDRED_MILESTONE_DURATION: f32 = 0.6;
pub const COMBO_THOUSAND_MILESTONE_DURATION: f32 = 0.7;

const MAX_HOLD_LIFE: f32 = 1.0;
const INITIAL_HOLD_LIFE: f32 = 1.0;
const TIMING_WINDOW_SECONDS_HOLD: f32 = 0.32;
const TIMING_WINDOW_SECONDS_ROLL: f32 = 0.35;

#[derive(Clone, Debug)]
pub struct Arrow {
    pub beat: f32,
    pub column: usize,
    #[allow(dead_code)]
    pub note_type: NoteType,
    pub note_index: usize,
}

#[derive(Clone, Debug)]
pub struct JudgmentRenderInfo {
    pub judgment: Judgment,
    pub judged_at: Instant,
}

#[derive(Copy, Clone, Debug)]
pub struct HoldJudgmentRenderInfo {
    pub result: HoldResult,
    pub triggered_at: Instant,
}

#[derive(Clone, Debug)]
pub struct ActiveTapExplosion {
    pub window: String,
    pub elapsed: f32,
    pub start_beat: f32,
}

#[derive(Clone, Debug)]
pub struct ActiveMineExplosion {
    pub elapsed: f32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ComboMilestoneKind {
    Hundred,
    Thousand,
}

#[derive(Clone, Debug)]
pub struct ActiveComboMilestone {
    pub kind: ComboMilestoneKind,
    pub elapsed: f32,
}

#[derive(Clone, Debug)]
pub struct ActiveHold {
    pub note_index: usize,
    pub end_time: f32,
    pub note_type: NoteType,
    pub let_go: bool,
    pub is_pressed: bool,
    pub life: f32,
}

#[inline(always)]
pub fn active_hold_is_engaged(active: &ActiveHold) -> bool {
    !active.let_go && active.life > 0.0
}

pub struct State {
    pub song: Arc<SongData>,
    pub background_texture_key: String,
    pub chart: Arc<ChartData>,
    pub timing: Arc<TimingData>,
    pub notes: Vec<Note>,

    pub song_start_instant: Instant,
    pub current_beat: f32,
    pub current_music_time: f32,
    pub note_spawn_cursor: usize,
    pub judged_row_cursor: usize,
    pub arrows: [Vec<Arrow>; 4],
    // Cached per-note timing to avoid per-frame recomputation
    pub note_time_cache: Vec<f32>,
    pub note_display_beat_cache: Vec<f32>,
    pub hold_end_time_cache: Vec<Option<f32>>,
    pub music_end_time: f32,
    pub music_rate: f32,

    pub combo: u32,
    pub miss_combo: u32,
    pub full_combo_grade: Option<JudgeGrade>,
    pub first_fc_attempt_broken: bool,
    pub judgment_counts: HashMap<JudgeGrade, u32>,
    pub scoring_counts: HashMap<JudgeGrade, u32>,
    pub last_judgment: Option<JudgmentRenderInfo>,
    pub hold_judgments: [Option<HoldJudgmentRenderInfo>; 4],

    pub life: f32,
    pub combo_after_miss: u32,
    pub is_failing: bool,
    pub is_in_freeze: bool,
    pub is_in_delay: bool,
    pub fail_time: Option<f32>,

    pub earned_grade_points: i32,
    pub possible_grade_points: i32,
    pub song_completed_naturally: bool,

    pub noteskin: Option<Noteskin>,
    pub active_color_index: i32,
    pub player_color: [f32; 4],
    pub scroll_speed: ScrollSpeedSetting,
    pub scroll_reference_bpm: f32,
    pub scroll_pixels_per_second: f32,
    pub scroll_travel_time: f32,
    pub draw_distance_before_targets: f32,
    pub draw_distance_after_targets: f32,
    pub receptor_glow_timers: [f32; 4],
    pub receptor_bop_timers: [f32; 4],
    pub tap_explosions: [Option<ActiveTapExplosion>; 4],
    pub mine_explosions: [Option<ActiveMineExplosion>; 4],
    pub active_holds: [Option<ActiveHold>; 4],
    pub combo_milestones: Vec<ActiveComboMilestone>,
    pub hands_achieved: u32,
    pub holds_total: u32,
    pub holds_held: u32,
    pub holds_held_for_score: u32,
    pub rolls_total: u32,
    pub rolls_held: u32,
    pub rolls_held_for_score: u32,
    pub mines_total: u32,
    pub mines_hit: u32,
    pub mines_hit_for_score: u32,
    pub mines_avoided: u32,
    hands_holding_count_for_stats: i32,

    pub total_elapsed_in_screen: f32,

    pub hold_to_exit_key: Option<KeyCode>,
    pub hold_to_exit_start: Option<Instant>,
    prev_inputs: [bool; 4],
    keyboard_lane_state: [bool; 4],
    gamepad_lane_state: [bool; 4],
    pending_edges: VecDeque<InputEdge>,

    log_timer: f32,
}

#[inline(always)]
fn is_state_dead(state: &State) -> bool {
    state.is_failing || state.life <= 0.0
}

fn apply_life_change(state: &mut State, delta: f32) {
    if is_state_dead(state) {
        state.life = 0.0;
        state.is_failing = true;
        return;
    }

    let mut final_delta = delta;

    if final_delta > 0.0 {
        if state.combo_after_miss > 0 {
            final_delta = 0.0;
            state.combo_after_miss -= 1;
        }
    } else if final_delta < 0.0 {
        state.combo_after_miss = REGEN_COMBO_AFTER_MISS;
    }

    state.life = (state.life + final_delta).clamp(0.0, 1.0);

    if state.life <= 0.0 {
        if !state.is_failing {
            state.fail_time = Some(state.current_music_time);
        }
        state.life = 0.0;
        state.is_failing = true;
        info!("Player has failed!");
    }
}

pub fn queue_input_edge(
    state: &mut State,
    source: InputSource,
    lane: Lane,
    pressed: bool,
    timestamp: Instant,
) {
    state.pending_edges.push_back(InputEdge {
        lane,
        pressed,
        source,
        timestamp,
    });
}

/// Parses the #DISPLAYBPM string to get a reference BPM for M-mods.
/// Returns None if the tag is missing, random (*), or invalid.
fn get_reference_bpm_from_display_tag(display_bpm_str: &str) -> Option<f32> {
    let s = display_bpm_str.trim();
    if s.is_empty() || s == "*" {
        return None;
    }

    // Check for a range (e.g., "100:200" or "100.00:200.00")
    if let Some((_, max_str)) = s.split_once(':') {
        return max_str.trim().parse::<f32>().ok();
    }

    // Otherwise, it should be a single number
    s.parse::<f32>().ok()
}

pub fn init(song: Arc<SongData>, chart: Arc<ChartData>, active_color_index: i32, music_rate: f32) -> State {
    info!("Initializing Gameplay Screen...");
    info!(
        "Loaded song '{}' and chart '{}'",
        song.title, chart.difficulty
    );

    let style = Style {
        num_cols: 4,
        num_players: 1,
    };
    let noteskin = noteskin::load(Path::new("assets/noteskins/cel/dance-single.txt"), &style)
        .ok()
        .or_else(|| noteskin::load(Path::new("assets/noteskins/fallback.txt"), &style).ok());

    let config = crate::config::get();
    let timing = Arc::new(TimingData::from_chart_data(
        -song.offset, config.global_offset_seconds,
        chart.chart_bpms.as_deref(),
        &song.normalized_bpms,
		chart.chart_stops.as_deref(),
		&song.normalized_stops,
		chart.chart_delays.as_deref(),
		&song.normalized_delays,
		chart.chart_warps.as_deref(),
		&song.normalized_warps,
		chart.chart_speeds.as_deref(),
		&song.normalized_speeds,
		chart.chart_scrolls.as_deref(),
		&song.normalized_scrolls,
        &chart.notes,
    ));

    let parsed_notes = note_parser::parse_chart_notes(&chart.notes);
    let mut notes: Vec<Note> = Vec::with_capacity(parsed_notes.len());
    let mut holds_total: u32 = 0;
    let mut rolls_total: u32 = 0;
    let mut mines_total: u32 = 0;
    for parsed in parsed_notes {
        let row_index = parsed.row_index;
        let Some(beat) = timing.get_beat_for_row(row_index) else {
            continue;
        };

        let note_type = parsed.note_type;
        match note_type {
            NoteType::Hold => {
                holds_total = holds_total.saturating_add(1);
            }
            NoteType::Roll => {
                rolls_total = rolls_total.saturating_add(1);
            }
            NoteType::Mine => {
                mines_total = mines_total.saturating_add(1);
            }
            NoteType::Tap => {}
        }

        let hold = match (note_type, parsed.tail_row_index) {
            (NoteType::Hold | NoteType::Roll, Some(tail_row)) => {
                timing.get_beat_for_row(tail_row).map(|end_beat| HoldData {
                    end_row_index: tail_row,
                    end_beat,
                    result: None,
                    life: INITIAL_HOLD_LIFE,
                    let_go_started_at: None,
                    let_go_starting_life: 0.0,
                    last_held_row_index: row_index,
                    last_held_beat: beat,
                })
            }
            _ => None,
        };

        notes.push(Note {
            beat,
            column: parsed.column,
            note_type,
            row_index,
            result: None,
            hold,
            mine_result: None,
        });
    }
    // ITG scoring counts one tap judgment per row (chords count as one).
    // Compute unique non-mine rows to determine possible tap grade points.
    let num_tap_rows = {
        use std::collections::HashSet;
        let mut rows: HashSet<usize> = HashSet::new();
        for n in &notes {
            if !matches!(n.note_type, NoteType::Mine) {
                rows.insert(n.row_index);
            }
        }
        rows.len() as u64
    };
    let possible_grade_points = (num_tap_rows * 5)
        + (holds_total as u64 * judgment::HOLD_SCORE_HELD as u64)
        + (rolls_total as u64 * judgment::HOLD_SCORE_HELD as u64);
    let possible_grade_points = possible_grade_points as i32;

    info!("Parsed {} notes from chart data.", notes.len());

    // Build immutable caches for timing-intensive lookups
    let note_time_cache: Vec<f32> = notes
        .iter()
        .map(|n| timing.get_time_for_beat(n.beat))
        .collect();
    let note_display_beat_cache: Vec<f32> = notes
        .iter()
        .map(|n| timing.get_displayed_beat(n.beat))
        .collect();
    let hold_end_time_cache: Vec<Option<f32>> = notes
        .iter()
        .map(|n| n.hold.as_ref().map(|h| timing.get_time_for_beat(h.end_beat)))
        .collect();

    let first_note_beat = notes.first().map_or(0.0, |n| n.beat);
    let first_second = timing.get_time_for_beat(first_note_beat);
    let start_delay = (MIN_SECONDS_TO_STEP - first_second).max(MIN_SECONDS_TO_MUSIC);
    let song_start_instant = Instant::now() + Duration::from_secs_f32(start_delay);

    if let Some(music_path) = &song.music_path {
        info!("Starting music with a preroll delay of {:.2}s", start_delay);
        let cut = audio::Cut {
            start_sec: (-start_delay) as f64,
            length_sec: f64::INFINITY,
            ..Default::default()
        };
        audio::play_music(music_path.clone(), cut, false, music_rate.max(0.01));
    } else {
        warn!("No music path found for song '{}'", song.title);
    }

    let profile = profile::get();
    let scroll_speed = profile.scroll_speed;
    let initial_bpm = timing.get_bpm_for_beat(first_note_beat);

    // THIS IS THE KEY CHANGE: Determine the reference BPM for M-Mods.
    let mut reference_bpm = 
        get_reference_bpm_from_display_tag(&song.display_bpm)
        .unwrap_or_else(|| {
            // Fallback logic: if #DISPLAYBPM is missing, '*', or invalid,
            // use the song's actual max BPM, capped for sanity.
            let mut actual_max = timing.get_capped_max_bpm(Some(M_MOD_HIGH_CAP));
            if !actual_max.is_finite() || actual_max <= 0.0 {
                actual_max = initial_bpm.max(120.0);
            }
            actual_max
        });
    
    if !reference_bpm.is_finite() || reference_bpm <= 0.0 {
        reference_bpm = initial_bpm.max(120.0);
    }

    let mut pixels_per_second = scroll_speed.pixels_per_second(initial_bpm, reference_bpm, music_rate);
    if !pixels_per_second.is_finite() || pixels_per_second <= 0.0 {
        warn!(
            "Scroll speed {} produced non-positive velocity; falling back to default.",
            scroll_speed
        );
        pixels_per_second =
            ScrollSpeedSetting::default().pixels_per_second(initial_bpm, reference_bpm, music_rate);
    }
    let draw_distance_before_targets = screen_height() * DRAW_DISTANCE_BEFORE_TARGETS_MULTIPLIER;
    let draw_distance_after_targets = DRAW_DISTANCE_AFTER_TARGETS;
    let mut travel_time =
        scroll_speed.travel_time_seconds(draw_distance_before_targets, initial_bpm, reference_bpm, music_rate);
    if !travel_time.is_finite() || travel_time <= 0.0 {
        travel_time = draw_distance_before_targets / pixels_per_second;
    }
    info!(
        "Scroll speed set to {} (ref BPM: {:.2}, effective BPM at start: {:.2}), {:.2} px/s",
        scroll_speed,
        reference_bpm,
        scroll_speed.effective_bpm(initial_bpm, reference_bpm, music_rate),
        pixels_per_second
    );

    // Compute and cache the final music end time once per song load
    let last_relevant_second = notes
        .iter()
        .enumerate()
        .fold(0.0_f32, |acc, (i, _)| {
            let start = note_time_cache[i];
            let end = hold_end_time_cache[i].unwrap_or(start);
            acc.max(end)
        });
    let music_end_time = last_relevant_second
        + crate::game::timing::effective_windows_s()[4]
        + TRANSITION_OUT_DURATION;

    State {
        song,
        chart,
        background_texture_key: "__white".to_string(),
        timing,
        notes,
        song_start_instant,
        current_beat: 0.0,
        current_music_time: -start_delay,
        note_spawn_cursor: 0,
        judged_row_cursor: 0,
        arrows: [vec![], vec![], vec![], vec![]],
        note_time_cache,
        note_display_beat_cache,
        hold_end_time_cache,
        music_end_time,
        music_rate: if music_rate.is_finite() && music_rate > 0.0 { music_rate } else { 1.0 },
        judgment_counts: HashMap::from_iter([
            (JudgeGrade::Fantastic, 0),
            (JudgeGrade::Excellent, 0),
            (JudgeGrade::Great, 0),
            (JudgeGrade::Decent, 0),
            (JudgeGrade::WayOff, 0),
            (JudgeGrade::Miss, 0),
        ]),
        scoring_counts: HashMap::from_iter([
            (JudgeGrade::Fantastic, 0),
            (JudgeGrade::Excellent, 0),
            (JudgeGrade::Great, 0),
            (JudgeGrade::Decent, 0),
            (JudgeGrade::WayOff, 0),
            (JudgeGrade::Miss, 0),
        ]),
        combo: 0,
        miss_combo: 0,
        full_combo_grade: None,
        first_fc_attempt_broken: false,
        last_judgment: None,
        hold_judgments: Default::default(),
        life: 0.5,
        combo_after_miss: 0,
        is_failing: false,
        is_in_freeze: false,
        is_in_delay: false,
        fail_time: None,
        earned_grade_points: 0,
        possible_grade_points,
        song_completed_naturally: false,
        noteskin,
        active_color_index,
        player_color: color::decorative_rgba(active_color_index),
        scroll_speed,
        scroll_reference_bpm: reference_bpm,
        scroll_pixels_per_second: pixels_per_second,
        scroll_travel_time: travel_time,
        draw_distance_before_targets,
        draw_distance_after_targets,
        receptor_glow_timers: [0.0; 4],
        receptor_bop_timers: [0.0; 4],
        tap_explosions: Default::default(),
        mine_explosions: Default::default(),
        active_holds: Default::default(),
        combo_milestones: Vec::new(),
        hands_achieved: 0,
        holds_total,
        holds_held: 0,
        holds_held_for_score: 0,
        rolls_total,
        rolls_held: 0,
        rolls_held_for_score: 0,
        mines_total,
        mines_hit: 0,
        mines_hit_for_score: 0,
        mines_avoided: 0,
        hands_holding_count_for_stats: 0,
        total_elapsed_in_screen: 0.0,
        hold_to_exit_key: None,
        hold_to_exit_start: None,
        prev_inputs: [false; 4],
        keyboard_lane_state: [false; 4],
        gamepad_lane_state: [false; 4],
        pending_edges: VecDeque::new(),
        log_timer: 0.0,
    }
}

fn update_itg_grade_totals(state: &mut State) {
    state.earned_grade_points = judgment::calculate_itg_grade_points(
        &state.scoring_counts,
        state.holds_held_for_score,
        state.rolls_held_for_score,
        state.mines_hit_for_score,
    );
}

fn grade_to_window(grade: JudgeGrade) -> Option<&'static str> {
    match grade {
        JudgeGrade::Fantastic => Some("W1"),
        JudgeGrade::Excellent => Some("W2"),
        JudgeGrade::Great => Some("W3"),
        JudgeGrade::Decent => Some("W4"),
        JudgeGrade::WayOff => Some("W5"),
        JudgeGrade::Miss => None,
    }
}

fn trigger_tap_explosion(state: &mut State, column: usize, grade: JudgeGrade) {
    let Some(window_key) = grade_to_window(grade) else {
        return;
    };

    let spawn_window = state.noteskin.as_ref().and_then(|ns| {
        if ns.tap_explosions.contains_key(window_key) {
            Some(window_key.to_string())
        } else {
            None
        }
    });

    if let Some(window) = spawn_window {
        state.tap_explosions[column] = Some(ActiveTapExplosion {
            window,
            elapsed: 0.0,
            start_beat: state.current_beat,
        });
    }
}

fn trigger_mine_explosion(state: &mut State, column: usize) {
    state.mine_explosions[column] = Some(ActiveMineExplosion { elapsed: 0.0 });
}

fn trigger_combo_milestone(state: &mut State, kind: ComboMilestoneKind) {
    if let Some(index) = state
        .combo_milestones
        .iter()
        .position(|milestone| milestone.kind == kind)
    {
        state.combo_milestones[index].elapsed = 0.0;
    } else {
        state
            .combo_milestones
            .push(ActiveComboMilestone { kind, elapsed: 0.0 });
    }
}

fn handle_mine_hit(
    state: &mut State,
    column: usize,
    arrow_list_index: usize,
    note_index: usize,
    time_error: f32,
) -> bool {
    let abs_time_error = time_error.abs();
    let mine_window = crate::game::timing::mine_window_s();
    if abs_time_error > mine_window {
        return false;
    }

    if state.notes[note_index].mine_result.is_some() {
        return false;
    }

    state.notes[note_index].mine_result = Some(MineResult::Hit);
    state.mines_hit = state.mines_hit.saturating_add(1);
    let mut updated_scoring = false;

    let note_row_index = state.notes[note_index].row_index;
    info!(
        "MINE HIT: Row {}, Col {}, Error: {:.2}ms",
        note_row_index,
        column,
        time_error * 1000.0
    );

    state.arrows[column].remove(arrow_list_index);
    apply_life_change(state, LifeChange::HIT_MINE);
    if !is_state_dead(state) {
        state.mines_hit_for_score = state.mines_hit_for_score.saturating_add(1);
        updated_scoring = true;
    }
    state.combo = 0;
    state.miss_combo = state.miss_combo.saturating_add(1);
    if state.full_combo_grade.is_some() {
        state.first_fc_attempt_broken = true;
    }
    state.full_combo_grade = None;
    state.receptor_glow_timers[column] = 0.0;
    trigger_mine_explosion(state, column);
    audio::play_sfx("assets/sounds/boom.ogg");

    if updated_scoring {
        update_itg_grade_totals(state);
    }

    true
}

fn try_hit_mine_while_held(state: &mut State, column: usize, current_time: f32) -> bool {
    let candidate = {
        let arrows = &state.arrows[column];
        let notes = &state.notes;
        let note_times = &state.note_time_cache;

        arrows.iter().enumerate().find_map(|(idx, arrow)| {
            let note = &notes[arrow.note_index];
            if !matches!(note.note_type, NoteType::Mine) || note.mine_result.is_some() {
                return None;
            }

            let note_time = note_times[arrow.note_index];
            Some((idx, arrow.note_index, note_time))
        })
    };

    let Some((arrow_idx, note_index, note_time)) = candidate else {
        return false;
    };

    let time_error = current_time - note_time;
    handle_mine_hit(state, column, arrow_idx, note_index, time_error)
}

fn handle_hold_let_go(state: &mut State, column: usize, note_index: usize) {
    if let Some(hold) = state.notes[note_index].hold.as_mut() {
        if hold.result == Some(HoldResult::LetGo) {
            return;
        }
        hold.result = Some(HoldResult::LetGo);
        if hold.let_go_started_at.is_none() {
            hold.let_go_started_at = Some(state.current_music_time);
            hold.let_go_starting_life = hold.life.clamp(0.0, MAX_HOLD_LIFE);
        }
    }

    if state.hands_holding_count_for_stats > 0 {
        state.hands_holding_count_for_stats -= 1;
    }

    state.hold_judgments[column] = Some(HoldJudgmentRenderInfo {
        result: HoldResult::LetGo,
        triggered_at: Instant::now(),
    });

    apply_life_change(state, LifeChange::LET_GO);
    if !is_state_dead(state) {
        update_itg_grade_totals(state);
    }
    state.combo = 0;
    state.miss_combo = state.miss_combo.saturating_add(1);
    if state.full_combo_grade.is_some() {
        state.first_fc_attempt_broken = true;
    }
    state.full_combo_grade = None;
    state.receptor_glow_timers[column] = 0.0;
}

fn handle_hold_success(state: &mut State, column: usize, note_index: usize) {
    if let Some(hold) = state.notes[note_index].hold.as_mut() {
        if hold.result == Some(HoldResult::Held) {
            return;
        }
        hold.result = Some(HoldResult::Held);
        hold.life = MAX_HOLD_LIFE;
        hold.let_go_started_at = None;
        hold.let_go_starting_life = 0.0;
        hold.last_held_row_index = hold.end_row_index;
        hold.last_held_beat = hold.end_beat;
    }

    if state.hands_holding_count_for_stats > 0 {
        state.hands_holding_count_for_stats -= 1;
    }

    let mut updated_scoring = false;
    match state.notes[note_index].note_type {
        NoteType::Hold => {
            state.holds_held = state.holds_held.saturating_add(1);
            if !is_state_dead(state) {
                state.holds_held_for_score = state.holds_held_for_score.saturating_add(1);
                updated_scoring = true;
            }
        }
        NoteType::Roll => {
            state.rolls_held = state.rolls_held.saturating_add(1);
            if !is_state_dead(state) {
                state.rolls_held_for_score = state.rolls_held_for_score.saturating_add(1);
                updated_scoring = true;
            }
        }
        _ => {}
    }
    apply_life_change(state, LifeChange::HELD);

    if updated_scoring {
        update_itg_grade_totals(state);
    }
    state.miss_combo = 0;

    trigger_tap_explosion(state, column, JudgeGrade::Excellent);

    state.hold_judgments[column] = Some(HoldJudgmentRenderInfo {
        result: HoldResult::Held,
        triggered_at: Instant::now(),
    });
}

fn refresh_roll_life_on_step(state: &mut State, column: usize) {
    let Some(active) = state.active_holds[column].as_mut() else {
        return;
    };

    if !matches!(active.note_type, NoteType::Roll) || active.let_go {
        return;
    }

    let Some(note) = state.notes.get_mut(active.note_index) else {
        return;
    };
    let Some(hold) = note.hold.as_mut() else {
        return;
    };
    if hold.result == Some(HoldResult::LetGo) {
        return;
    }

    active.life = MAX_HOLD_LIFE;
    hold.life = MAX_HOLD_LIFE;
    hold.let_go_started_at = None;
    hold.let_go_starting_life = 0.0;
}

fn update_active_holds(state: &mut State, inputs: &[bool; 4], current_time: f32, delta_time: f32) {
    for column in 0..state.active_holds.len() {
        let mut handle_let_go = None;
        let mut handle_success = None;

        {
            let active_opt = &mut state.active_holds[column];
            if let Some(active) = active_opt {
                let note_index = active.note_index;
                let note_start_row = state.notes[note_index].row_index;
                let note_start_beat = state.notes[note_index].beat;

                let Some(hold) = state.notes[note_index].hold.as_mut() else {
                    *active_opt = None;
                    continue;
                };

                if !active.let_go && active.life > 0.0 {
                    let prev_row = hold.last_held_row_index;
                    let prev_beat = hold.last_held_beat;
                    let mut current_row = state
                        .timing
                        .get_row_for_beat(state.current_beat)
                        .unwrap_or(note_start_row);
                    current_row = current_row.clamp(note_start_row, hold.end_row_index);
                    let final_row = prev_row.max(current_row);
                    if final_row != prev_row {
                        hold.last_held_row_index = final_row;
                        let mut new_beat = state
                            .timing
                            .get_beat_for_row(final_row)
                            .unwrap_or(state.current_beat);
                        new_beat = new_beat.clamp(note_start_beat, hold.end_beat);
                        if new_beat < prev_beat {
                            new_beat = prev_beat;
                        }
                        hold.last_held_beat = new_beat;
                    } else {
                        hold.last_held_beat = prev_beat.clamp(note_start_beat, hold.end_beat);
                    }
                }

                let pressed = inputs[column];
                active.is_pressed = pressed;

                if !active.let_go {
                    let window = match active.note_type {
                        NoteType::Hold => TIMING_WINDOW_SECONDS_HOLD,
                        NoteType::Roll => TIMING_WINDOW_SECONDS_ROLL,
                        _ => TIMING_WINDOW_SECONDS_HOLD,
                    };

                    match active.note_type {
                        NoteType::Hold => {
                            if pressed {
                                active.life = MAX_HOLD_LIFE;
                            } else if window > 0.0 {
                                // Degrade life in music-time seconds
                                active.life -= (delta_time * state.music_rate) / window;
                            } else {
                                active.life = 0.0;
                            }
                        }
                        NoteType::Roll => {
                            if window > 0.0 {
                                active.life -= (delta_time * state.music_rate) / window;
                            } else {
                                active.life = 0.0;
                            }
                        }
                        _ => {
                            if window > 0.0 {
                                active.life -= (delta_time * state.music_rate) / window;
                            } else {
                                active.life = 0.0;
                            }
                        }
                    }

                    if active.life < 0.0 {
                        active.life = 0.0;
                    } else if active.life > MAX_HOLD_LIFE {
                        active.life = MAX_HOLD_LIFE;
                    }
                }

                hold.life = active.life;
                hold.let_go_started_at = None;
                hold.let_go_starting_life = 0.0;

                if !active.let_go && active.life <= 0.0 {
                    active.let_go = true;
                    handle_let_go = Some((column, note_index));
                }

                if current_time >= active.end_time {
                    if !active.let_go && active.life > 0.0 {
                        handle_success = Some((column, note_index));
                    } else if !active.let_go {
                        active.let_go = true;
                        handle_let_go = Some((column, note_index));
                    }
                    *active_opt = None;
                } else if active.let_go {
                    *active_opt = None;
                }
            }
        }

        if let Some((column, note_index)) = handle_let_go {
            handle_hold_let_go(state, column, note_index);
        }

        if let Some((column, note_index)) = handle_success {
            handle_hold_success(state, column, note_index);
        }
    }
}

pub fn judge_a_tap(state: &mut State, column: usize, current_time: f32) -> bool {
    if let Some((arrow_list_index, arrow_to_judge)) = state.arrows[column]
        .iter()
        .enumerate()
        .find(|(_, arrow)| state.notes[arrow.note_index].result.is_none())
        .map(|(idx, arrow)| (idx, arrow.clone()))
    {
        let note_index = arrow_to_judge.note_index;
        let note_row_index = state.notes[note_index].row_index;
        let note_type = state.notes[note_index].note_type.clone();
        let note_time = state.note_time_cache[note_index];
        let time_error = current_time - note_time;
        let abs_time_error = time_error.abs();

        if matches!(note_type, NoteType::Mine) {
            if handle_mine_hit(state, column, arrow_list_index, note_index, time_error) {
                return true;
            }
            return false;
        }

        let windows = crate::game::timing::effective_windows_s();
        let fantastic_window = windows[0];
        let excellent_window = windows[1];
        let great_window = windows[2];
        let decent_window = windows[3];
        let way_off_window = windows[4];

        if abs_time_error <= way_off_window {
            let grade = if abs_time_error <= fantastic_window {
                JudgeGrade::Fantastic
            } else if abs_time_error <= excellent_window {
                JudgeGrade::Excellent
            } else if abs_time_error <= great_window {
                JudgeGrade::Great
            } else if abs_time_error <= decent_window {
                JudgeGrade::Decent
            } else {
                JudgeGrade::WayOff
            };

            let judgment = Judgment {
                time_error_ms: time_error * 1000.0,
                grade,
                row: note_row_index,
            };

            state.notes[note_index].result = Some(judgment);
            let note_type = state.notes[note_index].note_type.clone();
            let hold_end_time = state.hold_end_time_cache[note_index];
            info!(
                "JUDGED (pending): Row {}, Col {}, Error: {:.2}ms, Grade: {:?}",
                note_row_index,
                column,
                time_error * 1000.0,
                grade
            );

            state.arrows[column].remove(arrow_list_index);
            state.receptor_glow_timers[column] = RECEPTOR_GLOW_DURATION;
            trigger_tap_explosion(state, column, grade);

            if matches!(note_type, NoteType::Hold | NoteType::Roll) {
                if let Some(end_time) = hold_end_time {
                    if let Some(hold) = state.notes[note_index].hold.as_mut() {
                        hold.life = MAX_HOLD_LIFE;
                    }
                    state.active_holds[column] = Some(ActiveHold {
                        note_index,
                        end_time,
                        note_type,
                        let_go: false,
                        is_pressed: true,
                        life: MAX_HOLD_LIFE,
                    });
                }
            }

            return true;
        }
    }
    false
}

pub fn handle_key_press(state: &mut State, event: &KeyEvent, timestamp: Instant) -> ScreenAction {
    if let PhysicalKey::Code(key_code) = event.physical_key {
        if event.state == ElementState::Pressed && event.repeat {
            return ScreenAction::None;
        }

        if let Some(lane) = lane_from_keycode(key_code) {
            let pressed = event.state == ElementState::Pressed;
            queue_input_edge(state, InputSource::Keyboard, lane, pressed, timestamp);
        }

        match event.state {
            ElementState::Pressed => {
                if key_code == KeyCode::Escape || key_code == KeyCode::Enter {
                    state.hold_to_exit_key = Some(key_code);
                    state.hold_to_exit_start = Some(timestamp);
                    return ScreenAction::None;
                }
            }
            ElementState::Released => {
                if state.hold_to_exit_key == Some(key_code) {
                    state.hold_to_exit_key = None;
                    state.hold_to_exit_start = None;
                }
            }
        }
    }
    ScreenAction::None
}

fn finalize_row_judgment(state: &mut State, row_index: usize, judgments_in_row: Vec<Judgment>) {
    if judgments_in_row.is_empty() {
        return;
    }

    let row_has_miss = judgments_in_row
        .iter()
        .any(|judgment| judgment.grade == JudgeGrade::Miss);
    let row_has_successful_hit = judgments_in_row.iter().any(|judgment| {
        matches!(
            judgment.grade,
            JudgeGrade::Fantastic | JudgeGrade::Excellent | JudgeGrade::Great
        )
    });

    // Determine the single, row-level judgment used for life/combo/scoring.
    let final_judgment = if row_has_miss {
        judgments_in_row
            .iter()
            .find(|judgment| judgment.grade == JudgeGrade::Miss)
            .cloned()
    } else {
        judgments_in_row
            .iter()
            // Use absolute magnitude to determine the worst (furthest from 0ms)
            .max_by(|a, b| {
                a.time_error_ms
                    .abs()
                    .partial_cmp(&b.time_error_ms.abs())
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .cloned()
    };

    let Some(final_judgment) = final_judgment else {
        return;
    };
    let final_grade = final_judgment.grade;

    // Increment counts ONCE per row (jumps/hands should not overcount).
    *state.judgment_counts.entry(final_grade).or_insert(0) += 1;
    if !is_state_dead(state) {
        *state.scoring_counts.entry(final_grade).or_insert(0) += 1;
        update_itg_grade_totals(state);
    }

    info!(
        "FINALIZED: Row {}, Grade: {:?}, Offset: {:.2}ms",
        final_judgment.row, final_grade, final_judgment.time_error_ms
    );

    let life_delta = match final_grade {
        JudgeGrade::Fantastic => LifeChange::FANTASTIC,
        JudgeGrade::Excellent => LifeChange::EXCELLENT,
        JudgeGrade::Great => LifeChange::GREAT,
        JudgeGrade::Decent => LifeChange::DECENT,
        JudgeGrade::WayOff => LifeChange::WAY_OFF,
        JudgeGrade::Miss => LifeChange::MISS,
    };
    apply_life_change(state, life_delta);

    state.last_judgment = Some(JudgmentRenderInfo {
        judgment: final_judgment,
        judged_at: Instant::now(),
    });

    if row_has_successful_hit {
        state.miss_combo = 0;
    }
    if row_has_miss {
        state.miss_combo = state.miss_combo.saturating_add(1);
    }

    if row_has_miss || matches!(final_grade, JudgeGrade::Decent | JudgeGrade::WayOff) {
        state.combo = 0;
        if state.full_combo_grade.is_some() {
            state.first_fc_attempt_broken = true;
        }
        state.full_combo_grade = None;
    } else {
        state.combo += 1;

        let combo = state.combo;
        if combo > 0 && combo % 1000 == 0 {
            trigger_combo_milestone(state, ComboMilestoneKind::Thousand);
            trigger_combo_milestone(state, ComboMilestoneKind::Hundred);
        } else if combo > 0 && combo % 100 == 0 {
            trigger_combo_milestone(state, ComboMilestoneKind::Hundred);
        }

        if !state.first_fc_attempt_broken {
            let new_grade = if let Some(current_fc_grade) = &state.full_combo_grade {
                final_grade.max(*current_fc_grade)
            } else {
                final_grade
            };
            state.full_combo_grade = Some(new_grade);
        }
    }

    let mut successful_steps: u32 = 0;
    let mut holds_started_this_row: u32 = 0;

    for note in state
        .notes
        .iter()
        .filter(|n| n.row_index == row_index && !matches!(n.note_type, NoteType::Mine))
    {
        if note
            .result
            .as_ref()
            .is_some_and(|judgment| judgment.grade != JudgeGrade::Miss)
        {
            successful_steps = successful_steps.saturating_add(1);
            if matches!(note.note_type, NoteType::Hold | NoteType::Roll) {
                holds_started_this_row = holds_started_this_row.saturating_add(1);
            }
        }
    }

    let holding_before_row = state.hands_holding_count_for_stats.max(0) as u32;
    if successful_steps > 0 && successful_steps + holding_before_row >= 3 {
        state.hands_achieved = state.hands_achieved.saturating_add(1);
    }

    state.hands_holding_count_for_stats = state
        .hands_holding_count_for_stats
        .saturating_add(holds_started_this_row as i32);
}

fn update_judged_rows(state: &mut State) {
    loop {
        let max_row_index = state.notes.iter().map(|n| n.row_index).max().unwrap_or(0);

        if state.judged_row_cursor > max_row_index {
            break;
        }

        let is_row_complete = {
            let notes_on_row: Vec<&Note> = state
                .notes
                .iter()
                .filter(|n| n.row_index == state.judged_row_cursor)
                .collect();
            notes_on_row.is_empty()
                || notes_on_row.iter().all(|n| match n.note_type {
                    NoteType::Mine => n.mine_result.is_some(),
                    _ => n.result.is_some(),
                })
        };

        if is_row_complete {
            let judgments_on_row: Vec<Judgment> = state
                .notes
                .iter()
                .filter(|n| n.row_index == state.judged_row_cursor)
                .filter(|n| !matches!(n.note_type, NoteType::Mine))
                .filter_map(|n| n.result.clone())
                .collect();

            finalize_row_judgment(state, state.judged_row_cursor, judgments_on_row);
            state.judged_row_cursor += 1;
        } else {
            break;
        }
    }
}

// get_music_end_time removed; use state.music_end_time directly

#[inline(always)]
fn process_input_edges(state: &mut State, music_time_sec: f32, now: Instant) {
    while let Some(edge) = state.pending_edges.pop_front() {
        let lane_idx = edge.lane.index();
        let was_down = state.keyboard_lane_state[lane_idx] || state.gamepad_lane_state[lane_idx];

        match edge.source {
            InputSource::Keyboard => state.keyboard_lane_state[lane_idx] = edge.pressed,
            InputSource::Gamepad => state.gamepad_lane_state[lane_idx] = edge.pressed,
        }

        let is_down = state.keyboard_lane_state[lane_idx] || state.gamepad_lane_state[lane_idx];

        if edge.pressed && is_down && !was_down {
            let elapsed = now.saturating_duration_since(edge.timestamp).as_secs_f32();
            // Convert real-time elapsed to music timeline seconds by scaling with music_rate
            let event_music_time = music_time_sec - elapsed * state.music_rate;
            let hit_note = judge_a_tap(state, lane_idx, event_music_time);
            refresh_roll_life_on_step(state, lane_idx);
            if !hit_note {
                state.receptor_bop_timers[lane_idx] = 0.11;
            }
        }
    }
}

#[inline(always)]
fn decay_let_go_hold_life(state: &mut State) {
    for note in &mut state.notes {
        let Some(hold) = note.hold.as_mut() else { continue; };
        if hold.result == Some(HoldResult::Held) { continue; }
        let Some(start_time) = hold.let_go_started_at else { continue; };

        let base_life = hold.let_go_starting_life.clamp(0.0, MAX_HOLD_LIFE);
        if base_life <= 0.0 { hold.life = 0.0; continue; }

        let window = match note.note_type {
            NoteType::Roll => TIMING_WINDOW_SECONDS_ROLL,
            _ => TIMING_WINDOW_SECONDS_HOLD,
        };
        if window <= 0.0 { hold.life = 0.0; continue; }

        let elapsed = (state.current_music_time - start_time).max(0.0);
        hold.life = (base_life - elapsed / window).max(0.0);
    }
}

#[inline(always)]
fn tick_visual_effects(state: &mut State, delta_time: f32) {
    for timer in &mut state.receptor_glow_timers {
        *timer = (*timer - delta_time).max(0.0);
    }
    for timer in &mut state.receptor_bop_timers {
        *timer = (*timer - delta_time).max(0.0);
    }

    state.combo_milestones.retain_mut(|milestone| {
        milestone.elapsed += delta_time;
        let max_duration = match milestone.kind {
            ComboMilestoneKind::Hundred => COMBO_HUNDRED_MILESTONE_DURATION,
            ComboMilestoneKind::Thousand => COMBO_THOUSAND_MILESTONE_DURATION,
        };
        milestone.elapsed < max_duration
    });

    for explosion in &mut state.tap_explosions {
        if let Some(active) = explosion {
            active.elapsed += delta_time;
            let lifetime = state
                .noteskin
                .as_ref()
                .and_then(|ns| ns.tap_explosions.get(&active.window))
                .map(|explosion| explosion.animation.duration())
                .unwrap_or(0.0);
            if lifetime <= 0.0 || active.elapsed >= lifetime {
                *explosion = None;
            }
        }
    }

    for explosion in &mut state.mine_explosions {
        if let Some(active) = explosion {
            active.elapsed += delta_time;
            if active.elapsed >= MINE_EXPLOSION_DURATION {
                *explosion = None;
            }
        }
    }

    for slot in &mut state.hold_judgments {
        if let Some(render_info) = slot {
            if render_info.triggered_at.elapsed().as_secs_f32() >= HOLD_JUDGMENT_TOTAL_DURATION {
                *slot = None;
            }
        }
    }
}

#[inline(always)]
fn spawn_lookahead_arrows(state: &mut State, music_time_sec: f32) {
    // scroll_travel_time is in chart-time seconds; current_music_time is also chart-time.
    // Do not scale by rate here, or arrows will spawn too late/early at high/low rates.
    let lookahead_time = music_time_sec + state.scroll_travel_time;
    let lookahead_beat = state.timing.get_beat_for_time(lookahead_time);
    while state.note_spawn_cursor < state.notes.len()
        && state.notes[state.note_spawn_cursor].beat < lookahead_beat
    {
        let note = &state.notes[state.note_spawn_cursor];
        state.arrows[note.column].push(Arrow {
            beat: note.beat,
            column: note.column,
            note_type: note.note_type.clone(),
            note_index: state.note_spawn_cursor,
        });
        state.note_spawn_cursor += 1;
    }
}

#[inline(always)]
fn apply_passive_misses_and_mine_avoidance(state: &mut State, music_time_sec: f32) {
    let way_off_window = crate::game::timing::effective_windows_s()[4];
    for (col_idx, col_arrows) in state.arrows.iter_mut().enumerate() {
        let Some(next_arrow_index) = col_arrows
            .iter()
            .position(|arrow| state.notes[arrow.note_index].result.is_none())
        else { continue; };

        let arrow = col_arrows[next_arrow_index].clone();
        let note_index = arrow.note_index;
        let (note_row_index, note_type) = {
            let note = &state.notes[note_index];
            (note.row_index, note.note_type.clone())
        };
        let note_time = state.note_time_cache[note_index];

        if matches!(note_type, NoteType::Mine) {
            match state.notes[note_index].mine_result {
                Some(MineResult::Hit) => { col_arrows.remove(next_arrow_index); }
                Some(MineResult::Avoided) => {}
                None => {
                    let mine_window = crate::game::timing::mine_window_s();
                    if music_time_sec - note_time > mine_window {
                        state.notes[note_index].mine_result = Some(MineResult::Avoided);
                        state.mines_avoided = state.mines_avoided.saturating_add(1);
                        info!(
                            "MINE AVOIDED: Row {}, Col {}, Time: {:.2}s",
                            note_row_index, col_idx, music_time_sec
                        );
                    }
                }
            }
            continue;
        }

        if music_time_sec - note_time > way_off_window {
            let judgment = Judgment {
                time_error_ms: ((music_time_sec - note_time) * 1000.0),
                grade: JudgeGrade::Miss,
                row: note_row_index,
            };

            if let Some(hold) = state.notes[note_index].hold.as_mut() {
                if hold.result != Some(HoldResult::Held) {
                    hold.result = Some(HoldResult::LetGo);
                    if hold.let_go_started_at.is_none() {
                        hold.let_go_started_at = Some(music_time_sec);
                        hold.let_go_starting_life = hold.life.clamp(0.0, MAX_HOLD_LIFE);
                    }
                }
            }

            state.notes[note_index].result = Some(judgment);
            info!(
                "MISSED (pending): Row {}, Col {}",
                note_row_index, col_idx
            );
        }
    }
}

#[inline(always)]
fn cull_scrolled_out_arrows(state: &mut State, music_time_sec: f32) {
    let receptor_y = screen_center_y() + RECEPTOR_Y_OFFSET_FROM_CENTER;
    let miss_cull_threshold = receptor_y - state.draw_distance_after_targets;

    let (cmod_pps_opt, curr_disp_beat, beatmod_multiplier) = match state.scroll_speed {
        ScrollSpeedSetting::CMod(c_bpm) => {
            let pps = (c_bpm / 60.0) * ScrollSpeedSetting::ARROW_SPACING;
            (Some(pps), 0.0, 0.0)
        }
        ScrollSpeedSetting::XMod(_) | ScrollSpeedSetting::MMod(_) => {
            let curr_disp = state.timing.get_displayed_beat(state.current_beat);
            let speed_multiplier = state
                .timing
                .get_speed_multiplier(state.current_beat, state.current_music_time);
            let player_multiplier = state
                .scroll_speed
                .beat_multiplier(state.scroll_reference_bpm, state.music_rate);
            (None, curr_disp, player_multiplier * speed_multiplier)
        }
    };

    for col_arrows in &mut state.arrows {
        col_arrows.retain(|arrow| {
            let note = &state.notes[arrow.note_index];
            if matches!(note.note_type, NoteType::Mine) {
                match note.mine_result {
                    Some(MineResult::Avoided) => {}
                    Some(MineResult::Hit) => return false,
                    None => return true,
                }
            } else {
                let Some(judgment) = note.result.as_ref() else { return true; };
                if judgment.grade != JudgeGrade::Miss { return false; }
            }

            let y_pos = match state.scroll_speed {
                ScrollSpeedSetting::CMod(_) => {
                    let pps_chart = cmod_pps_opt.expect("cmod pps computed");
                    let note_time_chart = state.note_time_cache[arrow.note_index];
                    let rate = if state.music_rate.is_finite() && state.music_rate > 0.0 { state.music_rate } else { 1.0 };
                    let time_diff_real = (note_time_chart - music_time_sec) / rate;
                    receptor_y + time_diff_real * pps_chart
                }
                ScrollSpeedSetting::XMod(_) | ScrollSpeedSetting::MMod(_) => {
                    let note_disp_beat = state.note_display_beat_cache[arrow.note_index];
                    let beat_diff_disp = note_disp_beat - curr_disp_beat;
                    receptor_y
                        + beat_diff_disp
                            * ScrollSpeedSetting::ARROW_SPACING
                            * beatmod_multiplier
                }
            };

            y_pos >= miss_cull_threshold
        });
    }
}

pub fn update(state: &mut State, delta_time: f32) -> ScreenAction {
    if let (Some(key), Some(start_time)) = (state.hold_to_exit_key, state.hold_to_exit_start) {
        if start_time.elapsed() >= std::time::Duration::from_secs(1) {
            state.hold_to_exit_key = None;
            state.hold_to_exit_start = None;
            return match key {
                winit::keyboard::KeyCode::Enter => ScreenAction::Navigate(Screen::Evaluation),
                winit::keyboard::KeyCode::Escape => ScreenAction::Navigate(Screen::SelectMusic),
                _ => ScreenAction::None,
            };
        }
    }

    state.total_elapsed_in_screen += delta_time;

    let now = std::time::Instant::now();
    let music_time_sec = if now < state.song_start_instant {
        -(state
            .song_start_instant
            .saturating_duration_since(now)
            .as_secs_f32())
    } else {
        // Scale positive elapsed by music rate to keep gameplay in sync with audio speed
        now.saturating_duration_since(state.song_start_instant)
            .as_secs_f32() * state.music_rate
    };
    state.current_music_time = music_time_sec;
	let beat_info = state.timing.get_beat_info_from_time(music_time_sec);
	state.current_beat = beat_info.beat;
	state.is_in_freeze = beat_info.is_in_freeze;
	state.is_in_delay = beat_info.is_in_delay;

    let current_bpm = state.timing.get_bpm_for_beat(state.current_beat);

    let mut dynamic_speed = state
        .scroll_speed
        .pixels_per_second(current_bpm, state.scroll_reference_bpm, state.music_rate);
    if !dynamic_speed.is_finite() || dynamic_speed <= 0.0 {
        dynamic_speed = ScrollSpeedSetting::default()
            .pixels_per_second(current_bpm, state.scroll_reference_bpm, state.music_rate);
    }
    state.scroll_pixels_per_second = dynamic_speed;

    let draw_distance_before_targets = screen_height() * DRAW_DISTANCE_BEFORE_TARGETS_MULTIPLIER;
    state.draw_distance_before_targets = draw_distance_before_targets;
    state.draw_distance_after_targets = DRAW_DISTANCE_AFTER_TARGETS;
    let mut travel_time = state.scroll_speed.travel_time_seconds(
        draw_distance_before_targets,
        current_bpm,
        state.scroll_reference_bpm,
        state.music_rate,
    );
    if !travel_time.is_finite() || travel_time <= 0.0 {
        travel_time = draw_distance_before_targets / dynamic_speed;
    }
    state.scroll_travel_time = travel_time;

    if state.current_music_time >= state.music_end_time {
        info!("Music end time reached. Transitioning to evaluation.");
        state.song_completed_naturally = true;
        return ScreenAction::Navigate(Screen::Evaluation);
    }

    process_input_edges(state, music_time_sec, now);

    let current_inputs = [
        state.keyboard_lane_state[0] || state.gamepad_lane_state[0],
        state.keyboard_lane_state[1] || state.gamepad_lane_state[1],
        state.keyboard_lane_state[2] || state.gamepad_lane_state[2],
        state.keyboard_lane_state[3] || state.gamepad_lane_state[3],
    ];
    let prev_inputs = state.prev_inputs;

    for (col, (now_down, was_down)) in current_inputs.iter().copied().zip(prev_inputs).enumerate() {
        if now_down && was_down {
            let _ = try_hit_mine_while_held(state, col, music_time_sec);
        }
    }

    state.prev_inputs = current_inputs;

    update_active_holds(state, &current_inputs, music_time_sec, delta_time);
    decay_let_go_hold_life(state);

    tick_visual_effects(state, delta_time);

    spawn_lookahead_arrows(state, music_time_sec);

    apply_passive_misses_and_mine_avoidance(state, music_time_sec);

    cull_scrolled_out_arrows(state, music_time_sec);

    update_judged_rows(state);

    state.log_timer += delta_time;
    if state.log_timer >= 1.0 {
        let active_arrows: usize = state.arrows.iter().map(|v| v.len()).sum();
        log::info!(
            "Beat: {:.2}, Time: {:.2}, Combo: {}, Misses: {}, Active Arrows: {}",
            state.current_beat,
            music_time_sec,
            state.combo,
            state.miss_combo,
            active_arrows
        );
        state.log_timer -= 1.0;
    }

    ScreenAction::None
}
