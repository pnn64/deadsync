use crate::core::audio;
use crate::core::input::{InputEdge, InputSource, Lane, InputEvent, VirtualAction, lane_from_action};
use crate::core::space::*;
use crate::game::chart::ChartData;
use crate::game::judgment::{self, JudgeGrade, Judgment};
use crate::game::note::{HoldData, HoldResult, MineResult, Note, NoteType};
use crate::game::parsing::notes as note_parser;
use crate::game::parsing::noteskin::{self, Noteskin, Style};
use crate::game::song::SongData;
use crate::game::timing::{TimingData, TimingProfile, classify_offset_s};
use crate::game::{
    life::{LifeChange, REGEN_COMBO_AFTER_MISS},
    profile,
    scroll::ScrollSpeedSetting,
};
use crate::screens::{Screen, ScreenAction};
use crate::ui::color;
use log::{info, debug};
use std::collections::{HashMap, VecDeque};
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};
use winit::event::KeyEvent;
use winit::keyboard::KeyCode;

pub const TRANSITION_IN_DURATION: f32 = 0.4;
pub const TRANSITION_OUT_DURATION: f32 = 0.4;

// These mirror ScreenGameplay's MinSecondsToStep/MinSecondsToMusic metrics in ITGmania.
// Simply Love scales them by MusicRate, so we apply that in init().
const MIN_SECONDS_TO_STEP: f32 = 6.0;
const MIN_SECONDS_TO_MUSIC: f32 = 2.0;
// Additional linger time on ScreenGameplay after the last judgable note,
// approximating OutTransitionLength (5s) so that the perceived wait before
// ScreenEvaluation matches ITGmania/Simply Love.
const POST_SONG_DISPLAY_SECONDS: f32 = 5.0;
const M_MOD_HIGH_CAP: f32 = 600.0;

// Timing windows now sourced from game::timing

pub const RECEPTOR_Y_OFFSET_FROM_CENTER: f32 = -125.0;
pub const RECEPTOR_Y_OFFSET_FROM_CENTER_REVERSE: f32 = 145.0;
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

#[inline(always)]
fn quantize_offset_seconds(v: f32) -> f32 {
    let step = 0.001_f32;
    (v / step).round() * step
}

fn compute_music_end_time(
    notes: &[Note],
    note_time_cache: &[f32],
    hold_end_time_cache: &[Option<f32>],
    rate: f32,
) -> f32 {
    let last_relevant_second = notes
        .iter()
        .enumerate()
        .fold(0.0_f32, |acc, (i, _)| {
            let start = note_time_cache[i];
            let end = hold_end_time_cache[i].unwrap_or(start);
            acc.max(end)
        });

    let timing_profile = TimingProfile::default_itg_with_fa_plus();
    let mut max_window = timing_profile
        .windows_s
        .iter()
        .copied()
        .fold(0.0_f32, f32::max);
    max_window = max_window.max(timing_profile.mine_window_s);
    max_window = max_window.max(TIMING_WINDOW_SECONDS_HOLD);
    max_window = max_window.max(TIMING_WINDOW_SECONDS_ROLL);

    let max_step_distance = rate * max_window;
    let last_step_seconds = last_relevant_second + max_step_distance;
    last_step_seconds + POST_SONG_DISPLAY_SECONDS
}

#[derive(Clone, Debug)]
pub(crate) struct RowEntry {
    row_index: usize,
    // Non-mine, non-fake, judgable notes on this row
    nonmine_note_indices: Vec<usize>,
}

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

#[inline(always)]
fn compute_column_scroll_dirs(scroll_option: profile::ScrollOption) -> [f32; 4] {
    use profile::ScrollOption;
    let mut dirs = [1.0_f32, 1.0_f32, 1.0_f32, 1.0_f32];

    if scroll_option.contains(ScrollOption::Reverse) {
        for d in &mut dirs { *d *= -1.0; }
    }
    if scroll_option.contains(ScrollOption::Split) {
        dirs[2] *= -1.0; dirs[3] *= -1.0;
    }
    if scroll_option.contains(ScrollOption::Alternate) {
        dirs[1] *= -1.0; dirs[3] *= -1.0;
    }
    if scroll_option.contains(ScrollOption::Cross) {
        dirs[1] *= -1.0; dirs[2] *= -1.0;
    }
    dirs
}

pub struct State {
    pub song: Arc<SongData>,
    pub background_texture_key: String,
    pub chart: Arc<ChartData>,
    pub timing: Arc<TimingData>,
    pub timing_profile: TimingProfile,
    pub notes: Vec<Note>,

    pub song_start_instant: Instant,
    pub current_beat: f32,
    pub current_music_time: f32,
    pub note_spawn_cursor: usize,
    pub judged_row_cursor: usize,
    pub arrows: [Vec<Arrow>; 4],
    pub note_time_cache: Vec<f32>,
    pub note_display_beat_cache: Vec<f32>,
    pub hold_end_time_cache: Vec<Option<f32>>,
    pub hold_end_display_beat_cache: Vec<Option<f32>>,
    pub music_end_time: f32,
    pub music_rate: f32,
    pub global_offset_seconds: f32,
    pub initial_global_offset_seconds: f32,
    pub next_tap_miss_cursor: usize,
    pub next_mine_avoid_cursor: usize,
    pub row_entries: Vec<RowEntry>,
    
    // Optimization: Direct array lookup instead of HashMap
    pub row_map_cache: Vec<u32>, 

    pub decaying_hold_indices: Vec<usize>,
    pub hold_decay_active: Vec<bool>,
    pub life_history: Vec<(f32, f32)>, // (time, life_value)

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
    pub field_zoom: f32,
    pub scroll_pixels_per_second: f32,
    pub scroll_travel_time: f32,
    pub draw_distance_before_targets: f32,
    pub draw_distance_after_targets: f32,
    pub reverse_scroll: bool,
    pub column_scroll_dirs: [f32; 4],
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

    pub sync_overlay_message: Option<String>,

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

pub fn queue_input_edge(state: &mut State, source: InputSource, lane: Lane, pressed: bool, timestamp: Instant) {
    state.pending_edges.push_back(InputEdge { lane, pressed, source, timestamp });
}

fn get_reference_bpm_from_display_tag(display_bpm_str: &str) -> Option<f32> {
    let s = display_bpm_str.trim();
    if s.is_empty() || s == "*" { return None; }
    if let Some((_, max_str)) = s.split_once(':') {
        return max_str.trim().parse::<f32>().ok();
    }
    s.parse::<f32>().ok()
}

pub fn init(song: Arc<SongData>, chart: Arc<ChartData>, active_color_index: i32, music_rate: f32) -> State {
    info!("Initializing Gameplay Screen...");
    let rate = if music_rate.is_finite() && music_rate > 0.0 { music_rate } else { 1.0 };

    let style = Style { num_cols: 4, num_players: 1 };
    let profile = profile::get();
    let noteskin_path = match profile.noteskin {
        crate::game::profile::NoteSkin::Cel => "assets/noteskins/cel/dance-single.txt",
        crate::game::profile::NoteSkin::Metal => "assets/noteskins/metal/dance-single.txt",
        crate::game::profile::NoteSkin::EnchantmentV2 => "assets/noteskins/enchantment-v2/dance-single.txt",
        crate::game::profile::NoteSkin::DevCel2024V3 => "assets/noteskins/devcel-2024-v3/dance-single.txt",
    };
    let noteskin = noteskin::load(Path::new(noteskin_path), &style)
        .ok()
        .or_else(|| noteskin::load(Path::new("assets/noteskins/cel/dance-single.txt"), &style).ok())
        .or_else(|| noteskin::load(Path::new("assets/noteskins/fallback.txt"), &style).ok());

    let mini_value = (profile.mini_percent as f32).clamp(-100.0, 150.0) / 100.0;
    let mut field_zoom = 1.0 - mini_value * 0.5;
    if field_zoom.abs() < 0.01 {
        field_zoom = 0.01;
    }

    let config = crate::config::get();
    let timing = Arc::new(TimingData::from_chart_data(
        -song.offset, config.global_offset_seconds,
        chart.chart_bpms.as_deref(), &song.normalized_bpms,
		chart.chart_stops.as_deref(), &song.normalized_stops,
		chart.chart_delays.as_deref(), &song.normalized_delays,
		chart.chart_warps.as_deref(), &song.normalized_warps,
		chart.chart_speeds.as_deref(), &song.normalized_speeds,
		chart.chart_scrolls.as_deref(), &song.normalized_scrolls,
        chart.chart_fakes.as_deref(), &song.normalized_fakes,
        &chart.notes,
    ));

    let parsed_notes = note_parser::parse_chart_notes(&chart.notes);
    let mut notes: Vec<Note> = Vec::with_capacity(parsed_notes.len());
    let mut holds_total: u32 = 0;
    let mut rolls_total: u32 = 0;
    let mut mines_total: u32 = 0;
    let mut max_row_index = 0;

    for parsed in parsed_notes {
        let row_index = parsed.row_index;
        if row_index > max_row_index { max_row_index = row_index; }
        
        let Some(beat) = timing.get_beat_for_row(row_index) else { continue; };
        let explicit_fake_tap = matches!(parsed.note_type, NoteType::Fake);
        let fake_by_segment = timing.is_fake_at_beat(beat);
        let is_fake = explicit_fake_tap || fake_by_segment;
        let note_type = if explicit_fake_tap { NoteType::Tap } else { parsed.note_type };

        // Pre-calculate judgability to avoid binary searches during gameplay
        let judgable_by_timing = timing.is_judgable_at_beat(beat);
        let can_be_judged = !is_fake && judgable_by_timing;

        if can_be_judged {
            match note_type {
                NoteType::Hold => { holds_total = holds_total.saturating_add(1); }
                NoteType::Roll => { rolls_total = rolls_total.saturating_add(1); }
                NoteType::Mine => { mines_total = mines_total.saturating_add(1); }
                NoteType::Tap => {}
                NoteType::Fake => {}
            }
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
            is_fake,
            can_be_judged,
        });
    }

    let num_tap_rows = {
        use std::collections::HashSet;
        let mut rows: HashSet<usize> = HashSet::new();
        for n in &notes {
            if !matches!(n.note_type, NoteType::Mine) && n.can_be_judged {
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

    let note_time_cache: Vec<f32> = notes.iter().map(|n| timing.get_time_for_beat(n.beat)).collect();
    let note_display_beat_cache: Vec<f32> = notes.iter().map(|n| timing.get_displayed_beat(n.beat)).collect();
    let hold_end_time_cache: Vec<Option<f32>> = notes.iter().map(|n| n.hold.as_ref().map(|h| timing.get_time_for_beat(h.end_beat))).collect();
    let hold_end_display_beat_cache: Vec<Option<f32>> = notes.iter().map(|n| n.hold.as_ref().map(|h| timing.get_displayed_beat(h.end_beat))).collect();

    let mut row_map: HashMap<usize, Vec<usize>> = HashMap::new();
    for (i, n) in notes.iter().enumerate() {
        if matches!(n.note_type, NoteType::Mine) { continue; }
        if !n.can_be_judged { continue; }
        row_map.entry(n.row_index).or_default().push(i);
    }
    let mut row_entries: Vec<RowEntry> = row_map
        .into_iter()
        .map(|(row_index, nonmine_note_indices)| RowEntry { row_index, nonmine_note_indices })
        .collect();
    row_entries.sort_by_key(|e| e.row_index);

    // Build optimized O(1) lookup table for row entries
    let mut row_map_cache = vec![u32::MAX; max_row_index + 1];
    for (pos, entry) in row_entries.iter().enumerate() {
        if entry.row_index < row_map_cache.len() {
            row_map_cache[entry.row_index] = pos as u32;
        }
    }

    let first_note_beat = notes.first().map_or(0.0, |n| n.beat);
    let first_second = timing.get_time_for_beat(first_note_beat);
    // ITGmania's ScreenGameplay::StartPlayingSong uses theme metrics
    // MinSecondsToStep / MinSecondsToMusic. Simply Love scales both by
    // MusicRate, so we apply the same here to keep real-world lead-in time
    // consistent across rates.
    let min_time_to_notes = MIN_SECONDS_TO_STEP * rate;
    let min_time_to_music = MIN_SECONDS_TO_MUSIC * rate;
    let mut start_delay = min_time_to_notes - first_second;
    if start_delay < min_time_to_music {
        start_delay = min_time_to_music;
    }
    if start_delay < 0.0 {
        start_delay = 0.0;
    }
    let song_start_instant = Instant::now() + Duration::from_secs_f32(start_delay);

    if let Some(music_path) = &song.music_path {
        info!("Starting music with a preroll delay of {:.2}s", start_delay);
        let cut = audio::Cut { start_sec: (-start_delay) as f64, length_sec: f64::INFINITY, ..Default::default() };
        audio::play_music(music_path.clone(), cut, false, rate.max(0.01));
    }

    let profile = profile::get();
    let scroll_speed = profile.scroll_speed;
    let initial_bpm = timing.get_bpm_for_beat(first_note_beat);

    let centered = profile.scroll_option.contains(profile::ScrollOption::Centered);

    let mut reference_bpm = get_reference_bpm_from_display_tag(&song.display_bpm).unwrap_or_else(|| {
        let mut actual_max = timing.get_capped_max_bpm(Some(M_MOD_HIGH_CAP));
        if !actual_max.is_finite() || actual_max <= 0.0 { actual_max = initial_bpm.max(120.0); }
        actual_max
    });
    if !reference_bpm.is_finite() || reference_bpm <= 0.0 { reference_bpm = initial_bpm.max(120.0); }

    let mut pixels_per_second = scroll_speed.pixels_per_second(initial_bpm, reference_bpm, rate);
    if !pixels_per_second.is_finite() || pixels_per_second <= 0.0 {
        pixels_per_second = ScrollSpeedSetting::default().pixels_per_second(initial_bpm, reference_bpm, rate);
    }
    let draw_distance_before_targets = screen_height() * DRAW_DISTANCE_BEFORE_TARGETS_MULTIPLIER;
    
    // If Centered, we need to draw arrows well past the center line.
    let draw_distance_after_targets = if centered {
        screen_height() * 0.6
    } else {
        DRAW_DISTANCE_AFTER_TARGETS
    };

    let mut travel_time = scroll_speed.travel_time_seconds(
        draw_distance_before_targets,
        initial_bpm,
        reference_bpm,
        rate,
    );
    if !travel_time.is_finite() || travel_time <= 0.0 { travel_time = draw_distance_before_targets / pixels_per_second; }

    let timing_profile = TimingProfile::default_itg_with_fa_plus();
    let music_end_time = compute_music_end_time(&notes, &note_time_cache, &hold_end_time_cache, rate);
    let notes_len = notes.len();
    let column_scroll_dirs = compute_column_scroll_dirs(profile.scroll_option);

    State {
        song,
        chart,
        background_texture_key: "__white".to_string(),
        timing,
        timing_profile,
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
        hold_end_display_beat_cache,
        music_end_time,
        music_rate: rate,
        global_offset_seconds: config.global_offset_seconds,
        initial_global_offset_seconds: config.global_offset_seconds,
        next_tap_miss_cursor: 0,
        next_mine_avoid_cursor: 0,
        row_entries,
        row_map_cache,
        decaying_hold_indices: Vec::new(),
        hold_decay_active: vec![false; notes_len],
        life_history: Vec::with_capacity(10000),
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
        field_zoom,
        scroll_pixels_per_second: pixels_per_second,
        scroll_travel_time: travel_time,
        draw_distance_before_targets,
        draw_distance_after_targets,
        reverse_scroll: profile.reverse_scroll,
        column_scroll_dirs,
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
        sync_overlay_message: None,
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
        &state.scoring_counts, state.holds_held_for_score, state.rolls_held_for_score, state.mines_hit_for_score,
    );
}

fn grade_to_window(grade: JudgeGrade) -> Option<&'static str> {
    match grade {
        JudgeGrade::Fantastic => Some("W1"), JudgeGrade::Excellent => Some("W2"),
        JudgeGrade::Great => Some("W3"), JudgeGrade::Decent => Some("W4"),
        JudgeGrade::WayOff => Some("W5"), JudgeGrade::Miss => None,
    }
}

fn trigger_tap_explosion(state: &mut State, column: usize, grade: JudgeGrade) {
    let Some(window_key) = grade_to_window(grade) else { return; };
    let spawn_window = state.noteskin.as_ref().and_then(|ns| if ns.tap_explosions.contains_key(window_key) { Some(window_key.to_string()) } else { None });
    if let Some(window) = spawn_window {
        state.tap_explosions[column] = Some(ActiveTapExplosion { window, elapsed: 0.0, start_beat: state.current_beat });
    }
}

fn trigger_mine_explosion(state: &mut State, column: usize) {
    state.mine_explosions[column] = Some(ActiveMineExplosion { elapsed: 0.0 });
}

fn trigger_combo_milestone(state: &mut State, kind: ComboMilestoneKind) {
    if let Some(index) = state.combo_milestones.iter().position(|milestone| milestone.kind == kind) {
        state.combo_milestones[index].elapsed = 0.0;
    } else {
        state.combo_milestones.push(ActiveComboMilestone { kind, elapsed: 0.0 });
    }
}

fn handle_mine_hit(state: &mut State, column: usize, arrow_list_index: usize, note_index: usize, time_error: f32) -> bool {
    let rate = if state.music_rate.is_finite() && state.music_rate > 0.0 { state.music_rate } else { 1.0 };
    let abs_time_error = (time_error / rate).abs();
    let mine_window = state.timing_profile.mine_window_s;
    if abs_time_error > mine_window { return false; }
    if state.notes[note_index].mine_result.is_some() || state.notes[note_index].is_fake { return false; }
    if !state.notes[note_index].can_be_judged { return false; }

    state.notes[note_index].mine_result = Some(MineResult::Hit);
    state.mines_hit = state.mines_hit.saturating_add(1);
    let mut updated_scoring = false;

    state.arrows[column].remove(arrow_list_index);
    apply_life_change(state, LifeChange::HIT_MINE);
    if !is_state_dead(state) {
        state.mines_hit_for_score = state.mines_hit_for_score.saturating_add(1);
        updated_scoring = true;
    }
    state.combo = 0;
    state.miss_combo = state.miss_combo.saturating_add(1);
    if state.full_combo_grade.is_some() { state.first_fc_attempt_broken = true; }
    state.full_combo_grade = None;
    state.receptor_glow_timers[column] = 0.0;
    trigger_mine_explosion(state, column);
    audio::play_sfx("assets/sounds/boom.ogg");
    debug!(
        "JUDGE MINE HIT: row={}, col={}, beat={:.3}, note_time={:.4}s, hit_time={:.4}s, offset_ms={:.2}, rate={:.3}",
        state.notes[note_index].row_index,
        column,
        state.notes[note_index].beat,
        state.note_time_cache[note_index],
        state.note_time_cache[note_index] + time_error,
        (time_error / rate) * 1000.0,
        rate
    );
    if updated_scoring { update_itg_grade_totals(state); }
    true
}

#[inline(always)]
fn try_hit_mine_while_held(state: &mut State, column: usize, current_time: f32) -> bool {
    let mine_window = state.timing_profile.mine_window_s;
    let rate = if state.music_rate.is_finite() && state.music_rate > 0.0 { state.music_rate } else { 1.0 };
    let search_radius = mine_window * rate;
    let start_t = current_time - search_radius;
    let end_t = current_time + search_radius;
    let times = &state.note_time_cache;
    let start_idx = times.partition_point(|&t| t < start_t);
    let end_idx = times.partition_point(|&t| t <= end_t);
    let mut best: Option<(usize, f32)> = None;
    for i in start_idx..end_idx {
        let note = &state.notes[i];
        if note.column != column { continue; }
        if !matches!(note.note_type, NoteType::Mine) { continue; }
        if !note.can_be_judged { continue; }
        if note.mine_result.is_some() { continue; }
        let note_time = times[i];
        let time_error = current_time - note_time;
        let abs_err = (time_error / rate).abs();
        if abs_err <= mine_window {
            match best {
                Some((_, best_err)) if abs_err >= best_err => {}
                _ => best = Some((i, time_error)),
            }
        }
    }
    let Some((note_index, time_error)) = best else { return false; };
    if let Some(arrow_idx) = state.arrows[column].iter().position(|a| a.note_index == note_index) {
        handle_mine_hit(state, column, arrow_idx, note_index, time_error)
    } else {
        hit_mine_timebased(state, column, note_index, time_error)
    }
}

#[inline(always)]
fn hit_mine_timebased(state: &mut State, column: usize, note_index: usize, time_error: f32) -> bool {
    let rate = if state.music_rate.is_finite() && state.music_rate > 0.0 { state.music_rate } else { 1.0 };
    let abs_time_error = (time_error / rate).abs();
    let mine_window = state.timing_profile.mine_window_s;
    if abs_time_error > mine_window { return false; }
    if state.notes[note_index].mine_result.is_some() || state.notes[note_index].is_fake { return false; }
    if !state.notes[note_index].can_be_judged { return false; }

    state.notes[note_index].mine_result = Some(MineResult::Hit);
    state.mines_hit = state.mines_hit.saturating_add(1);
    let mut updated_scoring = false;
    if let Some(pos) = state.arrows[column].iter().position(|a| a.note_index == note_index) {
        state.arrows[column].remove(pos);
    }
    apply_life_change(state, LifeChange::HIT_MINE);
    if !is_state_dead(state) {
        state.mines_hit_for_score = state.mines_hit_for_score.saturating_add(1);
        updated_scoring = true;
    }
    state.combo = 0;
    state.miss_combo = state.miss_combo.saturating_add(1);
    if state.full_combo_grade.is_some() { state.first_fc_attempt_broken = true; }
    state.full_combo_grade = None;
    state.receptor_glow_timers[column] = 0.0;
    trigger_mine_explosion(state, column);
    audio::play_sfx("assets/sounds/boom.ogg");
    debug!(
        "JUDGE MINE HIT (timebased): row={}, col={}, beat={:.3}, note_time={:.4}s, hit_time={:.4}s, offset_ms={:.2}, rate={:.3}",
        state.notes[note_index].row_index,
        column,
        state.notes[note_index].beat,
        state.note_time_cache[note_index],
        state.note_time_cache[note_index] + time_error,
        (time_error / rate) * 1000.0,
        rate
    );
    if updated_scoring { update_itg_grade_totals(state); }
    true
}

fn handle_hold_let_go(state: &mut State, column: usize, note_index: usize) {
    if let Some(hold) = state.notes[note_index].hold.as_mut() {
        if hold.result == Some(HoldResult::LetGo) { return; }
        hold.result = Some(HoldResult::LetGo);
        if hold.let_go_started_at.is_none() {
            hold.let_go_started_at = Some(state.current_music_time);
            hold.let_go_starting_life = hold.life.clamp(0.0, MAX_HOLD_LIFE);
            if note_index < state.hold_decay_active.len() && !state.hold_decay_active[note_index] {
                state.hold_decay_active[note_index] = true;
                state.decaying_hold_indices.push(note_index);
            }
        }
    }
    if state.hands_holding_count_for_stats > 0 { state.hands_holding_count_for_stats -= 1; }
    state.hold_judgments[column] = Some(HoldJudgmentRenderInfo { result: HoldResult::LetGo, triggered_at: Instant::now() });
    apply_life_change(state, LifeChange::LET_GO);
    if !is_state_dead(state) { update_itg_grade_totals(state); }
    state.combo = 0;
    state.miss_combo = state.miss_combo.saturating_add(1);
    if state.full_combo_grade.is_some() { state.first_fc_attempt_broken = true; }
    state.full_combo_grade = None;
    state.receptor_glow_timers[column] = 0.0;
}

fn handle_hold_success(state: &mut State, column: usize, note_index: usize) {
    if let Some(hold) = state.notes[note_index].hold.as_mut() {
        if hold.result == Some(HoldResult::Held) { return; }
        hold.result = Some(HoldResult::Held);
        hold.life = MAX_HOLD_LIFE;
        hold.let_go_started_at = None;
        hold.let_go_starting_life = 0.0;
        hold.last_held_row_index = hold.end_row_index;
        hold.last_held_beat = hold.end_beat;
    }
    if note_index < state.hold_decay_active.len() && state.hold_decay_active[note_index] {
        state.hold_decay_active[note_index] = false;
    }
    if state.hands_holding_count_for_stats > 0 { state.hands_holding_count_for_stats -= 1; }
    let mut updated_scoring = false;
    match state.notes[note_index].note_type {
        NoteType::Hold => {
            state.holds_held = state.holds_held.saturating_add(1);
            if !is_state_dead(state) { state.holds_held_for_score = state.holds_held_for_score.saturating_add(1); updated_scoring = true; }
        }
        NoteType::Roll => {
            state.rolls_held = state.rolls_held.saturating_add(1);
            if !is_state_dead(state) { state.rolls_held_for_score = state.rolls_held_for_score.saturating_add(1); updated_scoring = true; }
        }
        _ => {}
    }
    apply_life_change(state, LifeChange::HELD);
    if updated_scoring { update_itg_grade_totals(state); }
    state.miss_combo = 0;
    trigger_tap_explosion(state, column, JudgeGrade::Excellent);
    state.hold_judgments[column] = Some(HoldJudgmentRenderInfo { result: HoldResult::Held, triggered_at: Instant::now() });
}

fn refresh_roll_life_on_step(state: &mut State, column: usize) {
    let Some(active) = state.active_holds[column].as_mut() else { return; };
    if !matches!(active.note_type, NoteType::Roll) || active.let_go { return; }
    let Some(note) = state.notes.get_mut(active.note_index) else { return; };
    let Some(hold) = note.hold.as_mut() else { return; };
    if hold.result == Some(HoldResult::LetGo) { return; }
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
                let Some(hold) = state.notes[note_index].hold.as_mut() else { *active_opt = None; continue; };
                let pressed = inputs[column];
                active.is_pressed = pressed;

                if !active.let_go && active.life > 0.0 {
                    let prev_row = hold.last_held_row_index;
                    let prev_beat = hold.last_held_beat;
                    if pressed {
                        let mut current_row = state.timing.get_row_for_beat(state.current_beat).unwrap_or(note_start_row);
                        current_row = current_row.clamp(note_start_row, hold.end_row_index);
                        let final_row = prev_row.max(current_row);
                        if final_row != prev_row {
                            hold.last_held_row_index = final_row;
                            let mut new_beat = state.timing.get_beat_for_row(final_row).unwrap_or(state.current_beat);
                            new_beat = new_beat.clamp(note_start_beat, hold.end_beat);
                            if new_beat < prev_beat { new_beat = prev_beat; }
                            hold.last_held_beat = new_beat;
                        } else {
                            hold.last_held_beat = prev_beat.clamp(note_start_beat, hold.end_beat);
                        }
                    } else {
                        hold.last_held_beat = prev_beat.clamp(note_start_beat, hold.end_beat);
                    }
                }

                if !active.let_go {
                    let window = match active.note_type { NoteType::Hold => TIMING_WINDOW_SECONDS_HOLD, NoteType::Roll => TIMING_WINDOW_SECONDS_ROLL, _ => TIMING_WINDOW_SECONDS_HOLD };
                    match active.note_type {
                        NoteType::Hold => {
                            if pressed {
                                active.life = MAX_HOLD_LIFE;
                            } else if window > 0.0 {
                                active.life -= delta_time / window;
                            } else {
                                active.life = 0.0;
                            }
                        }
                        NoteType::Roll => {
                            if window > 0.0 {
                                active.life -= delta_time / window;
                            } else {
                                active.life = 0.0;
                            }
                        }
                        _ => {
                            if window > 0.0 {
                                active.life -= delta_time / window;
                            } else {
                                active.life = 0.0;
                            }
                        }
                    }
                    active.life = active.life.clamp(0.0, MAX_HOLD_LIFE);
                }
                hold.life = active.life;
                hold.let_go_started_at = None;
                hold.let_go_starting_life = 0.0;

                if !active.let_go && active.life <= 0.0 {
                    active.let_go = true;
                    handle_let_go = Some((column, note_index));
                }

                if current_time >= active.end_time {
                    if !active.let_go && active.life > 0.0 { handle_success = Some((column, note_index)); }
                    else if !active.let_go { active.let_go = true; handle_let_go = Some((column, note_index)); }
                    *active_opt = None;
                } else if active.let_go { *active_opt = None; }
            }
        }
        if let Some((column, note_index)) = handle_let_go { handle_hold_let_go(state, column, note_index); }
        if let Some((column, note_index)) = handle_success { handle_hold_success(state, column, note_index); }
    }
}

pub fn judge_a_tap(state: &mut State, column: usize, current_time: f32) -> bool {
    let windows = state.timing_profile.windows_s;
    let way_off_window = windows[4];
    let mine_window = state.timing_profile.mine_window_s;
    let rate = if state.music_rate.is_finite() && state.music_rate > 0.0 { state.music_rate } else { 1.0 };
    let mut best: Option<(usize, usize, f32)> = None;
    for (idx, arrow) in state.arrows[column].iter().enumerate().filter(|(_, a)| state.notes[a.note_index].result.is_none()) {
        let n = &state.notes[arrow.note_index];
        if !n.can_be_judged { continue; }
        if n.is_fake { continue; }
        let note_index = arrow.note_index;
        let note_time = state.note_time_cache[note_index];
        let abs_err = ((current_time - note_time) / rate).abs();
        let window = if matches!(n.note_type, NoteType::Mine) { mine_window } else { way_off_window };
        if abs_err <= window {
            match best {
                Some((_, _, best_err)) if abs_err >= best_err => {}
                _ => best = Some((idx, note_index, abs_err)),
            }
        }
    }

    if let Some((arrow_list_index, note_index, _)) = best {
        let note_row_index = state.notes[note_index].row_index;
        let note_type = state.notes[note_index].note_type;
        let note_time = state.note_time_cache[note_index];
        let time_error_music = current_time - note_time;
        let time_error_real = time_error_music / rate;
        let abs_time_error = time_error_real.abs();

        if matches!(note_type, NoteType::Mine) {
            if state.notes[note_index].is_fake { return false; }
            if handle_mine_hit(state, column, arrow_list_index, note_index, time_error_music) { return true; }
            return false;
        }
        let mine_hit_on_press = try_hit_mine_while_held(state, column, current_time);

        if abs_time_error <= way_off_window {
            let notes_on_row: Vec<usize> = if let Some(&pos) = state.row_map_cache.get(note_row_index).filter(|&&x| x != u32::MAX) {
                state.row_entries[pos as usize].nonmine_note_indices.iter().copied().filter(|&i| state.notes[i].result.is_none()).collect()
            } else {
                state.notes.iter().enumerate().filter(|(_, n)| n.row_index == note_row_index && !matches!(n.note_type, NoteType::Mine) && !n.is_fake).filter(|(_, n)| n.result.is_none()).map(|(i, _)| i).collect()
            };

            if notes_on_row.is_empty() { return false; }
            let all_pressed = notes_on_row.iter().all(|&i| { let col = state.notes[i].column; state.keyboard_lane_state[col] || state.gamepad_lane_state[col] });
            if !all_pressed { return false; }

            let (grade, window) = classify_offset_s(time_error_real, &state.timing_profile);

            for &idx in &notes_on_row {
                let note_col = state.notes[idx].column;
                let row_note_time = state.note_time_cache[idx];
                let te_music = current_time - row_note_time;
                let te_real = te_music / rate;
                state.notes[idx].result = Some(Judgment {
                    time_error_ms: te_real * 1000.0,
                    grade,
                    window: Some(window),
                });
                debug!(
                    "JUDGE TAP: grade={:?}, row={}, col={}, beat={:.3}, note_time={:.4}s, press_time={:.4}s, offset_ms={:.2}, rate={:.3}",
                    grade,
                    note_row_index,
                    note_col,
                    state.notes[idx].beat,
                    row_note_time,
                    current_time,
                    te_real * 1000.0,
                    rate
                );
                for col_arrows in &mut state.arrows {
                    if let Some(pos) = col_arrows.iter().position(|a| a.note_index == idx) {
                        col_arrows.remove(pos);
                        break;
                    }
                }
                state.receptor_glow_timers[note_col] = RECEPTOR_GLOW_DURATION;
                trigger_tap_explosion(state, note_col, grade);
                if let Some(end_time) = state.hold_end_time_cache[idx]
                    && matches!(state.notes[idx].note_type, NoteType::Hold | NoteType::Roll) {
                        if let Some(hold) = state.notes[idx].hold.as_mut() { hold.life = MAX_HOLD_LIFE; }
                        state.active_holds[note_col] = Some(ActiveHold {
                            note_index: idx, end_time, note_type: state.notes[idx].note_type,
                            let_go: false, is_pressed: true, life: MAX_HOLD_LIFE,
                        });
                    }
            }
            return true;
        }
        return mine_hit_on_press;
    }
    try_hit_mine_while_held(state, column, current_time)
}

pub fn handle_input(state: &mut State, ev: &InputEvent) -> ScreenAction {
    if let Some(lane) = lane_from_action(ev.action) {
        queue_input_edge(state, ev.source, lane, ev.pressed, ev.timestamp);
        return ScreenAction::None;
    }
    match ev.action {
        VirtualAction::p1_start => {
            if ev.pressed { state.hold_to_exit_key = Some(KeyCode::Enter); state.hold_to_exit_start = Some(ev.timestamp); }
            else if state.hold_to_exit_key == Some(KeyCode::Enter) { state.hold_to_exit_key = None; state.hold_to_exit_start = None; }
        }
        VirtualAction::p1_back => {
            if ev.pressed { state.hold_to_exit_key = Some(KeyCode::Escape); state.hold_to_exit_start = Some(ev.timestamp); }
            else if state.hold_to_exit_key == Some(KeyCode::Escape) { state.hold_to_exit_key = None; state.hold_to_exit_start = None; }
        }
        _ => {}
    }
    ScreenAction::None
}

pub fn handle_raw_key_event(state: &mut State, key: &KeyEvent, shift_held: bool) -> ScreenAction {
    use winit::event::ElementState;
    use winit::keyboard::PhysicalKey;

    if key.state != ElementState::Pressed {
        return ScreenAction::None;
    }
    if !shift_held {
        return ScreenAction::None;
    }

    let PhysicalKey::Code(code) = key.physical_key else {
        return ScreenAction::None;
    };

    let delta = match code {
        KeyCode::F11 => -0.001_f32,
        KeyCode::F12 => 0.001_f32,
        _ => return ScreenAction::None,
    };

    let old_offset = state.global_offset_seconds;
    let new_offset = old_offset + delta;
    if (new_offset - old_offset).abs() < 0.000_001_f32 {
        return ScreenAction::None;
    }

    if let Some(timing) = Arc::get_mut(&mut state.timing) {
        timing.set_global_offset_seconds(new_offset);
    }

    for (time, note) in state.note_time_cache.iter_mut().zip(&state.notes) {
        *time = state.timing.get_time_for_beat(note.beat);
    }
    for (time_opt, note) in state.hold_end_time_cache.iter_mut().zip(&state.notes) {
        *time_opt = note
            .hold
            .as_ref()
            .map(|h| state.timing.get_time_for_beat(h.end_beat));
    }

    state.music_end_time = compute_music_end_time(
        &state.notes,
        &state.note_time_cache,
        &state.hold_end_time_cache,
        state.music_rate,
    );

    state.global_offset_seconds = new_offset;

    if (new_offset - state.initial_global_offset_seconds).abs() < 0.000_001_f32 {
        state.sync_overlay_message = None;
        return ScreenAction::None;
    }

    let start_q = quantize_offset_seconds(state.initial_global_offset_seconds);
    let new_q = quantize_offset_seconds(new_offset);
    let delta_q = new_q - start_q;
    if delta_q.abs() < 0.000_1_f32 {
        state.sync_overlay_message = None;
        return ScreenAction::None;
    }

    let direction = if delta_q > 0.0 { "earlier" } else { "later" };
    let msg = format!(
        "Global Offset from {:+.3} to {:+.3} (notes {})",
        start_q, new_q, direction
    );
    state.sync_overlay_message = Some(msg);
    ScreenAction::None
}

fn finalize_row_judgment(state: &mut State, row_index: usize, judgments_in_row: Vec<Judgment>) {
    if judgments_in_row.is_empty() { return; }
    let row_has_miss = judgments_in_row.iter().any(|judgment| judgment.grade == JudgeGrade::Miss);
    let row_has_successful_hit = judgments_in_row.iter().any(|judgment| matches!(judgment.grade, JudgeGrade::Fantastic | JudgeGrade::Excellent | JudgeGrade::Great));
    let final_judgment = judgment::aggregate_row_final_judgment(judgments_in_row.iter()).cloned();
    let Some(final_judgment) = final_judgment else { return; };
    let final_grade = final_judgment.grade;
    *state.judgment_counts.entry(final_grade).or_insert(0) += 1;
    if !is_state_dead(state) {
        *state.scoring_counts.entry(final_grade).or_insert(0) += 1;
        update_itg_grade_totals(state);
    }
    let life_delta = match final_grade {
        JudgeGrade::Fantastic => LifeChange::FANTASTIC, JudgeGrade::Excellent => LifeChange::EXCELLENT,
        JudgeGrade::Great => LifeChange::GREAT, JudgeGrade::Decent => LifeChange::DECENT,
        JudgeGrade::WayOff => LifeChange::WAY_OFF, JudgeGrade::Miss => LifeChange::MISS,
    };
    apply_life_change(state, life_delta);
    state.last_judgment = Some(JudgmentRenderInfo { judgment: final_judgment, judged_at: Instant::now() });
    if row_has_successful_hit { state.miss_combo = 0; }
    if row_has_miss { state.miss_combo = state.miss_combo.saturating_add(1); }
    if row_has_miss || matches!(final_grade, JudgeGrade::Decent | JudgeGrade::WayOff) {
        state.combo = 0;
        if state.full_combo_grade.is_some() { state.first_fc_attempt_broken = true; }
        state.full_combo_grade = None;
    } else {
        let combo_increment: u32 = if let Some(&pos) = state.row_map_cache.get(row_index).filter(|&&x| x != u32::MAX) {
            state.row_entries[pos as usize].nonmine_note_indices.len() as u32
        } else {
            state.notes.iter().filter(|n| n.row_index == row_index && !matches!(n.note_type, NoteType::Mine)).count() as u32
        };
        state.combo = state.combo.saturating_add(combo_increment);
        let combo = state.combo;
        if combo > 0 && combo.is_multiple_of(1000) {
            trigger_combo_milestone(state, ComboMilestoneKind::Thousand);
            trigger_combo_milestone(state, ComboMilestoneKind::Hundred);
        } else if combo > 0 && combo.is_multiple_of(100) {
            trigger_combo_milestone(state, ComboMilestoneKind::Hundred);
        }
        if !state.first_fc_attempt_broken {
            let new_grade = if let Some(current_fc_grade) = &state.full_combo_grade { final_grade.max(*current_fc_grade) } else { final_grade };
            state.full_combo_grade = Some(new_grade);
        }
    }
    let row_has_wayoff = judgments_in_row.iter().any(|judgment| judgment.grade == JudgeGrade::WayOff);
    if !row_has_miss && !row_has_wayoff {
        let notes_on_row_count: usize = if let Some(&pos) = state.row_map_cache.get(row_index).filter(|&&x| x != u32::MAX) {
            state.row_entries[pos as usize].nonmine_note_indices.len()
        } else {
            state.notes.iter().filter(|n| n.row_index == row_index && !matches!(n.note_type, NoteType::Mine) && !n.is_fake).count()
        };
        let carried_holds_down: usize = state.active_holds.iter().filter_map(|a| a.as_ref()).filter(|a| active_hold_is_engaged(a)).filter(|a| {
            let note = &state.notes[a.note_index];
            if note.row_index >= row_index { return false; }
            if let Some(h) = note.hold.as_ref() { h.last_held_row_index >= row_index } else { false }
        }).count();
        if notes_on_row_count + carried_holds_down >= 3 {
            state.hands_achieved = state.hands_achieved.saturating_add(1);
        }
    }
}

fn update_judged_rows(state: &mut State) {
    loop {
        if state.judged_row_cursor >= state.row_entries.len() { break; }
        let row_entry = &state.row_entries[state.judged_row_cursor];
        if row_entry.nonmine_note_indices.is_empty() { state.judged_row_cursor += 1; continue; }
        let is_row_complete = row_entry.nonmine_note_indices.iter().all(|&i| state.notes[i].result.is_some());
        if is_row_complete {
            let mut judgments_on_row: Vec<Judgment> = Vec::with_capacity(row_entry.nonmine_note_indices.len());
            for &i in &row_entry.nonmine_note_indices {
                if let Some(j) = state.notes[i].result.clone() { judgments_on_row.push(j); }
            }
            finalize_row_judgment(state, row_entry.row_index, judgments_on_row);
            state.judged_row_cursor += 1;
        } else {
            break;
        }
    }
}

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
            let event_music_time = music_time_sec - elapsed * state.music_rate;
            let hit_note = judge_a_tap(state, lane_idx, event_music_time);
            refresh_roll_life_on_step(state, lane_idx);
            if !hit_note { state.receptor_bop_timers[lane_idx] = 0.11; }
        }
    }
}

#[inline(always)]
fn decay_let_go_hold_life(state: &mut State) {
    let mut i = 0;
    while i < state.decaying_hold_indices.len() {
        let note_index = state.decaying_hold_indices[i];
        let Some(note) = state.notes.get_mut(note_index) else { state.decaying_hold_indices.swap_remove(i); continue; };
        let Some(hold) = note.hold.as_mut() else { state.hold_decay_active[note_index] = false; state.decaying_hold_indices.swap_remove(i); continue; };
        if hold.result == Some(HoldResult::Held) || hold.let_go_started_at.is_none() {
            state.hold_decay_active[note_index] = false;
            state.decaying_hold_indices.swap_remove(i);
            continue;
        }
        let window = match note.note_type { NoteType::Roll => TIMING_WINDOW_SECONDS_ROLL, _ => TIMING_WINDOW_SECONDS_HOLD };
        if window <= 0.0 { hold.life = 0.0; i += 1; continue; }
        let start_time = hold.let_go_started_at.unwrap();
        let base_life = hold.let_go_starting_life.clamp(0.0, MAX_HOLD_LIFE);
        if base_life <= 0.0 { hold.life = 0.0; i += 1; continue; }
        let rate = if state.music_rate.is_finite() && state.music_rate > 0.0 { state.music_rate } else { 1.0 };
        let elapsed_music = (state.current_music_time - start_time).max(0.0);
        let elapsed_real = elapsed_music / rate;
        hold.life = (base_life - elapsed_real / window).max(0.0);
        i += 1;
    }
}

#[inline(always)]
fn tick_visual_effects(state: &mut State, delta_time: f32) {
    for timer in &mut state.receptor_glow_timers { *timer = (*timer - delta_time).max(0.0); }
    for timer in &mut state.receptor_bop_timers { *timer = (*timer - delta_time).max(0.0); }
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
            let lifetime = state.noteskin.as_ref().and_then(|ns| ns.tap_explosions.get(&active.window)).map(|explosion| explosion.animation.duration()).unwrap_or(0.0);
            if lifetime <= 0.0 || active.elapsed >= lifetime { *explosion = None; }
        }
    }
    for explosion in &mut state.mine_explosions {
        if let Some(active) = explosion {
            active.elapsed += delta_time;
            if active.elapsed >= MINE_EXPLOSION_DURATION { *explosion = None; }
        }
    }
    for slot in &mut state.hold_judgments {
        if let Some(render_info) = slot
            && render_info.triggered_at.elapsed().as_secs_f32() >= HOLD_JUDGMENT_TOTAL_DURATION { *slot = None; }
    }
}

#[inline(always)]
fn apply_time_based_mine_avoidance(state: &mut State, music_time_sec: f32) {
    let mine_window = state.timing_profile.mine_window_s;
    let rate = if state.music_rate.is_finite() && state.music_rate > 0.0 { state.music_rate } else { 1.0 };
    let cutoff_time = music_time_sec - mine_window * rate;
    let len = state.notes.len();
    while state.next_mine_avoid_cursor < len {
        let i = state.next_mine_avoid_cursor;
        let note_time = state.note_time_cache[i];
        if note_time > cutoff_time { break; }
        if let Some(note) = state.notes.get_mut(i)
            && matches!(note.note_type, NoteType::Mine) && note.can_be_judged && note.mine_result.is_none() {
                note.mine_result = Some(MineResult::Avoided);
                state.mines_avoided = state.mines_avoided.saturating_add(1);
                info!("MINE AVOIDED: Row {}, Col {}, Time: {:.2}s", note.row_index, note.column, music_time_sec);
            }
        state.next_mine_avoid_cursor += 1;
    }
}

#[inline(always)]
fn spawn_lookahead_arrows(state: &mut State, music_time_sec: f32) {
    match state.scroll_speed {
        ScrollSpeedSetting::CMod(_) => {
            let lookahead_time = music_time_sec + state.scroll_travel_time;
            let lookahead_beat = state.timing.get_beat_for_time(lookahead_time);
            while state.note_spawn_cursor < state.notes.len() && state.notes[state.note_spawn_cursor].beat < lookahead_beat {
                let note = &state.notes[state.note_spawn_cursor];
                state.arrows[note.column].push(Arrow { beat: note.beat, column: note.column, note_type: note.note_type, note_index: state.note_spawn_cursor });
                state.note_spawn_cursor += 1;
            }
        }
        ScrollSpeedSetting::XMod(_) | ScrollSpeedSetting::MMod(_) => {
            let speed_multiplier = state.timing.get_speed_multiplier(state.current_beat, state.current_music_time);
            let player_multiplier = state.scroll_speed.beat_multiplier(state.scroll_reference_bpm, state.music_rate);
            let final_multiplier = player_multiplier * speed_multiplier;
            if final_multiplier > 0.0 {
                let pixels_per_beat = ScrollSpeedSetting::ARROW_SPACING * final_multiplier * state.field_zoom;
                let lookahead_in_displayed_beats = state.draw_distance_before_targets / pixels_per_beat;
                let current_displayed_beat = state.timing.get_displayed_beat(state.current_beat);
                let target_displayed_beat = current_displayed_beat + lookahead_in_displayed_beats;
                while state.note_spawn_cursor < state.notes.len() {
                    let note_disp_beat = state.note_display_beat_cache[state.note_spawn_cursor];
                    if note_disp_beat < target_displayed_beat {
                        let note = &state.notes[state.note_spawn_cursor];
                        state.arrows[note.column].push(Arrow { beat: note.beat, column: note.column, note_type: note.note_type, note_index: state.note_spawn_cursor });
                        state.note_spawn_cursor += 1;
                    } else { break; }
                }
            }
        }
    }
}

#[inline(always)]
fn apply_passive_misses_and_mine_avoidance(state: &mut State, music_time_sec: f32) {
    let way_off_window = state.timing_profile.windows_s[4];
    let rate = if state.music_rate.is_finite() && state.music_rate > 0.0 { state.music_rate } else { 1.0 };
    for (col_idx, col_arrows) in state.arrows.iter_mut().enumerate() {
        let Some(next_arrow_index) = col_arrows.iter().position(|arrow| state.notes[arrow.note_index].result.is_none()) else { continue; };
        let note_index = col_arrows[next_arrow_index].note_index;
        let (note_row_index, note_type) = { let note = &state.notes[note_index]; (note.row_index, note.note_type) };
        let note_time = state.note_time_cache[note_index];

        if matches!(note_type, NoteType::Mine) {
            match state.notes[note_index].mine_result {
                Some(MineResult::Hit) => { col_arrows.remove(next_arrow_index); }
                Some(MineResult::Avoided) => {}
                None => {
                    let mine_window = state.timing_profile.mine_window_s;
                    let rate = if state.music_rate.is_finite() && state.music_rate > 0.0 { state.music_rate } else { 1.0 };
                    if music_time_sec - note_time > mine_window * rate
                        && state.notes[note_index].can_be_judged {
                            state.notes[note_index].mine_result = Some(MineResult::Avoided);
                            state.mines_avoided = state.mines_avoided.saturating_add(1);
                            info!("MINE AVOIDED: Row {}, Col {}, Time: {:.2}s", note_row_index, col_idx, music_time_sec);
                        }
                }
            }
            continue;
        }
        if state.notes[note_index].is_fake { continue; }
        if !state.notes[note_index].can_be_judged { continue; }
        if music_time_sec - note_time > way_off_window * rate {
            let time_err_music = music_time_sec - note_time;
            let time_err_real = time_err_music / rate;
            let judgment = Judgment {
                time_error_ms: time_err_real * 1000.0,
                grade: JudgeGrade::Miss,
                window: None,
            };
            if let Some(hold) = state.notes[note_index].hold.as_mut()
                && hold.result != Some(HoldResult::Held) {
                    hold.result = Some(HoldResult::LetGo);
                    if hold.let_go_started_at.is_none() {
                        hold.let_go_started_at = Some(music_time_sec);
                        hold.let_go_starting_life = hold.life.clamp(0.0, MAX_HOLD_LIFE);
                        if note_index < state.hold_decay_active.len() && !state.hold_decay_active[note_index] {
                            state.hold_decay_active[note_index] = true;
                            state.decaying_hold_indices.push(note_index);
                        }
                    }
                }
            state.notes[note_index].result = Some(judgment);
            info!("MISSED (pending): Row {}, Col {}", note_row_index, col_idx);
        }
    }
}

#[inline(always)]
fn apply_time_based_tap_misses(state: &mut State, music_time_sec: f32) {
    let way_off_window = state.timing_profile.windows_s[4];
    let rate = if state.music_rate.is_finite() && state.music_rate > 0.0 { state.music_rate } else { 1.0 };
    let cutoff_time = music_time_sec - way_off_window * rate;
    let len = state.notes.len();
    while state.next_tap_miss_cursor < len {
        let i = state.next_tap_miss_cursor;
        let note_time = state.note_time_cache[i];
        if note_time > cutoff_time { break; }
        if let Some(note) = state.notes.get_mut(i)
            && !matches!(note.note_type, NoteType::Mine) && note.can_be_judged && note.result.is_none() {
                let row = note.row_index;
                let time_err_music = music_time_sec - note_time;
                let time_err_real = time_err_music / rate;
                note.result = Some(Judgment {
                    time_error_ms: time_err_real * 1000.0,
                    grade: JudgeGrade::Miss,
                    window: None,
                });
                debug!(
                    "JUDGE TAP MISS (time-based): row={}, col={}, beat={:.3}, note_time={:.4}s, miss_time={:.4}s, offset_ms={:.2}, rate={:.3}",
                    row,
                    note.column,
                    note.beat,
                    note_time,
                    music_time_sec,
                    time_err_real * 1000.0,
                    rate
                );
                if let Some(hold) = note.hold.as_mut()
                    && hold.result != Some(HoldResult::Held) {
                        hold.result = Some(HoldResult::LetGo);
                        if hold.let_go_started_at.is_none() {
                            hold.let_go_started_at = Some(music_time_sec);
                            hold.let_go_starting_life = hold.life.clamp(0.0, MAX_HOLD_LIFE);
                            if i < state.hold_decay_active.len() && !state.hold_decay_active[i] {
                                state.hold_decay_active[i] = true;
                                state.decaying_hold_indices.push(i);
                            }
                        }
                    }
                info!("MISSED (time-based): Row {}", row);
            }
        state.next_tap_miss_cursor += 1;
    }
}

#[inline(always)]
fn cull_scrolled_out_arrows(state: &mut State, music_time_sec: f32) {
    let receptor_y_normal = screen_center_y() + RECEPTOR_Y_OFFSET_FROM_CENTER;
    let receptor_y_reverse = screen_center_y() + RECEPTOR_Y_OFFSET_FROM_CENTER_REVERSE;
    let (cmod_pps_opt, curr_disp_beat, beatmod_multiplier) = match state.scroll_speed {
        ScrollSpeedSetting::CMod(c_bpm) => {
            let pps = (c_bpm / 60.0) * ScrollSpeedSetting::ARROW_SPACING * state.field_zoom;
            (Some(pps), 0.0, 0.0)
        }
        ScrollSpeedSetting::XMod(_) | ScrollSpeedSetting::MMod(_) => {
            let curr_disp = state.timing.get_displayed_beat(state.current_beat);
            let speed_multiplier = state.timing.get_speed_multiplier(state.current_beat, state.current_music_time);
            let player_multiplier = state.scroll_speed.beat_multiplier(state.scroll_reference_bpm, state.music_rate);
            (None, curr_disp, player_multiplier * speed_multiplier)
        }
    };

    let profile = profile::get();
    let is_centered = profile.scroll_option.contains(profile::ScrollOption::Centered);

    for (col_idx, col_arrows) in state.arrows.iter_mut().enumerate() {
        let raw_dir = state.column_scroll_dirs.get(col_idx).copied().unwrap_or_else(|| if state.reverse_scroll { -1.0 } else { 1.0 });
        let dir = if raw_dir >= 0.0 { 1.0 } else { -1.0 };
        let receptor_y = if is_centered {
            // Centered receptors ignore Reverse for positioning (but not for direction)
            // We apply notefield offset here too for consistency
            screen_center_y() + profile.note_field_offset_y as f32
        } else if dir >= 0.0 { 
            receptor_y_normal 
        } else { 
            receptor_y_reverse 
        };

        let miss_cull_threshold = receptor_y - dir * state.draw_distance_after_targets;
        col_arrows.retain(|arrow| {
            let note = &state.notes[arrow.note_index];
            if matches!(note.note_type, NoteType::Mine) {
                if note.is_fake {
                    let y_pos = match state.scroll_speed {
                        ScrollSpeedSetting::CMod(c_bpm) => {
                            let pps_chart = (c_bpm / 60.0) * ScrollSpeedSetting::ARROW_SPACING;
                            let note_time_chart = state.note_time_cache[arrow.note_index];
                            let rate = if state.music_rate.is_finite() && state.music_rate > 0.0 { state.music_rate } else { 1.0 };
                            let time_diff_real = (note_time_chart - music_time_sec) / rate;
                            receptor_y + dir * time_diff_real * pps_chart
                        }
                        ScrollSpeedSetting::XMod(_) | ScrollSpeedSetting::MMod(_) => {
                            let note_disp_beat = state.note_display_beat_cache[arrow.note_index];
                            let beat_diff_disp = note_disp_beat - curr_disp_beat;
                            receptor_y + dir * beat_diff_disp * ScrollSpeedSetting::ARROW_SPACING * beatmod_multiplier * state.field_zoom
                        }
                    };
                    return if dir < 0.0 { y_pos <= miss_cull_threshold } else { y_pos >= miss_cull_threshold };
                }
                match note.mine_result { Some(MineResult::Avoided) => {}, Some(MineResult::Hit) => return false, None => return true, }
            } else if note.is_fake {
                let y_pos = match state.scroll_speed {
                    ScrollSpeedSetting::CMod(c_bpm) => {
                        let pps_chart = (c_bpm / 60.0) * ScrollSpeedSetting::ARROW_SPACING;
                        let note_time_chart = state.note_time_cache[arrow.note_index];
                        let rate = if state.music_rate.is_finite() && state.music_rate > 0.0 { state.music_rate } else { 1.0 };
                        let time_diff_real = (note_time_chart - music_time_sec) / rate;
                        receptor_y + dir * time_diff_real * pps_chart
                    }
                    ScrollSpeedSetting::XMod(_) | ScrollSpeedSetting::MMod(_) => {
                        let note_disp_beat = state.note_display_beat_cache[arrow.note_index];
                        let beat_diff_disp = note_disp_beat - curr_disp_beat;
                        receptor_y + dir * beat_diff_disp * ScrollSpeedSetting::ARROW_SPACING * beatmod_multiplier * state.field_zoom
                    }
                };
                return if dir < 0.0 { y_pos <= miss_cull_threshold } else { y_pos >= miss_cull_threshold };
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
                    receptor_y + dir * time_diff_real * pps_chart
                }
                ScrollSpeedSetting::XMod(_) | ScrollSpeedSetting::MMod(_) => {
                    let note_disp_beat = state.note_display_beat_cache[arrow.note_index];
                    let beat_diff_disp = note_disp_beat - curr_disp_beat;
                    receptor_y + dir * beat_diff_disp * ScrollSpeedSetting::ARROW_SPACING * beatmod_multiplier * state.field_zoom
                }
            };
            if dir < 0.0 { y_pos <= miss_cull_threshold } else { y_pos >= miss_cull_threshold }
        });
    }
}

pub fn update(state: &mut State, delta_time: f32) -> ScreenAction {
    if let (Some(key), Some(start_time)) = (state.hold_to_exit_key, state.hold_to_exit_start)
        && start_time.elapsed() >= std::time::Duration::from_secs(1) {
            state.hold_to_exit_key = None;
            state.hold_to_exit_start = None;
            return match key {
                winit::keyboard::KeyCode::Enter => ScreenAction::Navigate(Screen::Evaluation),
                winit::keyboard::KeyCode::Escape => ScreenAction::Navigate(Screen::SelectMusic),
                _ => ScreenAction::None,
            };
        }
    state.total_elapsed_in_screen += delta_time;
    let now = std::time::Instant::now();
    let rate = if state.music_rate.is_finite() && state.music_rate > 0.0 { state.music_rate } else { 1.0 };
    let anchor = -state.global_offset_seconds;
    let music_time_sec = if now < state.song_start_instant {
        // Before the song actually starts, simulate ITGmania's behavior of using
        // a negative song position that advances at MusicRate. This keeps XMod/MMod
        // scroll speeds consistent during the lead-in.
        let delta = state.song_start_instant.saturating_duration_since(now).as_secs_f32();
        anchor * (1.0 - rate) - delta * rate
    } else {
        let elapsed = now.saturating_duration_since(state.song_start_instant).as_secs_f32();
        elapsed * rate + anchor * (1.0 - rate)
    };
    state.current_music_time = music_time_sec;
    
    // Optimization: only record if time has advanced slightly to avoid duplicates
    if state.life_history.last().is_none_or(|(t, _)| *t < music_time_sec) {
        state.life_history.push((music_time_sec, state.life));
    }

	let beat_info = state.timing.get_beat_info_from_time(music_time_sec);
	state.current_beat = beat_info.beat;
	state.is_in_freeze = beat_info.is_in_freeze;
	state.is_in_delay = beat_info.is_in_delay;

    let current_bpm = state.timing.get_bpm_for_beat(state.current_beat);
    let mut dynamic_speed = state.scroll_speed.pixels_per_second(current_bpm, state.scroll_reference_bpm, state.music_rate);
    if !dynamic_speed.is_finite() || dynamic_speed <= 0.0 { dynamic_speed = ScrollSpeedSetting::default().pixels_per_second(current_bpm, state.scroll_reference_bpm, state.music_rate); }
    state.scroll_pixels_per_second = dynamic_speed;

    let draw_distance_before_targets = screen_height() * DRAW_DISTANCE_BEFORE_TARGETS_MULTIPLIER;
    state.draw_distance_before_targets = draw_distance_before_targets;

    
    // Dynamic update of draw distance logic based on profile
    let is_centered = profile::get().scroll_option.contains(profile::ScrollOption::Centered);
    state.draw_distance_after_targets = if is_centered { screen_height() * 0.6 } else { DRAW_DISTANCE_AFTER_TARGETS };


    let mut travel_time = state.scroll_speed.travel_time_seconds(draw_distance_before_targets, current_bpm, state.scroll_reference_bpm, state.music_rate);
    if !travel_time.is_finite() || travel_time <= 0.0 { travel_time = draw_distance_before_targets / dynamic_speed; }
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
        if now_down && was_down { let _ = try_hit_mine_while_held(state, col, music_time_sec); }
    }
    state.prev_inputs = current_inputs;

    update_active_holds(state, &current_inputs, music_time_sec, delta_time);
    decay_let_go_hold_life(state);
    tick_visual_effects(state, delta_time);
    spawn_lookahead_arrows(state, music_time_sec);
    apply_time_based_mine_avoidance(state, music_time_sec);
    apply_passive_misses_and_mine_avoidance(state, music_time_sec);
    apply_time_based_tap_misses(state, music_time_sec);
    cull_scrolled_out_arrows(state, music_time_sec);
    update_judged_rows(state);

    state.log_timer += delta_time;
    if state.log_timer >= 1.0 {
        let active_arrows: usize = state.arrows.iter().map(|v| v.len()).sum();
        log::info!("Beat: {:.2}, Time: {:.2}, Combo: {}, Misses: {}, Active Arrows: {}", state.current_beat, music_time_sec, state.combo, state.miss_combo, active_arrows);
        state.log_timer -= 1.0;
    }
    ScreenAction::None
}
