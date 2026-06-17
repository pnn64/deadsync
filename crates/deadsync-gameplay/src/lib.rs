use deadsync_chart::{ChartData, ChartDisplayBpm, SongData, SyncPref};
use deadsync_core::input::{InputSource, Lane, MAX_COLS, MAX_PLAYERS};
use deadsync_core::note::NoteType;
use deadsync_core::song_time::{
    SongTimeNs, clamp_song_time_ns, normalized_song_rate, scaled_song_time_ns,
    song_time_ns_add_seconds, song_time_ns_from_seconds, song_time_ns_invalid,
    song_time_ns_span_seconds, song_time_ns_to_seconds,
};
use deadsync_core::timing::{ROWS_PER_BEAT, beat_to_note_row};
use deadsync_rules::combo::{self, ComboState, ComboUpdate};
use deadsync_rules::judgment::{self, JudgeGrade, Judgment, TimingWindow};
use deadsync_rules::note::{
    HoldData, HoldResult, MAX_HOLD_LIFE, MineResult, Note, TIMING_WINDOW_SECONDS_HOLD,
    TIMING_WINDOW_SECONDS_ROLL, advance_hold_last_held, advance_hold_life_ns,
};
use deadsync_rules::scroll::ScrollSpeedSetting;
use deadsync_rules::stream::{
    StreamSegment, measure_densities, stream_sequences_threshold, zmod_stream_totals_full_measures,
};
use deadsync_rules::timing::{
    FA_PLUS_W0_MS, FA_PLUS_W010_MS, TimingData, TimingProfile, TimingProfileNs, WindowCounts,
    classify_offset_ns_with_disabled_windows, largest_enabled_tap_window_ns,
};
use std::collections::VecDeque;
use std::hash::Hasher;
use std::path::PathBuf;
use std::str::FromStr;
use std::time::{Duration, Instant};
use twox_hash::XxHash64;

// ITGmania ScreenGameplay MinSecondsToStep/MinSecondsToMusic defaults.
const MIN_SECONDS_TO_STEP: f32 = 6.0;
const MIN_SECONDS_TO_MUSIC: f32 = 2.0;
// Simply Love: ScreenGameplay GiveUpSeconds=0.33.
pub const GIVE_UP_HOLD_SECONDS: f32 = 0.33;
// Mirrors ScreenGameplay::AbortGiveUpText tween duration (1/2 second).
pub const GIVE_UP_ABORT_TEXT_SECONDS: f32 = 0.5;
pub const BACK_OUT_HOLD_SECONDS: f32 = 1.0;
pub const OFFSET_ADJUST_STEP_SECONDS: f32 = 0.001;
pub const OFFSET_ADJUST_REPEAT_DELAY: Duration = Duration::from_millis(300);
pub const OFFSET_ADJUST_REPEAT_INTERVAL: Duration = Duration::from_millis(50);
// Simply Love: ScreenGameplay out.lua (sleep 0.5, linear 1.0).
const GIVE_UP_OUT_FADE_DELAY_SECONDS: f32 = 0.5;
const GIVE_UP_OUT_FADE_SECONDS: f32 = 1.0;
// Simply Love: _fade out normal.lua (sleep 0.1, linear 0.4).
const BACK_OUT_FADE_DELAY_SECONDS: f32 = 0.1;
const BACK_OUT_FADE_SECONDS: f32 = 0.4;
pub const GAMEPLAY_INPUT_BACKLOG_WARN: usize = 128;
pub const MAX_ACTIVE_INPUT_SLOTS: usize = 128;
pub const AUTOSYNC_OFFSET_SAMPLE_COUNT: usize = 24;
pub const AUTOSYNC_STDDEV_MAX_SECONDS: f32 = 0.03;
const REPLAY_EDGE_FLOOR_PER_LANE: usize = 64;
pub const REPLAY_EDGE_RATE_PER_SEC: usize = 256;
pub const INITIAL_HOLD_LIFE: f32 = 1.0;
pub const TOGGLE_FLASH_DURATION: f32 = 1.5;
pub const TOGGLE_FLASH_FADE_START: f32 = 0.8;
pub const INSERT_MASK_BIT_WIDE: u8 = 1u8 << 0;
pub const INSERT_MASK_BIT_BIG: u8 = 1u8 << 1;
pub const INSERT_MASK_BIT_QUICK: u8 = 1u8 << 2;
pub const INSERT_MASK_BIT_BMRIZE: u8 = 1u8 << 3;
pub const INSERT_MASK_BIT_SKIPPY: u8 = 1u8 << 4;
pub const INSERT_MASK_BIT_ECHO: u8 = 1u8 << 5;
pub const INSERT_MASK_BIT_STOMP: u8 = 1u8 << 6;
pub const INSERT_MASK_BIT_MINES: u8 = 1u8 << 7;
pub const REMOVE_MASK_BIT_LITTLE: u8 = 1u8 << 0;
pub const REMOVE_MASK_BIT_NO_MINES: u8 = 1u8 << 1;
pub const REMOVE_MASK_BIT_NO_HOLDS: u8 = 1u8 << 2;
pub const REMOVE_MASK_BIT_NO_JUMPS: u8 = 1u8 << 3;
pub const REMOVE_MASK_BIT_NO_HANDS: u8 = 1u8 << 4;
pub const REMOVE_MASK_BIT_NO_QUADS: u8 = 1u8 << 5;
pub const REMOVE_MASK_BIT_NO_LIFTS: u8 = 1u8 << 6;
pub const REMOVE_MASK_BIT_NO_FAKES: u8 = 1u8 << 7;
pub const HOLDS_MASK_BIT_PLANTED: u8 = 1u8 << 0;
pub const HOLDS_MASK_BIT_FLOORED: u8 = 1u8 << 1;
pub const HOLDS_MASK_BIT_TWISTER: u8 = 1u8 << 2;
pub const HOLDS_MASK_BIT_NO_ROLLS: u8 = 1u8 << 3;
pub const HOLDS_MASK_BIT_HOLDS_TO_ROLLS: u8 = 1u8 << 4;
// ITG's MaxInputLatencySeconds preference defaults to 0.0.
const MAX_INPUT_LATENCY_SECONDS: f32 = 0.0;
// ITGmania Player::Step searches a wide row range first, then scores the
// selected note against the active timing window.
const STEP_SEARCH_DISTANCE_SECONDS: f32 = 1.0;
const COLUMN_CUE_MIN_SECONDS: f32 = 1.5;
const STEP_CAL_JUMP_WINDOW_S: f32 = 0.25;
pub const ASSIST_TICK_LOOKAHEAD_MARGIN_SECONDS: f32 = 0.050;
const QUANT_4TH: u8 = 0;
const QUANT_8TH: u8 = 1;
const QUANT_12TH: u8 = 2;
const QUANT_16TH: u8 = 3;
const QUANT_24TH: u8 = 4;
const QUANT_32ND: u8 = 5;
const QUANT_48TH: u8 = 6;
const QUANT_64TH: u8 = 7;
const QUANT_192ND: u8 = 8;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ActiveInputSlot {
    pub source: InputSource,
    pub input_slot: u32,
    pub lane_mask: u8,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LaneInputUpdate {
    pub was_down: bool,
    pub is_down: bool,
    pub slot_was_down: bool,
    pub slot_table_full: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum GameplayInputPlayStyle {
    #[default]
    Single,
    Versus,
    Double,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum GameplayInputPlayerSide {
    #[default]
    P1,
    P2,
}

pub const EMPTY_ACTIVE_INPUT_SLOT: ActiveInputSlot = ActiveInputSlot {
    source: InputSource::Keyboard,
    input_slot: 0,
    lane_mask: 0,
};

#[inline(always)]
pub const fn remap_live_input_lane(
    play_style: GameplayInputPlayStyle,
    player_side: GameplayInputPlayerSide,
    lane: Lane,
) -> Option<Lane> {
    match (play_style, player_side, lane) {
        // Single-player: reject the other side entirely so only one set of
        // bindings can play.
        (
            GameplayInputPlayStyle::Single,
            GameplayInputPlayerSide::P1,
            Lane::P2Left | Lane::P2Down | Lane::P2Up | Lane::P2Right,
        ) => None,
        (
            GameplayInputPlayStyle::Single,
            GameplayInputPlayerSide::P2,
            Lane::Left | Lane::Down | Lane::Up | Lane::Right,
        ) => None,
        // P2-only single: remap P2 lanes into the 4-col field.
        (GameplayInputPlayStyle::Single, GameplayInputPlayerSide::P2, Lane::P2Left) => {
            Some(Lane::Left)
        }
        (GameplayInputPlayStyle::Single, GameplayInputPlayerSide::P2, Lane::P2Down) => {
            Some(Lane::Down)
        }
        (GameplayInputPlayStyle::Single, GameplayInputPlayerSide::P2, Lane::P2Up) => Some(Lane::Up),
        (GameplayInputPlayStyle::Single, GameplayInputPlayerSide::P2, Lane::P2Right) => {
            Some(Lane::Right)
        }
        _ => Some(lane),
    }
}

#[inline(always)]
pub const fn input_lane_bit(lane_idx: usize) -> u8 {
    1u8 << lane_idx
}

#[inline(always)]
pub const fn normalized_input_slot(input_slot: u32, fallback_slot: u32, invalid_slot: u32) -> u32 {
    if input_slot == invalid_slot {
        fallback_slot
    } else {
        input_slot
    }
}

pub fn active_input_slot_lane_is_down(
    slots: &[ActiveInputSlot],
    slot_count: usize,
    lane_idx: usize,
    source: InputSource,
    input_slot: u32,
) -> bool {
    let bit = input_lane_bit(lane_idx);
    slots[..slot_count.min(slots.len())].iter().any(|slot| {
        slot.source == source && slot.input_slot == input_slot && slot.lane_mask & bit != 0
    })
}

#[inline(always)]
fn find_active_input_slot(
    slots: &[ActiveInputSlot],
    slot_count: usize,
    source: InputSource,
    input_slot: u32,
) -> Option<usize> {
    slots[..slot_count.min(slots.len())]
        .iter()
        .position(|slot| slot.source == source && slot.input_slot == input_slot)
}

#[inline(always)]
fn insert_active_input_slot(
    slots: &mut [ActiveInputSlot],
    slot_count: &mut usize,
    source: InputSource,
    input_slot: u32,
) -> Option<usize> {
    if let Some(idx) = find_active_input_slot(slots, *slot_count, source, input_slot) {
        return Some(idx);
    }
    if *slot_count >= slots.len() {
        return None;
    }
    let idx = *slot_count;
    slots[idx] = ActiveInputSlot {
        source,
        input_slot,
        lane_mask: 0,
    };
    *slot_count += 1;
    Some(idx)
}

#[inline(always)]
fn remove_active_input_slot_if_empty(
    slots: &mut [ActiveInputSlot],
    slot_count: &mut usize,
    idx: usize,
) {
    if idx >= *slot_count || slots[idx].lane_mask != 0 {
        return;
    }
    *slot_count = (*slot_count).saturating_sub(1);
    if idx < *slot_count {
        slots[idx] = slots[*slot_count];
    }
}

pub fn update_active_input_slot(
    slots: &mut [ActiveInputSlot],
    slot_count: &mut usize,
    lane_counts: &mut [u16],
    lane_idx: usize,
    source: InputSource,
    input_slot: u32,
    pressed: bool,
) -> LaneInputUpdate {
    if lane_idx >= lane_counts.len() || lane_idx >= MAX_COLS {
        return LaneInputUpdate::default();
    }
    *slot_count = (*slot_count).min(slots.len());
    let bit = input_lane_bit(lane_idx);
    let was_down = lane_counts[lane_idx] != 0;
    let mut slot_was_down = false;
    let mut slot_table_full = false;

    if pressed {
        if let Some(idx) = insert_active_input_slot(slots, slot_count, source, input_slot) {
            slot_was_down = slots[idx].lane_mask & bit != 0;
            if !slot_was_down {
                slots[idx].lane_mask |= bit;
                lane_counts[lane_idx] = lane_counts[lane_idx].saturating_add(1);
            }
        } else {
            slot_table_full = true;
        }
    } else if let Some(idx) = find_active_input_slot(slots, *slot_count, source, input_slot) {
        slot_was_down = slots[idx].lane_mask & bit != 0;
        if slot_was_down {
            slots[idx].lane_mask &= !bit;
            lane_counts[lane_idx] = lane_counts[lane_idx].saturating_sub(1);
            remove_active_input_slot_if_empty(slots, slot_count, idx);
        }
    }

    LaneInputUpdate {
        was_down,
        is_down: lane_counts[lane_idx] != 0,
        slot_was_down,
        slot_table_full,
    }
}

#[inline(always)]
pub fn autosync_mean_ns(samples: &[SongTimeNs; AUTOSYNC_OFFSET_SAMPLE_COUNT]) -> SongTimeNs {
    let mut sum = 0i128;
    for value in samples {
        sum += i128::from(*value);
    }
    let count = AUTOSYNC_OFFSET_SAMPLE_COUNT as i128;
    let rounded = if sum >= 0 {
        (sum + count / 2) / count
    } else {
        (sum - count / 2) / count
    };
    rounded.clamp(i64::MIN as i128, i64::MAX as i128) as SongTimeNs
}

#[inline(always)]
pub fn autosync_stddev_seconds(
    samples: &[SongTimeNs; AUTOSYNC_OFFSET_SAMPLE_COUNT],
    mean_ns: SongTimeNs,
) -> f32 {
    let mut dev = 0.0_f64;
    for value in samples {
        let d = (i128::from(*value) - i128::from(mean_ns)) as f64 / 1_000_000_000.0;
        dev += d * d;
    }
    (dev / AUTOSYNC_OFFSET_SAMPLE_COUNT as f64).sqrt() as f32
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum AutosyncOffsetCorrection {
    Song(f32),
    Machine(f32),
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct AutosyncSampleResult {
    pub standard_deviation: Option<f32>,
    pub correction: Option<AutosyncOffsetCorrection>,
}

pub fn apply_autosync_offset_sample(
    samples: &mut [SongTimeNs; AUTOSYNC_OFFSET_SAMPLE_COUNT],
    sample_count: &mut usize,
    mode: AutosyncMode,
    note_off_by_ns: SongTimeNs,
) -> AutosyncSampleResult {
    if song_time_ns_invalid(note_off_by_ns) || mode == AutosyncMode::Off {
        return AutosyncSampleResult::default();
    }

    let sample_ix = (*sample_count).min(AUTOSYNC_OFFSET_SAMPLE_COUNT.saturating_sub(1));
    samples[sample_ix] = note_off_by_ns;
    *sample_count = (*sample_count).saturating_add(1);
    if *sample_count < AUTOSYNC_OFFSET_SAMPLE_COUNT {
        return AutosyncSampleResult::default();
    }

    let mean_ns = autosync_mean_ns(samples);
    let stddev = autosync_stddev_seconds(samples, mean_ns);
    let correction = if stddev < AUTOSYNC_STDDEV_MAX_SECONDS {
        let mean = song_time_ns_to_seconds(mean_ns);
        match mode {
            AutosyncMode::Off => None,
            AutosyncMode::Song => Some(AutosyncOffsetCorrection::Song(mean)),
            AutosyncMode::Machine => Some(AutosyncOffsetCorrection::Machine(mean)),
        }
    } else {
        None
    };

    *sample_count = 0;
    AutosyncSampleResult {
        standard_deviation: Some(stddev),
        correction,
    }
}

const DISPLAY_CLOCK_CORRECTION_HALF_LIFE_S: f32 = 0.012;
const DISPLAY_CLOCK_MAX_LAG_S: f32 = 0.020;
const DISPLAY_CLOCK_MAX_LEAD_S: f32 = 0.006;
const DISPLAY_CLOCK_RESET_ERROR_S: f32 = 0.100;
const DISPLAY_CLOCK_MAX_STEP_S: f32 = 1.0 / 60.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DisplayClockDiagEventKind {
    ResetJump,
    TargetJump,
    ClampStep,
    ErrorThreshold,
    CatchUpStart,
}

impl std::fmt::Display for DisplayClockDiagEventKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let label = match self {
            Self::ResetJump => "reset_jump",
            Self::TargetJump => "target_jump",
            Self::ClampStep => "clamp_step",
            Self::ErrorThreshold => "error_threshold",
            Self::CatchUpStart => "catch_up_start",
        };
        f.write_str(label)
    }
}

#[derive(Clone, Copy, Debug)]
pub struct DisplayClockStepEvent {
    pub kind: DisplayClockDiagEventKind,
    pub target_time_sec: f32,
    pub previous_time_sec: f32,
    pub current_time_sec: f32,
    pub error_seconds: f32,
    pub step_seconds: f32,
    pub limit_seconds: f32,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct DisplayClockHealth {
    pub error_seconds: f32,
    pub catching_up: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct FrameStableDisplayClock {
    current_time_ns: SongTimeNs,
    target_time_ns: SongTimeNs,
    catching_up: bool,
    error_over_threshold: bool,
}

impl FrameStableDisplayClock {
    #[inline(always)]
    pub const fn new(time_ns: SongTimeNs) -> Self {
        Self {
            current_time_ns: time_ns,
            target_time_ns: time_ns,
            catching_up: false,
            error_over_threshold: false,
        }
    }

    #[inline(always)]
    pub fn reset(&mut self, time_ns: SongTimeNs) -> SongTimeNs {
        self.current_time_ns = time_ns;
        self.target_time_ns = time_ns;
        self.catching_up = false;
        self.error_over_threshold = false;
        time_ns
    }

    #[inline(always)]
    pub fn health(self) -> DisplayClockHealth {
        DisplayClockHealth {
            error_seconds: song_time_ns_span_seconds(
                i128::from(self.target_time_ns) - i128::from(self.current_time_ns),
            ),
            catching_up: self.catching_up,
        }
    }
}

#[inline(always)]
fn display_clock_step_event(
    kind: DisplayClockDiagEventKind,
    target_time_ns: SongTimeNs,
    previous_time_ns: SongTimeNs,
    current_time_ns: SongTimeNs,
    error_ns: i128,
    step_ns: i128,
    limit_ns: i128,
) -> DisplayClockStepEvent {
    DisplayClockStepEvent {
        kind,
        target_time_sec: song_time_ns_to_seconds(target_time_ns),
        previous_time_sec: song_time_ns_to_seconds(previous_time_ns),
        current_time_sec: song_time_ns_to_seconds(current_time_ns),
        error_seconds: song_time_ns_span_seconds(error_ns),
        step_seconds: song_time_ns_span_seconds(step_ns),
        limit_seconds: song_time_ns_span_seconds(limit_ns),
    }
}

pub fn frame_stable_display_clock_step(
    display_clock: &mut FrameStableDisplayClock,
    target_display_time_ns: SongTimeNs,
    delta_time: f32,
    seconds_per_second: f32,
    first_update: bool,
    mut note_event: impl FnMut(DisplayClockStepEvent),
) -> SongTimeNs {
    display_clock.target_time_ns = target_display_time_ns;
    if first_update
        || song_time_ns_invalid(display_clock.current_time_ns)
        || song_time_ns_invalid(target_display_time_ns)
        || !delta_time.is_finite()
        || delta_time <= 0.0
    {
        return display_clock.reset(target_display_time_ns);
    }

    let slope = normalized_song_rate(seconds_per_second);
    let previous_display_time_ns = display_clock.current_time_ns;
    let previous_catching_up = display_clock.catching_up;
    let previous_error_over_threshold = display_clock.error_over_threshold;
    let target_delta_ns = i128::from(target_display_time_ns) - i128::from(previous_display_time_ns);
    let max_error_ns = i128::from(scaled_song_time_ns(DISPLAY_CLOCK_RESET_ERROR_S, slope));
    if target_delta_ns.abs() > max_error_ns {
        note_event(display_clock_step_event(
            DisplayClockDiagEventKind::ResetJump,
            target_display_time_ns,
            previous_display_time_ns,
            target_display_time_ns,
            target_delta_ns,
            target_delta_ns,
            max_error_ns,
        ));
        return display_clock.reset(target_display_time_ns);
    }

    let advanced_ns =
        i128::from(previous_display_time_ns) + i128::from(scaled_song_time_ns(delta_time, slope));
    let correction_alpha = 1.0 - f32::exp2(-delta_time / DISPLAY_CLOCK_CORRECTION_HALF_LIFE_S);
    let mut corrected_ns = advanced_ns
        + ((i128::from(target_display_time_ns) - advanced_ns) as f64 * correction_alpha as f64)
            .round() as i128;
    let max_step_ns = i128::from(scaled_song_time_ns(DISPLAY_CLOCK_MAX_STEP_S, slope));
    if target_delta_ns.abs() > (max_step_ns as f64 * 2.0).round() as i128 {
        note_event(display_clock_step_event(
            DisplayClockDiagEventKind::TargetJump,
            target_display_time_ns,
            previous_display_time_ns,
            clamp_song_time_ns(corrected_ns),
            target_delta_ns,
            target_delta_ns,
            (max_step_ns as f64 * 2.0).round() as i128,
        ));
    }
    let step_ns = corrected_ns - i128::from(previous_display_time_ns);
    let mut clamped_step = false;
    if step_ns.abs() > (max_step_ns as f64 * 1.2).round() as i128 {
        corrected_ns = i128::from(previous_display_time_ns) + step_ns.signum() * max_step_ns;
        clamped_step = true;
    }
    let min_allowed_ns = i128::from(target_display_time_ns)
        - i128::from(scaled_song_time_ns(DISPLAY_CLOCK_MAX_LAG_S, slope));
    let max_allowed_ns = i128::from(target_display_time_ns)
        + i128::from(scaled_song_time_ns(DISPLAY_CLOCK_MAX_LEAD_S, slope));
    corrected_ns = corrected_ns
        .clamp(min_allowed_ns, max_allowed_ns)
        .max(i128::from(previous_display_time_ns));
    display_clock.current_time_ns = clamp_song_time_ns(corrected_ns);
    let error_ns = i128::from(target_display_time_ns) - corrected_ns;
    display_clock.catching_up = error_ns.abs() > (max_step_ns / 2);
    display_clock.error_over_threshold = error_ns.abs() > max_step_ns;
    if clamped_step {
        note_event(display_clock_step_event(
            DisplayClockDiagEventKind::ClampStep,
            target_display_time_ns,
            previous_display_time_ns,
            display_clock.current_time_ns,
            error_ns,
            corrected_ns - i128::from(previous_display_time_ns),
            max_step_ns,
        ));
    }
    if !previous_error_over_threshold && display_clock.error_over_threshold {
        note_event(display_clock_step_event(
            DisplayClockDiagEventKind::ErrorThreshold,
            target_display_time_ns,
            previous_display_time_ns,
            display_clock.current_time_ns,
            error_ns,
            corrected_ns - i128::from(previous_display_time_ns),
            max_step_ns,
        ));
    }
    if !previous_catching_up && display_clock.catching_up {
        note_event(display_clock_step_event(
            DisplayClockDiagEventKind::CatchUpStart,
            target_display_time_ns,
            previous_display_time_ns,
            display_clock.current_time_ns,
            error_ns,
            corrected_ns - i128::from(previous_display_time_ns),
            max_step_ns / 2,
        ));
    }
    display_clock.current_time_ns
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum GameplayTimingTickMode {
    #[default]
    Off,
    Assist,
    Hit,
}

#[inline(always)]
pub const fn next_timing_tick_mode(mode: GameplayTimingTickMode) -> GameplayTimingTickMode {
    match mode {
        GameplayTimingTickMode::Off => GameplayTimingTickMode::Assist,
        GameplayTimingTickMode::Assist => GameplayTimingTickMode::Hit,
        GameplayTimingTickMode::Hit => GameplayTimingTickMode::Off,
    }
}

#[inline(always)]
pub const fn timing_tick_mode_status_line(mode: GameplayTimingTickMode) -> Option<&'static str> {
    match mode {
        GameplayTimingTickMode::Off => None,
        GameplayTimingTickMode::Assist => Some("Assist Tick"),
        GameplayTimingTickMode::Hit => Some("Hit Tick"),
    }
}

#[inline(always)]
pub const fn timing_tick_mode_debug_label(mode: GameplayTimingTickMode) -> &'static str {
    match mode {
        GameplayTimingTickMode::Off => "off",
        GameplayTimingTickMode::Assist => "assist tick",
        GameplayTimingTickMode::Hit => "hit tick",
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GameplayOffsetAdjustKey {
    Decrease,
    Increase,
}

#[inline(always)]
pub const fn offset_adjust_slot_for_key(key: GameplayOffsetAdjustKey) -> usize {
    match key {
        GameplayOffsetAdjustKey::Decrease => 0,
        GameplayOffsetAdjustKey::Increase => 1,
    }
}

#[inline(always)]
pub const fn offset_adjust_delta_for_key(key: GameplayOffsetAdjustKey) -> f32 {
    match key {
        GameplayOffsetAdjustKey::Decrease => -OFFSET_ADJUST_STEP_SECONDS,
        GameplayOffsetAdjustKey::Increase => OFFSET_ADJUST_STEP_SECONDS,
    }
}

#[inline(always)]
pub fn offset_adjust_repeat_ready(held_elapsed: Duration, last_elapsed: Duration) -> bool {
    held_elapsed >= OFFSET_ADJUST_REPEAT_DELAY && last_elapsed >= OFFSET_ADJUST_REPEAT_INTERVAL
}

pub fn start_offset_adjust_hold_state(
    held_since: &mut [Option<Instant>; 2],
    last_at: &mut [Option<Instant>; 2],
    key: GameplayOffsetAdjustKey,
    at: Instant,
) -> f32 {
    let slot = offset_adjust_slot_for_key(key);
    held_since[slot] = Some(at);
    last_at[slot] = Some(at);
    offset_adjust_delta_for_key(key)
}

pub fn clear_offset_adjust_hold_state(
    held_since: &mut [Option<Instant>; 2],
    last_at: &mut [Option<Instant>; 2],
    key: GameplayOffsetAdjustKey,
) {
    let slot = offset_adjust_slot_for_key(key);
    held_since[slot] = None;
    last_at[slot] = None;
}

pub fn tick_offset_adjust_hold_state(
    held_since: &[Option<Instant>; 2],
    last_at: &mut [Option<Instant>; 2],
    key: GameplayOffsetAdjustKey,
    now: Instant,
) -> Option<f32> {
    let slot = offset_adjust_slot_for_key(key);
    let (Some(held_since), Some(previous_at)) = (held_since[slot], last_at[slot]) else {
        return None;
    };
    if !offset_adjust_repeat_ready(
        now.duration_since(held_since),
        now.duration_since(previous_at),
    ) {
        return None;
    }
    last_at[slot] = Some(now);
    Some(offset_adjust_delta_for_key(key))
}

#[inline(always)]
pub const fn player_life_is_dead(life: f32, is_failing: bool) -> bool {
    is_failing || life <= 0.0
}

#[inline(always)]
pub fn course_submit_life_eligible(life: Option<&deadsync_rules::life::LifeMeter>) -> bool {
    life.is_none_or(|life| !life.is_failing && life.fail_time.is_none() && life.life > 0.0)
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct GameplayLifeDeltaUpdate {
    pub failed_now: bool,
    pub was_dead: bool,
}

pub fn apply_gameplay_life_delta(
    meter: &mut deadsync_rules::life::LifeMeter,
    life_history: &mut Vec<(f32, f32)>,
    course_submit_life: Option<&mut deadsync_rules::life::LifeMeter>,
    current_music_time: f32,
    delta: f32,
) -> GameplayLifeDeltaUpdate {
    if player_life_is_dead(meter.life, meter.is_failing) {
        meter.life = 0.0;
        meter.is_failing = true;
        return GameplayLifeDeltaUpdate {
            failed_now: false,
            was_dead: true,
        };
    }

    let result = deadsync_rules::life::apply_life_delta(meter, current_music_time, delta);
    if (result.new_life - result.old_life).abs() > 0.000_001_f32 {
        deadsync_rules::life::record_life_history(
            life_history,
            current_music_time,
            result.old_life,
        );
        deadsync_rules::life::record_life_history(
            life_history,
            current_music_time,
            result.new_life,
        );
    }
    if let Some(meter) = course_submit_life {
        let _ = deadsync_rules::life::apply_life_delta(meter, current_music_time, delta);
    }

    GameplayLifeDeltaUpdate {
        failed_now: result.failed_now,
        was_dead: false,
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GameplayViewport {
    width: f32,
    height: f32,
}

impl GameplayViewport {
    pub const fn design() -> Self {
        Self {
            width: 854.0,
            height: 480.0,
        }
    }

    pub fn new(width: f32, height: f32) -> Self {
        Self {
            width: if width.is_finite() && width > 0.0 {
                width
            } else {
                Self::design().width
            },
            height: if height.is_finite() && height > 0.0 {
                height
            } else {
                Self::design().height
            },
        }
    }

    #[inline(always)]
    pub const fn width(self) -> f32 {
        self.width
    }

    #[inline(always)]
    pub const fn height(self) -> f32 {
        self.height
    }

    #[inline(always)]
    pub const fn center_x(self) -> f32 {
        self.width * 0.5
    }

    #[inline(always)]
    pub const fn center_y(self) -> f32 {
        self.height * 0.5
    }

    #[inline(always)]
    pub fn is_wide(self) -> bool {
        self.width / self.height >= 1.6
    }
}

impl Default for GameplayViewport {
    fn default() -> Self {
        Self::design()
    }
}

pub const RECEPTOR_Y_OFFSET_FROM_CENTER: f32 = -125.0;
pub const RECEPTOR_Y_OFFSET_FROM_CENTER_REVERSE: f32 = 145.0;
pub const DRAW_DISTANCE_BEFORE_TARGETS_MULTIPLIER: f32 = 1.5;
pub const DRAW_DISTANCE_AFTER_TARGETS: f32 = 130.0;

#[inline(always)]
pub fn scroll_receptor_y(
    reverse_percent: f32,
    centered_percent: f32,
    normal_y: f32,
    reverse_y: f32,
    centered_y: f32,
) -> f32 {
    let reverse_y = lerp(normal_y, reverse_y, reverse_percent.clamp(0.0, 1.0));
    (centered_y - reverse_y).mul_add(centered_percent, reverse_y)
}

#[inline(always)]
pub fn draw_distance_before_targets(viewport_height: f32, draw_scale: f32) -> f32 {
    viewport_height * DRAW_DISTANCE_BEFORE_TARGETS_MULTIPLIER * draw_scale
}

#[inline(always)]
pub fn draw_distance_after_targets(
    viewport_height: f32,
    draw_scale: f32,
    centered_percent: f32,
) -> f32 {
    lerp(
        DRAW_DISTANCE_AFTER_TARGETS * draw_scale,
        viewport_height * 0.6 * draw_scale,
        centered_percent.clamp(0.0, 1.0),
    )
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum StepStatsPlayStyle {
    #[default]
    Single,
    Double,
    Versus,
}

pub fn step_stats_notefield_width(cols_per_player: usize) -> Option<f32> {
    if cols_per_player == 0 {
        return None;
    }
    // Simply Love GetNotefieldWidth() parity: this is a style width, not the
    // rendered field width. Mini and Spacing must not move step statistics.
    Some(cols_per_player as f32 * 64.0)
}

pub fn step_stats_upper_density_graph_width(play_style: StepStatsPlayStyle) -> f32 {
    // zmod UpperNPSGraph parity:
    //   width = GetNotefieldWidth()
    //   if OnePlayerTwoSides then width = width / 2
    //   width = width - 30
    let mut width = match play_style {
        StepStatsPlayStyle::Double => 512.0_f32,
        StepStatsPlayStyle::Single | StepStatsPlayStyle::Versus => 256.0_f32,
    };
    if play_style == StepStatsPlayStyle::Double {
        width *= 0.5_f32;
    }
    (width - 30.0_f32).max(0.0_f32)
}

pub fn step_stats_density_graph_width(
    play_style: StepStatsPlayStyle,
    cols_per_player: usize,
    num_players: usize,
    screen_w: f32,
    screen_h: f32,
    wide: bool,
    center_1player_notefield: bool,
) -> f32 {
    let is_ultrawide = screen_w / screen_h.max(1.0_f32) > (21.0_f32 / 9.0_f32);
    let note_field_is_centered = match play_style {
        StepStatsPlayStyle::Double => true,
        StepStatsPlayStyle::Single => num_players == 1 && center_1player_notefield,
        StepStatsPlayStyle::Versus => false,
    };

    let mut sidepane_width = screen_w * 0.5_f32;
    if !is_ultrawide && note_field_is_centered && wide {
        let nf_width = step_stats_notefield_width(cols_per_player)
            .unwrap_or(256.0_f32)
            .max(1.0_f32);
        sidepane_width = ((screen_w - nf_width) * 0.5_f32).max(1.0_f32);
    }
    if is_ultrawide && num_players > 1 {
        sidepane_width = (screen_w * 0.2_f32).max(1.0_f32);
    }

    // Simply Love StepStatistics/DensityGraph.lua: double squeezes the graph
    // to 95% of the side pane and positions it in the right dark pane.
    if play_style == StepStatsPlayStyle::Double {
        return (sidepane_width * 0.95_f32).max(1.0_f32);
    }
    sidepane_width.round().max(1.0_f32)
}

#[inline(always)]
pub fn push_density_life_point(points: &mut Vec<[f32; 2]>, x: f32, y: f32) -> bool {
    const EPS: f32 = 0.000_1_f32;
    const ANGLE_SIN2_MAX: f32 = 0.032_f32; // sin(0.18rad)^2

    if let Some(last) = points.last_mut()
        && x <= last[0] + EPS
    {
        if (y - last[1]).abs() <= EPS {
            return false;
        }
        last[1] = y;
        return true;
    }

    if points.len() >= 2 {
        let a = points[points.len() - 2];
        let b = points[points.len() - 1];
        let abx = b[0] - a[0];
        let aby = b[1] - a[1];
        let bcx = x - b[0];
        let bcy = y - b[1];
        let ab_len_sq = abx.mul_add(abx, aby * aby);
        let bc_len_sq = bcx.mul_add(bcx, bcy * bcy);
        let dot = abx.mul_add(bcx, aby * bcy);
        if dot > 0.0_f32 && ab_len_sq > EPS && bc_len_sq > EPS {
            let cross = abx.mul_add(bcy, -(aby * bcx));
            let cross_sq = cross * cross;
            if cross_sq <= ANGLE_SIN2_MAX * ab_len_sq * bc_len_sq {
                let last_ix = points.len() - 1;
                points[last_ix] = [x, y];
                return true;
            }
        }
    }

    points.push([x, y]);
    true
}

pub fn reference_bpm_from_display_tag(
    chart_display_bpm: Option<&ChartDisplayBpm>,
    song_display_bpm: &str,
) -> Option<f32> {
    match chart_display_bpm {
        Some(ChartDisplayBpm::Specified { max, .. }) => {
            let value = *max as f32;
            if value.is_finite() && value > 0.0 {
                return Some(value);
            }
        }
        Some(ChartDisplayBpm::Random) => return None,
        None => {}
    }

    let tag = song_display_bpm.trim();
    if tag.is_empty() || tag == "*" {
        return None;
    }
    if let Some((_, max_tag)) = tag.split_once(':') {
        return max_tag.trim().parse::<f32>().ok();
    }
    tag.parse::<f32>().ok()
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SongLuaCompilePlayStyle {
    #[default]
    Single,
    Double,
    Versus,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SongLuaRuntimeTimeUnit {
    Beat,
    Second,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SongLuaRuntimeSpanMode {
    Len,
    End,
}

#[inline(always)]
pub fn song_lua_target_matches_player(target_player: Option<u8>, player: usize) -> bool {
    match target_player {
        Some(target) => usize::from(target) == player + 1,
        None => true,
    }
}

#[inline(always)]
pub fn song_lua_end_value(start: f32, limit: f32, span_mode: SongLuaRuntimeSpanMode) -> f32 {
    match span_mode {
        SongLuaRuntimeSpanMode::Len => start + limit.max(0.0),
        SongLuaRuntimeSpanMode::End => limit,
    }
}

#[inline(always)]
pub fn song_lua_time_to_second(
    unit: SongLuaRuntimeTimeUnit,
    value: f32,
    timing_player: &TimingData,
    global_offset_seconds: f32,
) -> f32 {
    match unit {
        SongLuaRuntimeTimeUnit::Beat => timing_player.get_time_for_beat(value),
        SongLuaRuntimeTimeUnit::Second => value - global_offset_seconds,
    }
}

#[inline(always)]
pub fn song_lua_message_second(
    beat: f32,
    timing_player: &TimingData,
    global_offset_seconds: f32,
) -> Option<f32> {
    let event_second = song_lua_time_to_second(
        SongLuaRuntimeTimeUnit::Beat,
        beat,
        timing_player,
        global_offset_seconds,
    );
    event_second.is_finite().then_some(event_second)
}

pub fn song_lua_window_seconds(
    unit: SongLuaRuntimeTimeUnit,
    start: f32,
    limit: f32,
    span_mode: SongLuaRuntimeSpanMode,
    timing_player: &TimingData,
    global_offset_seconds: f32,
) -> Option<(f32, f32)> {
    let end = song_lua_end_value(start, limit, span_mode);
    let start_second = song_lua_time_to_second(unit, start, timing_player, global_offset_seconds);
    let end_second = song_lua_time_to_second(unit, end, timing_player, global_offset_seconds);
    if !start_second.is_finite() || !end_second.is_finite() || end_second < start_second {
        return None;
    }
    Some((start_second, end_second))
}

pub fn song_lua_sustain_end_second(
    unit: SongLuaRuntimeTimeUnit,
    start: f32,
    limit: f32,
    span_mode: SongLuaRuntimeSpanMode,
    sustain: Option<f32>,
    timing_player: &TimingData,
    global_offset_seconds: f32,
    end_second: f32,
) -> f32 {
    let Some(sustain) = sustain else {
        return end_second;
    };
    let sustain_value = match span_mode {
        SongLuaRuntimeSpanMode::Len => song_lua_end_value(start, limit, span_mode) + sustain,
        SongLuaRuntimeSpanMode::End => sustain,
    };
    let sustain_end_second =
        song_lua_time_to_second(unit, sustain_value, timing_player, global_offset_seconds);
    if sustain_end_second.is_finite() && sustain_end_second > end_second {
        sustain_end_second
    } else {
        end_second
    }
}

pub fn song_lua_compile_player_screen_x(
    num_players: usize,
    player_index: usize,
    viewport: GameplayViewport,
    play_style: SongLuaCompilePlayStyle,
    single_player_uses_p2_side: bool,
    note_field_offset_x: f32,
    center_1player_notefield: bool,
) -> f32 {
    let clamped_width = viewport.width().clamp(640.0, 854.0);
    let centered_one_side = num_players == 1
        && play_style == SongLuaCompilePlayStyle::Single
        && center_1player_notefield;
    let centered_both_sides = num_players == 1 && play_style == SongLuaCompilePlayStyle::Double;
    let p2_side = if num_players == 1 {
        single_player_uses_p2_side
    } else {
        player_index == 1
    };
    let base_center_x = if num_players == 2 {
        if p2_side {
            viewport.center_x() + (clamped_width * 0.25)
        } else {
            viewport.center_x() - (clamped_width * 0.25)
        }
    } else if centered_both_sides || centered_one_side {
        viewport.center_x()
    } else if p2_side {
        viewport.center_x() + (clamped_width * 0.25)
    } else {
        viewport.center_x() - (clamped_width * 0.25)
    };
    if num_players == 1 && (centered_both_sides || centered_one_side) {
        viewport.center_x()
    } else {
        let offset_sign = if p2_side { 1.0 } else { -1.0 };
        base_center_x + offset_sign * note_field_offset_x.clamp(0.0, 50.0)
    }
}

pub const MINI_PERCENT_MIN: f32 = -100.0;
pub const MINI_PERCENT_MAX: f32 = 150.0;

#[inline(always)]
pub fn effective_mini_percent(
    active_mini_percent: Option<f32>,
    fallback_mini_percent: f32,
    base_cleared: bool,
) -> f32 {
    let mini = active_mini_percent
        .filter(|v| v.is_finite())
        .unwrap_or(if base_cleared {
            0.0
        } else {
            fallback_mini_percent
        });
    mini.clamp(MINI_PERCENT_MIN, MINI_PERCENT_MAX)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MiniAttackMode {
    Absolute,
    Delta,
}

#[inline(always)]
pub fn attack_mini_target_percent(value: f32, mode: MiniAttackMode, base: f32) -> f32 {
    match mode {
        MiniAttackMode::Absolute => value,
        MiniAttackMode::Delta => base + value,
    }
}

#[inline(always)]
pub fn approach_attack_value(
    current: &mut Option<f32>,
    target: Option<f32>,
    base: f32,
    speed: Option<f32>,
    delta_time: f32,
    unit_scale: f32,
) {
    let Some(target) = target.filter(|value| value.is_finite()) else {
        *current = None;
        return;
    };
    if delta_time <= f32::EPSILON {
        *current = Some(target);
        return;
    }
    let Some(speed) = speed.filter(|value| value.is_finite()) else {
        *current = Some(target);
        return;
    };
    let step = delta_time.max(0.0) * speed.max(0.0) * unit_scale;
    if step <= f32::EPSILON {
        return;
    }
    let mut value = current.filter(|value| value.is_finite()).unwrap_or(base);
    approach_f32(&mut value, target, step);
    *current = Some(value);
}

#[inline(always)]
pub fn approach_attack_mini_percent_to_target(
    current: &mut Option<f32>,
    target: Option<f32>,
    base: f32,
    speed: Option<f32>,
    delta_time: f32,
) {
    approach_attack_value(current, target, base, speed, delta_time, 100.0);
    if let Some(value) = current.as_mut() {
        *value = value.clamp(MINI_PERCENT_MIN, MINI_PERCENT_MAX);
    }
}

#[inline(always)]
pub fn mini_value_for_percent(
    mini_percent: f32,
    fallback_mini_percent: f32,
    big_active: bool,
) -> f32 {
    let mut mini = if mini_percent.is_finite() {
        mini_percent
    } else {
        fallback_mini_percent
    };
    if big_active {
        // ITG _fallback/ArrowCloud map Effect Big to mod,-100% mini.
        mini -= 100.0;
    }
    mini.clamp(MINI_PERCENT_MIN, MINI_PERCENT_MAX) / 100.0
}

#[inline(always)]
pub fn player_draw_scale_for_mini(tilt: f32, mini_value: f32) -> f32 {
    (1.0 + 0.5 * tilt.abs()) * (1.0 + mini_value.abs())
}

const ACCEL_MASK_BIT_BOOST: u8 = 1u8 << 0;
const ACCEL_MASK_BIT_BRAKE: u8 = 1u8 << 1;
const ACCEL_MASK_BIT_WAVE: u8 = 1u8 << 2;
const ACCEL_MASK_BIT_EXPAND: u8 = 1u8 << 3;
const ACCEL_MASK_BIT_BOOMERANG: u8 = 1u8 << 4;
const VISUAL_MASK_BIT_DRUNK: u16 = 1u16 << 0;
const VISUAL_MASK_BIT_DIZZY: u16 = 1u16 << 1;
const VISUAL_MASK_BIT_CONFUSION: u16 = 1u16 << 2;
pub const VISUAL_MASK_BIT_BIG: u16 = 1u16 << 3;
const VISUAL_MASK_BIT_FLIP: u16 = 1u16 << 4;
const VISUAL_MASK_BIT_INVERT: u16 = 1u16 << 5;
const VISUAL_MASK_BIT_TORNADO: u16 = 1u16 << 6;
const VISUAL_MASK_BIT_TIPSY: u16 = 1u16 << 7;
const VISUAL_MASK_BIT_BUMPY: u16 = 1u16 << 8;
const VISUAL_MASK_BIT_BEAT: u16 = 1u16 << 9;
const APPEARANCE_MASK_BIT_HIDDEN: u8 = 1u8 << 0;
const APPEARANCE_MASK_BIT_SUDDEN: u8 = 1u8 << 1;
const APPEARANCE_MASK_BIT_STEALTH: u8 = 1u8 << 2;
const APPEARANCE_MASK_BIT_BLINK: u8 = 1u8 << 3;
const APPEARANCE_MASK_BIT_RANDOM_VANISH: u8 = 1u8 << 4;

#[derive(Clone, Copy, Debug, Default)]
pub struct AccelOverrides {
    pub boost: Option<f32>,
    pub brake: Option<f32>,
    pub wave: Option<f32>,
    pub expand: Option<f32>,
    pub boomerang: Option<f32>,
}

impl AccelOverrides {
    #[inline(always)]
    pub fn any(self) -> bool {
        self.boost.is_some()
            || self.brake.is_some()
            || self.wave.is_some()
            || self.expand.is_some()
            || self.boomerang.is_some()
    }
}

#[derive(Clone, Copy, Debug)]
pub struct VisualOverrides {
    pub drunk: Option<f32>,
    pub dizzy: Option<f32>,
    pub confusion: Option<f32>,
    pub confusion_offset: Option<f32>,
    pub confusion_offset_cols: [Option<f32>; MAX_COLS],
    pub flip: Option<f32>,
    pub invert: Option<f32>,
    pub tornado: Option<f32>,
    pub tipsy: Option<f32>,
    pub tiny: Option<f32>,
    pub bumpy: Option<f32>,
    pub bumpy_offset: Option<f32>,
    pub bumpy_period: Option<f32>,
    pub bumpy_cols: [Option<f32>; MAX_COLS],
    pub tiny_cols: [Option<f32>; MAX_COLS],
    pub move_x_cols: [Option<f32>; MAX_COLS],
    pub move_y_cols: [Option<f32>; MAX_COLS],
    pub pulse_inner: Option<f32>,
    pub pulse_outer: Option<f32>,
    pub pulse_period: Option<f32>,
    pub pulse_offset: Option<f32>,
    pub beat: Option<f32>,
}

impl Default for VisualOverrides {
    fn default() -> Self {
        Self {
            drunk: None,
            dizzy: None,
            confusion: None,
            confusion_offset: None,
            confusion_offset_cols: [None; MAX_COLS],
            flip: None,
            invert: None,
            tornado: None,
            tipsy: None,
            tiny: None,
            bumpy: None,
            bumpy_offset: None,
            bumpy_period: None,
            bumpy_cols: [None; MAX_COLS],
            tiny_cols: [None; MAX_COLS],
            move_x_cols: [None; MAX_COLS],
            move_y_cols: [None; MAX_COLS],
            pulse_inner: None,
            pulse_outer: None,
            pulse_period: None,
            pulse_offset: None,
            beat: None,
        }
    }
}

impl VisualOverrides {
    #[inline(always)]
    pub fn any(self) -> bool {
        self.drunk.is_some()
            || self.dizzy.is_some()
            || self.confusion.is_some()
            || self.confusion_offset.is_some()
            || self.confusion_offset_cols.iter().any(Option::is_some)
            || self.flip.is_some()
            || self.invert.is_some()
            || self.tornado.is_some()
            || self.tipsy.is_some()
            || self.tiny.is_some()
            || self.bumpy.is_some()
            || self.bumpy_offset.is_some()
            || self.bumpy_period.is_some()
            || self.bumpy_cols.iter().any(Option::is_some)
            || self.tiny_cols.iter().any(Option::is_some)
            || self.move_x_cols.iter().any(Option::is_some)
            || self.move_y_cols.iter().any(Option::is_some)
            || self.pulse_inner.is_some()
            || self.pulse_outer.is_some()
            || self.pulse_period.is_some()
            || self.pulse_offset.is_some()
            || self.beat.is_some()
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct AppearanceOverrides {
    pub hidden: Option<f32>,
    pub hidden_offset: Option<f32>,
    pub sudden: Option<f32>,
    pub sudden_offset: Option<f32>,
    pub stealth: Option<f32>,
    pub blink: Option<f32>,
    pub random_vanish: Option<f32>,
}

impl AppearanceOverrides {
    #[inline(always)]
    pub fn any(self) -> bool {
        self.hidden.is_some()
            || self.hidden_offset.is_some()
            || self.sudden.is_some()
            || self.sudden_offset.is_some()
            || self.stealth.is_some()
            || self.blink.is_some()
            || self.random_vanish.is_some()
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct VisibilityOverrides {
    pub dark: Option<f32>,
    pub blind: Option<f32>,
    pub cover: Option<f32>,
}

impl VisibilityOverrides {
    #[inline(always)]
    pub fn any(self) -> bool {
        self.dark.is_some() || self.blind.is_some() || self.cover.is_some()
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct ScrollOverrides {
    pub reverse: Option<f32>,
    pub split: Option<f32>,
    pub alternate: Option<f32>,
    pub cross: Option<f32>,
    pub centered: Option<f32>,
}

impl ScrollOverrides {
    #[inline(always)]
    pub fn any(self) -> bool {
        self.reverse.is_some()
            || self.split.is_some()
            || self.alternate.is_some()
            || self.cross.is_some()
            || self.centered.is_some()
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct PerspectiveOverrides {
    pub tilt: Option<f32>,
    pub skew: Option<f32>,
}

impl PerspectiveOverrides {
    #[inline(always)]
    pub fn any(self) -> bool {
        self.tilt.is_some() || self.skew.is_some()
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct AccelEffects {
    pub boost: f32,
    pub brake: f32,
    pub wave: f32,
    pub expand: f32,
    pub boomerang: f32,
}

impl AccelEffects {
    #[inline(always)]
    pub fn from_mask_bits(mask: u8) -> Self {
        Self {
            boost: f32::from((mask & ACCEL_MASK_BIT_BOOST) != 0),
            brake: f32::from((mask & ACCEL_MASK_BIT_BRAKE) != 0),
            wave: f32::from((mask & ACCEL_MASK_BIT_WAVE) != 0),
            expand: f32::from((mask & ACCEL_MASK_BIT_EXPAND) != 0),
            boomerang: f32::from((mask & ACCEL_MASK_BIT_BOOMERANG) != 0),
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct VisualEffects {
    pub drunk: f32,
    pub dizzy: f32,
    pub confusion: f32,
    pub confusion_offset: f32,
    pub confusion_offset_cols: [f32; MAX_COLS],
    pub big: f32,
    pub flip: f32,
    pub invert: f32,
    pub tornado: f32,
    pub tipsy: f32,
    pub tiny: f32,
    pub bumpy: f32,
    pub bumpy_offset: f32,
    pub bumpy_period: f32,
    pub bumpy_cols: [f32; MAX_COLS],
    pub tiny_cols: [f32; MAX_COLS],
    pub move_x_cols: [f32; MAX_COLS],
    pub move_y_cols: [f32; MAX_COLS],
    pub pulse_inner: f32,
    pub pulse_outer: f32,
    pub pulse_period: f32,
    pub pulse_offset: f32,
    pub beat: f32,
}

impl VisualEffects {
    #[inline(always)]
    pub fn from_mask_bits(mask: u16) -> Self {
        Self {
            drunk: f32::from((mask & VISUAL_MASK_BIT_DRUNK) != 0),
            dizzy: f32::from((mask & VISUAL_MASK_BIT_DIZZY) != 0),
            confusion: f32::from((mask & VISUAL_MASK_BIT_CONFUSION) != 0),
            confusion_offset: 0.0,
            confusion_offset_cols: [0.0; MAX_COLS],
            big: f32::from((mask & VISUAL_MASK_BIT_BIG) != 0),
            flip: f32::from((mask & VISUAL_MASK_BIT_FLIP) != 0),
            invert: f32::from((mask & VISUAL_MASK_BIT_INVERT) != 0),
            tornado: f32::from((mask & VISUAL_MASK_BIT_TORNADO) != 0),
            tipsy: f32::from((mask & VISUAL_MASK_BIT_TIPSY) != 0),
            tiny: 0.0,
            bumpy: f32::from((mask & VISUAL_MASK_BIT_BUMPY) != 0),
            bumpy_offset: 0.0,
            bumpy_period: 0.0,
            bumpy_cols: [0.0; MAX_COLS],
            tiny_cols: [0.0; MAX_COLS],
            move_x_cols: [0.0; MAX_COLS],
            move_y_cols: [0.0; MAX_COLS],
            pulse_inner: 0.0,
            pulse_outer: 0.0,
            pulse_period: 0.0,
            pulse_offset: 0.0,
            beat: f32::from((mask & VISUAL_MASK_BIT_BEAT) != 0),
        }
    }

    #[inline(always)]
    pub fn to_mask_bits(self) -> u16 {
        let mut mask = 0;
        if self.drunk > f32::EPSILON {
            mask |= VISUAL_MASK_BIT_DRUNK;
        }
        if self.dizzy > f32::EPSILON {
            mask |= VISUAL_MASK_BIT_DIZZY;
        }
        if self.confusion > f32::EPSILON {
            mask |= VISUAL_MASK_BIT_CONFUSION;
        }
        if self.big > f32::EPSILON {
            mask |= VISUAL_MASK_BIT_BIG;
        }
        if self.flip > f32::EPSILON {
            mask |= VISUAL_MASK_BIT_FLIP;
        }
        if self.invert > f32::EPSILON {
            mask |= VISUAL_MASK_BIT_INVERT;
        }
        if self.tornado > f32::EPSILON {
            mask |= VISUAL_MASK_BIT_TORNADO;
        }
        if self.tipsy > f32::EPSILON {
            mask |= VISUAL_MASK_BIT_TIPSY;
        }
        if self.bumpy > f32::EPSILON || self.bumpy_cols.iter().any(|v| *v > f32::EPSILON) {
            mask |= VISUAL_MASK_BIT_BUMPY;
        }
        if self.beat > f32::EPSILON {
            mask |= VISUAL_MASK_BIT_BEAT;
        }
        mask
    }
}

const OUTRO_ATTACK_CLEAR_RATE: f32 = 1.0;
const OUTRO_ATTACK_CLEAR_EPSILON: f32 = 0.0001;

#[inline(always)]
fn approach_optional_visual(value: &mut Option<f32>, target: f32, step: f32) {
    let Some(current) = value.as_mut() else {
        return;
    };
    approach_f32(current, target, step);
    if (*current - target).abs() <= OUTRO_ATTACK_CLEAR_EPSILON {
        *value = None;
    }
}

#[inline(always)]
fn approach_optional_visual_cols(
    values: &mut [Option<f32>; MAX_COLS],
    targets: [f32; MAX_COLS],
    step: f32,
) {
    for (value, target) in values.iter_mut().zip(targets) {
        approach_optional_visual(value, target, step);
    }
}

pub fn approach_visual_overrides_to_base(
    visual: &mut VisualOverrides,
    base: VisualEffects,
    delta_time: f32,
) {
    let step = delta_time * OUTRO_ATTACK_CLEAR_RATE;
    approach_optional_visual(&mut visual.drunk, base.drunk, step);
    approach_optional_visual(&mut visual.dizzy, base.dizzy, step);
    approach_optional_visual(&mut visual.confusion, base.confusion, step);
    approach_optional_visual(&mut visual.confusion_offset, base.confusion_offset, step);
    approach_optional_visual_cols(
        &mut visual.confusion_offset_cols,
        base.confusion_offset_cols,
        step,
    );
    approach_optional_visual(&mut visual.flip, base.flip, step);
    approach_optional_visual(&mut visual.invert, base.invert, step);
    approach_optional_visual(&mut visual.tornado, base.tornado, step);
    approach_optional_visual(&mut visual.tipsy, base.tipsy, step);
    approach_optional_visual(&mut visual.tiny, base.tiny, step);
    approach_optional_visual(&mut visual.bumpy, base.bumpy, step);
    approach_optional_visual(&mut visual.bumpy_offset, base.bumpy_offset, step);
    approach_optional_visual(&mut visual.bumpy_period, base.bumpy_period, step);
    approach_optional_visual_cols(&mut visual.bumpy_cols, base.bumpy_cols, step);
    approach_optional_visual_cols(&mut visual.tiny_cols, base.tiny_cols, step);
    approach_optional_visual_cols(&mut visual.move_x_cols, base.move_x_cols, step);
    approach_optional_visual_cols(&mut visual.move_y_cols, base.move_y_cols, step);
    approach_optional_visual(&mut visual.pulse_inner, base.pulse_inner, step);
    approach_optional_visual(&mut visual.pulse_outer, base.pulse_outer, step);
    approach_optional_visual(&mut visual.pulse_period, base.pulse_period, step);
    approach_optional_visual(&mut visual.pulse_offset, base.pulse_offset, step);
    approach_optional_visual(&mut visual.beat, base.beat, step);
}

#[inline(always)]
fn approach_attack_cols(
    current: &mut [Option<f32>; MAX_COLS],
    target: [Option<f32>; MAX_COLS],
    base: [f32; MAX_COLS],
    speed: [Option<f32>; MAX_COLS],
    delta_time: f32,
) {
    for (((current, target), base), speed) in current.iter_mut().zip(target).zip(base).zip(speed) {
        approach_attack_value(current, target, base, speed, delta_time, 1.0);
    }
}

pub fn approach_visual_overrides_to_target(
    current: &mut VisualOverrides,
    target: VisualOverrides,
    speed: VisualOverrides,
    base: VisualEffects,
    delta_time: f32,
) {
    approach_attack_value(
        &mut current.drunk,
        target.drunk,
        base.drunk,
        speed.drunk,
        delta_time,
        1.0,
    );
    approach_attack_value(
        &mut current.dizzy,
        target.dizzy,
        base.dizzy,
        speed.dizzy,
        delta_time,
        1.0,
    );
    approach_attack_value(
        &mut current.confusion,
        target.confusion,
        base.confusion,
        speed.confusion,
        delta_time,
        1.0,
    );
    approach_attack_value(
        &mut current.confusion_offset,
        target.confusion_offset,
        base.confusion_offset,
        speed.confusion_offset,
        delta_time,
        1.0,
    );
    approach_attack_cols(
        &mut current.confusion_offset_cols,
        target.confusion_offset_cols,
        base.confusion_offset_cols,
        speed.confusion_offset_cols,
        delta_time,
    );
    approach_attack_value(
        &mut current.flip,
        target.flip,
        base.flip,
        speed.flip,
        delta_time,
        1.0,
    );
    approach_attack_value(
        &mut current.invert,
        target.invert,
        base.invert,
        speed.invert,
        delta_time,
        1.0,
    );
    approach_attack_value(
        &mut current.tornado,
        target.tornado,
        base.tornado,
        speed.tornado,
        delta_time,
        1.0,
    );
    approach_attack_value(
        &mut current.tipsy,
        target.tipsy,
        base.tipsy,
        speed.tipsy,
        delta_time,
        1.0,
    );
    approach_attack_value(
        &mut current.tiny,
        target.tiny,
        base.tiny,
        speed.tiny,
        delta_time,
        1.0,
    );
    approach_attack_value(
        &mut current.bumpy,
        target.bumpy,
        base.bumpy,
        speed.bumpy,
        delta_time,
        1.0,
    );
    approach_attack_value(
        &mut current.bumpy_offset,
        target.bumpy_offset,
        base.bumpy_offset,
        speed.bumpy_offset,
        delta_time,
        1.0,
    );
    approach_attack_value(
        &mut current.bumpy_period,
        target.bumpy_period,
        base.bumpy_period,
        speed.bumpy_period,
        delta_time,
        1.0,
    );
    approach_attack_cols(
        &mut current.bumpy_cols,
        target.bumpy_cols,
        base.bumpy_cols,
        speed.bumpy_cols,
        delta_time,
    );
    approach_attack_cols(
        &mut current.tiny_cols,
        target.tiny_cols,
        base.tiny_cols,
        speed.tiny_cols,
        delta_time,
    );
    approach_attack_cols(
        &mut current.move_x_cols,
        target.move_x_cols,
        base.move_x_cols,
        speed.move_x_cols,
        delta_time,
    );
    approach_attack_cols(
        &mut current.move_y_cols,
        target.move_y_cols,
        base.move_y_cols,
        speed.move_y_cols,
        delta_time,
    );
    approach_attack_value(
        &mut current.pulse_inner,
        target.pulse_inner,
        base.pulse_inner,
        speed.pulse_inner,
        delta_time,
        1.0,
    );
    approach_attack_value(
        &mut current.pulse_outer,
        target.pulse_outer,
        base.pulse_outer,
        speed.pulse_outer,
        delta_time,
        1.0,
    );
    approach_attack_value(
        &mut current.pulse_period,
        target.pulse_period,
        base.pulse_period,
        speed.pulse_period,
        delta_time,
        1.0,
    );
    approach_attack_value(
        &mut current.pulse_offset,
        target.pulse_offset,
        base.pulse_offset,
        speed.pulse_offset,
        delta_time,
        1.0,
    );
    approach_attack_value(
        &mut current.beat,
        target.beat,
        base.beat,
        speed.beat,
        delta_time,
        1.0,
    );
}

#[derive(Clone, Copy, Debug, Default)]
pub struct AppearanceEffects {
    pub hidden: f32,
    pub hidden_offset: f32,
    pub sudden: f32,
    pub sudden_offset: f32,
    pub stealth: f32,
    pub blink: f32,
    pub random_vanish: f32,
}

impl AppearanceEffects {
    #[inline(always)]
    pub fn from_mask_bits(mask: u8) -> Self {
        Self {
            hidden: f32::from((mask & APPEARANCE_MASK_BIT_HIDDEN) != 0),
            hidden_offset: 0.0,
            sudden: f32::from((mask & APPEARANCE_MASK_BIT_SUDDEN) != 0),
            sudden_offset: 0.0,
            stealth: f32::from((mask & APPEARANCE_MASK_BIT_STEALTH) != 0),
            blink: f32::from((mask & APPEARANCE_MASK_BIT_BLINK) != 0),
            random_vanish: f32::from((mask & APPEARANCE_MASK_BIT_RANDOM_VANISH) != 0),
        }
    }

    #[inline(always)]
    pub fn approach_speeds() -> Self {
        Self {
            hidden: 1.0,
            hidden_offset: 1.0,
            sudden: 1.0,
            sudden_offset: 1.0,
            stealth: 1.0,
            blink: 1.0,
            random_vanish: 1.0,
        }
    }
}

#[inline(always)]
pub fn apply_appearance_target(
    target: &mut AppearanceEffects,
    speed: &mut AppearanceEffects,
    overrides: AppearanceOverrides,
    override_speeds: AppearanceOverrides,
) {
    if let Some(value) = overrides.hidden {
        target.hidden = value;
        speed.hidden = override_speeds.hidden.unwrap_or(1.0).max(0.0);
    }
    if let Some(value) = overrides.hidden_offset {
        target.hidden_offset = value;
        speed.hidden_offset = override_speeds.hidden_offset.unwrap_or(1.0).max(0.0);
    }
    if let Some(value) = overrides.sudden {
        target.sudden = value;
        speed.sudden = override_speeds.sudden.unwrap_or(1.0).max(0.0);
    }
    if let Some(value) = overrides.sudden_offset {
        target.sudden_offset = value;
        speed.sudden_offset = override_speeds.sudden_offset.unwrap_or(1.0).max(0.0);
    }
    if let Some(value) = overrides.stealth {
        target.stealth = value;
        speed.stealth = override_speeds.stealth.unwrap_or(1.0).max(0.0);
    }
    if let Some(value) = overrides.blink {
        target.blink = value;
        speed.blink = override_speeds.blink.unwrap_or(1.0).max(0.0);
    }
    if let Some(value) = overrides.random_vanish {
        target.random_vanish = value;
        speed.random_vanish = override_speeds.random_vanish.unwrap_or(1.0).max(0.0);
    }
}

#[inline(always)]
pub fn approach_appearance_effects(
    current: &mut AppearanceEffects,
    target: AppearanceEffects,
    speed: AppearanceEffects,
    delta_time: f32,
) {
    let delta_time = delta_time.max(0.0);
    approach_f32(
        &mut current.hidden,
        target.hidden,
        delta_time * speed.hidden,
    );
    approach_f32(
        &mut current.hidden_offset,
        target.hidden_offset,
        delta_time * speed.hidden_offset,
    );
    approach_f32(
        &mut current.sudden,
        target.sudden,
        delta_time * speed.sudden,
    );
    approach_f32(
        &mut current.sudden_offset,
        target.sudden_offset,
        delta_time * speed.sudden_offset,
    );
    approach_f32(
        &mut current.stealth,
        target.stealth,
        delta_time * speed.stealth,
    );
    approach_f32(&mut current.blink, target.blink, delta_time * speed.blink);
    approach_f32(
        &mut current.random_vanish,
        target.random_vanish,
        delta_time * speed.random_vanish,
    );
}

#[derive(Clone, Copy, Debug, Default)]
pub struct VisibilityEffects {
    pub dark: f32,
    pub blind: f32,
    pub cover: f32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ChartAttackEffects {
    pub insert_mask: u8,
    pub remove_mask: u8,
    pub holds_mask: u8,
    pub turn_bits: u16,
}

impl ChartAttackEffects {
    #[inline(always)]
    pub const fn has_note_masks(self) -> bool {
        self.insert_mask != 0 || self.remove_mask != 0 || self.holds_mask != 0
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScoreValidityOptions {
    pub chart_effects: ChartAttackEffects,
    pub attack_mode: GameplayAttackMode,
    pub music_rate: f32,
}

impl Default for ScoreValidityOptions {
    fn default() -> Self {
        Self {
            chart_effects: ChartAttackEffects::default(),
            attack_mode: GameplayAttackMode::default(),
            music_rate: 1.0,
        }
    }
}

pub fn score_invalid_reason_lines_for_options(
    chart: &ChartData,
    options: ScoreValidityOptions,
) -> Vec<&'static str> {
    let mut reasons = Vec::with_capacity(6);
    let rate = normalized_song_rate(options.music_rate);
    if rate < 1.0 {
        reasons.push("music rate is below 1.0x");
    }

    let remove_mask = options.chart_effects.remove_mask;
    if (remove_mask & REMOVE_MASK_BIT_NO_HOLDS) != 0 && chart.stats.holds > 0 {
        reasons.push("No Holds is enabled on a chart with holds");
    }
    if (remove_mask & REMOVE_MASK_BIT_NO_MINES) != 0 && chart.mines_nonfake > 0 {
        reasons.push("No Mines is enabled on a chart with mines");
    }
    if (remove_mask & REMOVE_MASK_BIT_NO_JUMPS) != 0 && chart.stats.jumps > 0 {
        reasons.push("No Jumps is enabled on a chart with jumps");
    }
    if (remove_mask & REMOVE_MASK_BIT_NO_HANDS) != 0 && chart.stats.hands > 0 {
        reasons.push("No Hands is enabled on a chart with hands");
    }
    if (remove_mask & REMOVE_MASK_BIT_NO_QUADS) != 0 && chart.stats.hands > 0 {
        reasons.push("No Quads is enabled on a chart with quads");
    }
    if (remove_mask & REMOVE_MASK_BIT_NO_LIFTS) != 0 && chart.stats.lifts > 0 {
        reasons.push("No Lifts is enabled on a chart with lifts");
    }
    if (remove_mask & REMOVE_MASK_BIT_NO_FAKES) != 0 && chart.stats.fakes > 0 {
        reasons.push("No Fakes is enabled on a chart with fakes");
    }

    let holds_mask = options.chart_effects.holds_mask;
    if (holds_mask & HOLDS_MASK_BIT_NO_ROLLS) != 0 && chart.stats.rolls > 0 {
        reasons.push("No Rolls is enabled on a chart with rolls");
    }
    if (remove_mask & REMOVE_MASK_BIT_LITTLE) != 0 {
        reasons.push("Little is enabled");
    }

    let insert_mask = options.chart_effects.insert_mask;
    if (insert_mask & INSERT_MASK_BIT_ECHO) != 0 {
        reasons.push("Echo is enabled");
    }
    if (holds_mask & HOLDS_MASK_BIT_PLANTED) != 0 {
        reasons.push("Planted is enabled");
    }
    if (holds_mask & HOLDS_MASK_BIT_FLOORED) != 0 {
        reasons.push("Floored is enabled");
    }
    if (holds_mask & HOLDS_MASK_BIT_TWISTER) != 0 {
        reasons.push("Twister is enabled");
    }

    match options.attack_mode {
        GameplayAttackMode::Off => {
            if chart.has_chart_attacks {
                reasons.push("AttackMode=Off is enabled on a chart with attacks");
            }
        }
        GameplayAttackMode::On => {}
        GameplayAttackMode::Random => reasons.push("AttackMode=Random is enabled"),
    }

    reasons
}

#[derive(Clone, Copy, Debug, Default)]
pub struct CourseDisplayTotals {
    pub possible_grade_points: i32,
    pub total_steps: u32,
    pub holds_total: u32,
    pub rolls_total: u32,
    pub mines_total: u32,
}

pub fn course_display_totals_for_chart(chart: &ChartData) -> CourseDisplayTotals {
    CourseDisplayTotals {
        possible_grade_points: chart.possible_grade_points,
        total_steps: chart.stats.total_steps,
        holds_total: chart.holds_total,
        rolls_total: chart.rolls_total,
        mines_total: chart.mines_total,
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct CourseDisplayCarry {
    // ITGmania keeps the same lifemeter alive between nonstop course songs.
    pub life: f32,
    pub judgment_counts: [u32; 6],
    pub scoring_counts: [u32; 6],
    pub full_combo_grade: Option<JudgeGrade>,
    pub current_combo_grade: Option<JudgeGrade>,
    pub current_combo_window_counts: WindowCounts,
    pub first_fc_attempt_broken: bool,
    // Canonical FA+ split (15ms) used for EX scoring/evaluation.
    pub window_counts: WindowCounts,
    // Canonical 10ms split used for H.EX scoring/evaluation.
    pub window_counts_10ms_blue: WindowCounts,
    // Display split used by gameplay counters (legacy 10ms or custom ms option).
    pub window_counts_display_blue: WindowCounts,
    pub holds_held: u32,
    pub rolls_held: u32,
    pub mines_avoided: u32,
    pub holds_held_for_score: u32,
    pub holds_let_go_for_score: u32,
    pub rolls_held_for_score: u32,
    pub rolls_let_go_for_score: u32,
    pub mines_hit_for_score: u32,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct CourseComboCarryState {
    pub combo: u32,
    pub full_combo_grade: Option<JudgeGrade>,
    pub current_combo_grade: Option<JudgeGrade>,
    pub current_combo_window_counts: WindowCounts,
    pub first_fc_attempt_broken: bool,
}

pub fn course_life_after_carry(current_life: f32, course_carry: Option<CourseDisplayCarry>) -> f32 {
    let Some(carry) = course_carry else {
        return current_life;
    };
    if carry.life.is_finite() {
        carry.life.clamp(0.0, 1.0)
    } else {
        current_life
    }
}

pub fn apply_course_combo_carry_state(
    state: &mut CourseComboCarryState,
    carry_combo_between_songs: bool,
    replay_mode: bool,
    combo_carry: u32,
    course_carry: Option<CourseDisplayCarry>,
) {
    if carry_combo_between_songs && !replay_mode {
        state.combo = combo_carry;
        if let Some(carry) = course_carry {
            if combo_carry > 0 {
                state.full_combo_grade = carry.full_combo_grade;
                state.current_combo_grade = carry.current_combo_grade;
                state.current_combo_window_counts = carry.current_combo_window_counts;
                state.first_fc_attempt_broken = carry.first_fc_attempt_broken;
            } else {
                state.first_fc_attempt_broken =
                    carry.first_fc_attempt_broken || carry.full_combo_grade.is_some();
            }
        }
    } else if course_carry.is_some() {
        state.first_fc_attempt_broken = true;
    }
}

#[derive(Clone, Copy, Debug)]
pub struct DisplayWindowCountsSources {
    pub canonical: WindowCounts,
    pub ten_ms_blue: WindowCounts,
    pub display_blue: WindowCounts,
}

impl Default for DisplayWindowCountsSources {
    fn default() -> Self {
        Self {
            canonical: WindowCounts::default(),
            ten_ms_blue: WindowCounts::default(),
            display_blue: WindowCounts::default(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum DisplayWindowCountsMode {
    Canonical,
    TenMsBlue,
    DisplayBlue,
    CustomBlue { split_ms: f32 },
}

impl Default for DisplayWindowCountsMode {
    fn default() -> Self {
        Self::Canonical
    }
}

#[inline(always)]
fn display_float_match(a: f32, b: f32) -> bool {
    (a - b).abs() <= 0.000_1
}

pub fn display_window_counts_mode(
    blue_window_ms: Option<f32>,
    display_blue_window_ms: f32,
) -> DisplayWindowCountsMode {
    let Some(ms) = blue_window_ms else {
        return DisplayWindowCountsMode::Canonical;
    };
    let split_ms = judgment::normalized_blue_window_ms(ms);
    let display_split_ms = judgment::normalized_blue_window_ms(display_blue_window_ms);
    if display_float_match(split_ms, FA_PLUS_W0_MS) {
        DisplayWindowCountsMode::Canonical
    } else if display_float_match(split_ms, FA_PLUS_W010_MS) {
        DisplayWindowCountsMode::TenMsBlue
    } else if display_float_match(split_ms, display_split_ms) {
        DisplayWindowCountsMode::DisplayBlue
    } else {
        DisplayWindowCountsMode::CustomBlue { split_ms }
    }
}

pub fn display_window_counts_current(
    sources: DisplayWindowCountsSources,
    mode: DisplayWindowCountsMode,
) -> Option<WindowCounts> {
    match mode {
        DisplayWindowCountsMode::Canonical => Some(sources.canonical),
        DisplayWindowCountsMode::TenMsBlue => Some(sources.ten_ms_blue),
        DisplayWindowCountsMode::DisplayBlue => Some(sources.display_blue),
        DisplayWindowCountsMode::CustomBlue { .. } => None,
    }
}

pub fn display_window_counts_with_carry(
    current: WindowCounts,
    carry: CourseDisplayCarry,
    mode: DisplayWindowCountsMode,
) -> WindowCounts {
    let carry_counts = match mode {
        DisplayWindowCountsMode::Canonical => carry.window_counts,
        DisplayWindowCountsMode::TenMsBlue => carry.window_counts_10ms_blue,
        DisplayWindowCountsMode::DisplayBlue | DisplayWindowCountsMode::CustomBlue { .. } => {
            carry.window_counts_display_blue
        }
    };
    judgment::add_window_counts(current, carry_counts)
}

pub fn record_display_window_counts_for_judgment(
    canonical: &mut WindowCounts,
    ten_ms_blue: &mut WindowCounts,
    display_blue: &mut WindowCounts,
    judgment: &Judgment,
    display_blue_window_ms: f32,
) {
    judgment::add_judgment_to_window_counts(canonical, judgment, FA_PLUS_W0_MS);
    judgment::add_judgment_to_window_counts(ten_ms_blue, judgment, FA_PLUS_W010_MS);
    judgment::add_judgment_to_window_counts(display_blue, judgment, display_blue_window_ms);
}

pub fn record_combo_window_count_for_judgment(counts: &mut WindowCounts, judgment: &Judgment) {
    judgment::add_judgment_to_window_counts(counts, judgment, FA_PLUS_W0_MS);
}

pub fn display_judgment_count_for_grade(
    stage_counts: judgment::JudgeCounts,
    carry: CourseDisplayCarry,
    grade: JudgeGrade,
) -> u32 {
    let ix = judgment::display_judge_ix(grade);
    stage_counts[ix].saturating_add(carry.judgment_counts[ix])
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum GameplayScoreDisplayMode {
    #[default]
    Normal,
    Predictive,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct CourseDisplayStage {
    pub life: f32,
    pub judgment_counts: judgment::JudgeCounts,
    pub scoring_counts: judgment::JudgeCounts,
    pub full_combo_grade: Option<JudgeGrade>,
    pub current_combo_grade: Option<JudgeGrade>,
    pub current_combo_window_counts: WindowCounts,
    pub combo: u32,
    pub first_fc_attempt_broken: bool,
    pub window_counts: WindowCounts,
    pub window_counts_10ms_blue: WindowCounts,
    pub window_counts_display_blue: WindowCounts,
    pub holds_held: u32,
    pub rolls_held: u32,
    pub mines_avoided: u32,
    pub holds_held_for_score: u32,
    pub holds_let_go_for_score: u32,
    pub rolls_held_for_score: u32,
    pub rolls_let_go_for_score: u32,
    pub mines_hit_for_score: u32,
}

pub fn course_display_carry_for_stage(
    previous: CourseDisplayCarry,
    stage: CourseDisplayStage,
) -> CourseDisplayCarry {
    let mut judgment_counts = [0u32; judgment::JUDGE_GRADE_COUNT];
    let mut scoring_counts = [0u32; judgment::JUDGE_GRADE_COUNT];
    let mut ix = 0usize;
    while ix < judgment::JUDGE_GRADE_COUNT {
        judgment_counts[ix] =
            previous.judgment_counts[ix].saturating_add(stage.judgment_counts[ix]);
        scoring_counts[ix] = previous.scoring_counts[ix].saturating_add(stage.scoring_counts[ix]);
        ix += 1;
    }

    let first_fc_attempt_broken = previous.first_fc_attempt_broken || stage.first_fc_attempt_broken;
    let full_combo_grade = if first_fc_attempt_broken {
        None
    } else {
        match (previous.full_combo_grade, stage.full_combo_grade) {
            (Some(prev), Some(current)) => Some(prev.max(current)),
            (Some(prev), None) => Some(prev),
            (None, current) => current,
        }
    };

    CourseDisplayCarry {
        life: stage.life.clamp(0.0, 1.0),
        judgment_counts,
        scoring_counts,
        full_combo_grade,
        current_combo_grade: stage.current_combo_grade,
        current_combo_window_counts: if stage.combo > 0 {
            stage.current_combo_window_counts
        } else {
            WindowCounts::default()
        },
        first_fc_attempt_broken,
        window_counts: judgment::add_window_counts(previous.window_counts, stage.window_counts),
        window_counts_10ms_blue: judgment::add_window_counts(
            previous.window_counts_10ms_blue,
            stage.window_counts_10ms_blue,
        ),
        window_counts_display_blue: judgment::add_window_counts(
            previous.window_counts_display_blue,
            stage.window_counts_display_blue,
        ),
        holds_held: previous.holds_held.saturating_add(stage.holds_held),
        rolls_held: previous.rolls_held.saturating_add(stage.rolls_held),
        mines_avoided: previous.mines_avoided.saturating_add(stage.mines_avoided),
        holds_held_for_score: previous
            .holds_held_for_score
            .saturating_add(stage.holds_held_for_score),
        holds_let_go_for_score: previous
            .holds_let_go_for_score
            .saturating_add(stage.holds_let_go_for_score),
        rolls_held_for_score: previous
            .rolls_held_for_score
            .saturating_add(stage.rolls_held_for_score),
        rolls_let_go_for_score: previous
            .rolls_let_go_for_score
            .saturating_add(stage.rolls_let_go_for_score),
        mines_hit_for_score: previous
            .mines_hit_for_score
            .saturating_add(stage.mines_hit_for_score),
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct CourseDisplayTiming {
    pub elapsed_seconds: f32,
    pub total_seconds: f32,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct ExScoreInputs {
    pub counts: WindowCounts,
    pub counts_10ms: WindowCounts,
    pub holds_held_for_score: u32,
    pub holds_let_go_for_score: u32,
    pub rolls_held_for_score: u32,
    pub rolls_let_go_for_score: u32,
    pub mines_hit_for_score: u32,
}

pub fn ex_score_data_from_display_inputs(
    inputs: ExScoreInputs,
    carry: CourseDisplayCarry,
    totals: CourseDisplayTotals,
) -> judgment::ExScoreData {
    let (holds_held, holds_resolved) = judgment::scored_hold_totals_with_carry(
        inputs.holds_held_for_score,
        inputs.holds_let_go_for_score,
        carry.holds_held_for_score,
        carry.holds_let_go_for_score,
    );
    let (rolls_held, rolls_resolved) = judgment::scored_hold_totals_with_carry(
        inputs.rolls_held_for_score,
        inputs.rolls_let_go_for_score,
        carry.rolls_held_for_score,
        carry.rolls_let_go_for_score,
    );
    judgment::ExScoreData {
        counts: inputs.counts,
        counts_10ms: inputs.counts_10ms,
        holds_held,
        holds_resolved,
        rolls_held,
        rolls_resolved,
        mines_hit: inputs
            .mines_hit_for_score
            .saturating_add(carry.mines_hit_for_score),
        total_steps: totals.total_steps,
        holds_total: totals.holds_total,
        rolls_total: totals.rolls_total,
        mines_total: totals.mines_total,
    }
}

#[inline(always)]
pub fn effective_ex_score_inputs(
    live: ExScoreInputs,
    failed_snapshot: Option<ExScoreInputs>,
) -> ExScoreInputs {
    failed_snapshot.unwrap_or(live)
}

#[inline(always)]
pub fn capture_failed_ex_score_inputs(
    failed_snapshot: &mut Option<ExScoreInputs>,
    fail_time: Option<f32>,
    live: ExScoreInputs,
) -> bool {
    if fail_time.is_none() || failed_snapshot.is_some() {
        return false;
    }
    *failed_snapshot = Some(live);
    true
}

#[derive(Clone, Copy, Debug, Default)]
pub struct ItgScoreStage {
    pub scoring_counts: judgment::JudgeCounts,
    pub holds_held_for_score: u32,
    pub holds_let_go_for_score: u32,
    pub rolls_held_for_score: u32,
    pub rolls_let_go_for_score: u32,
    pub mines_hit_for_score: u32,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct ItgScoreInputs {
    pub scoring_counts: judgment::JudgeCounts,
    pub holds_held_for_score: u32,
    pub rolls_held_for_score: u32,
    pub mines_hit_for_score: u32,
    pub holds_resolved_for_score: u32,
    pub rolls_resolved_for_score: u32,
    pub possible_grade_points: i32,
}

pub fn itg_score_inputs_from_display(
    stage: ItgScoreStage,
    carry: CourseDisplayCarry,
    totals: CourseDisplayTotals,
) -> ItgScoreInputs {
    let mut scoring_counts = stage.scoring_counts;
    let mut ix = 0usize;
    while ix < judgment::JUDGE_GRADE_COUNT {
        scoring_counts[ix] = scoring_counts[ix].saturating_add(carry.scoring_counts[ix]);
        ix += 1;
    }

    let holds_held_for_score = stage
        .holds_held_for_score
        .saturating_add(carry.holds_held_for_score);
    let rolls_held_for_score = stage
        .rolls_held_for_score
        .saturating_add(carry.rolls_held_for_score);
    ItgScoreInputs {
        scoring_counts,
        holds_held_for_score,
        rolls_held_for_score,
        mines_hit_for_score: stage
            .mines_hit_for_score
            .saturating_add(carry.mines_hit_for_score),
        holds_resolved_for_score: holds_held_for_score
            .saturating_add(stage.holds_let_go_for_score)
            .saturating_add(carry.holds_let_go_for_score),
        rolls_resolved_for_score: rolls_held_for_score
            .saturating_add(stage.rolls_let_go_for_score)
            .saturating_add(carry.rolls_let_go_for_score),
        possible_grade_points: totals.possible_grade_points,
    }
}

pub fn itg_score_percent_from_inputs(inputs: ItgScoreInputs) -> f64 {
    judgment::calculate_itg_score_percent_from_counts(
        &inputs.scoring_counts,
        inputs.holds_held_for_score,
        inputs.rolls_held_for_score,
        inputs.mines_hit_for_score,
        inputs.possible_grade_points,
    )
}

pub fn predictive_itg_score_percent_from_inputs(inputs: ItgScoreInputs) -> f64 {
    let actual = judgment::calculate_itg_grade_points_from_counts(
        &inputs.scoring_counts,
        inputs.holds_held_for_score,
        inputs.rolls_held_for_score,
        inputs.mines_hit_for_score,
    );
    let current_possible = judgment::current_possible_grade_points_from_counts(
        &inputs.scoring_counts,
        inputs.holds_resolved_for_score,
        inputs.rolls_resolved_for_score,
    );
    let (kept, _, _) = judgment::predictive_itg_score_percents(
        current_possible,
        inputs.possible_grade_points,
        actual,
    );
    kept
}

pub fn display_itg_score_percent_for_mode(
    inputs: ItgScoreInputs,
    mode: GameplayScoreDisplayMode,
) -> f64 {
    match mode {
        GameplayScoreDisplayMode::Normal => itg_score_percent_from_inputs(inputs) * 100.0,
        GameplayScoreDisplayMode::Predictive => predictive_itg_score_percent_from_inputs(inputs),
    }
}

pub fn display_ex_score_percent_for_mode(
    score: &judgment::ExScoreData,
    mode: GameplayScoreDisplayMode,
) -> f64 {
    match mode {
        GameplayScoreDisplayMode::Normal => judgment::ex_score_percent(score),
        GameplayScoreDisplayMode::Predictive => judgment::predictive_ex_score_percents(score).0,
    }
}

pub fn display_hard_ex_score_percent_for_mode(
    score: &judgment::ExScoreData,
    mode: GameplayScoreDisplayMode,
) -> f64 {
    match mode {
        GameplayScoreDisplayMode::Normal => judgment::hard_ex_score_percent(score),
        GameplayScoreDisplayMode::Predictive => {
            judgment::predictive_hard_ex_score_percents(score).0
        }
    }
}

#[inline(always)]
pub fn stream_segments_for_note_data(
    notes: &[u8],
    lanes: usize,
    constant_bpm: bool,
) -> (Vec<StreamSegment>, f32, f32) {
    let densities = measure_densities(notes, lanes);
    zmod_stream_totals_for_densities(&densities, constant_bpm)
}

pub fn measure_counter_segments_for_densities(
    densities: &[usize],
    notes_threshold: Option<usize>,
) -> Vec<StreamSegment> {
    notes_threshold.map_or_else(Vec::new, |threshold| {
        stream_sequences_threshold(densities, threshold)
    })
}

#[inline(always)]
pub fn zmod_stream_totals_for_densities(
    densities: &[usize],
    constant_bpm: bool,
) -> (Vec<StreamSegment>, f32, f32) {
    zmod_stream_totals_full_measures(densities, constant_bpm)
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum GameplayTargetScoreSetting {
    CMinus,
    C,
    CPlus,
    BMinus,
    B,
    BPlus,
    AMinus,
    A,
    APlus,
    SMinus,
    #[default]
    S,
    SPlus,
    MachineBest,
    PersonalBest,
}

pub const fn target_score_setting_percent(setting: GameplayTargetScoreSetting) -> Option<f64> {
    match setting {
        GameplayTargetScoreSetting::CMinus => Some(50.0),
        GameplayTargetScoreSetting::C => Some(55.0),
        GameplayTargetScoreSetting::CPlus => Some(60.0),
        GameplayTargetScoreSetting::BMinus => Some(64.0),
        GameplayTargetScoreSetting::B => Some(68.0),
        GameplayTargetScoreSetting::BPlus => Some(72.0),
        GameplayTargetScoreSetting::AMinus => Some(76.0),
        GameplayTargetScoreSetting::A => Some(80.0),
        GameplayTargetScoreSetting::APlus => Some(83.0),
        GameplayTargetScoreSetting::SMinus => Some(86.0),
        GameplayTargetScoreSetting::S => Some(89.0),
        GameplayTargetScoreSetting::SPlus => Some(92.0),
        GameplayTargetScoreSetting::MachineBest | GameplayTargetScoreSetting::PersonalBest => None,
    }
}

pub fn resolve_target_score_percent(
    setting: GameplayTargetScoreSetting,
    personal_best: Option<f64>,
    machine_best: Option<f64>,
) -> f64 {
    match setting {
        GameplayTargetScoreSetting::MachineBest => machine_best.or(personal_best),
        GameplayTargetScoreSetting::PersonalBest => personal_best,
        fixed => target_score_setting_percent(fixed),
    }
    .unwrap_or(89.0)
}

#[derive(Clone, Debug, PartialEq)]
pub struct ChartAttackWindow {
    pub start_second: f32,
    pub len_seconds: f32,
    pub mods: String,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum GameplayAttackMode {
    #[default]
    Off,
    On,
    Random,
}

pub const RANDOM_ATTACK_RUN_TIME_SECONDS: f32 = 6.0;
pub const RANDOM_ATTACK_OVERLAP_SECONDS: f32 = 0.5;
pub const RANDOM_ATTACK_START_SECONDS_INIT: f32 = -1.0;
pub const RANDOM_ATTACK_MIN_GAMEPLAY_SECONDS: f32 = 1.0;

// Mirrors ITGmania Data/RandomAttacks.txt categories for mods deadsync currently supports.
pub const RANDOM_ATTACK_MOD_POOL: [&str; 29] = [
    "0.5x",
    "1x",
    "1.5x",
    "2x",
    "boost",
    "brake",
    "wave",
    "expand",
    "drunk",
    "dizzy",
    "confusion",
    "65% mini",
    "20% flip",
    "30% invert",
    "30% tornado",
    "tipsy",
    "beat",
    "bumpy",
    "50% hidden",
    "50% sudden",
    "30% blink",
    "30% reverse",
    "reverse",
    "centered",
    "hallway",
    "space",
    "incoming",
    "overhead",
    "distant",
];

pub fn parse_chart_attack_windows(raw: &str) -> Vec<ChartAttackWindow> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Vec::new();
    }

    let upper = raw.to_ascii_uppercase();
    let mut starts = Vec::with_capacity(8);
    let mut scan = 0usize;
    while let Some(pos) = upper[scan..].find("TIME=") {
        let idx = scan + pos;
        starts.push(idx);
        scan = idx.saturating_add(5);
        if scan >= raw.len() {
            break;
        }
    }
    if starts.is_empty() {
        return Vec::new();
    }

    let mut out = Vec::with_capacity(starts.len());
    for (i, start) in starts.iter().copied().enumerate() {
        let end = starts.get(i + 1).copied().unwrap_or(raw.len());
        let chunk = &raw[start..end];
        let mut time = None;
        let mut len = None;
        let mut end_time = None;
        let mut mods = None;

        for part in chunk.split(':') {
            let part = part.trim();
            let Some((k, v)) = part.split_once('=') else {
                continue;
            };
            let key = k.trim().to_ascii_uppercase();
            let value = v.trim().trim_end_matches(',').trim();
            if value.is_empty() {
                continue;
            }
            match key.as_str() {
                "TIME" => time = value.parse::<f32>().ok(),
                "LEN" => len = value.parse::<f32>().ok(),
                "END" => end_time = value.parse::<f32>().ok(),
                "MODS" => mods = Some(value.to_string()),
                _ => {}
            }
        }

        let (Some(start_second), Some(mods)) = (time, mods) else {
            continue;
        };
        if !start_second.is_finite() || mods.is_empty() {
            continue;
        }
        let mut len_seconds = len.unwrap_or(0.0);
        if let Some(end_second) = end_time
            && end_second.is_finite()
        {
            len_seconds = end_second - start_second;
        }
        if !len_seconds.is_finite() || len_seconds < 0.0 {
            len_seconds = 0.0;
        }
        out.push(ChartAttackWindow {
            start_second,
            len_seconds,
            mods,
        });
    }

    out
}

#[inline(always)]
pub fn random_attack_seed(base_seed: u64, player: usize, attacks_len: usize) -> u64 {
    base_seed
        ^ (0xC2B2_AE3D_27D4_EB4F_u64.wrapping_mul(player as u64 + 1))
        ^ (attacks_len as u64).wrapping_mul(0x9E37_79B9_u64)
}

pub fn build_random_attack_windows(
    song_length_seconds: f32,
    player: usize,
    base_seed: u64,
) -> Vec<ChartAttackWindow> {
    if !song_length_seconds.is_finite() || song_length_seconds <= 0.0 {
        return Vec::new();
    }
    let period = (RANDOM_ATTACK_RUN_TIME_SECONDS - RANDOM_ATTACK_OVERLAP_SECONDS).max(0.0);
    if period <= f32::EPSILON || RANDOM_ATTACK_MOD_POOL.is_empty() {
        return Vec::new();
    }
    let first_start =
        (period + RANDOM_ATTACK_START_SECONDS_INIT).max(RANDOM_ATTACK_MIN_GAMEPLAY_SECONDS);
    if first_start >= song_length_seconds {
        return Vec::new();
    }

    let max_windows = ((song_length_seconds - first_start) / period)
        .floor()
        .max(0.0) as usize
        + 1;
    let mut out = Vec::with_capacity(max_windows);
    let mut rng = TurnRng::new(random_attack_seed(base_seed, player, max_windows));
    let mut start = first_start;
    while start < song_length_seconds {
        let mod_idx = rng.gen_range(RANDOM_ATTACK_MOD_POOL.len());
        out.push(ChartAttackWindow {
            start_second: start,
            len_seconds: RANDOM_ATTACK_RUN_TIME_SECONDS,
            mods: RANDOM_ATTACK_MOD_POOL[mod_idx].to_string(),
        });
        start += period;
    }
    out
}

pub fn build_attack_windows_for_mode(
    chart_attacks: Option<&str>,
    attack_mode: GameplayAttackMode,
    player: usize,
    base_seed: u64,
    song_length_seconds: f32,
) -> Vec<ChartAttackWindow> {
    match attack_mode {
        GameplayAttackMode::Off => Vec::new(),
        GameplayAttackMode::On => chart_attacks
            .map(parse_chart_attack_windows)
            .unwrap_or_default(),
        GameplayAttackMode::Random => {
            build_random_attack_windows(song_length_seconds, player, base_seed)
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct ParsedAttackMods {
    pub insert_mask: u8,
    pub remove_mask: u8,
    pub holds_mask: u8,
    pub turn_option: GameplayTurnOption,
    pub clear_all: bool,
    pub accel: AccelOverrides,
    pub visual: VisualOverrides,
    pub visual_speed: VisualOverrides,
    pub appearance: AppearanceOverrides,
    pub appearance_speed: AppearanceOverrides,
    pub visibility: VisibilityOverrides,
    pub scroll: ScrollOverrides,
    pub scroll_approach_speed: ScrollOverrides,
    pub perspective: PerspectiveOverrides,
    pub scroll_speed: Option<ScrollSpeedSetting>,
    pub mini_percent: Option<f32>,
    pub mini_speed: Option<f32>,
}

impl Default for ParsedAttackMods {
    fn default() -> Self {
        Self {
            insert_mask: 0,
            remove_mask: 0,
            holds_mask: 0,
            turn_option: GameplayTurnOption::None,
            clear_all: false,
            accel: AccelOverrides::default(),
            visual: VisualOverrides::default(),
            visual_speed: VisualOverrides::default(),
            appearance: AppearanceOverrides::default(),
            appearance_speed: AppearanceOverrides::default(),
            visibility: VisibilityOverrides::default(),
            scroll: ScrollOverrides::default(),
            scroll_approach_speed: ScrollOverrides::default(),
            perspective: PerspectiveOverrides::default(),
            scroll_speed: None,
            mini_percent: None,
            mini_speed: None,
        }
    }
}

impl ParsedAttackMods {
    #[inline(always)]
    pub fn has_chart_effect(self) -> bool {
        self.insert_mask != 0
            || self.remove_mask != 0
            || self.holds_mask != 0
            || self.turn_option != GameplayTurnOption::None
    }

    #[inline(always)]
    pub fn has_runtime_mask_effect(self) -> bool {
        self.clear_all
            || self.accel.any()
            || self.visual.any()
            || self.appearance.any()
            || self.visibility.any()
            || self.scroll.any()
            || self.perspective.any()
            || self.scroll_speed.is_some()
            || self.mini_percent.is_some()
    }
}

pub fn chart_attacks_enabled_for_mode(
    chart_attacks: Option<&str>,
    attack_mode: GameplayAttackMode,
) -> bool {
    match attack_mode {
        GameplayAttackMode::Off => false,
        GameplayAttackMode::On => chart_attacks.is_some_and(|raw| !raw.trim().is_empty()),
        GameplayAttackMode::Random => true,
    }
}

pub fn player_chart_changes_for_options(
    has_uncommon_masks: bool,
    turn_option: GameplayTurnOption,
    chart_attacks: Option<&str>,
    attack_mode: GameplayAttackMode,
) -> bool {
    has_uncommon_masks
        || turn_option != GameplayTurnOption::None
        || chart_attacks_enabled_for_mode(chart_attacks, attack_mode)
}

pub fn begin_outro_attack_visual_clear(
    attacks_cleared_for_outro: &mut bool,
    num_players: usize,
    active_attack_visual: &[VisualOverrides; MAX_PLAYERS],
    outro_attack_visual: &mut [VisualOverrides; MAX_PLAYERS],
) {
    if *attacks_cleared_for_outro {
        return;
    }
    *attacks_cleared_for_outro = true;
    for player in 0..num_players.min(MAX_PLAYERS) {
        outro_attack_visual[player] = active_attack_visual[player];
    }
}

#[derive(Clone, Copy, Debug)]
pub struct AttackMaskWindow {
    pub start_second: f32,
    pub end_second: f32,
    pub sustain_end_second: f32,
    pub persist_after_end: bool,
    pub clear_all: bool,
    pub chart: ChartAttackEffects,
    pub accel: AccelOverrides,
    pub visual: VisualOverrides,
    pub visual_speed: VisualOverrides,
    pub appearance: AppearanceOverrides,
    pub appearance_speed: AppearanceOverrides,
    pub visibility: VisibilityOverrides,
    pub scroll: ScrollOverrides,
    pub scroll_approach_speed: ScrollOverrides,
    pub perspective: PerspectiveOverrides,
    pub scroll_speed: Option<ScrollSpeedSetting>,
    pub mini_percent: Option<f32>,
    pub mini_mode: MiniAttackMode,
    pub mini_speed: Option<f32>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SongLuaEaseMaskTarget {
    AccelBoost,
    AccelBrake,
    AccelWave,
    AccelExpand,
    AccelBoomerang,
    VisualDrunk,
    VisualDizzy,
    VisualConfusion,
    VisualConfusionOffset,
    VisualConfusionOffsetColumn(usize),
    VisualFlip,
    VisualInvert,
    VisualTornado,
    VisualTipsy,
    VisualTiny,
    VisualBumpy,
    VisualBumpyOffset,
    VisualBumpyPeriod,
    VisualBumpyColumn(usize),
    VisualTinyColumn(usize),
    VisualMoveXColumn(usize),
    VisualMoveYColumn(usize),
    VisualPulseInner,
    VisualPulseOuter,
    VisualPulsePeriod,
    VisualPulseOffset,
    VisualBeat,
    AppearanceHidden,
    AppearanceSudden,
    AppearanceStealth,
    AppearanceBlink,
    AppearanceRandomVanish,
    VisibilityDark,
    VisibilityBlind,
    VisibilityCover,
    ScrollReverse,
    ScrollSplit,
    ScrollAlternate,
    ScrollCross,
    ScrollCentered,
    PerspectiveTilt,
    PerspectiveSkew,
    ScrollSpeedX,
    ScrollSpeedC,
    ScrollSpeedM,
    MiniPercent,
    PlayerX,
    PlayerY,
    PlayerZ,
    PlayerRotationX,
    PlayerRotationZ,
    PlayerRotationY,
    PlayerSkewX,
    PlayerSkewY,
    PlayerZoom,
    PlayerZoomX,
    PlayerZoomY,
    PlayerZoomZ,
    ConfusionYOffsetY,
}

#[derive(Clone, Debug)]
pub struct SongLuaEaseMaskWindow {
    pub start_second: f32,
    pub end_second: f32,
    pub sustain_end_second: f32,
    pub target: SongLuaEaseMaskTarget,
    pub from: f32,
    pub to: f32,
    pub easing: Option<String>,
    pub opt1: Option<f32>,
    pub opt2: Option<f32>,
}

#[derive(Clone, Debug)]
pub struct SongLuaColumnOffsetWindowRuntime {
    pub column: usize,
    pub start_second: f32,
    pub end_second: f32,
    pub sustain_end_second: f32,
    pub from_y: f32,
    pub to_y: f32,
    pub easing: Option<String>,
    pub opt1: Option<f32>,
    pub opt2: Option<f32>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SongLuaNoteHideWindowRuntime {
    pub column: usize,
    pub start_beat: f32,
    pub end_beat: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SongLuaOverlayMessageRuntime {
    pub event_second: f32,
    pub command_index: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SongLuaOverlayEaseWindowRuntime<StateDelta> {
    pub overlay_index: usize,
    pub start_second: f32,
    pub end_second: f32,
    pub sustain_end_second: f32,
    pub cutoff_second: Option<f32>,
    pub from: StateDelta,
    pub to: StateDelta,
    pub easing: Option<String>,
    pub opt1: Option<f32>,
    pub opt2: Option<f32>,
}

#[inline(always)]
fn song_lua_normalized_value(value: f32) -> f32 {
    value / 100.0
}

fn push_song_lua_ease_target(
    out: &mut Vec<SongLuaEaseMaskWindow>,
    target: SongLuaEaseMaskTarget,
    start_second: f32,
    end_second: f32,
    sustain_end_second: f32,
    from: f32,
    to: f32,
    easing: Option<&str>,
    opt1: Option<f32>,
    opt2: Option<f32>,
) {
    out.push(SongLuaEaseMaskWindow {
        start_second,
        end_second,
        sustain_end_second,
        target,
        from,
        to,
        easing: easing.map(ToString::to_string),
        opt1,
        opt2,
    });
}

pub fn append_song_lua_ease_targets(
    out: &mut Vec<SongLuaEaseMaskWindow>,
    start_second: f32,
    end_second: f32,
    sustain_end_second: f32,
    target_name: &str,
    from: f32,
    to: f32,
    easing: Option<&str>,
    opt1: Option<f32>,
    opt2: Option<f32>,
) -> bool {
    let key = attack_token_key(target_name);
    if key.is_empty() {
        return false;
    }
    let pct_from = song_lua_normalized_value(from);
    let pct_to = song_lua_normalized_value(to);
    let mut push = |target, from, to| {
        push_song_lua_ease_target(
            out,
            target,
            start_second,
            end_second,
            sustain_end_second,
            from,
            to,
            easing,
            opt1,
            opt2,
        );
    };

    if let Some(col) = mod_column_suffix(&key, "bumpy") {
        push(
            SongLuaEaseMaskTarget::VisualBumpyColumn(col),
            pct_from,
            pct_to,
        );
        return true;
    }
    if let Some(col) = mod_column_suffix(&key, "tiny") {
        push(
            SongLuaEaseMaskTarget::VisualTinyColumn(col),
            pct_from,
            pct_to,
        );
        return true;
    }
    if let Some(col) = mod_column_suffix(&key, "movex") {
        push(
            SongLuaEaseMaskTarget::VisualMoveXColumn(col),
            pct_from,
            pct_to,
        );
        return true;
    }
    if let Some(col) = mod_column_suffix(&key, "movey") {
        push(
            SongLuaEaseMaskTarget::VisualMoveYColumn(col),
            pct_from,
            pct_to,
        );
        return true;
    }
    if let Some(col) = mod_column_suffix(&key, "confusionoffset") {
        push(
            SongLuaEaseMaskTarget::VisualConfusionOffsetColumn(col),
            pct_from,
            pct_to,
        );
        return true;
    }

    match key.as_str() {
        "boost" => push(SongLuaEaseMaskTarget::AccelBoost, pct_from, pct_to),
        "brake" => push(SongLuaEaseMaskTarget::AccelBrake, pct_from, pct_to),
        "wave" => push(SongLuaEaseMaskTarget::AccelWave, pct_from, pct_to),
        "expand" => push(SongLuaEaseMaskTarget::AccelExpand, pct_from, pct_to),
        "boomerang" => push(SongLuaEaseMaskTarget::AccelBoomerang, pct_from, pct_to),
        "drunk" => push(SongLuaEaseMaskTarget::VisualDrunk, pct_from, pct_to),
        "dizzy" => push(SongLuaEaseMaskTarget::VisualDizzy, pct_from, pct_to),
        "confusion" => push(SongLuaEaseMaskTarget::VisualConfusion, pct_from, pct_to),
        "confusionoffset" => push(
            SongLuaEaseMaskTarget::VisualConfusionOffset,
            pct_from,
            pct_to,
        ),
        "flip" => push(SongLuaEaseMaskTarget::VisualFlip, pct_from, pct_to),
        "invert" => push(SongLuaEaseMaskTarget::VisualInvert, pct_from, pct_to),
        "tornado" => push(SongLuaEaseMaskTarget::VisualTornado, pct_from, pct_to),
        "tipsy" => push(SongLuaEaseMaskTarget::VisualTipsy, pct_from, pct_to),
        "bumpy" => push(SongLuaEaseMaskTarget::VisualBumpy, pct_from, pct_to),
        "bumpyoffset" => push(SongLuaEaseMaskTarget::VisualBumpyOffset, pct_from, pct_to),
        "bumpyperiod" => push(SongLuaEaseMaskTarget::VisualBumpyPeriod, pct_from, pct_to),
        "pulseinner" => push(SongLuaEaseMaskTarget::VisualPulseInner, pct_from, pct_to),
        "pulseouter" => push(SongLuaEaseMaskTarget::VisualPulseOuter, pct_from, pct_to),
        "pulseperiod" => push(SongLuaEaseMaskTarget::VisualPulsePeriod, pct_from, pct_to),
        "pulseoffset" => push(SongLuaEaseMaskTarget::VisualPulseOffset, pct_from, pct_to),
        "beat" => push(SongLuaEaseMaskTarget::VisualBeat, pct_from, pct_to),
        "hidden" => push(SongLuaEaseMaskTarget::AppearanceHidden, pct_from, pct_to),
        "sudden" => push(SongLuaEaseMaskTarget::AppearanceSudden, pct_from, pct_to),
        "stealth" => push(SongLuaEaseMaskTarget::AppearanceStealth, pct_from, pct_to),
        "blink" => push(SongLuaEaseMaskTarget::AppearanceBlink, pct_from, pct_to),
        "rvanish" | "randomvanish" | "reversevanish" => push(
            SongLuaEaseMaskTarget::AppearanceRandomVanish,
            pct_from,
            pct_to,
        ),
        "dark" => push(SongLuaEaseMaskTarget::VisibilityDark, pct_from, pct_to),
        "blind" => push(SongLuaEaseMaskTarget::VisibilityBlind, pct_from, pct_to),
        "cover" => push(SongLuaEaseMaskTarget::VisibilityCover, pct_from, pct_to),
        "reverse" => push(SongLuaEaseMaskTarget::ScrollReverse, pct_from, pct_to),
        "split" => push(SongLuaEaseMaskTarget::ScrollSplit, pct_from, pct_to),
        "alternate" => push(SongLuaEaseMaskTarget::ScrollAlternate, pct_from, pct_to),
        "cross" => push(SongLuaEaseMaskTarget::ScrollCross, pct_from, pct_to),
        "centered" => push(SongLuaEaseMaskTarget::ScrollCentered, pct_from, pct_to),
        "incoming" => {
            push(SongLuaEaseMaskTarget::PerspectiveTilt, -pct_from, -pct_to);
            push(SongLuaEaseMaskTarget::PerspectiveSkew, pct_from, pct_to);
        }
        "space" => {
            push(SongLuaEaseMaskTarget::PerspectiveTilt, pct_from, pct_to);
            push(SongLuaEaseMaskTarget::PerspectiveSkew, pct_from, pct_to);
        }
        "hallway" => {
            push(SongLuaEaseMaskTarget::PerspectiveTilt, -pct_from, -pct_to);
            push(SongLuaEaseMaskTarget::PerspectiveSkew, 0.0, 0.0);
        }
        "distant" => {
            push(SongLuaEaseMaskTarget::PerspectiveTilt, pct_from, pct_to);
            push(SongLuaEaseMaskTarget::PerspectiveSkew, 0.0, 0.0);
        }
        "overhead" => {
            push(SongLuaEaseMaskTarget::PerspectiveTilt, 0.0, 0.0);
            push(SongLuaEaseMaskTarget::PerspectiveSkew, 0.0, 0.0);
        }
        "xmod" => push(SongLuaEaseMaskTarget::ScrollSpeedX, from, to),
        "cmod" => push(SongLuaEaseMaskTarget::ScrollSpeedC, from, to),
        "mmod" => push(SongLuaEaseMaskTarget::ScrollSpeedM, from, to),
        "tiny" => push(SongLuaEaseMaskTarget::VisualTiny, pct_from, pct_to),
        "mini" => push(SongLuaEaseMaskTarget::MiniPercent, from, to),
        "skewx" => push(SongLuaEaseMaskTarget::PlayerSkewX, pct_from, pct_to),
        "skewy" => push(SongLuaEaseMaskTarget::PlayerSkewY, pct_from, pct_to),
        "confusionyoffset" => push(
            SongLuaEaseMaskTarget::ConfusionYOffsetY,
            pct_from * (180.0 / std::f32::consts::PI),
            pct_to * (180.0 / std::f32::consts::PI),
        ),
        _ => return false,
    }
    true
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SongLuaPlayerTransformValues {
    pub x: Option<f32>,
    pub y: Option<f32>,
    pub z: Option<f32>,
    pub rotation_x: Option<f32>,
    pub rotation_z: Option<f32>,
    pub rotation_y: Option<f32>,
    pub skew_x: Option<f32>,
    pub skew_y: Option<f32>,
    pub zoom_x: Option<f32>,
    pub zoom_y: Option<f32>,
    pub zoom_z: Option<f32>,
    pub confusion_y_offset: Option<f32>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SongLuaPlayerTransform {
    pub x: Option<f32>,
    pub y: Option<f32>,
    pub z: f32,
    pub rotation_x: f32,
    pub rotation_z: f32,
    pub rotation_y: f32,
    pub skew_x: f32,
    pub skew_y: f32,
    pub zoom_x: f32,
    pub zoom_y: f32,
    pub zoom_z: f32,
    pub confusion_y_offset: f32,
}

impl Default for SongLuaPlayerTransform {
    fn default() -> Self {
        Self {
            x: None,
            y: None,
            z: 0.0,
            rotation_x: 0.0,
            rotation_z: 0.0,
            rotation_y: 0.0,
            skew_x: 0.0,
            skew_y: 0.0,
            zoom_x: 1.0,
            zoom_y: 1.0,
            zoom_z: 1.0,
            confusion_y_offset: 0.0,
        }
    }
}

#[inline(always)]
fn finite_transform_option(value: Option<f32>) -> Option<f32> {
    value.filter(|v| v.is_finite())
}

#[inline(always)]
fn finite_transform_or(value: Option<f32>, fallback: f32) -> f32 {
    finite_transform_option(value).unwrap_or(fallback)
}

impl SongLuaPlayerTransformValues {
    pub fn resolve(self) -> SongLuaPlayerTransform {
        SongLuaPlayerTransform {
            x: finite_transform_option(self.x),
            y: finite_transform_option(self.y),
            z: finite_transform_or(self.z, 0.0),
            rotation_x: finite_transform_or(self.rotation_x, 0.0),
            rotation_z: finite_transform_or(self.rotation_z, 0.0),
            rotation_y: finite_transform_or(self.rotation_y, 0.0),
            skew_x: finite_transform_or(self.skew_x, 0.0),
            skew_y: finite_transform_or(self.skew_y, 0.0),
            zoom_x: finite_transform_or(self.zoom_x, 1.0),
            zoom_y: finite_transform_or(self.zoom_y, 1.0),
            zoom_z: finite_transform_or(self.zoom_z, 1.0),
            confusion_y_offset: finite_transform_or(self.confusion_y_offset, 0.0),
        }
    }
}

pub fn song_lua_apply_player_transform_target(
    target: SongLuaEaseMaskTarget,
    value: f32,
    player: &mut SongLuaPlayerTransformValues,
) {
    if !value.is_finite() {
        return;
    }
    match target {
        SongLuaEaseMaskTarget::PlayerX => player.x = Some(value),
        SongLuaEaseMaskTarget::PlayerY => player.y = Some(value),
        SongLuaEaseMaskTarget::PlayerZ => player.z = Some(value),
        SongLuaEaseMaskTarget::PlayerRotationX => player.rotation_x = Some(value),
        SongLuaEaseMaskTarget::PlayerRotationZ => player.rotation_z = Some(value),
        SongLuaEaseMaskTarget::PlayerRotationY => player.rotation_y = Some(value),
        SongLuaEaseMaskTarget::PlayerSkewX => player.skew_x = Some(value),
        SongLuaEaseMaskTarget::PlayerSkewY => player.skew_y = Some(value),
        SongLuaEaseMaskTarget::PlayerZoom => {
            player.zoom_x = Some(value);
            player.zoom_y = Some(value);
            player.zoom_z = Some(value);
        }
        SongLuaEaseMaskTarget::PlayerZoomX => player.zoom_x = Some(value),
        SongLuaEaseMaskTarget::PlayerZoomY => player.zoom_y = Some(value),
        SongLuaEaseMaskTarget::PlayerZoomZ => player.zoom_z = Some(value),
        SongLuaEaseMaskTarget::ConfusionYOffsetY => player.confusion_y_offset = Some(value),
        _ => {}
    }
}

pub fn song_lua_apply_eased_target(
    target: SongLuaEaseMaskTarget,
    value: f32,
    accel: &mut AccelOverrides,
    visual: &mut VisualOverrides,
    appearance: &mut AppearanceEffects,
    visibility: &mut VisibilityOverrides,
    scroll: &mut ScrollOverrides,
    perspective: &mut PerspectiveOverrides,
    scroll_speed: &mut Option<ScrollSpeedSetting>,
    mini_percent: &mut Option<f32>,
    player: &mut SongLuaPlayerTransformValues,
) {
    if !value.is_finite() {
        return;
    }
    match target {
        SongLuaEaseMaskTarget::AccelBoost => accel.boost = Some(value),
        SongLuaEaseMaskTarget::AccelBrake => accel.brake = Some(value),
        SongLuaEaseMaskTarget::AccelWave => accel.wave = Some(value),
        SongLuaEaseMaskTarget::AccelExpand => accel.expand = Some(value),
        SongLuaEaseMaskTarget::AccelBoomerang => accel.boomerang = Some(value),
        SongLuaEaseMaskTarget::VisualDrunk => visual.drunk = Some(value),
        SongLuaEaseMaskTarget::VisualDizzy => visual.dizzy = Some(value),
        SongLuaEaseMaskTarget::VisualConfusion => visual.confusion = Some(value),
        SongLuaEaseMaskTarget::VisualConfusionOffset => visual.confusion_offset = Some(value),
        SongLuaEaseMaskTarget::VisualConfusionOffsetColumn(col) => {
            if col < MAX_COLS {
                visual.confusion_offset_cols[col] = Some(value);
            }
        }
        SongLuaEaseMaskTarget::VisualFlip => visual.flip = Some(value),
        SongLuaEaseMaskTarget::VisualInvert => visual.invert = Some(value),
        SongLuaEaseMaskTarget::VisualTornado => visual.tornado = Some(value),
        SongLuaEaseMaskTarget::VisualTipsy => visual.tipsy = Some(value),
        SongLuaEaseMaskTarget::VisualTiny => visual.tiny = Some(value),
        SongLuaEaseMaskTarget::VisualBumpy => visual.bumpy = Some(value),
        SongLuaEaseMaskTarget::VisualBumpyOffset => visual.bumpy_offset = Some(value),
        SongLuaEaseMaskTarget::VisualBumpyPeriod => visual.bumpy_period = Some(value),
        SongLuaEaseMaskTarget::VisualBumpyColumn(col) => {
            if col < MAX_COLS {
                visual.bumpy_cols[col] = Some(value);
            }
        }
        SongLuaEaseMaskTarget::VisualTinyColumn(col) => {
            if col < MAX_COLS {
                visual.tiny_cols[col] = Some(value);
            }
        }
        SongLuaEaseMaskTarget::VisualMoveXColumn(col) => {
            if col < MAX_COLS {
                visual.move_x_cols[col] = Some(value);
            }
        }
        SongLuaEaseMaskTarget::VisualMoveYColumn(col) => {
            if col < MAX_COLS {
                visual.move_y_cols[col] = Some(value);
            }
        }
        SongLuaEaseMaskTarget::VisualPulseInner => visual.pulse_inner = Some(value),
        SongLuaEaseMaskTarget::VisualPulseOuter => visual.pulse_outer = Some(value),
        SongLuaEaseMaskTarget::VisualPulsePeriod => visual.pulse_period = Some(value),
        SongLuaEaseMaskTarget::VisualPulseOffset => visual.pulse_offset = Some(value),
        SongLuaEaseMaskTarget::VisualBeat => visual.beat = Some(value),
        SongLuaEaseMaskTarget::AppearanceHidden => appearance.hidden = value,
        SongLuaEaseMaskTarget::AppearanceSudden => appearance.sudden = value,
        SongLuaEaseMaskTarget::AppearanceStealth => appearance.stealth = value,
        SongLuaEaseMaskTarget::AppearanceBlink => appearance.blink = value,
        SongLuaEaseMaskTarget::AppearanceRandomVanish => appearance.random_vanish = value,
        SongLuaEaseMaskTarget::VisibilityDark => visibility.dark = Some(value),
        SongLuaEaseMaskTarget::VisibilityBlind => visibility.blind = Some(value),
        SongLuaEaseMaskTarget::VisibilityCover => visibility.cover = Some(value),
        SongLuaEaseMaskTarget::ScrollReverse => scroll.reverse = Some(value),
        SongLuaEaseMaskTarget::ScrollSplit => scroll.split = Some(value),
        SongLuaEaseMaskTarget::ScrollAlternate => scroll.alternate = Some(value),
        SongLuaEaseMaskTarget::ScrollCross => scroll.cross = Some(value),
        SongLuaEaseMaskTarget::ScrollCentered => scroll.centered = Some(value),
        SongLuaEaseMaskTarget::PerspectiveTilt => perspective.tilt = Some(value),
        SongLuaEaseMaskTarget::PerspectiveSkew => perspective.skew = Some(value),
        SongLuaEaseMaskTarget::ScrollSpeedX => {
            if value > 0.0 {
                *scroll_speed = Some(ScrollSpeedSetting::XMod(value));
            }
        }
        SongLuaEaseMaskTarget::ScrollSpeedC => {
            if value > 0.0 {
                *scroll_speed = Some(ScrollSpeedSetting::CMod(value));
            }
        }
        SongLuaEaseMaskTarget::ScrollSpeedM => {
            if value > 0.0 {
                *scroll_speed = Some(ScrollSpeedSetting::MMod(value));
            }
        }
        SongLuaEaseMaskTarget::MiniPercent => *mini_percent = Some(value),
        SongLuaEaseMaskTarget::PlayerX
        | SongLuaEaseMaskTarget::PlayerY
        | SongLuaEaseMaskTarget::PlayerZ
        | SongLuaEaseMaskTarget::PlayerRotationX
        | SongLuaEaseMaskTarget::PlayerRotationZ
        | SongLuaEaseMaskTarget::PlayerRotationY
        | SongLuaEaseMaskTarget::PlayerSkewX
        | SongLuaEaseMaskTarget::PlayerSkewY
        | SongLuaEaseMaskTarget::PlayerZoom
        | SongLuaEaseMaskTarget::PlayerZoomX
        | SongLuaEaseMaskTarget::PlayerZoomY
        | SongLuaEaseMaskTarget::PlayerZoomZ
        | SongLuaEaseMaskTarget::ConfusionYOffsetY => {
            song_lua_apply_player_transform_target(target, value, player);
        }
    }
}

pub fn attack_mask_window_from_parts(
    attack: &ChartAttackWindow,
    mods: ParsedAttackMods,
) -> Option<AttackMaskWindow> {
    if !mods.has_runtime_mask_effect() && !mods.has_chart_effect() {
        return None;
    }
    let start_second = attack.start_second;
    let end_second = start_second + attack.len_seconds.max(0.0);
    if !start_second.is_finite() || !end_second.is_finite() || end_second <= start_second {
        return None;
    }
    Some(AttackMaskWindow {
        start_second,
        end_second,
        sustain_end_second: end_second,
        persist_after_end: false,
        clear_all: mods.clear_all,
        chart: ChartAttackEffects {
            insert_mask: mods.insert_mask,
            remove_mask: mods.remove_mask,
            holds_mask: mods.holds_mask,
            turn_bits: turn_option_bits(mods.turn_option),
        },
        accel: mods.accel,
        visual: mods.visual,
        visual_speed: mods.visual_speed,
        appearance: mods.appearance,
        appearance_speed: mods.appearance_speed,
        visibility: mods.visibility,
        scroll: mods.scroll,
        scroll_approach_speed: mods.scroll_approach_speed,
        perspective: mods.perspective,
        scroll_speed: mods.scroll_speed,
        mini_percent: mods.mini_percent,
        mini_mode: MiniAttackMode::Absolute,
        mini_speed: mods.mini_speed,
    })
}

pub fn build_attack_mask_windows(attacks: &[ChartAttackWindow]) -> Vec<AttackMaskWindow> {
    if attacks.is_empty() {
        return Vec::new();
    }
    let mut windows = Vec::with_capacity(attacks.len());
    for attack in attacks {
        if let Some(window) = attack_mask_window_from_parts(attack, parse_attack_mods(&attack.mods))
        {
            windows.push(window);
        }
    }
    windows
}

#[inline(always)]
pub const fn song_lua_player_transform_target(target: SongLuaEaseMaskTarget) -> bool {
    matches!(
        target,
        SongLuaEaseMaskTarget::PlayerX
            | SongLuaEaseMaskTarget::PlayerY
            | SongLuaEaseMaskTarget::PlayerZ
            | SongLuaEaseMaskTarget::PlayerRotationX
            | SongLuaEaseMaskTarget::PlayerRotationZ
            | SongLuaEaseMaskTarget::PlayerRotationY
            | SongLuaEaseMaskTarget::PlayerSkewX
            | SongLuaEaseMaskTarget::PlayerSkewY
            | SongLuaEaseMaskTarget::PlayerZoom
            | SongLuaEaseMaskTarget::PlayerZoomX
            | SongLuaEaseMaskTarget::PlayerZoomY
            | SongLuaEaseMaskTarget::PlayerZoomZ
            | SongLuaEaseMaskTarget::ConfusionYOffsetY
    )
}

#[inline(always)]
fn song_lua_constant_sets_target(window: &AttackMaskWindow, target: SongLuaEaseMaskTarget) -> bool {
    if window.clear_all && !song_lua_player_transform_target(target) {
        return true;
    }
    match target {
        SongLuaEaseMaskTarget::AccelBoost => window.accel.boost.is_some(),
        SongLuaEaseMaskTarget::AccelBrake => window.accel.brake.is_some(),
        SongLuaEaseMaskTarget::AccelWave => window.accel.wave.is_some(),
        SongLuaEaseMaskTarget::AccelExpand => window.accel.expand.is_some(),
        SongLuaEaseMaskTarget::AccelBoomerang => window.accel.boomerang.is_some(),
        SongLuaEaseMaskTarget::VisualDrunk => window.visual.drunk.is_some(),
        SongLuaEaseMaskTarget::VisualDizzy => window.visual.dizzy.is_some(),
        SongLuaEaseMaskTarget::VisualConfusion => window.visual.confusion.is_some(),
        SongLuaEaseMaskTarget::VisualConfusionOffset => window.visual.confusion_offset.is_some(),
        SongLuaEaseMaskTarget::VisualConfusionOffsetColumn(col) => window
            .visual
            .confusion_offset_cols
            .get(col)
            .is_some_and(Option::is_some),
        SongLuaEaseMaskTarget::VisualFlip => window.visual.flip.is_some(),
        SongLuaEaseMaskTarget::VisualInvert => window.visual.invert.is_some(),
        SongLuaEaseMaskTarget::VisualTornado => window.visual.tornado.is_some(),
        SongLuaEaseMaskTarget::VisualTipsy => window.visual.tipsy.is_some(),
        SongLuaEaseMaskTarget::VisualTiny => window.visual.tiny.is_some(),
        SongLuaEaseMaskTarget::VisualBumpy => window.visual.bumpy.is_some(),
        SongLuaEaseMaskTarget::VisualBumpyOffset => window.visual.bumpy_offset.is_some(),
        SongLuaEaseMaskTarget::VisualBumpyPeriod => window.visual.bumpy_period.is_some(),
        SongLuaEaseMaskTarget::VisualBumpyColumn(col) => window
            .visual
            .bumpy_cols
            .get(col)
            .is_some_and(Option::is_some),
        SongLuaEaseMaskTarget::VisualTinyColumn(col) => window
            .visual
            .tiny_cols
            .get(col)
            .is_some_and(Option::is_some),
        SongLuaEaseMaskTarget::VisualMoveXColumn(col) => window
            .visual
            .move_x_cols
            .get(col)
            .is_some_and(Option::is_some),
        SongLuaEaseMaskTarget::VisualMoveYColumn(col) => window
            .visual
            .move_y_cols
            .get(col)
            .is_some_and(Option::is_some),
        SongLuaEaseMaskTarget::VisualPulseInner => window.visual.pulse_inner.is_some(),
        SongLuaEaseMaskTarget::VisualPulseOuter => window.visual.pulse_outer.is_some(),
        SongLuaEaseMaskTarget::VisualPulsePeriod => window.visual.pulse_period.is_some(),
        SongLuaEaseMaskTarget::VisualPulseOffset => window.visual.pulse_offset.is_some(),
        SongLuaEaseMaskTarget::VisualBeat => window.visual.beat.is_some(),
        SongLuaEaseMaskTarget::AppearanceHidden => window.appearance.hidden.is_some(),
        SongLuaEaseMaskTarget::AppearanceSudden => window.appearance.sudden.is_some(),
        SongLuaEaseMaskTarget::AppearanceStealth => window.appearance.stealth.is_some(),
        SongLuaEaseMaskTarget::AppearanceBlink => window.appearance.blink.is_some(),
        SongLuaEaseMaskTarget::AppearanceRandomVanish => window.appearance.random_vanish.is_some(),
        SongLuaEaseMaskTarget::VisibilityDark => window.visibility.dark.is_some(),
        SongLuaEaseMaskTarget::VisibilityBlind => window.visibility.blind.is_some(),
        SongLuaEaseMaskTarget::VisibilityCover => window.visibility.cover.is_some(),
        SongLuaEaseMaskTarget::ScrollReverse => window.scroll.reverse.is_some(),
        SongLuaEaseMaskTarget::ScrollSplit => window.scroll.split.is_some(),
        SongLuaEaseMaskTarget::ScrollAlternate => window.scroll.alternate.is_some(),
        SongLuaEaseMaskTarget::ScrollCross => window.scroll.cross.is_some(),
        SongLuaEaseMaskTarget::ScrollCentered => window.scroll.centered.is_some(),
        SongLuaEaseMaskTarget::PerspectiveTilt => window.perspective.tilt.is_some(),
        SongLuaEaseMaskTarget::PerspectiveSkew => window.perspective.skew.is_some(),
        SongLuaEaseMaskTarget::ScrollSpeedX
        | SongLuaEaseMaskTarget::ScrollSpeedC
        | SongLuaEaseMaskTarget::ScrollSpeedM => window.scroll_speed.is_some(),
        SongLuaEaseMaskTarget::MiniPercent => window.mini_percent.is_some(),
        SongLuaEaseMaskTarget::PlayerX
        | SongLuaEaseMaskTarget::PlayerY
        | SongLuaEaseMaskTarget::PlayerZ
        | SongLuaEaseMaskTarget::PlayerRotationX
        | SongLuaEaseMaskTarget::PlayerRotationZ
        | SongLuaEaseMaskTarget::PlayerRotationY
        | SongLuaEaseMaskTarget::PlayerSkewX
        | SongLuaEaseMaskTarget::PlayerSkewY
        | SongLuaEaseMaskTarget::PlayerZoom
        | SongLuaEaseMaskTarget::PlayerZoomX
        | SongLuaEaseMaskTarget::PlayerZoomY
        | SongLuaEaseMaskTarget::PlayerZoomZ
        | SongLuaEaseMaskTarget::ConfusionYOffsetY => false,
    }
}

fn song_lua_constant_cutoff_second(
    constant: &AttackMaskWindow,
    window: &SongLuaEaseMaskWindow,
    epsilon: f32,
) -> Option<f32> {
    if !constant.start_second.is_finite()
        || !constant.end_second.is_finite()
        || !window.end_second.is_finite()
        || !song_lua_constant_sets_target(constant, window.target)
    {
        return None;
    }
    if constant.end_second <= window.end_second + epsilon {
        return None;
    }
    if constant.start_second <= window.end_second + epsilon {
        Some(window.end_second)
    } else {
        Some(constant.start_second)
    }
}

pub fn song_lua_extend_ease_tails(
    out: &mut [SongLuaEaseMaskWindow],
    constants: &[AttackMaskWindow],
) {
    const SAME_TICK_EPSILON: f32 = 0.001;

    for i in 0..out.len() {
        let window = &out[i];
        let default_end = if window.sustain_end_second > window.end_second + SAME_TICK_EPSILON {
            window.sustain_end_second
        } else {
            f32::MAX
        };
        let cutoff_second = out
            .iter()
            .enumerate()
            .filter_map(|(j, other)| {
                if i == j
                    || other.target != window.target
                    || !other.start_second.is_finite()
                    || other.start_second <= window.start_second + SAME_TICK_EPSILON
                {
                    None
                } else {
                    Some(other.start_second)
                }
            })
            .fold(None::<f32>, |acc, start| {
                Some(match acc {
                    Some(current) => current.min(start),
                    None => start,
                })
            });
        let constant_cutoff = constants
            .iter()
            .filter_map(|constant| {
                song_lua_constant_cutoff_second(constant, window, SAME_TICK_EPSILON)
            })
            .fold(cutoff_second, |acc, start| {
                Some(match acc {
                    Some(current) => current.min(start),
                    None => start,
                })
            });
        out[i].sustain_end_second =
            constant_cutoff.map_or(default_end, |cutoff| default_end.min(cutoff));
    }
}

pub fn song_lua_extend_column_offset_tails(out: &mut [SongLuaColumnOffsetWindowRuntime]) {
    const SAME_TICK_EPSILON: f32 = 0.001;

    for i in 0..out.len() {
        let window = &out[i];
        let default_end = if window.sustain_end_second > window.end_second + SAME_TICK_EPSILON {
            window.sustain_end_second
        } else {
            f32::MAX
        };
        let cutoff_second = out
            .iter()
            .enumerate()
            .filter_map(|(j, other)| {
                if i == j
                    || other.column != window.column
                    || !other.start_second.is_finite()
                    || other.start_second <= window.start_second + SAME_TICK_EPSILON
                {
                    None
                } else {
                    Some(other.start_second)
                }
            })
            .fold(None::<f32>, |acc, start| {
                Some(match acc {
                    Some(current) => current.min(start),
                    None => start,
                })
            });
        out[i].sustain_end_second =
            cutoff_second.map_or(default_end, |cutoff| default_end.min(cutoff));
    }
}

#[inline(always)]
pub fn song_lua_note_hidden(
    windows: &[SongLuaNoteHideWindowRuntime],
    local_col: usize,
    beat: f32,
) -> bool {
    const EPS: f32 = 1.0e-4;
    windows.iter().any(|window| {
        window.column == local_col
            && beat + EPS >= window.start_beat
            && beat <= window.end_beat + EPS
    })
}

#[inline(always)]
pub fn offset_song_lua_message_events(events: &mut [SongLuaOverlayMessageRuntime], delta: f32) {
    if !delta.is_finite() || delta.abs() <= f32::EPSILON {
        return;
    }
    for event in events {
        event.event_second += delta;
    }
}

pub fn group_song_lua_overlay_eases<StateDelta>(
    overlay_count: usize,
    overlay_eases: Vec<SongLuaOverlayEaseWindowRuntime<StateDelta>>,
) -> (
    Vec<SongLuaOverlayEaseWindowRuntime<StateDelta>>,
    Vec<std::ops::Range<usize>>,
) {
    let mut buckets = Vec::with_capacity(overlay_count);
    buckets.resize_with(overlay_count, Vec::new);
    for ease in overlay_eases {
        if let Some(bucket) = buckets.get_mut(ease.overlay_index) {
            bucket.push(ease);
        }
    }
    let total_len = buckets.iter().map(Vec::len).sum();
    let mut flat = Vec::with_capacity(total_len);
    let mut ranges = Vec::with_capacity(overlay_count);
    for mut bucket in buckets {
        bucket.sort_by(|left, right| {
            left.start_second
                .total_cmp(&right.start_second)
                .then_with(|| left.end_second.total_cmp(&right.end_second))
                .then_with(|| left.sustain_end_second.total_cmp(&right.sustain_end_second))
        });
        let start = flat.len();
        flat.extend(bucket);
        ranges.push(start..flat.len());
    }
    (flat, ranges)
}

#[inline(always)]
pub fn offset_song_lua_overlay_eases<StateDelta>(
    eases: &mut [SongLuaOverlayEaseWindowRuntime<StateDelta>],
    delta: f32,
) {
    if !delta.is_finite() || delta.abs() <= f32::EPSILON {
        return;
    }
    for ease in eases {
        ease.start_second += delta;
        ease.end_second += delta;
        ease.sustain_end_second += delta;
        ease.cutoff_second = ease.cutoff_second.map(|cutoff| cutoff + delta);
    }
}

#[inline(always)]
fn song_lua_lerp_unclamped(a: f32, b: f32, t: f32) -> f32 {
    (b - a).mul_add(t, a)
}

pub fn song_lua_ease_window_value(window: &SongLuaEaseMaskWindow, now: f32) -> Option<f32> {
    if !now.is_finite()
        || !window.start_second.is_finite()
        || !window.sustain_end_second.is_finite()
        || !window.from.is_finite()
        || !window.to.is_finite()
        || now < window.start_second
        || now >= window.sustain_end_second
    {
        return None;
    }
    if !window.end_second.is_finite()
        || window.end_second <= window.start_second
        || now >= window.end_second
    {
        return Some(window.to);
    }
    let duration = window.end_second - window.start_second;
    if duration <= f32::EPSILON {
        return Some(window.to);
    }
    let factor = song_lua_ease_factor(
        window.easing.as_deref(),
        (now - window.start_second) / duration,
        window.opt1,
        window.opt2,
    );
    let value = song_lua_lerp_unclamped(window.from, window.to, factor);
    if value.is_finite() {
        Some(value)
    } else {
        Some(window.to)
    }
}

#[inline(always)]
pub fn chart_attack_row_range(
    attack: &ChartAttackWindow,
    timing_player: &TimingData,
) -> Option<(usize, usize)> {
    let start_beat = timing_player.get_beat_for_time(attack.start_second);
    let end_beat = timing_player.get_beat_for_time(attack.start_second + attack.len_seconds);
    let rows_per_beat = ROWS_PER_BEAT.max(1) as f32;
    let start_row = (start_beat.max(0.0) * rows_per_beat).round() as usize;
    let end_row = (end_beat.max(0.0) * rows_per_beat).round() as usize;
    (end_row >= start_row).then_some((start_row, end_row))
}

#[inline(always)]
pub fn chart_attack_turn_seed(base_seed: u64, player: usize, window_index: usize) -> u64 {
    base_seed
        ^ (0x9E37_79B9_u64.wrapping_mul(player as u64 + 1))
        ^ ((window_index as u64).wrapping_mul(0xA5A5_5A5A_u64))
}

pub fn apply_attack_turn_mod(
    notes: &mut [Note],
    col_offset: usize,
    cols: usize,
    turn_option: GameplayTurnOption,
    seed: u64,
    player: usize,
) {
    if notes.is_empty() || turn_option == GameplayTurnOption::None {
        return;
    }
    let note_range = (0usize, notes.len());
    match turn_option {
        GameplayTurnOption::None => {}
        GameplayTurnOption::Blender => {
            apply_turn_permutation(
                notes,
                note_range,
                col_offset,
                cols,
                GameplayTurnOption::Shuffle,
                seed,
            );
            apply_super_shuffle_taps(
                notes,
                note_range,
                col_offset,
                cols,
                seed ^ (0xD00D_F00D_u64.wrapping_mul(player as u64 + 1)),
            );
        }
        GameplayTurnOption::Random => {
            apply_hyper_shuffle(
                notes,
                note_range,
                col_offset,
                cols,
                seed ^ (0xA5A5_5A5A_u64.wrapping_mul(player as u64 + 1)),
            );
        }
        other => {
            apply_turn_permutation(notes, note_range, col_offset, cols, other, seed);
        }
    }
}

pub fn apply_chart_attack_window(
    notes: &mut Vec<Note>,
    timing_player: &TimingData,
    col_offset: usize,
    cols: usize,
    player: usize,
    row_bounds: (usize, usize),
    mods: ParsedAttackMods,
    turn_seed: u64,
) {
    let (start_row, end_row) = row_bounds;
    if notes.is_empty() || end_row < start_row || !mods.has_chart_effect() {
        return;
    }
    let mut in_range = Vec::with_capacity(notes.len());
    let mut out_range = Vec::with_capacity(notes.len());
    for note in notes.drain(..) {
        if note.row_index >= start_row && note.row_index <= end_row {
            in_range.push(note);
        } else {
            out_range.push(note);
        }
    }
    if in_range.is_empty() {
        *notes = out_range;
        return;
    }

    apply_uncommon_masks_with_masks(
        &mut in_range,
        mods.insert_mask,
        mods.remove_mask,
        mods.holds_mask,
        timing_player,
        col_offset,
        cols,
        &out_range,
        Some(row_bounds),
        player,
    );
    apply_attack_turn_mod(
        &mut in_range,
        col_offset,
        cols,
        mods.turn_option,
        turn_seed,
        player,
    );

    out_range.extend(in_range);
    *notes = out_range;
    sort_player_notes(notes);
}

pub fn apply_chart_attack_windows(
    notes: &mut Vec<Note>,
    attacks: &[ChartAttackWindow],
    timing_player: &TimingData,
    col_offset: usize,
    cols: usize,
    player: usize,
    base_seed: u64,
) {
    for (i, attack) in attacks.iter().enumerate() {
        let mods = parse_attack_mods(&attack.mods);
        if !mods.has_chart_effect() {
            continue;
        }
        let Some(row_bounds) = chart_attack_row_range(attack, timing_player) else {
            continue;
        };
        apply_chart_attack_window(
            notes,
            timing_player,
            col_offset,
            cols,
            player,
            row_bounds,
            mods,
            chart_attack_turn_seed(base_seed, player, i),
        );
    }
}

pub fn apply_chart_attacks_for_mode(
    notes: &mut Vec<Note>,
    chart_attacks: Option<&str>,
    attack_mode: GameplayAttackMode,
    timing_player: &TimingData,
    col_offset: usize,
    cols: usize,
    player: usize,
    base_seed: u64,
    song_length_seconds: f32,
) {
    let attacks = build_attack_windows_for_mode(
        chart_attacks,
        attack_mode,
        player,
        base_seed,
        song_length_seconds,
    );
    if !attacks.is_empty() {
        apply_chart_attack_windows(
            notes,
            &attacks,
            timing_player,
            col_offset,
            cols,
            player,
            base_seed,
        );
    }
}

#[derive(Clone, Copy)]
pub struct ChartAttackTransformPlayer<'a> {
    pub chart_attacks: Option<&'a str>,
    pub attack_mode: GameplayAttackMode,
    pub timing_player: &'a TimingData,
}

impl ChartAttackTransformPlayer<'_> {
    #[inline(always)]
    pub fn has_chart_attacks(self) -> bool {
        chart_attacks_enabled_for_mode(self.chart_attacks, self.attack_mode)
    }
}

pub fn apply_chart_attack_transforms(
    notes: &mut Vec<Note>,
    note_ranges: &mut [(usize, usize); MAX_PLAYERS],
    cols_per_player: usize,
    num_players: usize,
    players: &[ChartAttackTransformPlayer<'_>; MAX_PLAYERS],
    base_seed: u64,
    song_length_seconds: f32,
) {
    let active_players = num_players.min(MAX_PLAYERS);
    if active_players == 0
        || !players
            .iter()
            .take(active_players)
            .any(|player| player.has_chart_attacks())
    {
        return;
    }

    let mut transformed = Vec::with_capacity(notes.len());
    let mut transformed_ranges = [(0usize, 0usize); MAX_PLAYERS];
    for player in 0..active_players {
        let (start, end) = note_ranges[player];
        let slice_end = end.min(notes.len());
        let slice_start = start.min(slice_end);
        let out_start = transformed.len();
        let attack_player = players[player];
        if !attack_player.has_chart_attacks() {
            transformed.extend_from_slice(&notes[slice_start..slice_end]);
            transformed_ranges[player] = (out_start, transformed.len());
            continue;
        }

        let mut player_notes = notes[slice_start..slice_end].to_vec();
        apply_chart_attacks_for_mode(
            &mut player_notes,
            attack_player.chart_attacks,
            attack_player.attack_mode,
            attack_player.timing_player,
            player.saturating_mul(cols_per_player),
            cols_per_player,
            player,
            base_seed,
            song_length_seconds,
        );
        transformed.extend(player_notes);
        transformed_ranges[player] = (out_start, transformed.len());
    }

    if active_players == 1 {
        transformed_ranges[1] = transformed_ranges[0];
    }
    *notes = transformed;
    *note_ranges = transformed_ranges;
}

#[derive(Clone, Copy, Debug, Default)]
pub struct AttackActiveTargets {
    pub clear_all: bool,
    pub visual: VisualOverrides,
    pub scroll: ScrollOverrides,
    pub mini_percent: bool,
}

#[inline(always)]
fn mark_active_target(targets: &mut Option<f32>, value: Option<f32>) {
    if value.is_some() {
        *targets = Some(0.0);
    }
}

fn mark_visual_targets(targets: &mut VisualOverrides, visual: VisualOverrides) {
    mark_active_target(&mut targets.drunk, visual.drunk);
    mark_active_target(&mut targets.dizzy, visual.dizzy);
    mark_active_target(&mut targets.confusion, visual.confusion);
    mark_active_target(&mut targets.confusion_offset, visual.confusion_offset);
    for (target, value) in targets
        .confusion_offset_cols
        .iter_mut()
        .zip(visual.confusion_offset_cols)
    {
        mark_active_target(target, value);
    }
    mark_active_target(&mut targets.flip, visual.flip);
    mark_active_target(&mut targets.invert, visual.invert);
    mark_active_target(&mut targets.tornado, visual.tornado);
    mark_active_target(&mut targets.tipsy, visual.tipsy);
    mark_active_target(&mut targets.tiny, visual.tiny);
    mark_active_target(&mut targets.bumpy, visual.bumpy);
    mark_active_target(&mut targets.bumpy_offset, visual.bumpy_offset);
    mark_active_target(&mut targets.bumpy_period, visual.bumpy_period);
    for (target, value) in targets.bumpy_cols.iter_mut().zip(visual.bumpy_cols) {
        mark_active_target(target, value);
    }
    for (target, value) in targets.tiny_cols.iter_mut().zip(visual.tiny_cols) {
        mark_active_target(target, value);
    }
    for (target, value) in targets.move_x_cols.iter_mut().zip(visual.move_x_cols) {
        mark_active_target(target, value);
    }
    for (target, value) in targets.move_y_cols.iter_mut().zip(visual.move_y_cols) {
        mark_active_target(target, value);
    }
    mark_active_target(&mut targets.pulse_inner, visual.pulse_inner);
    mark_active_target(&mut targets.pulse_outer, visual.pulse_outer);
    mark_active_target(&mut targets.pulse_period, visual.pulse_period);
    mark_active_target(&mut targets.pulse_offset, visual.pulse_offset);
    mark_active_target(&mut targets.beat, visual.beat);
}

fn mark_scroll_targets(targets: &mut ScrollOverrides, scroll: ScrollOverrides) {
    mark_active_target(&mut targets.reverse, scroll.reverse);
    mark_active_target(&mut targets.split, scroll.split);
    mark_active_target(&mut targets.alternate, scroll.alternate);
    mark_active_target(&mut targets.cross, scroll.cross);
    mark_active_target(&mut targets.centered, scroll.centered);
}

pub fn collect_active_attack_targets(
    windows: &[AttackMaskWindow],
    now: f32,
) -> AttackActiveTargets {
    let mut targets = AttackActiveTargets::default();
    for window in windows {
        if now < window.start_second || now >= window.end_second {
            continue;
        }
        if window.clear_all {
            targets.clear_all = true;
        }
        mark_visual_targets(&mut targets.visual, window.visual);
        mark_scroll_targets(&mut targets.scroll, window.scroll);
        if window.mini_percent.is_some() {
            targets.mini_percent = true;
        }
    }
    targets
}

#[inline(always)]
pub fn persisted_target_allowed(
    persisted: bool,
    active_clear_all: bool,
    active_target: Option<f32>,
) -> bool {
    !persisted || (!active_clear_all && active_target.is_none())
}

#[inline(always)]
pub fn persisted_mini_allowed(persisted: bool, active_targets: AttackActiveTargets) -> bool {
    !persisted || (!active_targets.clear_all && !active_targets.mini_percent)
}

#[derive(Clone, Copy, Debug)]
pub struct ActiveAttackMaskValues {
    pub clear_all: bool,
    pub chart: ChartAttackEffects,
    pub accel: AccelOverrides,
    pub visual: VisualOverrides,
    pub visual_speed: VisualOverrides,
    pub appearance_target: AppearanceEffects,
    pub appearance_speed: AppearanceEffects,
    pub visibility: VisibilityOverrides,
    pub scroll: ScrollOverrides,
    pub scroll_approach_speed: ScrollOverrides,
    pub perspective: PerspectiveOverrides,
    pub scroll_speed: Option<ScrollSpeedSetting>,
    pub mini_percent: Option<f32>,
    pub mini_speed: Option<f32>,
}

impl ActiveAttackMaskValues {
    #[inline(always)]
    pub fn new(base_appearance: AppearanceEffects) -> Self {
        Self {
            clear_all: false,
            chart: ChartAttackEffects::default(),
            accel: AccelOverrides::default(),
            visual: VisualOverrides::default(),
            visual_speed: VisualOverrides::default(),
            appearance_target: base_appearance,
            appearance_speed: AppearanceEffects::approach_speeds(),
            visibility: VisibilityOverrides::default(),
            scroll: ScrollOverrides::default(),
            scroll_approach_speed: ScrollOverrides::default(),
            perspective: PerspectiveOverrides::default(),
            scroll_speed: None,
            mini_percent: None,
            mini_speed: None,
        }
    }

    #[inline(always)]
    fn clear_for_window(&mut self) {
        *self = Self::new(AppearanceEffects::default());
        self.clear_all = true;
    }
}

#[derive(Clone, Copy, Debug)]
pub struct ActiveAttackRefreshInput<'a> {
    pub now: f32,
    pub delta_time: f32,
    pub attacks_cleared_for_outro: bool,
    pub base_appearance: AppearanceEffects,
    pub base_visual: VisualEffects,
    pub base_scroll: ScrollEffects,
    pub base_mini_percent: f32,
    pub attack_windows: &'a [AttackMaskWindow],
    pub song_lua_ease_windows: &'a [SongLuaEaseMaskWindow],
}

#[derive(Clone, Copy, Debug)]
pub struct ActiveAttackRefreshState {
    pub attack_current_appearance: AppearanceEffects,
    pub active_attack_visual: VisualOverrides,
    pub active_attack_visibility: VisibilityOverrides,
    pub active_attack_scroll: ScrollOverrides,
    pub active_attack_mini_percent: Option<f32>,
    pub outro_attack_visual: VisualOverrides,
}

#[derive(Clone, Copy, Debug)]
pub struct ActiveAttackRefreshOutput {
    pub attack_target_appearance: AppearanceEffects,
    pub attack_speed_appearance: AppearanceEffects,
    pub attack_current_appearance: AppearanceEffects,
    pub active_attack_clear_all: bool,
    pub active_attack_chart: ChartAttackEffects,
    pub active_attack_accel: AccelOverrides,
    pub active_attack_visual: VisualOverrides,
    pub active_attack_appearance: AppearanceEffects,
    pub active_attack_visibility: VisibilityOverrides,
    pub active_attack_scroll: ScrollOverrides,
    pub active_attack_perspective: PerspectiveOverrides,
    pub active_attack_scroll_speed: Option<ScrollSpeedSetting>,
    pub active_attack_mini_percent: Option<f32>,
    pub outro_attack_visual: VisualOverrides,
    pub player_transform: SongLuaPlayerTransformValues,
}

pub fn apply_song_lua_player_eases(
    player: &mut SongLuaPlayerTransformValues,
    windows: &[SongLuaEaseMaskWindow],
    now: f32,
) {
    for window in windows {
        if let Some(value) = song_lua_ease_window_value(window, now) {
            song_lua_apply_player_transform_target(window.target, value, player);
        }
    }
}

pub fn apply_song_lua_attack_eases(
    attack: &mut ActiveAttackMaskValues,
    appearance: &mut AppearanceEffects,
    player: &mut SongLuaPlayerTransformValues,
    windows: &[SongLuaEaseMaskWindow],
    now: f32,
    mini_base_percent: f32,
) {
    for window in windows {
        if let Some(value) = song_lua_ease_window_value(window, now) {
            let value = if matches!(window.target, SongLuaEaseMaskTarget::MiniPercent) {
                mini_base_percent + value
            } else {
                value
            };
            song_lua_apply_eased_target(
                window.target,
                value,
                &mut attack.accel,
                &mut attack.visual,
                appearance,
                &mut attack.visibility,
                &mut attack.scroll,
                &mut attack.perspective,
                &mut attack.scroll_speed,
                &mut attack.mini_percent,
                player,
            );
        }
    }
}

pub fn apply_active_attack_mask_window(
    values: &mut ActiveAttackMaskValues,
    window: &AttackMaskWindow,
    active_targets: AttackActiveTargets,
    persisted: bool,
    profile_mini_percent: f32,
) {
    if window.clear_all {
        values.clear_for_window();
    }
    values.chart.insert_mask |= window.chart.insert_mask;
    values.chart.remove_mask |= window.chart.remove_mask;
    values.chart.holds_mask |= window.chart.holds_mask;
    values.chart.turn_bits |= window.chart.turn_bits;

    if let Some(v) = window.accel.boost {
        values.accel.boost = Some(v);
    }
    if let Some(v) = window.accel.brake {
        values.accel.brake = Some(v);
    }
    if let Some(v) = window.accel.wave {
        values.accel.wave = Some(v);
    }
    if let Some(v) = window.accel.expand {
        values.accel.expand = Some(v);
    }
    if let Some(v) = window.accel.boomerang {
        values.accel.boomerang = Some(v);
    }

    apply_active_visual_window(values, window, active_targets, persisted);
    apply_appearance_target(
        &mut values.appearance_target,
        &mut values.appearance_speed,
        window.appearance,
        window.appearance_speed,
    );

    if let Some(v) = window.visibility.dark {
        values.visibility.dark = Some(v);
    }
    if let Some(v) = window.visibility.blind {
        values.visibility.blind = Some(v);
    }
    if let Some(v) = window.visibility.cover {
        values.visibility.cover = Some(v);
    }

    apply_active_scroll_window(values, window, active_targets, persisted);

    if let Some(v) = window.perspective.tilt {
        values.perspective.tilt = Some(v);
    }
    if let Some(v) = window.perspective.skew {
        values.perspective.skew = Some(v);
    }
    if let Some(speed) = window.scroll_speed {
        values.scroll_speed = Some(speed);
    }
    if let Some(mini) = window.mini_percent.filter(|v| v.is_finite())
        && persisted_mini_allowed(persisted, active_targets)
    {
        let base = if values.clear_all {
            0.0
        } else {
            profile_mini_percent
        };
        values.mini_percent =
            Some(attack_mini_target_percent(mini, window.mini_mode, base).clamp(-100.0, 150.0));
        values.mini_speed = window.mini_speed;
    }
}

pub fn refresh_active_attack_player(
    input: ActiveAttackRefreshInput<'_>,
    mut state: ActiveAttackRefreshState,
) -> ActiveAttackRefreshOutput {
    let active_targets = collect_active_attack_targets(input.attack_windows, input.now);
    let mut attack = ActiveAttackMaskValues::new(input.base_appearance);
    let mut player_transform = SongLuaPlayerTransformValues::default();
    for window in input.attack_windows {
        let persisted = window.persist_after_end && input.now >= window.end_second;
        if !input.attacks_cleared_for_outro
            && input.now >= window.start_second
            && input.now < window.sustain_end_second
            && (input.now < window.end_second || persisted)
        {
            apply_active_attack_mask_window(
                &mut attack,
                window,
                active_targets,
                persisted,
                input.base_mini_percent,
            );
        }
    }

    approach_appearance_effects(
        &mut state.attack_current_appearance,
        attack.appearance_target,
        attack.appearance_speed,
        input.delta_time,
    );
    let mut appearance = state.attack_current_appearance;
    if input.attacks_cleared_for_outro {
        apply_song_lua_player_eases(
            &mut player_transform,
            input.song_lua_ease_windows,
            input.now,
        );
        let mut visual = state.outro_attack_visual;
        approach_visual_overrides_to_base(&mut visual, input.base_visual, input.delta_time);
        return ActiveAttackRefreshOutput {
            attack_target_appearance: attack.appearance_target,
            attack_speed_appearance: attack.appearance_speed,
            attack_current_appearance: appearance,
            active_attack_clear_all: false,
            active_attack_chart: ChartAttackEffects::default(),
            active_attack_accel: AccelOverrides::default(),
            active_attack_visual: visual,
            active_attack_appearance: appearance,
            active_attack_visibility: state.active_attack_visibility,
            active_attack_scroll: ScrollOverrides::default(),
            active_attack_perspective: PerspectiveOverrides::default(),
            active_attack_scroll_speed: None,
            active_attack_mini_percent: None,
            outro_attack_visual: visual,
            player_transform,
        };
    }

    let base_visual = if attack.clear_all {
        VisualEffects::default()
    } else {
        input.base_visual
    };
    approach_visual_overrides_to_target(
        &mut state.active_attack_visual,
        attack.visual,
        attack.visual_speed,
        base_visual,
        input.delta_time,
    );
    attack.visual = state.active_attack_visual;

    let base_scroll = if attack.clear_all {
        ScrollEffects::default()
    } else {
        input.base_scroll
    };
    approach_scroll_overrides_to_target(
        &mut state.active_attack_scroll,
        attack.scroll,
        attack.scroll_approach_speed,
        base_scroll,
        input.delta_time,
    );
    attack.scroll = state.active_attack_scroll;

    let base_mini_percent = if attack.clear_all {
        0.0
    } else {
        input.base_mini_percent
    };
    approach_attack_mini_percent_to_target(
        &mut state.active_attack_mini_percent,
        attack.mini_percent,
        base_mini_percent,
        attack.mini_speed,
        input.delta_time,
    );
    attack.mini_percent = state.active_attack_mini_percent;

    apply_song_lua_attack_eases(
        &mut attack,
        &mut appearance,
        &mut player_transform,
        input.song_lua_ease_windows,
        input.now,
        base_mini_percent,
    );
    if let Some(mini) = attack.mini_percent.filter(|v| v.is_finite()) {
        attack.mini_percent = Some(mini.clamp(-100.0, 150.0));
    }

    ActiveAttackRefreshOutput {
        attack_target_appearance: attack.appearance_target,
        attack_speed_appearance: attack.appearance_speed,
        attack_current_appearance: appearance,
        active_attack_clear_all: attack.clear_all,
        active_attack_chart: attack.chart,
        active_attack_accel: attack.accel,
        active_attack_visual: attack.visual,
        active_attack_appearance: appearance,
        active_attack_visibility: attack.visibility,
        active_attack_scroll: attack.scroll,
        active_attack_perspective: attack.perspective,
        active_attack_scroll_speed: attack.scroll_speed,
        active_attack_mini_percent: attack.mini_percent,
        outro_attack_visual: state.outro_attack_visual,
        player_transform,
    }
}

fn apply_active_visual_target(
    value: &mut Option<f32>,
    speed: &mut Option<f32>,
    incoming: Option<f32>,
    incoming_speed: Option<f32>,
    active_target: Option<f32>,
    active_clear_all: bool,
    persisted: bool,
) {
    if let Some(v) = incoming
        && persisted_target_allowed(persisted, active_clear_all, active_target)
    {
        *value = Some(v);
        *speed = incoming_speed;
    }
}

fn apply_active_visual_cols(
    values: &mut [Option<f32>; MAX_COLS],
    speeds: &mut [Option<f32>; MAX_COLS],
    incoming: [Option<f32>; MAX_COLS],
    incoming_speeds: [Option<f32>; MAX_COLS],
    active: [Option<f32>; MAX_COLS],
    active_clear_all: bool,
    persisted: bool,
) {
    for col in 0..MAX_COLS {
        apply_active_visual_target(
            &mut values[col],
            &mut speeds[col],
            incoming[col],
            incoming_speeds[col],
            active[col],
            active_clear_all,
            persisted,
        );
    }
}

fn apply_active_visual_window(
    values: &mut ActiveAttackMaskValues,
    window: &AttackMaskWindow,
    active_targets: AttackActiveTargets,
    persisted: bool,
) {
    let active_clear_all = active_targets.clear_all;
    apply_active_visual_target(
        &mut values.visual.drunk,
        &mut values.visual_speed.drunk,
        window.visual.drunk,
        window.visual_speed.drunk,
        active_targets.visual.drunk,
        active_clear_all,
        persisted,
    );
    apply_active_visual_target(
        &mut values.visual.dizzy,
        &mut values.visual_speed.dizzy,
        window.visual.dizzy,
        window.visual_speed.dizzy,
        active_targets.visual.dizzy,
        active_clear_all,
        persisted,
    );
    apply_active_visual_target(
        &mut values.visual.confusion,
        &mut values.visual_speed.confusion,
        window.visual.confusion,
        window.visual_speed.confusion,
        active_targets.visual.confusion,
        active_clear_all,
        persisted,
    );
    apply_active_visual_target(
        &mut values.visual.confusion_offset,
        &mut values.visual_speed.confusion_offset,
        window.visual.confusion_offset,
        window.visual_speed.confusion_offset,
        active_targets.visual.confusion_offset,
        active_clear_all,
        persisted,
    );
    apply_active_visual_cols(
        &mut values.visual.confusion_offset_cols,
        &mut values.visual_speed.confusion_offset_cols,
        window.visual.confusion_offset_cols,
        window.visual_speed.confusion_offset_cols,
        active_targets.visual.confusion_offset_cols,
        active_clear_all,
        persisted,
    );
    apply_active_visual_target(
        &mut values.visual.flip,
        &mut values.visual_speed.flip,
        window.visual.flip,
        window.visual_speed.flip,
        active_targets.visual.flip,
        active_clear_all,
        persisted,
    );
    apply_active_visual_target(
        &mut values.visual.invert,
        &mut values.visual_speed.invert,
        window.visual.invert,
        window.visual_speed.invert,
        active_targets.visual.invert,
        active_clear_all,
        persisted,
    );
    apply_active_visual_target(
        &mut values.visual.tornado,
        &mut values.visual_speed.tornado,
        window.visual.tornado,
        window.visual_speed.tornado,
        active_targets.visual.tornado,
        active_clear_all,
        persisted,
    );
    apply_active_visual_target(
        &mut values.visual.tipsy,
        &mut values.visual_speed.tipsy,
        window.visual.tipsy,
        window.visual_speed.tipsy,
        active_targets.visual.tipsy,
        active_clear_all,
        persisted,
    );
    apply_active_visual_target(
        &mut values.visual.tiny,
        &mut values.visual_speed.tiny,
        window.visual.tiny,
        window.visual_speed.tiny,
        active_targets.visual.tiny,
        active_clear_all,
        persisted,
    );
    apply_active_visual_target(
        &mut values.visual.bumpy,
        &mut values.visual_speed.bumpy,
        window.visual.bumpy,
        window.visual_speed.bumpy,
        active_targets.visual.bumpy,
        active_clear_all,
        persisted,
    );
    apply_active_visual_target(
        &mut values.visual.bumpy_offset,
        &mut values.visual_speed.bumpy_offset,
        window.visual.bumpy_offset,
        window.visual_speed.bumpy_offset,
        active_targets.visual.bumpy_offset,
        active_clear_all,
        persisted,
    );
    apply_active_visual_target(
        &mut values.visual.bumpy_period,
        &mut values.visual_speed.bumpy_period,
        window.visual.bumpy_period,
        window.visual_speed.bumpy_period,
        active_targets.visual.bumpy_period,
        active_clear_all,
        persisted,
    );
    apply_active_visual_cols(
        &mut values.visual.bumpy_cols,
        &mut values.visual_speed.bumpy_cols,
        window.visual.bumpy_cols,
        window.visual_speed.bumpy_cols,
        active_targets.visual.bumpy_cols,
        active_clear_all,
        persisted,
    );
    apply_active_visual_cols(
        &mut values.visual.tiny_cols,
        &mut values.visual_speed.tiny_cols,
        window.visual.tiny_cols,
        window.visual_speed.tiny_cols,
        active_targets.visual.tiny_cols,
        active_clear_all,
        persisted,
    );
    apply_active_visual_cols(
        &mut values.visual.move_x_cols,
        &mut values.visual_speed.move_x_cols,
        window.visual.move_x_cols,
        window.visual_speed.move_x_cols,
        active_targets.visual.move_x_cols,
        active_clear_all,
        persisted,
    );
    apply_active_visual_cols(
        &mut values.visual.move_y_cols,
        &mut values.visual_speed.move_y_cols,
        window.visual.move_y_cols,
        window.visual_speed.move_y_cols,
        active_targets.visual.move_y_cols,
        active_clear_all,
        persisted,
    );
    apply_active_visual_target(
        &mut values.visual.pulse_inner,
        &mut values.visual_speed.pulse_inner,
        window.visual.pulse_inner,
        window.visual_speed.pulse_inner,
        active_targets.visual.pulse_inner,
        active_clear_all,
        persisted,
    );
    apply_active_visual_target(
        &mut values.visual.pulse_outer,
        &mut values.visual_speed.pulse_outer,
        window.visual.pulse_outer,
        window.visual_speed.pulse_outer,
        active_targets.visual.pulse_outer,
        active_clear_all,
        persisted,
    );
    apply_active_visual_target(
        &mut values.visual.pulse_period,
        &mut values.visual_speed.pulse_period,
        window.visual.pulse_period,
        window.visual_speed.pulse_period,
        active_targets.visual.pulse_period,
        active_clear_all,
        persisted,
    );
    apply_active_visual_target(
        &mut values.visual.pulse_offset,
        &mut values.visual_speed.pulse_offset,
        window.visual.pulse_offset,
        window.visual_speed.pulse_offset,
        active_targets.visual.pulse_offset,
        active_clear_all,
        persisted,
    );
    apply_active_visual_target(
        &mut values.visual.beat,
        &mut values.visual_speed.beat,
        window.visual.beat,
        window.visual_speed.beat,
        active_targets.visual.beat,
        active_clear_all,
        persisted,
    );
}

fn apply_active_scroll_target(
    value: &mut Option<f32>,
    speed: &mut Option<f32>,
    incoming: Option<f32>,
    incoming_speed: Option<f32>,
    active_target: Option<f32>,
    active_clear_all: bool,
    persisted: bool,
) {
    if let Some(v) = incoming
        && persisted_target_allowed(persisted, active_clear_all, active_target)
    {
        *value = Some(v);
        *speed = incoming_speed;
    }
}

fn apply_active_scroll_window(
    values: &mut ActiveAttackMaskValues,
    window: &AttackMaskWindow,
    active_targets: AttackActiveTargets,
    persisted: bool,
) {
    let active_clear_all = active_targets.clear_all;
    apply_active_scroll_target(
        &mut values.scroll.reverse,
        &mut values.scroll_approach_speed.reverse,
        window.scroll.reverse,
        window.scroll_approach_speed.reverse,
        active_targets.scroll.reverse,
        active_clear_all,
        persisted,
    );
    apply_active_scroll_target(
        &mut values.scroll.split,
        &mut values.scroll_approach_speed.split,
        window.scroll.split,
        window.scroll_approach_speed.split,
        active_targets.scroll.split,
        active_clear_all,
        persisted,
    );
    apply_active_scroll_target(
        &mut values.scroll.alternate,
        &mut values.scroll_approach_speed.alternate,
        window.scroll.alternate,
        window.scroll_approach_speed.alternate,
        active_targets.scroll.alternate,
        active_clear_all,
        persisted,
    );
    apply_active_scroll_target(
        &mut values.scroll.cross,
        &mut values.scroll_approach_speed.cross,
        window.scroll.cross,
        window.scroll_approach_speed.cross,
        active_targets.scroll.cross,
        active_clear_all,
        persisted,
    );
    apply_active_scroll_target(
        &mut values.scroll.centered,
        &mut values.scroll_approach_speed.centered,
        window.scroll.centered,
        window.scroll_approach_speed.centered,
        active_targets.scroll.centered,
        active_clear_all,
        persisted,
    );
}

#[inline(always)]
pub const fn turn_option_bits(turn: GameplayTurnOption) -> u16 {
    match turn {
        GameplayTurnOption::None => 0,
        GameplayTurnOption::Mirror => 1 << 0,
        GameplayTurnOption::Left => 1 << 1,
        GameplayTurnOption::Right => 1 << 2,
        GameplayTurnOption::LRMirror => 1 << 3,
        GameplayTurnOption::UDMirror => 1 << 4,
        GameplayTurnOption::Shuffle => 1 << 5,
        GameplayTurnOption::Blender => 1 << 6,
        GameplayTurnOption::Random => 1 << 7,
    }
}

pub fn attack_token_key(token: &str) -> String {
    let mut key = String::with_capacity(token.len());
    for ch in token.chars() {
        if ch.is_ascii_alphanumeric() {
            key.push(ch.to_ascii_lowercase());
        }
    }
    while key.as_bytes().first().is_some_and(u8::is_ascii_digit) {
        key.remove(0);
    }
    key
}

#[inline(always)]
pub fn mod_column_suffix(key: &str, prefix: &str) -> Option<usize> {
    let suffix = key.strip_prefix(prefix)?;
    if suffix.is_empty() {
        return None;
    }
    let col = suffix.parse::<usize>().ok()?;
    (1..=MAX_COLS).contains(&col).then_some(col - 1)
}

#[inline(always)]
fn parse_attack_scroll_override(token: &str) -> Option<ScrollSpeedSetting> {
    let trimmed = token.trim();
    let value = trimmed
        .strip_suffix('x')
        .or_else(|| trimmed.strip_suffix('X'))
        .and_then(|v| v.trim().parse::<f32>().ok());
    if let Some(v) = value.filter(|v| v.is_finite() && *v > 0.0) {
        return Some(ScrollSpeedSetting::XMod(v));
    }
    ScrollSpeedSetting::from_str(trimmed).ok()
}

#[inline(always)]
fn parse_attack_approach_prefix(token: &str) -> (f32, &str) {
    let token = token.trim();
    let Some(prefix) = token.split_ascii_whitespace().next() else {
        return (1.0, token);
    };
    if prefix.len() <= 1 || !prefix.starts_with('*') {
        return (1.0, token);
    }
    let Some(speed) = prefix[1..]
        .parse::<f32>()
        .ok()
        .filter(|value| value.is_finite())
    else {
        return (1.0, token);
    };
    (speed.max(0.0), token[prefix.len()..].trim_start())
}

#[inline(always)]
fn attack_level(percent_value: Option<f32>) -> Option<f32> {
    let raw = percent_value.unwrap_or(100.0);
    raw.is_finite().then_some(raw / 100.0)
}

#[inline(always)]
fn parse_attack_percent_prefix(token: &str) -> (Option<f32>, &str) {
    let Some(idx) = token.find('%') else {
        return (None, token);
    };
    let value = token[..idx].trim().parse::<f32>().ok();
    (value, token[idx + 1..].trim())
}

#[inline(always)]
fn parse_attack_level_token(token: &str) -> (Option<f32>, &str) {
    let token = token.trim();
    if token.len() >= 3 && token[..3].eq_ignore_ascii_case("no ") {
        return (Some(0.0), token[3..].trim());
    }
    parse_attack_percent_prefix(token)
}

#[inline(always)]
fn set_approached_mod(
    value: &mut Option<f32>,
    value_speed: &mut Option<f32>,
    target: Option<f32>,
    approach_speed: f32,
) {
    *value = target;
    if target.is_some() {
        *value_speed = Some(approach_speed.max(0.0));
    }
}

fn apply_runtime_mod(
    out: &mut ParsedAttackMods,
    key: &str,
    percent_value: Option<f32>,
    approach_speed: f32,
) {
    if let Some(col) = mod_column_suffix(key, "bumpy") {
        set_approached_mod(
            &mut out.visual.bumpy_cols[col],
            &mut out.visual_speed.bumpy_cols[col],
            attack_level(percent_value),
            approach_speed,
        );
        return;
    }
    if let Some(col) = mod_column_suffix(key, "tiny") {
        set_approached_mod(
            &mut out.visual.tiny_cols[col],
            &mut out.visual_speed.tiny_cols[col],
            attack_level(percent_value),
            approach_speed,
        );
        return;
    }
    if let Some(col) = mod_column_suffix(key, "movex") {
        set_approached_mod(
            &mut out.visual.move_x_cols[col],
            &mut out.visual_speed.move_x_cols[col],
            attack_level(percent_value),
            approach_speed,
        );
        return;
    }
    if let Some(col) = mod_column_suffix(key, "movey") {
        set_approached_mod(
            &mut out.visual.move_y_cols[col],
            &mut out.visual_speed.move_y_cols[col],
            attack_level(percent_value),
            approach_speed,
        );
        return;
    }
    if let Some(col) = mod_column_suffix(key, "confusionoffset") {
        set_approached_mod(
            &mut out.visual.confusion_offset_cols[col],
            &mut out.visual_speed.confusion_offset_cols[col],
            attack_level(percent_value),
            approach_speed,
        );
        return;
    }

    match key {
        "wide" => out.insert_mask |= INSERT_MASK_BIT_WIDE,
        "big" => out.insert_mask |= INSERT_MASK_BIT_BIG,
        "quick" => out.insert_mask |= INSERT_MASK_BIT_QUICK,
        "bmrize" => out.insert_mask |= INSERT_MASK_BIT_BMRIZE,
        "skippy" => out.insert_mask |= INSERT_MASK_BIT_SKIPPY,
        "echo" => out.insert_mask |= INSERT_MASK_BIT_ECHO,
        "stomp" => out.insert_mask |= INSERT_MASK_BIT_STOMP,
        "mines" => out.insert_mask |= INSERT_MASK_BIT_MINES,
        "little" => out.remove_mask |= REMOVE_MASK_BIT_LITTLE,
        "nomines" => out.remove_mask |= REMOVE_MASK_BIT_NO_MINES,
        "noholds" => out.remove_mask |= REMOVE_MASK_BIT_NO_HOLDS,
        "nojumps" => out.remove_mask |= REMOVE_MASK_BIT_NO_JUMPS,
        "nohands" => out.remove_mask |= REMOVE_MASK_BIT_NO_HANDS,
        "noquads" => out.remove_mask |= REMOVE_MASK_BIT_NO_QUADS,
        "nolifts" => out.remove_mask |= REMOVE_MASK_BIT_NO_LIFTS,
        "nofakes" => out.remove_mask |= REMOVE_MASK_BIT_NO_FAKES,
        "planted" => out.holds_mask |= HOLDS_MASK_BIT_PLANTED,
        "floored" => out.holds_mask |= HOLDS_MASK_BIT_FLOORED,
        "twister" => out.holds_mask |= HOLDS_MASK_BIT_TWISTER,
        "norolls" => out.holds_mask |= HOLDS_MASK_BIT_NO_ROLLS,
        "holdrolls" | "holdstorolls" => out.holds_mask |= HOLDS_MASK_BIT_HOLDS_TO_ROLLS,
        "mirror" => out.turn_option = GameplayTurnOption::Mirror,
        "left" => out.turn_option = GameplayTurnOption::Left,
        "right" => out.turn_option = GameplayTurnOption::Right,
        "lrmirror" => out.turn_option = GameplayTurnOption::LRMirror,
        "udmirror" => out.turn_option = GameplayTurnOption::UDMirror,
        "shuffle" => out.turn_option = GameplayTurnOption::Shuffle,
        "supershuffle" | "blender" => out.turn_option = GameplayTurnOption::Blender,
        "hypershuffle" => out.turn_option = GameplayTurnOption::Random,
        "reverse" => set_approached_mod(
            &mut out.scroll.reverse,
            &mut out.scroll_approach_speed.reverse,
            attack_level(percent_value),
            approach_speed,
        ),
        "split" => set_approached_mod(
            &mut out.scroll.split,
            &mut out.scroll_approach_speed.split,
            attack_level(percent_value),
            approach_speed,
        ),
        "alternate" => set_approached_mod(
            &mut out.scroll.alternate,
            &mut out.scroll_approach_speed.alternate,
            attack_level(percent_value),
            approach_speed,
        ),
        "cross" => set_approached_mod(
            &mut out.scroll.cross,
            &mut out.scroll_approach_speed.cross,
            attack_level(percent_value),
            approach_speed,
        ),
        "centered" => set_approached_mod(
            &mut out.scroll.centered,
            &mut out.scroll_approach_speed.centered,
            attack_level(percent_value),
            approach_speed,
        ),
        "boost" => out.accel.boost = attack_level(percent_value),
        "brake" => out.accel.brake = attack_level(percent_value),
        "wave" => out.accel.wave = attack_level(percent_value),
        "expand" => out.accel.expand = attack_level(percent_value),
        "boomerang" => out.accel.boomerang = attack_level(percent_value),
        "drunk" => set_approached_mod(
            &mut out.visual.drunk,
            &mut out.visual_speed.drunk,
            attack_level(percent_value),
            approach_speed,
        ),
        "dizzy" => set_approached_mod(
            &mut out.visual.dizzy,
            &mut out.visual_speed.dizzy,
            attack_level(percent_value),
            approach_speed,
        ),
        "confusion" => set_approached_mod(
            &mut out.visual.confusion,
            &mut out.visual_speed.confusion,
            attack_level(percent_value),
            approach_speed,
        ),
        "confusionoffset" => set_approached_mod(
            &mut out.visual.confusion_offset,
            &mut out.visual_speed.confusion_offset,
            attack_level(percent_value),
            approach_speed,
        ),
        "flip" => set_approached_mod(
            &mut out.visual.flip,
            &mut out.visual_speed.flip,
            attack_level(percent_value),
            approach_speed,
        ),
        "invert" => set_approached_mod(
            &mut out.visual.invert,
            &mut out.visual_speed.invert,
            attack_level(percent_value),
            approach_speed,
        ),
        "tornado" => set_approached_mod(
            &mut out.visual.tornado,
            &mut out.visual_speed.tornado,
            attack_level(percent_value),
            approach_speed,
        ),
        "tipsy" => set_approached_mod(
            &mut out.visual.tipsy,
            &mut out.visual_speed.tipsy,
            attack_level(percent_value),
            approach_speed,
        ),
        "bumpy" => set_approached_mod(
            &mut out.visual.bumpy,
            &mut out.visual_speed.bumpy,
            attack_level(percent_value),
            approach_speed,
        ),
        "bumpyoffset" => set_approached_mod(
            &mut out.visual.bumpy_offset,
            &mut out.visual_speed.bumpy_offset,
            attack_level(percent_value),
            approach_speed,
        ),
        "bumpyperiod" => set_approached_mod(
            &mut out.visual.bumpy_period,
            &mut out.visual_speed.bumpy_period,
            attack_level(percent_value),
            approach_speed,
        ),
        "pulseinner" => set_approached_mod(
            &mut out.visual.pulse_inner,
            &mut out.visual_speed.pulse_inner,
            attack_level(percent_value),
            approach_speed,
        ),
        "pulseouter" => set_approached_mod(
            &mut out.visual.pulse_outer,
            &mut out.visual_speed.pulse_outer,
            attack_level(percent_value),
            approach_speed,
        ),
        "pulseperiod" => set_approached_mod(
            &mut out.visual.pulse_period,
            &mut out.visual_speed.pulse_period,
            attack_level(percent_value),
            approach_speed,
        ),
        "pulseoffset" => set_approached_mod(
            &mut out.visual.pulse_offset,
            &mut out.visual_speed.pulse_offset,
            attack_level(percent_value),
            approach_speed,
        ),
        "beat" => set_approached_mod(
            &mut out.visual.beat,
            &mut out.visual_speed.beat,
            attack_level(percent_value),
            approach_speed,
        ),
        "tiny" => set_approached_mod(
            &mut out.visual.tiny,
            &mut out.visual_speed.tiny,
            attack_level(percent_value),
            approach_speed,
        ),
        "mini" => {
            let mini = percent_value.unwrap_or(100.0);
            if mini.is_finite() {
                out.mini_percent = Some(mini);
                out.mini_speed = Some(approach_speed.max(0.0));
            }
        }
        "hidden" => {
            out.appearance.hidden = attack_level(percent_value);
            out.appearance_speed.hidden = Some(approach_speed);
        }
        "hiddenoffset" => {
            out.appearance.hidden_offset = attack_level(percent_value);
            out.appearance_speed.hidden_offset = Some(approach_speed);
        }
        "sudden" => {
            out.appearance.sudden = attack_level(percent_value);
            out.appearance_speed.sudden = Some(approach_speed);
        }
        "suddenoffset" => {
            out.appearance.sudden_offset = attack_level(percent_value);
            out.appearance_speed.sudden_offset = Some(approach_speed);
        }
        "stealth" => {
            out.appearance.stealth = attack_level(percent_value);
            out.appearance_speed.stealth = Some(approach_speed);
        }
        "blink" => {
            out.appearance.blink = attack_level(percent_value);
            out.appearance_speed.blink = Some(approach_speed);
        }
        "rvanish" | "randomvanish" | "reversevanish" => {
            out.appearance.random_vanish = attack_level(percent_value);
            out.appearance_speed.random_vanish = Some(approach_speed);
        }
        "dark" => out.visibility.dark = attack_level(percent_value),
        "blind" => out.visibility.blind = attack_level(percent_value),
        "cover" => out.visibility.cover = attack_level(percent_value),
        "overhead" => {
            out.perspective.tilt = Some(0.0);
            out.perspective.skew = Some(0.0);
        }
        "incoming" => {
            let level = attack_level(percent_value).unwrap_or(1.0);
            out.perspective.tilt = Some(-level);
            out.perspective.skew = Some(level);
        }
        "space" => {
            let level = attack_level(percent_value).unwrap_or(1.0);
            out.perspective.tilt = Some(level);
            out.perspective.skew = Some(level);
        }
        "hallway" => {
            let level = attack_level(percent_value).unwrap_or(1.0);
            out.perspective.tilt = Some(-level);
            out.perspective.skew = Some(0.0);
        }
        "distant" => {
            let level = attack_level(percent_value).unwrap_or(1.0);
            out.perspective.tilt = Some(level);
            out.perspective.skew = Some(0.0);
        }
        _ => {}
    }
}

pub fn parse_attack_mods(mods: &str) -> ParsedAttackMods {
    let mut out = ParsedAttackMods::default();
    for token in mods.split(',') {
        let (approach_speed, token) = parse_attack_approach_prefix(token);
        if token.is_empty() {
            continue;
        }
        if let Some(scroll_speed) = parse_attack_scroll_override(token) {
            out.scroll_speed = Some(scroll_speed);
            continue;
        }
        let (percent_value, token_key) = parse_attack_level_token(token);
        let key = attack_token_key(token_key);
        if key.is_empty() {
            continue;
        }
        match key.as_str() {
            "clearall" => {
                out = ParsedAttackMods {
                    clear_all: true,
                    ..ParsedAttackMods::default()
                };
            }
            _ => apply_runtime_mod(&mut out, key.as_str(), percent_value, approach_speed),
        }
    }
    out
}

#[inline(always)]
fn parse_song_lua_mod_amount(word: &str) -> Option<f32> {
    let word = word.trim();
    if word.eq_ignore_ascii_case("no") {
        return Some(0.0);
    }
    if let Some(value) = word.strip_suffix('%') {
        return value.trim().parse::<f32>().ok();
    }
    word.parse::<f32>().ok()
}

pub fn parse_song_lua_runtime_mods(mods: &str) -> ParsedAttackMods {
    let mut out = ParsedAttackMods::default();
    for token in mods.split(',') {
        let token = token.trim();
        if token.is_empty() {
            continue;
        }
        let parts: Vec<&str> = token
            .split_ascii_whitespace()
            .filter(|part| !part.is_empty())
            .collect();
        if parts.is_empty() {
            continue;
        }
        if parts.len() == 1 {
            if let Some(scroll_speed) = parse_attack_scroll_override(parts[0]) {
                out.scroll_speed = Some(scroll_speed);
                continue;
            }
            let key = attack_token_key(parts[0]);
            if key.is_empty() {
                continue;
            }
            if key == "clearall" {
                out = ParsedAttackMods {
                    clear_all: true,
                    ..ParsedAttackMods::default()
                };
                continue;
            }
            apply_runtime_mod(&mut out, key.as_str(), Some(100.0), 1.0);
            continue;
        }

        if parts[0].starts_with('*') {
            let approach_speed = parse_attack_approach_prefix(parts[0]).0;
            if parts.len() == 2 {
                if let Some(scroll_speed) = parse_attack_scroll_override(parts[1]) {
                    out.scroll_speed = Some(scroll_speed);
                    continue;
                }
                let key = attack_token_key(parts[1]);
                if !key.is_empty() {
                    apply_runtime_mod(&mut out, key.as_str(), Some(100.0), approach_speed);
                }
                continue;
            }
            let key = attack_token_key(parts[2]);
            if key.is_empty() {
                continue;
            }
            let amount = parse_song_lua_mod_amount(parts[1]).unwrap_or(0.0);
            apply_runtime_mod(&mut out, key.as_str(), Some(amount), approach_speed);
            continue;
        }

        let key = attack_token_key(parts[1]);
        if key.is_empty() {
            continue;
        }
        let amount = parse_song_lua_mod_amount(parts[0]).unwrap_or(0.0);
        apply_runtime_mod(&mut out, key.as_str(), Some(amount), 1.0);
    }
    out
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ScrollEffects {
    pub reverse: f32,
    pub split: f32,
    pub alternate: f32,
    pub cross: f32,
    pub centered: f32,
}

impl ScrollEffects {
    #[inline(always)]
    pub fn from_flags(
        reverse: bool,
        split: bool,
        alternate: bool,
        cross: bool,
        centered: bool,
    ) -> Self {
        Self {
            reverse: f32::from(reverse),
            split: f32::from(split),
            alternate: f32::from(alternate),
            cross: f32::from(cross),
            centered: f32::from(centered),
        }
    }

    #[inline(always)]
    pub fn reverse_percent_for_column(self, local_col: usize, num_cols: usize) -> f32 {
        scroll_reverse_percent_for_column(
            ScrollReverseOptions {
                reverse: self.reverse,
                split: self.split,
                alternate: self.alternate,
                cross: self.cross,
            },
            local_col,
            num_cols,
        )
    }

    #[inline(always)]
    pub fn reverse_scale_for_column(self, local_col: usize, num_cols: usize) -> f32 {
        scroll_reverse_scale_for_column(
            ScrollReverseOptions {
                reverse: self.reverse,
                split: self.split,
                alternate: self.alternate,
                cross: self.cross,
            },
            local_col,
            num_cols,
        )
    }
}

pub fn approach_scroll_overrides_to_target(
    current: &mut ScrollOverrides,
    target: ScrollOverrides,
    speed: ScrollOverrides,
    base: ScrollEffects,
    delta_time: f32,
) {
    approach_attack_value(
        &mut current.reverse,
        target.reverse,
        base.reverse,
        speed.reverse,
        delta_time,
        1.0,
    );
    approach_attack_value(
        &mut current.split,
        target.split,
        base.split,
        speed.split,
        delta_time,
        1.0,
    );
    approach_attack_value(
        &mut current.alternate,
        target.alternate,
        base.alternate,
        speed.alternate,
        delta_time,
        1.0,
    );
    approach_attack_value(
        &mut current.cross,
        target.cross,
        base.cross,
        speed.cross,
        delta_time,
        1.0,
    );
    approach_attack_value(
        &mut current.centered,
        target.centered,
        base.centered,
        speed.centered,
        delta_time,
        1.0,
    );
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PerspectiveEffects {
    pub tilt: f32,
    pub skew: f32,
}

#[inline(always)]
pub fn merge_attack_value(base: f32, attack: Option<f32>) -> f32 {
    attack.filter(|v| v.is_finite()).unwrap_or(base)
}

#[inline(always)]
pub fn merge_attack_accel_effects(base: AccelEffects, attack: AccelOverrides) -> AccelEffects {
    AccelEffects {
        boost: merge_attack_value(base.boost, attack.boost),
        brake: merge_attack_value(base.brake, attack.brake),
        wave: merge_attack_value(base.wave, attack.wave),
        expand: merge_attack_value(base.expand, attack.expand),
        boomerang: merge_attack_value(base.boomerang, attack.boomerang),
    }
}

pub fn merge_attack_visual_effects(base: VisualEffects, attack: VisualOverrides) -> VisualEffects {
    let mut confusion_offset_cols = base.confusion_offset_cols;
    let mut bumpy_cols = base.bumpy_cols;
    let mut tiny_cols = base.tiny_cols;
    let mut move_x_cols = base.move_x_cols;
    let mut move_y_cols = base.move_y_cols;
    for i in 0..MAX_COLS {
        if let Some(v) = attack.confusion_offset_cols[i].filter(|v| v.is_finite()) {
            confusion_offset_cols[i] = v;
        }
        if let Some(v) = attack.bumpy_cols[i].filter(|v| v.is_finite()) {
            bumpy_cols[i] = v;
        }
        if let Some(v) = attack.tiny_cols[i].filter(|v| v.is_finite()) {
            tiny_cols[i] = v;
        }
        if let Some(v) = attack.move_x_cols[i].filter(|v| v.is_finite()) {
            move_x_cols[i] = v;
        }
        if let Some(v) = attack.move_y_cols[i].filter(|v| v.is_finite()) {
            move_y_cols[i] = v;
        }
    }
    VisualEffects {
        drunk: merge_attack_value(base.drunk, attack.drunk),
        dizzy: merge_attack_value(base.dizzy, attack.dizzy),
        confusion: merge_attack_value(base.confusion, attack.confusion),
        confusion_offset: merge_attack_value(base.confusion_offset, attack.confusion_offset),
        confusion_offset_cols,
        big: base.big,
        flip: merge_attack_value(base.flip, attack.flip),
        invert: merge_attack_value(base.invert, attack.invert),
        tornado: merge_attack_value(base.tornado, attack.tornado),
        tipsy: merge_attack_value(base.tipsy, attack.tipsy),
        tiny: merge_attack_value(base.tiny, attack.tiny),
        bumpy: merge_attack_value(base.bumpy, attack.bumpy),
        bumpy_offset: merge_attack_value(base.bumpy_offset, attack.bumpy_offset),
        bumpy_period: merge_attack_value(base.bumpy_period, attack.bumpy_period),
        bumpy_cols,
        tiny_cols,
        move_x_cols,
        move_y_cols,
        pulse_inner: merge_attack_value(base.pulse_inner, attack.pulse_inner),
        pulse_outer: merge_attack_value(base.pulse_outer, attack.pulse_outer),
        pulse_period: merge_attack_value(base.pulse_period, attack.pulse_period),
        pulse_offset: merge_attack_value(base.pulse_offset, attack.pulse_offset),
        beat: merge_attack_value(base.beat, attack.beat),
    }
}

#[inline(always)]
pub fn merge_attack_visibility_effects(
    base: VisibilityEffects,
    attack: VisibilityOverrides,
) -> VisibilityEffects {
    VisibilityEffects {
        dark: merge_attack_value(base.dark, attack.dark),
        blind: merge_attack_value(base.blind, attack.blind),
        cover: merge_attack_value(base.cover, attack.cover),
    }
}

#[inline(always)]
pub fn merge_attack_scroll_effects(base: ScrollEffects, attack: ScrollOverrides) -> ScrollEffects {
    ScrollEffects {
        reverse: merge_attack_value(base.reverse, attack.reverse),
        split: merge_attack_value(base.split, attack.split),
        alternate: merge_attack_value(base.alternate, attack.alternate),
        cross: merge_attack_value(base.cross, attack.cross),
        centered: merge_attack_value(base.centered, attack.centered),
    }
}

#[inline(always)]
pub fn merge_attack_perspective_effects(
    base: PerspectiveEffects,
    attack: PerspectiveOverrides,
) -> PerspectiveEffects {
    PerspectiveEffects {
        tilt: merge_attack_value(base.tilt, attack.tilt),
        skew: merge_attack_value(base.skew, attack.skew),
    }
}

#[inline(always)]
pub fn effective_attack_accel_effects(
    base_cleared: bool,
    profile_mask_bits: u8,
    attack: AccelOverrides,
) -> AccelEffects {
    let base = if base_cleared {
        AccelEffects::default()
    } else {
        AccelEffects::from_mask_bits(profile_mask_bits)
    };
    merge_attack_accel_effects(base, attack)
}

#[inline(always)]
pub fn effective_attack_visual_effects(
    base_cleared: bool,
    profile_mask_bits: u16,
    attack: VisualOverrides,
) -> VisualEffects {
    let base = if base_cleared {
        VisualEffects::default()
    } else {
        VisualEffects::from_mask_bits(profile_mask_bits)
    };
    merge_attack_visual_effects(base, attack)
}

#[inline(always)]
pub fn effective_attack_visibility_effects(attack: VisibilityOverrides) -> VisibilityEffects {
    merge_attack_visibility_effects(VisibilityEffects::default(), attack)
}

#[inline(always)]
pub fn effective_attack_scroll_effects(
    base_cleared: bool,
    base_scroll: ScrollEffects,
    attack: ScrollOverrides,
) -> ScrollEffects {
    let base = if base_cleared {
        ScrollEffects::default()
    } else {
        base_scroll
    };
    merge_attack_scroll_effects(base, attack)
}

#[inline(always)]
pub fn effective_attack_perspective_effects(
    base_cleared: bool,
    base_perspective: PerspectiveEffects,
    attack: PerspectiveOverrides,
) -> PerspectiveEffects {
    let base = if base_cleared {
        PerspectiveEffects::default()
    } else {
        base_perspective
    };
    merge_attack_perspective_effects(base, attack)
}

#[inline(always)]
pub fn effective_attack_scroll_speed(
    base_cleared: bool,
    active_scroll_speed: Option<ScrollSpeedSetting>,
    base_scroll_speed: ScrollSpeedSetting,
) -> ScrollSpeedSetting {
    active_scroll_speed.unwrap_or_else(|| {
        if base_cleared {
            ScrollSpeedSetting::default()
        } else {
            base_scroll_speed
        }
    })
}

pub const SPACING_PERCENT_MIN: i32 = -100;
pub const SPACING_PERCENT_MAX: i32 = 100;

/// Multiplier applied to noteskin per-column lateral offsets for Spacing.
#[inline(always)]
pub fn spacing_multiplier_for_percent(spacing_percent: i32) -> f32 {
    let clamped = spacing_percent.clamp(SPACING_PERCENT_MIN, SPACING_PERCENT_MAX);
    1.0 + clamped as f32 / 100.0
}

#[inline(always)]
pub fn toggle_flash_alpha(timer_remaining: f32) -> Option<f32> {
    if timer_remaining <= 0.0 {
        return None;
    }
    let age = TOGGLE_FLASH_DURATION - timer_remaining;
    let alpha = if age < TOGGLE_FLASH_FADE_START {
        1.0
    } else {
        let fade_len = TOGGLE_FLASH_DURATION - TOGGLE_FLASH_FADE_START;
        1.0 - ((age - TOGGLE_FLASH_FADE_START) / fade_len).clamp(0.0, 1.0)
    };
    Some(alpha)
}

#[inline(always)]
pub fn tick_positive_timer(timer: &mut f32, delta_time: f32) {
    if *timer > 0.0 {
        *timer = (*timer - delta_time).max(0.0);
    }
}

#[inline(always)]
pub fn approach_f32(current: &mut f32, target: f32, step: f32) {
    if !current.is_finite() || !target.is_finite() {
        *current = target;
        return;
    }
    let step = step.max(0.0);
    if step <= f32::EPSILON || (*current - target).abs() <= f32::EPSILON {
        return;
    }
    let delta = target - *current;
    let step = delta.clamp(-step, step);
    if step.abs() >= delta.abs() {
        *current = target;
    } else {
        *current += step;
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum GameplayFailType {
    Immediate,
    ImmediateContinue,
}

impl Default for GameplayFailType {
    fn default() -> Self {
        Self::ImmediateContinue
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HoldToExitKey {
    Start,
    Back,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AutosyncMode {
    Off,
    Song,
    Machine,
}

#[inline(always)]
pub const fn autosync_mode_status_line(mode: AutosyncMode) -> Option<&'static str> {
    match mode {
        AutosyncMode::Off => None,
        AutosyncMode::Song => Some("AutoSync Song"),
        AutosyncMode::Machine => Some("AutoSync Machine"),
    }
}

#[inline(always)]
pub const fn next_autosync_mode(mode: AutosyncMode, course_active: bool) -> AutosyncMode {
    match mode {
        AutosyncMode::Off if course_active => AutosyncMode::Machine,
        AutosyncMode::Off => AutosyncMode::Song,
        AutosyncMode::Song => AutosyncMode::Machine,
        AutosyncMode::Machine => AutosyncMode::Off,
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum GameplayTurnOption {
    #[default]
    None,
    Mirror,
    LRMirror,
    UDMirror,
    Left,
    Right,
    Shuffle,
    Blender,
    Random,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ColumnScrollFlags {
    pub reverse: bool,
    pub split: bool,
    pub alternate: bool,
    pub cross: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ScrollReverseOptions {
    pub reverse: f32,
    pub split: f32,
    pub alternate: f32,
    pub cross: f32,
}

#[inline(always)]
pub fn scroll_reverse_percent_for_column(
    options: ScrollReverseOptions,
    local_col: usize,
    num_cols: usize,
) -> f32 {
    if num_cols == 0 {
        return 0.0;
    }
    let mut percent = options.reverse;
    if local_col >= num_cols / 2 {
        percent += options.split;
    }
    if (local_col & 1) != 0 {
        percent += options.alternate;
    }
    let first_cross_col = num_cols / 4;
    let last_cross_col = num_cols.saturating_sub(first_cross_col + 1);
    if local_col >= first_cross_col && local_col <= last_cross_col {
        percent += options.cross;
    }
    if percent > 2.0 {
        percent = percent.rem_euclid(2.0);
    }
    if percent > 1.0 {
        return lerp(1.0, 0.0, percent - 1.0);
    }
    percent.clamp(0.0, 1.0)
}

#[inline(always)]
pub fn scroll_reverse_scale_for_column(
    options: ScrollReverseOptions,
    local_col: usize,
    num_cols: usize,
) -> f32 {
    1.0 - 2.0 * scroll_reverse_percent_for_column(options, local_col, num_cols)
}

pub fn column_scroll_dirs_for_flags(flags: ColumnScrollFlags, num_cols: usize) -> [f32; MAX_COLS] {
    let mut dirs = [1.0_f32; MAX_COLS];
    let n = num_cols.min(MAX_COLS);

    if flags.reverse {
        for d in dirs.iter_mut().take(n) {
            *d *= -1.0;
        }
    }
    if flags.split {
        for base in (0..n).step_by(4) {
            if base + 2 < n {
                dirs[base + 2] *= -1.0;
            }
            if base + 3 < n {
                dirs[base + 3] *= -1.0;
            }
        }
    }
    if flags.alternate {
        for base in (0..n).step_by(4) {
            if base + 1 < n {
                dirs[base + 1] *= -1.0;
            }
            if base + 3 < n {
                dirs[base + 3] *= -1.0;
            }
        }
    }
    if flags.cross {
        for base in (0..n).step_by(4) {
            if base + 1 < n {
                dirs[base + 1] *= -1.0;
            }
            if base + 2 < n {
                dirs[base + 2] *= -1.0;
            }
        }
    }
    dirs
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GameplayConfig {
    pub mine_hit_sound: bool,
    pub default_fail_type: GameplayFailType,
    pub global_offset_seconds: f32,
    pub visual_delay_seconds: f32,
    pub machine_pack_ini_offsets: bool,
    pub machine_default_sync_pref: SyncPref,
    pub machine_allow_per_player_global_offsets: bool,
    pub machine_enable_replays: bool,
    pub center_1player_notefield: bool,
    pub delayed_back: bool,
}

impl Default for GameplayConfig {
    fn default() -> Self {
        Self {
            mine_hit_sound: true,
            default_fail_type: GameplayFailType::ImmediateContinue,
            global_offset_seconds: -0.008,
            visual_delay_seconds: 0.0,
            machine_pack_ini_offsets: false,
            machine_default_sync_pref: SyncPref::Null,
            machine_allow_per_player_global_offsets: false,
            machine_enable_replays: true,
            center_1player_notefield: false,
            delayed_back: true,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum GameplayMiniIndicatorMode {
    #[default]
    None,
    SubtractiveScoring,
    PredictiveScoring,
    PaceScoring,
    RivalScoring,
    Pacemaker,
    StreamProg,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GameplayMiniIndicatorOptions {
    pub requested_mode: GameplayMiniIndicatorMode,
    pub measure_counter_enabled: bool,
    pub subtractive_scoring: bool,
    pub pacemaker: bool,
}

#[inline(always)]
pub const fn mini_indicator_mode_for_options(
    options: GameplayMiniIndicatorOptions,
) -> GameplayMiniIndicatorMode {
    match options.requested_mode {
        GameplayMiniIndicatorMode::None if options.subtractive_scoring => {
            GameplayMiniIndicatorMode::SubtractiveScoring
        }
        GameplayMiniIndicatorMode::None if options.pacemaker => {
            GameplayMiniIndicatorMode::Pacemaker
        }
        mode => mode,
    }
}

#[inline(always)]
pub const fn mini_indicator_needs_stream_data(options: GameplayMiniIndicatorOptions) -> bool {
    options.measure_counter_enabled
        || !matches!(
            mini_indicator_mode_for_options(options),
            GameplayMiniIndicatorMode::None
        )
}

#[derive(Clone, Debug)]
pub struct GameplayMiniIndicatorData {
    pub personal_best_percent: [Option<f64>; MAX_PLAYERS],
    pub machine_best_percent: [Option<f64>; MAX_PLAYERS],
}

impl Default for GameplayMiniIndicatorData {
    fn default() -> Self {
        Self {
            personal_best_percent: [None; MAX_PLAYERS],
            machine_best_percent: [None; MAX_PLAYERS],
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct LeadInTiming {
    pub min_seconds_to_step: f32,
    pub min_seconds_to_music: f32,
}

impl Default for LeadInTiming {
    fn default() -> Self {
        Self {
            min_seconds_to_step: MIN_SECONDS_TO_STEP,
            min_seconds_to_music: MIN_SECONDS_TO_MUSIC,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct GameplayStreamClockSnapshot {
    pub stream_seconds: f32,
    pub music_nanos: SongTimeNs,
    pub music_seconds_per_second: f32,
    pub has_music_mapping: bool,
    pub valid_at: Instant,
    pub valid_at_host_nanos: u64,
}

impl Default for GameplayStreamClockSnapshot {
    fn default() -> Self {
        Self {
            stream_seconds: 0.0,
            music_nanos: 0,
            music_seconds_per_second: 1.0,
            has_music_mapping: false,
            valid_at: Instant::now(),
            valid_at_host_nanos: 0,
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct GameplayAudioSnapshot {
    pub stream_clock: GameplayStreamClockSnapshot,
    pub assist_sfx_generation: u64,
    pub output_delay_seconds: f32,
    pub timing_diag_enabled: bool,
    pub timing_diag_callback_gap_ns: u64,
}

#[derive(Clone, Copy, Debug)]
pub struct SongClockSnapshot {
    pub song_time_ns: SongTimeNs,
    pub seconds_per_second: f32,
    pub mapped_audio: bool,
    pub valid_at: Instant,
    pub valid_at_host_nanos: u64,
    pub timing_diag_enabled: bool,
    pub timing_diag_callback_gap_ns: u64,
}

#[inline(always)]
pub fn current_song_clock_snapshot(
    audio_snapshot: GameplayAudioSnapshot,
    music_rate: f32,
    audio_lead_in_seconds: f32,
    global_offset_seconds: f32,
) -> SongClockSnapshot {
    let stream_clock = audio_snapshot.stream_clock;
    let fallback_rate = normalized_song_rate(music_rate);
    if stream_clock.has_music_mapping {
        return SongClockSnapshot {
            song_time_ns: stream_clock.music_nanos,
            seconds_per_second: if stream_clock.music_seconds_per_second.is_finite()
                && stream_clock.music_seconds_per_second > 0.0
            {
                stream_clock.music_seconds_per_second
            } else {
                fallback_rate
            },
            mapped_audio: true,
            valid_at: stream_clock.valid_at,
            valid_at_host_nanos: stream_clock.valid_at_host_nanos,
            timing_diag_enabled: audio_snapshot.timing_diag_enabled,
            timing_diag_callback_gap_ns: audio_snapshot.timing_diag_callback_gap_ns,
        };
    }

    let song_time = music_time_from_stream_position(
        stream_clock.stream_seconds,
        audio_lead_in_seconds,
        global_offset_seconds,
        fallback_rate,
    );
    SongClockSnapshot {
        song_time_ns: song_time_ns_from_seconds(song_time),
        seconds_per_second: fallback_rate,
        mapped_audio: false,
        valid_at: stream_clock.valid_at,
        valid_at_host_nanos: stream_clock.valid_at_host_nanos,
        timing_diag_enabled: audio_snapshot.timing_diag_enabled,
        timing_diag_callback_gap_ns: audio_snapshot.timing_diag_callback_gap_ns,
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GameplayMusicCut {
    pub start_sec: f64,
    pub length_sec: f64,
    pub fade_in_sec: f64,
    pub fade_out_sec: f64,
}

impl Default for GameplayMusicCut {
    fn default() -> Self {
        Self {
            start_sec: 0.0,
            length_sec: f64::INFINITY,
            fade_in_sec: 0.0,
            fade_out_sec: 0.0,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum GameplayAudioCommand {
    StopMusic,
    PlayMusic {
        path: PathBuf,
        cut: GameplayMusicCut,
        looping: bool,
        rate: f32,
    },
    PlayPreloadedSfx(&'static str),
    PlayPreloadedAssistTick(&'static str),
    PlayAssistTickAtMusicTime {
        path: &'static str,
        music_seconds: f64,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GameplaySessionCommand {
    SetTimingTickMode(GameplayTimingTickMode),
}

#[derive(Clone, Copy, Debug)]
pub struct ColumnCueColumn {
    pub column: usize,
    pub is_mine: bool,
}

#[derive(Clone, Debug)]
pub struct ColumnCue {
    pub start_time: f32,
    pub duration: f32,
    pub columns: Vec<ColumnCueColumn>,
}

// Lead-in/out fade applied to every crossover cue.
pub const CROSSOVER_CUE_FADE_SECONDS: f32 = 0.075;

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct CrossoverRow {
    pub beat: f32,
    // Occupancy bitmask of the foot-bearing columns for this row.
    pub column_mask: u8,
    // Whether the parity solver flagged this row as a crossover.
    pub crossover: bool,
    // Kept raw so the cue builder can honor the per-player bracket toggle.
    pub bracket: bool,
}

impl CrossoverRow {
    // A bracket crossover only counts when the player opts brackets in.
    #[inline]
    pub const fn is_active_crossover(&self, include_brackets: bool) -> bool {
        self.crossover && (include_brackets || !self.bracket)
    }
}

pub fn build_crossover_rows<const LANES: usize>(
    notes: &[Note],
    note_range: (usize, usize),
    col_start: usize,
) -> (Vec<[u8; LANES]>, Vec<f32>) {
    use std::collections::BTreeMap;

    let (start, end) = note_range;
    let mut rows: BTreeMap<usize, ([u8; LANES], f32)> = BTreeMap::new();
    for note in &notes[start..end] {
        if note.column < col_start || note.column - col_start >= LANES {
            continue;
        }
        let lane = note.column - col_start;
        let ch = if note.is_fake {
            match note.note_type {
                NoteType::Mine => b'M',
                _ => continue,
            }
        } else {
            match note.note_type {
                NoteType::Tap => b'1',
                NoteType::Lift => b'L',
                NoteType::Hold => b'2',
                NoteType::Roll => b'4',
                NoteType::Mine => b'M',
                NoteType::Fake => continue,
            }
        };
        let entry = rows
            .entry(note.row_index)
            .or_insert(([b'0'; LANES], note.beat));
        if ch == b'M' {
            if entry.0[lane] == b'0' {
                entry.0[lane] = b'M';
            }
        } else {
            entry.0[lane] = ch;
        }
        if let Some(hold) = note.hold.as_ref() {
            let tail = rows
                .entry(hold.end_row_index)
                .or_insert(([b'0'; LANES], hold.end_beat));
            if tail.0[lane] == b'0' {
                tail.0[lane] = b'3';
            }
        }
    }
    let mut row_arrays = Vec::with_capacity(rows.len());
    let mut row_to_beat = Vec::with_capacity(rows.len());
    for (_row_index, (arr, beat)) in rows {
        row_arrays.push(arr);
        row_to_beat.push(beat);
    }
    (row_arrays, row_to_beat)
}

// Lowest matching lane wins so results are deterministic. `pos % 4` keeps this
// working for the second pad of doubles, not just the left pad.
pub fn crossover_arrow_col(column_mask: u8, want_outer: bool) -> Option<usize> {
    let mut m = column_mask;
    while m != 0 {
        let c = m.trailing_zeros() as usize;
        m &= m - 1;
        let pos = c % 4;
        let is_outer = pos == 0 || pos == 3;
        if is_outer == want_outer {
            return Some(c);
        }
    }
    None
}

#[allow(clippy::too_many_arguments)]
pub fn build_crossover_cues_from_annotations(
    annos: &[CrossoverRow],
    timing_player: &TimingData,
    col_start: usize,
    duration_ms: u16,
    quantization: u8,
    include_brackets: bool,
    first_visible_time: f32,
) -> Vec<ColumnCue> {
    let arrow_time =
        |beat: f32| -> f32 { song_time_ns_to_seconds(timing_player.get_time_for_beat_ns(beat)) };
    build_crossover_cues_core(
        annos,
        arrow_time,
        col_start,
        duration_ms,
        quantization,
        include_brackets,
        first_visible_time,
    )
}

// Split from the TimingData entry so tests can use a compact beat-to-seconds
// mapping without constructing full timing data.
#[allow(clippy::too_many_arguments)]
fn build_crossover_cues_core(
    annos: &[CrossoverRow],
    arrow_time: impl Fn(f32) -> f32,
    col_start: usize,
    duration_ms: u16,
    quantization: u8,
    include_brackets: bool,
    first_visible_time: f32,
) -> Vec<ColumnCue> {
    if annos.len() < 2 {
        return Vec::new();
    }
    let duration = f32::from(duration_ms) / 1000.0;
    let fade = CROSSOVER_CUE_FADE_SECONDS;
    let quant = if quantization == 0 {
        1.0
    } else {
        f32::from(quantization)
    };
    let spacing_threshold = 4.0 / quant + 0.001;

    let mut cues: Vec<ColumnCue> = Vec::new();
    for i in 1..annos.len() {
        let current = &annos[i];
        let prev = &annos[i - 1];
        if !current.is_active_crossover(include_brackets)
            || prev.is_active_crossover(include_brackets)
        {
            continue;
        }
        let next = annos.get(i + 1);
        let next_next = annos.get(i + 2);
        let is_scooby = next.is_some_and(|a| a.is_active_crossover(include_brackets));
        let first_condition = current.beat - prev.beat <= spacing_threshold;
        let second_condition = next.is_some_and(|n| n.beat - current.beat <= spacing_threshold);
        let third_condition = is_scooby
            && match (next, next_next) {
                (Some(n), Some(nn)) => nn.beat - n.beat <= spacing_threshold,
                _ => false,
            };
        if !(first_condition || second_condition || third_condition) {
            continue;
        }
        let (Some(prev_col), Some(curr_col)) = (
            crossover_arrow_col(prev.column_mask, false),
            crossover_arrow_col(current.column_mask, true),
        ) else {
            continue;
        };
        let prev_arrow_time = arrow_time(prev.beat);
        let cur_arrow_time = arrow_time(current.beat);
        let mut columns = vec![
            ColumnCueColumn {
                column: col_start + curr_col,
                is_mine: false,
            },
            ColumnCueColumn {
                column: col_start + prev_col,
                is_mine: false,
            },
        ];
        let mut start_time = prev_arrow_time - duration;
        let mut cue_duration = duration + fade;
        if !first_condition {
            cue_duration += cur_arrow_time - prev_arrow_time;
        }
        if is_scooby
            && let Some(next_anno) = next
            && let Some(next_col) = crossover_arrow_col(next_anno.column_mask, true)
        {
            columns.push(ColumnCueColumn {
                column: col_start + next_col,
                is_mine: true,
            });
        }
        if let Some(last) = cues.last() {
            let prev_end = last.start_time + last.duration;
            if start_time < prev_end {
                let duration_difference = prev_end - start_time;
                start_time = prev_end - fade;
                cue_duration = cue_duration - duration_difference + fade;
            }
        }
        cues.push(ColumnCue {
            start_time,
            duration: cue_duration,
            columns,
        });
    }

    if first_visible_time < 0.0
        && let Some(first) = cues.first_mut()
        && first.start_time <= 0.0
    {
        first.duration -= first_visible_time;
        first.start_time += first_visible_time;
    }
    cues
}

#[derive(Clone, Debug)]
pub struct JudgmentRenderInfo {
    pub judgment: Judgment,
    pub started_at_screen_s: f32,
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub struct MineJudgmentRenderInfo {
    pub result: MineResult,
    pub column: usize,
    pub started_at_screen_s: f32,
}

#[derive(Copy, Clone, Debug)]
pub struct HoldJudgmentRenderInfo {
    pub result: HoldResult,
    pub started_at_screen_s: f32,
}

#[derive(Copy, Clone, Debug)]
pub struct HeldMissRenderInfo {
    pub started_at_screen_s: f32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FinalNoteResultEffects {
    pub mark_row_finalized: bool,
    pub trigger_miss_flash_column: Option<usize>,
    pub held_miss_column: Option<usize>,
}

#[inline(always)]
pub const fn final_note_result_effects(
    was_unjudged: bool,
    judgment: &Judgment,
    column: usize,
    column_count: usize,
) -> FinalNoteResultEffects {
    if !was_unjudged {
        return FinalNoteResultEffects {
            mark_row_finalized: false,
            trigger_miss_flash_column: None,
            held_miss_column: None,
        };
    }
    let is_miss = matches!(judgment.grade, JudgeGrade::Miss);
    FinalNoteResultEffects {
        mark_row_finalized: true,
        trigger_miss_flash_column: if is_miss { Some(column) } else { None },
        held_miss_column: if is_miss && judgment.miss_because_held && column < column_count {
            Some(column)
        } else {
            None
        },
    }
}

pub fn register_provisional_early_note_result(note: &mut Note, judgment: Judgment) -> bool {
    if note.early_result.is_some() {
        return false;
    }
    note.early_result = Some(judgment);
    true
}

pub fn apply_final_note_result(
    note: &mut Note,
    judgment: Judgment,
    column_count: usize,
) -> FinalNoteResultEffects {
    let effects =
        final_note_result_effects(note.result.is_none(), &judgment, note.column, column_count);
    note.result = Some(judgment);
    effects
}

pub const HOLD_JUDGMENT_TOTAL_DURATION: f32 = 0.8;
pub const HELD_MISS_TOTAL_DURATION: f32 = 0.5;
pub const RECEPTOR_GLOW_DURATION: f32 = 0.2;
pub const COLUMN_FLASH_MISS_DURATION: f32 = 0.16;
pub const COLUMN_FLASH_JUDGMENT_DURATION: f32 = 0.33;
pub const COMBO_HUNDRED_MILESTONE_DURATION: f32 = 0.6;
pub const COMBO_THOUSAND_MILESTONE_DURATION: f32 = 0.7;

#[inline(always)]
pub const fn column_flash_duration(grade: JudgeGrade) -> f32 {
    match grade {
        JudgeGrade::Miss => COLUMN_FLASH_MISS_DURATION,
        JudgeGrade::Fantastic
        | JudgeGrade::Excellent
        | JudgeGrade::Great
        | JudgeGrade::Decent
        | JudgeGrade::WayOff => COLUMN_FLASH_JUDGMENT_DURATION,
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ColumnFlashOptions {
    pub enabled: bool,
    pub blue_fantastic: bool,
    pub white_fantastic: bool,
    pub excellent: bool,
    pub great: bool,
    pub decent: bool,
    pub way_off: bool,
    pub miss: bool,
}

#[inline(always)]
pub const fn column_flash_enabled_for_options(
    options: ColumnFlashOptions,
    grade: JudgeGrade,
    blue_fantastic: bool,
) -> bool {
    if !options.enabled {
        return false;
    }
    match grade {
        JudgeGrade::Fantastic => {
            if blue_fantastic {
                options.blue_fantastic
            } else {
                options.white_fantastic
            }
        }
        JudgeGrade::Excellent => options.excellent,
        JudgeGrade::Great => options.great,
        JudgeGrade::Decent => options.decent,
        JudgeGrade::WayOff => options.way_off,
        JudgeGrade::Miss => options.miss,
    }
}

#[derive(Copy, Clone, Debug)]
pub struct ActiveTapExplosion {
    pub window: &'static str,
    pub bright: bool,
    pub elapsed: f32,
    pub duration: f32,
    pub start_beat: f32,
}

#[derive(Copy, Clone, Debug)]
pub struct ActiveColumnFlash {
    pub grade: JudgeGrade,
    pub blue_fantastic: bool,
    pub started_at_screen_s: f32,
}

#[derive(Copy, Clone, Debug)]
pub struct ColumnTapJudgment {
    pub grade: JudgeGrade,
    pub blue_fantastic: bool,
    pub at_screen_s: f32,
}

#[derive(Clone, Debug)]
pub struct ActiveMineExplosion {
    pub elapsed: f32,
    pub duration: f32,
    pub started_at_screen_s: f32,
}

#[inline(always)]
pub fn tick_tap_explosion_slot(slot: &mut Option<ActiveTapExplosion>, delta_time: f32) {
    if let Some(active) = slot {
        active.elapsed += delta_time;
        if active.duration <= 0.0 || active.elapsed >= active.duration {
            *slot = None;
        }
    }
}

#[inline(always)]
pub fn tick_mine_explosion_slot(slot: &mut Option<ActiveMineExplosion>, delta_time: f32) {
    if let Some(active) = slot {
        active.elapsed += delta_time;
        if active.duration <= 0.0 || active.elapsed >= active.duration {
            *slot = None;
        }
    }
}

#[inline(always)]
pub fn column_flash_expired_at(flash: ActiveColumnFlash, screen_time_s: f32) -> bool {
    screen_time_s - flash.started_at_screen_s >= column_flash_duration(flash.grade)
}

#[inline(always)]
pub fn hold_judgment_expired_at(render_info: HoldJudgmentRenderInfo, screen_time_s: f32) -> bool {
    screen_time_s - render_info.started_at_screen_s >= HOLD_JUDGMENT_TOTAL_DURATION
}

#[inline(always)]
pub fn held_miss_judgment_expired_at(render_info: HeldMissRenderInfo, screen_time_s: f32) -> bool {
    screen_time_s - render_info.started_at_screen_s >= HELD_MISS_TOTAL_DURATION
}

pub const MINE_EXPLOSION_DURATION: f32 = 0.6;
pub const RECEPTOR_STEP_WINDOW_COUNT: usize = 7;
pub const RECEPTOR_STEP_WINDOWS: [Option<&str>; RECEPTOR_STEP_WINDOW_COUNT] = [
    None,
    Some("W1"),
    Some("W2"),
    Some("W3"),
    Some("W4"),
    Some("W5"),
    Some("Miss"),
];
pub const TAP_EXPLOSION_WINDOW_COUNT: usize = 7;
pub const TAP_EXPLOSION_WINDOWS: [&str; TAP_EXPLOSION_WINDOW_COUNT] =
    ["W1", "W2", "W3", "W4", "W5", "Miss", "Held"];

#[inline(always)]
pub const fn grade_to_window(grade: JudgeGrade) -> Option<&'static str> {
    match grade {
        JudgeGrade::Fantastic => Some("W1"),
        JudgeGrade::Excellent => Some("W2"),
        JudgeGrade::Great => Some("W3"),
        JudgeGrade::Decent => Some("W4"),
        JudgeGrade::WayOff => Some("W5"),
        JudgeGrade::Miss => Some("Miss"),
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FantasticFeedbackOptions {
    pub show_fa_plus_window: bool,
    pub fa_plus_10ms_blue_window: bool,
    pub split_15_10ms: bool,
    pub custom_fantastic_window: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FantasticWindowOptions {
    pub base_fa_plus_s: f32,
    pub custom_fantastic_window_s: Option<f32>,
    pub fa_plus_10ms_blue_window: bool,
}

#[inline(always)]
pub fn fantastic_window_seconds(options: FantasticWindowOptions) -> f32 {
    options
        .custom_fantastic_window_s
        .unwrap_or(options.base_fa_plus_s)
}

#[inline(always)]
pub fn blue_fantastic_window_ms(options: FantasticWindowOptions) -> f32 {
    if let Some(custom_s) = options.custom_fantastic_window_s {
        return custom_s * 1000.0;
    }
    if options.fa_plus_10ms_blue_window {
        return 10.0;
    }
    options.base_fa_plus_s * 1000.0
}

#[derive(Clone, Copy, Debug)]
pub struct PlayerJudgmentTiming {
    pub profile_music_ns: TimingProfileNs,
    pub disabled_windows: [bool; 5],
    pub largest_tap_window_music_ns: SongTimeNs,
}

impl Default for PlayerJudgmentTiming {
    fn default() -> Self {
        Self {
            profile_music_ns: TimingProfileNs::default(),
            disabled_windows: [false; 5],
            largest_tap_window_music_ns: 0,
        }
    }
}

#[inline(always)]
pub fn build_player_judgment_timing_for_options(
    mut timing_profile: TimingProfile,
    fantastic_options: FantasticWindowOptions,
    disabled_windows: [bool; 5],
    music_rate: f32,
) -> PlayerJudgmentTiming {
    timing_profile.fa_plus_window_s = Some(fantastic_window_seconds(fantastic_options));
    let profile_music_ns = TimingProfileNs::from_profile_scaled(&timing_profile, music_rate);
    let largest_tap_window_music_ns =
        largest_enabled_tap_window_ns(&profile_music_ns, &disabled_windows)
            .unwrap_or(profile_music_ns.windows_ns[2]);

    PlayerJudgmentTiming {
        profile_music_ns,
        disabled_windows,
        largest_tap_window_music_ns,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NoteHitEval {
    pub note_time_ns: SongTimeNs,
    pub measured_offset_music_ns: SongTimeNs,
    pub grade: JudgeGrade,
    pub window: TimingWindow,
}

#[inline(always)]
pub fn note_hit_eval_for_timing(
    timing: PlayerJudgmentTiming,
    note_time_ns: SongTimeNs,
    current_time_ns: SongTimeNs,
) -> Option<NoteHitEval> {
    let measured_offset_music_ns = current_time_ns.saturating_sub(note_time_ns);
    if i128::from(measured_offset_music_ns).abs() > i128::from(timing.largest_tap_window_music_ns) {
        return None;
    }
    let (grade, window) = classify_offset_ns_with_disabled_windows(
        measured_offset_music_ns,
        &timing.profile_music_ns,
        &timing.disabled_windows,
    )?;
    Some(NoteHitEval {
        note_time_ns,
        measured_offset_music_ns,
        grade,
        window,
    })
}

#[inline(always)]
pub fn tap_judgment_uses_bright_explosion_for_options(
    options: FantasticFeedbackOptions,
    judgment: &Judgment,
) -> bool {
    if !options.show_fa_plus_window || judgment.grade != JudgeGrade::Fantastic {
        return false;
    }
    if options.fa_plus_10ms_blue_window
        && !options.split_15_10ms
        && !options.custom_fantastic_window
    {
        return judgment.time_error_ms.abs() > FA_PLUS_W010_MS;
    }
    judgment.window == Some(TimingWindow::W1)
}

#[derive(Debug, Clone, Copy)]
pub enum GameplayTween {
    Linear,
    Accelerate,
    Decelerate,
}

impl GameplayTween {
    #[inline(always)]
    pub fn ease(self, progress: f32) -> f32 {
        let t = progress.clamp(0.0, 1.0);
        match self {
            Self::Linear => t,
            Self::Accelerate => t * t,
            Self::Decelerate => 1.0 - (1.0 - t) * (1.0 - t),
        }
    }
}

#[inline(always)]
fn song_lua_pow_in(t: f32, power: f32) -> f32 {
    t.powf(power)
}

#[inline(always)]
fn song_lua_pow_out(t: f32, power: f32) -> f32 {
    1.0 - (1.0 - t).powf(power)
}

#[inline(always)]
fn song_lua_pow_in_out(t: f32, power: f32) -> f32 {
    if t < 0.5 {
        0.5 * (2.0 * t).powf(power)
    } else {
        1.0 - 0.5 * (2.0 * (1.0 - t)).powf(power)
    }
}

#[inline(always)]
fn song_lua_pow_out_in(t: f32, power: f32) -> f32 {
    if t < 0.5 {
        0.5 * song_lua_pow_out(t * 2.0, power)
    } else {
        0.5 + 0.5 * song_lua_pow_in((t * 2.0) - 1.0, power)
    }
}

fn song_lua_out_bounce(t: f32) -> f32 {
    const N1: f32 = 7.5625;
    const D1: f32 = 2.75;
    if t < 1.0 / D1 {
        N1 * t * t
    } else if t < 2.0 / D1 {
        let t = t - 1.5 / D1;
        N1 * t * t + 0.75
    } else if t < 2.5 / D1 {
        let t = t - 2.25 / D1;
        N1 * t * t + 0.9375
    } else {
        let t = t - 2.625 / D1;
        N1 * t * t + 0.984_375
    }
}

#[inline(always)]
fn song_lua_in_bounce(t: f32) -> f32 {
    1.0 - song_lua_out_bounce(1.0 - t)
}

#[inline(always)]
fn song_lua_in_out_bounce(t: f32) -> f32 {
    if t < 0.5 {
        0.5 * song_lua_in_bounce(t * 2.0)
    } else {
        0.5 + 0.5 * song_lua_out_bounce((t * 2.0) - 1.0)
    }
}

pub fn song_lua_ease_factor(
    easing: Option<&str>,
    t: f32,
    opt1: Option<f32>,
    opt2: Option<f32>,
) -> f32 {
    let t = t.clamp(0.0, 1.0);
    let overshoot = opt1.filter(|v| v.is_finite()).unwrap_or(1.70158);
    let elastic_period = opt1.filter(|v| v.is_finite() && *v > 0.0).unwrap_or(0.3);
    let elastic_tau = std::f32::consts::TAU / elastic_period;
    match easing.unwrap_or("linear") {
        "instant" => 1.0,
        "linear" => t,
        "inQuad" => song_lua_pow_in(t, 2.0),
        "outQuad" => song_lua_pow_out(t, 2.0),
        "inOutQuad" => song_lua_pow_in_out(t, 2.0),
        "outInQuad" => song_lua_pow_out_in(t, 2.0),
        "inCubic" => song_lua_pow_in(t, 3.0),
        "outCubic" => song_lua_pow_out(t, 3.0),
        "inOutCubic" => song_lua_pow_in_out(t, 3.0),
        "outInCubic" => song_lua_pow_out_in(t, 3.0),
        "inQuart" => song_lua_pow_in(t, 4.0),
        "outQuart" => song_lua_pow_out(t, 4.0),
        "inOutQuart" => song_lua_pow_in_out(t, 4.0),
        "outInQuart" => song_lua_pow_out_in(t, 4.0),
        "inQuint" => song_lua_pow_in(t, 5.0),
        "outQuint" => song_lua_pow_out(t, 5.0),
        "inOutQuint" => song_lua_pow_in_out(t, 5.0),
        "outInQuint" => song_lua_pow_out_in(t, 5.0),
        "inSine" => 1.0 - (t * std::f32::consts::FRAC_PI_2).cos(),
        "outSine" => (t * std::f32::consts::FRAC_PI_2).sin(),
        "inOutSine" => -((std::f32::consts::PI * t).cos() - 1.0) * 0.5,
        "outInSine" => {
            if t < 0.5 {
                0.5 * ((t * std::f32::consts::PI).sin())
            } else {
                0.5 + 0.5 * (1.0 - (((t * 2.0) - 1.0) * std::f32::consts::FRAC_PI_2).cos())
            }
        }
        "inExpo" => {
            if t <= 0.0 {
                0.0
            } else {
                2.0_f32.powf((10.0 * t) - 10.0)
            }
        }
        "outExpo" => {
            if t >= 1.0 {
                1.0
            } else {
                1.0 - 2.0_f32.powf(-10.0 * t)
            }
        }
        "inOutExpo" => {
            if t <= 0.0 {
                0.0
            } else if t >= 1.0 {
                1.0
            } else if t < 0.5 {
                0.5 * 2.0_f32.powf((20.0 * t) - 10.0)
            } else {
                1.0 - (0.5 * 2.0_f32.powf((-20.0 * t) + 10.0))
            }
        }
        "outInExpo" => {
            if t < 0.5 {
                0.5 * (1.0 - 2.0_f32.powf(-20.0 * t))
            } else if t >= 1.0 {
                1.0
            } else {
                0.5 + 0.5 * 2.0_f32.powf((20.0 * t) - 20.0)
            }
        }
        "inCirc" => 1.0 - (1.0 - (t * t)).sqrt(),
        "outCirc" => (1.0 - ((t - 1.0) * (t - 1.0))).sqrt(),
        "inOutCirc" => {
            if t < 0.5 {
                0.5 * (1.0 - (1.0 - 4.0 * t * t).sqrt())
            } else {
                0.5 * ((1.0 - ((-2.0 * t + 2.0) * (-2.0 * t + 2.0))).sqrt() + 1.0)
            }
        }
        "outInCirc" => {
            if t < 0.5 {
                0.5 * (1.0 - ((2.0 * t - 1.0) * (2.0 * t - 1.0))).sqrt()
            } else {
                0.5 + 0.5 * (1.0 - (1.0 - ((2.0 * t - 1.0) * (2.0 * t - 1.0))).sqrt())
            }
        }
        "inElastic" => {
            if t <= 0.0 {
                0.0
            } else if t >= 1.0 {
                1.0
            } else {
                let u = t - 1.0;
                -(2.0_f32.powf(10.0 * u)) * ((u - elastic_period * 0.25) * elastic_tau).sin()
            }
        }
        "outElastic" => {
            if t <= 0.0 {
                0.0
            } else if t >= 1.0 {
                1.0
            } else {
                2.0_f32.powf(-10.0 * t) * ((t - elastic_period * 0.25) * elastic_tau).sin() + 1.0
            }
        }
        "inOutElastic" => {
            if t <= 0.0 {
                0.0
            } else if t >= 1.0 {
                1.0
            } else if t < 0.5 {
                let u = (2.0 * t) - 1.0;
                -0.5 * 2.0_f32.powf(10.0 * u) * ((u - elastic_period * 0.375) * elastic_tau).sin()
            } else {
                let u = (2.0 * t) - 1.0;
                0.5 * 2.0_f32.powf(-10.0 * u) * ((u - elastic_period * 0.375) * elastic_tau).sin()
                    + 1.0
            }
        }
        "outInElastic" => {
            if t < 0.5 {
                0.5 * song_lua_ease_factor(Some("outElastic"), t * 2.0, opt1, opt2)
            } else {
                0.5 + 0.5 * song_lua_ease_factor(Some("inElastic"), (t * 2.0) - 1.0, opt1, opt2)
            }
        }
        "inBack" => t * t * (((overshoot + 1.0) * t) - overshoot),
        "outBack" => {
            let u = t - 1.0;
            (u * u * (((overshoot + 1.0) * u) + overshoot)) + 1.0
        }
        "inOutBack" => {
            let s = overshoot * 1.525;
            if t < 0.5 {
                let u = 2.0 * t;
                0.5 * (u * u * (((s + 1.0) * u) - s))
            } else {
                let u = (2.0 * t) - 2.0;
                0.5 * (u * u * (((s + 1.0) * u) + s) + 2.0)
            }
        }
        "outInBack" => {
            if t < 0.5 {
                0.5 * song_lua_ease_factor(Some("outBack"), t * 2.0, opt1, opt2)
            } else {
                0.5 + 0.5 * song_lua_ease_factor(Some("inBack"), (t * 2.0) - 1.0, opt1, opt2)
            }
        }
        "inBounce" => song_lua_in_bounce(t),
        "outBounce" => song_lua_out_bounce(t),
        "inOutBounce" => song_lua_in_out_bounce(t),
        "outInBounce" => {
            if t < 0.5 {
                0.5 * song_lua_out_bounce(t * 2.0)
            } else {
                0.5 + 0.5 * song_lua_in_bounce((t * 2.0) - 1.0)
            }
        }
        _ => t,
    }
}

#[derive(Debug, Clone, Copy)]
pub struct GameplayReceptorGlowBehavior {
    pub press_duration: f32,
    pub press_alpha_start: f32,
    pub press_alpha_end: f32,
    pub press_zoom_start: f32,
    pub press_zoom_end: f32,
    pub press_tween: GameplayTween,
    pub duration: f32,
    pub alpha_start: f32,
    pub alpha_end: f32,
    pub zoom_start: f32,
    pub zoom_end: f32,
    pub tween: GameplayTween,
    pub blend_add: bool,
}

impl GameplayReceptorGlowBehavior {
    #[inline(always)]
    pub fn sample_press(self, timer_remaining: f32) -> (f32, f32) {
        let duration = self.press_duration.max(0.0);
        if duration <= f32::EPSILON {
            return (
                self.press_alpha_end.clamp(0.0, 1.0),
                self.press_zoom_end.max(0.0),
            );
        }
        let elapsed = (duration - timer_remaining.clamp(0.0, duration)).clamp(0.0, duration);
        let progress = elapsed / duration;
        let eased = self.press_tween.ease(progress);
        let alpha =
            (self.press_alpha_end - self.press_alpha_start).mul_add(eased, self.press_alpha_start);
        let zoom =
            (self.press_zoom_end - self.press_zoom_start).mul_add(eased, self.press_zoom_start);
        (alpha.clamp(0.0, 1.0), zoom.max(0.0))
    }

    #[inline(always)]
    pub fn sample_lift(
        self,
        timer_remaining: f32,
        start_alpha: f32,
        start_zoom: f32,
    ) -> (f32, f32) {
        let duration = self.duration.max(0.0);
        if duration <= f32::EPSILON {
            return (self.alpha_end.clamp(0.0, 1.0), self.zoom_end.max(0.0));
        }
        let elapsed = (duration - timer_remaining.clamp(0.0, duration)).clamp(0.0, duration);
        let progress = elapsed / duration;
        let eased = self.tween.ease(progress);
        let alpha = (self.alpha_end - start_alpha).mul_add(eased, start_alpha);
        let zoom = (self.zoom_end - start_zoom).mul_add(eased, start_zoom);
        (alpha.clamp(0.0, 1.0), zoom.max(0.0))
    }
}

impl Default for GameplayReceptorGlowBehavior {
    fn default() -> Self {
        Self {
            press_duration: 0.0,
            press_alpha_start: 1.0,
            press_alpha_end: 1.0,
            press_zoom_start: 1.0,
            press_zoom_end: 1.0,
            press_tween: GameplayTween::Linear,
            duration: 0.2,
            alpha_start: 1.0,
            alpha_end: 0.0,
            zoom_start: 1.0,
            zoom_end: 1.0,
            tween: GameplayTween::Decelerate,
            blend_add: true,
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct GameplayReceptorGlowState {
    pub press_timer: f32,
    pub lift_timer: f32,
    pub lift_start_alpha: f32,
    pub lift_start_zoom: f32,
    pub lane_pressed: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct GameplayReceptorGlowTimers {
    pub press_timer: f32,
    pub lift_timer: f32,
    pub lift_start_alpha: f32,
    pub lift_start_zoom: f32,
}

#[inline(always)]
pub fn receptor_glow_duration(behavior: GameplayReceptorGlowBehavior) -> f32 {
    Some(behavior.duration)
        .filter(|duration| *duration > f32::EPSILON)
        .unwrap_or(RECEPTOR_GLOW_DURATION)
}

#[inline(always)]
pub fn receptor_glow_visual(
    behavior: GameplayReceptorGlowBehavior,
    state: GameplayReceptorGlowState,
) -> Option<(f32, f32)> {
    if state.press_timer > f32::EPSILON && behavior.press_duration > f32::EPSILON {
        return Some(behavior.sample_press(state.press_timer));
    }
    if state.lane_pressed {
        return Some((behavior.press_alpha_end, behavior.press_zoom_end));
    }
    if state.lift_timer > f32::EPSILON {
        return Some(behavior.sample_lift(
            state.lift_timer,
            state.lift_start_alpha,
            state.lift_start_zoom,
        ));
    }
    None
}

#[inline(always)]
pub fn receptor_glow_pulse_timers(
    behavior: GameplayReceptorGlowBehavior,
) -> GameplayReceptorGlowTimers {
    GameplayReceptorGlowTimers {
        press_timer: 0.0,
        lift_timer: receptor_glow_duration(behavior),
        lift_start_alpha: behavior.press_alpha_start,
        lift_start_zoom: behavior.press_zoom_start,
    }
}

#[inline(always)]
pub fn receptor_glow_press_timers(
    behavior: GameplayReceptorGlowBehavior,
) -> GameplayReceptorGlowTimers {
    GameplayReceptorGlowTimers {
        press_timer: behavior.press_duration,
        lift_timer: 0.0,
        lift_start_alpha: behavior.press_alpha_end,
        lift_start_zoom: behavior.press_zoom_end,
    }
}

#[inline(always)]
pub fn receptor_glow_lift_start(
    behavior: GameplayReceptorGlowBehavior,
    press_timer: f32,
) -> (f32, f32) {
    if press_timer > f32::EPSILON && behavior.press_duration > f32::EPSILON {
        behavior.sample_press(press_timer)
    } else {
        (behavior.press_alpha_end, behavior.press_zoom_end)
    }
}

#[inline(always)]
pub fn receptor_glow_release_timers(
    behavior: GameplayReceptorGlowBehavior,
    press_timer: f32,
) -> GameplayReceptorGlowTimers {
    let (alpha, zoom) = receptor_glow_lift_start(behavior, press_timer);
    GameplayReceptorGlowTimers {
        press_timer: 0.0,
        lift_timer: receptor_glow_duration(behavior),
        lift_start_alpha: alpha,
        lift_start_zoom: zoom,
    }
}

#[inline(always)]
pub fn tick_receptor_glow_timers(
    behavior: GameplayReceptorGlowBehavior,
    timers: GameplayReceptorGlowTimers,
    lane_pressed: bool,
    delta_time: f32,
) -> GameplayReceptorGlowTimers {
    if lane_pressed {
        return GameplayReceptorGlowTimers {
            press_timer: (timers.press_timer - delta_time).max(0.0),
            lift_timer: 0.0,
            ..timers
        };
    }
    if timers.press_timer > f32::EPSILON {
        if timers.press_timer <= delta_time {
            receptor_glow_release_timers(behavior, timers.press_timer)
        } else {
            GameplayReceptorGlowTimers {
                press_timer: timers.press_timer - delta_time,
                ..timers
            }
        }
    } else {
        GameplayReceptorGlowTimers {
            lift_timer: (timers.lift_timer - delta_time).max(0.0),
            ..timers
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct GameplayReceptorStepBehavior {
    pub duration: f32,
    pub zoom_start: f32,
    pub zoom_end: f32,
    pub tween: GameplayTween,
    pub interrupts: bool,
}

impl GameplayReceptorStepBehavior {
    pub const fn identity() -> Self {
        Self {
            duration: 0.0,
            zoom_start: 1.0,
            zoom_end: 1.0,
            tween: GameplayTween::Linear,
            interrupts: false,
        }
    }

    #[inline(always)]
    pub fn sample_zoom(self, timer_remaining: f32) -> f32 {
        let duration = self.duration.max(0.0);
        if duration <= f32::EPSILON {
            return self.zoom_end.max(0.0);
        }
        let elapsed = (duration - timer_remaining.clamp(0.0, duration)).clamp(0.0, duration);
        let progress = elapsed / duration;
        let eased = self.tween.ease(progress);
        (self.zoom_end - self.zoom_start)
            .mul_add(eased, self.zoom_start)
            .max(0.0)
    }
}

impl Default for GameplayReceptorStepBehavior {
    fn default() -> Self {
        Self {
            duration: 0.11,
            zoom_start: 0.75,
            zoom_end: 1.0,
            tween: GameplayTween::Linear,
            interrupts: true,
        }
    }
}

#[inline(always)]
pub fn default_receptor_step_behavior_for_window(
    window: Option<&str>,
) -> GameplayReceptorStepBehavior {
    match window {
        Some("W1" | "W2" | "W3" | "W4" | "W5" | "Miss") => GameplayReceptorStepBehavior::identity(),
        _ => GameplayReceptorStepBehavior::default(),
    }
}

#[inline(always)]
pub fn receptor_step_window_index(window: Option<&str>) -> usize {
    match window {
        Some("W1") => 1,
        Some("W2") => 2,
        Some("W3") => 3,
        Some("W4") => 4,
        Some("W5") => 5,
        Some("Miss") => 6,
        _ => 0,
    }
}

#[inline(always)]
pub fn tap_explosion_window_index(window: &str) -> Option<usize> {
    match window {
        "W1" => Some(0),
        "W2" => Some(1),
        "W3" => Some(2),
        "W4" => Some(3),
        "W5" => Some(4),
        "Miss" => Some(5),
        "Held" => Some(6),
        _ => None,
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TapExplosionOptions {
    pub fantastic: bool,
    pub excellent: bool,
    pub great: bool,
    pub decent: bool,
    pub way_off: bool,
    pub miss: bool,
    pub held: bool,
    pub holding: bool,
}

#[inline(always)]
pub fn tap_explosion_enabled_for_options(options: TapExplosionOptions, window: &str) -> bool {
    match window {
        "W0" | "W1" => options.fantastic,
        "W2" => options.excellent,
        "W3" => options.great,
        "W4" => options.decent,
        "W5" => options.way_off,
        "Miss" => options.miss,
        "Held" => options.held,
        _ => false,
    }
}

#[inline(always)]
pub const fn hold_explosion_enabled_for_options(options: TapExplosionOptions) -> bool {
    options.holding
}

#[derive(Clone, Debug)]
pub struct GameplayNoteskinEffects {
    receptor_glow_behavior: [GameplayReceptorGlowBehavior; MAX_PLAYERS],
    receptor_step_behaviors:
        [[[GameplayReceptorStepBehavior; RECEPTOR_STEP_WINDOW_COUNT]; MAX_COLS]; MAX_PLAYERS],
    tap_explosion_durations:
        [[[[Option<f32>; 2]; TAP_EXPLOSION_WINDOW_COUNT]; MAX_COLS]; MAX_PLAYERS],
    mine_explosion_duration: [f32; MAX_PLAYERS],
}

impl GameplayNoteskinEffects {
    #[inline(always)]
    pub fn set_receptor_glow_behavior(
        &mut self,
        player: usize,
        behavior: GameplayReceptorGlowBehavior,
    ) {
        if player < MAX_PLAYERS {
            self.receptor_glow_behavior[player] = behavior;
        }
    }

    #[inline(always)]
    pub fn set_receptor_step_behavior(
        &mut self,
        player: usize,
        local_col: usize,
        window: Option<&str>,
        behavior: GameplayReceptorStepBehavior,
    ) {
        if player < MAX_PLAYERS && local_col < MAX_COLS {
            self.receptor_step_behaviors[player][local_col][receptor_step_window_index(window)] =
                behavior;
        }
    }

    #[inline(always)]
    pub fn set_tap_explosion_duration(
        &mut self,
        player: usize,
        local_col: usize,
        window: &str,
        bright: bool,
        duration: Option<f32>,
    ) {
        if player < MAX_PLAYERS
            && local_col < MAX_COLS
            && let Some(window_idx) = tap_explosion_window_index(window)
        {
            self.tap_explosion_durations[player][local_col][window_idx][usize::from(bright)] =
                duration;
        }
    }

    #[inline(always)]
    pub fn set_mine_explosion_duration(&mut self, player: usize, duration: f32) {
        if player < MAX_PLAYERS {
            self.mine_explosion_duration[player] = duration;
        }
    }

    #[inline(always)]
    pub fn receptor_glow_behavior_for_player(&self, player: usize) -> GameplayReceptorGlowBehavior {
        self.receptor_glow_behavior[player.min(MAX_PLAYERS - 1)]
    }

    #[inline(always)]
    pub fn receptor_step_behavior_for_col(
        &self,
        player: usize,
        local_col: usize,
        window: Option<&str>,
    ) -> GameplayReceptorStepBehavior {
        self.receptor_step_behaviors[player.min(MAX_PLAYERS - 1)][local_col.min(MAX_COLS - 1)]
            [receptor_step_window_index(window)]
    }

    #[inline(always)]
    pub fn tap_explosion_duration(
        &self,
        player: usize,
        local_col: usize,
        window: &str,
        bright: bool,
    ) -> Option<f32> {
        tap_explosion_window_index(window).and_then(|window_idx| {
            self.tap_explosion_durations[player.min(MAX_PLAYERS - 1)][local_col.min(MAX_COLS - 1)]
                [window_idx][usize::from(bright)]
        })
    }

    #[inline(always)]
    pub fn mine_explosion_duration(&self, player: usize) -> f32 {
        self.mine_explosion_duration[player.min(MAX_PLAYERS - 1)]
    }
}

impl Default for GameplayNoteskinEffects {
    fn default() -> Self {
        let receptor_step_behaviors = std::array::from_fn(|_| {
            std::array::from_fn(|_| {
                std::array::from_fn(|idx| {
                    default_receptor_step_behavior_for_window(RECEPTOR_STEP_WINDOWS[idx])
                })
            })
        });
        Self {
            receptor_glow_behavior: std::array::from_fn(|_| {
                GameplayReceptorGlowBehavior::default()
            }),
            receptor_step_behaviors,
            tap_explosion_durations: std::array::from_fn(|_| {
                std::array::from_fn(|_| std::array::from_fn(|_| [None, None]))
            }),
            mine_explosion_duration: [MINE_EXPLOSION_DURATION; MAX_PLAYERS],
        }
    }
}

pub struct GameplayNoteskinData {
    pub effects: GameplayNoteskinEffects,
}

impl Default for GameplayNoteskinData {
    fn default() -> Self {
        Self {
            effects: GameplayNoteskinEffects::default(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ComboMilestoneKind {
    Hundred,
    Thousand,
}

#[derive(Clone, Debug)]
pub struct ActiveComboMilestone {
    pub kind: ComboMilestoneKind,
    pub elapsed: f32,
}

pub const MINE_HIT_INCREMENTS_MISS_COMBO: bool = false;
pub const HOLD_SUCCESS_RESETS_MISS_COMBO: bool = false;
pub const COMBO_BREAK_ON_IMMEDIATE_HOLD_LET_GO: bool = false;

pub fn trigger_combo_milestone(
    milestones: &mut Vec<ActiveComboMilestone>,
    kind: ComboMilestoneKind,
) {
    if let Some(index) = milestones
        .iter()
        .position(|milestone| milestone.kind == kind)
    {
        milestones[index].elapsed = 0.0;
    } else {
        milestones.push(ActiveComboMilestone { kind, elapsed: 0.0 });
    }
}

#[inline(always)]
pub const fn combo_milestone_duration(kind: ComboMilestoneKind) -> f32 {
    match kind {
        ComboMilestoneKind::Hundred => COMBO_HUNDRED_MILESTONE_DURATION,
        ComboMilestoneKind::Thousand => COMBO_THOUSAND_MILESTONE_DURATION,
    }
}

pub fn tick_combo_milestones(milestones: &mut Vec<ActiveComboMilestone>, delta_time: f32) {
    milestones.retain_mut(|milestone| {
        milestone.elapsed += delta_time;
        milestone.elapsed < combo_milestone_duration(milestone.kind)
    });
}

pub fn apply_combo_update_feedback(
    current_combo_window_counts: &mut WindowCounts,
    milestones: &mut Vec<ActiveComboMilestone>,
    update: ComboUpdate,
) {
    if update.combo_broken {
        *current_combo_window_counts = WindowCounts::default();
    }
    if update.hit_thousand_milestone {
        trigger_combo_milestone(milestones, ComboMilestoneKind::Thousand);
    }
    if update.hit_hundred_milestone {
        trigger_combo_milestone(milestones, ComboMilestoneKind::Hundred);
    }
}

pub fn apply_mine_hit_combo_policy(state: &mut ComboState) -> ComboUpdate {
    if MINE_HIT_INCREMENTS_MISS_COMBO {
        combo::break_combo_state(state, 1)
    } else {
        ComboUpdate::default()
    }
}

pub fn apply_hold_success_combo_policy(state: &mut ComboState) -> ComboUpdate {
    // ITG dance/pump scoring does not let Held / Roll Held reset miss combo.
    if HOLD_SUCCESS_RESETS_MISS_COMBO {
        state.miss_combo = 0;
    }
    ComboUpdate::default()
}

pub fn apply_hold_let_go_combo_policy(state: &mut ComboState) -> ComboUpdate {
    if COMBO_BREAK_ON_IMMEDIATE_HOLD_LET_GO {
        combo::break_combo_state(state, 1)
    } else {
        combo::clear_full_combo_state(state);
        ComboUpdate::default()
    }
}

// Simply Love danger overlay semantics (ScreenGameplay underlay/PerPlayer/Danger.lua).
// Metrics: itgmania/Themes/Simply Love/metrics.ini -> DangerThreshold=0.2
const DANGER_THRESHOLD: f32 = 0.2;
const DANGER_BASE_ALPHA: f32 = 0.7;
const DANGER_FADE_IN_S: f32 = 0.3;
const DANGER_HIDE_FADE_S: f32 = 0.3;
const DANGER_FLASH_IN_S: f32 = 0.3;
const DANGER_FLASH_OUT_S: f32 = 0.3;
const DANGER_FLASH_ALPHA: f32 = 0.8;
const DANGER_EFFECT_PERIOD_S: f32 = 1.0;
const DANGER_EC1_RGBA: [f32; 4] = [1.0, 0.0, 0.24, 0.1];
const DANGER_EC2_RGBA: [f32; 4] = [1.0, 0.0, 0.0, 0.35];

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum HealthState {
    #[default]
    Alive,
    Danger,
    Dead,
}

#[derive(Clone, Copy, Debug, Default)]
enum DangerAnim {
    #[default]
    Hidden,
    Danger {
        started_at: f32,
        alpha_start: f32,
    },
    FadeOut {
        started_at: f32,
        rgba_start: [f32; 4],
    },
    Flash {
        started_at: f32,
        rgb: [f32; 3],
    },
}

#[derive(Clone, Copy, Debug, Default)]
pub struct DangerFx {
    last_health: HealthState,
    prev_health: HealthState,
    anim: DangerAnim,
}

#[inline(always)]
fn lerp(a: f32, b: f32, t: f32) -> f32 {
    (b - a).mul_add(t.clamp(0.0, 1.0), a)
}

#[inline(always)]
fn danger_flash_alpha(age: f32) -> f32 {
    if !age.is_finite() || age <= 0.0 {
        return 0.0;
    }
    if age < DANGER_FLASH_IN_S {
        return DANGER_FLASH_ALPHA * (age / DANGER_FLASH_IN_S).clamp(0.0, 1.0);
    }
    let t2 = age - DANGER_FLASH_IN_S;
    if t2 < DANGER_FLASH_OUT_S {
        return DANGER_FLASH_ALPHA * (1.0 - (t2 / DANGER_FLASH_OUT_S).clamp(0.0, 1.0));
    }
    0.0
}

#[inline(always)]
fn danger_effect_rgba(age: f32, base_alpha: f32) -> [f32; 4] {
    let period = DANGER_EFFECT_PERIOD_S;
    if !age.is_finite() || !base_alpha.is_finite() || base_alpha <= 0.0 || period <= 0.0 {
        return [0.0, 0.0, 0.0, 0.0];
    }
    let phase = (age.rem_euclid(period) / period).clamp(0.0, 1.0);
    let f = ((phase + 0.25) * std::f32::consts::TAU)
        .sin()
        .mul_add(0.5, 0.5);
    let inv = 1.0 - f;

    let r = DANGER_EC1_RGBA[0] * f + DANGER_EC2_RGBA[0] * inv;
    let g = DANGER_EC1_RGBA[1] * f + DANGER_EC2_RGBA[1] * inv;
    let b = DANGER_EC1_RGBA[2] * f + DANGER_EC2_RGBA[2] * inv;
    let a = (DANGER_EC1_RGBA[3] * f + DANGER_EC2_RGBA[3] * inv) * base_alpha;
    [r, g, b, a]
}

#[inline(always)]
fn danger_anim_base_alpha(anim: &DangerAnim, now: f32) -> f32 {
    let now = if now.is_finite() { now } else { 0.0 };
    match *anim {
        DangerAnim::Hidden => 0.0,
        DangerAnim::Danger {
            started_at,
            alpha_start,
        } => {
            let age = now - started_at;
            if !age.is_finite() || age <= 0.0 {
                alpha_start
            } else if age < DANGER_FADE_IN_S {
                lerp(alpha_start, DANGER_BASE_ALPHA, age / DANGER_FADE_IN_S)
            } else {
                DANGER_BASE_ALPHA
            }
        }
        DangerAnim::FadeOut {
            started_at,
            rgba_start,
        } => {
            let age = now - started_at;
            if !age.is_finite() || age <= 0.0 {
                rgba_start[3]
            } else if age < DANGER_HIDE_FADE_S {
                lerp(rgba_start[3], 0.0, age / DANGER_HIDE_FADE_S)
            } else {
                0.0
            }
        }
        DangerAnim::Flash { started_at, .. } => danger_flash_alpha(now - started_at),
    }
}

#[inline(always)]
fn danger_anim_rgba(anim: &DangerAnim, now: f32) -> [f32; 4] {
    let now = if now.is_finite() { now } else { 0.0 };
    match *anim {
        DangerAnim::Hidden => [0.0, 0.0, 0.0, 0.0],
        DangerAnim::Danger {
            started_at,
            alpha_start,
        } => {
            let age = now - started_at;
            let base_alpha = if !age.is_finite() || age <= 0.0 {
                alpha_start
            } else if age < DANGER_FADE_IN_S {
                lerp(alpha_start, DANGER_BASE_ALPHA, age / DANGER_FADE_IN_S)
            } else {
                DANGER_BASE_ALPHA
            };
            danger_effect_rgba(age, base_alpha)
        }
        DangerAnim::FadeOut {
            started_at,
            rgba_start,
        } => {
            let age = now - started_at;
            let a = if !age.is_finite() || age <= 0.0 {
                rgba_start[3]
            } else if age < DANGER_HIDE_FADE_S {
                lerp(rgba_start[3], 0.0, age / DANGER_HIDE_FADE_S)
            } else {
                0.0
            };
            [rgba_start[0], rgba_start[1], rgba_start[2], a]
        }
        DangerAnim::Flash { started_at, rgb } => {
            let a = danger_flash_alpha(now - started_at);
            [rgb[0], rgb[1], rgb[2], a]
        }
    }
}

#[inline(always)]
pub fn danger_health_state(life: f32, is_failing: bool) -> HealthState {
    if is_failing || life <= 0.0 {
        HealthState::Dead
    } else if life < DANGER_THRESHOLD {
        HealthState::Danger
    } else {
        HealthState::Alive
    }
}

#[inline(always)]
pub fn danger_fx_rgba(fx: &DangerFx, now: f32) -> [f32; 4] {
    danger_anim_rgba(&fx.anim, now)
}

#[inline(always)]
pub fn update_danger_fx_for_health(
    fx: &mut DangerFx,
    health: HealthState,
    now: f32,
    hide_danger: bool,
) {
    if fx.last_health == health {
        return;
    }

    if hide_danger {
        if health == HealthState::Dead {
            fx.anim = DangerAnim::Flash {
                started_at: now,
                rgb: [1.0, 0.0, 0.0],
            };
        }
        fx.last_health = health;
        return;
    }

    match health {
        HealthState::Danger => {
            fx.anim = DangerAnim::Danger {
                started_at: now,
                alpha_start: danger_anim_base_alpha(&fx.anim, now),
            };
            fx.prev_health = HealthState::Danger;
        }
        HealthState::Dead => {
            fx.anim = DangerAnim::Flash {
                started_at: now,
                rgb: [1.0, 0.0, 0.0],
            };
        }
        HealthState::Alive => {
            fx.anim = if fx.prev_health == HealthState::Danger {
                DangerAnim::Flash {
                    started_at: now,
                    rgb: [0.0, 1.0, 0.0],
                }
            } else {
                DangerAnim::FadeOut {
                    started_at: now,
                    rgba_start: danger_anim_rgba(&fx.anim, now),
                }
            };
            fx.prev_health = HealthState::Alive;
        }
    }
    fx.last_health = health;
}

#[derive(Clone, Debug)]
pub struct ActiveHold {
    pub note_index: usize,
    pub start_time_ns: SongTimeNs,
    pub end_time_ns: SongTimeNs,
    pub note_type: NoteType,
    pub let_go: bool,
    pub is_pressed: bool,
    pub life: f32,
    pub last_update_time_ns: SongTimeNs,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActiveHoldResolution {
    LetGo {
        note_index: usize,
        time_ns: SongTimeNs,
    },
    Success {
        note_index: usize,
    },
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ActiveHoldAdvance {
    pub clear_active: bool,
    pub resolution: Option<ActiveHoldResolution>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct HoldResultStatsUpdate {
    pub decrement_hands_holding: bool,
    pub holds_held: u32,
    pub holds_held_for_score: u32,
    pub holds_let_go_for_score: u32,
    pub rolls_held: u32,
    pub rolls_held_for_score: u32,
    pub rolls_let_go_for_score: u32,
    pub update_grade_totals: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct HoldResultStatsState {
    pub hands_holding_count_for_stats: i32,
    pub holds_held: u32,
    pub holds_held_for_score: u32,
    pub holds_let_go_for_score: u32,
    pub rolls_held: u32,
    pub rolls_held_for_score: u32,
    pub rolls_let_go_for_score: u32,
}

pub fn apply_hold_result_stats_update(
    state: &mut HoldResultStatsState,
    update: HoldResultStatsUpdate,
) {
    if update.decrement_hands_holding && state.hands_holding_count_for_stats > 0 {
        state.hands_holding_count_for_stats -= 1;
    }
    state.holds_held = state.holds_held.saturating_add(update.holds_held);
    state.holds_held_for_score = state
        .holds_held_for_score
        .saturating_add(update.holds_held_for_score);
    state.holds_let_go_for_score = state
        .holds_let_go_for_score
        .saturating_add(update.holds_let_go_for_score);
    state.rolls_held = state.rolls_held.saturating_add(update.rolls_held);
    state.rolls_held_for_score = state
        .rolls_held_for_score
        .saturating_add(update.rolls_held_for_score);
    state.rolls_let_go_for_score = state
        .rolls_let_go_for_score
        .saturating_add(update.rolls_let_go_for_score);
}

#[inline(always)]
pub const fn replaced_active_hold_settle_time(
    active_note_index: usize,
    active_end_time_ns: SongTimeNs,
    next_note_index: usize,
    next_start_time_ns: SongTimeNs,
) -> Option<SongTimeNs> {
    if active_note_index == next_note_index || active_end_time_ns > next_start_time_ns {
        None
    } else {
        Some(active_end_time_ns)
    }
}

#[inline(always)]
pub fn begin_hold_life_decay(
    hold: &mut HoldData,
    hold_decay_active: &mut [bool],
    decaying_hold_indices: &mut Vec<usize>,
    note_index: usize,
    start_time_ns: SongTimeNs,
) {
    if hold.let_go_started_at.is_none() {
        hold.let_go_started_at = Some(start_time_ns);
        hold.let_go_starting_life = hold.life.clamp(0.0, MAX_HOLD_LIFE);
    }
    if note_index < hold_decay_active.len() && !hold_decay_active[note_index] {
        hold_decay_active[note_index] = true;
        decaying_hold_indices.push(note_index);
    }
}

pub fn apply_hold_let_go_result(
    hold: Option<&mut HoldData>,
    hold_decay_active: &mut [bool],
    decaying_hold_indices: &mut Vec<usize>,
    note_index: usize,
    let_go_time_ns: SongTimeNs,
) -> bool {
    let Some(hold) = hold else {
        return true;
    };
    if hold.result == Some(HoldResult::LetGo) {
        return false;
    }
    hold.result = Some(HoldResult::LetGo);
    begin_hold_life_decay(
        hold,
        hold_decay_active,
        decaying_hold_indices,
        note_index,
        let_go_time_ns,
    );
    true
}

pub fn apply_hold_success_result(
    hold: Option<&mut HoldData>,
    hold_decay_active: &mut [bool],
    note_index: usize,
) -> bool {
    let Some(hold) = hold else {
        return true;
    };
    if hold.result == Some(HoldResult::Held) {
        return false;
    }
    hold.result = Some(HoldResult::Held);
    hold.life = MAX_HOLD_LIFE;
    hold.let_go_started_at = None;
    hold.let_go_starting_life = 0.0;
    hold.last_held_row_index = hold.end_row_index;
    hold.last_held_beat = hold.end_beat;
    if note_index < hold_decay_active.len() {
        hold_decay_active[note_index] = false;
    }
    true
}

pub const fn hold_result_stats_update(
    note_type: NoteType,
    result: HoldResult,
    scoring_blocked: bool,
    player_dead: bool,
) -> HoldResultStatsUpdate {
    let mut update = HoldResultStatsUpdate {
        decrement_hands_holding: true,
        ..HoldResultStatsUpdate::ZERO
    };
    if scoring_blocked {
        return update;
    }

    match (note_type, result, player_dead) {
        (NoteType::Hold, HoldResult::Held, false) => {
            update.holds_held = 1;
            update.holds_held_for_score = 1;
            update.update_grade_totals = true;
        }
        (NoteType::Hold, HoldResult::Held, true) => {
            update.holds_held = 1;
        }
        (NoteType::Hold, HoldResult::LetGo, false) => {
            update.holds_let_go_for_score = 1;
            update.update_grade_totals = true;
        }
        (NoteType::Roll, HoldResult::Held, false) => {
            update.rolls_held = 1;
            update.rolls_held_for_score = 1;
            update.update_grade_totals = true;
        }
        (NoteType::Roll, HoldResult::Held, true) => {
            update.rolls_held = 1;
        }
        (NoteType::Roll, HoldResult::LetGo, false) => {
            update.rolls_let_go_for_score = 1;
            update.update_grade_totals = true;
        }
        _ => {}
    }
    update
}

impl HoldResultStatsUpdate {
    pub const ZERO: Self = Self {
        decrement_hands_holding: false,
        holds_held: 0,
        holds_held_for_score: 0,
        holds_let_go_for_score: 0,
        rolls_held: 0,
        rolls_held_for_score: 0,
        rolls_let_go_for_score: 0,
        update_grade_totals: false,
    };
}

pub fn started_active_hold_state(
    hold: Option<&mut HoldData>,
    note_index: usize,
    note_type: NoteType,
    start_time_ns: SongTimeNs,
    end_time_ns: SongTimeNs,
    current_time_ns: SongTimeNs,
) -> ActiveHold {
    if let Some(hold) = hold {
        hold.life = MAX_HOLD_LIFE;
        hold.let_go_started_at = None;
        hold.let_go_starting_life = 0.0;
    }
    ActiveHold {
        note_index,
        start_time_ns,
        end_time_ns,
        note_type,
        let_go: false,
        is_pressed: true,
        life: MAX_HOLD_LIFE,
        last_update_time_ns: current_time_ns,
    }
}

pub fn refresh_roll_life_for_step(
    active: &mut ActiveHold,
    hold: &mut HoldData,
    event_time_ns: SongTimeNs,
) -> bool {
    if !matches!(active.note_type, NoteType::Roll)
        || active.let_go
        || active.life <= 0.0
        || song_time_ns_invalid(event_time_ns)
        || event_time_ns < active.start_time_ns
        || matches!(hold.result, Some(HoldResult::LetGo | HoldResult::Missed))
    {
        return false;
    }

    active.life = MAX_HOLD_LIFE;
    active.last_update_time_ns = active
        .last_update_time_ns
        .max(event_time_ns.min(active.end_time_ns));
    hold.life = MAX_HOLD_LIFE;
    hold.let_go_started_at = None;
    hold.let_go_starting_life = 0.0;
    true
}

pub fn advance_active_hold_to_time(
    active: &mut ActiveHold,
    hold: &mut HoldData,
    timing: &TimingData,
    note_start_row: usize,
    note_start_beat: f32,
    target_time_ns: SongTimeNs,
    music_rate: f32,
) -> ActiveHoldAdvance {
    let note_index = active.note_index;
    let from_time_ns = active.last_update_time_ns;
    let final_time_ns = target_time_ns.max(from_time_ns).min(active.end_time_ns);
    let mut resolution = None;

    if !active.let_go && active.life <= 0.0 {
        active.let_go = true;
        resolution = Some(ActiveHoldResolution::LetGo {
            note_index,
            time_ns: from_time_ns.max(active.start_time_ns),
        });
    } else if final_time_ns > from_time_ns && !active.let_go {
        let body_from_ns = from_time_ns.max(active.start_time_ns);
        let body_to_ns = final_time_ns.max(active.start_time_ns);
        if body_to_ns > body_from_ns && active.life > 0.0 {
            let advance = advance_hold_life_ns(
                active.note_type,
                active.life,
                active.is_pressed,
                body_to_ns.saturating_sub(body_from_ns),
                music_rate,
            );
            // ITG updates iLastHeldRow before subtracting hold life for the
            // frame. If this interval drains life to zero, keep the visual
            // last-held row at the frame target while still resolving LetGo at
            // the exact crossing.
            let progress_time = song_time_ns_to_seconds(body_to_ns);
            if body_to_ns > body_from_ns && progress_time.is_finite() {
                let current_beat = timing.get_beat_for_time(progress_time);
                advance_hold_last_held(hold, timing, current_beat, note_start_row, note_start_beat);
            }
            active.life = advance.life_after;
            hold.life = active.life;
            if let Some(zero_elapsed_music_ns) = advance.zero_elapsed_music_ns {
                active.let_go = true;
                resolution = Some(ActiveHoldResolution::LetGo {
                    note_index,
                    time_ns: body_from_ns.saturating_add(zero_elapsed_music_ns),
                });
            }
        }
        active.last_update_time_ns = final_time_ns;
    }

    if !active.let_go {
        hold.let_go_started_at = None;
        hold.let_go_starting_life = 0.0;
    }
    if resolution.is_none() && !active.let_go && final_time_ns >= active.end_time_ns {
        resolution = Some(ActiveHoldResolution::Success { note_index });
    }

    ActiveHoldAdvance {
        clear_active: resolution.is_some() || active.let_go,
        resolution,
    }
}

#[derive(Clone, Copy, Debug)]
pub struct TurnRng {
    state: u64,
}

pub fn turn_seed_for_song(song: &SongData) -> u64 {
    let mut hasher = XxHash64::with_seed(0);
    hasher.write(song.simfile_path.to_string_lossy().as_bytes());
    hasher.finish()
}

impl TurnRng {
    #[inline(always)]
    pub fn new(seed: u64) -> Self {
        let seed = if seed == 0 {
            0x9E37_79B9_7F4A_7C15
        } else {
            seed
        };
        Self { state: seed }
    }

    #[inline(always)]
    pub fn next_u32(&mut self) -> u32 {
        // xorshift64*
        let mut x = self.state;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.state = x;
        (x >> 32) as u32
    }

    #[inline(always)]
    pub fn next_f32_unit(&mut self) -> f32 {
        (self.next_u32() as f32) * (1.0 / 4_294_967_296.0)
    }

    #[inline(always)]
    pub fn gen_range(&mut self, upper_exclusive: usize) -> usize {
        if upper_exclusive <= 1 {
            0
        } else {
            (self.next_u32() as usize) % upper_exclusive
        }
    }

    pub fn shuffle<T>(&mut self, slice: &mut [T]) {
        if slice.len() <= 1 {
            return;
        }
        for i in (1..slice.len()).rev() {
            let j = self.gen_range(i + 1);
            slice.swap(i, j);
        }
    }
}

#[inline(always)]
fn random_range_song_time_ns(rng: &mut TurnRng, min: SongTimeNs, max: SongTimeNs) -> SongTimeNs {
    if max <= min {
        return min;
    }
    let span = i128::from(max) - i128::from(min);
    let offset = (span as f64 * f64::from(rng.next_f32_unit())).floor() as i128;
    clamp_song_time_ns(i128::from(min) + offset)
}

#[inline(always)]
pub fn autoplay_random_offset_music_ns_for_window(
    rng: &mut TurnRng,
    timing_profile: TimingProfileNs,
    window: TimingWindow,
) -> SongTimeNs {
    let w0 = timing_profile.fa_plus_window_ns.unwrap_or(0);
    let (inner, outer) = match window {
        TimingWindow::W0 => (0, w0),
        TimingWindow::W1 => (w0, timing_profile.windows_ns[0]),
        TimingWindow::W2 => (timing_profile.windows_ns[0], timing_profile.windows_ns[1]),
        TimingWindow::W3 => (timing_profile.windows_ns[1], timing_profile.windows_ns[2]),
        TimingWindow::W4 => (timing_profile.windows_ns[2], timing_profile.windows_ns[3]),
        TimingWindow::W5 => (timing_profile.windows_ns[3], timing_profile.windows_ns[4]),
    };
    if outer <= 0 {
        return 0;
    }
    if inner <= 0 || inner >= outer {
        return random_range_song_time_ns(rng, -outer, outer);
    }
    if rng.next_u32() & 1 == 0 {
        random_range_song_time_ns(rng, -outer, -inner)
    } else {
        random_range_song_time_ns(rng, inner, outer)
    }
}

#[inline(always)]
pub fn autoplay_judgment_offset_music_ns(
    live_autoplay: bool,
    rng: &mut TurnRng,
    timing_profile: TimingProfileNs,
    window: TimingWindow,
    measured_offset_music_ns: SongTimeNs,
) -> SongTimeNs {
    if !live_autoplay {
        return measured_offset_music_ns;
    }
    autoplay_random_offset_music_ns_for_window(rng, timing_profile, window)
}

#[inline(always)]
pub const fn live_autoplay_enabled_from_flags(autoplay_enabled: bool, replay_mode: bool) -> bool {
    autoplay_enabled && !replay_mode
}

#[inline(always)]
pub const fn autoplay_blocks_scoring_from_flags(autoplay_enabled: bool, replay_mode: bool) -> bool {
    live_autoplay_enabled_from_flags(autoplay_enabled, replay_mode)
}

#[inline(always)]
pub const fn autoplay_cursor_for_enable(
    next_tap_miss_cursor: usize,
    note_range: (usize, usize),
) -> usize {
    let start = note_range.0;
    let end = note_range.1;
    if end < start {
        return start;
    }
    if next_tap_miss_cursor < start {
        start
    } else if next_tap_miss_cursor > end {
        end
    } else {
        next_tap_miss_cursor
    }
}

#[inline(always)]
pub fn active_hold_is_engaged(active: &ActiveHold) -> bool {
    !active.let_go && active.life > 0.0
}

#[inline(always)]
pub fn autoplay_due_active_hold_resolution(
    active: &ActiveHold,
    cutoff_time_ns: SongTimeNs,
) -> Option<ActiveHoldResolution> {
    if active.end_time_ns > cutoff_time_ns {
        return None;
    }
    if active_hold_is_engaged(active) {
        Some(ActiveHoldResolution::Success {
            note_index: active.note_index,
        })
    } else {
        Some(ActiveHoldResolution::LetGo {
            note_index: active.note_index,
            time_ns: active.end_time_ns,
        })
    }
}

#[inline(always)]
pub fn hold_head_render_flags(
    active_state: Option<&ActiveHold>,
    current_beat: f32,
    note_beat: f32,
) -> (bool, bool) {
    let reached_receptor = current_beat >= note_beat;
    let engaged = reached_receptor && active_state.is_some_and(active_hold_is_engaged);
    let use_active = engaged
        && active_state.is_some_and(|h| matches!(h.note_type, NoteType::Roll) || h.is_pressed);
    (engaged, use_active)
}

#[inline(always)]
pub fn hold_explosion_active(
    active_state: Option<&ActiveHold>,
    current_beat: f32,
    note_beat: f32,
) -> bool {
    current_beat >= note_beat && active_state.is_some_and(active_hold_is_engaged)
}

#[inline(always)]
pub fn let_go_head_beat(
    note_beat: f32,
    end_beat: f32,
    last_held_beat: f32,
    visible_beat: f32,
) -> f32 {
    last_held_beat
        .clamp(note_beat, end_beat)
        .min(visible_beat.max(note_beat))
}

#[inline(always)]
pub const fn input_queue_cap(num_cols: usize) -> usize {
    // Pre-size one backlog-warning bucket per 4-panel field so live gameplay
    // does not grow the queue before crossing its first pressure threshold.
    let fields = if num_cols <= 4 {
        1
    } else {
        num_cols.div_ceil(4)
    };
    GAMEPLAY_INPUT_BACKLOG_WARN * fields
}

#[inline(always)]
pub fn replay_edge_cap(
    num_cols: usize,
    replay_cells: usize,
    replay_mode: bool,
    song_seconds: f32,
) -> usize {
    if replay_mode {
        return 0;
    }
    // Live recording stores physical press/release edges, so reserve two edges
    // per playable note cell, keep a small per-lane floor for early misses, and
    // add a duration budget so a whole-song run does not grow on dense mashing.
    let chart_cap = replay_cells.saturating_mul(2);
    let floor_cap = num_cols.saturating_mul(REPLAY_EDGE_FLOOR_PER_LANE);
    let seconds_cap = replay_seconds_cap(num_cols, song_seconds);
    chart_cap.max(floor_cap).max(seconds_cap)
}

#[inline(always)]
fn replay_seconds_cap(num_cols: usize, song_seconds: f32) -> usize {
    if !song_seconds.is_finite() || song_seconds <= 0.0 {
        return 0;
    }
    (song_seconds.ceil() as usize)
        .saturating_mul(num_cols)
        .saturating_mul(REPLAY_EDGE_RATE_PER_SEC)
}

#[inline(always)]
pub const fn lane_press_started(pressed: bool, was_down: bool, is_down: bool) -> bool {
    pressed && !was_down && is_down
}

#[inline(always)]
pub const fn lane_release_finished(pressed: bool, was_down: bool, is_down: bool) -> bool {
    !pressed && was_down && !is_down
}

#[inline(always)]
pub const fn lane_edge_judges_tap(pressed: bool, slot_was_down: bool) -> bool {
    pressed && !slot_was_down
}

#[inline(always)]
pub const fn lane_edge_judges_lift(pressed: bool, slot_was_down: bool) -> bool {
    !pressed && slot_was_down
}

#[inline(always)]
pub const fn active_hold_counts_as_pressed(live_autoplay: bool, lane_pressed: bool) -> bool {
    live_autoplay || lane_pressed
}

#[inline(always)]
pub const fn counts_for_early_rescore(note_type: NoteType) -> bool {
    matches!(
        note_type,
        NoteType::Tap | NoteType::Lift | NoteType::Hold | NoteType::Roll
    )
}

#[inline(always)]
pub const fn row_final_grade_hides_note(grade: JudgeGrade) -> bool {
    // deadsync's gameplay ruleset is ITG timing with optional FA+ visual
    // overlays, so match Simply Love ITG's MinTNSToHideNotes=W3 behavior.
    matches!(
        grade,
        JudgeGrade::Fantastic | JudgeGrade::Excellent | JudgeGrade::Great
    )
}

#[inline(always)]
pub const fn lane_edge_matches_note_type(pressed: bool, note_type: NoteType) -> bool {
    match note_type {
        NoteType::Tap | NoteType::Hold | NoteType::Roll => pressed,
        NoteType::Lift => !pressed,
        NoteType::Fake => pressed,
        NoteType::Mine => false,
    }
}

#[inline(always)]
pub fn note_has_displayable_hold(note: &Note) -> bool {
    matches!(note.note_type, NoteType::Hold | NoteType::Roll) && note.hold.is_some()
}

#[inline(always)]
pub fn column_cue_is_mine(note: &Note) -> Option<bool> {
    if note.is_fake {
        return None;
    }
    match note.note_type {
        NoteType::Tap | NoteType::Lift | NoteType::Hold | NoteType::Roll => Some(false),
        NoteType::Mine => Some(true),
        NoteType::Fake => None,
    }
}

pub fn build_column_cues_for_player(
    notes: &[Note],
    note_range: (usize, usize),
    note_time_cache_ns: &[SongTimeNs],
    col_start: usize,
    col_end: usize,
    first_visible_time: f32,
) -> Vec<ColumnCue> {
    let (start, end) = note_range;
    if start >= end || col_start >= col_end {
        return Vec::new();
    }

    let mut column_times: Vec<(f32, Vec<ColumnCueColumn>)> = Vec::with_capacity(end - start);
    let mut i = start;
    while i < end {
        let row = notes[i].row_index;
        let mut row_time = 0.0_f32;
        let mut has_row_time = false;
        let mut columns = Vec::with_capacity(4);
        while i < end && notes[i].row_index == row {
            let note = &notes[i];
            if note.column >= col_start
                && note.column < col_end
                && let Some(is_mine) = column_cue_is_mine(note)
            {
                if !has_row_time {
                    row_time = song_time_ns_to_seconds(note_time_cache_ns[i]);
                    has_row_time = true;
                }
                columns.push(ColumnCueColumn {
                    column: note.column,
                    is_mine,
                });
            }
            i += 1;
        }
        if has_row_time {
            columns.sort_unstable_by_key(|c| c.column);
            columns.dedup_by_key(|c| c.column);
            column_times.push((row_time, columns));
        }
    }

    let mut cues = Vec::with_capacity(column_times.len());
    let mut prev_time = 0.0_f32;
    for (time, columns) in column_times {
        let duration = time - prev_time;
        if duration >= COLUMN_CUE_MIN_SECONDS || prev_time == 0.0 {
            cues.push(ColumnCue {
                start_time: prev_time,
                duration,
                columns,
            });
        }
        prev_time = time;
    }

    if first_visible_time < 0.0
        && let Some(first) = cues.first_mut()
    {
        first.duration -= first_visible_time;
        first.start_time += first_visible_time;
    }
    cues
}

#[inline(always)]
pub fn late_note_resolution_window_ns(timing_profile: &TimingProfile, rate: f32) -> SongTimeNs {
    // Mirror ITG's shared late-resolution window from Player::GetMaxStepDistanceSeconds():
    // late taps, missed hold heads, and avoided mines all wait for the largest
    // relevant gameplay window instead of resolving on their own local window.
    let profile_music_ns = TimingProfileNs::from_profile_scaled(timing_profile, rate);
    profile_music_ns
        .windows_ns
        .into_iter()
        .fold(0, i64::max)
        .max(profile_music_ns.mine_window_ns)
        .max(scaled_song_time_ns(TIMING_WINDOW_SECONDS_HOLD, rate))
        .max(scaled_song_time_ns(TIMING_WINDOW_SECONDS_ROLL, rate))
}

#[inline(always)]
pub fn max_step_distance_ns(timing_profile: &TimingProfile, rate: f32) -> SongTimeNs {
    late_note_resolution_window_ns(timing_profile, rate)
        .saturating_add(song_time_ns_from_seconds(MAX_INPUT_LATENCY_SECONDS))
}

pub fn compute_end_times_ns(
    notes: &[Note],
    note_time_cache_ns: &[SongTimeNs],
    hold_end_time_cache_ns: &[Option<SongTimeNs>],
    rate: f32,
    audio_end_time_ns: SongTimeNs,
) -> (SongTimeNs, SongTimeNs) {
    let mut last_judgable_time_ns = 0;
    let mut last_relevant_time_ns = 0;
    for (i, note) in notes.iter().enumerate() {
        let start_time_ns = note_time_cache_ns[i];
        if song_time_ns_invalid(start_time_ns) {
            continue;
        }
        let end_time_ns = hold_end_time_cache_ns[i]
            .filter(|&time_ns| !song_time_ns_invalid(time_ns))
            .unwrap_or(start_time_ns);
        last_relevant_time_ns = last_relevant_time_ns.max(end_time_ns);
        if note.can_be_judged {
            last_judgable_time_ns = last_judgable_time_ns.max(end_time_ns);
        }
    }

    let timing_profile = TimingProfile::default_itg_with_fa_plus();
    let max_step_distance_ns = max_step_distance_ns(&timing_profile, rate);
    (
        last_judgable_time_ns.saturating_add(max_step_distance_ns),
        last_relevant_time_ns
            .saturating_add(max_step_distance_ns)
            .max(audio_end_time_ns),
    )
}

#[inline(always)]
pub fn song_audio_end_time_ns(song: &SongData) -> SongTimeNs {
    let chart_end = song.precise_last_second();
    let audio_len = song.music_length_seconds;
    let end_seconds = match (
        chart_end.is_finite() && chart_end > 0.0,
        audio_len.is_finite() && audio_len > 0.0,
    ) {
        (true, true) => chart_end.min(audio_len),
        (true, false) => chart_end,
        (false, true) => audio_len,
        (false, false) => return 0,
    };
    song_time_ns_from_seconds(end_seconds)
}

#[inline(always)]
pub fn missed_note_cutoff_row_for_timing(timing: &TimingData, cutoff_time_ns: SongTimeNs) -> usize {
    let beat_info = timing.get_beat_info_from_time_ns(cutoff_time_ns);
    let mut cutoff_note_row = beat_to_note_row(beat_info.beat);
    if beat_info.is_in_freeze && !beat_info.is_in_delay {
        cutoff_note_row = cutoff_note_row.saturating_add(1);
    }
    timing.cutoff_row_for_note_row(cutoff_note_row)
}

#[inline(always)]
pub fn timing_row_floor(timing: &TimingData, beat: f32) -> usize {
    let Some(mut row) = timing.get_row_for_beat(beat) else {
        return 0;
    };
    if row > 0
        && timing
            .get_beat_for_row(row)
            .is_some_and(|row_beat| row_beat > beat)
    {
        row -= 1;
    }
    row
}

#[inline(always)]
pub fn assist_row_no_offset_for_timing(
    timing: &TimingData,
    global_offset_seconds: f32,
    music_time_ns: SongTimeNs,
) -> i32 {
    // ITG parity: assist clap/metronome uses no global-offset timing.
    // TimingData::get_beat_for_time_ns() applies global offset internally, so
    // feed (time - offset) to cancel it out.
    let beat_no_offset = timing.get_beat_for_time_ns(song_time_ns_add_seconds(
        music_time_ns,
        -global_offset_seconds,
    ));
    timing_row_floor(timing, beat_no_offset).min(i32::MAX as usize) as i32
}

#[inline(always)]
pub fn recent_step_tracks(
    pressed_since_ns: &[Option<SongTimeNs>; MAX_COLS],
    start: usize,
    end: usize,
    event_music_time_ns: SongTimeNs,
) -> usize {
    if song_time_ns_invalid(event_music_time_ns) {
        return 0;
    }
    let jump_window_ns = song_time_ns_from_seconds(STEP_CAL_JUMP_WINDOW_S);
    pressed_since_ns[start..end]
        .iter()
        .filter(|pressed_at| {
            pressed_at.is_some_and(|pressed_at_ns| {
                let age_ns = event_music_time_ns.saturating_sub(pressed_at_ns);
                age_ns >= 0 && age_ns < jump_window_ns
            })
        })
        .count()
}

#[inline(always)]
pub fn recent_step_calories(
    pressed_since_ns: &[Option<SongTimeNs>; MAX_COLS],
    start: usize,
    end: usize,
    event_music_time_ns: SongTimeNs,
    weight_pounds: i32,
) -> f32 {
    if song_time_ns_invalid(event_music_time_ns) {
        return 0.0;
    }
    let tracks = recent_step_tracks(pressed_since_ns, start, end, event_music_time_ns);
    judgment::step_calories(weight_pounds, tracks)
}

#[inline(always)]
pub fn stage_music_cut(lead_in_seconds: f32) -> GameplayMusicCut {
    GameplayMusicCut {
        start_sec: f64::from(-lead_in_seconds.max(0.0)),
        length_sec: f64::INFINITY,
        ..Default::default()
    }
}

#[inline(always)]
pub fn visible_notefield_time_ns(
    music_time_ns: SongTimeNs,
    visual_delay_seconds: f32,
) -> SongTimeNs {
    song_time_ns_add_seconds(music_time_ns, -visual_delay_seconds)
}

#[inline(always)]
pub fn music_time_from_stream_position(
    stream_position_seconds: f32,
    lead_in_seconds: f32,
    global_offset_seconds: f32,
    rate: f32,
) -> f32 {
    let rate = if rate.is_finite() && rate > 0.0 {
        rate
    } else {
        1.0
    };
    let lead_in = lead_in_seconds.max(0.0);
    let anchor = -global_offset_seconds;
    (stream_position_seconds - lead_in).mul_add(rate, anchor * (1.0 - rate))
}

#[inline(always)]
pub fn assist_clap_cursor_for_row(rows: &[usize], row: i32) -> usize {
    if row < 0 {
        0
    } else {
        rows.partition_point(|&r| r <= row as usize)
    }
}

pub fn build_assist_clap_rows(notes: &[Note], note_range: (usize, usize)) -> Vec<usize> {
    let (start, end) = note_range;
    if start >= end {
        return Vec::new();
    }

    let mut rows = Vec::with_capacity(end - start);
    let mut i = start;
    while i < end {
        let row = notes[i].row_index;
        let mut has_clap = false;
        while i < end && notes[i].row_index == row {
            let note = &notes[i];
            if note.can_be_judged
                && !note.is_fake
                && matches!(
                    note.note_type,
                    NoteType::Tap | NoteType::Lift | NoteType::Hold | NoteType::Roll
                )
            {
                has_clap = true;
            }
            i += 1;
        }
        if has_clap {
            rows.push(row);
        }
    }
    rows
}

#[inline(always)]
pub fn assist_lookahead_music_horizon_seconds(delay_seconds: f32, slope: f32) -> f32 {
    let horizon_real = (delay_seconds + ASSIST_TICK_LOOKAHEAD_MARGIN_SECONDS).max(0.0);
    let slope = if slope.is_finite() && slope > 0.0 {
        slope
    } else {
        1.0
    };
    horizon_real * slope
}

pub fn build_note_count_stats(notes: &[Note], note_range: (usize, usize)) -> Vec<NoteCountStat> {
    let (start, end) = note_range;
    let mut cursor = start.min(notes.len());
    let end = end.min(notes.len());
    let mut count = 0usize;
    let mut stats = Vec::new();

    while cursor < end {
        let row_index = notes[cursor].row_index;
        let beat = notes[cursor].beat;
        let notes_lower = count;
        while cursor < end && notes[cursor].row_index == row_index {
            count = count.saturating_add(1);
            cursor += 1;
        }
        stats.push(NoteCountStat {
            beat,
            notes_lower,
            notes_upper: count,
        });
    }

    stats
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FinalizedRowOutcome {
    pub final_grade: JudgeGrade,
}

#[derive(Clone, Debug)]
pub struct RowEntry {
    pub row_index: usize,
    pub time_ns: SongTimeNs,
    // Non-mine, non-fake, judgable notes on this row.
    pub nonmine_note_indices: [usize; MAX_COLS],
    pub nonmine_note_count: u8,
    pub rescore_track_count: u8,
    pub unresolved_count: u8,
    pub unresolved_nonlift_count: u8,
    pub had_provisional_early_hit: bool,
    pub final_outcome: Option<FinalizedRowOutcome>,
}

impl RowEntry {
    #[inline(always)]
    pub fn note_indices(&self) -> &[usize] {
        &self.nonmine_note_indices[..usize::from(self.nonmine_note_count)]
    }
}

pub fn score_rows_finalized_for_players(
    row_entries: &[RowEntry],
    row_entry_ranges: &[(usize, usize); MAX_PLAYERS],
    num_players: usize,
) -> bool {
    let players = num_players.min(MAX_PLAYERS);
    row_entry_ranges[..players].iter().all(|&(start, end)| {
        let end = end.min(row_entries.len());
        let start = start.min(end);
        row_entries[start..end]
            .iter()
            .all(|row| row.final_outcome.is_some())
    })
}

#[inline(always)]
pub fn first_time_index_at_or_after(
    times_ns: &[SongTimeNs],
    range: (usize, usize),
    time_ns: SongTimeNs,
) -> usize {
    let end = range.1.min(times_ns.len());
    let start = range.0.min(end);
    start + times_ns[start..end].partition_point(|&t| t < time_ns)
}

#[inline(always)]
pub fn first_row_entry_index_at_or_after_time(
    row_entries: &[RowEntry],
    range: (usize, usize),
    time_ns: SongTimeNs,
) -> usize {
    let end = range.1.min(row_entries.len());
    let start = range.0.min(end);
    start + row_entries[start..end].partition_point(|row| row.time_ns < time_ns)
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PracticePlayerCursors {
    pub note_cursor: usize,
    pub row_cursor: usize,
    pub mine_ix_cursor: usize,
    pub mine_avoid_cursor: usize,
}

pub fn practice_player_cursors(
    note_time_cache_ns: &[SongTimeNs],
    note_range: (usize, usize),
    row_entries: &[RowEntry],
    row_range: (usize, usize),
    mine_note_time_ns: &[SongTimeNs],
    mine_note_ix: &[usize],
    judge_start_ns: SongTimeNs,
) -> PracticePlayerCursors {
    let note_cursor = first_time_index_at_or_after(note_time_cache_ns, note_range, judge_start_ns);
    let row_cursor = first_row_entry_index_at_or_after_time(row_entries, row_range, judge_start_ns);
    let mine_ix_cursor = mine_note_time_ns.partition_point(|&t| t < judge_start_ns);
    let note_end = note_range.1.min(note_time_cache_ns.len());
    let mine_avoid_cursor = mine_note_ix
        .get(mine_ix_cursor)
        .copied()
        .unwrap_or(note_end);

    PracticePlayerCursors {
        note_cursor,
        row_cursor,
        mine_ix_cursor,
        mine_avoid_cursor,
    }
}

#[inline(always)]
pub fn count_rescore_tracks_on_row(row_entry: &RowEntry) -> usize {
    usize::from(row_entry.rescore_track_count)
}

pub fn build_row_entry(
    row_index: usize,
    nonmine_note_indices: [usize; MAX_COLS],
    nonmine_note_count: u8,
    notes: &[Note],
    note_time_cache_ns: &[SongTimeNs],
) -> RowEntry {
    debug_assert!(nonmine_note_count != 0);
    let time_ns = note_time_cache_ns[nonmine_note_indices[0]];
    let mut rescore_track_count = 0u8;
    let mut unresolved_count = 0u8;
    let mut unresolved_nonlift_count = 0u8;
    let mut had_provisional_early_hit = false;
    for &note_index in &nonmine_note_indices[..usize::from(nonmine_note_count)] {
        let note = &notes[note_index];
        if counts_for_early_rescore(note.note_type) {
            rescore_track_count = rescore_track_count.saturating_add(1);
        }
        if note.result.is_none() {
            unresolved_count = unresolved_count.saturating_add(1);
            if note.note_type != NoteType::Lift {
                unresolved_nonlift_count = unresolved_nonlift_count.saturating_add(1);
            }
        }
        had_provisional_early_hit |= note.early_result.is_some();
    }
    RowEntry {
        row_index,
        time_ns,
        nonmine_note_indices,
        nonmine_note_count,
        rescore_track_count,
        unresolved_count,
        unresolved_nonlift_count,
        had_provisional_early_hit,
        final_outcome: None,
    }
}

pub fn reset_practice_notes_and_rows(
    notes: &mut [Note],
    row_entries: &mut [RowEntry],
    note_time_cache_ns: &[SongTimeNs],
) {
    for note in notes.iter_mut() {
        note.result = None;
        note.early_result = None;
        note.mine_result = None;
        if let Some(hold) = note.hold.as_mut() {
            hold.result = None;
            hold.life = MAX_HOLD_LIFE;
            hold.let_go_started_at = None;
            hold.let_go_starting_life = MAX_HOLD_LIFE;
            hold.last_held_row_index = note.row_index;
            hold.last_held_beat = note.beat;
        }
    }

    for row_entry in row_entries {
        *row_entry = build_row_entry(
            row_entry.row_index,
            row_entry.nonmine_note_indices,
            row_entry.nonmine_note_count,
            notes,
            note_time_cache_ns,
        );
    }
}

#[inline(always)]
pub fn mark_row_entry_provisional_early_result(
    row_entries: &mut [RowEntry],
    note_row_entry_indices: &[u32],
    note_index: usize,
) -> bool {
    let Some(&row_entry_index) = note_row_entry_indices.get(note_index) else {
        return false;
    };
    if row_entry_index == u32::MAX {
        return false;
    }
    let Some(row_entry) = row_entries.get_mut(row_entry_index as usize) else {
        return false;
    };
    row_entry.had_provisional_early_hit = true;
    true
}

#[inline(always)]
pub fn mark_row_entry_note_finalized(
    row_entries: &mut [RowEntry],
    note_row_entry_indices: &[u32],
    note_index: usize,
    note_type: NoteType,
) -> bool {
    let Some(&row_entry_index) = note_row_entry_indices.get(note_index) else {
        return false;
    };
    if row_entry_index == u32::MAX {
        return false;
    }
    let Some(row_entry) = row_entries.get_mut(row_entry_index as usize) else {
        return false;
    };
    row_entry.unresolved_count = row_entry.unresolved_count.saturating_sub(1);
    if note_type != NoteType::Lift {
        row_entry.unresolved_nonlift_count = row_entry.unresolved_nonlift_count.saturating_sub(1);
    }
    true
}

#[inline(always)]
pub fn row_entry_index_for_cached_row(row_map_cache: &[u32], row_index: usize) -> Option<usize> {
    let pos = *row_map_cache.get(row_index)?;
    if pos == u32::MAX {
        return None;
    }
    Some(pos as usize)
}

#[inline(always)]
pub fn finalized_row_outcome_for_entry(
    row_entries: &[RowEntry],
    row_entry_index: usize,
) -> Option<FinalizedRowOutcome> {
    row_entries
        .get(row_entry_index)
        .and_then(|row_entry| row_entry.final_outcome)
}

#[inline(always)]
pub fn finalized_row_outcome_for_cached_row(
    row_entries: &[RowEntry],
    row_map_cache: &[u32],
    row_index: usize,
) -> Option<FinalizedRowOutcome> {
    let row_entry_index = row_entry_index_for_cached_row(row_map_cache, row_index)?;
    finalized_row_outcome_for_entry(row_entries, row_entry_index)
}

#[inline(always)]
pub fn row_entry_for_cached_row<'a>(
    row_entries: &'a [RowEntry],
    row_map_cache: &[u32],
    row_index: usize,
) -> Option<&'a RowEntry> {
    let pos = row_entry_index_for_cached_row(row_map_cache, row_index)?;
    let row_entry = row_entries.get(pos as usize)?;
    debug_assert_eq!(row_entry.row_index, row_index);
    Some(row_entry)
}

#[inline(always)]
pub fn completed_row_final_judgment<'a>(
    notes: &'a [Note],
    row_entry: &RowEntry,
) -> Option<&'a Judgment> {
    let mut row_judgments: [Option<&Judgment>; MAX_COLS] = [None; MAX_COLS];
    let mut row_judgment_count = 0usize;

    for &note_index in row_entry.note_indices() {
        let judgment = notes[note_index].result.as_ref()?;
        debug_assert!(row_judgment_count < row_judgments.len());
        row_judgments[row_judgment_count] = Some(judgment);
        row_judgment_count += 1;
    }

    judgment::aggregate_row_final_judgment(
        row_judgments[..row_judgment_count]
            .iter()
            .filter_map(|judgment| *judgment),
    )
}

#[inline(always)]
pub fn completed_row_flash_note_indices_and_judgment(
    notes: &[Note],
    row_entry: &RowEntry,
) -> Option<([usize; MAX_COLS], usize, Judgment)> {
    let Some(final_judgment) = completed_row_final_judgment(notes, row_entry) else {
        return None;
    };

    let mut out = [usize::MAX; MAX_COLS];
    let mut len = 0usize;
    for &note_index in row_entry.note_indices() {
        debug_assert!(len < out.len());
        out[len] = note_index;
        len += 1;
    }
    Some((out, len, *final_judgment))
}

#[inline(always)]
pub const fn suppress_final_bad_rescore_visual(
    row_had_provisional_early_hit: bool,
    final_grade: JudgeGrade,
) -> bool {
    row_had_provisional_early_hit && matches!(final_grade, JudgeGrade::Decent | JudgeGrade::WayOff)
}

#[inline(always)]
pub const fn finalized_row_awards_hand(
    final_grade: JudgeGrade,
    note_count: u32,
    carried_holds_down: usize,
) -> bool {
    if matches!(final_grade, JudgeGrade::Miss | JudgeGrade::WayOff) {
        return false;
    }
    note_count as usize + carried_holds_down >= 3
}

pub fn carried_holds_down_at_row(
    notes: &[Note],
    active_holds: &[Option<ActiveHold>],
    col_range: (usize, usize),
    row_index: usize,
) -> usize {
    let start = col_range.0.min(active_holds.len());
    let end = col_range.1.min(active_holds.len());
    if start >= end {
        return 0;
    }
    active_holds[start..end]
        .iter()
        .filter_map(|active| active.as_ref())
        .filter(|active| active_hold_is_engaged(active))
        .filter(|active| {
            let Some(note) = notes.get(active.note_index) else {
                return false;
            };
            note.row_index < row_index
                && note
                    .hold
                    .as_ref()
                    .is_some_and(|hold| hold.last_held_row_index >= row_index)
        })
        .count()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlayerRowScanState {
    BeyondLookahead,
    Pending,
    Ready {
        row_index: usize,
        skip_life_change: bool,
    },
    Finalized,
}

#[inline(always)]
pub fn player_row_scan_state(
    row_entries: &[RowEntry],
    row_entry_index: usize,
    lookahead_time_ns: SongTimeNs,
) -> PlayerRowScanState {
    let row_entry = &row_entries[row_entry_index];
    if row_entry.final_outcome.is_some() {
        return PlayerRowScanState::Finalized;
    }
    if row_entry.time_ns > lookahead_time_ns {
        return PlayerRowScanState::BeyondLookahead;
    }
    if row_entry.unresolved_count != 0 {
        return PlayerRowScanState::Pending;
    }
    PlayerRowScanState::Ready {
        row_index: row_entry.row_index,
        skip_life_change: row_entry.had_provisional_early_hit,
    }
}

#[inline(always)]
pub fn next_ready_row_in_lookahead<F>(
    start: usize,
    row_count: usize,
    mut row_state: F,
) -> Option<(usize, usize, bool)>
where
    F: FnMut(usize) -> PlayerRowScanState,
{
    let mut row_entry_index = start;
    while row_entry_index < row_count {
        match row_state(row_entry_index) {
            PlayerRowScanState::BeyondLookahead => break,
            PlayerRowScanState::Ready {
                row_index,
                skip_life_change,
            } => return Some((row_entry_index, row_index, skip_life_change)),
            PlayerRowScanState::Pending | PlayerRowScanState::Finalized => {}
        }
        row_entry_index += 1;
    }
    None
}

#[inline(always)]
pub fn advance_judged_row_cursor<F>(cursor: usize, row_count: usize, mut row_state: F) -> usize
where
    F: FnMut(usize) -> PlayerRowScanState,
{
    let mut next_cursor = cursor;
    while next_cursor < row_count {
        match row_state(next_cursor) {
            PlayerRowScanState::Finalized => {
                next_cursor += 1;
            }
            PlayerRowScanState::BeyondLookahead
            | PlayerRowScanState::Pending
            | PlayerRowScanState::Ready { .. } => break,
        }
    }
    next_cursor
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RowGrid {
    pub row_index: usize,
    pub note_indices: [usize; MAX_COLS],
}

#[inline(always)]
pub fn notes_row_sorted(notes: &[Note]) -> bool {
    notes
        .windows(2)
        .all(|pair| pair[0].row_index <= pair[1].row_index)
}

pub fn build_row_grids(
    notes: &[Note],
    note_range: (usize, usize),
    col_offset: usize,
    cols: usize,
) -> Vec<RowGrid> {
    let (start, end) = note_range;
    debug_assert!(start <= end && end <= notes.len());
    debug_assert!(notes_row_sorted(&notes[start..end]));

    let mut rows = Vec::<RowGrid>::new();
    for (offset, note) in notes[start..end].iter().enumerate() {
        let note_idx = start + offset;
        if note.column < col_offset {
            continue;
        }
        let local = note.column - col_offset;
        if local >= cols || local >= MAX_COLS {
            continue;
        }
        if !matches!(rows.last(), Some(row) if row.row_index == note.row_index) {
            rows.push(RowGrid {
                row_index: note.row_index,
                note_indices: [usize::MAX; MAX_COLS],
            });
        }
        rows.last_mut()
            .expect("row grid inserted for current note")
            .note_indices[local] = note_idx;
    }
    rows
}

#[inline(always)]
fn note_counts_for_simultaneous_limit(note: &Note) -> bool {
    match note.note_type {
        NoteType::Tap | NoteType::Lift => !note.is_fake,
        NoteType::Hold | NoteType::Roll => true,
        NoteType::Mine | NoteType::Fake => false,
    }
}

pub fn enforce_max_simultaneous_notes(
    notes: &mut Vec<Note>,
    max_simultaneous: usize,
    col_offset: usize,
    cols: usize,
) {
    if notes.is_empty() || cols == 0 || cols > MAX_COLS {
        return;
    }
    debug_assert!(notes_row_sorted(notes));

    let mut remove_idx = vec![false; notes.len()];
    let mut active_hold_ends: [Option<usize>; MAX_COLS] = [None; MAX_COLS];
    let mut row_candidates = Vec::<(usize, usize)>::with_capacity(MAX_COLS);

    let mut row_start = 0usize;
    while row_start < notes.len() {
        let row = notes[row_start].row_index;
        let mut row_end = row_start + 1;
        while row_end < notes.len() && notes[row_end].row_index == row {
            row_end += 1;
        }

        for held in active_hold_ends.iter_mut().take(cols) {
            if held.is_some_and(|end| end < row) {
                *held = None;
            }
        }

        let active_holds = active_hold_ends
            .iter()
            .take(cols)
            .filter(|end| end.is_some())
            .count();

        row_candidates.clear();
        for (offset, note) in notes[row_start..row_end].iter().enumerate() {
            let idx = row_start + offset;
            if note.column < col_offset {
                continue;
            }
            let local_col = note.column - col_offset;
            if local_col >= cols || !note_counts_for_simultaneous_limit(note) {
                continue;
            }
            row_candidates.push((local_col, idx));
        }

        if row_candidates.is_empty() {
            row_start = row_end;
            continue;
        }

        row_candidates.sort_unstable_by_key(|(local_col, _)| *local_col);
        let mut tracks_to_remove = active_holds
            .saturating_add(row_candidates.len())
            .saturating_sub(max_simultaneous);

        if tracks_to_remove > 0 {
            for &(_, idx) in &row_candidates {
                if tracks_to_remove == 0 {
                    break;
                }
                remove_idx[idx] = true;
                tracks_to_remove -= 1;
            }
        }

        for &(local_col, idx) in &row_candidates {
            if remove_idx[idx] || !matches!(notes[idx].note_type, NoteType::Hold | NoteType::Roll) {
                continue;
            }
            let end_row = notes[idx]
                .hold
                .as_ref()
                .map(|hold| hold.end_row_index)
                .unwrap_or(row);
            if active_hold_ends[local_col].is_none_or(|current| current < end_row) {
                active_hold_ends[local_col] = Some(end_row);
            }
        }

        row_start = row_end;
    }

    if remove_idx.iter().all(|remove| !*remove) {
        return;
    }

    let mut idx = 0usize;
    notes.retain(|_| {
        let keep = !remove_idx[idx];
        idx += 1;
        keep
    });
}

#[inline(always)]
pub fn local_player_col(column: usize, col_offset: usize, cols: usize) -> Option<usize> {
    if column < col_offset {
        return None;
    }
    let local = column - col_offset;
    (local < cols).then_some(local)
}

#[inline(always)]
pub const fn player_index_for_column(
    num_players: usize,
    cols_per_player: usize,
    column: usize,
) -> usize {
    if num_players <= 1 || cols_per_player == 0 {
        return 0;
    }
    let player = column / cols_per_player;
    let last_player = num_players.saturating_sub(1);
    if player > last_player {
        last_player
    } else {
        player
    }
}

#[inline(always)]
pub const fn local_column_for_field(cols_per_player: usize, column: usize) -> usize {
    if cols_per_player == 0 {
        column
    } else {
        column % cols_per_player
    }
}

pub fn sort_player_notes(notes: &mut [Note]) {
    notes.sort_unstable_by_key(|note| (note.row_index, note.column));
}

pub fn player_rows(notes: &[Note], col_offset: usize, cols: usize) -> Vec<usize> {
    let mut rows = Vec::with_capacity(notes.len());
    for note in notes {
        if local_player_col(note.column, col_offset, cols).is_some() {
            rows.push(note.row_index);
        }
    }
    rows.sort_unstable();
    rows.dedup();
    rows
}

pub fn count_nonempty_tracks_at_row(
    notes: &[Note],
    row: usize,
    col_offset: usize,
    cols: usize,
) -> usize {
    let mut seen = [false; MAX_COLS];
    for note in notes {
        if note.row_index != row {
            continue;
        }
        if let Some(local) = local_player_col(note.column, col_offset, cols) {
            seen[local] = true;
        }
    }
    seen[..cols].iter().filter(|&&on| on).count()
}

pub fn count_tap_or_hold_tracks_at_row(
    notes: &[Note],
    row: usize,
    col_offset: usize,
    cols: usize,
) -> usize {
    let mut seen = [false; MAX_COLS];
    for note in notes {
        if note.row_index != row {
            continue;
        }
        if !matches!(
            note.note_type,
            NoteType::Tap | NoteType::Lift | NoteType::Hold | NoteType::Roll
        ) {
            continue;
        }
        if let Some(local) = local_player_col(note.column, col_offset, cols) {
            seen[local] = true;
        }
    }
    seen[..cols].iter().filter(|&&on| on).count()
}

pub fn count_tap_tracks_at_row(
    notes: &[Note],
    row: usize,
    col_offset: usize,
    cols: usize,
) -> usize {
    let mut seen = [false; MAX_COLS];
    for note in notes {
        if note.row_index != row
            || !matches!(note.note_type, NoteType::Tap | NoteType::Lift)
            || note.is_fake
        {
            continue;
        }
        if let Some(local) = local_player_col(note.column, col_offset, cols) {
            seen[local] = true;
        }
    }
    seen[..cols].iter().filter(|&&on| on).count()
}

pub fn first_nonempty_track_at_row(
    notes: &[Note],
    row: usize,
    col_offset: usize,
    cols: usize,
) -> Option<usize> {
    let mut first: Option<usize> = None;
    for note in notes {
        if note.row_index != row {
            continue;
        }
        let Some(local) = local_player_col(note.column, col_offset, cols) else {
            continue;
        };
        first = Some(match first {
            Some(curr) => curr.min(local),
            None => local,
        });
    }
    first
}

pub fn first_tap_track_at_row(
    notes: &[Note],
    row: usize,
    col_offset: usize,
    cols: usize,
) -> Option<usize> {
    let mut first: Option<usize> = None;
    for note in notes {
        if note.row_index != row
            || !matches!(note.note_type, NoteType::Tap | NoteType::Lift)
            || note.is_fake
        {
            continue;
        }
        let Some(local) = local_player_col(note.column, col_offset, cols) else {
            continue;
        };
        first = Some(match first {
            Some(curr) => curr.min(local),
            None => local,
        });
    }
    first
}

pub fn cell_has_any_note(notes: &[Note], row: usize, column: usize) -> bool {
    notes
        .iter()
        .any(|note| note.row_index == row && note.column == column)
}

pub fn cell_has_nonfake_note(notes: &[Note], row: usize, column: usize) -> bool {
    notes
        .iter()
        .any(|note| note.row_index == row && note.column == column && !note.is_fake)
}

pub fn remove_cell_notes(notes: &mut Vec<Note>, row: usize, column: usize) {
    notes.retain(|note| !(note.row_index == row && note.column == column));
}

pub fn is_hold_body_at_row(notes: &[Note], row: usize, column: usize) -> bool {
    let mut latest: Option<&Note> = None;
    for note in notes {
        if note.column != column || note.row_index > row {
            continue;
        }
        if latest.is_none_or(|curr| note.row_index >= curr.row_index) {
            latest = Some(note);
        }
    }
    let Some(note) = latest else {
        return false;
    };
    if !matches!(note.note_type, NoteType::Hold | NoteType::Roll) || note.row_index >= row {
        return false;
    }
    note.hold
        .as_ref()
        .is_some_and(|hold| hold.end_row_index >= row)
}

pub fn count_held_tracks_at_row(
    notes: &[Note],
    row: usize,
    col_offset: usize,
    cols: usize,
) -> usize {
    (0..cols)
        .filter(|local| is_hold_body_at_row(notes, row, col_offset + *local))
        .count()
}

pub fn set_added_tap_note(
    notes: &mut Vec<Note>,
    timing_player: &TimingData,
    row: usize,
    column: usize,
) -> bool {
    let Some(beat) = timing_player.get_beat_for_row(row) else {
        return false;
    };
    remove_cell_notes(notes, row, column);
    let quantization_idx = quantization_index_from_beat(beat);
    notes.push(Note {
        beat,
        quantization_idx,
        column,
        note_type: NoteType::Tap,
        row_index: row,
        result: None,
        early_result: None,
        hold: None,
        mine_result: None,
        is_fake: false,
        can_be_judged: timing_player.is_judgable_at_beat(beat),
    });
    true
}

pub fn set_added_mine_note(
    notes: &mut Vec<Note>,
    timing_player: &TimingData,
    row: usize,
    column: usize,
) -> bool {
    let Some(beat) = timing_player.get_beat_for_row(row) else {
        return false;
    };
    remove_cell_notes(notes, row, column);
    let quantization_idx = quantization_index_from_beat(beat);
    notes.push(Note {
        beat,
        quantization_idx,
        column,
        note_type: NoteType::Mine,
        row_index: row,
        result: None,
        early_result: None,
        hold: None,
        mine_result: None,
        is_fake: false,
        can_be_judged: timing_player.is_judgable_at_beat(beat),
    });
    true
}

pub fn convert_tap_row_to_mines(notes: &mut [Note], row: usize) {
    for note in notes.iter_mut() {
        if note.row_index == row && note.note_type == NoteType::Tap {
            note.note_type = NoteType::Mine;
            note.hold = None;
            note.mine_result = None;
        }
    }
}

pub fn track_range_has_any_note(
    notes: &[Note],
    column: usize,
    start_row: usize,
    end_row: usize,
) -> bool {
    notes.iter().any(|note| {
        note.column == column && note.row_index >= start_row && note.row_index <= end_row
    })
}

pub fn apply_mines_insert(
    notes: &mut Vec<Note>,
    context_notes: &[Note],
    timing_player: &TimingData,
    col_offset: usize,
    cols: usize,
    start_row: usize,
    end_row: usize,
) {
    if cols == 0 || cols > MAX_COLS || end_row < start_row {
        return;
    }

    let mut row_count = 0usize;
    let mut place_every_rows = 6usize;
    for row in player_rows(notes, col_offset, cols) {
        if row < start_row || row > end_row {
            continue;
        }
        row_count = row_count.saturating_add(1);
        if row_count < place_every_rows {
            continue;
        }
        convert_tap_row_to_mines(notes, row);
        row_count = 0;
        place_every_rows = if place_every_rows == 6 { 7 } else { 6 };
    }

    let half_beat_rows = (ROWS_PER_BEAT.max(1) / 2) as usize;
    let hold_heads: Vec<(usize, usize)> = notes
        .iter()
        .filter_map(|note| {
            matches!(note.note_type, NoteType::Hold | NoteType::Roll)
                .then_some((note.column, note.hold.as_ref()?.end_row_index))
        })
        .collect();
    let mut full_context = Vec::with_capacity(context_notes.len() + notes.len() + hold_heads.len());
    full_context.extend_from_slice(context_notes);
    full_context.extend(notes.iter().cloned());
    for (column, end_row_index) in hold_heads {
        let mine_row = end_row_index.saturating_add(half_beat_rows);
        if mine_row < start_row || mine_row > end_row {
            continue;
        }
        let range_start = mine_row.saturating_sub(half_beat_rows).saturating_add(1);
        let range_end = mine_row.saturating_add(half_beat_rows).saturating_sub(1);
        if track_range_has_any_note(&full_context, column, range_start, range_end) {
            continue;
        }
        if !set_added_mine_note(notes, timing_player, mine_row, column) {
            continue;
        }
        convert_tap_row_to_mines(notes, mine_row);
        if let Some(note) = notes
            .iter()
            .find(|note| note.column == column && note.row_index == mine_row)
        {
            full_context.push(note.clone());
        }
    }
}

#[inline(always)]
pub fn stomp_mirror_track(local_track: usize, cols: usize) -> usize {
    match cols {
        4 => [3, 2, 1, 0][local_track],
        8 => [1, 0, 3, 2, 5, 4, 7, 6][local_track],
        _ => cols.saturating_sub(1).saturating_sub(local_track),
    }
}

pub fn apply_insert_intelligent_taps(
    notes: &mut Vec<Note>,
    timing_player: &TimingData,
    col_offset: usize,
    cols: usize,
    window_size_rows: usize,
    insert_offset_rows: usize,
    window_stride_rows: usize,
    skippy_mode: bool,
) {
    if cols == 0 || cols > MAX_COLS || insert_offset_rows > window_size_rows {
        return;
    }
    let rows = player_rows(notes, col_offset, cols);
    let require_begin = !skippy_mode;
    let require_end = true;
    for &row in &rows {
        if row % window_stride_rows != 0 {
            continue;
        }
        let row_earlier = row;
        let row_later = row_earlier.saturating_add(window_size_rows);
        let row_to_add = row_earlier.saturating_add(insert_offset_rows);

        if require_begin
            && (count_nonempty_tracks_at_row(notes, row_earlier, col_offset, cols) != 1
                || count_tap_or_hold_tracks_at_row(notes, row_earlier, col_offset, cols) != 1)
        {
            continue;
        }
        if require_end
            && (count_nonempty_tracks_at_row(notes, row_later, col_offset, cols) != 1
                || count_tap_or_hold_tracks_at_row(notes, row_later, col_offset, cols) != 1)
        {
            continue;
        }

        let mut note_in_middle = false;
        for local in 0..cols {
            if is_hold_body_at_row(notes, row_earlier.saturating_add(1), col_offset + local) {
                note_in_middle = true;
                break;
            }
        }
        if !note_in_middle {
            for note in notes.iter() {
                if local_player_col(note.column, col_offset, cols).is_none() {
                    continue;
                }
                if note.row_index >= row_earlier.saturating_add(1)
                    && note.row_index <= row_later.saturating_sub(1)
                {
                    note_in_middle = true;
                    break;
                }
            }
        }
        if note_in_middle {
            continue;
        }

        let earlier_track = first_nonempty_track_at_row(notes, row_earlier, col_offset, cols);
        let later_track = first_nonempty_track_at_row(notes, row_later, col_offset, cols);
        let Some(later_track) = later_track else {
            continue;
        };
        let track_to_add =
            if skippy_mode && earlier_track.is_some() && earlier_track != Some(later_track) {
                earlier_track.unwrap_or(0)
            } else if let Some(earlier_track) = earlier_track {
                if earlier_track.abs_diff(later_track) >= 2 {
                    earlier_track.min(later_track).saturating_add(1)
                } else if earlier_track.min(later_track) >= 1 {
                    earlier_track.min(later_track) - 1
                } else if earlier_track.max(later_track).saturating_add(1) < cols {
                    earlier_track.max(later_track).saturating_add(1)
                } else {
                    0
                }
            } else {
                0
            };

        let _ = set_added_tap_note(
            notes,
            timing_player,
            row_to_add,
            col_offset.saturating_add(track_to_add),
        );
    }
}

pub fn apply_wide_insert(
    notes: &mut Vec<Note>,
    timing_player: &TimingData,
    col_offset: usize,
    cols: usize,
) {
    if cols == 0 || cols > MAX_COLS {
        return;
    }
    let rows = player_rows(notes, col_offset, cols);
    let rows_per_beat = ROWS_PER_BEAT.max(1) as usize;
    let half_beat = rows_per_beat / 2;
    let even_beat_stride = rows_per_beat.saturating_mul(2);
    for row in rows {
        if row % even_beat_stride != 0 {
            continue;
        }
        if count_held_tracks_at_row(notes, row, col_offset, cols) > 0 {
            continue;
        }
        if count_tap_tracks_at_row(notes, row, col_offset, cols) != 1 {
            continue;
        }
        let mut has_space = true;
        for note in notes.iter() {
            if local_player_col(note.column, col_offset, cols).is_none() {
                continue;
            }
            if note.row_index >= row.saturating_sub(half_beat).saturating_add(1)
                && note.row_index <= row.saturating_add(half_beat)
                && note.row_index != row
            {
                has_space = false;
                break;
            }
        }
        if !has_space {
            continue;
        }
        let Some(orig_track) = first_tap_track_at_row(notes, row, col_offset, cols) else {
            continue;
        };
        let beat_i = ((row as f32) / (rows_per_beat as f32)).round() as i32;
        let mut add_track = (orig_track as i32) + (beat_i % 5) - 2;
        add_track = add_track.clamp(0, cols.saturating_sub(1) as i32);
        if add_track as usize == orig_track {
            add_track = (add_track + 1).clamp(0, cols.saturating_sub(1) as i32);
        }
        if add_track as usize == orig_track {
            add_track = (add_track - 1).clamp(0, cols.saturating_sub(1) as i32);
        }
        let mut add_track = add_track as usize;
        if cell_has_nonfake_note(notes, row, col_offset.saturating_add(add_track)) {
            add_track = (add_track + 1) % cols;
        }
        let _ = set_added_tap_note(
            notes,
            timing_player,
            row,
            col_offset.saturating_add(add_track),
        );
    }
}

pub fn apply_stomp_insert(
    notes: &mut Vec<Note>,
    timing_player: &TimingData,
    col_offset: usize,
    cols: usize,
) {
    if cols == 0 || cols > MAX_COLS {
        return;
    }
    let rows = player_rows(notes, col_offset, cols);
    let half_beat = (ROWS_PER_BEAT.max(1) as usize) / 2;
    for row in rows {
        if count_tap_tracks_at_row(notes, row, col_offset, cols) != 1 {
            continue;
        }
        let mut tap_in_middle = false;
        let row_begin = row.saturating_sub(half_beat);
        let row_end = row.saturating_add(half_beat);
        for note in notes.iter() {
            if local_player_col(note.column, col_offset, cols).is_none()
                || !matches!(note.note_type, NoteType::Tap | NoteType::Lift)
                || note.is_fake
                || note.row_index == row
            {
                continue;
            }
            if note.row_index > row_begin && note.row_index < row_end {
                tap_in_middle = true;
                break;
            }
        }
        if tap_in_middle || count_held_tracks_at_row(notes, row, col_offset, cols) >= 1 {
            continue;
        }
        let Some(track) = first_tap_track_at_row(notes, row, col_offset, cols) else {
            continue;
        };
        let add_track = stomp_mirror_track(track, cols);
        let _ = set_added_tap_note(
            notes,
            timing_player,
            row,
            col_offset.saturating_add(add_track),
        );
    }
}

pub fn apply_echo_insert(
    notes: &mut Vec<Note>,
    timing_player: &TimingData,
    col_offset: usize,
    cols: usize,
) {
    if cols == 0 || cols > MAX_COLS {
        return;
    }
    let rows_per_interval = (ROWS_PER_BEAT.max(1) as usize) / 2;
    if rows_per_interval == 0 {
        return;
    }
    let max_row = player_rows(notes, col_offset, cols)
        .into_iter()
        .max()
        .unwrap_or(0);
    let end_row = max_row.saturating_add(1);
    let mut echo_track: Option<usize> = None;
    let mut row = 0usize;
    while row <= end_row {
        if count_nonempty_tracks_at_row(notes, row, col_offset, cols) == 0 {
            row = row.saturating_add(rows_per_interval);
            continue;
        }
        if let Some(track) = first_tap_track_at_row(notes, row, col_offset, cols) {
            echo_track = Some(track);
        }
        let Some(track) = echo_track else {
            row = row.saturating_add(rows_per_interval);
            continue;
        };
        let row_window_end = row.saturating_add(rows_per_interval.saturating_mul(2));
        let mut note_in_middle = false;
        for note in notes.iter() {
            if local_player_col(note.column, col_offset, cols).is_none() {
                continue;
            }
            if note.row_index > row && note.row_index < row_window_end {
                note_in_middle = true;
                break;
            }
        }
        if note_in_middle {
            row = row.saturating_add(rows_per_interval);
            continue;
        }

        let row_echo = row.saturating_add(rows_per_interval);
        if count_held_tracks_at_row(notes, row_echo, col_offset, cols) >= 2
            || is_hold_body_at_row(notes, row_echo, col_offset + track)
        {
            row = row.saturating_add(rows_per_interval);
            continue;
        }
        let _ = set_added_tap_note(notes, timing_player, row_echo, col_offset + track);
        row = row.saturating_add(rows_per_interval);
    }
}

fn find_tap_index(notes: &[Note], row: usize, column: usize) -> Option<usize> {
    notes.iter().position(|note| {
        note.row_index == row
            && note.column == column
            && note.note_type == NoteType::Tap
            && !note.is_fake
    })
}

pub fn convert_taps_to_holds(
    notes: &mut [Note],
    timing_player: &TimingData,
    col_offset: usize,
    cols: usize,
    simultaneous_holds: usize,
) {
    if cols == 0 || cols > MAX_COLS {
        return;
    }
    let rows = player_rows(notes, col_offset, cols);
    let rows_per_beat = ROWS_PER_BEAT.max(1) as usize;

    for &row in &rows {
        let mut added_this_row = 0usize;
        for local in 0..cols {
            if added_this_row > simultaneous_holds {
                break;
            }
            let col = col_offset + local;
            let Some(head_idx) = find_tap_index(notes, row, col) else {
                continue;
            };
            let mut taps_left = simultaneous_holds as isize;
            let mut end_row = row.saturating_add(1);
            let mut add_hold = true;

            for &next_row in rows.iter().filter(|&&r| r > row) {
                end_row = next_row;
                if cell_has_any_note(notes, next_row, col) {
                    add_hold = false;
                    break;
                }

                let mut tracks_down = 0usize;
                for check_local in 0..cols {
                    let check_col = col_offset + check_local;
                    if is_hold_body_at_row(notes, next_row, check_col)
                        || cell_has_any_note(notes, next_row, check_col)
                    {
                        tracks_down = tracks_down.saturating_add(1);
                    }
                }

                taps_left -= tracks_down as isize;
                if taps_left == 0 {
                    break;
                }
                if taps_left < 0 {
                    add_hold = false;
                    break;
                }
            }

            if !add_hold {
                continue;
            }
            if end_row == row.saturating_add(1) {
                end_row = row.saturating_add(rows_per_beat);
            }

            let Some(end_beat) = timing_player.get_beat_for_row(end_row) else {
                continue;
            };
            let head_beat = notes[head_idx].beat;
            notes[head_idx].note_type = NoteType::Hold;
            notes[head_idx].hold = Some(HoldData {
                end_row_index: end_row,
                end_beat,
                result: None,
                life: INITIAL_HOLD_LIFE,
                let_go_started_at: None,
                let_go_starting_life: 0.0,
                last_held_row_index: row,
                last_held_beat: head_beat,
            });
            added_this_row = added_this_row.saturating_add(1);
        }
    }
}

pub fn apply_uncommon_masks_with_masks(
    notes: &mut Vec<Note>,
    insert_mask: u8,
    remove_mask: u8,
    holds_mask: u8,
    timing_player: &TimingData,
    col_offset: usize,
    cols: usize,
    context_notes: &[Note],
    row_bounds: Option<(usize, usize)>,
    _player: usize,
) {
    if (remove_mask & REMOVE_MASK_BIT_LITTLE) != 0 {
        let rows_per_beat = ROWS_PER_BEAT.max(1) as usize;
        notes.retain(|note| note.row_index % rows_per_beat == 0);
    }

    if (holds_mask & HOLDS_MASK_BIT_NO_ROLLS) != 0 {
        for note in notes.iter_mut() {
            if note.note_type == NoteType::Roll {
                note.note_type = NoteType::Hold;
            }
        }
    }

    if (remove_mask & REMOVE_MASK_BIT_NO_HOLDS) != 0 {
        for note in notes.iter_mut() {
            if note.note_type == NoteType::Hold {
                note.note_type = NoteType::Tap;
                note.hold = None;
            }
        }
    }

    if (remove_mask & REMOVE_MASK_BIT_NO_MINES) != 0 {
        notes.retain(|note| !matches!(note.note_type, NoteType::Mine));
    }

    if (remove_mask & REMOVE_MASK_BIT_NO_JUMPS) != 0 {
        enforce_max_simultaneous_notes(notes, 1, col_offset, cols);
    }

    if (remove_mask & REMOVE_MASK_BIT_NO_FAKES) != 0 {
        notes.retain(|note| note.can_be_judged && !note.is_fake);
    }

    if (remove_mask & REMOVE_MASK_BIT_NO_HANDS) != 0 {
        enforce_max_simultaneous_notes(notes, 2, col_offset, cols);
    }

    if (remove_mask & REMOVE_MASK_BIT_NO_QUADS) != 0 {
        enforce_max_simultaneous_notes(notes, 3, col_offset, cols);
    }

    if (insert_mask & INSERT_MASK_BIT_BIG) != 0 {
        apply_insert_intelligent_taps(
            notes,
            timing_player,
            col_offset,
            cols,
            ROWS_PER_BEAT.max(1) as usize,
            (ROWS_PER_BEAT.max(1) / 2) as usize,
            ROWS_PER_BEAT.max(1) as usize,
            false,
        );
    }
    if (insert_mask & INSERT_MASK_BIT_QUICK) != 0 {
        apply_insert_intelligent_taps(
            notes,
            timing_player,
            col_offset,
            cols,
            (ROWS_PER_BEAT.max(1) / 2) as usize,
            (ROWS_PER_BEAT.max(1) / 4) as usize,
            ROWS_PER_BEAT.max(1) as usize,
            false,
        );
    }
    if (insert_mask & INSERT_MASK_BIT_BMRIZE) != 0 {
        apply_insert_intelligent_taps(
            notes,
            timing_player,
            col_offset,
            cols,
            ROWS_PER_BEAT.max(1) as usize,
            (ROWS_PER_BEAT.max(1) / 2) as usize,
            ROWS_PER_BEAT.max(1) as usize,
            false,
        );
        apply_insert_intelligent_taps(
            notes,
            timing_player,
            col_offset,
            cols,
            (ROWS_PER_BEAT.max(1) / 2) as usize,
            (ROWS_PER_BEAT.max(1) / 4) as usize,
            ROWS_PER_BEAT.max(1) as usize,
            false,
        );
    }
    if (insert_mask & INSERT_MASK_BIT_SKIPPY) != 0 {
        apply_insert_intelligent_taps(
            notes,
            timing_player,
            col_offset,
            cols,
            ROWS_PER_BEAT.max(1) as usize,
            ((ROWS_PER_BEAT.max(1) * 3) / 4) as usize,
            ROWS_PER_BEAT.max(1) as usize,
            true,
        );
    }
    if (insert_mask & INSERT_MASK_BIT_MINES) != 0
        && let Some((start_row, end_row)) = row_bounds
    {
        apply_mines_insert(
            notes,
            context_notes,
            timing_player,
            col_offset,
            cols,
            start_row,
            end_row,
        );
    }
    if (insert_mask & INSERT_MASK_BIT_ECHO) != 0 {
        apply_echo_insert(notes, timing_player, col_offset, cols);
    }
    if (insert_mask & INSERT_MASK_BIT_WIDE) != 0 {
        apply_wide_insert(notes, timing_player, col_offset, cols);
    }
    if (insert_mask & INSERT_MASK_BIT_STOMP) != 0 {
        apply_stomp_insert(notes, timing_player, col_offset, cols);
    }

    if (holds_mask & HOLDS_MASK_BIT_PLANTED) != 0 {
        convert_taps_to_holds(notes, timing_player, col_offset, cols, 1);
    }
    if (holds_mask & HOLDS_MASK_BIT_FLOORED) != 0 {
        convert_taps_to_holds(notes, timing_player, col_offset, cols, 2);
    }
    if (holds_mask & HOLDS_MASK_BIT_TWISTER) != 0 {
        convert_taps_to_holds(notes, timing_player, col_offset, cols, 3);
    }

    if (holds_mask & HOLDS_MASK_BIT_HOLDS_TO_ROLLS) != 0 {
        for note in notes.iter_mut() {
            if note.note_type == NoteType::Hold {
                note.note_type = NoteType::Roll;
            }
        }
    }
    if (remove_mask & REMOVE_MASK_BIT_NO_LIFTS) != 0 {
        notes.retain(|note| note.note_type != NoteType::Lift);
    }

    sort_player_notes(notes);
}

pub fn apply_uncommon_chart_transforms(
    notes: &mut Vec<Note>,
    note_ranges: &mut [(usize, usize); MAX_PLAYERS],
    cols_per_player: usize,
    num_players: usize,
    player_effects: &[ChartAttackEffects; MAX_PLAYERS],
    timing_players: &[&TimingData; MAX_PLAYERS],
) {
    if num_players == 0
        || !player_effects
            .iter()
            .take(num_players)
            .any(|effects| effects.has_note_masks())
    {
        return;
    }

    let mut transformed = Vec::with_capacity(notes.len());
    let mut transformed_ranges = [(0usize, 0usize); MAX_PLAYERS];

    for player in 0..num_players {
        let (start, end) = note_ranges[player];
        let slice_end = end.min(notes.len());
        let slice_start = start.min(slice_end);
        let out_start = transformed.len();
        let effects = player_effects[player];
        if !effects.has_note_masks() {
            transformed.extend_from_slice(&notes[slice_start..slice_end]);
            transformed_ranges[player] = (out_start, transformed.len());
            continue;
        }

        let mut player_notes = notes[slice_start..slice_end].to_vec();
        apply_uncommon_masks_with_masks(
            &mut player_notes,
            effects.insert_mask,
            effects.remove_mask,
            effects.holds_mask,
            timing_players[player],
            player.saturating_mul(cols_per_player),
            cols_per_player,
            &[],
            None,
            player,
        );
        transformed.extend(player_notes);
        transformed_ranges[player] = (out_start, transformed.len());
    }

    if num_players == 1 {
        transformed_ranges[1] = transformed_ranges[0];
    }

    *notes = transformed;
    *note_ranges = transformed_ranges;
}

fn turn_take_from(turn: GameplayTurnOption, cols: usize, seed: u64) -> Option<Vec<usize>> {
    if cols == 0 {
        return None;
    }
    match (turn, cols) {
        (GameplayTurnOption::None, _) => None,
        (GameplayTurnOption::Mirror, _) => Some((0..cols).rev().collect()),
        (GameplayTurnOption::LRMirror, 4) => Some(vec![3, 1, 2, 0]),
        (GameplayTurnOption::LRMirror, 8) => Some(vec![7, 5, 6, 4, 3, 1, 2, 0]),
        (GameplayTurnOption::UDMirror, 4) => Some(vec![0, 2, 1, 3]),
        (GameplayTurnOption::UDMirror, 8) => Some(vec![0, 2, 1, 3, 4, 6, 5, 7]),
        (GameplayTurnOption::Left, 4) => Some(vec![2, 0, 3, 1]),
        (GameplayTurnOption::Left, 8) => Some(vec![2, 0, 3, 1, 6, 4, 7, 5]),
        (GameplayTurnOption::Right, 4) => Some(vec![1, 3, 0, 2]),
        (GameplayTurnOption::Right, 8) => Some(vec![1, 3, 0, 2, 5, 7, 4, 6]),
        (GameplayTurnOption::Shuffle, _) => {
            let orig: Vec<usize> = (0..cols).collect();
            let mut attempt_seed = seed as u32;
            loop {
                let mut out = orig.clone();
                let mut rng = TurnRng::new(u64::from(attempt_seed));
                rng.shuffle(&mut out);
                if cols <= 1 || out != orig {
                    return Some(out);
                }
                attempt_seed = attempt_seed.wrapping_add(1);
            }
        }
        _ => None,
    }
}

pub fn apply_turn_permutation(
    notes: &mut [Note],
    note_range: (usize, usize),
    col_offset: usize,
    cols: usize,
    turn: GameplayTurnOption,
    seed: u64,
) {
    let Some(take_from) = turn_take_from(turn, cols, seed) else {
        return;
    };
    if take_from.len() != cols {
        return;
    }
    let mut old_to_new = vec![0usize; cols];
    for (new_col, &old_col) in take_from.iter().enumerate() {
        if old_col < cols {
            old_to_new[old_col] = new_col;
        }
    }
    let (start, end) = note_range;
    for n in &mut notes[start..end] {
        if n.column < col_offset {
            continue;
        }
        let local = n.column - col_offset;
        if local < cols {
            n.column = col_offset + old_to_new[local];
        }
    }
}

fn update_active_turn_holds_for_row(
    notes: &[Note],
    row_index: usize,
    grid: &[usize; MAX_COLS],
    cols: usize,
    hold_end_row: &mut [Option<usize>; MAX_COLS],
) {
    for hold_end in hold_end_row.iter_mut().take(cols.min(MAX_COLS)) {
        if let Some(end) = *hold_end
            && row_index > end
        {
            *hold_end = None;
        }
    }

    for (col, &idx) in grid.iter().enumerate().take(cols.min(MAX_COLS)) {
        if idx == usize::MAX {
            continue;
        }
        if matches!(notes[idx].note_type, NoteType::Hold | NoteType::Roll) {
            let end = notes[idx]
                .hold
                .as_ref()
                .map(|h| h.end_row_index)
                .unwrap_or(row_index);
            hold_end_row[col] = Some(end);
        }
    }
}

pub fn apply_super_shuffle_taps(
    notes: &mut [Note],
    note_range: (usize, usize),
    col_offset: usize,
    cols: usize,
    seed: u64,
) {
    if cols == 0 || cols > MAX_COLS {
        return;
    }
    let row_grids = build_row_grids(notes, note_range, col_offset, cols);
    let mut rng = TurnRng::new(seed);
    let mut hold_end_row: [Option<usize>; MAX_COLS] = [None; MAX_COLS];

    for row_grid in row_grids {
        let row = row_grid.row_index;
        let mut grid = row_grid.note_indices;
        update_active_turn_holds_for_row(notes, row, &grid, cols, &mut hold_end_row);

        for t1 in 0..cols {
            if hold_end_row[t1].is_some() {
                continue;
            }
            let idx1 = grid[t1];
            if idx1 == usize::MAX {
                continue;
            }
            if matches!(notes[idx1].note_type, NoteType::Hold | NoteType::Roll) {
                continue;
            }

            let mut tried_mask: u16 = 0;
            for _ in 0..4 {
                let t2 = rng.gen_range(cols);
                let bit = 1u16 << (t2 as u32);
                if (tried_mask & bit) != 0 {
                    continue;
                }
                tried_mask |= bit;
                if t1 == t2 {
                    break;
                }
                if hold_end_row[t2].is_some() {
                    continue;
                }
                let idx2 = grid[t2];
                if idx2 != usize::MAX
                    && matches!(notes[idx2].note_type, NoteType::Hold | NoteType::Roll)
                {
                    continue;
                }

                if idx2 == usize::MAX {
                    notes[idx1].column = col_offset + t2;
                    grid[t2] = idx1;
                    grid[t1] = usize::MAX;
                } else {
                    notes[idx1].column = col_offset + t2;
                    notes[idx2].column = col_offset + t1;
                    grid.swap(t1, t2);
                }
                break;
            }
        }
    }
}

pub fn apply_hyper_shuffle(
    notes: &mut [Note],
    note_range: (usize, usize),
    col_offset: usize,
    cols: usize,
    seed: u64,
) {
    if cols == 0 || cols > MAX_COLS {
        return;
    }
    let row_grids = build_row_grids(notes, note_range, col_offset, cols);
    let mut rng = TurnRng::new(seed);
    let mut hold_end_row: [Option<usize>; MAX_COLS] = [None; MAX_COLS];

    for row_grid in row_grids {
        let row = row_grid.row_index;
        let grid = row_grid.note_indices;
        for hold_end in hold_end_row.iter_mut().take(cols) {
            if let Some(end) = *hold_end
                && row > end
            {
                *hold_end = None;
            }
        }

        let mut free_cols = [0usize; MAX_COLS];
        let mut free_len = 0usize;
        for (col, hold_end) in hold_end_row.iter().enumerate().take(cols) {
            if hold_end.is_none() {
                free_cols[free_len] = col;
                free_len += 1;
            }
        }
        if free_len == 0 {
            continue;
        }

        let mut row_notes = [usize::MAX; MAX_COLS];
        let mut notes_len = 0usize;
        for (col, &idx) in grid.iter().enumerate().take(cols) {
            if hold_end_row[col].is_some() {
                continue;
            }
            if idx == usize::MAX {
                continue;
            }
            row_notes[notes_len] = idx;
            notes_len += 1;
        }
        if notes_len == 0 {
            continue;
        }

        rng.shuffle(&mut free_cols[..free_len]);
        let place_len = notes_len.min(free_len);
        for (&idx, &col) in row_notes.iter().zip(free_cols.iter()).take(place_len) {
            notes[idx].column = col_offset + col;
        }

        for &idx in row_notes.iter().take(place_len) {
            if !matches!(notes[idx].note_type, NoteType::Hold | NoteType::Roll) {
                continue;
            }
            let local = notes[idx].column.saturating_sub(col_offset);
            if local >= cols {
                continue;
            }
            let end = notes[idx]
                .hold
                .as_ref()
                .map(|h| h.end_row_index)
                .unwrap_or(row);
            hold_end_row[local] = Some(end);
        }
    }
}

pub fn apply_turn_options(
    notes: &mut [Note],
    note_ranges: [(usize, usize); MAX_PLAYERS],
    cols_per_player: usize,
    num_players: usize,
    player_turn_options: [GameplayTurnOption; MAX_PLAYERS],
    base_seed: u64,
) {
    for (player, turn) in player_turn_options
        .iter()
        .copied()
        .enumerate()
        .take(num_players.min(MAX_PLAYERS))
    {
        let note_range = note_ranges[player];
        let col_offset = player * cols_per_player;
        match turn {
            GameplayTurnOption::None => {}
            GameplayTurnOption::Blender => {
                apply_turn_permutation(
                    notes,
                    note_range,
                    col_offset,
                    cols_per_player,
                    GameplayTurnOption::Shuffle,
                    base_seed,
                );
                apply_super_shuffle_taps(
                    notes,
                    note_range,
                    col_offset,
                    cols_per_player,
                    base_seed ^ (0xD00D_F00D_u64.wrapping_mul(player as u64 + 1)),
                );
            }
            GameplayTurnOption::Random => {
                apply_hyper_shuffle(
                    notes,
                    note_range,
                    col_offset,
                    cols_per_player,
                    base_seed ^ (0xA5A5_5A5A_u64.wrapping_mul(player as u64 + 1)),
                );
            }
            other => {
                apply_turn_permutation(
                    notes,
                    note_range,
                    col_offset,
                    cols_per_player,
                    other,
                    base_seed,
                );
            }
        }
    }
}

#[inline(always)]
pub fn mine_window_bounds_ns(
    mine_times_ns: &[SongTimeNs],
    start_t_ns: SongTimeNs,
    end_t_ns: SongTimeNs,
) -> (usize, usize) {
    (
        mine_times_ns.partition_point(|&t| t < start_t_ns),
        mine_times_ns.partition_point(|&t| t <= end_t_ns),
    )
}

#[inline(always)]
pub fn lane_note_window_bounds_ns(
    note_indices: &[usize],
    note_times_ns: &[SongTimeNs],
    start_t_ns: SongTimeNs,
    end_t_ns: SongTimeNs,
) -> (usize, usize) {
    (
        note_indices.partition_point(|&note_index| note_times_ns[note_index] < start_t_ns),
        note_indices.partition_point(|&note_index| note_times_ns[note_index] <= end_t_ns),
    )
}

#[inline(always)]
pub fn lane_note_window_bounds_rows(
    note_indices: &[usize],
    notes: &[Note],
    start_row: usize,
    end_row: usize,
) -> (usize, usize) {
    (
        note_indices.partition_point(|&note_index| notes[note_index].row_index < start_row),
        note_indices.partition_point(|&note_index| notes[note_index].row_index < end_row),
    )
}

#[inline(always)]
pub fn timing_row_nearest(timing: &TimingData, beat: f32) -> usize {
    timing.get_row_for_beat(beat).unwrap_or(0)
}

#[inline(always)]
pub fn step_search_row_bounds(
    timing: &TimingData,
    current_time_ns: SongTimeNs,
    current_row_index: usize,
) -> (usize, usize) {
    let forward_time_ns = song_time_ns_add_seconds(current_time_ns, STEP_SEARCH_DISTANCE_SECONDS);
    let backward_time_ns = song_time_ns_add_seconds(current_time_ns, -STEP_SEARCH_DISTANCE_SECONDS);
    let forward_row = timing_row_nearest(timing, timing.get_beat_for_time_ns(forward_time_ns));
    let backward_row = timing_row_nearest(timing, timing.get_beat_for_time_ns(backward_time_ns));
    let step_rows = forward_row
        .saturating_sub(current_row_index)
        .max(current_row_index.saturating_sub(backward_row))
        .saturating_add(ROWS_PER_BEAT.max(1) as usize);
    (
        current_row_index.saturating_sub(step_rows),
        current_row_index.saturating_add(step_rows),
    )
}

#[inline(always)]
pub fn closest_lane_note_ns(
    note_indices: &[usize],
    notes: &[Note],
    note_times_ns: &[SongTimeNs],
    timing: &TimingData,
    current_time_ns: SongTimeNs,
    current_row_index: usize,
    search_start_idx: usize,
    search_end_idx: usize,
) -> Option<(usize, SongTimeNs)> {
    let mut best: Option<(usize, SongTimeNs)> = None;
    let mut best_row_distance = usize::MAX;
    let mut best_row_index = 0usize;
    for &note_index in &note_indices[search_start_idx..search_end_idx] {
        let note = &notes[note_index];
        let mine_already_judged =
            matches!(note.note_type, NoteType::Mine) && note.mine_result.is_some();
        let fake_note_blocks = note.is_fake && timing.is_judgable_at_beat(note.beat);
        if note.result.is_some() || mine_already_judged || !(note.can_be_judged || fake_note_blocks)
        {
            continue;
        }
        let row_distance = current_row_index.abs_diff(note.row_index);
        let signed_err_music = current_time_ns as i128 - note_times_ns[note_index] as i128;
        // Match ITGmania Player::GetClosestNote: choose by row proximity, and
        // break exact ties toward the later row.
        match best {
            Some(_) if row_distance > best_row_distance => {}
            Some(_) if row_distance == best_row_distance && note.row_index <= best_row_index => {}
            _ => {
                best = Some((note_index, signed_err_music as SongTimeNs));
                best_row_distance = row_distance;
                best_row_index = note.row_index;
            }
        }
    }
    best
}

#[inline(always)]
pub fn crossed_mine_bounds_ns(
    mine_times_ns: &[SongTimeNs],
    prev_time_ns: SongTimeNs,
    current_time_ns: SongTimeNs,
) -> (usize, usize) {
    (
        mine_times_ns.partition_point(|&t| t <= prev_time_ns),
        mine_times_ns.partition_point(|&t| t <= current_time_ns),
    )
}

#[inline(always)]
pub fn crossed_held_mine_can_hit(note: &Note, column: usize) -> bool {
    matches!(note.note_type, NoteType::Mine)
        && note.can_be_judged
        && note.mine_result.is_none()
        && !note.is_fake
        && note.column == column
}

#[inline(always)]
pub fn crossed_mine_held_start_time(
    now_down: bool,
    was_down: bool,
    pressed_since_ns: Option<SongTimeNs>,
    previous_music_time_ns: SongTimeNs,
    current_music_time_ns: SongTimeNs,
) -> Option<SongTimeNs> {
    if !now_down
        || song_time_ns_invalid(previous_music_time_ns)
        || song_time_ns_invalid(current_music_time_ns)
        || current_music_time_ns <= previous_music_time_ns
    {
        return None;
    }
    if was_down {
        return Some(previous_music_time_ns);
    }
    let pressed_since_ns = pressed_since_ns?;
    if song_time_ns_invalid(pressed_since_ns) || pressed_since_ns >= current_music_time_ns {
        return None;
    }
    Some(pressed_since_ns.max(previous_music_time_ns))
}

#[inline(always)]
pub const fn note_tracks_held_miss(note_type: NoteType) -> bool {
    matches!(note_type, NoteType::Tap | NoteType::Hold | NoteType::Roll)
}

pub fn track_held_miss_window_for_player(
    notes: &[Note],
    note_times_ns: &[SongTimeNs],
    tap_miss_held_window: &mut [bool],
    note_range: (usize, usize),
    col_range: (usize, usize),
    next_tap_miss_cursor: usize,
    inputs: &[bool; MAX_COLS],
    music_time_ns: SongTimeNs,
    largest_window_ns: SongTimeNs,
) {
    if largest_window_ns <= 0 {
        return;
    }
    let note_end = note_range
        .1
        .min(notes.len())
        .min(note_times_ns.len())
        .min(tap_miss_held_window.len());
    let mut cursor = next_tap_miss_cursor.max(note_range.0.min(note_end));
    let col_start = col_range.0.min(MAX_COLS);
    let col_end = col_range.1.min(MAX_COLS).max(col_start);
    let future_cutoff_time_ns = music_time_ns.saturating_add(largest_window_ns);
    let mut seen_tracks = [false; MAX_COLS];

    while cursor < note_end {
        let note_time_ns = note_times_ns[cursor];
        if note_time_ns > future_cutoff_time_ns {
            break;
        }
        let note = &notes[cursor];
        if !note.can_be_judged
            || note.result.is_some()
            || note.column < col_start
            || note.column >= col_end
            || !note_tracks_held_miss(note.note_type)
        {
            cursor += 1;
            continue;
        }
        let local_track = note.column - col_start;
        if seen_tracks[local_track] {
            cursor += 1;
            continue;
        }
        let offset_ns = (note_time_ns as i128 - music_time_ns as i128).unsigned_abs();
        if offset_ns > largest_window_ns as u128 {
            cursor += 1;
            continue;
        }
        seen_tracks[local_track] = true;
        if inputs[note.column] {
            tap_miss_held_window[cursor] = true;
        }
        cursor += 1;
    }
}

#[inline(always)]
pub fn collect_edge_judge_indices(
    row_note_count: usize,
    lead_note_index: usize,
) -> Option<([usize; MAX_COLS], usize)> {
    if row_note_count == 0 {
        return None;
    }
    let mut judge_indices = [usize::MAX; MAX_COLS];
    judge_indices[0] = lead_note_index;
    Some((judge_indices, 1))
}

#[inline(always)]
pub fn quantization_index_from_beat(beat: f32) -> u8 {
    // Match ITG's BeatToNoteType path: round beat->row at 48 rows/beat,
    // then classify by measure-subdivision divisibility.
    let row = (beat * 48.0).round() as i32;
    if row.rem_euclid(48) == 0 {
        QUANT_4TH
    } else if row.rem_euclid(24) == 0 {
        QUANT_8TH
    } else if row.rem_euclid(16) == 0 {
        QUANT_12TH
    } else if row.rem_euclid(12) == 0 {
        QUANT_16TH
    } else if row.rem_euclid(8) == 0 {
        QUANT_24TH
    } else if row.rem_euclid(6) == 0 {
        QUANT_32ND
    } else if row.rem_euclid(4) == 0 {
        QUANT_48TH
    } else if row.rem_euclid(3) == 0 {
        QUANT_64TH
    } else {
        QUANT_192ND
    }
}

#[derive(Clone, Copy, Debug)]
pub struct RecordedLaneEdge {
    pub lane_index: u8,
    pub pressed: bool,
    pub source: InputSource,
    pub event_music_time_ns: SongTimeNs,
}

#[derive(Clone, Copy, Debug)]
pub struct ReplayInputEdge {
    pub lane_index: u8,
    pub pressed: bool,
    pub source: InputSource,
    pub event_music_time_ns: SongTimeNs,
}

#[derive(Clone, Copy, Debug)]
pub struct ReplayOffsetSnapshot {
    pub beat0_time_ns: SongTimeNs,
}

pub fn build_replay_input_edges(
    replay_edges: &[ReplayInputEdge],
    num_players: usize,
    cols_per_player: usize,
    num_cols: usize,
    recorded_beat0_time_ns: SongTimeNs,
    current_beat0_time_ns: [SongTimeNs; MAX_PLAYERS],
) -> Vec<RecordedLaneEdge> {
    let mut replay_input = Vec::with_capacity(replay_edges.len());
    let mut out_of_order = false;
    let mut prev_time_ns = None;

    for edge in replay_edges {
        let lane = edge.lane_index as usize;
        if lane >= num_cols || song_time_ns_invalid(edge.event_music_time_ns) {
            continue;
        }

        let player = player_index_for_column(num_players, cols_per_player, lane);
        let player_beat0_time_ns = current_beat0_time_ns[player];
        let replay_beat0_shift_ns = if song_time_ns_invalid(recorded_beat0_time_ns)
            || song_time_ns_invalid(player_beat0_time_ns)
        {
            0
        } else {
            player_beat0_time_ns.saturating_sub(recorded_beat0_time_ns)
        };
        let event_music_time_ns = edge
            .event_music_time_ns
            .saturating_add(replay_beat0_shift_ns);

        if prev_time_ns.is_some_and(|prev| event_music_time_ns < prev) {
            out_of_order = true;
        }
        prev_time_ns = Some(event_music_time_ns);
        replay_input.push(RecordedLaneEdge {
            lane_index: edge.lane_index,
            pressed: edge.pressed,
            source: edge.source,
            event_music_time_ns,
        });
    }

    if out_of_order {
        replay_input.sort_by_key(|edge| edge.event_music_time_ns);
    }
    replay_input
}

pub fn next_ready_replay_edge(
    replay_input: &[RecordedLaneEdge],
    replay_cursor: &mut usize,
    current_music_time_ns: SongTimeNs,
) -> Option<RecordedLaneEdge> {
    let edge = replay_input.get(*replay_cursor).copied()?;
    if edge.event_music_time_ns > current_music_time_ns {
        return None;
    }
    *replay_cursor = replay_cursor.saturating_add(1);
    Some(edge)
}

#[derive(Clone, Copy, Debug)]
pub struct ErrorBarTick {
    pub started_at: f32,
    pub offset_s: f32,
    pub window: TimingWindow,
}

#[derive(Clone, Copy, Debug)]
pub struct ErrorBarText {
    pub started_at: f32,
    pub early: bool,
    pub offset_ms: f32,
    pub scaled: bool,
    pub scale_start_ms: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct OffsetIndicatorText {
    pub started_at: f32,
    pub offset_ms: f32,
    pub window: TimingWindow,
}

pub const AVERAGE_ERROR_BAR_INTERVAL_MS_MIN: u32 = 100;
pub const AVERAGE_ERROR_BAR_INTERVAL_MS_MAX: u32 = 2000;
pub const AVERAGE_ERROR_BAR_INTERVAL_MS_STEP: u32 = 100;
pub const ERROR_BAR_LONG_AVG_SAMPLE_FILTER_S: f32 = 0.060;
pub const ERROR_BAR_LONG_AVG_PRUNE_PER_TAP: usize = 4;

#[inline(always)]
pub const fn clamp_average_error_bar_interval_ms(ms: u32) -> u32 {
    let clamped = if ms < AVERAGE_ERROR_BAR_INTERVAL_MS_MIN {
        AVERAGE_ERROR_BAR_INTERVAL_MS_MIN
    } else if ms > AVERAGE_ERROR_BAR_INTERVAL_MS_MAX {
        AVERAGE_ERROR_BAR_INTERVAL_MS_MAX
    } else {
        ms
    };
    let steps = (clamped - AVERAGE_ERROR_BAR_INTERVAL_MS_MIN
        + AVERAGE_ERROR_BAR_INTERVAL_MS_STEP / 2)
        / AVERAGE_ERROR_BAR_INTERVAL_MS_STEP;
    AVERAGE_ERROR_BAR_INTERVAL_MS_MIN + steps * AVERAGE_ERROR_BAR_INTERVAL_MS_STEP
}

#[inline(always)]
pub const fn error_bar_window_ix(window: TimingWindow) -> usize {
    match window {
        TimingWindow::W0 => 0,
        TimingWindow::W1 => 1,
        TimingWindow::W2 => 2,
        TimingWindow::W3 => 3,
        TimingWindow::W4 => 4,
        TimingWindow::W5 => 5,
    }
}

#[inline(always)]
pub fn error_bar_long_term_offset_s(
    samples: &mut VecDeque<(f32, f32)>,
    total: &mut f32,
    music_time_s: f32,
    offset_s: f32,
    average_window_ms: u32,
) -> (f32, usize) {
    let now_ms = (music_time_s * 1000.0).max(0.0);
    if offset_s.abs() <= ERROR_BAR_LONG_AVG_SAMPLE_FILTER_S {
        samples.push_back((now_ms, offset_s));
        *total += offset_s;
    }

    let long_window_ms = clamp_average_error_bar_interval_ms(average_window_ms) as f32 * 16.0;
    let mut popped = 0usize;
    while popped < ERROR_BAR_LONG_AVG_PRUNE_PER_TAP {
        let Some((time_ms, _)) = samples.front() else {
            break;
        };
        if now_ms - *time_ms <= long_window_ms {
            break;
        }
        if let Some((_, v)) = samples.pop_front() {
            *total -= v;
            popped += 1;
        } else {
            break;
        }
    }

    let len = samples.len();
    let mean = if len > 0 { *total / len as f32 } else { 0.0 };
    (mean, len)
}

#[inline(always)]
pub fn error_bar_push_tick<const N: usize>(
    ticks: &mut [Option<ErrorBarTick>; N],
    next: &mut usize,
    multi_tick: bool,
    tick: ErrorBarTick,
) {
    let ix = if multi_tick {
        let ix = (*next) % N;
        *next = (*next + 1) % N;
        ix
    } else {
        0
    };
    ticks[ix] = Some(tick);
    if !multi_tick {
        *next = 0;
    }
}

#[inline(always)]
pub fn error_bar_average_offset_s(
    samples: &mut VecDeque<(f32, f32)>,
    music_time_s: f32,
    offset_s: f32,
    window_ms: u32,
) -> (f32, usize) {
    let now_ms = ((music_time_s * 100.0).round() * 10.0).max(0.0);
    samples.push_back((now_ms, offset_s));

    let window_ms = clamp_average_error_bar_interval_ms(window_ms) as f32;
    while let Some((t, _)) = samples.front() {
        if now_ms - *t <= window_ms {
            break;
        }
        samples.pop_front();
    }

    let mut sum = 0.0_f32;
    let mut count: usize = 0;
    let mut oldest_in_window: Option<f32> = None;
    for &(t, v) in samples.iter().rev() {
        if now_ms - t > window_ms {
            break;
        }
        sum += v;
        count += 1;
        oldest_in_window = Some(v);
    }
    if count == 0 {
        return (offset_s, 1);
    }
    if count > 1
        && (count & 1) == 1
        && let Some(oldest) = oldest_in_window
    {
        sum -= oldest;
        count -= 1;
    }
    let avg = sum / (count.max(1) as f32);
    (avg, count)
}

#[derive(Clone, Copy, Debug)]
pub struct NoteCountStat {
    pub beat: f32,
    pub notes_lower: usize,
    pub notes_upper: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExitTransitionKind {
    Out,
    Cancel,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GameplayExit {
    Complete,
    Cancel,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GameplayAction {
    None,
    Navigate(GameplayExit),
    NavigateNoFade(GameplayExit),
}

#[derive(Clone, Copy, Debug)]
pub struct ExitTransition {
    pub kind: ExitTransitionKind,
    pub started_at: Instant,
}

#[inline(always)]
pub const fn hold_to_exit_seconds(key: HoldToExitKey) -> f32 {
    match key {
        HoldToExitKey::Start => GIVE_UP_HOLD_SECONDS,
        HoldToExitKey::Back => BACK_OUT_HOLD_SECONDS,
    }
}

#[inline(always)]
pub const fn exit_total_seconds(kind: ExitTransitionKind) -> f32 {
    match kind {
        ExitTransitionKind::Out => GIVE_UP_OUT_FADE_DELAY_SECONDS + GIVE_UP_OUT_FADE_SECONDS,
        ExitTransitionKind::Cancel => BACK_OUT_FADE_DELAY_SECONDS + BACK_OUT_FADE_SECONDS,
    }
}

#[inline(always)]
pub fn exit_transition_alpha_elapsed(kind: ExitTransitionKind, elapsed_s: f32) -> f32 {
    let (delay, fade) = match kind {
        ExitTransitionKind::Out => (GIVE_UP_OUT_FADE_DELAY_SECONDS, GIVE_UP_OUT_FADE_SECONDS),
        ExitTransitionKind::Cancel => (BACK_OUT_FADE_DELAY_SECONDS, BACK_OUT_FADE_SECONDS),
    };
    if fade <= 0.0 {
        return 1.0;
    }
    let alpha = if elapsed_s <= delay {
        0.0
    } else {
        (elapsed_s - delay) / fade
    };
    alpha.clamp(0.0, 1.0)
}

#[inline(always)]
pub fn exit_transition_alpha(exit: &ExitTransition) -> f32 {
    exit_transition_alpha_elapsed(exit.kind, exit.started_at.elapsed().as_secs_f32())
}

#[inline(always)]
pub const fn gameplay_exit_for_kind(kind: ExitTransitionKind) -> GameplayExit {
    match kind {
        ExitTransitionKind::Out => GameplayExit::Complete,
        ExitTransitionKind::Cancel => GameplayExit::Cancel,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use deadsync_chart::{ArrowStats, ChartData, StaminaCounts, TechCounts};
    use deadsync_core::song_time::{
        INVALID_SONG_TIME_NS, song_time_ns_from_seconds, song_time_ns_to_seconds,
    };
    use deadsync_core::timing::ROWS_PER_BEAT;
    use deadsync_rules::note::{HoldData, Note};
    use deadsync_rules::timing::{DelaySegment, FakeSegment, StopSegment, TimingSegments};
    use std::collections::VecDeque;
    use std::path::PathBuf;

    fn assert_near(actual: f32, expected: f32) {
        assert!(
            (actual - expected).abs() <= 0.000_001,
            "expected {expected}, got {actual}"
        );
    }

    #[test]
    fn player_life_dead_policy_uses_failing_or_empty_life() {
        assert!(player_life_is_dead(1.0, true));
        assert!(player_life_is_dead(0.0, false));
        assert!(player_life_is_dead(-0.1, false));
        assert!(!player_life_is_dead(0.1, false));
    }

    #[test]
    fn course_submit_life_eligibility_uses_optional_meter_state() {
        assert!(course_submit_life_eligible(None));
        assert!(course_submit_life_eligible(Some(
            &deadsync_rules::life::LifeMeter::course_submit_start()
        )));
        assert!(!course_submit_life_eligible(Some(
            &deadsync_rules::life::LifeMeter {
                is_failing: true,
                ..deadsync_rules::life::LifeMeter::course_submit_start()
            }
        )));
        assert!(!course_submit_life_eligible(Some(
            &deadsync_rules::life::LifeMeter {
                fail_time: Some(12.0),
                ..deadsync_rules::life::LifeMeter::course_submit_start()
            }
        )));
        assert!(!course_submit_life_eligible(Some(
            &deadsync_rules::life::LifeMeter {
                life: 0.0,
                ..deadsync_rules::life::LifeMeter::course_submit_start()
            }
        )));
    }

    #[test]
    fn gameplay_life_delta_records_history_for_life_changes() {
        let mut meter = deadsync_rules::life::LifeMeter::new(0.5);
        let mut history = vec![(0.0, 0.5)];

        let update = apply_gameplay_life_delta(
            &mut meter,
            &mut history,
            None,
            12.0,
            deadsync_rules::life::LIFE_GREAT,
        );

        assert_eq!(update, GameplayLifeDeltaUpdate::default());
        assert_near(meter.life, 0.504);
        assert_eq!(history, vec![(0.0, 0.5), (12.0, 0.504)]);
    }

    #[test]
    fn gameplay_life_delta_reports_failure_and_updates_course_submit_life() {
        let mut meter = deadsync_rules::life::LifeMeter::new(1.0);
        let mut course_submit_life = deadsync_rules::life::LifeMeter::course_submit_start();
        let mut history = vec![(0.0, 1.0)];

        let update = apply_gameplay_life_delta(
            &mut meter,
            &mut history,
            Some(&mut course_submit_life),
            12.0,
            -0.6,
        );

        assert!(!update.failed_now);
        assert!(!update.was_dead);
        assert_near(meter.life, 0.4);
        assert!(!meter.is_failing);
        assert_eq!(meter.fail_time, None);
        assert_eq!(course_submit_life.life, 0.0);
        assert!(course_submit_life.is_failing);
        assert_eq!(course_submit_life.fail_time, Some(12.0));
    }

    #[test]
    fn gameplay_life_delta_reports_active_meter_failure() {
        let mut meter = deadsync_rules::life::LifeMeter::new(0.05);
        let mut history = vec![(0.0, 0.05)];

        let update = apply_gameplay_life_delta(
            &mut meter,
            &mut history,
            None,
            8.0,
            deadsync_rules::life::LIFE_MISS,
        );

        assert!(update.failed_now);
        assert!(!update.was_dead);
        assert_eq!(meter.life, 0.0);
        assert!(meter.is_failing);
        assert_eq!(meter.fail_time, Some(8.0));
        assert_eq!(history, vec![(0.0, 0.05), (8.0, 0.0)]);
    }

    #[test]
    fn gameplay_life_delta_clamps_already_dead_meter() {
        let mut meter = deadsync_rules::life::LifeMeter {
            life: -0.25,
            combo_after_miss: 3,
            is_failing: false,
            fail_time: None,
        };
        let mut history = vec![(0.0, 0.0)];

        let update = apply_gameplay_life_delta(
            &mut meter,
            &mut history,
            None,
            8.0,
            deadsync_rules::life::LIFE_GREAT,
        );

        assert!(!update.failed_now);
        assert!(update.was_dead);
        assert_eq!(meter.life, 0.0);
        assert!(meter.is_failing);
        assert_eq!(meter.fail_time, None);
        assert_eq!(history, vec![(0.0, 0.0)]);
    }

    fn song_lua_ease_mask_window(
        target: SongLuaEaseMaskTarget,
        start_second: f32,
        end_second: f32,
        sustain_end_second: f32,
        from: f32,
        to: f32,
    ) -> SongLuaEaseMaskWindow {
        SongLuaEaseMaskWindow {
            start_second,
            end_second,
            sustain_end_second,
            target,
            from,
            to,
            easing: None,
            opt1: None,
            opt2: None,
        }
    }

    fn song_lua_column_offset_window(
        column: usize,
        start_second: f32,
        end_second: f32,
        sustain_end_second: f32,
    ) -> SongLuaColumnOffsetWindowRuntime {
        SongLuaColumnOffsetWindowRuntime {
            column,
            start_second,
            end_second,
            sustain_end_second,
            from_y: 0.0,
            to_y: 64.0,
            easing: None,
            opt1: None,
            opt2: None,
        }
    }

    fn song_lua_overlay_ease_window(
        overlay_index: usize,
        start_second: f32,
        end_second: f32,
        sustain_end_second: f32,
        cutoff_second: Option<f32>,
    ) -> SongLuaOverlayEaseWindowRuntime<u8> {
        SongLuaOverlayEaseWindowRuntime {
            overlay_index,
            start_second,
            end_second,
            sustain_end_second,
            cutoff_second,
            from: 1,
            to: 2,
            easing: None,
            opt1: None,
            opt2: None,
        }
    }

    fn attack_mask_window(
        start_second: f32,
        end_second: f32,
        mods: ParsedAttackMods,
    ) -> AttackMaskWindow {
        attack_mask_window_from_parts(
            &ChartAttackWindow {
                start_second,
                len_seconds: end_second - start_second,
                mods: String::new(),
            },
            mods,
        )
        .expect("test attack mask window must have an effect")
    }

    #[test]
    fn exit_timing_matches_screen_policy() {
        assert_eq!(hold_to_exit_seconds(HoldToExitKey::Start), 0.33);
        assert_eq!(hold_to_exit_seconds(HoldToExitKey::Back), 1.0);

        assert_eq!(exit_total_seconds(ExitTransitionKind::Out), 1.5);
        assert_eq!(exit_total_seconds(ExitTransitionKind::Cancel), 0.5);

        assert_eq!(
            gameplay_exit_for_kind(ExitTransitionKind::Out),
            GameplayExit::Complete
        );
        assert_eq!(
            gameplay_exit_for_kind(ExitTransitionKind::Cancel),
            GameplayExit::Cancel
        );
    }

    #[test]
    fn exit_alpha_respects_delay_and_fade() {
        assert_near(
            exit_transition_alpha_elapsed(ExitTransitionKind::Out, 0.5),
            0.0,
        );
        assert_near(
            exit_transition_alpha_elapsed(ExitTransitionKind::Out, 1.0),
            0.5,
        );
        assert_near(
            exit_transition_alpha_elapsed(ExitTransitionKind::Cancel, 0.3),
            0.5,
        );
        assert_near(
            exit_transition_alpha_elapsed(ExitTransitionKind::Cancel, 9.0),
            1.0,
        );
    }

    #[test]
    fn notefield_viewport_policy_matches_runtime_layout() {
        assert_near(RECEPTOR_Y_OFFSET_FROM_CENTER, -125.0);
        assert_near(RECEPTOR_Y_OFFSET_FROM_CENTER_REVERSE, 145.0);

        assert_near(scroll_receptor_y(0.0, 0.0, 100.0, 500.0, 300.0), 100.0);
        assert_near(scroll_receptor_y(1.0, 0.0, 100.0, 500.0, 300.0), 500.0);
        assert_near(scroll_receptor_y(0.5, 0.0, 100.0, 500.0, 300.0), 300.0);
        assert_near(scroll_receptor_y(0.0, 1.0, 100.0, 500.0, 300.0), 300.0);
        assert_near(scroll_receptor_y(0.0, 2.0, 100.0, 500.0, 300.0), 500.0);
    }

    #[test]
    fn draw_distances_scale_by_viewport_and_centered_scroll() {
        assert_near(
            draw_distance_before_targets(480.0, 1.0),
            480.0 * DRAW_DISTANCE_BEFORE_TARGETS_MULTIPLIER,
        );
        assert_near(draw_distance_before_targets(480.0, 1.5), 1080.0);
        assert_near(
            draw_distance_after_targets(480.0, 1.0, 0.0),
            DRAW_DISTANCE_AFTER_TARGETS,
        );
        assert_near(draw_distance_after_targets(480.0, 1.0, 1.0), 288.0);
        assert_near(draw_distance_after_targets(480.0, 1.0, 0.5), 209.0);
        assert_near(draw_distance_after_targets(480.0, 1.0, 2.0), 288.0);
    }

    #[test]
    fn step_stats_notefield_width_matches_sl_style_widths() {
        assert_eq!(step_stats_notefield_width(4), Some(256.0));
        assert_eq!(step_stats_notefield_width(8), Some(512.0));
        assert_eq!(step_stats_notefield_width(0), None);
    }

    #[test]
    fn step_stats_upper_density_width_matches_zmod_policy() {
        assert_near(
            step_stats_upper_density_graph_width(StepStatsPlayStyle::Single),
            226.0,
        );
        assert_near(
            step_stats_upper_density_graph_width(StepStatsPlayStyle::Versus),
            226.0,
        );
        assert_near(
            step_stats_upper_density_graph_width(StepStatsPlayStyle::Double),
            226.0,
        );
    }

    #[test]
    fn step_stats_density_graph_width_matches_sl_double() {
        let width = step_stats_density_graph_width(
            StepStatsPlayStyle::Double,
            8,
            1,
            854.0,
            480.0,
            true,
            false,
        );
        let expected = ((854.0 - 512.0) * 0.5) * 0.95;
        assert_near(width, expected);
    }

    #[test]
    fn step_stats_density_graph_width_handles_centered_and_ultrawide() {
        assert_near(
            step_stats_density_graph_width(
                StepStatsPlayStyle::Single,
                4,
                1,
                854.0,
                480.0,
                true,
                true,
            ),
            299.0,
        );
        assert_near(
            step_stats_density_graph_width(
                StepStatsPlayStyle::Versus,
                4,
                2,
                2560.0,
                1080.0,
                true,
                false,
            ),
            512.0,
        );
    }

    #[test]
    fn density_life_points_skip_duplicate_and_update_same_time() {
        let mut points = vec![[1.0, 0.25]];

        assert!(!push_density_life_point(&mut points, 1.0, 0.25));
        assert_eq!(points, vec![[1.0, 0.25]]);

        assert!(push_density_life_point(&mut points, 1.0, 0.5));
        assert_eq!(points, vec![[1.0, 0.5]]);
    }

    #[test]
    fn density_life_points_replace_nearly_straight_segments() {
        let mut points = vec![[0.0, 0.0], [1.0, 0.1]];

        assert!(push_density_life_point(&mut points, 2.0, 0.2));

        assert_eq!(points.len(), 2);
        assert_near(points[1][0], 2.0);
        assert_near(points[1][1], 0.2);
    }

    #[test]
    fn density_life_points_keep_sharp_turns() {
        let mut points = vec![[0.0, 0.0], [1.0, 0.0]];

        assert!(push_density_life_point(&mut points, 2.0, 1.0));

        assert_eq!(points, vec![[0.0, 0.0], [1.0, 0.0], [2.0, 1.0]]);
    }

    #[test]
    fn reference_bpm_prefers_chart_display_max() {
        let display_bpm = ChartDisplayBpm::Specified {
            min: 100.0,
            max: 180.0,
        };

        assert_eq!(
            reference_bpm_from_display_tag(Some(&display_bpm), "120"),
            Some(180.0)
        );
    }

    #[test]
    fn reference_bpm_random_chart_display_suppresses_fallback() {
        assert_eq!(
            reference_bpm_from_display_tag(Some(&ChartDisplayBpm::Random), "100:160"),
            None
        );
    }

    #[test]
    fn reference_bpm_uses_song_range_max_and_single_value() {
        assert_eq!(reference_bpm_from_display_tag(None, "100:160"), Some(160.0));
        assert_eq!(reference_bpm_from_display_tag(None, " 145 "), Some(145.0));
    }

    #[test]
    fn reference_bpm_ignores_empty_star_and_invalid_chart_max() {
        let invalid_chart = ChartDisplayBpm::Specified {
            min: 100.0,
            max: f64::NAN,
        };

        assert_eq!(reference_bpm_from_display_tag(None, ""), None);
        assert_eq!(reference_bpm_from_display_tag(None, "*"), None);
        assert_eq!(
            reference_bpm_from_display_tag(Some(&invalid_chart), "125"),
            Some(125.0)
        );
    }

    #[test]
    fn song_lua_compile_player_screen_x_places_two_players() {
        let viewport = GameplayViewport::design();

        assert_near(
            song_lua_compile_player_screen_x(
                2,
                0,
                viewport,
                SongLuaCompilePlayStyle::Versus,
                false,
                20.0,
                false,
            ),
            193.5,
        );
        assert_near(
            song_lua_compile_player_screen_x(
                2,
                1,
                viewport,
                SongLuaCompilePlayStyle::Versus,
                false,
                20.0,
                false,
            ),
            660.5,
        );
    }

    #[test]
    fn song_lua_compile_player_screen_x_centers_single_and_double() {
        let viewport = GameplayViewport::design();

        assert_near(
            song_lua_compile_player_screen_x(
                1,
                0,
                viewport,
                SongLuaCompilePlayStyle::Single,
                false,
                50.0,
                true,
            ),
            viewport.center_x(),
        );
        assert_near(
            song_lua_compile_player_screen_x(
                1,
                0,
                viewport,
                SongLuaCompilePlayStyle::Double,
                true,
                50.0,
                false,
            ),
            viewport.center_x(),
        );
    }

    #[test]
    fn song_lua_compile_player_screen_x_uses_side_and_offset_policy() {
        let viewport = GameplayViewport::design();

        assert_near(
            song_lua_compile_player_screen_x(
                1,
                0,
                viewport,
                SongLuaCompilePlayStyle::Single,
                false,
                10.0,
                false,
            ),
            203.5,
        );
        assert_near(
            song_lua_compile_player_screen_x(
                1,
                0,
                viewport,
                SongLuaCompilePlayStyle::Single,
                true,
                999.0,
                false,
            ),
            690.5,
        );
    }

    #[test]
    fn mini_value_uses_fallback_big_adjustment_and_clamps() {
        assert_near(mini_value_for_percent(50.0, 0.0, false), 0.5);
        assert_near(mini_value_for_percent(f32::NAN, 25.0, false), 0.25);
        assert_near(mini_value_for_percent(50.0, 0.0, true), -0.5);
        assert_near(mini_value_for_percent(-250.0, 0.0, false), -1.0);
        assert_near(mini_value_for_percent(250.0, 0.0, false), 1.5);
    }

    #[test]
    fn effective_mini_percent_uses_active_fallback_and_clear_all() {
        assert_eq!(MINI_PERCENT_MIN, -100.0);
        assert_eq!(MINI_PERCENT_MAX, 150.0);
        assert_near(effective_mini_percent(Some(25.0), 50.0, false), 25.0);
        assert_near(effective_mini_percent(Some(f32::NAN), 50.0, false), 50.0);
        assert_near(effective_mini_percent(None, 50.0, true), 0.0);
        assert_near(effective_mini_percent(None, 50.0, false), 50.0);
        assert_near(effective_mini_percent(Some(250.0), 0.0, false), 150.0);
        assert_near(effective_mini_percent(Some(-250.0), 0.0, false), -100.0);
    }

    #[test]
    fn mini_attack_target_supports_absolute_and_delta_modes() {
        assert_near(
            attack_mini_target_percent(25.0, MiniAttackMode::Absolute, 50.0),
            25.0,
        );
        assert_near(
            attack_mini_target_percent(25.0, MiniAttackMode::Delta, 50.0),
            75.0,
        );
    }

    #[test]
    fn attack_value_approaches_or_snaps_to_target() {
        let mut current = Some(10.0);
        approach_attack_value(&mut current, Some(50.0), 0.0, Some(2.0), 0.5, 10.0);
        assert_near(current.unwrap(), 20.0);

        approach_attack_value(&mut current, Some(50.0), 0.0, None, 1.0, 10.0);
        assert_near(current.unwrap(), 50.0);

        approach_attack_value(&mut current, None, 0.0, Some(1.0), 1.0, 10.0);
        assert_eq!(current, None);
    }

    #[test]
    fn attack_value_merge_uses_finite_override_or_base() {
        assert_near(merge_attack_value(0.25, Some(0.75)), 0.75);
        assert_near(merge_attack_value(0.25, Some(f32::NAN)), 0.25);
        assert_near(merge_attack_value(0.25, None), 0.25);
    }

    #[test]
    fn attack_effect_merges_apply_scalar_overrides() {
        let accel = merge_attack_accel_effects(
            AccelEffects {
                boost: 0.25,
                wave: 0.5,
                ..AccelEffects::default()
            },
            AccelOverrides {
                boost: Some(1.0),
                wave: Some(f32::NAN),
                ..AccelOverrides::default()
            },
        );
        assert_near(accel.boost, 1.0);
        assert_near(accel.wave, 0.5);

        let visibility = merge_attack_visibility_effects(
            VisibilityEffects {
                dark: 0.1,
                blind: 0.2,
                cover: 0.3,
            },
            VisibilityOverrides {
                dark: Some(1.0),
                blind: Some(f32::NAN),
                cover: None,
            },
        );
        assert_near(visibility.dark, 1.0);
        assert_near(visibility.blind, 0.2);
        assert_near(visibility.cover, 0.3);
    }

    #[test]
    fn attack_visual_merge_preserves_big_and_overrides_columns() {
        let mut base = VisualEffects {
            drunk: 0.25,
            big: 1.0,
            bumpy: 0.5,
            ..VisualEffects::default()
        };
        base.bumpy_cols[1] = 0.25;
        base.tiny_cols[2] = 0.5;

        let mut attack = VisualOverrides {
            drunk: Some(1.0),
            bumpy: Some(f32::NAN),
            ..VisualOverrides::default()
        };
        attack.bumpy_cols[1] = Some(0.75);
        attack.tiny_cols[2] = Some(f32::NAN);

        let visual = merge_attack_visual_effects(base, attack);

        assert_near(visual.drunk, 1.0);
        assert_near(visual.big, 1.0);
        assert_near(visual.bumpy, 0.5);
        assert_near(visual.bumpy_cols[1], 0.75);
        assert_near(visual.tiny_cols[2], 0.5);
    }

    #[test]
    fn attack_scroll_and_perspective_merges_use_base_for_invalid_overrides() {
        let scroll = merge_attack_scroll_effects(
            ScrollEffects {
                reverse: 0.25,
                split: 0.5,
                ..ScrollEffects::default()
            },
            ScrollOverrides {
                reverse: Some(1.0),
                split: Some(f32::NAN),
                centered: Some(0.75),
                ..ScrollOverrides::default()
            },
        );
        assert_near(scroll.reverse, 1.0);
        assert_near(scroll.split, 0.5);
        assert_near(scroll.centered, 0.75);

        let perspective = merge_attack_perspective_effects(
            PerspectiveEffects {
                tilt: -0.5,
                skew: 0.25,
            },
            PerspectiveOverrides {
                tilt: Some(f32::NAN),
                skew: Some(1.0),
            },
        );
        assert_near(perspective.tilt, -0.5);
        assert_near(perspective.skew, 1.0);
    }

    #[test]
    fn effective_attack_outputs_use_profile_base_and_active_overrides() {
        let accel = effective_attack_accel_effects(
            false,
            ACCEL_MASK_BIT_BOOST,
            AccelOverrides {
                brake: Some(0.5),
                ..AccelOverrides::default()
            },
        );
        assert_near(accel.boost, 1.0);
        assert_near(accel.brake, 0.5);

        let visual = effective_attack_visual_effects(
            false,
            VISUAL_MASK_BIT_BIG,
            VisualOverrides {
                drunk: Some(0.75),
                ..VisualOverrides::default()
            },
        );
        assert_near(visual.big, 1.0);
        assert_near(visual.drunk, 0.75);

        let visibility = effective_attack_visibility_effects(VisibilityOverrides {
            dark: Some(1.0),
            ..VisibilityOverrides::default()
        });
        assert_near(visibility.dark, 1.0);

        let scroll = effective_attack_scroll_effects(
            false,
            ScrollEffects {
                reverse: 0.25,
                split: 0.5,
                ..ScrollEffects::default()
            },
            ScrollOverrides {
                reverse: Some(f32::NAN),
                centered: Some(0.75),
                ..ScrollOverrides::default()
            },
        );
        assert_near(scroll.reverse, 0.25);
        assert_near(scroll.split, 0.5);
        assert_near(scroll.centered, 0.75);

        let perspective = effective_attack_perspective_effects(
            false,
            PerspectiveEffects {
                tilt: -0.5,
                skew: 0.25,
            },
            PerspectiveOverrides {
                tilt: Some(1.0),
                ..PerspectiveOverrides::default()
            },
        );
        assert_near(perspective.tilt, 1.0);
        assert_near(perspective.skew, 0.25);
    }

    #[test]
    fn effective_attack_outputs_clear_base_but_keep_active_overrides() {
        let accel = effective_attack_accel_effects(
            true,
            ACCEL_MASK_BIT_BOOST,
            AccelOverrides {
                wave: Some(0.5),
                ..AccelOverrides::default()
            },
        );
        assert_near(accel.boost, 0.0);
        assert_near(accel.wave, 0.5);

        let visual = effective_attack_visual_effects(
            true,
            VISUAL_MASK_BIT_BIG,
            VisualOverrides {
                drunk: Some(0.75),
                ..VisualOverrides::default()
            },
        );
        assert_near(visual.big, 0.0);
        assert_near(visual.drunk, 0.75);

        let scroll = effective_attack_scroll_effects(
            true,
            ScrollEffects {
                reverse: 1.0,
                ..ScrollEffects::default()
            },
            ScrollOverrides {
                centered: Some(0.5),
                ..ScrollOverrides::default()
            },
        );
        assert_near(scroll.reverse, 0.0);
        assert_near(scroll.centered, 0.5);
    }

    #[test]
    fn effective_attack_scroll_speed_uses_active_or_base_clear_policy() {
        assert!(matches!(
            effective_attack_scroll_speed(
                false,
                Some(ScrollSpeedSetting::CMod(650.0)),
                ScrollSpeedSetting::XMod(2.0),
            ),
            ScrollSpeedSetting::CMod(v) if (v - 650.0).abs() <= 0.000_001
        ));
        assert!(matches!(
            effective_attack_scroll_speed(false, None, ScrollSpeedSetting::XMod(2.0)),
            ScrollSpeedSetting::XMod(v) if (v - 2.0).abs() <= 0.000_001
        ));
        assert_eq!(
            effective_attack_scroll_speed(true, None, ScrollSpeedSetting::XMod(2.0)),
            ScrollSpeedSetting::default()
        );
    }

    #[test]
    fn attack_mini_approach_uses_base_and_clamps() {
        let mut current = None;
        approach_attack_mini_percent_to_target(&mut current, Some(100.0), 0.0, Some(1.0), 0.5);
        assert_near(current.unwrap(), 50.0);

        let mut invalid_current = Some(f32::NAN);
        approach_attack_mini_percent_to_target(
            &mut invalid_current,
            Some(75.0),
            25.0,
            Some(0.25),
            1.0,
        );
        assert_near(invalid_current.unwrap(), 50.0);

        let mut high = None;
        approach_attack_mini_percent_to_target(&mut high, Some(250.0), 0.0, None, 1.0);
        assert_near(high.unwrap(), 150.0);

        let mut low = None;
        approach_attack_mini_percent_to_target(&mut low, Some(-250.0), 0.0, None, 1.0);
        assert_near(low.unwrap(), -100.0);
    }

    #[test]
    fn player_draw_scale_uses_tilt_and_absolute_mini() {
        assert_near(player_draw_scale_for_mini(0.0, 0.0), 1.0);
        assert_near(player_draw_scale_for_mini(-1.0, 0.0), 1.5);
        assert_near(player_draw_scale_for_mini(0.0, -0.5), 1.5);
        assert_near(player_draw_scale_for_mini(1.0, 0.5), 2.25);
    }

    #[test]
    fn accel_effects_decode_profile_mask_bits() {
        let effects = AccelEffects::from_mask_bits(
            ACCEL_MASK_BIT_BOOST
                | ACCEL_MASK_BIT_BRAKE
                | ACCEL_MASK_BIT_WAVE
                | ACCEL_MASK_BIT_EXPAND
                | ACCEL_MASK_BIT_BOOMERANG,
        );

        assert_near(effects.boost, 1.0);
        assert_near(effects.brake, 1.0);
        assert_near(effects.wave, 1.0);
        assert_near(effects.expand, 1.0);
        assert_near(effects.boomerang, 1.0);
        assert_eq!(AccelEffects::from_mask_bits(0).boost, 0.0);
    }

    #[test]
    fn visual_effects_decode_and_reencode_mask_bits() {
        let mask = VISUAL_MASK_BIT_DRUNK
            | VISUAL_MASK_BIT_BIG
            | VISUAL_MASK_BIT_FLIP
            | VISUAL_MASK_BIT_BUMPY
            | VISUAL_MASK_BIT_BEAT;
        let effects = VisualEffects::from_mask_bits(mask);

        assert_near(effects.drunk, 1.0);
        assert_near(effects.big, 1.0);
        assert_near(effects.flip, 1.0);
        assert_near(effects.bumpy, 1.0);
        assert_near(effects.beat, 1.0);
        assert_eq!(effects.to_mask_bits() & mask, mask);

        let mut column_bumpy = VisualEffects::default();
        column_bumpy.bumpy_cols[2] = 0.5;
        assert_near(column_bumpy.bumpy, 0.0);
        assert_ne!(column_bumpy.to_mask_bits() & VISUAL_MASK_BIT_BUMPY, 0);
    }

    #[test]
    fn visual_overrides_approach_base_and_clear_when_reached() {
        let mut visual = VisualOverrides {
            drunk: Some(1.0),
            tipsy: None,
            ..VisualOverrides::default()
        };
        visual.bumpy_cols[1] = Some(1.0);

        let mut base = VisualEffects::default();
        base.bumpy_cols[1] = 0.25;

        approach_visual_overrides_to_base(&mut visual, base, 0.5);

        assert_near(visual.drunk.unwrap(), 0.5);
        assert_eq!(visual.tipsy, None);
        assert_near(visual.bumpy_cols[1].unwrap(), 0.5);

        approach_visual_overrides_to_base(&mut visual, base, 1.0);

        assert_eq!(visual.drunk, None);
        assert_eq!(visual.bumpy_cols[1], None);
    }

    #[test]
    fn visual_overrides_approach_target_scalars_and_columns() {
        let mut current = VisualOverrides {
            flip: Some(1.0),
            ..VisualOverrides::default()
        };

        let mut target = VisualOverrides {
            drunk: Some(1.0),
            flip: None,
            ..VisualOverrides::default()
        };
        target.bumpy_cols[2] = Some(-1.0);

        let mut speed = VisualOverrides {
            drunk: Some(2.0),
            ..VisualOverrides::default()
        };
        speed.bumpy_cols[2] = Some(4.0);

        let mut base = VisualEffects {
            drunk: 0.25,
            ..VisualEffects::default()
        };
        base.bumpy_cols[2] = 0.0;

        approach_visual_overrides_to_target(&mut current, target, speed, base, 0.25);

        assert_near(current.drunk.unwrap(), 0.75);
        assert_eq!(current.flip, None);
        assert_near(current.bumpy_cols[2].unwrap(), -1.0);
    }

    #[test]
    fn appearance_effects_decode_mask_bits_and_default_speeds() {
        let effects = AppearanceEffects::from_mask_bits(
            APPEARANCE_MASK_BIT_HIDDEN
                | APPEARANCE_MASK_BIT_SUDDEN
                | APPEARANCE_MASK_BIT_STEALTH
                | APPEARANCE_MASK_BIT_BLINK
                | APPEARANCE_MASK_BIT_RANDOM_VANISH,
        );

        assert_near(effects.hidden, 1.0);
        assert_near(effects.sudden, 1.0);
        assert_near(effects.stealth, 1.0);
        assert_near(effects.blink, 1.0);
        assert_near(effects.random_vanish, 1.0);

        let speeds = AppearanceEffects::approach_speeds();
        assert_near(speeds.hidden, 1.0);
        assert_near(speeds.hidden_offset, 1.0);
        assert_near(speeds.random_vanish, 1.0);
    }

    #[test]
    fn appearance_target_applies_overrides_and_speeds() {
        let mut target = AppearanceEffects {
            hidden: 0.2,
            sudden: 0.3,
            blink: 0.4,
            ..AppearanceEffects::default()
        };
        let mut speed = AppearanceEffects::approach_speeds();

        apply_appearance_target(
            &mut target,
            &mut speed,
            AppearanceOverrides {
                hidden: Some(0.75),
                sudden: Some(0.25),
                random_vanish: Some(1.0),
                ..AppearanceOverrides::default()
            },
            AppearanceOverrides {
                hidden: Some(2.0),
                sudden: Some(-1.0),
                ..AppearanceOverrides::default()
            },
        );

        assert_near(target.hidden, 0.75);
        assert_near(speed.hidden, 2.0);
        assert_near(target.sudden, 0.25);
        assert_near(speed.sudden, 0.0);
        assert_near(target.blink, 0.4);
        assert_near(speed.blink, 1.0);
        assert_near(target.random_vanish, 1.0);
        assert_near(speed.random_vanish, 1.0);
    }

    #[test]
    fn appearance_effects_approach_targets_by_speed() {
        let mut current = AppearanceEffects {
            hidden: 0.0,
            sudden: 1.0,
            random_vanish: 0.25,
            ..AppearanceEffects::default()
        };

        approach_appearance_effects(
            &mut current,
            AppearanceEffects {
                hidden: 1.0,
                sudden: 0.0,
                random_vanish: 1.0,
                ..AppearanceEffects::default()
            },
            AppearanceEffects {
                hidden: 2.0,
                sudden: 4.0,
                random_vanish: 100.0,
                ..AppearanceEffects::default()
            },
            0.25,
        );

        assert_near(current.hidden, 0.5);
        assert_near(current.sudden, 0.0);
        assert_near(current.random_vanish, 1.0);

        approach_appearance_effects(
            &mut current,
            AppearanceEffects::default(),
            AppearanceEffects::approach_speeds(),
            -1.0,
        );

        assert_near(current.hidden, 0.5);
        assert_near(current.random_vanish, 1.0);
    }

    #[test]
    fn chart_attack_windows_parse_time_len_and_mods_chunks() {
        let windows = parse_chart_attack_windows(
            "TIME=1.25:LEN=2.5:MODS=*2 50% drunk, TIME=5:END=8:MODS=clearall",
        );

        assert_eq!(windows.len(), 2);
        assert_near(windows[0].start_second, 1.25);
        assert_near(windows[0].len_seconds, 2.5);
        assert_eq!(windows[0].mods, "*2 50% drunk");
        assert_near(windows[1].start_second, 5.0);
        assert_near(windows[1].len_seconds, 3.0);
        assert_eq!(windows[1].mods, "clearall");
    }

    #[test]
    fn chart_attack_windows_skip_bad_chunks_and_clamp_lengths() {
        let windows = parse_chart_attack_windows(
            "garbage TIME=nan:LEN=2:MODS=drunk TIME=4:END=2:MODS=tipsy \
             TIME=6:LEN=abc:MODS=wave TIME=9:LEN=1:MODS=,",
        );

        assert_eq!(windows.len(), 2);
        assert_near(windows[0].start_second, 4.0);
        assert_near(windows[0].len_seconds, 0.0);
        assert_eq!(windows[0].mods, "tipsy");
        assert_near(windows[1].start_second, 6.0);
        assert_near(windows[1].len_seconds, 0.0);
        assert_eq!(windows[1].mods, "wave");
        assert!(parse_chart_attack_windows("").is_empty());
        assert!(parse_chart_attack_windows("LEN=1:MODS=drunk").is_empty());
    }

    #[test]
    fn random_attack_windows_use_fixed_timing_policy() {
        let windows = build_random_attack_windows(18.0, 0, 12345);

        assert_eq!(windows.len(), 3);
        assert_near(windows[0].start_second, 4.5);
        assert_near(windows[1].start_second, 10.0);
        assert_near(windows[2].start_second, 15.5);
        for window in &windows {
            assert_near(window.len_seconds, RANDOM_ATTACK_RUN_TIME_SECONDS);
            assert!(RANDOM_ATTACK_MOD_POOL.contains(&window.mods.as_str()));
        }
    }

    #[test]
    fn random_attack_windows_are_seeded_by_player_and_count() {
        let player_one = build_random_attack_windows(18.0, 0, 99);
        let player_one_again = build_random_attack_windows(18.0, 0, 99);
        let player_two = build_random_attack_windows(18.0, 1, 99);
        let longer_song = build_random_attack_windows(24.0, 0, 99);

        assert_eq!(player_one, player_one_again);
        assert_ne!(player_one, player_two);
        assert_ne!(player_one, longer_song);
        assert_ne!(
            random_attack_seed(99, 0, player_one.len()),
            random_attack_seed(99, 1, player_one.len()),
        );
    }

    #[test]
    fn random_attack_windows_skip_invalid_or_too_short_songs() {
        assert!(build_random_attack_windows(f32::NAN, 0, 1).is_empty());
        assert!(build_random_attack_windows(0.0, 0, 1).is_empty());
        assert!(build_random_attack_windows(4.5, 0, 1).is_empty());
        assert_eq!(build_random_attack_windows(4.6, 0, 1).len(), 1);
    }

    #[test]
    fn attack_windows_for_mode_select_chart_random_or_off() {
        let chart = "TIME=1:LEN=2:MODS=drunk";

        assert!(
            build_attack_windows_for_mode(Some(chart), GameplayAttackMode::Off, 0, 99, 18.0)
                .is_empty()
        );

        let parsed =
            build_attack_windows_for_mode(Some(chart), GameplayAttackMode::On, 0, 99, 18.0);
        assert_eq!(parsed.len(), 1);
        assert_near(parsed[0].start_second, 1.0);
        assert_near(parsed[0].len_seconds, 2.0);
        assert_eq!(parsed[0].mods, "drunk");

        let random =
            build_attack_windows_for_mode(Some(chart), GameplayAttackMode::Random, 0, 99, 18.0);
        assert_eq!(random, build_random_attack_windows(18.0, 0, 99));
    }

    #[test]
    fn attack_windows_for_mode_handles_missing_chart_attacks() {
        assert!(
            build_attack_windows_for_mode(None, GameplayAttackMode::On, 0, 99, 18.0).is_empty()
        );
        assert!(
            !build_attack_windows_for_mode(None, GameplayAttackMode::Random, 0, 99, 18.0)
                .is_empty()
        );
    }

    #[test]
    fn chart_attacks_enabled_for_mode_matches_profile_policy() {
        assert!(!chart_attacks_enabled_for_mode(
            Some("TIME=1:LEN=2:MODS=drunk"),
            GameplayAttackMode::Off,
        ));
        assert!(!chart_attacks_enabled_for_mode(
            Some("   "),
            GameplayAttackMode::On,
        ));
        assert!(chart_attacks_enabled_for_mode(
            Some("TIME=1:LEN=2:MODS=drunk"),
            GameplayAttackMode::On,
        ));
        assert!(chart_attacks_enabled_for_mode(
            None,
            GameplayAttackMode::Random,
        ));
    }

    #[test]
    fn player_chart_changes_for_options_tracks_chart_mutation_sources() {
        assert!(!player_chart_changes_for_options(
            false,
            GameplayTurnOption::None,
            Some("TIME=1:LEN=2:MODS=drunk"),
            GameplayAttackMode::Off,
        ));
        assert!(player_chart_changes_for_options(
            true,
            GameplayTurnOption::None,
            None,
            GameplayAttackMode::Off,
        ));
        assert!(player_chart_changes_for_options(
            false,
            GameplayTurnOption::Mirror,
            None,
            GameplayAttackMode::Off,
        ));
        assert!(player_chart_changes_for_options(
            false,
            GameplayTurnOption::None,
            Some("TIME=1:LEN=2:MODS=drunk"),
            GameplayAttackMode::On,
        ));
    }

    #[test]
    fn outro_attack_visual_clear_snapshots_active_visual_once() {
        let mut cleared = false;
        let mut active = [VisualOverrides::default(); MAX_PLAYERS];
        let mut outro = [VisualOverrides::default(); MAX_PLAYERS];
        active[0].drunk = Some(0.25);
        active[1].tipsy = Some(0.75);

        begin_outro_attack_visual_clear(&mut cleared, 2, &active, &mut outro);

        assert!(cleared);
        assert_eq!(outro[0].drunk, Some(0.25));
        assert_eq!(outro[1].tipsy, Some(0.75));

        active[0].drunk = Some(1.0);
        active[1].tipsy = Some(1.0);
        begin_outro_attack_visual_clear(&mut cleared, 2, &active, &mut outro);

        assert_eq!(outro[0].drunk, Some(0.25));
        assert_eq!(outro[1].tipsy, Some(0.75));
    }

    #[test]
    fn outro_attack_visual_clear_only_copies_active_players() {
        let mut cleared = false;
        let mut active = [VisualOverrides::default(); MAX_PLAYERS];
        let mut outro = [VisualOverrides::default(); MAX_PLAYERS];
        active[0].drunk = Some(0.25);
        active[1].tipsy = Some(0.75);

        begin_outro_attack_visual_clear(&mut cleared, 1, &active, &mut outro);

        assert!(cleared);
        assert_eq!(outro[0].drunk, Some(0.25));
        assert!(!outro[1].any());
    }

    #[test]
    fn active_attack_refresh_applies_active_windows_and_eases() {
        let attack_windows = [attack_mask_window(
            0.0,
            2.0,
            parse_attack_mods("50% drunk,30% reverse,25% mini,stealth,dark,C650"),
        )];
        let lua_windows = [song_lua_ease_mask_window(
            SongLuaEaseMaskTarget::PlayerRotationZ,
            0.0,
            2.0,
            2.0,
            0.0,
            90.0,
        )];

        let output = refresh_active_attack_player(
            ActiveAttackRefreshInput {
                now: 1.0,
                delta_time: 0.5,
                attacks_cleared_for_outro: false,
                base_appearance: AppearanceEffects::default(),
                base_visual: VisualEffects::default(),
                base_scroll: ScrollEffects::default(),
                base_mini_percent: 10.0,
                attack_windows: &attack_windows,
                song_lua_ease_windows: &lua_windows,
            },
            ActiveAttackRefreshState {
                attack_current_appearance: AppearanceEffects::default(),
                active_attack_visual: VisualOverrides::default(),
                active_attack_visibility: VisibilityOverrides::default(),
                active_attack_scroll: ScrollOverrides::default(),
                active_attack_mini_percent: None,
                outro_attack_visual: VisualOverrides::default(),
            },
        );

        assert!(!output.active_attack_clear_all);
        assert_near(output.attack_target_appearance.stealth, 1.0);
        assert_near(output.active_attack_appearance.stealth, 0.5);
        assert_eq!(output.active_attack_visual.drunk, Some(0.5));
        assert_eq!(output.active_attack_visibility.dark, Some(1.0));
        assert_eq!(output.active_attack_scroll.reverse, Some(0.3));
        assert_eq!(output.active_attack_mini_percent, Some(25.0));
        assert!(matches!(
            output.active_attack_scroll_speed,
            Some(ScrollSpeedSetting::CMod(v)) if (v - 650.0).abs() <= 0.000_001
        ));
        assert_eq!(output.player_transform.rotation_z, Some(45.0));
    }

    #[test]
    fn active_attack_refresh_outro_clears_visuals_and_preserves_visibility() {
        let lua_windows = [song_lua_ease_mask_window(
            SongLuaEaseMaskTarget::PlayerRotationZ,
            0.0,
            2.0,
            2.0,
            0.0,
            90.0,
        )];
        let mut outro_visual = VisualOverrides::default();
        outro_visual.drunk = Some(0.5);
        let visibility = VisibilityOverrides {
            dark: Some(1.0),
            ..VisibilityOverrides::default()
        };

        let output = refresh_active_attack_player(
            ActiveAttackRefreshInput {
                now: 1.0,
                delta_time: 1.0,
                attacks_cleared_for_outro: true,
                base_appearance: AppearanceEffects::default(),
                base_visual: VisualEffects::default(),
                base_scroll: ScrollEffects::default(),
                base_mini_percent: 0.0,
                attack_windows: &[],
                song_lua_ease_windows: &lua_windows,
            },
            ActiveAttackRefreshState {
                attack_current_appearance: AppearanceEffects::default(),
                active_attack_visual: VisualOverrides::default(),
                active_attack_visibility: visibility,
                active_attack_scroll: ScrollOverrides {
                    reverse: Some(1.0),
                    ..ScrollOverrides::default()
                },
                active_attack_mini_percent: Some(50.0),
                outro_attack_visual: outro_visual,
            },
        );

        assert!(!output.active_attack_clear_all);
        assert!(!output.active_attack_visual.any());
        assert!(!output.outro_attack_visual.any());
        assert_eq!(output.active_attack_visibility.dark, Some(1.0));
        assert!(!output.active_attack_scroll.any());
        assert_eq!(output.active_attack_mini_percent, None);
        assert_eq!(output.player_transform.rotation_z, Some(45.0));
    }

    #[test]
    fn attack_mask_windows_filter_noops_and_invalid_durations() {
        let attacks = [
            ChartAttackWindow {
                start_second: 1.0,
                len_seconds: 0.0,
                mods: "drunk".to_string(),
            },
            ChartAttackWindow {
                start_second: 2.0,
                len_seconds: 1.0,
                mods: "unknown".to_string(),
            },
            ChartAttackWindow {
                start_second: f32::NAN,
                len_seconds: 1.0,
                mods: "drunk".to_string(),
            },
        ];

        assert!(build_attack_mask_windows(&attacks).is_empty());
    }

    #[test]
    fn attack_mask_window_keeps_runtime_mods() {
        let attack = ChartAttackWindow {
            start_second: 1.5,
            len_seconds: 2.25,
            mods: "*2 50% drunk,25% mini,C600".to_string(),
        };
        let window = attack_mask_window_from_parts(&attack, parse_attack_mods(&attack.mods))
            .expect("runtime mods should build an attack mask window");

        assert_near(window.start_second, 1.5);
        assert_near(window.end_second, 3.75);
        assert_near(window.sustain_end_second, 3.75);
        assert!(!window.persist_after_end);
        assert!(!window.clear_all);
        assert_eq!(window.chart, ChartAttackEffects::default());
        assert_eq!(window.scroll_speed, Some(ScrollSpeedSetting::CMod(600.0)));
        assert_eq!(window.mini_percent, Some(25.0));
        assert_eq!(window.mini_mode, MiniAttackMode::Absolute);
        assert_eq!(window.mini_speed, Some(1.0));
        assert_eq!(window.visual.drunk, Some(0.5));
        assert_eq!(window.visual_speed.drunk, Some(2.0));
    }

    #[test]
    fn attack_mask_window_keeps_chart_masks_and_turn_bits() {
        let attack = ChartAttackWindow {
            start_second: 4.0,
            len_seconds: 3.0,
            mods: "mirror,mines,noholds,planted".to_string(),
        };
        let window = attack_mask_window_from_parts(&attack, parse_attack_mods(&attack.mods))
            .expect("chart mods should build an attack mask window");

        assert_eq!(window.chart.insert_mask, INSERT_MASK_BIT_MINES);
        assert_eq!(window.chart.remove_mask, REMOVE_MASK_BIT_NO_HOLDS);
        assert_eq!(window.chart.holds_mask, HOLDS_MASK_BIT_PLANTED);
        assert_eq!(
            window.chart.turn_bits,
            turn_option_bits(GameplayTurnOption::Mirror)
        );
        assert!(!window.clear_all);
        assert_eq!(window.scroll_speed, None);
        assert_eq!(window.mini_percent, None);
    }

    #[test]
    fn attack_mask_windows_keep_clearall() {
        let attacks = [ChartAttackWindow {
            start_second: 5.0,
            len_seconds: 1.0,
            mods: "clearall".to_string(),
        }];
        let windows = build_attack_mask_windows(&attacks);

        assert_eq!(windows.len(), 1);
        assert!(windows[0].clear_all);
        assert_eq!(windows[0].chart, ChartAttackEffects::default());
    }

    #[test]
    fn chart_attack_row_range_uses_timing_seconds() {
        let timing = test_timing(ROWS_PER_BEAT as usize * 4);
        let attack = ChartAttackWindow {
            start_second: 0.5,
            len_seconds: 1.0,
            mods: "mirror".to_string(),
        };

        assert_eq!(
            chart_attack_row_range(&attack, &timing),
            Some((ROWS_PER_BEAT as usize / 2, ROWS_PER_BEAT as usize * 3 / 2)),
        );
        assert_eq!(
            chart_attack_turn_seed(99, 0, 0),
            chart_attack_turn_seed(99, 0, 0),
        );
        assert_ne!(
            chart_attack_turn_seed(99, 0, 0),
            chart_attack_turn_seed(99, 1, 0),
        );
    }

    #[test]
    fn attack_turn_mod_applies_mirror_and_special_turns() {
        let mut notes = (0..4)
            .map(|col| {
                let mut note = test_note_at(NoteType::Tap, None, false, 0, 0.0);
                note.column = col;
                note
            })
            .collect::<Vec<_>>();

        apply_attack_turn_mod(&mut notes, 0, 4, GameplayTurnOption::Mirror, 1, 0);

        let cols: Vec<_> = notes.iter().map(|note| note.column).collect();
        assert_eq!(cols, vec![3, 2, 1, 0]);

        apply_attack_turn_mod(&mut notes, 0, 4, GameplayTurnOption::None, 1, 0);
        let unchanged_cols: Vec<_> = notes.iter().map(|note| note.column).collect();
        assert_eq!(unchanged_cols, cols);
    }

    #[test]
    fn chart_attack_windows_apply_only_targeted_rows() {
        let timing = test_timing(ROWS_PER_BEAT as usize * 3);
        let mut notes = vec![
            test_note_at(NoteType::Tap, None, false, 0, 0.0),
            test_note_at(NoteType::Tap, None, false, ROWS_PER_BEAT as usize, 1.0),
            test_note_at(NoteType::Tap, None, false, ROWS_PER_BEAT as usize * 2, 2.0),
        ];
        notes[0].column = 0;
        notes[1].column = 1;
        notes[2].column = 2;

        apply_chart_attack_windows(
            &mut notes,
            &[ChartAttackWindow {
                start_second: 0.5,
                len_seconds: 1.0,
                mods: "mirror".to_string(),
            }],
            &timing,
            0,
            4,
            0,
            7,
        );

        let rows_and_cols: Vec<_> = notes
            .iter()
            .map(|note| (note.row_index, note.column))
            .collect();
        assert_eq!(
            rows_and_cols,
            vec![
                (0, 0),
                (ROWS_PER_BEAT as usize, 2),
                (ROWS_PER_BEAT as usize * 2, 2),
            ],
        );
    }

    #[test]
    fn chart_attacks_for_mode_apply_enabled_chart_windows() {
        let timing = test_timing(ROWS_PER_BEAT as usize * 3);
        let mut notes = vec![
            test_note_at(NoteType::Tap, None, false, 0, 0.0),
            test_note_at(NoteType::Tap, None, false, ROWS_PER_BEAT as usize, 1.0),
            test_note_at(NoteType::Tap, None, false, ROWS_PER_BEAT as usize * 2, 2.0),
        ];
        notes[0].column = 0;
        notes[1].column = 1;
        notes[2].column = 2;

        apply_chart_attacks_for_mode(
            &mut notes,
            Some("TIME=0.5:LEN=1:MODS=mirror"),
            GameplayAttackMode::On,
            &timing,
            0,
            4,
            0,
            7,
            3.0,
        );

        let rows_and_cols: Vec<_> = notes
            .iter()
            .map(|note| (note.row_index, note.column))
            .collect();
        assert_eq!(
            rows_and_cols,
            vec![
                (0, 0),
                (ROWS_PER_BEAT as usize, 2),
                (ROWS_PER_BEAT as usize * 2, 2),
            ],
        );
    }

    #[test]
    fn chart_attacks_for_mode_noops_when_disabled_or_missing() {
        let timing = test_timing(ROWS_PER_BEAT as usize * 3);
        let original = vec![
            test_note_at(NoteType::Tap, None, false, 0, 0.0),
            test_note_at(NoteType::Tap, None, false, ROWS_PER_BEAT as usize, 1.0),
        ];
        let original_rows_and_cols: Vec<_> = original
            .iter()
            .map(|note| (note.row_index, note.column))
            .collect();
        let mut off_notes = original.clone();
        let mut missing_notes = original.clone();

        apply_chart_attacks_for_mode(
            &mut off_notes,
            Some("TIME=0:LEN=2:MODS=mirror"),
            GameplayAttackMode::Off,
            &timing,
            0,
            4,
            0,
            7,
            3.0,
        );
        apply_chart_attacks_for_mode(
            &mut missing_notes,
            None,
            GameplayAttackMode::On,
            &timing,
            0,
            4,
            0,
            7,
            3.0,
        );

        let off_rows_and_cols: Vec<_> = off_notes
            .iter()
            .map(|note| (note.row_index, note.column))
            .collect();
        let missing_rows_and_cols: Vec<_> = missing_notes
            .iter()
            .map(|note| (note.row_index, note.column))
            .collect();
        assert_eq!(off_rows_and_cols, original_rows_and_cols);
        assert_eq!(missing_rows_and_cols, original_rows_and_cols);
    }

    #[test]
    fn chart_attack_transforms_apply_per_player_and_rebuild_ranges() {
        let timing = test_timing(ROWS_PER_BEAT as usize * 3);
        let mut notes = vec![
            test_note_at(NoteType::Tap, None, false, 0, 0.0),
            test_note_at(NoteType::Tap, None, false, ROWS_PER_BEAT as usize, 1.0),
            test_note_at(NoteType::Tap, None, false, 0, 0.0),
            test_note_at(NoteType::Tap, None, false, ROWS_PER_BEAT as usize, 1.0),
        ];
        notes[0].column = 0;
        notes[1].column = 1;
        notes[2].column = 4;
        notes[3].column = 5;
        let mut note_ranges = [(0usize, 2usize), (2usize, 4usize)];
        let disabled = ChartAttackTransformPlayer {
            chart_attacks: None,
            attack_mode: GameplayAttackMode::On,
            timing_player: &timing,
        };
        let mut players = [disabled; MAX_PLAYERS];
        players[0] = ChartAttackTransformPlayer {
            chart_attacks: Some("TIME=0.5:LEN=1:MODS=mirror"),
            attack_mode: GameplayAttackMode::On,
            timing_player: &timing,
        };

        apply_chart_attack_transforms(&mut notes, &mut note_ranges, 4, 2, &players, 7, 3.0);

        assert_eq!(note_ranges, [(0, 2), (2, 4)]);
        let rows_and_cols: Vec<_> = notes
            .iter()
            .map(|note| (note.row_index, note.column))
            .collect();
        assert_eq!(
            rows_and_cols,
            vec![
                (0, 0),
                (ROWS_PER_BEAT as usize, 2),
                (0, 4),
                (ROWS_PER_BEAT as usize, 5),
            ],
        );
    }

    #[test]
    fn chart_attack_transforms_duplicate_single_player_range() {
        let timing = test_timing(ROWS_PER_BEAT as usize * 3);
        let mut notes = vec![
            test_note_at(NoteType::Tap, None, false, 0, 0.0),
            test_note_at(NoteType::Tap, None, false, ROWS_PER_BEAT as usize, 1.0),
        ];
        notes[0].column = 0;
        notes[1].column = 1;
        let mut note_ranges = [(0usize, 2usize), (99usize, 99usize)];
        let disabled = ChartAttackTransformPlayer {
            chart_attacks: None,
            attack_mode: GameplayAttackMode::On,
            timing_player: &timing,
        };
        let mut players = [disabled; MAX_PLAYERS];
        players[0] = ChartAttackTransformPlayer {
            chart_attacks: Some("TIME=0.5:LEN=1:MODS=mirror"),
            attack_mode: GameplayAttackMode::On,
            timing_player: &timing,
        };

        apply_chart_attack_transforms(&mut notes, &mut note_ranges, 4, 1, &players, 7, 3.0);

        assert_eq!(note_ranges[0], (0, 2));
        assert_eq!(note_ranges[1], note_ranges[0]);
    }

    #[test]
    fn active_attack_targets_mark_current_runtime_targets_only() {
        let windows = build_attack_mask_windows(&[
            ChartAttackWindow {
                start_second: 0.0,
                len_seconds: 1.0,
                mods: "tipsy".to_string(),
            },
            ChartAttackWindow {
                start_second: 1.0,
                len_seconds: 2.0,
                mods: "50% drunk,30% reverse,25% mini".to_string(),
            },
            ChartAttackWindow {
                start_second: 5.0,
                len_seconds: 1.0,
                mods: "clearall".to_string(),
            },
        ]);

        let targets = collect_active_attack_targets(&windows, 2.0);

        assert!(!targets.clear_all);
        assert_eq!(targets.visual.drunk, Some(0.0));
        assert_eq!(targets.visual.tipsy, None);
        assert_eq!(targets.scroll.reverse, Some(0.0));
        assert!(targets.mini_percent);
    }

    #[test]
    fn active_attack_targets_use_half_open_time_windows() {
        let windows = build_attack_mask_windows(&[ChartAttackWindow {
            start_second: 1.0,
            len_seconds: 1.0,
            mods: "clearall".to_string(),
        }]);

        assert!(!collect_active_attack_targets(&windows, 0.99).clear_all);
        assert!(collect_active_attack_targets(&windows, 1.0).clear_all);
        assert!(collect_active_attack_targets(&windows, 1.99).clear_all);
        assert!(!collect_active_attack_targets(&windows, 2.0).clear_all);
    }

    #[test]
    fn persisted_attack_targets_are_blocked_by_active_replacements() {
        assert!(persisted_target_allowed(false, true, Some(0.0)));
        assert!(persisted_target_allowed(true, false, None));
        assert!(!persisted_target_allowed(true, true, None));
        assert!(!persisted_target_allowed(true, false, Some(0.0)));

        let mut targets = AttackActiveTargets::default();
        assert!(persisted_mini_allowed(false, targets));
        assert!(persisted_mini_allowed(true, targets));

        targets.mini_percent = true;
        assert!(!persisted_mini_allowed(true, targets));

        targets.mini_percent = false;
        targets.clear_all = true;
        assert!(!persisted_mini_allowed(true, targets));
    }

    #[test]
    fn active_attack_mask_window_applies_values_and_speeds() {
        let mut mods = ParsedAttackMods {
            scroll_speed: Some(ScrollSpeedSetting::CMod(650.0)),
            mini_percent: Some(40.0),
            ..ParsedAttackMods::default()
        };
        mods.accel.boost = Some(0.75);
        mods.visual.drunk = Some(1.0);
        mods.visual_speed.drunk = Some(0.25);
        mods.appearance.hidden = Some(1.0);
        mods.appearance_speed.hidden = Some(0.5);
        mods.visibility.dark = Some(1.0);
        mods.scroll.reverse = Some(0.5);
        mods.scroll_approach_speed.reverse = Some(0.75);
        mods.perspective.tilt = Some(-1.0);
        let window = attack_mask_window(1.0, 4.0, mods);
        let mut values = ActiveAttackMaskValues::new(AppearanceEffects::default());

        apply_active_attack_mask_window(
            &mut values,
            &window,
            AttackActiveTargets::default(),
            false,
            20.0,
        );

        assert_near(values.accel.boost.unwrap(), 0.75);
        assert_near(values.visual.drunk.unwrap(), 1.0);
        assert_near(values.visual_speed.drunk.unwrap(), 0.25);
        assert_near(values.appearance_target.hidden, 1.0);
        assert_near(values.appearance_speed.hidden, 0.5);
        assert_near(values.visibility.dark.unwrap(), 1.0);
        assert_near(values.scroll.reverse.unwrap(), 0.5);
        assert_near(values.scroll_approach_speed.reverse.unwrap(), 0.75);
        assert_near(values.perspective.tilt.unwrap(), -1.0);
        assert!(matches!(
            values.scroll_speed,
            Some(ScrollSpeedSetting::CMod(v)) if (v - 650.0).abs() <= 0.000_001
        ));
        assert_near(values.mini_percent.unwrap(), 40.0);
    }

    #[test]
    fn active_attack_mask_window_clearall_resets_values_and_delta_mini_base() {
        let mut values = ActiveAttackMaskValues::new(AppearanceEffects {
            hidden: 1.0,
            ..AppearanceEffects::default()
        });
        values.accel.boost = Some(1.0);

        let mut mods = ParsedAttackMods {
            clear_all: true,
            mini_percent: Some(25.0),
            ..ParsedAttackMods::default()
        };
        mods.visual.drunk = Some(0.5);
        let mut window = attack_mask_window(1.0, 4.0, mods);
        window.mini_mode = MiniAttackMode::Delta;

        apply_active_attack_mask_window(
            &mut values,
            &window,
            AttackActiveTargets::default(),
            false,
            100.0,
        );

        assert!(values.clear_all);
        assert_eq!(values.accel.boost, None);
        assert_near(values.appearance_target.hidden, 0.0);
        assert_near(values.visual.drunk.unwrap(), 0.5);
        assert_near(values.mini_percent.unwrap(), 25.0);
    }

    #[test]
    fn active_attack_mask_window_blocks_persisted_replaced_targets() {
        let mut mods = ParsedAttackMods::default();
        mods.visual.drunk = Some(0.75);
        mods.visual.bumpy_cols[2] = Some(1.0);
        mods.scroll.reverse = Some(0.5);
        let window = attack_mask_window(1.0, 4.0, mods);
        let mut targets = AttackActiveTargets::default();
        targets.visual.drunk = Some(0.0);
        targets.scroll.reverse = Some(0.0);
        let mut values = ActiveAttackMaskValues::new(AppearanceEffects::default());

        apply_active_attack_mask_window(&mut values, &window, targets, true, 0.0);

        assert_eq!(values.visual.drunk, None);
        assert_eq!(values.scroll.reverse, None);
        assert_near(values.visual.bumpy_cols[2].unwrap(), 1.0);
    }

    #[test]
    fn attack_mod_parser_keeps_scroll_override_and_partial_levels() {
        let mods = parse_attack_mods("0.5x,20% flip,50% hidden,30% blink,25% mini");

        assert_eq!(mods.scroll_speed, Some(ScrollSpeedSetting::XMod(0.5)));
        assert_eq!(mods.visual.flip, Some(0.2));
        assert_eq!(mods.appearance.hidden, Some(0.5));
        assert_eq!(mods.appearance.blink, Some(0.3));
        assert_eq!(mods.mini_percent, Some(25.0));
    }

    #[test]
    fn attack_mod_parser_maps_chart_masks_and_turn_options() {
        let mods = parse_attack_mods(
            "wide,big,quick,bmrize,skippy,echo,stomp,mines,little,nomines,noholds,\
             nojumps,nohands,noquads,nolifts,nofakes,planted,floored,twister,norolls,\
             holdstorolls,mirror,left,right,lrmirror,udmirror,shuffle,blender,hypershuffle",
        );

        assert_eq!(
            mods.insert_mask,
            INSERT_MASK_BIT_WIDE
                | INSERT_MASK_BIT_BIG
                | INSERT_MASK_BIT_QUICK
                | INSERT_MASK_BIT_BMRIZE
                | INSERT_MASK_BIT_SKIPPY
                | INSERT_MASK_BIT_ECHO
                | INSERT_MASK_BIT_STOMP
                | INSERT_MASK_BIT_MINES,
        );
        assert_eq!(
            mods.remove_mask,
            REMOVE_MASK_BIT_LITTLE
                | REMOVE_MASK_BIT_NO_MINES
                | REMOVE_MASK_BIT_NO_HOLDS
                | REMOVE_MASK_BIT_NO_JUMPS
                | REMOVE_MASK_BIT_NO_HANDS
                | REMOVE_MASK_BIT_NO_QUADS
                | REMOVE_MASK_BIT_NO_LIFTS
                | REMOVE_MASK_BIT_NO_FAKES,
        );
        assert_eq!(
            mods.holds_mask,
            HOLDS_MASK_BIT_PLANTED
                | HOLDS_MASK_BIT_FLOORED
                | HOLDS_MASK_BIT_TWISTER
                | HOLDS_MASK_BIT_NO_ROLLS
                | HOLDS_MASK_BIT_HOLDS_TO_ROLLS,
        );
        assert_eq!(mods.turn_option, GameplayTurnOption::Random);
        assert_eq!(turn_option_bits(GameplayTurnOption::Mirror), 1 << 0);
        assert_eq!(turn_option_bits(GameplayTurnOption::Random), 1 << 7);
    }

    #[test]
    fn attack_mod_parser_clearall_discards_prior_mods_and_no_prefix_zeroes_levels() {
        let mods = parse_attack_mods("drunk,clearall,30% blink,no hidden");

        assert!(mods.clear_all);
        assert_eq!(mods.visual.drunk, None);
        assert_eq!(mods.appearance.blink, Some(0.3));
        assert_eq!(mods.appearance.hidden, Some(0.0));
    }

    #[test]
    fn attack_mod_parser_accepts_scroll_perspective_and_approach_prefixes() {
        let mods = parse_attack_mods(
            "C600,*1000 sudden,*1000 -125% suddenoffset,*2.4 150% hiddenoffset,\
             30% reverse,centered,50% incoming,dark,50% blind,75% cover",
        );

        assert_eq!(mods.scroll_speed, Some(ScrollSpeedSetting::CMod(600.0)));
        assert_eq!(mods.appearance.sudden, Some(1.0));
        assert_eq!(mods.appearance.sudden_offset, Some(-1.25));
        assert_eq!(mods.appearance.hidden_offset, Some(1.5));
        assert_eq!(mods.appearance_speed.sudden, Some(1000.0));
        assert_eq!(mods.appearance_speed.sudden_offset, Some(1000.0));
        assert_eq!(mods.appearance_speed.hidden_offset, Some(2.4));
        assert_eq!(mods.scroll.reverse, Some(0.3));
        assert_eq!(mods.scroll.centered, Some(1.0));
        assert_eq!(mods.perspective.tilt, Some(-0.5));
        assert_eq!(mods.perspective.skew, Some(0.5));
        assert_eq!(mods.visibility.dark, Some(1.0));
        assert_eq!(mods.visibility.blind, Some(0.5));
        assert_eq!(mods.visibility.cover, Some(0.75));
    }

    #[test]
    fn song_lua_runtime_mod_parser_accepts_itgmania_forms() {
        let mods = parse_song_lua_runtime_mods(
            "*9999 25 invert,*9999 no hidden,*9999 3x,*9999 -25 tiny,\
             *9999 25 mini,*9999 50 incoming,*9999 15 bumpy3,*9999 250 tiny2,\
             *9999 -125 bumpyperiod,*9999 100 pulseouter",
        );

        assert_eq!(mods.visual.invert, Some(0.25));
        assert_eq!(mods.appearance.hidden, Some(0.0));
        assert_eq!(mods.scroll_speed, Some(ScrollSpeedSetting::XMod(3.0)));
        assert_eq!(mods.visual.tiny, Some(-0.25));
        assert_eq!(mods.mini_percent, Some(25.0));
        assert_eq!(mods.perspective.tilt, Some(-0.5));
        assert_eq!(mods.perspective.skew, Some(0.5));
        assert_eq!(mods.visual.bumpy, None);
        assert_eq!(mods.visual.bumpy_cols[2], Some(0.15));
        assert_eq!(mods.visual.tiny_cols[1], Some(2.5));
        assert_eq!(mods.visual.bumpy_period, Some(-1.25));
        assert_eq!(mods.visual.pulse_outer, Some(1.0));
    }

    #[test]
    fn song_lua_runtime_mod_parser_scales_column_moves() {
        let mods = parse_song_lua_runtime_mods(
            "*10000 -80 movey1,*10000 40 movex2,*10000 -314 confusionoffset3,\
             *10000 -80 tiny",
        );

        assert_eq!(mods.visual.move_y_cols[0], Some(-0.8));
        assert_eq!(mods.visual.move_x_cols[1], Some(0.4));
        assert_eq!(mods.visual.confusion_offset_cols[2], Some(-3.14));
        assert_eq!(mods.visual.tiny, Some(-0.8));
        assert_eq!(mods.mini_percent, None);
    }

    #[test]
    fn effect_overrides_report_active_scalar_values() {
        assert!(!AccelOverrides::default().any());
        assert!(!AppearanceOverrides::default().any());
        assert!(!VisibilityOverrides::default().any());
        assert!(!ScrollOverrides::default().any());
        assert!(!PerspectiveOverrides::default().any());

        assert!(
            AccelOverrides {
                wave: Some(0.0),
                ..AccelOverrides::default()
            }
            .any()
        );
        assert!(
            AppearanceOverrides {
                stealth: Some(0.0),
                ..AppearanceOverrides::default()
            }
            .any()
        );
        assert!(
            VisibilityOverrides {
                cover: Some(0.0),
                ..VisibilityOverrides::default()
            }
            .any()
        );
        assert!(
            ScrollOverrides {
                centered: Some(0.0),
                ..ScrollOverrides::default()
            }
            .any()
        );
        assert!(
            PerspectiveOverrides {
                skew: Some(0.0),
                ..PerspectiveOverrides::default()
            }
            .any()
        );
    }

    #[test]
    fn visual_overrides_report_active_column_values() {
        assert!(!VisualOverrides::default().any());

        let mut bumpy = VisualOverrides::default();
        bumpy.bumpy_cols[MAX_COLS - 1] = Some(0.0);
        assert!(bumpy.any());

        let mut tiny = VisualOverrides::default();
        tiny.tiny_cols[1] = Some(0.25);
        assert!(tiny.any());

        let mut move_x = VisualOverrides::default();
        move_x.move_x_cols[0] = Some(-4.0);
        assert!(move_x.any());

        let mut move_y = VisualOverrides::default();
        move_y.move_y_cols[2] = Some(8.0);
        assert!(move_y.any());

        let mut confusion = VisualOverrides::default();
        confusion.confusion_offset_cols[3] = Some(90.0);
        assert!(confusion.any());
    }

    #[test]
    fn spacing_multiplier_clamps_and_scales_percent() {
        assert_eq!(SPACING_PERCENT_MIN, -100);
        assert_eq!(SPACING_PERCENT_MAX, 100);
        assert_near(spacing_multiplier_for_percent(0), 1.0);
        assert_near(spacing_multiplier_for_percent(25), 1.25);
        assert_near(spacing_multiplier_for_percent(-50), 0.5);
        assert_near(spacing_multiplier_for_percent(250), 2.0);
        assert_near(spacing_multiplier_for_percent(-250), 0.0);
    }

    #[test]
    fn toggle_flash_alpha_uses_hold_then_fade_countdown() {
        assert_eq!(toggle_flash_alpha(0.0), None);
        assert_eq!(toggle_flash_alpha(-1.0), None);
        assert_near(toggle_flash_alpha(TOGGLE_FLASH_DURATION).unwrap(), 1.0);
        assert_near(
            toggle_flash_alpha(TOGGLE_FLASH_DURATION - TOGGLE_FLASH_FADE_START).unwrap(),
            1.0,
        );
        assert_near(toggle_flash_alpha(0.35).unwrap(), 0.5);
        assert_near(toggle_flash_alpha(0.001).unwrap(), 0.001 / 0.7);
    }

    #[test]
    fn toggle_flash_alpha_preserves_overfull_timer_as_opaque() {
        assert_near(
            toggle_flash_alpha(TOGGLE_FLASH_DURATION + 1.0).unwrap(),
            1.0,
        );
    }

    #[test]
    fn positive_timer_tick_drains_only_active_timers() {
        let mut active = 0.5;
        tick_positive_timer(&mut active, 0.2);
        assert_near(active, 0.3);
        tick_positive_timer(&mut active, 1.0);
        assert_near(active, 0.0);

        let mut inactive = 0.0;
        tick_positive_timer(&mut inactive, 0.2);
        assert_near(inactive, 0.0);

        let mut negative = -0.5;
        tick_positive_timer(&mut negative, 0.2);
        assert_near(negative, -0.5);
    }

    #[test]
    fn approach_f32_steps_toward_target_without_overshoot() {
        let mut value = 0.0;
        approach_f32(&mut value, 1.0, 0.25);
        assert_near(value, 0.25);

        approach_f32(&mut value, 1.0, 2.0);
        assert_near(value, 1.0);

        approach_f32(&mut value, -1.0, 0.5);
        assert_near(value, 0.5);
    }

    #[test]
    fn approach_f32_handles_bad_inputs_like_runtime_policy() {
        let mut value = 0.5;
        approach_f32(&mut value, 1.0, 0.0);
        assert_near(value, 0.5);

        approach_f32(&mut value, 1.0, -1.0);
        assert_near(value, 0.5);

        value = f32::INFINITY;
        approach_f32(&mut value, 2.0, 0.25);
        assert_near(value, 2.0);

        approach_f32(&mut value, f32::NAN, 0.25);
        assert!(value.is_nan());
    }

    #[test]
    fn audio_commands_preserve_playback_payloads() {
        let cut = GameplayMusicCut {
            start_sec: 1.0,
            length_sec: 2.0,
            fade_in_sec: 0.25,
            fade_out_sec: 0.5,
        };
        let command = GameplayAudioCommand::PlayMusic {
            path: PathBuf::from("songs/test.ogg"),
            cut,
            looping: false,
            rate: 1.25,
        };

        assert_eq!(
            command,
            GameplayAudioCommand::PlayMusic {
                path: PathBuf::from("songs/test.ogg"),
                cut,
                looping: false,
                rate: 1.25,
            }
        );
        assert_eq!(
            GameplayAudioCommand::StopMusic,
            GameplayAudioCommand::StopMusic
        );
        assert_eq!(
            GameplayAudioCommand::PlayPreloadedAssistTick("assets/sounds/assist_tick.ogg"),
            GameplayAudioCommand::PlayPreloadedAssistTick("assets/sounds/assist_tick.ogg")
        );
    }

    #[test]
    fn feedback_durations_match_runtime_policy() {
        assert_near(HOLD_JUDGMENT_TOTAL_DURATION, 0.8);
        assert_near(HELD_MISS_TOTAL_DURATION, 0.5);
        assert_near(RECEPTOR_GLOW_DURATION, 0.2);
        assert_near(COMBO_HUNDRED_MILESTONE_DURATION, 0.6);
        assert_near(COMBO_THOUSAND_MILESTONE_DURATION, 0.7);
    }

    #[test]
    fn combo_milestone_trigger_appends_or_resets_existing_kind() {
        let mut milestones = Vec::new();
        trigger_combo_milestone(&mut milestones, ComboMilestoneKind::Hundred);

        assert_eq!(milestones.len(), 1);
        assert_eq!(milestones[0].kind, ComboMilestoneKind::Hundred);
        assert_near(milestones[0].elapsed, 0.0);

        milestones[0].elapsed = 0.4;
        trigger_combo_milestone(&mut milestones, ComboMilestoneKind::Hundred);
        assert_eq!(milestones.len(), 1);
        assert_near(milestones[0].elapsed, 0.0);

        trigger_combo_milestone(&mut milestones, ComboMilestoneKind::Thousand);
        assert_eq!(milestones.len(), 2);
        assert_eq!(milestones[1].kind, ComboMilestoneKind::Thousand);
        assert_near(milestones[1].elapsed, 0.0);
    }

    #[test]
    fn combo_milestone_tick_ages_and_drops_expired_kinds() {
        assert_near(
            combo_milestone_duration(ComboMilestoneKind::Hundred),
            COMBO_HUNDRED_MILESTONE_DURATION,
        );
        assert_near(
            combo_milestone_duration(ComboMilestoneKind::Thousand),
            COMBO_THOUSAND_MILESTONE_DURATION,
        );

        let mut milestones = vec![
            ActiveComboMilestone {
                kind: ComboMilestoneKind::Hundred,
                elapsed: COMBO_HUNDRED_MILESTONE_DURATION - 0.1,
            },
            ActiveComboMilestone {
                kind: ComboMilestoneKind::Thousand,
                elapsed: COMBO_THOUSAND_MILESTONE_DURATION - 0.2,
            },
        ];

        tick_combo_milestones(&mut milestones, 0.15);

        assert_eq!(milestones.len(), 1);
        assert_eq!(milestones[0].kind, ComboMilestoneKind::Thousand);
        assert_near(
            milestones[0].elapsed,
            COMBO_THOUSAND_MILESTONE_DURATION - 0.05,
        );

        tick_combo_milestones(&mut milestones, 0.05);
        assert!(milestones.is_empty());
    }

    #[test]
    fn combo_update_feedback_resets_window_counts_on_break() {
        let mut counts = WindowCounts {
            w0: 1,
            w1: 2,
            w2: 3,
            w3: 4,
            w4: 5,
            w5: 6,
            miss: 7,
        };
        let mut milestones = Vec::new();

        apply_combo_update_feedback(
            &mut counts,
            &mut milestones,
            ComboUpdate {
                combo_broken: true,
                ..ComboUpdate::default()
            },
        );

        assert_eq!(counts.w0, 0);
        assert_eq!(counts.w1, 0);
        assert_eq!(counts.w2, 0);
        assert_eq!(counts.w3, 0);
        assert_eq!(counts.w4, 0);
        assert_eq!(counts.w5, 0);
        assert_eq!(counts.miss, 0);
        assert!(milestones.is_empty());
    }

    #[test]
    fn combo_update_feedback_triggers_milestones() {
        let mut counts = WindowCounts::default();
        let mut milestones = Vec::new();

        apply_combo_update_feedback(
            &mut counts,
            &mut milestones,
            ComboUpdate {
                hit_hundred_milestone: true,
                hit_thousand_milestone: true,
                ..ComboUpdate::default()
            },
        );

        assert_eq!(milestones.len(), 2);
        assert_eq!(milestones[0].kind, ComboMilestoneKind::Thousand);
        assert_eq!(milestones[1].kind, ComboMilestoneKind::Hundred);
    }

    #[test]
    fn combo_update_feedback_resets_existing_milestone_elapsed() {
        let mut counts = WindowCounts::default();
        let mut milestones = vec![ActiveComboMilestone {
            kind: ComboMilestoneKind::Hundred,
            elapsed: 0.5,
        }];

        apply_combo_update_feedback(
            &mut counts,
            &mut milestones,
            ComboUpdate {
                hit_hundred_milestone: true,
                ..ComboUpdate::default()
            },
        );

        assert_eq!(milestones.len(), 1);
        assert_eq!(milestones[0].kind, ComboMilestoneKind::Hundred);
        assert_near(milestones[0].elapsed, 0.0);
    }

    #[test]
    fn mine_hit_combo_policy_preserves_combo_by_default() {
        let mut state = ComboState {
            combo: 50,
            miss_combo: 3,
            full_combo_grade: Some(JudgeGrade::Great),
            current_combo_grade: Some(JudgeGrade::Great),
            ..ComboState::default()
        };
        let original = state;

        let update = apply_mine_hit_combo_policy(&mut state);

        assert_eq!(state, original);
        assert_eq!(update, ComboUpdate::default());
    }

    #[test]
    fn hold_success_combo_policy_preserves_miss_combo() {
        let mut state = ComboState {
            combo: 12,
            miss_combo: 4,
            current_combo_grade: Some(JudgeGrade::Excellent),
            ..ComboState::default()
        };
        let original = state;

        let update = apply_hold_success_combo_policy(&mut state);

        assert_eq!(state, original);
        assert_eq!(update, ComboUpdate::default());
    }

    #[test]
    fn hold_let_go_combo_policy_clears_full_combo_without_breaking_combo() {
        assert!(!COMBO_BREAK_ON_IMMEDIATE_HOLD_LET_GO);

        let mut state = ComboState {
            combo: 42,
            miss_combo: 5,
            full_combo_grade: Some(JudgeGrade::Great),
            current_combo_grade: Some(JudgeGrade::Great),
            ..ComboState::default()
        };

        let update = apply_hold_let_go_combo_policy(&mut state);

        assert_eq!(state.combo, 42);
        assert_eq!(state.miss_combo, 5);
        assert!(state.full_combo_grade.is_none());
        assert_eq!(state.current_combo_grade, Some(JudgeGrade::Great));
        assert!(state.first_fc_attempt_broken);
        assert_eq!(update, ComboUpdate::default());
    }

    #[test]
    fn column_flash_duration_uses_short_miss_and_judgment_fade() {
        assert_near(
            column_flash_duration(JudgeGrade::Miss),
            COLUMN_FLASH_MISS_DURATION,
        );
        assert_near(
            column_flash_duration(JudgeGrade::Fantastic),
            COLUMN_FLASH_JUDGMENT_DURATION,
        );
        assert_near(
            column_flash_duration(JudgeGrade::WayOff),
            COLUMN_FLASH_JUDGMENT_DURATION,
        );
    }

    #[test]
    fn column_flash_options_gate_grade_bits() {
        let options = ColumnFlashOptions {
            enabled: true,
            great: true,
            miss: true,
            ..ColumnFlashOptions::default()
        };

        assert!(column_flash_enabled_for_options(
            options,
            JudgeGrade::Great,
            false
        ));
        assert!(column_flash_enabled_for_options(
            options,
            JudgeGrade::Miss,
            false
        ));
        assert!(!column_flash_enabled_for_options(
            options,
            JudgeGrade::Excellent,
            false
        ));
        assert!(!column_flash_enabled_for_options(
            ColumnFlashOptions {
                enabled: false,
                great: true,
                ..ColumnFlashOptions::default()
            },
            JudgeGrade::Great,
            false
        ));
    }

    #[test]
    fn column_flash_options_split_fantastic_colors() {
        let blue_only = ColumnFlashOptions {
            enabled: true,
            blue_fantastic: true,
            ..ColumnFlashOptions::default()
        };
        let white_only = ColumnFlashOptions {
            enabled: true,
            white_fantastic: true,
            ..ColumnFlashOptions::default()
        };

        assert!(column_flash_enabled_for_options(
            blue_only,
            JudgeGrade::Fantastic,
            true
        ));
        assert!(!column_flash_enabled_for_options(
            blue_only,
            JudgeGrade::Fantastic,
            false
        ));
        assert!(column_flash_enabled_for_options(
            white_only,
            JudgeGrade::Fantastic,
            false
        ));
        assert!(!column_flash_enabled_for_options(
            white_only,
            JudgeGrade::Fantastic,
            true
        ));
    }

    #[test]
    fn feedback_explosion_slots_tick_elapsed_and_expire() {
        let mut tap = Some(ActiveTapExplosion {
            window: "W1",
            bright: false,
            elapsed: 0.2,
            duration: 0.5,
            start_beat: 8.0,
        });
        tick_tap_explosion_slot(&mut tap, 0.2);
        assert_near(tap.expect("tap explosion should remain").elapsed, 0.4);
        tick_tap_explosion_slot(&mut tap, 0.1);
        assert!(tap.is_none());

        let mut instant_tap = Some(ActiveTapExplosion {
            window: "Miss",
            bright: false,
            elapsed: 0.0,
            duration: 0.0,
            start_beat: 0.0,
        });
        tick_tap_explosion_slot(&mut instant_tap, 0.0);
        assert!(instant_tap.is_none());

        let mut mine = Some(ActiveMineExplosion {
            elapsed: 0.1,
            duration: 0.3,
            started_at_screen_s: 2.0,
        });
        tick_mine_explosion_slot(&mut mine, 0.1);
        assert_near(
            mine.as_ref().expect("mine explosion should remain").elapsed,
            0.2,
        );
        tick_mine_explosion_slot(&mut mine, 0.1);
        assert!(mine.is_none());
    }

    #[test]
    fn feedback_slots_expire_at_runtime_durations() {
        let flash = ActiveColumnFlash {
            grade: JudgeGrade::Great,
            blue_fantastic: false,
            started_at_screen_s: 10.0,
        };
        assert!(!column_flash_expired_at(
            flash,
            10.0 + COLUMN_FLASH_JUDGMENT_DURATION - 0.001
        ));
        assert!(column_flash_expired_at(
            flash,
            10.0 + COLUMN_FLASH_JUDGMENT_DURATION + 0.001
        ));

        let miss_flash = ActiveColumnFlash {
            grade: JudgeGrade::Miss,
            ..flash
        };
        assert!(column_flash_expired_at(
            miss_flash,
            10.0 + COLUMN_FLASH_MISS_DURATION + 0.001
        ));

        let hold = HoldJudgmentRenderInfo {
            result: HoldResult::Held,
            started_at_screen_s: 3.0,
        };
        assert!(!hold_judgment_expired_at(
            hold,
            3.0 + HOLD_JUDGMENT_TOTAL_DURATION - 0.001
        ));
        assert!(hold_judgment_expired_at(
            hold,
            3.0 + HOLD_JUDGMENT_TOTAL_DURATION + 0.001
        ));

        let held_miss = HeldMissRenderInfo {
            started_at_screen_s: 4.0,
        };
        assert!(!held_miss_judgment_expired_at(
            held_miss,
            4.0 + HELD_MISS_TOTAL_DURATION - 0.001
        ));
        assert!(held_miss_judgment_expired_at(
            held_miss,
            4.0 + HELD_MISS_TOTAL_DURATION + 0.001
        ));
    }

    #[test]
    fn final_note_result_effects_mark_first_finalization() {
        let judgment = test_judgment(JudgeGrade::Great);

        assert_eq!(
            final_note_result_effects(true, &judgment, 2, MAX_COLS),
            FinalNoteResultEffects {
                mark_row_finalized: true,
                trigger_miss_flash_column: None,
                held_miss_column: None,
            }
        );
        assert_eq!(
            final_note_result_effects(false, &judgment, 2, MAX_COLS),
            FinalNoteResultEffects::default()
        );
    }

    #[test]
    fn final_note_result_effects_trigger_miss_feedback() {
        let judgment = test_judgment(JudgeGrade::Miss);

        assert_eq!(
            final_note_result_effects(true, &judgment, 2, MAX_COLS),
            FinalNoteResultEffects {
                mark_row_finalized: true,
                trigger_miss_flash_column: Some(2),
                held_miss_column: None,
            }
        );
    }

    #[test]
    fn final_note_result_effects_record_held_miss_column_when_in_bounds() {
        let mut judgment = test_judgment(JudgeGrade::Miss);
        judgment.miss_because_held = true;

        assert_eq!(
            final_note_result_effects(true, &judgment, 2, MAX_COLS),
            FinalNoteResultEffects {
                mark_row_finalized: true,
                trigger_miss_flash_column: Some(2),
                held_miss_column: Some(2),
            }
        );
        assert_eq!(
            final_note_result_effects(true, &judgment, MAX_COLS, MAX_COLS),
            FinalNoteResultEffects {
                mark_row_finalized: true,
                trigger_miss_flash_column: Some(MAX_COLS),
                held_miss_column: None,
            }
        );
    }

    #[test]
    fn register_provisional_early_note_result_only_sets_first_result() {
        let first = test_judgment(JudgeGrade::Great);
        let second = test_judgment(JudgeGrade::Miss);
        let mut note = test_note(NoteType::Tap, None, false);

        assert!(register_provisional_early_note_result(&mut note, first));
        assert_eq!(
            note.early_result.as_ref().map(|j| j.grade),
            Some(first.grade)
        );
        assert!(!register_provisional_early_note_result(&mut note, second));
        assert_eq!(
            note.early_result.as_ref().map(|j| j.grade),
            Some(first.grade)
        );
    }

    #[test]
    fn apply_final_note_result_sets_result_and_returns_first_effects() {
        let first = test_judgment(JudgeGrade::Miss);
        let second = test_judgment(JudgeGrade::Great);
        let mut note = test_note(NoteType::Tap, None, false);
        note.column = 2;

        let first_effects = apply_final_note_result(&mut note, first, MAX_COLS);
        assert_eq!(note.result.as_ref().map(|j| j.grade), Some(first.grade));
        assert_eq!(
            first_effects,
            FinalNoteResultEffects {
                mark_row_finalized: true,
                trigger_miss_flash_column: Some(2),
                held_miss_column: None,
            }
        );

        let second_effects = apply_final_note_result(&mut note, second, MAX_COLS);
        assert_eq!(note.result.as_ref().map(|j| j.grade), Some(second.grade));
        assert_eq!(second_effects, FinalNoteResultEffects::default());
    }

    #[test]
    fn danger_health_state_uses_life_threshold_and_fail_state() {
        assert_eq!(danger_health_state(1.0, false), HealthState::Alive);
        assert_eq!(danger_health_state(0.2, false), HealthState::Alive);
        assert_eq!(danger_health_state(0.199, false), HealthState::Danger);
        assert_eq!(danger_health_state(0.0, false), HealthState::Dead);
        assert_eq!(danger_health_state(1.0, true), HealthState::Dead);
    }

    #[test]
    fn danger_fx_enters_danger_and_flashes_recovery() {
        let mut fx = DangerFx::default();
        update_danger_fx_for_health(&mut fx, HealthState::Danger, 10.0, false);

        assert_eq!(danger_fx_rgba(&fx, 10.0), [0.0, 0.0, 0.0, 0.0]);
        assert!(danger_fx_rgba(&fx, 10.3)[3] > 0.0);

        update_danger_fx_for_health(&mut fx, HealthState::Alive, 11.0, false);
        let flash = danger_fx_rgba(&fx, 11.15);
        assert_eq!(flash[0], 0.0);
        assert_eq!(flash[1], 1.0);
        assert_eq!(flash[2], 0.0);
        assert!(flash[3] > 0.0);
    }

    #[test]
    fn danger_fx_hide_danger_only_flashes_death() {
        let mut fx = DangerFx::default();
        update_danger_fx_for_health(&mut fx, HealthState::Danger, 1.0, true);
        assert_eq!(danger_fx_rgba(&fx, 1.2), [0.0, 0.0, 0.0, 0.0]);

        update_danger_fx_for_health(&mut fx, HealthState::Dead, 2.0, true);
        let flash = danger_fx_rgba(&fx, 2.15);
        assert_eq!(flash[0], 1.0);
        assert_eq!(flash[1], 0.0);
        assert_eq!(flash[2], 0.0);
        assert!(flash[3] > 0.0);
    }

    #[test]
    fn error_bar_window_indices_follow_timing_window_order() {
        assert_eq!(error_bar_window_ix(TimingWindow::W0), 0);
        assert_eq!(error_bar_window_ix(TimingWindow::W1), 1);
        assert_eq!(error_bar_window_ix(TimingWindow::W5), 5);
    }

    #[test]
    fn error_bar_push_tick_overwrites_single_or_rotates_multi() {
        let mut single = [None; 2];
        let mut single_next = 1;
        error_bar_push_tick(
            &mut single,
            &mut single_next,
            false,
            ErrorBarTick {
                started_at: 1.0,
                offset_s: 0.010,
                window: TimingWindow::W1,
            },
        );
        assert_eq!(single_next, 0);
        assert_eq!(single[0].map(|tick| tick.offset_s), Some(0.010));
        assert!(single[1].is_none());

        let mut multi = [None; 2];
        let mut multi_next = 0;
        for offset_s in [0.010, 0.020, 0.030] {
            error_bar_push_tick(
                &mut multi,
                &mut multi_next,
                true,
                ErrorBarTick {
                    started_at: 1.0,
                    offset_s,
                    window: TimingWindow::W1,
                },
            );
        }
        assert_eq!(multi_next, 1);
        assert_eq!(multi[0].map(|tick| tick.offset_s), Some(0.030));
        assert_eq!(multi[1].map(|tick| tick.offset_s), Some(0.020));
    }

    #[test]
    fn average_error_bar_interval_controls_sample_window() {
        let mut broad = VecDeque::from([(0.0, 0.010), (100.0, 0.020), (200.0, 0.030)]);
        let (broad_avg, broad_count) = error_bar_average_offset_s(&mut broad, 0.5, 0.050, 400);
        assert!((broad_avg - 0.040).abs() <= 1e-6);
        assert_eq!(broad_count, 2);

        let mut narrow = VecDeque::from([(0.0, 0.010), (100.0, 0.020), (200.0, 0.030)]);
        let (narrow_avg, narrow_count) = error_bar_average_offset_s(&mut narrow, 0.5, 0.050, 200);
        assert!((narrow_avg - 0.050).abs() <= 1e-6);
        assert_eq!(narrow_count, 1);
    }

    #[test]
    fn long_average_uses_short_interval_times_sixteen() {
        let mut samples = VecDeque::from([(0.0, 0.010), (3000.0, 0.020), (3300.0, 0.030)]);
        let mut total = 0.060;

        let (mean, len) = error_bar_long_term_offset_s(&mut samples, &mut total, 6.5, 0.040, 400);

        assert_eq!(len, 3);
        assert_eq!(samples.front().map(|(t, _)| *t), Some(3000.0));
        assert!((mean - 0.030).abs() <= 1e-6);
    }

    #[test]
    fn long_average_tracks_short_interval_changes() {
        let mut samples = VecDeque::from([(0.0, 0.010), (3000.0, 0.020), (3300.0, 0.030)]);
        let mut total = 0.060;

        let (mean, len) = error_bar_long_term_offset_s(&mut samples, &mut total, 6.5, 0.040, 200);

        assert_eq!(len, 2);
        assert_eq!(samples.front().map(|(t, _)| *t), Some(3300.0));
        assert!((mean - 0.035).abs() <= 1e-6);
    }

    #[test]
    fn input_queue_capacity_scales_by_field_count() {
        assert_eq!(input_queue_cap(0), GAMEPLAY_INPUT_BACKLOG_WARN);
        assert_eq!(input_queue_cap(4), GAMEPLAY_INPUT_BACKLOG_WARN);
        assert_eq!(input_queue_cap(5), GAMEPLAY_INPUT_BACKLOG_WARN * 2);
        assert_eq!(input_queue_cap(8), GAMEPLAY_INPUT_BACKLOG_WARN * 2);
    }

    #[test]
    fn remap_live_input_lane_filters_other_side_for_single_p1() {
        assert_eq!(
            remap_live_input_lane(
                GameplayInputPlayStyle::Single,
                GameplayInputPlayerSide::P1,
                Lane::Left,
            ),
            Some(Lane::Left)
        );
        assert_eq!(
            remap_live_input_lane(
                GameplayInputPlayStyle::Single,
                GameplayInputPlayerSide::P1,
                Lane::P2Left,
            ),
            None
        );
    }

    #[test]
    fn remap_live_input_lane_remaps_single_p2_into_first_field() {
        assert_eq!(
            remap_live_input_lane(
                GameplayInputPlayStyle::Single,
                GameplayInputPlayerSide::P2,
                Lane::Left,
            ),
            None
        );
        assert_eq!(
            remap_live_input_lane(
                GameplayInputPlayStyle::Single,
                GameplayInputPlayerSide::P2,
                Lane::P2Left,
            ),
            Some(Lane::Left)
        );
        assert_eq!(
            remap_live_input_lane(
                GameplayInputPlayStyle::Single,
                GameplayInputPlayerSide::P2,
                Lane::P2Right,
            ),
            Some(Lane::Right)
        );
    }

    #[test]
    fn remap_live_input_lane_keeps_double_and_versus_lanes() {
        assert_eq!(
            remap_live_input_lane(
                GameplayInputPlayStyle::Double,
                GameplayInputPlayerSide::P2,
                Lane::P2Left,
            ),
            Some(Lane::P2Left)
        );
        assert_eq!(
            remap_live_input_lane(
                GameplayInputPlayStyle::Versus,
                GameplayInputPlayerSide::P1,
                Lane::P2Down,
            ),
            Some(Lane::P2Down)
        );
    }

    #[test]
    fn active_input_slot_helpers_normalize_and_match_lanes() {
        assert_eq!(MAX_ACTIVE_INPUT_SLOTS, 128);
        assert_eq!(input_lane_bit(0), 0b0000_0001);
        assert_eq!(input_lane_bit(3), 0b0000_1000);
        assert_eq!(normalized_input_slot(u32::MAX, 4, u32::MAX), 4);
        assert_eq!(normalized_input_slot(12, 4, u32::MAX), 12);

        let slots = [
            ActiveInputSlot {
                source: InputSource::Keyboard,
                input_slot: 7,
                lane_mask: input_lane_bit(2) | input_lane_bit(4),
            },
            ActiveInputSlot {
                source: InputSource::Gamepad,
                input_slot: 7,
                lane_mask: input_lane_bit(1),
            },
            EMPTY_ACTIVE_INPUT_SLOT,
        ];

        assert!(active_input_slot_lane_is_down(
            &slots,
            2,
            2,
            InputSource::Keyboard,
            7,
        ));
        assert!(!active_input_slot_lane_is_down(
            &slots,
            2,
            1,
            InputSource::Keyboard,
            7,
        ));
        assert!(active_input_slot_lane_is_down(
            &slots,
            2,
            1,
            InputSource::Gamepad,
            7,
        ));
        assert!(!active_input_slot_lane_is_down(
            &slots,
            1,
            1,
            InputSource::Gamepad,
            7,
        ));
    }

    #[test]
    fn active_input_slot_update_holds_until_last_alias_release() {
        let mut slots = [EMPTY_ACTIVE_INPUT_SLOT; 4];
        let mut slot_count = 0;
        let mut lane_counts = [0_u16; MAX_COLS];
        let mut transitions = Vec::new();

        for (source, slot, pressed) in [
            (InputSource::Keyboard, 10, true),
            (InputSource::Keyboard, 11, true),
            (InputSource::Keyboard, 10, false),
            (InputSource::Keyboard, 11, false),
            (InputSource::Gamepad, 7, true),
            (InputSource::Keyboard, 12, true),
            (InputSource::Gamepad, 7, false),
            (InputSource::Keyboard, 12, false),
        ] {
            let update = update_active_input_slot(
                &mut slots,
                &mut slot_count,
                &mut lane_counts,
                0,
                source,
                slot,
                pressed,
            );
            transitions.push((update.was_down, update.is_down, update.slot_was_down));
            assert!(!update.slot_table_full);
        }

        assert_eq!(
            transitions,
            vec![
                (false, true, false),
                (true, true, false),
                (true, true, true),
                (true, false, true),
                (false, true, false),
                (true, true, false),
                (true, true, true),
                (true, false, true),
            ]
        );
        assert_eq!(slot_count, 0);
        assert_eq!(lane_counts[0], 0);
    }

    #[test]
    fn active_input_slot_update_reports_full_table_without_mutating_counts() {
        let mut slots = [EMPTY_ACTIVE_INPUT_SLOT; 1];
        let mut slot_count = 0;
        let mut lane_counts = [0_u16; MAX_COLS];

        let first = update_active_input_slot(
            &mut slots,
            &mut slot_count,
            &mut lane_counts,
            0,
            InputSource::Keyboard,
            10,
            true,
        );
        let full = update_active_input_slot(
            &mut slots,
            &mut slot_count,
            &mut lane_counts,
            1,
            InputSource::Keyboard,
            11,
            true,
        );

        assert_eq!(
            first,
            LaneInputUpdate {
                was_down: false,
                is_down: true,
                slot_was_down: false,
                slot_table_full: false,
            }
        );
        assert_eq!(
            full,
            LaneInputUpdate {
                was_down: false,
                is_down: false,
                slot_was_down: false,
                slot_table_full: true,
            }
        );
        assert_eq!(slot_count, 1);
        assert_eq!(lane_counts[0], 1);
        assert_eq!(lane_counts[1], 0);
    }

    #[test]
    fn autosync_sample_math_rounds_mean_and_measures_stddev() {
        assert_eq!(AUTOSYNC_OFFSET_SAMPLE_COUNT, 24);
        assert_near(AUTOSYNC_STDDEV_MAX_SECONDS, 0.03);

        let positive = [1_000_000_i64; AUTOSYNC_OFFSET_SAMPLE_COUNT];
        assert_eq!(autosync_mean_ns(&positive), 1_000_000);
        assert_near(
            autosync_stddev_seconds(&positive, autosync_mean_ns(&positive)),
            0.0,
        );

        let mut mixed = [0_i64; AUTOSYNC_OFFSET_SAMPLE_COUNT];
        mixed[0] = 1;
        assert_eq!(autosync_mean_ns(&mixed), 0);
        mixed[0] = -1;
        assert_eq!(autosync_mean_ns(&mixed), 0);

        let mut spread = [0_i64; AUTOSYNC_OFFSET_SAMPLE_COUNT];
        spread[0] = song_time_ns_from_seconds(0.03);
        spread[1] = song_time_ns_from_seconds(-0.03);
        let stddev = autosync_stddev_seconds(&spread, autosync_mean_ns(&spread));
        assert!(stddev > 0.008);
        assert!(stddev < AUTOSYNC_STDDEV_MAX_SECONDS);
    }

    #[test]
    fn autosync_offset_sample_buffers_until_full() {
        let mut samples = [0; AUTOSYNC_OFFSET_SAMPLE_COUNT];
        let mut sample_count = 0;

        for _ in 0..AUTOSYNC_OFFSET_SAMPLE_COUNT - 1 {
            let result = apply_autosync_offset_sample(
                &mut samples,
                &mut sample_count,
                AutosyncMode::Song,
                song_time_ns_from_seconds(0.010),
            );

            assert_eq!(result, AutosyncSampleResult::default());
        }

        assert_eq!(sample_count, AUTOSYNC_OFFSET_SAMPLE_COUNT - 1);
    }

    #[test]
    fn autosync_offset_sample_returns_stable_song_correction() {
        let mut samples = [0; AUTOSYNC_OFFSET_SAMPLE_COUNT];
        let mut sample_count = 0;

        let mut result = AutosyncSampleResult::default();
        for _ in 0..AUTOSYNC_OFFSET_SAMPLE_COUNT {
            result = apply_autosync_offset_sample(
                &mut samples,
                &mut sample_count,
                AutosyncMode::Song,
                song_time_ns_from_seconds(0.010),
            );
        }

        assert_eq!(sample_count, 0);
        assert_eq!(
            result.correction,
            Some(AutosyncOffsetCorrection::Song(0.010))
        );
        assert_eq!(result.standard_deviation, Some(0.0));
    }

    #[test]
    fn autosync_offset_sample_returns_machine_correction() {
        let mut samples = [song_time_ns_from_seconds(0.020); AUTOSYNC_OFFSET_SAMPLE_COUNT];
        let mut sample_count = AUTOSYNC_OFFSET_SAMPLE_COUNT - 1;

        let result = apply_autosync_offset_sample(
            &mut samples,
            &mut sample_count,
            AutosyncMode::Machine,
            song_time_ns_from_seconds(0.020),
        );

        assert_eq!(
            result.correction,
            Some(AutosyncOffsetCorrection::Machine(0.020))
        );
        assert_eq!(result.standard_deviation, Some(0.0));
    }

    #[test]
    fn autosync_offset_sample_no_correction_for_noisy_samples() {
        let mut samples = [0; AUTOSYNC_OFFSET_SAMPLE_COUNT];
        samples[0] = song_time_ns_from_seconds(0.20);
        let mut sample_count = AUTOSYNC_OFFSET_SAMPLE_COUNT - 1;

        let result = apply_autosync_offset_sample(
            &mut samples,
            &mut sample_count,
            AutosyncMode::Song,
            song_time_ns_from_seconds(-0.20),
        );

        assert_eq!(sample_count, 0);
        assert!(result.standard_deviation.unwrap() > AUTOSYNC_STDDEV_MAX_SECONDS);
        assert_eq!(result.correction, None);
    }

    #[test]
    fn autosync_offset_sample_ignores_invalid_and_off_mode() {
        let mut samples = [0; AUTOSYNC_OFFSET_SAMPLE_COUNT];
        let mut sample_count = 0;

        let invalid = apply_autosync_offset_sample(
            &mut samples,
            &mut sample_count,
            AutosyncMode::Song,
            INVALID_SONG_TIME_NS,
        );
        let off = apply_autosync_offset_sample(
            &mut samples,
            &mut sample_count,
            AutosyncMode::Off,
            song_time_ns_from_seconds(0.010),
        );

        assert_eq!(invalid, AutosyncSampleResult::default());
        assert_eq!(off, AutosyncSampleResult::default());
        assert_eq!(sample_count, 0);
    }

    #[test]
    fn display_clock_snaps_on_first_update() {
        let mut clock = FrameStableDisplayClock::new(song_time_ns_from_seconds(10.0));
        let mut events = Vec::new();

        let display_time = song_time_ns_to_seconds(frame_stable_display_clock_step(
            &mut clock,
            song_time_ns_from_seconds(20.0),
            1.0 / 60.0,
            1.0,
            true,
            |event| events.push(event.kind),
        ));

        assert_near(display_time, 20.0);
        assert!(events.is_empty());
        assert_near(clock.health().error_seconds, 0.0);
        assert!(!clock.health().catching_up);
    }

    #[test]
    fn display_clock_advances_smoothly_toward_target() {
        let mut clock = FrameStableDisplayClock::new(song_time_ns_from_seconds(100.0));
        let mut events = Vec::new();

        let display_time = song_time_ns_to_seconds(frame_stable_display_clock_step(
            &mut clock,
            song_time_ns_from_seconds(100.05),
            1.0 / 60.0,
            1.0,
            false,
            |event| events.push(event.kind),
        ));

        assert!(display_time > 100.0);
        assert!(display_time < 100.05);
        assert!(events.contains(&DisplayClockDiagEventKind::TargetJump));
        assert!(clock.health().catching_up);
    }

    #[test]
    fn display_clock_resets_when_far_from_target() {
        let mut clock = FrameStableDisplayClock::new(song_time_ns_from_seconds(100.0));
        let mut events = Vec::new();

        let display_time = song_time_ns_to_seconds(frame_stable_display_clock_step(
            &mut clock,
            song_time_ns_from_seconds(101.0),
            1.0 / 60.0,
            1.0,
            false,
            |event| events.push(event.kind),
        ));

        assert_near(display_time, 101.0);
        assert_eq!(events, vec![DisplayClockDiagEventKind::ResetJump]);
        assert_near(clock.health().error_seconds, 0.0);
        assert!(!clock.health().catching_up);
    }

    #[test]
    fn timing_tick_mode_cycles_and_labels_match_runtime_text() {
        assert_eq!(
            next_timing_tick_mode(GameplayTimingTickMode::Off),
            GameplayTimingTickMode::Assist
        );
        assert_eq!(
            next_timing_tick_mode(GameplayTimingTickMode::Assist),
            GameplayTimingTickMode::Hit
        );
        assert_eq!(
            next_timing_tick_mode(GameplayTimingTickMode::Hit),
            GameplayTimingTickMode::Off
        );

        assert_eq!(
            timing_tick_mode_status_line(GameplayTimingTickMode::Off),
            None
        );
        assert_eq!(
            timing_tick_mode_status_line(GameplayTimingTickMode::Assist),
            Some("Assist Tick")
        );
        assert_eq!(
            timing_tick_mode_status_line(GameplayTimingTickMode::Hit),
            Some("Hit Tick")
        );
        assert_eq!(
            timing_tick_mode_debug_label(GameplayTimingTickMode::Off),
            "off"
        );
        assert_eq!(
            timing_tick_mode_debug_label(GameplayTimingTickMode::Assist),
            "assist tick"
        );
        assert_eq!(
            timing_tick_mode_debug_label(GameplayTimingTickMode::Hit),
            "hit tick"
        );
    }

    #[test]
    fn offset_adjust_keys_map_to_slots_and_deltas() {
        assert_near(OFFSET_ADJUST_STEP_SECONDS, 0.001);
        assert_eq!(
            offset_adjust_slot_for_key(GameplayOffsetAdjustKey::Decrease),
            0
        );
        assert_eq!(
            offset_adjust_slot_for_key(GameplayOffsetAdjustKey::Increase),
            1
        );
        assert_near(
            offset_adjust_delta_for_key(GameplayOffsetAdjustKey::Decrease),
            -0.001,
        );
        assert_near(
            offset_adjust_delta_for_key(GameplayOffsetAdjustKey::Increase),
            0.001,
        );
    }

    #[test]
    fn offset_adjust_repeat_ready_respects_delay_and_interval() {
        assert!(!offset_adjust_repeat_ready(
            OFFSET_ADJUST_REPEAT_DELAY - Duration::from_millis(1),
            OFFSET_ADJUST_REPEAT_INTERVAL,
        ));
        assert!(!offset_adjust_repeat_ready(
            OFFSET_ADJUST_REPEAT_DELAY,
            OFFSET_ADJUST_REPEAT_INTERVAL - Duration::from_millis(1),
        ));
        assert!(offset_adjust_repeat_ready(
            OFFSET_ADJUST_REPEAT_DELAY,
            OFFSET_ADJUST_REPEAT_INTERVAL,
        ));
    }

    #[test]
    fn offset_adjust_hold_state_starts_and_clears_slots() {
        let now = Instant::now();
        let mut held_since = [None; 2];
        let mut last_at = [None; 2];

        let delta = start_offset_adjust_hold_state(
            &mut held_since,
            &mut last_at,
            GameplayOffsetAdjustKey::Decrease,
            now,
        );

        assert_near(delta, -OFFSET_ADJUST_STEP_SECONDS);
        assert_eq!(held_since[0], Some(now));
        assert_eq!(last_at[0], Some(now));
        assert_eq!(held_since[1], None);
        assert_eq!(last_at[1], None);

        clear_offset_adjust_hold_state(
            &mut held_since,
            &mut last_at,
            GameplayOffsetAdjustKey::Decrease,
        );

        assert_eq!(held_since, [None; 2]);
        assert_eq!(last_at, [None; 2]);
    }

    #[test]
    fn offset_adjust_hold_state_ticks_after_delay_and_interval() {
        let start = Instant::now();
        let mut held_since = [None; 2];
        let mut last_at = [None; 2];
        start_offset_adjust_hold_state(
            &mut held_since,
            &mut last_at,
            GameplayOffsetAdjustKey::Increase,
            start,
        );

        assert_eq!(
            tick_offset_adjust_hold_state(
                &held_since,
                &mut last_at,
                GameplayOffsetAdjustKey::Increase,
                start + OFFSET_ADJUST_REPEAT_DELAY - Duration::from_millis(1),
            ),
            None
        );

        let first_tick = start + OFFSET_ADJUST_REPEAT_DELAY;
        assert_eq!(
            tick_offset_adjust_hold_state(
                &held_since,
                &mut last_at,
                GameplayOffsetAdjustKey::Increase,
                first_tick,
            ),
            Some(OFFSET_ADJUST_STEP_SECONDS)
        );
        assert_eq!(
            last_at[offset_adjust_slot_for_key(GameplayOffsetAdjustKey::Increase)],
            Some(first_tick)
        );

        assert_eq!(
            tick_offset_adjust_hold_state(
                &held_since,
                &mut last_at,
                GameplayOffsetAdjustKey::Increase,
                first_tick + OFFSET_ADJUST_REPEAT_INTERVAL - Duration::from_millis(1),
            ),
            None
        );
        assert_eq!(
            tick_offset_adjust_hold_state(
                &held_since,
                &mut last_at,
                GameplayOffsetAdjustKey::Increase,
                first_tick + OFFSET_ADJUST_REPEAT_INTERVAL,
            ),
            Some(OFFSET_ADJUST_STEP_SECONDS)
        );
    }

    #[test]
    fn replay_capacity_uses_recording_budget() {
        assert_eq!(replay_edge_cap(4, 0, true, 120.0), 0);
        assert_eq!(replay_edge_cap(4, 0, false, 0.0), 4 * 64);
        assert_eq!(
            replay_edge_cap(4, 0, false, 2.0),
            4 * 2 * REPLAY_EDGE_RATE_PER_SEC
        );
        assert_eq!(
            replay_edge_cap(4, 120, false, 2.0),
            4 * 2 * REPLAY_EDGE_RATE_PER_SEC
        );
        assert_eq!(replay_edge_cap(4, 4000, false, 2.0), 8000);
        assert_eq!(
            replay_edge_cap(8, 1000, false, 1.0),
            8 * REPLAY_EDGE_RATE_PER_SEC
        );
    }

    fn replay_input_edge_at(lane_index: u8, time_ns: SongTimeNs) -> ReplayInputEdge {
        ReplayInputEdge {
            lane_index,
            pressed: true,
            source: InputSource::Keyboard,
            event_music_time_ns: time_ns,
        }
    }

    #[test]
    fn replay_input_builder_filters_invalid_lanes_and_times() {
        let input = [
            replay_input_edge_at(0, 10),
            replay_input_edge_at(4, 20),
            replay_input_edge_at(1, INVALID_SONG_TIME_NS),
        ];

        let replay = build_replay_input_edges(&input, 1, 4, 4, 0, [0, 0]);

        assert_eq!(replay.len(), 1);
        assert_eq!(replay[0].lane_index, 0);
        assert_eq!(replay[0].event_music_time_ns, 10);
    }

    #[test]
    fn replay_input_builder_shifts_by_player_beat_zero() {
        let input = [replay_input_edge_at(5, 100)];

        let replay = build_replay_input_edges(&input, 2, 4, 8, 30, [30, 80]);

        assert_eq!(replay.len(), 1);
        assert_eq!(replay[0].lane_index, 5);
        assert_eq!(replay[0].event_music_time_ns, 150);
    }

    #[test]
    fn replay_input_builder_skips_shift_for_invalid_recorded_offset() {
        let input = [replay_input_edge_at(5, 100)];

        let replay = build_replay_input_edges(&input, 2, 4, 8, INVALID_SONG_TIME_NS, [30, 80]);

        assert_eq!(replay.len(), 1);
        assert_eq!(replay[0].event_music_time_ns, 100);
    }

    #[test]
    fn replay_input_builder_sorts_only_when_needed() {
        let input = [
            replay_input_edge_at(0, 30),
            replay_input_edge_at(1, 10),
            replay_input_edge_at(2, 20),
        ];

        let replay = build_replay_input_edges(&input, 1, 4, 4, 0, [0, 0]);

        assert_eq!(
            replay
                .iter()
                .map(|edge| edge.event_music_time_ns)
                .collect::<Vec<_>>(),
            vec![10, 20, 30]
        );
    }

    fn recorded_edge_at(time_ns: SongTimeNs) -> RecordedLaneEdge {
        RecordedLaneEdge {
            lane_index: 1,
            pressed: true,
            source: InputSource::Keyboard,
            event_music_time_ns: time_ns,
        }
    }

    #[test]
    fn replay_edge_readiness_waits_for_event_time() {
        let input = [recorded_edge_at(20)];
        let mut cursor = 0;

        assert!(next_ready_replay_edge(&input, &mut cursor, 19).is_none());
        assert_eq!(cursor, 0);

        let edge = next_ready_replay_edge(&input, &mut cursor, 20).expect("ready edge");
        assert_eq!(edge.event_music_time_ns, 20);
        assert_eq!(cursor, 1);
    }

    #[test]
    fn replay_edge_readiness_consumes_in_order_and_handles_end() {
        let input = [recorded_edge_at(10), recorded_edge_at(30)];
        let mut cursor = 0;

        assert_eq!(
            next_ready_replay_edge(&input, &mut cursor, 30)
                .expect("first edge")
                .event_music_time_ns,
            10
        );
        assert_eq!(
            next_ready_replay_edge(&input, &mut cursor, 30)
                .expect("second edge")
                .event_music_time_ns,
            30
        );
        assert!(next_ready_replay_edge(&input, &mut cursor, 30).is_none());
        assert_eq!(cursor, 2);
    }

    #[test]
    fn replay_edge_readiness_ignores_cursor_beyond_input() {
        let input = [recorded_edge_at(10)];
        let mut cursor = 5;

        assert!(next_ready_replay_edge(&input, &mut cursor, 30).is_none());
        assert_eq!(cursor, 5);
    }

    #[test]
    fn column_scroll_dirs_apply_reverse_split_alternate_and_cross() {
        let reverse = column_scroll_dirs_for_flags(
            ColumnScrollFlags {
                reverse: true,
                ..ColumnScrollFlags::default()
            },
            4,
        );
        assert_eq!(&reverse[..4], &[-1.0, -1.0, -1.0, -1.0]);

        let split = column_scroll_dirs_for_flags(
            ColumnScrollFlags {
                split: true,
                ..ColumnScrollFlags::default()
            },
            4,
        );
        assert_eq!(&split[..4], &[1.0, 1.0, -1.0, -1.0]);

        let alternate = column_scroll_dirs_for_flags(
            ColumnScrollFlags {
                alternate: true,
                ..ColumnScrollFlags::default()
            },
            4,
        );
        assert_eq!(&alternate[..4], &[1.0, -1.0, 1.0, -1.0]);

        let cross = column_scroll_dirs_for_flags(
            ColumnScrollFlags {
                cross: true,
                ..ColumnScrollFlags::default()
            },
            4,
        );
        assert_eq!(&cross[..4], &[1.0, -1.0, -1.0, 1.0]);
    }

    #[test]
    fn scroll_reverse_percent_matches_itg_column_rules() {
        let options = ScrollReverseOptions {
            reverse: 1.0,
            split: 1.0,
            alternate: 1.0,
            cross: 0.0,
        };

        assert_near(scroll_reverse_percent_for_column(options, 0, 4), 1.0);
        assert_near(scroll_reverse_percent_for_column(options, 1, 4), 0.0);
        assert_near(scroll_reverse_percent_for_column(options, 2, 4), 0.0);
        assert_near(scroll_reverse_percent_for_column(options, 3, 4), 1.0);
    }

    #[test]
    fn scroll_reverse_percent_handles_cross_wrap_and_empty_fields() {
        let cross = ScrollReverseOptions {
            cross: 1.0,
            ..ScrollReverseOptions::default()
        };
        assert_near(scroll_reverse_percent_for_column(cross, 0, 4), 0.0);
        assert_near(scroll_reverse_percent_for_column(cross, 1, 4), 1.0);
        assert_near(scroll_reverse_percent_for_column(cross, 2, 4), 1.0);
        assert_near(scroll_reverse_percent_for_column(cross, 3, 4), 0.0);

        let wrapped = ScrollReverseOptions {
            reverse: 3.25,
            ..ScrollReverseOptions::default()
        };
        assert_near(scroll_reverse_percent_for_column(wrapped, 0, 4), 0.75);
        assert_near(scroll_reverse_percent_for_column(wrapped, 0, 0), 0.0);
    }

    #[test]
    fn scroll_reverse_scale_maps_percent_to_direction() {
        let reverse = ScrollReverseOptions {
            reverse: 1.0,
            ..ScrollReverseOptions::default()
        };
        assert_near(scroll_reverse_scale_for_column(reverse, 0, 4), -1.0);
        assert_near(
            scroll_reverse_scale_for_column(ScrollReverseOptions::default(), 0, 4),
            1.0,
        );
    }

    #[test]
    fn scroll_effects_build_from_flags_and_reuse_column_policy() {
        let scroll = ScrollEffects::from_flags(true, true, false, false, true);
        assert_near(scroll.reverse, 1.0);
        assert_near(scroll.split, 1.0);
        assert_near(scroll.alternate, 0.0);
        assert_near(scroll.cross, 0.0);
        assert_near(scroll.centered, 1.0);
        assert_near(scroll.reverse_percent_for_column(0, 4), 1.0);
        assert_near(scroll.reverse_percent_for_column(3, 4), 0.0);
        assert_near(scroll.reverse_scale_for_column(0, 4), -1.0);
    }

    #[test]
    fn song_lua_target_matching_uses_one_based_player_ids() {
        assert!(song_lua_target_matches_player(None, 0));
        assert!(song_lua_target_matches_player(Some(1), 0));
        assert!(song_lua_target_matches_player(Some(2), 1));
        assert!(!song_lua_target_matches_player(Some(2), 0));
    }

    #[test]
    fn song_lua_window_seconds_use_len_end_and_global_offset() {
        let timing = test_timing(ROWS_PER_BEAT as usize * 4);

        assert_eq!(
            song_lua_window_seconds(
                SongLuaRuntimeTimeUnit::Beat,
                1.0,
                2.0,
                SongLuaRuntimeSpanMode::Len,
                &timing,
                0.25,
            ),
            Some((1.0, 3.0))
        );
        assert_eq!(
            song_lua_window_seconds(
                SongLuaRuntimeTimeUnit::Beat,
                1.0,
                2.0,
                SongLuaRuntimeSpanMode::End,
                &timing,
                0.25,
            ),
            Some((1.0, 2.0))
        );
        assert_eq!(
            song_lua_window_seconds(
                SongLuaRuntimeTimeUnit::Second,
                5.0,
                7.0,
                SongLuaRuntimeSpanMode::End,
                &timing,
                0.25,
            ),
            Some((4.75, 6.75))
        );
    }

    #[test]
    fn song_lua_window_seconds_reject_invalid_ranges() {
        let timing = test_timing(ROWS_PER_BEAT as usize * 4);

        assert_eq!(
            song_lua_window_seconds(
                SongLuaRuntimeTimeUnit::Beat,
                3.0,
                2.0,
                SongLuaRuntimeSpanMode::End,
                &timing,
                0.0,
            ),
            None
        );
        assert_eq!(
            song_lua_window_seconds(
                SongLuaRuntimeTimeUnit::Second,
                f32::NAN,
                2.0,
                SongLuaRuntimeSpanMode::End,
                &timing,
                0.0,
            ),
            None
        );
    }

    #[test]
    fn song_lua_sustain_end_uses_span_policy_and_only_extends() {
        let timing = test_timing(ROWS_PER_BEAT as usize * 5);

        assert_near(
            song_lua_sustain_end_second(
                SongLuaRuntimeTimeUnit::Beat,
                1.0,
                2.0,
                SongLuaRuntimeSpanMode::Len,
                Some(1.0),
                &timing,
                0.0,
                3.0,
            ),
            4.0,
        );
        assert_near(
            song_lua_sustain_end_second(
                SongLuaRuntimeTimeUnit::Beat,
                1.0,
                2.0,
                SongLuaRuntimeSpanMode::End,
                Some(4.0),
                &timing,
                0.0,
                2.0,
            ),
            4.0,
        );
        assert_near(
            song_lua_sustain_end_second(
                SongLuaRuntimeTimeUnit::Beat,
                1.0,
                2.0,
                SongLuaRuntimeSpanMode::End,
                Some(1.5),
                &timing,
                0.0,
                2.0,
            ),
            2.0,
        );
    }

    #[test]
    fn song_lua_message_second_uses_beat_timing() {
        let timing = test_timing(ROWS_PER_BEAT as usize * 4);

        assert_eq!(song_lua_message_second(2.0, &timing, 99.0), Some(2.0));
    }

    #[test]
    fn scroll_overrides_approach_targets_by_speed() {
        let mut current = ScrollOverrides {
            reverse: Some(0.0),
            split: Some(1.0),
            cross: Some(0.25),
            ..ScrollOverrides::default()
        };
        let target = ScrollOverrides {
            reverse: Some(1.0),
            split: None,
            alternate: Some(0.5),
            cross: Some(1.0),
            ..ScrollOverrides::default()
        };
        let speed = ScrollOverrides {
            reverse: Some(2.0),
            alternate: None,
            cross: Some(0.0),
            ..ScrollOverrides::default()
        };
        let base = ScrollEffects {
            alternate: 0.25,
            ..ScrollEffects::default()
        };

        approach_scroll_overrides_to_target(&mut current, target, speed, base, 0.25);

        assert_near(current.reverse.unwrap(), 0.5);
        assert_eq!(current.split, None);
        assert_near(current.alternate.unwrap(), 0.5);
        assert_near(current.cross.unwrap(), 0.25);
    }

    #[test]
    fn column_scroll_dirs_apply_mods_per_four_panel_group() {
        let dirs = column_scroll_dirs_for_flags(
            ColumnScrollFlags {
                reverse: true,
                alternate: true,
                ..ColumnScrollFlags::default()
            },
            8,
        );
        assert_eq!(&dirs[..8], &[-1.0, 1.0, -1.0, 1.0, -1.0, 1.0, -1.0, 1.0]);
    }

    #[test]
    fn column_scroll_dirs_ignore_columns_after_requested_count() {
        let dirs = column_scroll_dirs_for_flags(
            ColumnScrollFlags {
                reverse: true,
                ..ColumnScrollFlags::default()
            },
            2,
        );
        assert_eq!(&dirs[..4], &[-1.0, -1.0, 1.0, 1.0]);

        let full = column_scroll_dirs_for_flags(
            ColumnScrollFlags {
                reverse: true,
                ..ColumnScrollFlags::default()
            },
            MAX_COLS + 10,
        );
        assert!(full.iter().all(|dir| *dir == -1.0));
    }

    #[test]
    fn gameplay_tween_eases_expected_curves() {
        assert_near(GameplayTween::Linear.ease(0.5), 0.5);
        assert_near(GameplayTween::Accelerate.ease(0.5), 0.25);
        assert_near(GameplayTween::Decelerate.ease(0.5), 0.75);
        assert_near(GameplayTween::Linear.ease(-1.0), 0.0);
        assert_near(GameplayTween::Linear.ease(2.0), 1.0);
    }

    #[test]
    fn song_lua_ease_targets_normalize_column_mods() {
        let mut windows = Vec::new();

        assert!(append_song_lua_ease_targets(
            &mut windows,
            1.0,
            2.0,
            4.0,
            "Bumpy4",
            25.0,
            75.0,
            Some("outQuad"),
            Some(0.5),
            Some(1.5),
        ));

        assert_eq!(windows.len(), 1);
        let window = &windows[0];
        assert_eq!(window.target, SongLuaEaseMaskTarget::VisualBumpyColumn(3));
        assert_near(window.start_second, 1.0);
        assert_near(window.end_second, 2.0);
        assert_near(window.sustain_end_second, 4.0);
        assert_near(window.from, 0.25);
        assert_near(window.to, 0.75);
        assert_eq!(window.easing.as_deref(), Some("outQuad"));
        assert_eq!(window.opt1, Some(0.5));
        assert_eq!(window.opt2, Some(1.5));
    }

    #[test]
    fn song_lua_ease_targets_expand_perspective_aliases() {
        let mut windows = Vec::new();

        assert!(append_song_lua_ease_targets(
            &mut windows,
            0.0,
            1.0,
            1.0,
            "incoming",
            20.0,
            60.0,
            None,
            None,
            None,
        ));

        assert_eq!(windows.len(), 2);
        assert_eq!(windows[0].target, SongLuaEaseMaskTarget::PerspectiveTilt);
        assert_near(windows[0].from, -0.2);
        assert_near(windows[0].to, -0.6);
        assert_eq!(windows[1].target, SongLuaEaseMaskTarget::PerspectiveSkew);
        assert_near(windows[1].from, 0.2);
        assert_near(windows[1].to, 0.6);
    }

    #[test]
    fn song_lua_ease_targets_keep_raw_speed_and_mini_values() {
        let mut windows = Vec::new();

        assert!(append_song_lua_ease_targets(
            &mut windows,
            0.0,
            1.0,
            1.0,
            "cmod",
            300.0,
            650.0,
            None,
            None,
            None,
        ));
        assert!(append_song_lua_ease_targets(
            &mut windows,
            0.0,
            1.0,
            1.0,
            "mini",
            25.0,
            50.0,
            None,
            None,
            None,
        ));

        assert_eq!(windows[0].target, SongLuaEaseMaskTarget::ScrollSpeedC);
        assert_near(windows[0].from, 300.0);
        assert_near(windows[0].to, 650.0);
        assert_eq!(windows[1].target, SongLuaEaseMaskTarget::MiniPercent);
        assert_near(windows[1].from, 25.0);
        assert_near(windows[1].to, 50.0);
    }

    #[test]
    fn song_lua_ease_targets_handle_aliases_and_reject_unknown() {
        let mut windows = Vec::new();

        assert!(append_song_lua_ease_targets(
            &mut windows,
            0.0,
            1.0,
            1.0,
            "reverse vanish",
            0.0,
            100.0,
            None,
            None,
            None,
        ));
        assert_eq!(
            windows[0].target,
            SongLuaEaseMaskTarget::AppearanceRandomVanish
        );
        assert_near(windows[0].to, 1.0);

        assert!(!append_song_lua_ease_targets(
            &mut windows,
            0.0,
            1.0,
            1.0,
            "",
            0.0,
            100.0,
            None,
            None,
            None,
        ));
        assert!(!append_song_lua_ease_targets(
            &mut windows,
            0.0,
            1.0,
            1.0,
            "unsupported",
            0.0,
            100.0,
            None,
            None,
            None,
        ));
        assert_eq!(windows.len(), 1);
    }

    #[test]
    fn song_lua_ease_targets_convert_confusion_y_offset() {
        let mut windows = Vec::new();

        assert!(append_song_lua_ease_targets(
            &mut windows,
            0.0,
            1.0,
            1.0,
            "confusionyoffset",
            std::f32::consts::PI * 50.0,
            std::f32::consts::PI * 100.0,
            None,
            None,
            None,
        ));

        assert_eq!(windows[0].target, SongLuaEaseMaskTarget::ConfusionYOffsetY);
        assert_near(windows[0].from, 90.0);
        assert_near(windows[0].to, 180.0);
    }

    #[test]
    fn song_lua_ease_factor_defaults_to_clamped_linear() {
        assert_near(song_lua_ease_factor(None, 0.25, None, None), 0.25);
        assert_near(song_lua_ease_factor(Some("linear"), -1.0, None, None), 0.0);
        assert_near(song_lua_ease_factor(Some("linear"), 2.0, None, None), 1.0);
        assert_near(
            song_lua_ease_factor(Some("unknown"), 0.75, None, None),
            0.75,
        );
    }

    #[test]
    fn song_lua_ease_factor_matches_core_polynomial_curves() {
        assert_near(song_lua_ease_factor(Some("instant"), 0.0, None, None), 1.0);
        assert_near(song_lua_ease_factor(Some("inQuad"), 0.5, None, None), 0.25);
        assert_near(song_lua_ease_factor(Some("outQuad"), 0.5, None, None), 0.75);
        assert_near(
            song_lua_ease_factor(Some("inOutQuad"), 0.25, None, None),
            0.125,
        );
        assert_near(
            song_lua_ease_factor(Some("outInQuad"), 0.25, None, None),
            0.375,
        );
    }

    #[test]
    fn song_lua_ease_factor_handles_bounce_back_and_elastic() {
        assert_near(song_lua_ease_factor(Some("inBounce"), 0.0, None, None), 0.0);
        assert_near(
            song_lua_ease_factor(Some("outBounce"), 1.0, None, None),
            1.0,
        );

        for easing in ["inBack", "outInBack", "inElastic", "outInElastic"] {
            assert!(song_lua_ease_factor(Some(easing), 0.35, Some(1.0), Some(0.2)).is_finite());
        }
    }

    #[test]
    fn song_lua_ease_window_value_interpolates_and_sustains() {
        let window = song_lua_ease_mask_window(
            SongLuaEaseMaskTarget::AppearanceStealth,
            1.0,
            3.0,
            5.0,
            10.0,
            30.0,
        );

        assert!(song_lua_ease_window_value(&window, 0.99).is_none());
        assert_near(song_lua_ease_window_value(&window, 2.0).unwrap(), 20.0);
        assert_near(song_lua_ease_window_value(&window, 4.0).unwrap(), 30.0);
        assert!(song_lua_ease_window_value(&window, 5.0).is_none());
        assert!(song_lua_ease_window_value(&window, f32::NAN).is_none());
    }

    #[test]
    fn song_lua_ease_window_value_snaps_invalid_durations_to_target() {
        let window =
            song_lua_ease_mask_window(SongLuaEaseMaskTarget::MiniPercent, 2.0, 2.0, 4.0, 0.0, 50.0);

        assert_near(song_lua_ease_window_value(&window, 2.5).unwrap(), 50.0);
    }

    #[test]
    fn song_lua_ease_tails_stop_at_next_same_target() {
        let mut windows = [
            song_lua_ease_mask_window(
                SongLuaEaseMaskTarget::AppearanceStealth,
                1.0,
                2.0,
                2.0,
                0.0,
                1.0,
            ),
            song_lua_ease_mask_window(
                SongLuaEaseMaskTarget::AppearanceStealth,
                4.0,
                5.0,
                5.0,
                1.0,
                0.0,
            ),
        ];

        song_lua_extend_ease_tails(&mut windows, &[]);

        assert_near(windows[0].sustain_end_second, 4.0);
        assert_eq!(windows[1].sustain_end_second, f32::MAX);
    }

    #[test]
    fn song_lua_ease_tails_stop_at_constant_masks() {
        let mut windows = [
            song_lua_ease_mask_window(
                SongLuaEaseMaskTarget::AppearanceStealth,
                1.0,
                2.0,
                2.0,
                0.0,
                1.0,
            ),
            song_lua_ease_mask_window(SongLuaEaseMaskTarget::PlayerX, 1.0, 2.0, 2.0, 0.0, 64.0),
        ];
        let mut mods = ParsedAttackMods {
            clear_all: true,
            ..ParsedAttackMods::default()
        };
        mods.appearance.hidden = Some(1.0);
        let constant = attack_mask_window(3.0, 6.0, mods);

        song_lua_extend_ease_tails(&mut windows, &[constant]);

        assert_near(windows[0].sustain_end_second, 3.0);
        assert_eq!(windows[1].sustain_end_second, f32::MAX);
    }

    #[test]
    fn song_lua_ease_tails_match_column_constant_targets() {
        let mut windows = [
            song_lua_ease_mask_window(
                SongLuaEaseMaskTarget::VisualBumpyColumn(2),
                1.0,
                2.0,
                2.0,
                0.0,
                1.0,
            ),
            song_lua_ease_mask_window(
                SongLuaEaseMaskTarget::VisualBumpyColumn(3),
                1.0,
                2.0,
                2.0,
                0.0,
                1.0,
            ),
        ];
        let mut mods = ParsedAttackMods::default();
        mods.visual.bumpy_cols[2] = Some(1.0);
        let constant = attack_mask_window(3.0, 6.0, mods);

        song_lua_extend_ease_tails(&mut windows, &[constant]);

        assert_near(windows[0].sustain_end_second, 3.0);
        assert_eq!(windows[1].sustain_end_second, f32::MAX);
    }

    #[test]
    fn song_lua_column_offset_tails_stop_at_next_same_column() {
        let mut windows = [
            song_lua_column_offset_window(2, 1.0, 2.0, 2.0),
            song_lua_column_offset_window(2, 4.0, 5.0, 5.0),
        ];

        song_lua_extend_column_offset_tails(&mut windows);

        assert_near(windows[0].sustain_end_second, 4.0);
        assert_eq!(windows[1].sustain_end_second, f32::MAX);
    }

    #[test]
    fn song_lua_column_offset_tails_ignore_other_columns_and_same_tick() {
        let mut windows = [
            song_lua_column_offset_window(0, 1.0, 2.0, 2.0),
            song_lua_column_offset_window(1, 3.0, 4.0, 4.0),
            song_lua_column_offset_window(0, 1.0005, 2.0, 2.0),
            song_lua_column_offset_window(0, 5.0, 6.0, 6.0),
        ];

        song_lua_extend_column_offset_tails(&mut windows);

        assert_near(windows[0].sustain_end_second, 5.0);
        assert_eq!(windows[1].sustain_end_second, f32::MAX);
        assert_near(windows[2].sustain_end_second, 5.0);
        assert_eq!(windows[3].sustain_end_second, f32::MAX);
    }

    #[test]
    fn song_lua_column_offset_tails_clamp_explicit_sustain_to_cutoff() {
        let mut windows = [
            song_lua_column_offset_window(0, 1.0, 2.0, 3.0),
            song_lua_column_offset_window(0, 5.0, 6.0, 6.0),
            song_lua_column_offset_window(1, 1.0, 2.0, 8.0),
            song_lua_column_offset_window(1, 5.0, 6.0, 6.0),
        ];

        song_lua_extend_column_offset_tails(&mut windows);

        assert_near(windows[0].sustain_end_second, 3.0);
        assert_near(windows[2].sustain_end_second, 5.0);
    }

    #[test]
    fn song_lua_note_hide_windows_cover_matching_column_bounds() {
        let windows = [
            SongLuaNoteHideWindowRuntime {
                column: 2,
                start_beat: 40.0,
                end_beat: 44.0,
            },
            SongLuaNoteHideWindowRuntime {
                column: 3,
                start_beat: 48.0,
                end_beat: 52.0,
            },
        ];

        assert!(song_lua_note_hidden(&windows, 2, 40.0));
        assert!(song_lua_note_hidden(&windows, 2, 44.0));
        assert!(song_lua_note_hidden(&windows, 2, 39.99995));
        assert!(!song_lua_note_hidden(&windows, 1, 42.0));
        assert!(!song_lua_note_hidden(&windows, 2, 44.01));
    }

    #[test]
    fn song_lua_message_events_offset_event_times_only() {
        let mut events = [
            SongLuaOverlayMessageRuntime {
                event_second: 1.25,
                command_index: 2,
            },
            SongLuaOverlayMessageRuntime {
                event_second: 3.5,
                command_index: 7,
            },
        ];

        offset_song_lua_message_events(&mut events, 4.0);

        assert_near(events[0].event_second, 5.25);
        assert_eq!(events[0].command_index, 2);
        assert_near(events[1].event_second, 7.5);
        assert_eq!(events[1].command_index, 7);
    }

    #[test]
    fn song_lua_message_events_ignore_zero_and_nonfinite_offsets() {
        let original = [
            SongLuaOverlayMessageRuntime {
                event_second: 1.25,
                command_index: 2,
            },
            SongLuaOverlayMessageRuntime {
                event_second: 3.5,
                command_index: 7,
            },
        ];
        let mut events = original;

        offset_song_lua_message_events(&mut events, 0.0);
        assert_eq!(events, original);

        offset_song_lua_message_events(&mut events, f32::NAN);
        assert_eq!(events, original);
    }

    #[test]
    fn song_lua_overlay_eases_group_by_overlay_and_sort_times() {
        let windows = vec![
            song_lua_overlay_ease_window(1, 4.0, 5.0, 5.0, None),
            song_lua_overlay_ease_window(0, 3.0, 4.0, 4.0, None),
            song_lua_overlay_ease_window(1, 1.0, 3.0, 3.0, None),
            song_lua_overlay_ease_window(3, 0.0, 1.0, 1.0, None),
            song_lua_overlay_ease_window(1, 1.0, 2.0, 2.0, None),
        ];

        let (flat, ranges) = group_song_lua_overlay_eases(2, windows);

        assert_eq!(ranges, vec![0..1, 1..4]);
        assert_eq!(flat.len(), 4);
        assert_eq!(flat[0].overlay_index, 0);
        assert_near(flat[1].start_second, 1.0);
        assert_near(flat[1].end_second, 2.0);
        assert_near(flat[2].start_second, 1.0);
        assert_near(flat[2].end_second, 3.0);
        assert_near(flat[3].start_second, 4.0);
    }

    #[test]
    fn song_lua_overlay_eases_offset_window_times_and_cutoffs() {
        let mut windows = [
            song_lua_overlay_ease_window(0, 1.0, 2.0, 4.0, Some(3.0)),
            song_lua_overlay_ease_window(1, 5.0, 6.0, 6.0, None),
        ];

        offset_song_lua_overlay_eases(&mut windows, 7.0);

        assert_near(windows[0].start_second, 8.0);
        assert_near(windows[0].end_second, 9.0);
        assert_near(windows[0].sustain_end_second, 11.0);
        assert_near(windows[0].cutoff_second.unwrap(), 10.0);
        assert_near(windows[1].start_second, 12.0);
        assert_eq!(windows[1].cutoff_second, None);
    }

    #[test]
    fn song_lua_overlay_eases_ignore_zero_and_nonfinite_offsets() {
        let original = [
            song_lua_overlay_ease_window(0, 1.0, 2.0, 4.0, Some(3.0)),
            song_lua_overlay_ease_window(1, 5.0, 6.0, 6.0, None),
        ];
        let mut windows = original.clone();

        offset_song_lua_overlay_eases(&mut windows, 0.0);
        assert_eq!(windows, original);

        offset_song_lua_overlay_eases(&mut windows, f32::INFINITY);
        assert_eq!(windows, original);
    }

    #[test]
    fn song_lua_player_transform_target_updates_player_values() {
        let mut player = SongLuaPlayerTransformValues::default();

        song_lua_apply_player_transform_target(
            SongLuaEaseMaskTarget::PlayerX,
            f32::NAN,
            &mut player,
        );
        song_lua_apply_player_transform_target(
            SongLuaEaseMaskTarget::VisualDrunk,
            1.0,
            &mut player,
        );
        assert_eq!(player, SongLuaPlayerTransformValues::default());

        song_lua_apply_player_transform_target(
            SongLuaEaseMaskTarget::PlayerZoom,
            1.25,
            &mut player,
        );
        song_lua_apply_player_transform_target(
            SongLuaEaseMaskTarget::PlayerSkewY,
            -0.5,
            &mut player,
        );

        assert_near(player.zoom_x.unwrap(), 1.25);
        assert_near(player.zoom_y.unwrap(), 1.25);
        assert_near(player.zoom_z.unwrap(), 1.25);
        assert_near(player.skew_y.unwrap(), -0.5);
    }

    #[test]
    fn song_lua_player_transform_resolve_filters_and_defaults_values() {
        let resolved = SongLuaPlayerTransformValues {
            x: Some(f32::NAN),
            y: Some(32.0),
            z: Some(f32::INFINITY),
            rotation_x: Some(12.0),
            rotation_z: None,
            rotation_y: Some(f32::NEG_INFINITY),
            skew_x: Some(-0.25),
            skew_y: Some(f32::NAN),
            zoom_x: None,
            zoom_y: Some(1.5),
            zoom_z: Some(f32::NAN),
            confusion_y_offset: Some(9.0),
        }
        .resolve();

        assert_eq!(resolved.x, None);
        assert_eq!(resolved.y, Some(32.0));
        assert_near(resolved.z, 0.0);
        assert_near(resolved.rotation_x, 12.0);
        assert_near(resolved.rotation_z, 0.0);
        assert_near(resolved.rotation_y, 0.0);
        assert_near(resolved.skew_x, -0.25);
        assert_near(resolved.skew_y, 0.0);
        assert_near(resolved.zoom_x, 1.0);
        assert_near(resolved.zoom_y, 1.5);
        assert_near(resolved.zoom_z, 1.0);
        assert_near(resolved.confusion_y_offset, 9.0);
    }

    #[test]
    fn song_lua_eased_target_updates_effect_outputs() {
        let mut accel = AccelOverrides::default();
        let mut visual = VisualOverrides::default();
        let mut appearance = AppearanceEffects::default();
        let mut visibility = VisibilityOverrides::default();
        let mut scroll = ScrollOverrides::default();
        let mut perspective = PerspectiveOverrides::default();
        let mut scroll_speed = None;
        let mut mini_percent = None;
        let mut player = SongLuaPlayerTransformValues::default();

        song_lua_apply_eased_target(
            SongLuaEaseMaskTarget::AccelBoost,
            0.75,
            &mut accel,
            &mut visual,
            &mut appearance,
            &mut visibility,
            &mut scroll,
            &mut perspective,
            &mut scroll_speed,
            &mut mini_percent,
            &mut player,
        );
        song_lua_apply_eased_target(
            SongLuaEaseMaskTarget::VisualBumpyColumn(2),
            1.5,
            &mut accel,
            &mut visual,
            &mut appearance,
            &mut visibility,
            &mut scroll,
            &mut perspective,
            &mut scroll_speed,
            &mut mini_percent,
            &mut player,
        );
        song_lua_apply_eased_target(
            SongLuaEaseMaskTarget::AppearanceStealth,
            0.25,
            &mut accel,
            &mut visual,
            &mut appearance,
            &mut visibility,
            &mut scroll,
            &mut perspective,
            &mut scroll_speed,
            &mut mini_percent,
            &mut player,
        );
        song_lua_apply_eased_target(
            SongLuaEaseMaskTarget::VisibilityDark,
            1.0,
            &mut accel,
            &mut visual,
            &mut appearance,
            &mut visibility,
            &mut scroll,
            &mut perspective,
            &mut scroll_speed,
            &mut mini_percent,
            &mut player,
        );
        song_lua_apply_eased_target(
            SongLuaEaseMaskTarget::ScrollReverse,
            0.5,
            &mut accel,
            &mut visual,
            &mut appearance,
            &mut visibility,
            &mut scroll,
            &mut perspective,
            &mut scroll_speed,
            &mut mini_percent,
            &mut player,
        );
        song_lua_apply_eased_target(
            SongLuaEaseMaskTarget::PerspectiveTilt,
            -1.0,
            &mut accel,
            &mut visual,
            &mut appearance,
            &mut visibility,
            &mut scroll,
            &mut perspective,
            &mut scroll_speed,
            &mut mini_percent,
            &mut player,
        );
        song_lua_apply_eased_target(
            SongLuaEaseMaskTarget::MiniPercent,
            30.0,
            &mut accel,
            &mut visual,
            &mut appearance,
            &mut visibility,
            &mut scroll,
            &mut perspective,
            &mut scroll_speed,
            &mut mini_percent,
            &mut player,
        );

        assert_near(accel.boost.unwrap(), 0.75);
        assert_near(visual.bumpy_cols[2].unwrap(), 1.5);
        assert_near(appearance.stealth, 0.25);
        assert_near(visibility.dark.unwrap(), 1.0);
        assert_near(scroll.reverse.unwrap(), 0.5);
        assert_near(perspective.tilt.unwrap(), -1.0);
        assert_near(mini_percent.unwrap(), 30.0);
    }

    #[test]
    fn song_lua_eased_target_handles_scroll_speed_and_player_targets() {
        let mut accel = AccelOverrides::default();
        let mut visual = VisualOverrides::default();
        let mut appearance = AppearanceEffects::default();
        let mut visibility = VisibilityOverrides::default();
        let mut scroll = ScrollOverrides::default();
        let mut perspective = PerspectiveOverrides::default();
        let mut scroll_speed = None;
        let mut mini_percent = None;
        let mut player = SongLuaPlayerTransformValues::default();

        song_lua_apply_eased_target(
            SongLuaEaseMaskTarget::ScrollSpeedC,
            -100.0,
            &mut accel,
            &mut visual,
            &mut appearance,
            &mut visibility,
            &mut scroll,
            &mut perspective,
            &mut scroll_speed,
            &mut mini_percent,
            &mut player,
        );
        assert!(scroll_speed.is_none());

        song_lua_apply_eased_target(
            SongLuaEaseMaskTarget::ScrollSpeedC,
            650.0,
            &mut accel,
            &mut visual,
            &mut appearance,
            &mut visibility,
            &mut scroll,
            &mut perspective,
            &mut scroll_speed,
            &mut mini_percent,
            &mut player,
        );
        assert!(matches!(
            scroll_speed,
            Some(ScrollSpeedSetting::CMod(v)) if (v - 650.0).abs() <= 0.000_001
        ));

        song_lua_apply_eased_target(
            SongLuaEaseMaskTarget::PlayerRotationZ,
            45.0,
            &mut accel,
            &mut visual,
            &mut appearance,
            &mut visibility,
            &mut scroll,
            &mut perspective,
            &mut scroll_speed,
            &mut mini_percent,
            &mut player,
        );
        assert_near(player.rotation_z.unwrap(), 45.0);
    }

    #[test]
    fn song_lua_player_eases_apply_only_player_targets() {
        let windows = [
            song_lua_ease_mask_window(SongLuaEaseMaskTarget::PlayerZoom, 1.0, 3.0, 3.0, 0.0, 2.0),
            song_lua_ease_mask_window(SongLuaEaseMaskTarget::VisualDrunk, 1.0, 3.0, 3.0, 0.0, 1.0),
            song_lua_ease_mask_window(SongLuaEaseMaskTarget::PlayerX, 3.0, 4.0, 4.0, 0.0, 100.0),
        ];
        let mut player = SongLuaPlayerTransformValues::default();

        apply_song_lua_player_eases(&mut player, &windows, 2.0);

        assert_near(player.zoom_x.unwrap(), 1.0);
        assert_near(player.zoom_y.unwrap(), 1.0);
        assert_near(player.zoom_z.unwrap(), 1.0);
        assert_eq!(player.x, None);
    }

    #[test]
    fn song_lua_attack_eases_apply_active_windows_and_mini_delta() {
        let windows = [
            song_lua_ease_mask_window(SongLuaEaseMaskTarget::AccelBoost, 1.0, 3.0, 3.0, 0.0, 1.0),
            song_lua_ease_mask_window(
                SongLuaEaseMaskTarget::AppearanceStealth,
                1.0,
                3.0,
                3.0,
                0.0,
                1.0,
            ),
            song_lua_ease_mask_window(
                SongLuaEaseMaskTarget::ScrollSpeedC,
                1.0,
                3.0,
                3.0,
                300.0,
                600.0,
            ),
            song_lua_ease_mask_window(
                SongLuaEaseMaskTarget::MiniPercent,
                1.0,
                3.0,
                3.0,
                10.0,
                20.0,
            ),
            song_lua_ease_mask_window(
                SongLuaEaseMaskTarget::PlayerRotationZ,
                1.0,
                3.0,
                3.0,
                0.0,
                90.0,
            ),
        ];
        let mut attack = ActiveAttackMaskValues::new(AppearanceEffects::default());
        let mut appearance = AppearanceEffects::default();
        let mut player = SongLuaPlayerTransformValues::default();

        apply_song_lua_attack_eases(
            &mut attack,
            &mut appearance,
            &mut player,
            &windows,
            2.0,
            30.0,
        );

        assert_near(attack.accel.boost.unwrap(), 0.5);
        assert_near(appearance.stealth, 0.5);
        assert!(matches!(
            attack.scroll_speed,
            Some(ScrollSpeedSetting::CMod(v)) if (v - 450.0).abs() <= 0.000_001
        ));
        assert_near(attack.mini_percent.unwrap(), 45.0);
        assert_near(player.rotation_z.unwrap(), 45.0);
    }

    #[test]
    fn noteskin_effect_defaults_match_runtime_fallbacks() {
        let effects = GameplayNoteskinEffects::default();

        let glow = effects.receptor_glow_behavior_for_player(0);
        assert_near(glow.duration, 0.2);
        assert!(glow.blend_add);

        let default_step = effects.receptor_step_behavior_for_col(0, 0, None);
        assert_near(default_step.duration, 0.11);
        assert!(default_step.interrupts);

        let scored_step = effects.receptor_step_behavior_for_col(0, 0, Some("W1"));
        assert_near(scored_step.duration, 0.0);
        assert!(!scored_step.interrupts);

        assert_eq!(effects.tap_explosion_duration(0, 0, "W1", false), None);
        assert_near(effects.mine_explosion_duration(0), MINE_EXPLOSION_DURATION);
    }

    #[test]
    fn judge_grades_map_to_noteskin_windows() {
        assert_eq!(grade_to_window(JudgeGrade::Fantastic), Some("W1"));
        assert_eq!(grade_to_window(JudgeGrade::Excellent), Some("W2"));
        assert_eq!(grade_to_window(JudgeGrade::Great), Some("W3"));
        assert_eq!(grade_to_window(JudgeGrade::Decent), Some("W4"));
        assert_eq!(grade_to_window(JudgeGrade::WayOff), Some("W5"));
        assert_eq!(grade_to_window(JudgeGrade::Miss), Some("Miss"));
    }

    #[test]
    fn tap_explosion_options_map_judgment_windows() {
        let options = TapExplosionOptions {
            fantastic: true,
            excellent: true,
            great: true,
            decent: true,
            way_off: true,
            miss: true,
            held: true,
            holding: true,
        };

        for window in ["W0", "W1", "W2", "W3", "W4", "W5", "Miss", "Held"] {
            assert!(tap_explosion_enabled_for_options(options, window));
        }
        assert!(!tap_explosion_enabled_for_options(options, "Holding"));
    }

    #[test]
    fn tap_explosion_options_gate_disabled_windows() {
        let options = TapExplosionOptions {
            miss: true,
            held: true,
            ..TapExplosionOptions::default()
        };

        assert!(tap_explosion_enabled_for_options(options, "Miss"));
        assert!(tap_explosion_enabled_for_options(options, "Held"));
        assert!(!tap_explosion_enabled_for_options(options, "W0"));
        assert!(!tap_explosion_enabled_for_options(options, "W1"));
        assert!(!tap_explosion_enabled_for_options(options, "W3"));
    }

    #[test]
    fn hold_explosion_options_use_holding_bit_only() {
        assert!(hold_explosion_enabled_for_options(TapExplosionOptions {
            holding: true,
            ..TapExplosionOptions::default()
        }));
        assert!(!hold_explosion_enabled_for_options(TapExplosionOptions {
            held: true,
            ..TapExplosionOptions::default()
        }));
    }

    #[test]
    fn fantastic_window_options_select_play_and_blue_windows() {
        let base = FantasticWindowOptions {
            base_fa_plus_s: 0.015,
            custom_fantastic_window_s: None,
            fa_plus_10ms_blue_window: false,
        };
        assert_near(fantastic_window_seconds(base), 0.015);
        assert_near(blue_fantastic_window_ms(base), 15.0);

        let ten_ms = FantasticWindowOptions {
            fa_plus_10ms_blue_window: true,
            ..base
        };
        assert_near(fantastic_window_seconds(ten_ms), 0.015);
        assert_near(blue_fantastic_window_ms(ten_ms), 10.0);

        let custom = FantasticWindowOptions {
            custom_fantastic_window_s: Some(0.012),
            ..ten_ms
        };
        assert_near(fantastic_window_seconds(custom), 0.012);
        assert_near(blue_fantastic_window_ms(custom), 12.0);
    }

    #[test]
    fn player_judgment_timing_applies_custom_fantastic_and_rate() {
        let timing = build_player_judgment_timing_for_options(
            TimingProfile::default_itg_with_fa_plus(),
            FantasticWindowOptions {
                base_fa_plus_s: 0.015,
                custom_fantastic_window_s: Some(0.012),
                fa_plus_10ms_blue_window: false,
            },
            [false; 5],
            1.5,
        );

        let fa_plus_ns = timing
            .profile_music_ns
            .fa_plus_window_ns
            .expect("fa+ window");
        assert!((fa_plus_ns - song_time_ns_from_seconds(0.018)).abs() <= 1);
        assert_eq!(
            timing.largest_tap_window_music_ns,
            timing.profile_music_ns.windows_ns[4]
        );
    }

    #[test]
    fn player_judgment_timing_respects_disabled_windows() {
        let mut disabled = [false; 5];
        disabled[3] = true;
        disabled[4] = true;
        let timing = build_player_judgment_timing_for_options(
            TimingProfile::default_itg_with_fa_plus(),
            FantasticWindowOptions {
                base_fa_plus_s: 0.015,
                custom_fantastic_window_s: None,
                fa_plus_10ms_blue_window: false,
            },
            disabled,
            1.0,
        );

        assert_eq!(timing.disabled_windows, disabled);
        assert_eq!(
            timing.largest_tap_window_music_ns,
            timing.profile_music_ns.windows_ns[2]
        );
    }

    #[test]
    fn note_hit_eval_for_timing_classifies_offsets() {
        let timing = build_player_judgment_timing_for_options(
            TimingProfile::default_itg_with_fa_plus(),
            FantasticWindowOptions {
                base_fa_plus_s: 0.015,
                custom_fantastic_window_s: None,
                fa_plus_10ms_blue_window: false,
            },
            [false; 5],
            1.0,
        );
        let note_time_ns = song_time_ns_from_seconds(10.0);
        let event_time_ns = note_time_ns + timing.profile_music_ns.windows_ns[2];

        let hit = note_hit_eval_for_timing(timing, note_time_ns, event_time_ns).expect("valid hit");

        assert_eq!(hit.note_time_ns, note_time_ns);
        assert_eq!(
            hit.measured_offset_music_ns,
            timing.profile_music_ns.windows_ns[2]
        );
        assert_eq!(hit.grade, JudgeGrade::Great);
        assert_eq!(hit.window, TimingWindow::W3);
    }

    #[test]
    fn note_hit_eval_for_timing_rejects_beyond_largest_window() {
        let timing = build_player_judgment_timing_for_options(
            TimingProfile::default_itg_with_fa_plus(),
            FantasticWindowOptions {
                base_fa_plus_s: 0.015,
                custom_fantastic_window_s: None,
                fa_plus_10ms_blue_window: false,
            },
            [false; 5],
            1.0,
        );
        let note_time_ns = song_time_ns_from_seconds(10.0);
        let event_time_ns = note_time_ns + timing.largest_tap_window_music_ns + 1;

        assert!(note_hit_eval_for_timing(timing, note_time_ns, event_time_ns).is_none());
    }

    #[test]
    fn fantastic_feedback_requires_fa_plus_and_fantastic_grade() {
        let fantastic = test_judgment(JudgeGrade::Fantastic);
        let excellent = test_judgment(JudgeGrade::Excellent);

        assert!(!tap_judgment_uses_bright_explosion_for_options(
            FantasticFeedbackOptions::default(),
            &fantastic,
        ));
        assert!(!tap_judgment_uses_bright_explosion_for_options(
            FantasticFeedbackOptions {
                show_fa_plus_window: true,
                ..FantasticFeedbackOptions::default()
            },
            &excellent,
        ));
    }

    #[test]
    fn fantastic_feedback_uses_w1_for_bright_tap_explosion() {
        let mut white = test_judgment(JudgeGrade::Fantastic);
        white.window = Some(TimingWindow::W1);
        let mut blue = white.clone();
        blue.window = Some(TimingWindow::W0);
        let options = FantasticFeedbackOptions {
            show_fa_plus_window: true,
            ..FantasticFeedbackOptions::default()
        };

        assert!(tap_judgment_uses_bright_explosion_for_options(
            options, &white
        ));
        assert!(!tap_judgment_uses_bright_explosion_for_options(
            options, &blue
        ));
    }

    #[test]
    fn fantastic_feedback_uses_10ms_blue_window_when_enabled() {
        let mut blue = test_judgment(JudgeGrade::Fantastic);
        blue.window = Some(TimingWindow::W0);
        blue.time_error_ms = FA_PLUS_W010_MS;
        let mut white = blue.clone();
        white.time_error_ms = FA_PLUS_W010_MS + 0.001;
        let options = FantasticFeedbackOptions {
            show_fa_plus_window: true,
            fa_plus_10ms_blue_window: true,
            ..FantasticFeedbackOptions::default()
        };

        assert!(!tap_judgment_uses_bright_explosion_for_options(
            options, &blue
        ));
        assert!(tap_judgment_uses_bright_explosion_for_options(
            options, &white
        ));

        let split_options = FantasticFeedbackOptions {
            split_15_10ms: true,
            ..options
        };
        assert!(!tap_judgment_uses_bright_explosion_for_options(
            split_options,
            &white,
        ));

        let custom_options = FantasticFeedbackOptions {
            custom_fantastic_window: true,
            ..options
        };
        assert!(!tap_judgment_uses_bright_explosion_for_options(
            custom_options,
            &white,
        ));
    }

    #[test]
    fn noteskin_effect_setters_clamp_player_and_column_reads() {
        let mut effects = GameplayNoteskinEffects::default();
        let last_player = MAX_PLAYERS - 1;
        let last_col = MAX_COLS - 1;
        effects.set_receptor_step_behavior(
            0,
            0,
            Some("W3"),
            GameplayReceptorStepBehavior {
                duration: 0.4,
                zoom_start: 0.5,
                zoom_end: 1.5,
                tween: GameplayTween::Accelerate,
                interrupts: false,
            },
        );
        effects.set_tap_explosion_duration(0, 0, "Held", true, Some(0.7));
        effects.set_mine_explosion_duration(0, 0.9);
        effects.set_tap_explosion_duration(last_player, last_col, "Held", true, Some(0.8));
        effects.set_mine_explosion_duration(last_player, 1.1);

        assert_near(
            effects
                .receptor_step_behavior_for_col(0, 0, Some("W3"))
                .duration,
            0.4,
        );
        assert_eq!(
            effects.tap_explosion_duration(0, 0, "Held", true),
            Some(0.7)
        );
        assert_near(effects.mine_explosion_duration(MAX_PLAYERS), 1.1);
        assert_eq!(
            effects.tap_explosion_duration(MAX_PLAYERS, MAX_COLS, "Held", true),
            Some(0.8)
        );
    }

    #[test]
    fn receptor_behaviors_sample_zoom_and_glow() {
        let step = GameplayReceptorStepBehavior {
            duration: 1.0,
            zoom_start: 0.5,
            zoom_end: 1.5,
            tween: GameplayTween::Linear,
            interrupts: true,
        };
        assert_near(step.sample_zoom(0.5), 1.0);

        let glow = GameplayReceptorGlowBehavior {
            press_duration: 1.0,
            press_alpha_start: 0.0,
            press_alpha_end: 1.0,
            press_zoom_start: 1.0,
            press_zoom_end: 2.0,
            press_tween: GameplayTween::Linear,
            duration: 1.0,
            alpha_start: 1.0,
            alpha_end: 0.0,
            zoom_start: 2.0,
            zoom_end: 1.0,
            tween: GameplayTween::Linear,
            blend_add: true,
        };
        let (press_alpha, press_zoom) = glow.sample_press(0.5);
        assert_near(press_alpha, 0.5);
        assert_near(press_zoom, 1.5);
        let (lift_alpha, lift_zoom) = glow.sample_lift(0.5, 1.0, 2.0);
        assert_near(lift_alpha, 0.5);
        assert_near(lift_zoom, 1.5);
    }

    #[test]
    fn receptor_glow_visual_selects_press_hold_lift_or_idle() {
        let behavior = GameplayReceptorGlowBehavior {
            press_duration: 1.0,
            press_alpha_start: 0.0,
            press_alpha_end: 1.0,
            press_zoom_start: 1.0,
            press_zoom_end: 2.0,
            press_tween: GameplayTween::Linear,
            duration: 1.0,
            alpha_start: 1.0,
            alpha_end: 0.0,
            zoom_start: 2.0,
            zoom_end: 1.0,
            tween: GameplayTween::Linear,
            blend_add: true,
        };

        let press = receptor_glow_visual(
            behavior,
            GameplayReceptorGlowState {
                press_timer: 0.5,
                lane_pressed: true,
                ..GameplayReceptorGlowState::default()
            },
        )
        .expect("active press tween should render");
        assert_near(press.0, 0.5);
        assert_near(press.1, 1.5);

        let held = receptor_glow_visual(
            behavior,
            GameplayReceptorGlowState {
                lane_pressed: true,
                ..GameplayReceptorGlowState::default()
            },
        )
        .expect("held lane should render press end state");
        assert_near(held.0, 1.0);
        assert_near(held.1, 2.0);

        let lift = receptor_glow_visual(
            behavior,
            GameplayReceptorGlowState {
                lift_timer: 0.5,
                lift_start_alpha: 1.0,
                lift_start_zoom: 2.0,
                ..GameplayReceptorGlowState::default()
            },
        )
        .expect("active lift tween should render");
        assert_near(lift.0, 0.5);
        assert_near(lift.1, 1.5);

        assert!(receptor_glow_visual(behavior, GameplayReceptorGlowState::default()).is_none());
    }

    #[test]
    fn receptor_glow_duration_and_lift_start_use_runtime_fallbacks() {
        let mut behavior = GameplayReceptorGlowBehavior {
            press_duration: 1.0,
            press_alpha_start: 0.0,
            press_alpha_end: 1.0,
            press_zoom_start: 1.0,
            press_zoom_end: 2.0,
            press_tween: GameplayTween::Linear,
            duration: 0.0,
            alpha_start: 1.0,
            alpha_end: 0.0,
            zoom_start: 2.0,
            zoom_end: 1.0,
            tween: GameplayTween::Linear,
            blend_add: true,
        };

        assert_near(receptor_glow_duration(behavior), RECEPTOR_GLOW_DURATION);
        let (alpha, zoom) = receptor_glow_lift_start(behavior, 0.5);
        assert_near(alpha, 0.5);
        assert_near(zoom, 1.5);

        behavior.press_duration = 0.0;
        let (alpha, zoom) = receptor_glow_lift_start(behavior, 0.5);
        assert_near(alpha, 1.0);
        assert_near(zoom, 2.0);
    }

    #[test]
    fn receptor_glow_timer_entries_match_press_pulse_and_release_policy() {
        let mut behavior = GameplayReceptorGlowBehavior {
            press_duration: 1.0,
            press_alpha_start: 0.25,
            press_alpha_end: 0.75,
            press_zoom_start: 1.25,
            press_zoom_end: 1.75,
            press_tween: GameplayTween::Linear,
            duration: 0.5,
            alpha_start: 1.0,
            alpha_end: 0.0,
            zoom_start: 2.0,
            zoom_end: 1.0,
            tween: GameplayTween::Linear,
            blend_add: true,
        };

        let press = receptor_glow_press_timers(behavior);
        assert_near(press.press_timer, 1.0);
        assert_near(press.lift_timer, 0.0);
        assert_near(press.lift_start_alpha, 0.75);
        assert_near(press.lift_start_zoom, 1.75);

        let pulse = receptor_glow_pulse_timers(behavior);
        assert_near(pulse.press_timer, 0.0);
        assert_near(pulse.lift_timer, 0.5);
        assert_near(pulse.lift_start_alpha, 0.25);
        assert_near(pulse.lift_start_zoom, 1.25);

        behavior.duration = 0.0;
        let pulse = receptor_glow_pulse_timers(behavior);
        assert_near(pulse.lift_timer, RECEPTOR_GLOW_DURATION);

        let release = receptor_glow_release_timers(behavior, 0.5);
        assert_near(release.press_timer, 0.0);
        assert_near(release.lift_timer, RECEPTOR_GLOW_DURATION);
        assert_near(release.lift_start_alpha, 0.5);
        assert_near(release.lift_start_zoom, 1.5);
    }

    #[test]
    fn receptor_glow_timer_tick_handles_pressed_release_and_lift_decay() {
        let behavior = GameplayReceptorGlowBehavior {
            press_duration: 1.0,
            press_alpha_start: 0.0,
            press_alpha_end: 1.0,
            press_zoom_start: 1.0,
            press_zoom_end: 2.0,
            press_tween: GameplayTween::Linear,
            duration: 0.4,
            alpha_start: 1.0,
            alpha_end: 0.0,
            zoom_start: 2.0,
            zoom_end: 1.0,
            tween: GameplayTween::Linear,
            blend_add: true,
        };

        let pressed = tick_receptor_glow_timers(
            behavior,
            GameplayReceptorGlowTimers {
                press_timer: 0.7,
                lift_timer: 0.25,
                lift_start_alpha: 0.4,
                lift_start_zoom: 1.4,
            },
            true,
            0.2,
        );
        assert_near(pressed.press_timer, 0.5);
        assert_near(pressed.lift_timer, 0.0);
        assert_near(pressed.lift_start_alpha, 0.4);
        assert_near(pressed.lift_start_zoom, 1.4);

        let release = tick_receptor_glow_timers(
            behavior,
            GameplayReceptorGlowTimers {
                press_timer: 0.3,
                lift_timer: 0.0,
                lift_start_alpha: 0.0,
                lift_start_zoom: 1.0,
            },
            false,
            0.3,
        );
        assert_near(release.press_timer, 0.0);
        assert_near(release.lift_timer, 0.4);
        assert_near(release.lift_start_alpha, 0.7);
        assert_near(release.lift_start_zoom, 1.7);

        let lift = tick_receptor_glow_timers(
            behavior,
            GameplayReceptorGlowTimers {
                press_timer: 0.0,
                lift_timer: 0.25,
                lift_start_alpha: 0.7,
                lift_start_zoom: 1.7,
            },
            false,
            0.1,
        );
        assert_near(lift.lift_timer, 0.15);
        assert_near(lift.lift_start_alpha, 0.7);
        assert_near(lift.lift_start_zoom, 1.7);
    }

    #[test]
    fn autoplay_random_offset_w1_uses_full_window_without_fa_plus() {
        let mut rng = TurnRng::new(1);
        let mut profile = TimingProfile::default_itg_with_fa_plus();
        profile.fa_plus_window_s = None;
        let profile_ns = TimingProfileNs::from_profile_scaled(&profile, 1.0);
        let outer = profile_ns.windows_ns[0];
        for _ in 0..32 {
            let offset =
                autoplay_random_offset_music_ns_for_window(&mut rng, profile_ns, TimingWindow::W1);
            assert!(offset.abs() <= outer);
        }
    }

    #[test]
    fn autoplay_random_offset_w1_excludes_w0_band_when_enabled() {
        let mut rng = TurnRng::new(2);
        let profile = TimingProfile::default_itg_with_fa_plus();
        let profile_ns = TimingProfileNs::from_profile_scaled(&profile, 1.0);
        let inner = profile_ns
            .fa_plus_window_ns
            .expect("default profile has W0");
        let outer = profile_ns.windows_ns[0];
        for _ in 0..32 {
            let offset =
                autoplay_random_offset_music_ns_for_window(&mut rng, profile_ns, TimingWindow::W1);
            assert!(offset.abs() >= inner);
            assert!(offset.abs() <= outer);
        }
    }

    #[test]
    fn autoplay_judgment_offset_preserves_measured_input_when_not_live() {
        let mut rng = TurnRng::new(3);
        let profile_ns =
            TimingProfileNs::from_profile_scaled(&TimingProfile::default_itg_with_fa_plus(), 1.0);

        assert_eq!(
            autoplay_judgment_offset_music_ns(false, &mut rng, profile_ns, TimingWindow::W1, 1234),
            1234
        );
    }

    #[test]
    fn autoplay_judgment_offset_randomizes_live_autoplay_window() {
        let mut rng = TurnRng::new(4);
        let profile = TimingProfile::default_itg_with_fa_plus();
        let profile_ns = TimingProfileNs::from_profile_scaled(&profile, 1.0);
        let inner = profile_ns
            .fa_plus_window_ns
            .expect("default profile has W0");
        let outer = profile_ns.windows_ns[0];

        let offset = autoplay_judgment_offset_music_ns(
            true,
            &mut rng,
            profile_ns,
            TimingWindow::W1,
            outer.saturating_mul(10),
        );

        assert!(offset.abs() >= inner);
        assert!(offset.abs() <= outer);
    }

    #[test]
    fn live_autoplay_and_scoring_block_flags_exclude_replay_mode() {
        assert!(live_autoplay_enabled_from_flags(true, false));
        assert!(!live_autoplay_enabled_from_flags(true, true));
        assert!(!live_autoplay_enabled_from_flags(false, false));
        assert!(autoplay_blocks_scoring_from_flags(true, false));
        assert!(!autoplay_blocks_scoring_from_flags(true, true));
        assert!(!autoplay_blocks_scoring_from_flags(false, false));
    }

    #[test]
    fn autoplay_enable_cursor_clamps_to_player_note_range() {
        assert_eq!(autoplay_cursor_for_enable(4, (10, 20)), 10);
        assert_eq!(autoplay_cursor_for_enable(14, (10, 20)), 14);
        assert_eq!(autoplay_cursor_for_enable(30, (10, 20)), 20);
        assert_eq!(autoplay_cursor_for_enable(30, (20, 10)), 20);
    }

    fn active_hold_for_autoplay(end_time_ns: SongTimeNs) -> ActiveHold {
        ActiveHold {
            note_index: 7,
            start_time_ns: 1_000,
            end_time_ns,
            note_type: NoteType::Hold,
            let_go: false,
            is_pressed: true,
            life: 1.0,
            last_update_time_ns: 1_000,
        }
    }

    #[test]
    fn autoplay_due_active_hold_waits_until_cutoff() {
        let active = active_hold_for_autoplay(2_000);

        assert_eq!(autoplay_due_active_hold_resolution(&active, 1_999), None);
    }

    #[test]
    fn autoplay_due_active_hold_succeeds_when_engaged() {
        let active = active_hold_for_autoplay(2_000);

        assert_eq!(
            autoplay_due_active_hold_resolution(&active, 2_000),
            Some(ActiveHoldResolution::Success { note_index: 7 })
        );
    }

    #[test]
    fn autoplay_due_active_hold_lets_go_when_marked_or_depleted() {
        let mut let_go = active_hold_for_autoplay(2_000);
        let_go.let_go = true;
        let mut depleted = active_hold_for_autoplay(3_000);
        depleted.life = 0.0;

        assert_eq!(
            autoplay_due_active_hold_resolution(&let_go, 2_500),
            Some(ActiveHoldResolution::LetGo {
                note_index: 7,
                time_ns: 2_000,
            })
        );
        assert_eq!(
            autoplay_due_active_hold_resolution(&depleted, 3_500),
            Some(ActiveHoldResolution::LetGo {
                note_index: 7,
                time_ns: 3_000,
            })
        );
    }

    #[test]
    fn lane_edges_classify_press_and_release() {
        assert!(lane_press_started(true, false, true));
        assert!(!lane_press_started(true, true, true));
        assert!(lane_release_finished(false, true, false));
        assert!(!lane_release_finished(false, true, true));

        assert!(lane_edge_judges_tap(true, false));
        assert!(!lane_edge_judges_tap(true, true));
        assert!(lane_edge_judges_lift(false, true));
        assert!(!lane_edge_judges_lift(false, false));
    }

    #[test]
    fn autoplay_keeps_active_holds_pressed() {
        assert!(active_hold_counts_as_pressed(true, false));
        assert!(active_hold_counts_as_pressed(true, true));
        assert!(active_hold_counts_as_pressed(false, true));
        assert!(!active_hold_counts_as_pressed(false, false));
    }

    fn test_note(note_type: NoteType, hold: Option<HoldData>, is_fake: bool) -> Note {
        test_note_at(note_type, hold, is_fake, 0, 0.0)
    }

    fn test_note_at(
        note_type: NoteType,
        hold: Option<HoldData>,
        is_fake: bool,
        row_index: usize,
        beat: f32,
    ) -> Note {
        Note {
            beat,
            quantization_idx: 0,
            column: 0,
            note_type,
            row_index,
            result: None,
            early_result: None,
            hold,
            mine_result: None,
            is_fake,
            can_be_judged: true,
        }
    }

    fn test_hold() -> HoldData {
        HoldData {
            end_row_index: 48,
            end_beat: 1.0,
            result: None,
            life: 1.0,
            let_go_started_at: None,
            let_go_starting_life: 1.0,
            last_held_row_index: 0,
            last_held_beat: 0.0,
        }
    }

    fn test_judgment(grade: JudgeGrade) -> Judgment {
        Judgment {
            time_error_ms: 0.0,
            time_error_music_ns: 0,
            grade,
            window: match grade {
                JudgeGrade::Fantastic => Some(TimingWindow::W1),
                JudgeGrade::Excellent => Some(TimingWindow::W2),
                JudgeGrade::Great => Some(TimingWindow::W3),
                JudgeGrade::Decent => Some(TimingWindow::W4),
                JudgeGrade::WayOff => Some(TimingWindow::W5),
                JudgeGrade::Miss => None,
            },
            miss_because_held: false,
        }
    }

    fn test_chart(stats: ArrowStats, mines_nonfake: u32, has_chart_attacks: bool) -> ChartData {
        ChartData {
            chart_type: String::new(),
            difficulty: String::new(),
            description: String::new(),
            chart_name: String::new(),
            meter: 0,
            step_artist: String::new(),
            music_path: None,
            short_hash: String::new(),
            stats,
            tech_counts: TechCounts::default(),
            mines_nonfake,
            stamina_counts: StaminaCounts::default(),
            total_streams: 0,
            matrix_rating: 0.0,
            max_nps: 0.0,
            sn_detailed_breakdown: String::new(),
            sn_partial_breakdown: String::new(),
            sn_simple_breakdown: String::new(),
            detailed_breakdown: String::new(),
            partial_breakdown: String::new(),
            simple_breakdown: String::new(),
            total_measures: 0,
            measure_nps_vec: Vec::new(),
            measure_seconds_vec: Vec::new(),
            first_second: 0.0,
            has_note_data: true,
            has_chart_attacks,
            possible_grade_points: 0,
            holds_total: 0,
            rolls_total: 0,
            mines_total: 0,
            display_bpm: None,
            min_bpm: 0.0,
            max_bpm: 0.0,
        }
    }

    fn test_active_hold(
        note_type: NoteType,
        is_pressed: bool,
        let_go: bool,
        life: f32,
    ) -> ActiveHold {
        ActiveHold {
            note_index: 42,
            start_time_ns: 0,
            end_time_ns: 1_000_000_000,
            note_type,
            let_go,
            is_pressed,
            life,
            last_update_time_ns: 0,
        }
    }

    #[test]
    fn score_validity_rejects_rate_below_one() {
        let chart = test_chart(ArrowStats::default(), 0, false);
        let reasons = score_invalid_reason_lines_for_options(
            &chart,
            ScoreValidityOptions {
                music_rate: 0.75,
                ..ScoreValidityOptions::default()
            },
        );

        assert_eq!(reasons, vec!["music rate is below 1.0x"]);

        let normalized = score_invalid_reason_lines_for_options(
            &chart,
            ScoreValidityOptions {
                music_rate: f32::NAN,
                ..ScoreValidityOptions::default()
            },
        );

        assert!(normalized.is_empty());
    }

    #[test]
    fn score_validity_rejects_matching_remove_masks() {
        let chart = test_chart(
            ArrowStats {
                jumps: 1,
                hands: 1,
                holds: 1,
                rolls: 1,
                lifts: 1,
                fakes: 1,
                ..ArrowStats::default()
            },
            1,
            false,
        );
        let reasons = score_invalid_reason_lines_for_options(
            &chart,
            ScoreValidityOptions {
                chart_effects: ChartAttackEffects {
                    remove_mask: REMOVE_MASK_BIT_NO_HOLDS
                        | REMOVE_MASK_BIT_NO_MINES
                        | REMOVE_MASK_BIT_NO_JUMPS
                        | REMOVE_MASK_BIT_NO_HANDS
                        | REMOVE_MASK_BIT_NO_QUADS
                        | REMOVE_MASK_BIT_NO_LIFTS
                        | REMOVE_MASK_BIT_NO_FAKES
                        | REMOVE_MASK_BIT_LITTLE,
                    holds_mask: HOLDS_MASK_BIT_NO_ROLLS,
                    ..ChartAttackEffects::default()
                },
                ..ScoreValidityOptions::default()
            },
        );

        assert!(reasons.contains(&"No Holds is enabled on a chart with holds"));
        assert!(reasons.contains(&"No Mines is enabled on a chart with mines"));
        assert!(reasons.contains(&"No Jumps is enabled on a chart with jumps"));
        assert!(reasons.contains(&"No Hands is enabled on a chart with hands"));
        assert!(reasons.contains(&"No Quads is enabled on a chart with quads"));
        assert!(reasons.contains(&"No Lifts is enabled on a chart with lifts"));
        assert!(reasons.contains(&"No Fakes is enabled on a chart with fakes"));
        assert!(reasons.contains(&"No Rolls is enabled on a chart with rolls"));
        assert!(reasons.contains(&"Little is enabled"));
    }

    #[test]
    fn score_validity_rejects_insert_and_hold_masks() {
        let chart = test_chart(ArrowStats::default(), 0, false);
        let reasons = score_invalid_reason_lines_for_options(
            &chart,
            ScoreValidityOptions {
                chart_effects: ChartAttackEffects {
                    insert_mask: INSERT_MASK_BIT_ECHO,
                    holds_mask: HOLDS_MASK_BIT_PLANTED
                        | HOLDS_MASK_BIT_FLOORED
                        | HOLDS_MASK_BIT_TWISTER,
                    ..ChartAttackEffects::default()
                },
                ..ScoreValidityOptions::default()
            },
        );

        assert!(reasons.contains(&"Echo is enabled"));
        assert!(reasons.contains(&"Planted is enabled"));
        assert!(reasons.contains(&"Floored is enabled"));
        assert!(reasons.contains(&"Twister is enabled"));
    }

    #[test]
    fn score_validity_rejects_attack_modes() {
        let chart_attacks = test_chart(ArrowStats::default(), 0, true);
        let no_chart_attacks = test_chart(ArrowStats::default(), 0, false);

        assert_eq!(
            score_invalid_reason_lines_for_options(
                &chart_attacks,
                ScoreValidityOptions {
                    attack_mode: GameplayAttackMode::Off,
                    ..ScoreValidityOptions::default()
                },
            ),
            vec!["AttackMode=Off is enabled on a chart with attacks"]
        );
        assert_eq!(
            score_invalid_reason_lines_for_options(
                &no_chart_attacks,
                ScoreValidityOptions {
                    attack_mode: GameplayAttackMode::Random,
                    ..ScoreValidityOptions::default()
                },
            ),
            vec!["AttackMode=Random is enabled"]
        );
        assert!(
            score_invalid_reason_lines_for_options(
                &chart_attacks,
                ScoreValidityOptions {
                    attack_mode: GameplayAttackMode::On,
                    ..ScoreValidityOptions::default()
                },
            )
            .is_empty()
        );
    }

    #[test]
    fn autosync_mode_status_lines_match_runtime_labels() {
        assert_eq!(autosync_mode_status_line(AutosyncMode::Off), None);
        assert_eq!(
            autosync_mode_status_line(AutosyncMode::Song),
            Some("AutoSync Song")
        );
        assert_eq!(
            autosync_mode_status_line(AutosyncMode::Machine),
            Some("AutoSync Machine")
        );
    }

    #[test]
    fn next_autosync_mode_cycles_and_skips_song_for_courses() {
        assert_eq!(
            next_autosync_mode(AutosyncMode::Off, false),
            AutosyncMode::Song
        );
        assert_eq!(
            next_autosync_mode(AutosyncMode::Song, false),
            AutosyncMode::Machine
        );
        assert_eq!(
            next_autosync_mode(AutosyncMode::Machine, false),
            AutosyncMode::Off
        );
        assert_eq!(
            next_autosync_mode(AutosyncMode::Off, true),
            AutosyncMode::Machine
        );
    }

    #[test]
    fn course_display_totals_copy_chart_totals() {
        let mut chart = test_chart(
            ArrowStats {
                total_steps: 123,
                ..ArrowStats::default()
            },
            0,
            false,
        );
        chart.possible_grade_points = 456;
        chart.holds_total = 7;
        chart.rolls_total = 8;
        chart.mines_total = 9;

        let totals = course_display_totals_for_chart(&chart);

        assert_eq!(totals.possible_grade_points, 456);
        assert_eq!(totals.total_steps, 123);
        assert_eq!(totals.holds_total, 7);
        assert_eq!(totals.rolls_total, 8);
        assert_eq!(totals.mines_total, 9);
    }

    #[test]
    fn course_display_carry_merges_stage_counts() {
        let carry = course_display_carry_for_stage(
            CourseDisplayCarry {
                life: 0.25,
                judgment_counts: [1, 2, 3, 4, 5, 6],
                scoring_counts: [6, 5, 4, 3, 2, 1],
                full_combo_grade: Some(JudgeGrade::Great),
                current_combo_window_counts: WindowCounts {
                    w1: 9,
                    ..WindowCounts::default()
                },
                window_counts: WindowCounts {
                    w1: 5,
                    ..WindowCounts::default()
                },
                window_counts_10ms_blue: WindowCounts {
                    w0: 6,
                    ..WindowCounts::default()
                },
                window_counts_display_blue: WindowCounts {
                    w2: 7,
                    ..WindowCounts::default()
                },
                holds_held_for_score: 8,
                holds_let_go_for_score: 9,
                rolls_held_for_score: 10,
                rolls_let_go_for_score: 11,
                mines_hit_for_score: 12,
                ..CourseDisplayCarry::default()
            },
            CourseDisplayStage {
                life: 1.25,
                judgment_counts: [10, 20, 30, 40, 50, 60],
                scoring_counts: [60, 50, 40, 30, 20, 10],
                full_combo_grade: Some(JudgeGrade::Excellent),
                current_combo_grade: Some(JudgeGrade::Fantastic),
                current_combo_window_counts: WindowCounts {
                    w0: 3,
                    ..WindowCounts::default()
                },
                combo: 0,
                window_counts: WindowCounts {
                    w1: 13,
                    ..WindowCounts::default()
                },
                window_counts_10ms_blue: WindowCounts {
                    w0: 14,
                    ..WindowCounts::default()
                },
                window_counts_display_blue: WindowCounts {
                    w2: 15,
                    ..WindowCounts::default()
                },
                holds_held_for_score: 16,
                holds_let_go_for_score: 17,
                rolls_held_for_score: 18,
                rolls_let_go_for_score: 19,
                mines_hit_for_score: 20,
                ..CourseDisplayStage::default()
            },
        );

        assert_eq!(carry.life, 1.0);
        assert_eq!(carry.judgment_counts, [11, 22, 33, 44, 55, 66]);
        assert_eq!(carry.scoring_counts, [66, 55, 44, 33, 22, 11]);
        assert_eq!(carry.full_combo_grade, Some(JudgeGrade::Great));
        assert_eq!(carry.current_combo_grade, Some(JudgeGrade::Fantastic));
        assert_eq!(carry.current_combo_window_counts.w0, 0);
        assert_eq!(carry.current_combo_window_counts.w1, 0);
        assert_eq!(carry.current_combo_window_counts.miss, 0);
        assert_eq!(carry.window_counts.w1, 18);
        assert_eq!(carry.window_counts_10ms_blue.w0, 20);
        assert_eq!(carry.window_counts_display_blue.w2, 22);
        assert_eq!(carry.holds_held_for_score, 24);
        assert_eq!(carry.holds_let_go_for_score, 26);
        assert_eq!(carry.rolls_held_for_score, 28);
        assert_eq!(carry.rolls_let_go_for_score, 30);
        assert_eq!(carry.mines_hit_for_score, 32);
    }

    #[test]
    fn course_display_carry_clears_full_combo_after_break() {
        let carry = course_display_carry_for_stage(
            CourseDisplayCarry {
                full_combo_grade: Some(JudgeGrade::Fantastic),
                ..CourseDisplayCarry::default()
            },
            CourseDisplayStage {
                combo: 4,
                current_combo_window_counts: WindowCounts {
                    w1: 4,
                    ..WindowCounts::default()
                },
                first_fc_attempt_broken: true,
                ..CourseDisplayStage::default()
            },
        );

        assert!(carry.full_combo_grade.is_none());
        assert!(carry.first_fc_attempt_broken);
        assert_eq!(carry.current_combo_window_counts.w1, 4);
    }

    #[test]
    fn course_life_carry_restores_finite_lifemeter_only() {
        assert_near(
            course_life_after_carry(
                0.5,
                Some(CourseDisplayCarry {
                    life: 0.32,
                    ..CourseDisplayCarry::default()
                }),
            ),
            0.32,
        );
        assert_near(
            course_life_after_carry(
                0.5,
                Some(CourseDisplayCarry {
                    life: 1.25,
                    ..CourseDisplayCarry::default()
                }),
            ),
            1.0,
        );
        assert_near(
            course_life_after_carry(
                0.5,
                Some(CourseDisplayCarry {
                    life: f32::NAN,
                    ..CourseDisplayCarry::default()
                }),
            ),
            0.5,
        );
        assert_near(course_life_after_carry(0.5, None), 0.5);
    }

    #[test]
    fn course_combo_carry_restores_combo_color_state() {
        let carry = CourseDisplayCarry {
            full_combo_grade: Some(JudgeGrade::Excellent),
            current_combo_grade: Some(JudgeGrade::Excellent),
            current_combo_window_counts: WindowCounts {
                w0: 7,
                ..WindowCounts::default()
            },
            ..CourseDisplayCarry::default()
        };
        let mut state = CourseComboCarryState::default();

        apply_course_combo_carry_state(&mut state, true, false, 37, Some(carry));

        assert_eq!(state.combo, 37);
        assert_eq!(state.full_combo_grade, Some(JudgeGrade::Excellent));
        assert_eq!(state.current_combo_grade, Some(JudgeGrade::Excellent));
        assert_eq!(state.current_combo_window_counts.w0, 7);
        assert!(!state.first_fc_attempt_broken);
    }

    #[test]
    fn course_combo_carry_marks_broken_attempts_without_combo() {
        let carry = CourseDisplayCarry {
            full_combo_grade: Some(JudgeGrade::Fantastic),
            current_combo_grade: Some(JudgeGrade::Fantastic),
            current_combo_window_counts: WindowCounts {
                w1: 3,
                ..WindowCounts::default()
            },
            ..CourseDisplayCarry::default()
        };
        let mut state = CourseComboCarryState::default();

        apply_course_combo_carry_state(&mut state, true, false, 0, Some(carry));

        assert_eq!(state.combo, 0);
        assert!(state.full_combo_grade.is_none());
        assert!(state.current_combo_grade.is_none());
        assert_eq!(state.current_combo_window_counts.w1, 0);
        assert!(state.first_fc_attempt_broken);
    }

    #[test]
    fn course_combo_carry_does_not_restore_in_replay_mode() {
        let mut state = CourseComboCarryState {
            combo: 9,
            full_combo_grade: Some(JudgeGrade::Great),
            current_combo_grade: Some(JudgeGrade::Great),
            ..CourseComboCarryState::default()
        };

        apply_course_combo_carry_state(
            &mut state,
            true,
            true,
            37,
            Some(CourseDisplayCarry::default()),
        );

        assert_eq!(state.combo, 9);
        assert_eq!(state.full_combo_grade, Some(JudgeGrade::Great));
        assert_eq!(state.current_combo_grade, Some(JudgeGrade::Great));
        assert!(state.first_fc_attempt_broken);
    }

    #[test]
    fn display_window_counts_mode_selects_cached_or_custom_splits() {
        assert_eq!(
            display_window_counts_mode(None, 10.0),
            DisplayWindowCountsMode::Canonical
        );
        assert_eq!(
            display_window_counts_mode(Some(FA_PLUS_W0_MS), 10.0),
            DisplayWindowCountsMode::Canonical
        );
        assert_eq!(
            display_window_counts_mode(Some(FA_PLUS_W010_MS), 15.0),
            DisplayWindowCountsMode::TenMsBlue
        );
        assert_eq!(
            display_window_counts_mode(Some(12.5), 12.5),
            DisplayWindowCountsMode::DisplayBlue
        );
        assert!(matches!(
            display_window_counts_mode(Some(12.5), 10.0),
            DisplayWindowCountsMode::CustomBlue { split_ms }
                if (split_ms - 12.5).abs() <= 0.000_1
        ));
    }

    #[test]
    fn display_window_counts_current_and_carry_use_selected_bucket() {
        let sources = DisplayWindowCountsSources {
            canonical: WindowCounts {
                w1: 1,
                ..WindowCounts::default()
            },
            ten_ms_blue: WindowCounts {
                w0: 2,
                ..WindowCounts::default()
            },
            display_blue: WindowCounts {
                w2: 3,
                ..WindowCounts::default()
            },
        };
        let carry = CourseDisplayCarry {
            window_counts: WindowCounts {
                w1: 10,
                ..WindowCounts::default()
            },
            window_counts_10ms_blue: WindowCounts {
                w0: 20,
                ..WindowCounts::default()
            },
            window_counts_display_blue: WindowCounts {
                w2: 30,
                ..WindowCounts::default()
            },
            ..CourseDisplayCarry::default()
        };

        let canonical = display_window_counts_with_carry(
            display_window_counts_current(sources, DisplayWindowCountsMode::Canonical).unwrap(),
            carry,
            DisplayWindowCountsMode::Canonical,
        );
        let ten_ms = display_window_counts_with_carry(
            display_window_counts_current(sources, DisplayWindowCountsMode::TenMsBlue).unwrap(),
            carry,
            DisplayWindowCountsMode::TenMsBlue,
        );
        let custom = display_window_counts_with_carry(
            WindowCounts {
                miss: 4,
                ..WindowCounts::default()
            },
            carry,
            DisplayWindowCountsMode::CustomBlue { split_ms: 12.5 },
        );

        assert_eq!(canonical.w1, 11);
        assert_eq!(ten_ms.w0, 22);
        assert!(
            display_window_counts_current(
                sources,
                DisplayWindowCountsMode::CustomBlue { split_ms: 12.5 },
            )
            .is_none()
        );
        assert_eq!(custom.miss, 4);
        assert_eq!(custom.w2, 30);
    }

    #[test]
    fn record_display_window_counts_updates_all_cached_buckets() {
        let judgment = Judgment {
            grade: JudgeGrade::Fantastic,
            time_error_ms: 16.0,
            time_error_music_ns: 16_000_000,
            window: Some(TimingWindow::W1),
            miss_because_held: false,
        };
        let mut canonical = WindowCounts::default();
        let mut ten_ms_blue = WindowCounts::default();
        let mut display_blue = WindowCounts::default();

        record_display_window_counts_for_judgment(
            &mut canonical,
            &mut ten_ms_blue,
            &mut display_blue,
            &judgment,
            20.0,
        );

        assert_eq!(canonical.w1, 1);
        assert_eq!(ten_ms_blue.w1, 1);
        assert_eq!(display_blue.w0, 1);
    }

    #[test]
    fn record_combo_window_count_uses_canonical_blue_window() {
        let judgment = Judgment {
            grade: JudgeGrade::Fantastic,
            time_error_ms: 16.0,
            time_error_music_ns: 16_000_000,
            window: Some(TimingWindow::W1),
            miss_because_held: false,
        };
        let mut counts = WindowCounts::default();

        record_combo_window_count_for_judgment(&mut counts, &judgment);

        assert_eq!(counts.w1, 1);
        assert_eq!(counts.w0, 0);
    }

    #[test]
    fn display_judgment_count_combines_stage_and_course_carry() {
        let carry = CourseDisplayCarry {
            judgment_counts: [10, 20, 30, 40, 50, 60],
            ..CourseDisplayCarry::default()
        };

        assert_eq!(
            display_judgment_count_for_grade([1, 2, 3, 4, 5, 6], carry, JudgeGrade::Great),
            33
        );
    }

    #[test]
    fn target_score_fixed_grades_map_to_percentages() {
        assert_eq!(
            target_score_setting_percent(GameplayTargetScoreSetting::CMinus),
            Some(50.0)
        );
        assert_eq!(
            target_score_setting_percent(GameplayTargetScoreSetting::S),
            Some(89.0)
        );
        assert_eq!(
            target_score_setting_percent(GameplayTargetScoreSetting::SPlus),
            Some(92.0)
        );
        assert_eq!(
            target_score_setting_percent(GameplayTargetScoreSetting::MachineBest),
            None
        );
    }

    #[test]
    fn target_score_resolves_best_score_settings() {
        assert_eq!(
            resolve_target_score_percent(GameplayTargetScoreSetting::MachineBest, Some(91.0), None),
            91.0
        );
        assert_eq!(
            resolve_target_score_percent(
                GameplayTargetScoreSetting::MachineBest,
                Some(91.0),
                Some(94.0),
            ),
            94.0
        );
        assert_eq!(
            resolve_target_score_percent(
                GameplayTargetScoreSetting::PersonalBest,
                Some(91.0),
                Some(94.0),
            ),
            91.0
        );
    }

    #[test]
    fn target_score_resolution_defaults_to_s_percent() {
        assert_eq!(
            resolve_target_score_percent(GameplayTargetScoreSetting::MachineBest, None, None),
            89.0
        );
        assert_eq!(
            resolve_target_score_percent(
                GameplayTargetScoreSetting::PersonalBest,
                None,
                Some(94.0)
            ),
            89.0
        );
    }

    #[test]
    fn mini_indicator_mode_for_options_uses_requested_mode_first() {
        let options = GameplayMiniIndicatorOptions {
            requested_mode: GameplayMiniIndicatorMode::RivalScoring,
            subtractive_scoring: true,
            pacemaker: true,
            ..GameplayMiniIndicatorOptions::default()
        };

        assert_eq!(
            mini_indicator_mode_for_options(options),
            GameplayMiniIndicatorMode::RivalScoring
        );
    }

    #[test]
    fn mini_indicator_mode_for_options_falls_back_to_scoring_flags() {
        assert_eq!(
            mini_indicator_mode_for_options(GameplayMiniIndicatorOptions {
                subtractive_scoring: true,
                pacemaker: true,
                ..GameplayMiniIndicatorOptions::default()
            }),
            GameplayMiniIndicatorMode::SubtractiveScoring
        );
        assert_eq!(
            mini_indicator_mode_for_options(GameplayMiniIndicatorOptions {
                pacemaker: true,
                ..GameplayMiniIndicatorOptions::default()
            }),
            GameplayMiniIndicatorMode::Pacemaker
        );
        assert_eq!(
            mini_indicator_mode_for_options(GameplayMiniIndicatorOptions::default()),
            GameplayMiniIndicatorMode::None
        );
    }

    #[test]
    fn mini_indicator_needs_stream_data_for_measure_or_mode() {
        assert!(!mini_indicator_needs_stream_data(
            GameplayMiniIndicatorOptions::default()
        ));
        assert!(mini_indicator_needs_stream_data(
            GameplayMiniIndicatorOptions {
                measure_counter_enabled: true,
                ..GameplayMiniIndicatorOptions::default()
            }
        ));
        assert!(mini_indicator_needs_stream_data(
            GameplayMiniIndicatorOptions {
                requested_mode: GameplayMiniIndicatorMode::StreamProg,
                ..GameplayMiniIndicatorOptions::default()
            }
        ));
        assert!(mini_indicator_needs_stream_data(
            GameplayMiniIndicatorOptions {
                pacemaker: true,
                ..GameplayMiniIndicatorOptions::default()
            }
        ));
    }

    #[test]
    fn ex_score_data_combines_live_inputs_with_course_carry() {
        let data = ex_score_data_from_display_inputs(
            ExScoreInputs {
                counts: WindowCounts {
                    w1: 3,
                    ..WindowCounts::default()
                },
                counts_10ms: WindowCounts {
                    w0: 2,
                    ..WindowCounts::default()
                },
                holds_held_for_score: 4,
                holds_let_go_for_score: 1,
                rolls_held_for_score: 5,
                rolls_let_go_for_score: 2,
                mines_hit_for_score: 6,
            },
            CourseDisplayCarry {
                holds_held_for_score: 7,
                holds_let_go_for_score: 3,
                rolls_held_for_score: 8,
                rolls_let_go_for_score: 4,
                mines_hit_for_score: 9,
                ..CourseDisplayCarry::default()
            },
            CourseDisplayTotals {
                total_steps: 10,
                holds_total: 11,
                rolls_total: 12,
                mines_total: 13,
                ..CourseDisplayTotals::default()
            },
        );

        assert_eq!(data.counts.w1, 3);
        assert_eq!(data.counts_10ms.w0, 2);
        assert_eq!(data.holds_held, 11);
        assert_eq!(data.holds_resolved, 15);
        assert_eq!(data.rolls_held, 13);
        assert_eq!(data.rolls_resolved, 19);
        assert_eq!(data.mines_hit, 15);
        assert_eq!(data.total_steps, 10);
        assert_eq!(data.holds_total, 11);
        assert_eq!(data.rolls_total, 12);
        assert_eq!(data.mines_total, 13);
    }

    #[test]
    fn effective_ex_score_inputs_prefers_failed_snapshot() {
        let live = ExScoreInputs {
            counts: WindowCounts {
                w1: 3,
                ..WindowCounts::default()
            },
            mines_hit_for_score: 4,
            ..ExScoreInputs::default()
        };
        let failed = ExScoreInputs {
            counts: WindowCounts {
                w2: 7,
                ..WindowCounts::default()
            },
            mines_hit_for_score: 8,
            ..ExScoreInputs::default()
        };

        let selected_live = effective_ex_score_inputs(live, None);
        let selected_failed = effective_ex_score_inputs(live, Some(failed));

        assert_eq!(selected_live.counts.w1, 3);
        assert_eq!(selected_live.mines_hit_for_score, 4);
        assert_eq!(selected_failed.counts.w2, 7);
        assert_eq!(selected_failed.mines_hit_for_score, 8);
    }

    #[test]
    fn failed_ex_score_capture_requires_fail_time() {
        let mut snapshot = None;
        let live = ExScoreInputs {
            mines_hit_for_score: 3,
            ..ExScoreInputs::default()
        };

        assert!(!capture_failed_ex_score_inputs(&mut snapshot, None, live));
        assert!(snapshot.is_none());
    }

    #[test]
    fn failed_ex_score_capture_records_first_snapshot_only() {
        let mut snapshot = None;
        let first = ExScoreInputs {
            holds_held_for_score: 2,
            ..ExScoreInputs::default()
        };
        let second = ExScoreInputs {
            holds_held_for_score: 9,
            ..ExScoreInputs::default()
        };

        assert!(capture_failed_ex_score_inputs(
            &mut snapshot,
            Some(10.0),
            first
        ));
        assert!(!capture_failed_ex_score_inputs(
            &mut snapshot,
            Some(12.0),
            second
        ));
        assert_eq!(snapshot.unwrap().holds_held_for_score, 2);
    }

    #[test]
    fn itg_score_inputs_combine_stage_and_course_carry() {
        let inputs = itg_score_inputs_from_display(
            ItgScoreStage {
                scoring_counts: [1, 2, 3, 4, 5, 6],
                holds_held_for_score: 7,
                holds_let_go_for_score: 8,
                rolls_held_for_score: 9,
                rolls_let_go_for_score: 10,
                mines_hit_for_score: 11,
            },
            CourseDisplayCarry {
                scoring_counts: [10, 20, 30, 40, 50, 60],
                holds_held_for_score: 12,
                holds_let_go_for_score: 13,
                rolls_held_for_score: 14,
                rolls_let_go_for_score: 15,
                mines_hit_for_score: 16,
                ..CourseDisplayCarry::default()
            },
            CourseDisplayTotals {
                possible_grade_points: 500,
                ..CourseDisplayTotals::default()
            },
        );

        assert_eq!(inputs.scoring_counts, [11, 22, 33, 44, 55, 66]);
        assert_eq!(inputs.holds_held_for_score, 19);
        assert_eq!(inputs.holds_resolved_for_score, 40);
        assert_eq!(inputs.rolls_held_for_score, 23);
        assert_eq!(inputs.rolls_resolved_for_score, 48);
        assert_eq!(inputs.mines_hit_for_score, 27);
        assert_eq!(inputs.possible_grade_points, 500);
    }

    #[test]
    fn itg_score_percent_helpers_preserve_display_units() {
        let inputs = ItgScoreInputs {
            scoring_counts: [1, 0, 0, 0, 0, 0],
            possible_grade_points: 10,
            ..ItgScoreInputs::default()
        };

        assert_eq!(itg_score_percent_from_inputs(inputs), 0.5);
        assert_eq!(predictive_itg_score_percent_from_inputs(inputs), 100.0);
    }

    #[test]
    fn score_display_mode_helpers_select_normal_or_predictive_percent() {
        let itg_inputs = ItgScoreInputs {
            scoring_counts: [1, 0, 0, 0, 0, 0],
            possible_grade_points: 10,
            ..ItgScoreInputs::default()
        };
        assert_eq!(
            display_itg_score_percent_for_mode(itg_inputs, GameplayScoreDisplayMode::Normal),
            50.0
        );
        assert_eq!(
            display_itg_score_percent_for_mode(itg_inputs, GameplayScoreDisplayMode::Predictive),
            100.0
        );

        let ex_score = judgment::ExScoreData {
            counts: WindowCounts {
                w0: 1,
                w1: 1,
                w2: 1,
                w3: 1,
                miss: 1,
                ..WindowCounts::default()
            },
            counts_10ms: WindowCounts {
                w0: 1,
                ..WindowCounts::default()
            },
            holds_held: 1,
            holds_resolved: 2,
            rolls_held: 1,
            rolls_resolved: 2,
            mines_hit: 1,
            total_steps: 6,
            holds_total: 2,
            rolls_total: 2,
            mines_total: 2,
        };

        assert_eq!(
            display_ex_score_percent_for_mode(&ex_score, GameplayScoreDisplayMode::Normal),
            judgment::ex_score_percent(&ex_score)
        );
        assert_eq!(
            display_ex_score_percent_for_mode(&ex_score, GameplayScoreDisplayMode::Predictive),
            judgment::predictive_ex_score_percents(&ex_score).0
        );
        assert_eq!(
            display_hard_ex_score_percent_for_mode(&ex_score, GameplayScoreDisplayMode::Normal),
            judgment::hard_ex_score_percent(&ex_score)
        );
        assert_eq!(
            display_hard_ex_score_percent_for_mode(&ex_score, GameplayScoreDisplayMode::Predictive),
            judgment::predictive_hard_ex_score_percents(&ex_score).0
        );
    }

    fn dense_note_data(measures: usize, rows_per_measure: usize, lanes: usize) -> Vec<u8> {
        let mut data = Vec::new();
        let row = match lanes {
            8 => b"10000000\n".as_slice(),
            _ => b"1000\n".as_slice(),
        };
        for measure in 0..measures {
            for _ in 0..rows_per_measure {
                data.extend_from_slice(row);
            }
            data.extend_from_slice(if measure + 1 == measures { b";" } else { b"," });
            data.push(b'\n');
        }
        data
    }

    #[test]
    fn stream_segments_for_note_data_uses_chart_note_bytes() {
        let data = dense_note_data(8, 32, 4);
        let (segments, total_stream, total_break) = stream_segments_for_note_data(&data, 4, true);

        assert_eq!(segments.len(), 1);
        assert_eq!(segments[0].start, 0);
        assert_eq!(segments[0].end, 8);
        assert!(!segments[0].is_break);
        assert_eq!(total_stream, 16.0);
        assert_eq!(total_break, 0.0);
    }

    #[test]
    fn stream_segments_for_note_data_supports_double_charts() {
        let data = dense_note_data(3, 16, 8);
        let (segments, total_stream, total_break) = stream_segments_for_note_data(&data, 8, false);

        assert_eq!(segments.len(), 1);
        assert_eq!(segments[0].start, 0);
        assert_eq!(segments[0].end, 3);
        assert!(!segments[0].is_break);
        assert_eq!(total_stream, 3.0);
        assert_eq!(total_break, 0.0);
    }

    #[test]
    fn measure_counter_segments_use_optional_threshold() {
        let densities = [12usize, 12, 0, 16];

        assert!(measure_counter_segments_for_densities(&densities, None).is_empty());

        let segments = measure_counter_segments_for_densities(&densities, Some(12));

        assert_eq!(segments.len(), 2);
        assert_eq!(segments[0].start, 0);
        assert_eq!(segments[0].end, 2);
        assert!(!segments[0].is_break);
        assert_eq!(segments[1].start, 3);
        assert_eq!(segments[1].end, 4);
        assert!(!segments[1].is_break);
    }

    #[test]
    fn zmod_stream_totals_for_densities_uses_constant_bpm_policy() {
        let densities = [32usize; 8];
        let (segments, total_stream, total_break) =
            zmod_stream_totals_for_densities(&densities, true);

        assert_eq!(segments.len(), 1);
        assert_eq!(segments[0].start, 0);
        assert_eq!(segments[0].end, 8);
        assert_eq!(total_stream, 16.0);
        assert_eq!(total_break, 0.0);
    }

    #[test]
    fn hold_head_render_flags_wait_for_receptor_and_live_hold() {
        let active = test_active_hold(NoteType::Hold, true, false, 1.0);
        let exhausted = test_active_hold(NoteType::Hold, true, false, 0.0);
        let let_go = test_active_hold(NoteType::Hold, true, true, 1.0);

        assert_eq!(
            hold_head_render_flags(Some(&active), 99.99, 100.0),
            (false, false)
        );
        assert_eq!(
            hold_head_render_flags(Some(&active), 100.0, 100.0),
            (true, true)
        );
        assert_eq!(
            hold_head_render_flags(Some(&exhausted), 100.0, 100.0),
            (false, false)
        );
        assert_eq!(
            hold_head_render_flags(Some(&let_go), 100.0, 100.0),
            (false, false)
        );
        assert_eq!(hold_head_render_flags(None, 100.0, 100.0), (false, false));
    }

    #[test]
    fn hold_head_render_flags_keep_rolls_active_between_taps() {
        let released_hold = test_active_hold(NoteType::Hold, false, false, 1.0);
        let released_roll = test_active_hold(NoteType::Roll, false, false, 1.0);

        assert_eq!(
            hold_head_render_flags(Some(&released_hold), 100.0, 100.0),
            (true, false)
        );
        assert_eq!(
            hold_head_render_flags(Some(&released_roll), 100.0, 100.0),
            (true, true)
        );
    }

    #[test]
    fn hold_explosion_active_requires_receptor_and_live_hold() {
        let active = test_active_hold(NoteType::Hold, true, false, 1.0);
        let exhausted = test_active_hold(NoteType::Hold, true, false, 0.0);
        let let_go = test_active_hold(NoteType::Hold, true, true, 1.0);

        assert!(!hold_explosion_active(Some(&active), 99.99, 100.0));
        assert!(hold_explosion_active(Some(&active), 100.0, 100.0));
        assert!(!hold_explosion_active(Some(&exhausted), 100.0, 100.0));
        assert!(!hold_explosion_active(Some(&let_go), 100.0, 100.0));
        assert!(!hold_explosion_active(None, 100.0, 100.0));
    }

    #[test]
    fn replaced_active_hold_settle_time_ignores_same_note() {
        assert_eq!(replaced_active_hold_settle_time(7, 100, 7, 200), None);
    }

    #[test]
    fn replaced_active_hold_settle_time_ignores_overlapping_hold() {
        assert_eq!(replaced_active_hold_settle_time(7, 300, 8, 200), None);
    }

    #[test]
    fn replaced_active_hold_settle_time_returns_previous_end() {
        assert_eq!(replaced_active_hold_settle_time(7, 200, 8, 200), Some(200));
        assert_eq!(replaced_active_hold_settle_time(7, 150, 8, 200), Some(150));
    }

    #[test]
    fn begin_hold_life_decay_starts_decay_once() {
        let mut hold = test_hold();
        hold.life = 0.25;
        let mut active = [false; 3];
        let mut indices = Vec::new();

        begin_hold_life_decay(&mut hold, &mut active, &mut indices, 1, 123);
        begin_hold_life_decay(&mut hold, &mut active, &mut indices, 1, 456);

        assert_eq!(hold.let_go_started_at, Some(123));
        assert_eq!(hold.let_go_starting_life, 0.25);
        assert_eq!(active, [false, true, false]);
        assert_eq!(indices, vec![1]);
    }

    #[test]
    fn begin_hold_life_decay_clamps_starting_life() {
        let mut high = test_hold();
        high.life = 2.0;
        let mut low = test_hold();
        low.life = -0.5;
        let mut active = [false; 2];
        let mut indices = Vec::new();

        begin_hold_life_decay(&mut high, &mut active, &mut indices, 0, 10);
        begin_hold_life_decay(&mut low, &mut active, &mut indices, 1, 20);

        assert_eq!(high.let_go_starting_life, MAX_HOLD_LIFE);
        assert_eq!(low.let_go_starting_life, 0.0);
    }

    #[test]
    fn begin_hold_life_decay_ignores_out_of_range_tracking_slot() {
        let mut hold = test_hold();
        let mut active = [false; 1];
        let mut indices = Vec::new();

        begin_hold_life_decay(&mut hold, &mut active, &mut indices, 4, 99);

        assert_eq!(hold.let_go_started_at, Some(99));
        assert_eq!(active, [false]);
        assert!(indices.is_empty());
    }

    #[test]
    fn apply_hold_let_go_result_marks_and_starts_decay() {
        let mut hold = test_hold();
        hold.life = 0.4;
        let mut active = [false; 2];
        let mut indices = Vec::new();

        assert!(apply_hold_let_go_result(
            Some(&mut hold),
            &mut active,
            &mut indices,
            1,
            123,
        ));

        assert_eq!(hold.result, Some(HoldResult::LetGo));
        assert_eq!(hold.let_go_started_at, Some(123));
        assert_eq!(hold.let_go_starting_life, 0.4);
        assert_eq!(active, [false, true]);
        assert_eq!(indices, vec![1]);
    }

    #[test]
    fn apply_hold_let_go_result_rejects_duplicate_but_allows_missing_hold() {
        let mut hold = test_hold();
        hold.result = Some(HoldResult::LetGo);
        let mut active = [false; 1];
        let mut indices = Vec::new();

        assert!(!apply_hold_let_go_result(
            Some(&mut hold),
            &mut active,
            &mut indices,
            0,
            10,
        ));
        assert!(apply_hold_let_go_result(
            None,
            &mut active,
            &mut indices,
            0,
            10,
        ));
        assert!(indices.is_empty());
    }

    #[test]
    fn apply_hold_success_result_marks_held_and_clears_decay() {
        let mut hold = test_hold();
        hold.life = 0.2;
        hold.let_go_started_at = Some(10);
        hold.let_go_starting_life = 0.2;
        hold.last_held_row_index = 1;
        hold.last_held_beat = 0.25;
        let mut active = [false, true];

        assert!(apply_hold_success_result(Some(&mut hold), &mut active, 1));

        assert_eq!(hold.result, Some(HoldResult::Held));
        assert_eq!(hold.life, MAX_HOLD_LIFE);
        assert_eq!(hold.let_go_started_at, None);
        assert_eq!(hold.let_go_starting_life, 0.0);
        assert_eq!(hold.last_held_row_index, hold.end_row_index);
        assert_eq!(hold.last_held_beat, hold.end_beat);
        assert_eq!(active, [false, false]);
    }

    #[test]
    fn apply_hold_success_result_rejects_duplicate_but_allows_missing_hold() {
        let mut hold = test_hold();
        hold.result = Some(HoldResult::Held);
        let mut active = [true];

        assert!(!apply_hold_success_result(Some(&mut hold), &mut active, 0));
        assert!(apply_hold_success_result(None, &mut active, 0));
        assert_eq!(active, [true]);
    }

    #[test]
    fn hold_result_stats_update_counts_held_holds_and_rolls() {
        let hold = hold_result_stats_update(NoteType::Hold, HoldResult::Held, false, false);
        let roll = hold_result_stats_update(NoteType::Roll, HoldResult::Held, false, false);

        assert_eq!(hold.holds_held, 1);
        assert_eq!(hold.holds_held_for_score, 1);
        assert!(hold.decrement_hands_holding);
        assert!(hold.update_grade_totals);
        assert_eq!(roll.rolls_held, 1);
        assert_eq!(roll.rolls_held_for_score, 1);
        assert!(roll.decrement_hands_holding);
        assert!(roll.update_grade_totals);
    }

    #[test]
    fn hold_result_stats_update_counts_let_go_for_score_only() {
        let hold = hold_result_stats_update(NoteType::Hold, HoldResult::LetGo, false, false);
        let roll = hold_result_stats_update(NoteType::Roll, HoldResult::LetGo, false, false);

        assert_eq!(hold.holds_held, 0);
        assert_eq!(hold.holds_let_go_for_score, 1);
        assert!(hold.update_grade_totals);
        assert_eq!(roll.rolls_held, 0);
        assert_eq!(roll.rolls_let_go_for_score, 1);
        assert!(roll.update_grade_totals);
    }

    #[test]
    fn hold_result_stats_update_respects_scoring_block_and_dead_state() {
        let blocked = hold_result_stats_update(NoteType::Hold, HoldResult::Held, true, false);
        let dead_held = hold_result_stats_update(NoteType::Hold, HoldResult::Held, false, true);
        let dead_let_go = hold_result_stats_update(NoteType::Hold, HoldResult::LetGo, false, true);

        assert_eq!(
            blocked,
            HoldResultStatsUpdate {
                decrement_hands_holding: true,
                ..HoldResultStatsUpdate::ZERO
            }
        );
        assert_eq!(dead_held.holds_held, 1);
        assert_eq!(dead_held.holds_held_for_score, 0);
        assert!(!dead_held.update_grade_totals);
        assert_eq!(
            dead_let_go,
            HoldResultStatsUpdate {
                decrement_hands_holding: true,
                ..HoldResultStatsUpdate::ZERO
            }
        );
    }

    #[test]
    fn hold_result_stats_update_ignores_non_hold_note_types() {
        let update = hold_result_stats_update(NoteType::Tap, HoldResult::Held, false, false);

        assert_eq!(
            update,
            HoldResultStatsUpdate {
                decrement_hands_holding: true,
                ..HoldResultStatsUpdate::ZERO
            }
        );
    }

    #[test]
    fn hold_result_stats_application_decrements_hands_and_adds_counts() {
        let mut state = HoldResultStatsState {
            hands_holding_count_for_stats: 2,
            holds_held: 10,
            holds_held_for_score: 20,
            holds_let_go_for_score: 30,
            rolls_held: 40,
            rolls_held_for_score: 50,
            rolls_let_go_for_score: 60,
        };

        apply_hold_result_stats_update(
            &mut state,
            HoldResultStatsUpdate {
                decrement_hands_holding: true,
                holds_held: 1,
                holds_held_for_score: 2,
                holds_let_go_for_score: 3,
                rolls_held: 4,
                rolls_held_for_score: 5,
                rolls_let_go_for_score: 6,
                update_grade_totals: true,
            },
        );

        assert_eq!(state.hands_holding_count_for_stats, 1);
        assert_eq!(state.holds_held, 11);
        assert_eq!(state.holds_held_for_score, 22);
        assert_eq!(state.holds_let_go_for_score, 33);
        assert_eq!(state.rolls_held, 44);
        assert_eq!(state.rolls_held_for_score, 55);
        assert_eq!(state.rolls_let_go_for_score, 66);
    }

    #[test]
    fn hold_result_stats_application_saturates_and_clamps_hand_count() {
        let mut zero_hands = HoldResultStatsState {
            hands_holding_count_for_stats: 0,
            holds_held: u32::MAX,
            ..HoldResultStatsState::default()
        };

        apply_hold_result_stats_update(
            &mut zero_hands,
            HoldResultStatsUpdate {
                decrement_hands_holding: true,
                holds_held: 1,
                ..HoldResultStatsUpdate::ZERO
            },
        );

        assert_eq!(zero_hands.hands_holding_count_for_stats, 0);
        assert_eq!(zero_hands.holds_held, u32::MAX);

        let mut negative_hands = HoldResultStatsState {
            hands_holding_count_for_stats: -3,
            ..HoldResultStatsState::default()
        };
        apply_hold_result_stats_update(
            &mut negative_hands,
            HoldResultStatsUpdate {
                decrement_hands_holding: true,
                ..HoldResultStatsUpdate::ZERO
            },
        );
        assert_eq!(negative_hands.hands_holding_count_for_stats, -3);
    }

    #[test]
    fn started_active_hold_state_resets_hold_and_builds_active_slot() {
        let mut hold = test_hold();
        hold.result = Some(HoldResult::LetGo);
        hold.life = 0.25;
        hold.let_go_started_at = Some(10);
        hold.let_go_starting_life = 0.25;

        let active = started_active_hold_state(Some(&mut hold), 3, NoteType::Hold, 100, 200, 150);

        assert_eq!(hold.result, Some(HoldResult::LetGo));
        assert_eq!(hold.life, MAX_HOLD_LIFE);
        assert_eq!(hold.let_go_started_at, None);
        assert_eq!(hold.let_go_starting_life, 0.0);
        assert_eq!(active.note_index, 3);
        assert_eq!(active.start_time_ns, 100);
        assert_eq!(active.end_time_ns, 200);
        assert_eq!(active.note_type, NoteType::Hold);
        assert!(!active.let_go);
        assert!(active.is_pressed);
        assert_eq!(active.life, MAX_HOLD_LIFE);
        assert_eq!(active.last_update_time_ns, 150);
    }

    #[test]
    fn started_active_hold_state_allows_missing_hold_data() {
        let active = started_active_hold_state(None, 4, NoteType::Roll, 10, 20, 12);

        assert_eq!(active.note_index, 4);
        assert_eq!(active.start_time_ns, 10);
        assert_eq!(active.end_time_ns, 20);
        assert_eq!(active.note_type, NoteType::Roll);
        assert_eq!(active.life, MAX_HOLD_LIFE);
    }

    #[test]
    fn refresh_roll_life_for_step_restores_life_and_progress_time() {
        let mut active = test_active_hold(NoteType::Roll, true, false, 0.2);
        active.start_time_ns = 100;
        active.end_time_ns = 300;
        active.last_update_time_ns = 120;
        let mut hold = test_hold();
        hold.life = 0.2;
        hold.let_go_started_at = Some(150);
        hold.let_go_starting_life = 0.2;

        assert!(refresh_roll_life_for_step(&mut active, &mut hold, 250));

        assert_eq!(active.life, MAX_HOLD_LIFE);
        assert_eq!(active.last_update_time_ns, 250);
        assert_eq!(hold.life, MAX_HOLD_LIFE);
        assert_eq!(hold.let_go_started_at, None);
        assert_eq!(hold.let_go_starting_life, 0.0);
    }

    #[test]
    fn refresh_roll_life_for_step_caps_progress_at_roll_end() {
        let mut active = test_active_hold(NoteType::Roll, true, false, 0.2);
        active.start_time_ns = 100;
        active.end_time_ns = 300;
        active.last_update_time_ns = 120;
        let mut hold = test_hold();

        assert!(refresh_roll_life_for_step(&mut active, &mut hold, 400));

        assert_eq!(active.last_update_time_ns, 300);
    }

    #[test]
    fn refresh_roll_life_for_step_rejects_non_roll_or_inactive_hold() {
        let mut hold_active = test_active_hold(NoteType::Hold, true, false, 0.2);
        let mut roll_let_go = test_active_hold(NoteType::Roll, true, true, 0.2);
        let mut exhausted_roll = test_active_hold(NoteType::Roll, true, false, 0.0);
        let mut hold = test_hold();

        assert!(!refresh_roll_life_for_step(&mut hold_active, &mut hold, 1));
        assert!(!refresh_roll_life_for_step(&mut roll_let_go, &mut hold, 1));
        assert!(!refresh_roll_life_for_step(
            &mut exhausted_roll,
            &mut hold,
            1,
        ));
    }

    #[test]
    fn refresh_roll_life_for_step_rejects_invalid_time_and_resolved_hold() {
        let mut early = test_active_hold(NoteType::Roll, true, false, 0.2);
        early.start_time_ns = 100;
        let mut invalid = early.clone();
        let mut resolved = early.clone();
        let mut hold = test_hold();
        let mut resolved_hold = test_hold();
        resolved_hold.result = Some(HoldResult::LetGo);

        assert!(!refresh_roll_life_for_step(&mut early, &mut hold, 99));
        assert!(!refresh_roll_life_for_step(
            &mut invalid,
            &mut hold,
            i64::MIN,
        ));
        assert!(!refresh_roll_life_for_step(
            &mut resolved,
            &mut resolved_hold,
            100,
        ));
    }

    #[test]
    fn advance_active_hold_to_time_resolves_success_at_tail() {
        let timing = test_timing(192);
        let mut active = test_active_hold(NoteType::Hold, true, false, MAX_HOLD_LIFE);
        active.start_time_ns = 0;
        active.end_time_ns = song_time_ns_from_seconds(1.0);
        active.last_update_time_ns = 0;
        let mut hold = test_hold();
        let target_time_ns = active.end_time_ns;

        let result = advance_active_hold_to_time(
            &mut active,
            &mut hold,
            &timing,
            0,
            0.0,
            target_time_ns,
            1.0,
        );

        assert_eq!(
            result.resolution,
            Some(ActiveHoldResolution::Success { note_index: 42 })
        );
        assert!(result.clear_active);
        assert_eq!(active.life, MAX_HOLD_LIFE);
        assert_eq!(hold.life, MAX_HOLD_LIFE);
    }

    #[test]
    fn advance_active_hold_to_time_resolves_let_go_at_zero_crossing() {
        let timing = test_timing(192);
        let mut active = test_active_hold(NoteType::Roll, false, false, 0.5);
        active.start_time_ns = 0;
        active.end_time_ns = song_time_ns_from_seconds(1.0);
        active.last_update_time_ns = 0;
        let mut hold = test_hold();
        hold.life = 0.5;
        let target_time_ns = active.end_time_ns;

        let result = advance_active_hold_to_time(
            &mut active,
            &mut hold,
            &timing,
            0,
            0.0,
            target_time_ns,
            1.0,
        );

        assert_eq!(
            result.resolution,
            Some(ActiveHoldResolution::LetGo {
                note_index: 42,
                time_ns: song_time_ns_from_seconds(TIMING_WINDOW_SECONDS_ROLL * 0.5),
            })
        );
        assert!(result.clear_active);
        assert!(active.let_go);
        assert_eq!(active.life, 0.0);
        assert_eq!(hold.life, 0.0);
    }

    #[test]
    fn advance_active_hold_to_time_resolves_pre_exhausted_life_at_start() {
        let timing = test_timing(192);
        let mut active = test_active_hold(NoteType::Hold, false, false, 0.0);
        active.start_time_ns = 100;
        active.last_update_time_ns = 50;
        let mut hold = test_hold();

        let result = advance_active_hold_to_time(&mut active, &mut hold, &timing, 0, 0.0, 200, 1.0);

        assert_eq!(
            result.resolution,
            Some(ActiveHoldResolution::LetGo {
                note_index: 42,
                time_ns: 100,
            })
        );
        assert!(result.clear_active);
        assert!(active.let_go);
    }

    #[test]
    fn advance_active_hold_to_time_updates_held_progress_row() {
        let timing = test_timing(192);
        let mut active = test_active_hold(NoteType::Hold, true, false, MAX_HOLD_LIFE);
        active.start_time_ns = 0;
        active.end_time_ns = song_time_ns_from_seconds(2.0);
        active.last_update_time_ns = 0;
        let mut hold = test_hold();
        hold.end_row_index = 192;
        hold.end_beat = 4.0;

        let result = advance_active_hold_to_time(
            &mut active,
            &mut hold,
            &timing,
            0,
            0.0,
            song_time_ns_from_seconds(0.5),
            1.0,
        );

        assert_eq!(result, ActiveHoldAdvance::default());
        assert!(hold.last_held_row_index > 0);
        assert!(hold.last_held_beat > 0.0);
        assert_eq!(active.last_update_time_ns, song_time_ns_from_seconds(0.5));
    }

    #[test]
    fn let_go_head_beat_stays_at_receptor_until_visible_clock_catches_up() {
        let waiting = let_go_head_beat(100.0, 108.0, 102.0, 101.25);
        let caught_up = let_go_head_beat(100.0, 108.0, 102.0, 103.0);
        let beyond_tail = let_go_head_beat(100.0, 108.0, 110.0, 120.0);

        assert!((waiting - 101.25).abs() <= 1.0e-6);
        assert!((caught_up - 102.0).abs() <= 1.0e-6);
        assert!((beyond_tail - 108.0).abs() <= 1.0e-6);
    }

    fn test_row_to_beat(last_row: usize) -> Vec<f32> {
        (0..=last_row)
            .map(|row| row as f32 / ROWS_PER_BEAT as f32)
            .collect()
    }

    fn test_timing(last_row: usize) -> TimingData {
        TimingData::from_segments(
            0.0,
            0.0,
            &TimingSegments::default(),
            &test_row_to_beat(last_row),
        )
    }

    fn test_song(chart_end: f32, audio_len: f32) -> SongData {
        SongData {
            simfile_path: PathBuf::from("song.ssc"),
            title: String::new(),
            subtitle: String::new(),
            translit_title: String::new(),
            translit_subtitle: String::new(),
            artist: String::new(),
            genre: String::new(),
            banner_path: None,
            background_path: None,
            background_changes: Vec::new(),
            background_layer2_changes: Vec::new(),
            foreground_changes: Vec::new(),
            background_lua_changes: Vec::new(),
            foreground_lua_changes: Vec::new(),
            has_lua: false,
            cdtitle_path: None,
            music_path: None,
            display_bpm: String::new(),
            offset: 0.0,
            sample_start: None,
            sample_length: None,
            min_bpm: 0.0,
            max_bpm: 0.0,
            normalized_bpms: String::new(),
            music_length_seconds: audio_len,
            first_second: 0.0,
            total_length_seconds: 0,
            precise_last_second_seconds: chart_end,
            charts: Vec::new(),
        }
    }

    #[test]
    fn note_types_define_rescore_and_edge_matching() {
        for note_type in [
            NoteType::Tap,
            NoteType::Lift,
            NoteType::Hold,
            NoteType::Roll,
        ] {
            assert!(counts_for_early_rescore(note_type));
        }
        assert!(!counts_for_early_rescore(NoteType::Mine));
        assert!(!counts_for_early_rescore(NoteType::Fake));

        assert!(lane_edge_matches_note_type(true, NoteType::Tap));
        assert!(!lane_edge_matches_note_type(false, NoteType::Tap));
        assert!(!lane_edge_matches_note_type(true, NoteType::Lift));
        assert!(lane_edge_matches_note_type(false, NoteType::Lift));
        assert!(!lane_edge_matches_note_type(true, NoteType::Mine));
        assert!(!lane_edge_matches_note_type(false, NoteType::Mine));
    }

    #[test]
    fn note_display_predicates_match_gameplay_visual_rules() {
        assert!(row_final_grade_hides_note(JudgeGrade::Fantastic));
        assert!(row_final_grade_hides_note(JudgeGrade::Excellent));
        assert!(row_final_grade_hides_note(JudgeGrade::Great));
        assert!(!row_final_grade_hides_note(JudgeGrade::Decent));
        assert!(!row_final_grade_hides_note(JudgeGrade::WayOff));
        assert!(!row_final_grade_hides_note(JudgeGrade::Miss));

        assert!(note_has_displayable_hold(&test_note(
            NoteType::Hold,
            Some(test_hold()),
            false,
        )));
        assert!(note_has_displayable_hold(&test_note(
            NoteType::Roll,
            Some(test_hold()),
            false,
        )));
        assert!(!note_has_displayable_hold(&test_note(
            NoteType::Hold,
            None,
            false,
        )));
        assert!(!note_has_displayable_hold(&test_note(
            NoteType::Tap,
            Some(test_hold()),
            false,
        )));
    }

    #[test]
    fn held_miss_tracking_only_uses_taps_holds_and_rolls() {
        assert!(note_tracks_held_miss(NoteType::Tap));
        assert!(note_tracks_held_miss(NoteType::Hold));
        assert!(note_tracks_held_miss(NoteType::Roll));
        assert!(!note_tracks_held_miss(NoteType::Mine));
        assert!(!note_tracks_held_miss(NoteType::Lift));
        assert!(!note_tracks_held_miss(NoteType::Fake));
    }

    #[test]
    fn held_miss_window_marks_first_pressed_track_in_window() {
        let mut tap = test_note_at(NoteType::Tap, None, false, 0, 0.0);
        tap.column = 0;
        let mut duplicate_track = test_note_at(NoteType::Tap, None, false, 1, 0.0);
        duplicate_track.column = 0;
        let mut hold = test_note_at(NoteType::Hold, Some(test_hold()), false, 2, 0.0);
        hold.column = 1;
        let mut roll = test_note_at(NoteType::Roll, Some(test_hold()), false, 3, 0.0);
        roll.column = 2;
        let mut mine = test_note_at(NoteType::Mine, None, false, 4, 0.0);
        mine.column = 3;
        let mut lift = test_note_at(NoteType::Lift, None, false, 5, 0.0);
        lift.column = 2;
        let mut unjudgable = test_note_at(NoteType::Tap, None, false, 6, 0.0);
        unjudgable.column = 3;
        unjudgable.can_be_judged = false;
        let notes = [tap, duplicate_track, hold, roll, mine, lift, unjudgable];
        let note_times = [1_000, 1_010, 1_020, 1_040, 1_050, 1_060, 1_070];
        let mut held_window = [false; 7];
        let mut inputs = [false; MAX_COLS];
        inputs[0] = true;
        inputs[1] = true;
        inputs[3] = true;

        track_held_miss_window_for_player(
            &notes,
            &note_times,
            &mut held_window,
            (0, notes.len()),
            (0, 4),
            0,
            &inputs,
            1_000,
            50,
        );

        assert_eq!(held_window, [true, false, true, false, false, false, false]);
    }

    #[test]
    fn column_cues_skip_fake_notes_and_mark_mines() {
        assert_eq!(
            column_cue_is_mine(&test_note(NoteType::Tap, None, false)),
            Some(false)
        );
        assert_eq!(
            column_cue_is_mine(&test_note(NoteType::Lift, None, false)),
            Some(false)
        );
        assert_eq!(
            column_cue_is_mine(&test_note(NoteType::Mine, None, false)),
            Some(true)
        );
        assert_eq!(
            column_cue_is_mine(&test_note(NoteType::Mine, None, true)),
            None
        );
        assert_eq!(
            column_cue_is_mine(&test_note(NoteType::Fake, None, false)),
            None
        );
    }

    #[test]
    fn column_cue_builder_filters_fakes_and_preserves_timed_gaps() {
        let mut first = test_note_at(NoteType::Tap, None, false, 48, 1.0);
        first.column = 0;
        let mut fake = test_note_at(NoteType::Tap, None, true, 96, 2.0);
        fake.column = 1;
        fake.can_be_judged = false;
        let mut later = test_note_at(NoteType::Tap, None, false, 192, 4.0);
        later.column = 2;
        let notes = [first, fake, later];
        let note_times = [
            song_time_ns_from_seconds(1.0),
            song_time_ns_from_seconds(2.0),
            song_time_ns_from_seconds(4.0),
        ];

        let cues = build_column_cues_for_player(&notes, (0, notes.len()), &note_times, 0, 4, 0.0);

        assert_eq!(cues.len(), 2);
        assert_near(cues[0].start_time, 0.0);
        assert_near(cues[0].duration, 1.0);
        assert_eq!(cues[0].columns.len(), 1);
        assert_eq!(cues[0].columns[0].column, 0);
        assert_eq!(cues[0].columns[0].is_mine, false);
        assert_near(cues[1].start_time, 1.0);
        assert_near(cues[1].duration, 3.0);
        assert_eq!(cues[1].columns.len(), 1);
        assert_eq!(cues[1].columns[0].column, 2);
    }

    #[test]
    fn column_cue_builder_sorts_dedups_and_offsets_first_visible_time() {
        let mut first = test_note_at(NoteType::Tap, None, false, 48, 1.0);
        first.column = 2;
        let mut duplicate = test_note_at(NoteType::Lift, None, false, 48, 1.0);
        duplicate.column = 2;
        let mut mine = test_note_at(NoteType::Mine, None, false, 48, 1.0);
        mine.column = 0;
        let notes = [first, duplicate, mine];
        let note_times = [
            song_time_ns_from_seconds(1.0),
            song_time_ns_from_seconds(1.0),
            song_time_ns_from_seconds(1.0),
        ];

        let cues = build_column_cues_for_player(&notes, (0, notes.len()), &note_times, 0, 4, -0.5);

        assert_eq!(cues.len(), 1);
        assert_near(cues[0].start_time, -0.5);
        assert_near(cues[0].duration, 1.5);
        assert_eq!(cues[0].columns.len(), 2);
        assert_eq!(cues[0].columns[0].column, 0);
        assert_eq!(cues[0].columns[0].is_mine, true);
        assert_eq!(cues[0].columns[1].column, 2);
        assert_eq!(cues[0].columns[1].is_mine, false);
    }

    fn xover_anno(beat: f32, note_count: u8, column_mask: u8, is_crossover: bool) -> CrossoverRow {
        debug_assert_eq!(
            u32::from(note_count),
            column_mask.count_ones(),
            "xover_anno note_count must equal the number of set columns",
        );
        CrossoverRow {
            beat,
            column_mask,
            crossover: is_crossover,
            bracket: note_count > 1,
        }
    }

    fn xover_time(beat: f32) -> f32 {
        beat * 0.5
    }

    #[test]
    fn crossover_rows_encode_notes_and_hold_tails_for_parity() {
        let mut tap = test_note_at(NoteType::Tap, None, false, 96, 2.0);
        tap.column = 1;
        let mut lift = test_note_at(NoteType::Lift, None, false, 48, 1.0);
        lift.column = 2;
        let mut hold = test_note_at(NoteType::Hold, Some(test_hold()), false, 144, 3.0);
        hold.column = 3;
        hold.hold.as_mut().unwrap().end_row_index = 192;
        hold.hold.as_mut().unwrap().end_beat = 4.0;
        let mut roll = test_note_at(NoteType::Roll, Some(test_hold()), false, 240, 5.0);
        roll.column = 0;
        roll.hold.as_mut().unwrap().end_row_index = 288;
        roll.hold.as_mut().unwrap().end_beat = 6.0;
        let mut mine = test_note_at(NoteType::Mine, None, false, 336, 7.0);
        mine.column = 2;

        let (rows, beats) = build_crossover_rows::<4>(&[tap, lift, hold, roll, mine], (0, 5), 0);

        assert_eq!(
            rows,
            vec![
                [b'0', b'0', b'L', b'0'],
                [b'0', b'1', b'0', b'0'],
                [b'0', b'0', b'0', b'2'],
                [b'0', b'0', b'0', b'3'],
                [b'4', b'0', b'0', b'0'],
                [b'3', b'0', b'0', b'0'],
                [b'0', b'0', b'M', b'0'],
            ]
        );
        assert_eq!(beats, vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0]);
    }

    #[test]
    fn crossover_rows_filter_columns_and_fake_notes() {
        let mut before = test_note_at(NoteType::Tap, None, false, 48, 1.0);
        before.column = 1;
        let mut in_range = test_note_at(NoteType::Tap, None, false, 48, 1.0);
        in_range.column = 2;
        let mut after = test_note_at(NoteType::Tap, None, false, 48, 1.0);
        after.column = 6;
        let mut fake_tap = test_note_at(NoteType::Tap, None, true, 96, 2.0);
        fake_tap.column = 3;
        let mut fake_mine = test_note_at(NoteType::Mine, None, true, 144, 3.0);
        fake_mine.column = 4;
        let notes = [before, in_range, after, fake_tap, fake_mine];

        let (rows, beats) = build_crossover_rows::<4>(&notes, (0, notes.len()), 2);

        assert_eq!(
            rows,
            vec![[b'1', b'0', b'0', b'0'], [b'0', b'0', b'M', b'0']]
        );
        assert_eq!(beats, vec![1.0, 3.0]);
    }

    #[test]
    fn crossover_rows_real_arrows_replace_same_cell_mines() {
        let mut mine = test_note_at(NoteType::Mine, None, false, 48, 1.0);
        mine.column = 0;
        let mut tap = test_note_at(NoteType::Tap, None, false, 48, 1.0);
        tap.column = 0;
        let mut same_lane_mine = test_note_at(NoteType::Mine, None, false, 96, 2.0);
        same_lane_mine.column = 1;
        let notes = [mine, tap, same_lane_mine];

        let (rows, beats) = build_crossover_rows::<4>(&notes, (0, notes.len()), 0);

        assert_eq!(
            rows,
            vec![[b'1', b'0', b'0', b'0'], [b'0', b'M', b'0', b'0']]
        );
        assert_eq!(beats, vec![1.0, 2.0]);
    }

    #[test]
    fn crossover_arrow_col_picks_outer_and_inner_panels() {
        assert_eq!(crossover_arrow_col(0b0001, true), Some(0));
        assert_eq!(crossover_arrow_col(0b0001, false), None);
        assert_eq!(crossover_arrow_col(0b1000, true), Some(3));
        assert_eq!(crossover_arrow_col(0b0010, false), Some(1));
        assert_eq!(crossover_arrow_col(0b0010, true), None);
        assert_eq!(crossover_arrow_col(0b0100, false), Some(2));
        assert_eq!(crossover_arrow_col(0b1001, true), Some(0));
        assert_eq!(crossover_arrow_col(0b0110, false), Some(1));
        assert_eq!(crossover_arrow_col(1 << 4, true), Some(4));
        assert_eq!(crossover_arrow_col(1 << 5, false), Some(5));
    }

    #[test]
    fn crossover_cue_builder_emits_single_and_scooby_cues() {
        let single = [
            xover_anno(0.0, 1, 0b0010, false),
            xover_anno(0.5, 1, 0b0001, true),
        ];
        let cues = build_crossover_cues_core(&single, xover_time, 0, 500, 8, false, 0.0);
        assert_eq!(cues.len(), 1);
        assert_near(cues[0].start_time, -0.5);
        assert_near(cues[0].duration, 0.575);
        assert_eq!(cues[0].columns.len(), 2);
        assert_eq!(cues[0].columns[0].column, 0);
        assert_eq!(cues[0].columns[0].is_mine, false);
        assert_eq!(cues[0].columns[1].column, 1);

        let scooby = [
            xover_anno(0.0, 1, 0b0010, false),
            xover_anno(0.5, 1, 0b0001, true),
            xover_anno(1.0, 1, 0b1000, true),
        ];
        let cues = build_crossover_cues_core(&scooby, xover_time, 0, 500, 8, false, 0.0);
        assert_eq!(cues.len(), 1);
        assert_eq!(cues[0].columns.len(), 3);
        assert_eq!(cues[0].columns[2].column, 3);
        assert_eq!(cues[0].columns[2].is_mine, true);
    }

    #[test]
    fn crossover_cue_builder_uses_quantization_and_gap_policy() {
        let isolated = [
            xover_anno(0.0, 1, 0b0010, false),
            xover_anno(5.0, 1, 0b0001, true),
        ];
        let cues = build_crossover_cues_core(&isolated, xover_time, 0, 500, 8, false, 0.0);
        assert!(cues.is_empty());

        let gap = [
            xover_anno(0.0, 1, 0b0010, false),
            xover_anno(2.0, 1, 0b0001, true),
            xover_anno(2.4, 1, 0b0010, false),
        ];
        let cues = build_crossover_cues_core(&gap, xover_time, 0, 500, 8, false, 0.0);
        assert_eq!(cues.len(), 1);
        assert_near(cues[0].duration, 1.575);
        assert_near(cues[0].start_time, -0.5);
    }

    #[test]
    fn crossover_cue_builder_clamps_overlap_and_first_visible_offset() {
        let overlapping = [
            xover_anno(0.0, 1, 0b0010, false),
            xover_anno(0.5, 1, 0b0001, true),
            xover_anno(0.6, 1, 0b0100, false),
            xover_anno(0.7, 1, 0b1000, true),
        ];
        let cues = build_crossover_cues_core(&overlapping, xover_time, 0, 500, 8, false, 0.0);
        assert_eq!(cues.len(), 2);
        assert_near(cues[1].start_time, 0.0);
        assert_near(cues[1].duration, 0.375);

        let early = [
            xover_anno(0.0, 1, 0b0010, false),
            xover_anno(0.5, 1, 0b0001, true),
        ];
        let cues = build_crossover_cues_core(&early, xover_time, 0, 500, 8, false, -0.3);
        assert_eq!(cues.len(), 1);
        assert_near(cues[0].start_time, -0.8);
        assert_near(cues[0].duration, 0.875);

        let later = [
            xover_anno(2.0, 1, 0b0010, false),
            xover_anno(2.5, 1, 0b0001, true),
        ];
        let cues = build_crossover_cues_core(&later, xover_time, 0, 500, 8, false, -0.3);
        assert_eq!(cues.len(), 1);
        assert_near(cues[0].start_time, 0.5);
        assert_near(cues[0].duration, 0.575);
    }

    #[test]
    fn crossover_cue_builder_offsets_columns_and_respects_brackets() {
        let shifted = [
            xover_anno(0.0, 1, 0b0010, false),
            xover_anno(0.5, 1, 0b0001, true),
        ];
        let cues = build_crossover_cues_core(&shifted, xover_time, 4, 500, 8, false, 0.0);
        assert_eq!(cues.len(), 1);
        assert_eq!(cues[0].columns[0].column, 4);
        assert_eq!(cues[0].columns[1].column, 5);

        let bracket = [
            xover_anno(0.0, 1, 0b0100, false),
            xover_anno(0.5, 2, 0b0011, true),
        ];
        let excluded = build_crossover_cues_core(&bracket, xover_time, 0, 500, 8, false, 0.0);
        assert!(excluded.is_empty());
        let included = build_crossover_cues_core(&bracket, xover_time, 0, 500, 8, true, 0.0);
        assert_eq!(included.len(), 1);

        let bracket_scooby = [
            xover_anno(0.0, 1, 0b0010, false),
            xover_anno(0.5, 1, 0b0001, true),
            xover_anno(1.0, 2, 0b1100, true),
        ];
        let excluded =
            build_crossover_cues_core(&bracket_scooby, xover_time, 0, 500, 8, false, 0.0);
        assert_eq!(excluded.len(), 1);
        assert_eq!(excluded[0].columns.len(), 2);
        let included = build_crossover_cues_core(&bracket_scooby, xover_time, 0, 500, 8, true, 0.0);
        assert_eq!(included.len(), 1);
        assert_eq!(included[0].columns.len(), 3);
        assert_eq!(included[0].columns[2].is_mine, true);
    }

    #[test]
    fn late_resolution_uses_largest_gameplay_window() {
        let timing_profile = TimingProfile::default_itg_with_fa_plus();
        let seconds = song_time_ns_to_seconds(late_note_resolution_window_ns(&timing_profile, 1.0));
        assert!((seconds - 0.3515).abs() <= 1e-6);
    }

    #[test]
    fn max_step_distance_scales_with_music_rate() {
        let timing_profile = TimingProfile::default_itg_with_fa_plus();
        let seconds = song_time_ns_to_seconds(max_step_distance_ns(&timing_profile, 1.5));
        assert!((seconds - 0.52725).abs() <= 1e-6);
    }

    #[test]
    fn song_audio_end_time_uses_positive_chart_or_audio_end() {
        assert_eq!(
            song_audio_end_time_ns(&test_song(5.0, 10.0)),
            song_time_ns_from_seconds(5.0)
        );
        assert_eq!(
            song_audio_end_time_ns(&test_song(f32::NAN, 10.0)),
            song_time_ns_from_seconds(10.0)
        );
        assert_eq!(
            song_audio_end_time_ns(&test_song(5.0, 0.0)),
            song_time_ns_from_seconds(5.0)
        );
        assert_eq!(song_audio_end_time_ns(&test_song(0.0, 0.0)), 0);
    }

    #[test]
    fn stage_music_cut_uses_negative_lead_in() {
        let cut = stage_music_cut(2.5);
        assert_eq!(cut.start_sec, -2.5);
        assert!(cut.length_sec.is_infinite());
        assert_eq!(cut.fade_in_sec, 0.0);
        assert_eq!(cut.fade_out_sec, 0.0);

        let clamped = stage_music_cut(-1.0);
        assert_eq!(clamped.start_sec, 0.0);
    }

    #[test]
    fn current_song_clock_snapshot_prefers_mapped_audio_time() {
        let now = Instant::now();
        let snapshot = current_song_clock_snapshot(
            GameplayAudioSnapshot {
                stream_clock: GameplayStreamClockSnapshot {
                    stream_seconds: 12.0,
                    music_nanos: song_time_ns_from_seconds(4.25),
                    music_seconds_per_second: 1.5,
                    has_music_mapping: true,
                    valid_at: now,
                    valid_at_host_nanos: 99,
                },
                timing_diag_enabled: true,
                timing_diag_callback_gap_ns: 123,
                ..GameplayAudioSnapshot::default()
            },
            0.75,
            2.0,
            -0.1,
        );

        assert_eq!(snapshot.song_time_ns, song_time_ns_from_seconds(4.25));
        assert_eq!(snapshot.seconds_per_second, 1.5);
        assert!(snapshot.mapped_audio);
        assert_eq!(snapshot.valid_at_host_nanos, 99);
        assert!(snapshot.timing_diag_enabled);
        assert_eq!(snapshot.timing_diag_callback_gap_ns, 123);
    }

    #[test]
    fn current_song_clock_snapshot_falls_back_for_invalid_mapped_slope() {
        let snapshot = current_song_clock_snapshot(
            GameplayAudioSnapshot {
                stream_clock: GameplayStreamClockSnapshot {
                    music_nanos: song_time_ns_from_seconds(2.0),
                    music_seconds_per_second: f32::NAN,
                    has_music_mapping: true,
                    ..GameplayStreamClockSnapshot::default()
                },
                ..GameplayAudioSnapshot::default()
            },
            1.25,
            0.0,
            0.0,
        );

        assert_eq!(snapshot.song_time_ns, song_time_ns_from_seconds(2.0));
        assert_eq!(snapshot.seconds_per_second, 1.25);
        assert!(snapshot.mapped_audio);
    }

    #[test]
    fn current_song_clock_snapshot_maps_unmapped_stream_position() {
        let snapshot = current_song_clock_snapshot(
            GameplayAudioSnapshot {
                stream_clock: GameplayStreamClockSnapshot {
                    stream_seconds: 3.0,
                    has_music_mapping: false,
                    ..GameplayStreamClockSnapshot::default()
                },
                ..GameplayAudioSnapshot::default()
            },
            1.5,
            2.0,
            -0.1,
        );

        assert_eq!(snapshot.song_time_ns, song_time_ns_from_seconds(1.45));
        assert_eq!(snapshot.seconds_per_second, 1.5);
        assert!(!snapshot.mapped_audio);
    }

    #[test]
    fn recent_step_tracks_count_current_press_inside_jump_window() {
        let mut pressed_since_ns = [None; MAX_COLS];
        pressed_since_ns[0] = Some(song_time_ns_from_seconds(10.0));
        pressed_since_ns[1] = Some(song_time_ns_from_seconds(9.9));
        pressed_since_ns[2] = Some(song_time_ns_from_seconds(9.74));
        pressed_since_ns[4] = Some(song_time_ns_from_seconds(10.0));

        assert_eq!(
            recent_step_tracks(&pressed_since_ns, 0, 4, song_time_ns_from_seconds(10.0)),
            2
        );
        assert_eq!(
            recent_step_tracks(&pressed_since_ns, 4, 8, song_time_ns_from_seconds(10.0)),
            1
        );
        assert_eq!(
            recent_step_tracks(&pressed_since_ns, 0, 4, INVALID_SONG_TIME_NS),
            0
        );
    }

    #[test]
    fn recent_step_calories_use_recent_track_count_and_weight() {
        let mut pressed_since_ns = [None; MAX_COLS];
        pressed_since_ns[0] = Some(song_time_ns_from_seconds(10.0));
        pressed_since_ns[1] = Some(song_time_ns_from_seconds(9.9));
        pressed_since_ns[2] = Some(song_time_ns_from_seconds(9.74));

        assert!(
            (recent_step_calories(
                &pressed_since_ns,
                0,
                4,
                song_time_ns_from_seconds(10.0),
                120,
            ) - judgment::step_calories(120, 2))
            .abs()
                <= 1e-6
        );
    }

    #[test]
    fn recent_step_calories_ignore_invalid_event_time() {
        let pressed_since_ns = [Some(song_time_ns_from_seconds(10.0)); MAX_COLS];

        assert_eq!(
            recent_step_calories(&pressed_since_ns, 0, 4, INVALID_SONG_TIME_NS, 120),
            0.0
        );
    }

    #[test]
    fn visible_notefield_time_subtracts_visual_delay() {
        let music_time_ns = song_time_ns_from_seconds(100.0);
        let visible = song_time_ns_to_seconds(visible_notefield_time_ns(music_time_ns, 0.010));

        assert!((visible - 99.990).abs() < 0.000_5);
    }

    #[test]
    fn stream_position_to_music_time_applies_lead_in_rate_and_offset_anchor() {
        assert_near(music_time_from_stream_position(3.0, 2.0, -0.100, 1.5), 1.45);
        assert_near(music_time_from_stream_position(3.0, -2.0, 0.0, 1.0), 3.0);
        assert_near(music_time_from_stream_position(3.0, 2.0, -0.100, 0.0), 1.0);
        assert_near(
            music_time_from_stream_position(3.0, 2.0, -0.100, f32::NAN),
            1.0,
        );
    }

    #[test]
    fn assist_clap_rows_include_judgable_lifts_and_skip_fakes() {
        let mut notes = vec![
            test_note_at(NoteType::Tap, None, true, 24, 0.5),
            test_note_at(NoteType::Lift, None, false, 48, 1.0),
            test_note_at(NoteType::Mine, None, false, 96, 2.0),
            test_note_at(NoteType::Roll, Some(test_hold()), false, 144, 3.0),
        ];
        notes[3]
            .hold
            .as_mut()
            .expect("roll has hold data")
            .end_row_index = 192;

        assert_eq!(
            build_assist_clap_rows(&notes, (0, notes.len())),
            vec![48, 144]
        );
        assert_eq!(build_assist_clap_rows(&notes, (2, 2)), Vec::<usize>::new());
    }

    #[test]
    fn assist_clap_cursor_skips_rows_at_or_before_current_row() {
        let rows = [48, 96, 144];

        assert_eq!(assist_clap_cursor_for_row(&rows, -1), 0);
        assert_eq!(assist_clap_cursor_for_row(&rows, 47), 0);
        assert_eq!(assist_clap_cursor_for_row(&rows, 48), 1);
        assert_eq!(assist_clap_cursor_for_row(&rows, 120), 2);
        assert_eq!(assist_clap_cursor_for_row(&rows, 144), 3);
    }

    #[test]
    fn assist_lookahead_horizon_adds_margin_and_scales_by_slope() {
        let h = assist_lookahead_music_horizon_seconds(0.020, 1.0);
        assert!((h - 0.070).abs() <= 1e-6, "h={h}");

        let h2 = assist_lookahead_music_horizon_seconds(0.020, 2.0);
        assert!((h2 - 0.140).abs() <= 1e-6, "h2={h2}");

        assert!(
            (assist_lookahead_music_horizon_seconds(0.0, f32::NAN)
                - ASSIST_TICK_LOOKAHEAD_MARGIN_SECONDS)
                .abs()
                <= 1e-6
        );
        assert!(assist_lookahead_music_horizon_seconds(-1.0, 1.0) >= 0.0);
    }

    #[test]
    fn end_times_wait_for_audio_tail() {
        let notes = [test_note_at(NoteType::Tap, None, false, 96, 2.0)];
        let note_times = [song_time_ns_from_seconds(2.0)];
        let hold_end_times = [None];
        let audio_end_time_ns = song_time_ns_from_seconds(10.0);

        let (notes_end_time_ns, music_end_time_ns) =
            compute_end_times_ns(&notes, &note_times, &hold_end_times, 1.0, audio_end_time_ns);

        assert!(notes_end_time_ns < audio_end_time_ns);
        assert_eq!(music_end_time_ns, audio_end_time_ns);
    }

    #[test]
    fn end_times_use_judgable_and_relevant_tails_separately() {
        let mut fake = test_note_at(NoteType::Fake, None, true, 240, 5.0);
        fake.can_be_judged = false;
        let notes = [test_note_at(NoteType::Tap, None, false, 48, 1.0), fake];
        let note_times = [
            song_time_ns_from_seconds(1.0),
            song_time_ns_from_seconds(5.0),
        ];
        let hold_end_times = [None, None];

        let (notes_end_time_ns, music_end_time_ns) =
            compute_end_times_ns(&notes, &note_times, &hold_end_times, 1.0, 0);

        assert!(notes_end_time_ns < note_times[1]);
        assert!(music_end_time_ns > note_times[1]);
    }

    #[test]
    fn missed_note_cutoff_row_matches_stop_delay_rules() {
        let row_to_beat = test_row_to_beat(ROWS_PER_BEAT as usize * 4);
        let stop_timing = TimingData::from_segments(
            0.0,
            0.0,
            &TimingSegments {
                bpms: vec![(0.0, 60.0)],
                stops: vec![StopSegment {
                    beat: 1.0,
                    duration: 2.0,
                }],
                ..TimingSegments::default()
            },
            &row_to_beat,
        );
        let stop_cutoff_time = stop_timing
            .get_time_for_beat_ns(1.0)
            .saturating_add(song_time_ns_from_seconds(0.5));
        assert_eq!(
            missed_note_cutoff_row_for_timing(&stop_timing, stop_cutoff_time),
            ROWS_PER_BEAT as usize + 1
        );

        let delay_timing = TimingData::from_segments(
            0.0,
            0.0,
            &TimingSegments {
                bpms: vec![(0.0, 60.0)],
                delays: vec![DelaySegment {
                    beat: 1.0,
                    duration: 2.0,
                }],
                ..TimingSegments::default()
            },
            &row_to_beat,
        );
        let delay_cutoff_time = delay_timing
            .get_time_for_beat_ns(1.0)
            .saturating_sub(song_time_ns_from_seconds(0.5));
        assert_eq!(
            missed_note_cutoff_row_for_timing(&delay_timing, delay_cutoff_time),
            ROWS_PER_BEAT as usize
        );
    }

    #[test]
    fn missed_note_cutoff_row_uses_chart_row_indices() {
        let timing = TimingData::from_segments(
            0.0,
            0.0,
            &TimingSegments {
                bpms: vec![(0.0, 60.0)],
                ..TimingSegments::default()
            },
            &[0.0, 4.0, 8.0],
        );

        assert_eq!(
            missed_note_cutoff_row_for_timing(&timing, timing.get_time_for_beat_ns(3.0)),
            1
        );
        assert_eq!(
            missed_note_cutoff_row_for_timing(&timing, timing.get_time_for_beat_ns(4.0)),
            1
        );
        assert_eq!(
            missed_note_cutoff_row_for_timing(&timing, timing.get_time_for_beat_ns(4.1)),
            2
        );
    }

    #[test]
    fn timing_row_floor_steps_back_when_row_is_after_beat() {
        let timing = test_timing(ROWS_PER_BEAT as usize * 2);

        assert_eq!(timing_row_floor(&timing, 1.0), ROWS_PER_BEAT as usize);
        assert_eq!(
            timing_row_floor(&timing, 1.0 - 0.001),
            ROWS_PER_BEAT as usize - 1
        );
        assert_eq!(timing_row_floor(&timing, -1.0), 0);
    }

    #[test]
    fn assist_row_no_offset_cancels_global_offset() {
        let timing = TimingData::from_segments(
            0.0,
            0.100,
            &TimingSegments::default(),
            &test_row_to_beat(ROWS_PER_BEAT as usize * 2),
        );
        let music_time_ns = song_time_ns_from_seconds(1.0);
        let direct_row = timing_row_floor(&timing, timing.get_beat_for_time_ns(music_time_ns));

        assert!(direct_row > ROWS_PER_BEAT as usize);
        assert_eq!(
            assist_row_no_offset_for_timing(&timing, 0.100, music_time_ns),
            ROWS_PER_BEAT as i32
        );
    }

    #[test]
    fn note_count_stats_group_rows_and_clamp_range() {
        let notes = [
            test_note_at(NoteType::Tap, None, false, 48, 1.0),
            test_note_at(NoteType::Lift, None, false, 48, 1.0),
            test_note_at(NoteType::Tap, None, false, 96, 2.0),
        ];

        let stats = build_note_count_stats(&notes, (0, 99));

        assert_eq!(stats.len(), 2);
        assert_eq!(stats[0].beat, 1.0);
        assert_eq!(stats[0].notes_lower, 0);
        assert_eq!(stats[0].notes_upper, 2);
        assert_eq!(stats[1].beat, 2.0);
        assert_eq!(stats[1].notes_lower, 2);
        assert_eq!(stats[1].notes_upper, 3);
    }

    #[test]
    fn first_time_index_lookup_uses_range_and_clamps_bounds() {
        let times = [10, 20, 30, 40];

        assert_eq!(first_time_index_at_or_after(&times, (1, 3), 5), 1);
        assert_eq!(first_time_index_at_or_after(&times, (1, 3), 25), 2);
        assert_eq!(first_time_index_at_or_after(&times, (1, 3), 35), 3);
        assert_eq!(first_time_index_at_or_after(&times, (2, 99), 35), 3);
        assert_eq!(first_time_index_at_or_after(&times, (99, 100), 35), 4);
    }

    #[test]
    fn first_row_entry_lookup_uses_row_time_and_clamps_bounds() {
        let row = |time_ns| RowEntry {
            row_index: 0,
            time_ns,
            nonmine_note_indices: [usize::MAX; MAX_COLS],
            nonmine_note_count: 0,
            rescore_track_count: 0,
            unresolved_count: 0,
            unresolved_nonlift_count: 0,
            had_provisional_early_hit: false,
            final_outcome: None,
        };
        let rows = [row(10), row(20), row(30), row(40)];

        assert_eq!(first_row_entry_index_at_or_after_time(&rows, (1, 3), 5), 1);
        assert_eq!(first_row_entry_index_at_or_after_time(&rows, (1, 3), 25), 2);
        assert_eq!(first_row_entry_index_at_or_after_time(&rows, (1, 3), 35), 3);
        assert_eq!(
            first_row_entry_index_at_or_after_time(&rows, (2, 99), 35),
            3
        );
        assert_eq!(
            first_row_entry_index_at_or_after_time(&rows, (99, 100), 35),
            4
        );
    }

    #[test]
    fn score_rows_finalized_accepts_all_finalized_ranges() {
        let row = |finalized: bool| RowEntry {
            row_index: 0,
            time_ns: 0,
            nonmine_note_indices: [usize::MAX; MAX_COLS],
            nonmine_note_count: 0,
            rescore_track_count: 0,
            unresolved_count: 0,
            unresolved_nonlift_count: 0,
            had_provisional_early_hit: false,
            final_outcome: finalized.then_some(FinalizedRowOutcome {
                final_grade: JudgeGrade::Great,
            }),
        };
        let rows = [row(true), row(true), row(true)];
        let ranges = [(0, 2), (2, 3)];

        assert!(score_rows_finalized_for_players(&rows, &ranges, 2));
    }

    #[test]
    fn score_rows_finalized_rejects_pending_row_in_active_range() {
        let row = |finalized: bool| RowEntry {
            row_index: 0,
            time_ns: 0,
            nonmine_note_indices: [usize::MAX; MAX_COLS],
            nonmine_note_count: 0,
            rescore_track_count: 0,
            unresolved_count: 0,
            unresolved_nonlift_count: 0,
            had_provisional_early_hit: false,
            final_outcome: finalized.then_some(FinalizedRowOutcome {
                final_grade: JudgeGrade::Great,
            }),
        };
        let rows = [row(true), row(false), row(true)];
        let ranges = [(0, 2), (2, 3)];

        assert!(!score_rows_finalized_for_players(&rows, &ranges, 2));
        assert!(score_rows_finalized_for_players(&rows, &ranges, 0));
    }

    #[test]
    fn score_rows_finalized_ignores_inactive_player_ranges() {
        let row = |finalized: bool| RowEntry {
            row_index: 0,
            time_ns: 0,
            nonmine_note_indices: [usize::MAX; MAX_COLS],
            nonmine_note_count: 0,
            rescore_track_count: 0,
            unresolved_count: 0,
            unresolved_nonlift_count: 0,
            had_provisional_early_hit: false,
            final_outcome: finalized.then_some(FinalizedRowOutcome {
                final_grade: JudgeGrade::Great,
            }),
        };
        let rows = [row(true), row(false)];
        let ranges = [(0, 1), (1, 2)];

        assert!(score_rows_finalized_for_players(&rows, &ranges, 1));
        assert!(!score_rows_finalized_for_players(&rows, &ranges, 2));
    }

    #[test]
    fn score_rows_finalized_clamps_empty_out_of_bounds_ranges() {
        let rows: [RowEntry; 0] = [];
        let ranges = [(99, 100), (0, 0)];

        assert!(score_rows_finalized_for_players(&rows, &ranges, 2));
    }

    #[test]
    fn practice_player_cursors_seek_notes_rows_and_mines() {
        let note_times = [10, 20, 30, 40];
        let row = |time_ns| RowEntry {
            row_index: 0,
            time_ns,
            nonmine_note_indices: [usize::MAX; MAX_COLS],
            nonmine_note_count: 0,
            rescore_track_count: 0,
            unresolved_count: 0,
            unresolved_nonlift_count: 0,
            had_provisional_early_hit: false,
            final_outcome: None,
        };
        let rows = [row(10), row(30), row(50)];
        let mine_times = [15, 35];
        let mine_ix = [7, 9];

        assert_eq!(
            practice_player_cursors(
                &note_times,
                (1, 4),
                &rows,
                (0, 3),
                &mine_times,
                &mine_ix,
                25
            ),
            PracticePlayerCursors {
                note_cursor: 2,
                row_cursor: 1,
                mine_ix_cursor: 1,
                mine_avoid_cursor: 9,
            }
        );
    }

    #[test]
    fn practice_player_cursors_fall_back_to_note_end_after_last_mine() {
        let note_times = [10, 20, 30, 40];
        let row = |time_ns| RowEntry {
            row_index: 0,
            time_ns,
            nonmine_note_indices: [usize::MAX; MAX_COLS],
            nonmine_note_count: 0,
            rescore_track_count: 0,
            unresolved_count: 0,
            unresolved_nonlift_count: 0,
            had_provisional_early_hit: false,
            final_outcome: None,
        };
        let rows = [row(10), row(30), row(50)];
        let mine_times = [15, 35];
        let mine_ix = [7, 9];

        assert_eq!(
            practice_player_cursors(
                &note_times,
                (1, 4),
                &rows,
                (0, 3),
                &mine_times,
                &mine_ix,
                99
            ),
            PracticePlayerCursors {
                note_cursor: 4,
                row_cursor: 3,
                mine_ix_cursor: 2,
                mine_avoid_cursor: 4,
            }
        );
    }

    #[test]
    fn row_entry_counts_unresolved_notes_and_rescore_tracks() {
        let mut judged = test_note_at(NoteType::Tap, None, false, 48, 1.0);
        judged.result = Some(Judgment {
            time_error_ms: 4.0,
            time_error_music_ns: 4_000_000,
            grade: JudgeGrade::Great,
            window: Some(TimingWindow::W3),
            miss_because_held: false,
        });
        judged.early_result = Some(Judgment {
            time_error_ms: -12.0,
            time_error_music_ns: -12_000_000,
            grade: JudgeGrade::Decent,
            window: Some(TimingWindow::W4),
            miss_because_held: false,
        });
        let notes = [
            judged,
            test_note_at(NoteType::Lift, None, false, 48, 1.0),
            test_note_at(NoteType::Mine, None, false, 48, 1.0),
        ];
        let note_times = [
            song_time_ns_from_seconds(1.0),
            song_time_ns_from_seconds(1.0),
            song_time_ns_from_seconds(1.0),
        ];
        let mut note_indices = [usize::MAX; MAX_COLS];
        note_indices[0] = 0;
        note_indices[1] = 1;
        let row_entry = build_row_entry(48, note_indices, 2, &notes, &note_times);

        assert_eq!(row_entry.row_index, 48);
        assert_eq!(row_entry.time_ns, note_times[0]);
        assert_eq!(row_entry.note_indices(), &[0, 1]);
        assert_eq!(count_rescore_tracks_on_row(&row_entry), 2);
        assert_eq!(row_entry.unresolved_count, 1);
        assert_eq!(row_entry.unresolved_nonlift_count, 0);
        assert!(row_entry.had_provisional_early_hit);
        assert_eq!(row_entry.final_outcome, None);
    }

    #[test]
    fn row_entry_counts_unresolved_nonlift_notes() {
        let notes = [
            test_note_at(NoteType::Tap, None, false, 48, 1.0),
            test_note_at(NoteType::Lift, None, false, 48, 1.0),
        ];
        let note_times = [
            song_time_ns_from_seconds(1.0),
            song_time_ns_from_seconds(1.0),
        ];
        let mut note_indices = [usize::MAX; MAX_COLS];
        note_indices[0] = 0;
        note_indices[1] = 1;
        let row_entry = build_row_entry(48, note_indices, 2, &notes, &note_times);

        assert_eq!(row_entry.unresolved_count, 2);
        assert_eq!(row_entry.unresolved_nonlift_count, 1);
    }

    #[test]
    fn practice_reset_clears_note_mine_and_hold_results() {
        let judgment = Judgment {
            time_error_ms: 4.0,
            time_error_music_ns: 4_000_000,
            grade: JudgeGrade::Great,
            window: Some(TimingWindow::W3),
            miss_because_held: false,
        };
        let mut hold = test_hold();
        hold.result = Some(HoldResult::LetGo);
        hold.life = 0.25;
        hold.let_go_started_at = Some(100);
        hold.let_go_starting_life = 0.25;
        hold.last_held_row_index = 0;
        hold.last_held_beat = 0.0;

        let mut notes = vec![
            test_note_at(NoteType::Hold, Some(hold), false, 48, 1.0),
            test_note_at(NoteType::Mine, None, false, 48, 1.0),
        ];
        notes[0].result = Some(judgment);
        notes[0].early_result = Some(judgment);
        notes[1].mine_result = Some(MineResult::Hit);
        let note_times = [
            song_time_ns_from_seconds(1.0),
            song_time_ns_from_seconds(1.0),
        ];
        let mut note_indices = [usize::MAX; MAX_COLS];
        note_indices[0] = 0;
        let mut row_entries = vec![build_row_entry(48, note_indices, 1, &notes, &note_times)];

        reset_practice_notes_and_rows(&mut notes, &mut row_entries, &note_times);

        assert!(notes[0].result.is_none());
        assert!(notes[0].early_result.is_none());
        assert_eq!(notes[1].mine_result, None);
        let reset_hold = notes[0].hold.as_ref().expect("hold data");
        assert_eq!(reset_hold.result, None);
        assert_eq!(reset_hold.life, MAX_HOLD_LIFE);
        assert_eq!(reset_hold.let_go_started_at, None);
        assert_eq!(reset_hold.let_go_starting_life, MAX_HOLD_LIFE);
        assert_eq!(reset_hold.last_held_row_index, notes[0].row_index);
        assert_eq!(reset_hold.last_held_beat, notes[0].beat);
    }

    #[test]
    fn practice_reset_rebuilds_row_entries_from_reset_notes() {
        let judgment = Judgment {
            time_error_ms: 4.0,
            time_error_music_ns: 4_000_000,
            grade: JudgeGrade::Great,
            window: Some(TimingWindow::W3),
            miss_because_held: false,
        };
        let mut notes = vec![test_note_at(NoteType::Tap, None, false, 48, 1.0)];
        notes[0].result = Some(judgment);
        notes[0].early_result = Some(judgment);
        let note_times = [song_time_ns_from_seconds(1.0)];
        let mut note_indices = [usize::MAX; MAX_COLS];
        note_indices[0] = 0;
        let mut row_entries = vec![build_row_entry(48, note_indices, 1, &notes, &note_times)];
        row_entries[0].final_outcome = Some(FinalizedRowOutcome {
            final_grade: JudgeGrade::Great,
        });

        reset_practice_notes_and_rows(&mut notes, &mut row_entries, &note_times);

        assert_eq!(row_entries[0].note_indices(), &[0]);
        assert_eq!(row_entries[0].unresolved_count, 1);
        assert_eq!(row_entries[0].unresolved_nonlift_count, 1);
        assert!(!row_entries[0].had_provisional_early_hit);
        assert_eq!(row_entries[0].final_outcome, None);
    }

    #[test]
    fn row_entry_provisional_early_marks_valid_row_only() {
        let notes = [test_note_at(NoteType::Tap, None, false, 48, 1.0)];
        let note_times = [song_time_ns_from_seconds(1.0)];
        let mut note_indices = [usize::MAX; MAX_COLS];
        note_indices[0] = 0;
        let mut row_entries = vec![build_row_entry(48, note_indices, 1, &notes, &note_times)];
        let note_row_entry_indices = [u32::MAX, 0];

        assert!(!mark_row_entry_provisional_early_result(
            &mut row_entries,
            &note_row_entry_indices,
            0,
        ));
        assert!(!row_entries[0].had_provisional_early_hit);
        assert!(mark_row_entry_provisional_early_result(
            &mut row_entries,
            &note_row_entry_indices,
            1,
        ));
        assert!(row_entries[0].had_provisional_early_hit);
    }

    #[test]
    fn row_entry_note_finalized_decrements_counts() {
        let notes = [
            test_note_at(NoteType::Tap, None, false, 48, 1.0),
            test_note_at(NoteType::Lift, None, false, 48, 1.0),
        ];
        let note_times = [
            song_time_ns_from_seconds(1.0),
            song_time_ns_from_seconds(1.0),
        ];
        let mut note_indices = [usize::MAX; MAX_COLS];
        note_indices[0] = 0;
        note_indices[1] = 1;
        let mut row_entries = vec![build_row_entry(48, note_indices, 2, &notes, &note_times)];
        let note_row_entry_indices = [0, 0];

        assert!(mark_row_entry_note_finalized(
            &mut row_entries,
            &note_row_entry_indices,
            0,
            NoteType::Tap,
        ));
        assert_eq!(row_entries[0].unresolved_count, 1);
        assert_eq!(row_entries[0].unresolved_nonlift_count, 0);
        assert!(mark_row_entry_note_finalized(
            &mut row_entries,
            &note_row_entry_indices,
            1,
            NoteType::Lift,
        ));
        assert_eq!(row_entries[0].unresolved_count, 0);
        assert_eq!(row_entries[0].unresolved_nonlift_count, 0);
    }

    #[test]
    fn row_entry_note_finalized_saturates_and_ignores_missing_rows() {
        let notes = [test_note_at(NoteType::Tap, None, false, 48, 1.0)];
        let note_times = [song_time_ns_from_seconds(1.0)];
        let mut note_indices = [usize::MAX; MAX_COLS];
        note_indices[0] = 0;
        let mut row_entries = vec![build_row_entry(48, note_indices, 1, &notes, &note_times)];
        row_entries[0].unresolved_count = 0;
        row_entries[0].unresolved_nonlift_count = 0;
        let note_row_entry_indices = [0, 99];

        assert!(mark_row_entry_note_finalized(
            &mut row_entries,
            &note_row_entry_indices,
            0,
            NoteType::Tap,
        ));
        assert_eq!(row_entries[0].unresolved_count, 0);
        assert_eq!(row_entries[0].unresolved_nonlift_count, 0);
        assert!(!mark_row_entry_note_finalized(
            &mut row_entries,
            &note_row_entry_indices,
            1,
            NoteType::Tap,
        ));
        assert!(!mark_row_entry_note_finalized(
            &mut row_entries,
            &note_row_entry_indices,
            2,
            NoteType::Tap,
        ));
    }

    #[test]
    fn cached_row_lookup_uses_row_map_and_final_outcome() {
        let mut note = test_note_at(NoteType::Tap, None, false, 48, 1.0);
        note.result = Some(test_judgment(JudgeGrade::Great));
        let notes = [note];
        let note_times = [song_time_ns_from_seconds(1.0)];
        let mut note_indices = [usize::MAX; MAX_COLS];
        note_indices[0] = 0;
        let mut row_entries = vec![build_row_entry(48, note_indices, 1, &notes, &note_times)];
        row_entries[0].final_outcome = Some(FinalizedRowOutcome {
            final_grade: JudgeGrade::Great,
        });
        let mut row_map_cache = vec![u32::MAX; 49];
        row_map_cache[48] = 0;

        let row_entry =
            row_entry_for_cached_row(&row_entries, &row_map_cache, 48).expect("cached row");
        let outcome = finalized_row_outcome_for_cached_row(&row_entries, &row_map_cache, 48)
            .expect("finalized row outcome");

        assert_eq!(row_entry.note_indices(), &[0]);
        assert_eq!(outcome.final_grade, JudgeGrade::Great);
        assert_eq!(row_entry_index_for_cached_row(&row_map_cache, 47), None);
    }

    #[test]
    fn completed_row_judgment_waits_for_all_notes_and_returns_indices() {
        let mut judged = test_note_at(NoteType::Tap, None, false, 48, 1.0);
        judged.result = Some(test_judgment(JudgeGrade::Great));
        let pending = test_note_at(NoteType::Tap, None, false, 48, 1.0);
        let notes = [judged, pending];
        let note_times = [
            song_time_ns_from_seconds(1.0),
            song_time_ns_from_seconds(1.0),
        ];
        let mut note_indices = [usize::MAX; MAX_COLS];
        note_indices[0] = 0;
        note_indices[1] = 1;
        let row_entry = build_row_entry(48, note_indices, 2, &notes, &note_times);
        assert!(completed_row_final_judgment(&notes, &row_entry).is_none());

        let mut notes = notes;
        notes[1].result = Some(test_judgment(JudgeGrade::Great));
        let judgment =
            completed_row_final_judgment(&notes, &row_entry).expect("completed row judgment");
        let (indices, len, flash_judgment) =
            completed_row_flash_note_indices_and_judgment(&notes, &row_entry)
                .expect("completed row flash judgment");

        assert_eq!(judgment.grade, JudgeGrade::Great);
        assert_eq!(flash_judgment.grade, JudgeGrade::Great);
        assert_eq!(&indices[..len], &[0, 1]);
    }

    #[test]
    fn judged_row_cursor_skips_finalized_and_finds_ready_rows() {
        let mut row1_note = test_note_at(NoteType::Tap, None, false, 48, 1.0);
        row1_note.result = Some(test_judgment(JudgeGrade::Great));
        let row2_note = test_note_at(NoteType::Tap, None, false, 96, 2.0);
        let mut row3_note = test_note_at(NoteType::Tap, None, false, 144, 3.0);
        row3_note.result = Some(test_judgment(JudgeGrade::Great));
        row3_note.early_result = Some(test_judgment(JudgeGrade::Decent));
        let notes = [row1_note, row2_note, row3_note];
        let note_times = [
            song_time_ns_from_seconds(1.0),
            song_time_ns_from_seconds(2.0),
            song_time_ns_from_seconds(3.0),
        ];
        let mut row1_indices = [usize::MAX; MAX_COLS];
        let mut row2_indices = [usize::MAX; MAX_COLS];
        let mut row3_indices = [usize::MAX; MAX_COLS];
        row1_indices[0] = 0;
        row2_indices[0] = 1;
        row3_indices[0] = 2;
        let mut row_entries = vec![
            build_row_entry(48, row1_indices, 1, &notes, &note_times),
            build_row_entry(96, row2_indices, 1, &notes, &note_times),
            build_row_entry(144, row3_indices, 1, &notes, &note_times),
        ];
        row_entries[0].final_outcome = Some(FinalizedRowOutcome {
            final_grade: JudgeGrade::Great,
        });

        let lookahead = song_time_ns_from_seconds(3.5);
        let cursor = advance_judged_row_cursor(0, row_entries.len(), |idx| {
            player_row_scan_state(&row_entries, idx, lookahead)
        });
        let ready = next_ready_row_in_lookahead(cursor, row_entries.len(), |idx| {
            player_row_scan_state(&row_entries, idx, lookahead)
        });

        assert_eq!(cursor, 1);
        assert_eq!(ready, Some((2, 144, true)));
        assert!(suppress_final_bad_rescore_visual(true, JudgeGrade::Decent));
        assert!(!suppress_final_bad_rescore_visual(true, JudgeGrade::Great));
    }

    #[test]
    fn finalized_row_awards_hand_for_notes_or_carried_holds() {
        assert!(finalized_row_awards_hand(JudgeGrade::Great, 3, 0));
        assert!(finalized_row_awards_hand(JudgeGrade::Excellent, 2, 1));
        assert!(finalized_row_awards_hand(JudgeGrade::Fantastic, 1, 2));
        assert!(!finalized_row_awards_hand(JudgeGrade::Great, 2, 0));
    }

    #[test]
    fn finalized_row_awards_hand_suppresses_bad_rows() {
        assert!(!finalized_row_awards_hand(JudgeGrade::Miss, 4, 0));
        assert!(!finalized_row_awards_hand(JudgeGrade::WayOff, 4, 0));
        assert!(finalized_row_awards_hand(JudgeGrade::Decent, 3, 0));
    }

    #[test]
    fn carried_holds_down_at_row_counts_engaged_prior_holds() {
        let mut held = test_hold();
        held.last_held_row_index = 96;
        let notes = vec![
            test_note_at(NoteType::Hold, Some(held), false, 48, 1.0),
            test_note_at(NoteType::Tap, None, false, 96, 2.0),
        ];
        let mut active = vec![None; 4];
        let mut hold = test_active_hold(NoteType::Hold, true, false, 1.0);
        hold.note_index = 0;
        active[1] = Some(hold);

        let count = carried_holds_down_at_row(&notes, &active, (0, 4), 96);

        assert_eq!(count, 1);
    }

    #[test]
    fn carried_holds_down_at_row_ignores_inactive_and_future_holds() {
        let mut carried = test_hold();
        carried.last_held_row_index = 96;
        let mut short = test_hold();
        short.last_held_row_index = 72;
        let notes = vec![
            test_note_at(NoteType::Hold, Some(carried.clone()), false, 48, 1.0),
            test_note_at(NoteType::Hold, Some(short), false, 48, 1.0),
            test_note_at(NoteType::Hold, Some(carried), false, 96, 2.0),
        ];
        let mut active = vec![None; 5];
        let mut let_go = test_active_hold(NoteType::Hold, true, true, 1.0);
        let_go.note_index = 0;
        active[1] = Some(let_go);
        let mut depleted = test_active_hold(NoteType::Hold, true, false, 0.0);
        depleted.note_index = 0;
        active[2] = Some(depleted);
        let mut ended_before_row = test_active_hold(NoteType::Hold, true, false, 1.0);
        ended_before_row.note_index = 1;
        active[3] = Some(ended_before_row);
        let mut starts_on_row = test_active_hold(NoteType::Hold, true, false, 1.0);
        starts_on_row.note_index = 2;
        active[4] = Some(starts_on_row);

        let count = carried_holds_down_at_row(&notes, &active, (0, 5), 96);

        assert_eq!(count, 0);
    }

    #[test]
    fn carried_holds_down_at_row_clamps_ranges_and_note_indices() {
        let mut held = test_hold();
        held.last_held_row_index = 144;
        let notes = vec![test_note_at(NoteType::Hold, Some(held), false, 48, 1.0)];
        let mut active = vec![None; 3];
        let mut valid = test_active_hold(NoteType::Hold, true, false, 1.0);
        valid.note_index = 0;
        active[1] = Some(valid);
        let mut invalid_note = test_active_hold(NoteType::Hold, true, false, 1.0);
        invalid_note.note_index = 99;
        active[2] = Some(invalid_note);

        assert_eq!(carried_holds_down_at_row(&notes, &active, (1, 99), 96), 1);
        assert_eq!(carried_holds_down_at_row(&notes, &active, (99, 100), 96), 0);
    }

    #[test]
    fn row_grids_group_sorted_rows_and_ignore_out_of_range_columns() {
        let notes = [
            test_note_at(NoteType::Tap, None, false, 48, 1.0),
            test_note_at(NoteType::Lift, None, false, 48, 1.0),
            test_note_at(NoteType::Tap, None, false, 96, 2.0),
            test_note_at(NoteType::Tap, None, false, 96, 2.0),
        ];
        let mut notes = notes;
        notes[0].column = 2;
        notes[1].column = 0;
        notes[2].column = 3;
        notes[3].column = 5;

        let rows = build_row_grids(&notes, (0, notes.len()), 0, 4);

        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].row_index, 48);
        assert_eq!(rows[0].note_indices[0], 1);
        assert_eq!(rows[0].note_indices[2], 0);
        assert_eq!(rows[1].row_index, 96);
        assert_eq!(rows[1].note_indices[3], 2);
        assert_eq!(rows[1].note_indices[0], usize::MAX);
    }

    #[test]
    fn player_rows_filter_by_column_range_and_sort_unique() {
        let mut notes = [
            test_note_at(NoteType::Tap, None, false, 96, 2.0),
            test_note_at(NoteType::Tap, None, false, 48, 1.0),
            test_note_at(NoteType::Tap, None, false, 48, 1.0),
            test_note_at(NoteType::Tap, None, false, 144, 3.0),
        ];
        notes[0].column = 5;
        notes[1].column = 2;
        notes[2].column = 3;
        notes[3].column = 9;

        assert_eq!(local_player_col(5, 2, 4), Some(3));
        assert_eq!(local_player_col(1, 2, 4), None);
        assert_eq!(player_rows(&notes, 2, 4), vec![48, 96]);

        sort_player_notes(&mut notes);
        let rows_and_cols: Vec<(usize, usize)> = notes
            .iter()
            .map(|note| (note.row_index, note.column))
            .collect();
        assert_eq!(rows_and_cols, vec![(48, 2), (48, 3), (96, 5), (144, 9)]);
    }

    #[test]
    fn column_field_helpers_resolve_player_and_local_column() {
        assert_eq!(player_index_for_column(1, 4, 7), 0);
        assert_eq!(player_index_for_column(2, 0, 7), 0);
        assert_eq!(player_index_for_column(2, 4, 0), 0);
        assert_eq!(player_index_for_column(2, 4, 3), 0);
        assert_eq!(player_index_for_column(2, 4, 4), 1);
        assert_eq!(player_index_for_column(2, 4, 9), 1);

        assert_eq!(local_column_for_field(0, 6), 6);
        assert_eq!(local_column_for_field(4, 6), 2);
    }

    #[test]
    fn simultaneous_limit_counts_active_holds_before_row_taps() {
        let mut hold = test_note_at(NoteType::Hold, Some(test_hold()), false, 0, 0.0);
        hold.column = 0;
        hold.hold
            .as_mut()
            .expect("hold has hold data")
            .end_row_index = 96;
        let mut tap1 = test_note_at(NoteType::Tap, None, false, 48, 1.0);
        tap1.column = 1;
        let mut tap2 = test_note_at(NoteType::Tap, None, false, 48, 1.0);
        tap2.column = 2;
        let mut notes = vec![hold, tap1, tap2];

        enforce_max_simultaneous_notes(&mut notes, 2, 0, 4);

        assert_eq!(notes.len(), 2);
        assert_eq!((notes[0].column, notes[0].row_index), (0, 0));
        assert_eq!((notes[1].column, notes[1].row_index), (2, 48));
    }

    #[test]
    fn row_track_helpers_count_taps_holds_and_first_tracks() {
        let mut notes = [
            test_note_at(NoteType::Tap, None, false, 48, 1.0),
            test_note_at(NoteType::Lift, None, false, 48, 1.0),
            test_note_at(NoteType::Tap, None, true, 48, 1.0),
            test_note_at(NoteType::Hold, Some(test_hold()), false, 24, 0.5),
        ];
        notes[0].column = 2;
        notes[1].column = 0;
        notes[2].column = 1;
        notes[3].column = 3;
        notes[3]
            .hold
            .as_mut()
            .expect("hold has hold data")
            .end_row_index = 96;

        assert_eq!(count_nonempty_tracks_at_row(&notes, 48, 0, 4), 3);
        assert_eq!(count_tap_or_hold_tracks_at_row(&notes, 48, 0, 4), 3);
        assert_eq!(count_tap_tracks_at_row(&notes, 48, 0, 4), 2);
        assert_eq!(first_nonempty_track_at_row(&notes, 48, 0, 4), Some(0));
        assert_eq!(first_tap_track_at_row(&notes, 48, 0, 4), Some(0));
        assert!(is_hold_body_at_row(&notes, 48, 3));
        assert_eq!(count_held_tracks_at_row(&notes, 48, 0, 4), 1);
        assert!(cell_has_any_note(&notes, 48, 1));
        assert!(!cell_has_nonfake_note(&notes, 48, 1));
        assert_eq!(stomp_mirror_track(1, 4), 2);
    }

    #[test]
    fn added_notes_replace_existing_cell_and_use_timing() {
        let timing = TimingData::from_segments(
            0.0,
            0.0,
            &TimingSegments::default(),
            &test_row_to_beat(ROWS_PER_BEAT as usize * 2),
        );
        let mut notes = vec![test_note_at(NoteType::Tap, None, false, 48, 1.0)];

        assert!(set_added_mine_note(&mut notes, &timing, 48, 0));
        assert_eq!(notes.len(), 1);
        assert_eq!(notes[0].note_type, NoteType::Mine);
        assert_eq!(notes[0].beat, 1.0);

        assert!(set_added_tap_note(&mut notes, &timing, 96, 1));
        assert!(cell_has_any_note(&notes, 96, 1));
        remove_cell_notes(&mut notes, 96, 1);
        assert!(!cell_has_any_note(&notes, 96, 1));
    }

    #[test]
    fn mines_insert_converts_every_sixth_nonempty_row() {
        let timing = TimingData::from_segments(
            0.0,
            0.0,
            &TimingSegments::default(),
            &test_row_to_beat(5 * ROWS_PER_BEAT as usize),
        );
        let mut notes = (0..6)
            .map(|i| {
                test_note_at(
                    NoteType::Tap,
                    None,
                    false,
                    i * ROWS_PER_BEAT as usize,
                    i as f32,
                )
            })
            .collect::<Vec<_>>();

        apply_mines_insert(
            &mut notes,
            &[],
            &timing,
            0,
            4,
            0,
            5 * ROWS_PER_BEAT as usize,
        );

        assert!(notes.iter().any(|note| {
            note.row_index == 5 * ROWS_PER_BEAT as usize && note.note_type == NoteType::Mine
        }));
    }

    #[test]
    fn mines_insert_adds_mine_half_beat_after_hold_end() {
        let timing = TimingData::from_segments(
            0.0,
            0.0,
            &TimingSegments::default(),
            &test_row_to_beat(3 * ROWS_PER_BEAT as usize),
        );
        let mut hold = test_note_at(NoteType::Hold, Some(test_hold()), false, 0, 0.0);
        hold.column = 1;
        hold.hold
            .as_mut()
            .expect("hold has hold data")
            .end_row_index = 2 * ROWS_PER_BEAT as usize;
        let mut notes = vec![hold];

        apply_mines_insert(
            &mut notes,
            &[],
            &timing,
            0,
            4,
            0,
            3 * ROWS_PER_BEAT as usize,
        );

        assert!(notes.iter().any(|note| {
            note.row_index == 2 * ROWS_PER_BEAT as usize + (ROWS_PER_BEAT as usize / 2)
                && note.column == 1
                && note.note_type == NoteType::Mine
        }));
    }

    #[test]
    fn intelligent_insert_adds_middle_tap_between_matching_endpoints() {
        let timing = TimingData::from_segments(
            0.0,
            0.0,
            &TimingSegments::default(),
            &test_row_to_beat(ROWS_PER_BEAT as usize * 2),
        );
        let mut notes = vec![
            test_note_at(NoteType::Tap, None, false, 0, 0.0),
            test_note_at(NoteType::Tap, None, false, ROWS_PER_BEAT as usize, 1.0),
        ];
        notes[0].column = 0;
        notes[1].column = 2;

        apply_insert_intelligent_taps(
            &mut notes,
            &timing,
            0,
            4,
            ROWS_PER_BEAT as usize,
            (ROWS_PER_BEAT / 2) as usize,
            ROWS_PER_BEAT as usize,
            false,
        );

        assert!(notes.iter().any(|note| {
            note.row_index == (ROWS_PER_BEAT / 2) as usize
                && note.column == 1
                && note.note_type == NoteType::Tap
        }));
    }

    #[test]
    fn wide_stomp_and_echo_insert_expected_taps() {
        let timing = TimingData::from_segments(
            0.0,
            0.0,
            &TimingSegments::default(),
            &test_row_to_beat(ROWS_PER_BEAT as usize * 4),
        );

        let mut wide = vec![test_note_at(NoteType::Tap, None, false, 0, 0.0)];
        wide[0].column = 1;
        apply_wide_insert(&mut wide, &timing, 0, 4);
        assert!(
            wide.iter()
                .any(|note| note.row_index == 0 && note.column != 1)
        );

        let mut stomp = vec![test_note_at(NoteType::Tap, None, false, 0, 0.0)];
        stomp[0].column = 1;
        apply_stomp_insert(&mut stomp, &timing, 0, 4);
        assert!(
            stomp
                .iter()
                .any(|note| note.row_index == 0 && note.column == 2)
        );

        let mut echo = vec![test_note_at(NoteType::Tap, None, false, 0, 0.0)];
        echo[0].column = 3;
        apply_echo_insert(&mut echo, &timing, 0, 4);
        assert!(
            echo.iter()
                .any(|note| { note.row_index == (ROWS_PER_BEAT / 2) as usize && note.column == 3 })
        );
    }

    #[test]
    fn convert_taps_to_holds_sets_hold_metadata() {
        let timing = TimingData::from_segments(
            0.0,
            0.0,
            &TimingSegments::default(),
            &test_row_to_beat(ROWS_PER_BEAT as usize * 2),
        );
        let mut notes = vec![test_note_at(NoteType::Tap, None, false, 0, 0.0)];
        notes[0].column = 0;

        convert_taps_to_holds(&mut notes, &timing, 0, 4, 1);

        assert_eq!(notes[0].note_type, NoteType::Hold);
        let hold = notes[0].hold.as_ref().expect("tap converted to hold");
        assert_eq!(hold.end_row_index, ROWS_PER_BEAT as usize);
        assert_eq!(hold.life, INITIAL_HOLD_LIFE);
        assert_eq!(hold.last_held_row_index, 0);
    }

    #[test]
    fn uncommon_remove_masks_filter_convert_and_cap_notes() {
        let timing = test_timing(ROWS_PER_BEAT as usize * 5);
        let mut notes = vec![
            test_note_at(NoteType::Tap, None, false, 0, 0.0),
            test_note_at(NoteType::Tap, None, false, 0, 0.0),
            test_note_at(NoteType::Tap, None, false, 0, 0.0),
            test_note_at(NoteType::Tap, None, false, ROWS_PER_BEAT as usize / 2, 0.5),
            test_note_at(NoteType::Mine, None, false, ROWS_PER_BEAT as usize, 1.0),
            test_note_at(NoteType::Tap, None, true, ROWS_PER_BEAT as usize * 2, 2.0),
            test_note_at(NoteType::Lift, None, false, ROWS_PER_BEAT as usize * 3, 3.0),
            test_note_at(
                NoteType::Hold,
                Some(test_hold()),
                false,
                ROWS_PER_BEAT as usize * 4,
                4.0,
            ),
        ];
        for (column, note) in notes.iter_mut().enumerate() {
            note.column = column % 4;
        }

        apply_uncommon_masks_with_masks(
            &mut notes,
            0,
            REMOVE_MASK_BIT_LITTLE
                | REMOVE_MASK_BIT_NO_MINES
                | REMOVE_MASK_BIT_NO_HOLDS
                | REMOVE_MASK_BIT_NO_HANDS
                | REMOVE_MASK_BIT_NO_LIFTS
                | REMOVE_MASK_BIT_NO_FAKES,
            HOLDS_MASK_BIT_NO_ROLLS,
            &timing,
            0,
            4,
            &[],
            None,
            0,
        );

        assert!(
            notes
                .iter()
                .all(|note| note.row_index % ROWS_PER_BEAT as usize == 0)
        );
        assert!(notes.iter().all(|note| {
            !note.is_fake
                && note.note_type != NoteType::Mine
                && note.note_type != NoteType::Lift
                && note.note_type != NoteType::Hold
                && note.hold.is_none()
        }));
        assert!(count_tap_tracks_at_row(&notes, 0, 0, 4) <= 2);
    }

    #[test]
    fn uncommon_insert_and_hold_masks_delegate_to_transforms() {
        let timing = test_timing(ROWS_PER_BEAT as usize * 3);
        let mut notes = vec![
            test_note_at(NoteType::Tap, None, false, 0, 0.0),
            test_note_at(NoteType::Tap, None, false, ROWS_PER_BEAT as usize, 1.0),
        ];
        notes[0].column = 0;
        notes[1].column = 2;

        apply_uncommon_masks_with_masks(
            &mut notes,
            INSERT_MASK_BIT_BIG,
            0,
            HOLDS_MASK_BIT_PLANTED,
            &timing,
            0,
            4,
            &[],
            None,
            0,
        );

        let inserted = notes
            .iter()
            .find(|note| {
                note.row_index == ROWS_PER_BEAT as usize / 2
                    && note.column == 1
                    && note.note_type == NoteType::Hold
            })
            .expect("big insert tap converted to hold");
        assert_eq!(
            inserted
                .hold
                .as_ref()
                .expect("inserted note converted to hold")
                .life,
            INITIAL_HOLD_LIFE
        );
    }

    #[test]
    fn uncommon_chart_transforms_preserve_ranges_without_masks() {
        let timing = test_timing(ROWS_PER_BEAT as usize);
        let timing_refs: [&TimingData; MAX_PLAYERS] = std::array::from_fn(|_| &timing);
        let mut notes = vec![
            test_note_at(NoteType::Tap, None, false, 0, 0.0),
            test_note_at(NoteType::Mine, None, false, 0, 0.0),
        ];
        notes[0].column = 0;
        notes[1].column = 4;
        let mut ranges = [(0, 1), (1, 2)];

        apply_uncommon_chart_transforms(
            &mut notes,
            &mut ranges,
            4,
            2,
            &[ChartAttackEffects::default(); MAX_PLAYERS],
            &timing_refs,
        );

        assert_eq!(notes.len(), 2);
        assert_eq!(notes[0].note_type, NoteType::Tap);
        assert_eq!(notes[0].column, 0);
        assert_eq!(notes[1].note_type, NoteType::Mine);
        assert_eq!(notes[1].column, 4);
        assert_eq!(ranges[0], (0, 1));
        assert_eq!(ranges[1], (1, 2));
    }

    #[test]
    fn uncommon_chart_transforms_rebuild_per_player_ranges() {
        let timing = test_timing(ROWS_PER_BEAT as usize);
        let timing_refs: [&TimingData; MAX_PLAYERS] = std::array::from_fn(|_| &timing);
        let mut notes = vec![
            test_note_at(NoteType::Tap, None, false, 0, 0.0),
            test_note_at(NoteType::Mine, None, false, 0, 0.0),
        ];
        notes[0].column = 0;
        notes[1].column = 4;
        let mut ranges = [(0, 1), (1, 2)];
        let mut effects = [ChartAttackEffects::default(); MAX_PLAYERS];
        effects[1].remove_mask = REMOVE_MASK_BIT_NO_MINES;

        apply_uncommon_chart_transforms(&mut notes, &mut ranges, 4, 2, &effects, &timing_refs);

        assert_eq!(notes.len(), 1);
        assert_eq!(notes[0].column, 0);
        assert_eq!(ranges[0], (0, 1));
        assert_eq!(ranges[1], (1, 1));
    }

    #[test]
    fn uncommon_chart_transforms_duplicate_single_player_range() {
        let timing = test_timing(ROWS_PER_BEAT as usize);
        let timing_refs: [&TimingData; MAX_PLAYERS] = std::array::from_fn(|_| &timing);
        let mut notes = vec![
            test_note_at(NoteType::Tap, None, false, 0, 0.0),
            test_note_at(NoteType::Mine, None, false, 0, 0.0),
        ];
        notes[0].column = 0;
        notes[1].column = 1;
        let mut ranges = [(0, 2), (0, 0)];
        let mut effects = [ChartAttackEffects::default(); MAX_PLAYERS];
        effects[0].remove_mask = REMOVE_MASK_BIT_NO_MINES;

        apply_uncommon_chart_transforms(&mut notes, &mut ranges, 4, 1, &effects, &timing_refs);

        assert_eq!(notes.len(), 1);
        assert_eq!(notes[0].note_type, NoteType::Tap);
        assert_eq!(ranges[0], (0, 1));
        assert_eq!(ranges[1], (0, 1));
    }

    #[test]
    fn notes_row_sorted_allows_equal_rows_only_in_order() {
        let sorted = [
            test_note_at(NoteType::Tap, None, false, 48, 1.0),
            test_note_at(NoteType::Tap, None, false, 48, 1.0),
            test_note_at(NoteType::Tap, None, false, 96, 2.0),
        ];
        let unsorted = [
            test_note_at(NoteType::Tap, None, false, 96, 2.0),
            test_note_at(NoteType::Tap, None, false, 48, 1.0),
        ];

        assert!(notes_row_sorted(&sorted));
        assert!(!notes_row_sorted(&unsorted));
    }

    #[test]
    fn turn_options_mirror_only_player_range_columns() {
        let mut notes = [
            test_note_at(NoteType::Tap, None, false, 48, 1.0),
            test_note_at(NoteType::Tap, None, false, 48, 1.0),
            test_note_at(NoteType::Tap, None, false, 48, 1.0),
            test_note_at(NoteType::Tap, None, false, 48, 1.0),
            test_note_at(NoteType::Tap, None, false, 48, 1.0),
            test_note_at(NoteType::Tap, None, false, 48, 1.0),
            test_note_at(NoteType::Tap, None, false, 48, 1.0),
            test_note_at(NoteType::Tap, None, false, 48, 1.0),
        ];
        for (col, note) in notes.iter_mut().enumerate() {
            note.column = col;
        }

        let turns = [GameplayTurnOption::Mirror, GameplayTurnOption::None];
        apply_turn_options(&mut notes, [(0, 4), (4, 8)], 4, 2, turns, 123);

        let columns: Vec<usize> = notes.iter().map(|note| note.column).collect();
        assert_eq!(columns, vec![3, 2, 1, 0, 4, 5, 6, 7]);
    }

    #[test]
    fn turn_options_left_maps_four_panel_columns() {
        let mut notes = [
            test_note_at(NoteType::Tap, None, false, 48, 1.0),
            test_note_at(NoteType::Tap, None, false, 48, 1.0),
            test_note_at(NoteType::Tap, None, false, 48, 1.0),
            test_note_at(NoteType::Tap, None, false, 48, 1.0),
        ];
        for (col, note) in notes.iter_mut().enumerate() {
            note.column = col;
        }

        let turns = [GameplayTurnOption::Left, GameplayTurnOption::None];
        let notes_len = notes.len();
        apply_turn_options(&mut notes, [(0, notes_len), (0, 0)], 4, 1, turns, 123);

        let columns: Vec<usize> = notes.iter().map(|note| note.column).collect();
        assert_eq!(columns, vec![1, 3, 0, 2]);
    }

    #[test]
    fn turn_seed_uses_simfile_path() {
        let mut first = test_song(0.0, 0.0);
        first.simfile_path = PathBuf::from("packs/a/song.ssc");
        let mut same = test_song(0.0, 0.0);
        same.simfile_path = PathBuf::from("packs/a/song.ssc");
        let mut other = test_song(0.0, 0.0);
        other.simfile_path = PathBuf::from("packs/b/song.ssc");

        assert_eq!(turn_seed_for_song(&first), turn_seed_for_song(&same));
        assert_ne!(turn_seed_for_song(&first), turn_seed_for_song(&other));
    }

    #[test]
    fn note_and_mine_window_bounds_use_left_open_right_closed_time() {
        let times = [
            song_time_ns_from_seconds(1.0),
            song_time_ns_from_seconds(1.5),
            song_time_ns_from_seconds(2.0),
            song_time_ns_from_seconds(2.5),
        ];
        let note_indices = [0, 1, 2, 3];
        let notes = [
            test_note_at(NoteType::Tap, None, false, 48, 1.0),
            test_note_at(NoteType::Tap, None, false, 96, 2.0),
            test_note_at(NoteType::Tap, None, false, 144, 3.0),
            test_note_at(NoteType::Tap, None, false, 192, 4.0),
        ];

        assert_eq!(mine_window_bounds_ns(&times, times[0], times[2]), (0, 3));
        assert_eq!(crossed_mine_bounds_ns(&times, times[0], times[2]), (1, 3));
        assert_eq!(
            lane_note_window_bounds_ns(&note_indices, &times, times[0], times[2]),
            (0, 3)
        );
        assert_eq!(
            lane_note_window_bounds_rows(&note_indices, &notes, 96, 192),
            (1, 3)
        );
    }

    #[test]
    fn crossed_held_mine_predicate_accepts_judgable_same_column_mine() {
        let mut note = test_note_at(NoteType::Mine, None, false, 48, 1.0);
        note.column = 1;

        assert!(crossed_held_mine_can_hit(&note, 1));
    }

    #[test]
    fn crossed_held_mine_predicate_filters_noneligible_mines() {
        let mut tap = test_note_at(NoteType::Tap, None, false, 48, 1.0);
        tap.column = 1;
        let mut fake_mine = test_note_at(NoteType::Mine, None, true, 48, 1.0);
        fake_mine.column = 1;
        let mut wrong_column = test_note_at(NoteType::Mine, None, false, 48, 1.0);
        wrong_column.column = 2;
        let mut already_scored = test_note_at(NoteType::Mine, None, false, 48, 1.0);
        already_scored.column = 1;
        already_scored.mine_result = Some(MineResult::Avoided);
        let mut unjudgable = test_note_at(NoteType::Mine, None, false, 48, 1.0);
        unjudgable.column = 1;
        unjudgable.can_be_judged = false;

        assert!(!crossed_held_mine_can_hit(&tap, 1));
        assert!(!crossed_held_mine_can_hit(&fake_mine, 1));
        assert!(!crossed_held_mine_can_hit(&wrong_column, 1));
        assert!(!crossed_held_mine_can_hit(&already_scored, 1));
        assert!(!crossed_held_mine_can_hit(&unjudgable, 1));
    }

    #[test]
    fn step_search_bounds_expand_one_second_plus_one_beat() {
        let timing = test_timing(144);
        assert_eq!(
            step_search_row_bounds(&timing, song_time_ns_from_seconds(1.0), 48),
            (0, 144)
        );
    }

    #[test]
    fn step_search_bounds_saturate_before_song_start() {
        let timing = test_timing(144);
        assert_eq!(
            step_search_row_bounds(&timing, song_time_ns_from_seconds(0.0), 0),
            (0, 96)
        );
    }

    #[test]
    fn crossed_mine_held_start_tracks_existing_or_new_hold() {
        let previous = song_time_ns_from_seconds(1.0);
        let pressed_before = song_time_ns_from_seconds(0.9);
        let pressed_after = song_time_ns_from_seconds(1.25);
        let current = song_time_ns_from_seconds(1.5);

        assert_eq!(
            crossed_mine_held_start_time(true, true, None, previous, current),
            Some(previous)
        );
        assert_eq!(
            crossed_mine_held_start_time(true, false, Some(pressed_before), previous, current),
            Some(previous)
        );
        assert_eq!(
            crossed_mine_held_start_time(true, false, Some(pressed_after), previous, current),
            Some(pressed_after)
        );
        assert_eq!(
            crossed_mine_held_start_time(false, true, Some(previous), previous, current),
            None
        );
        assert_eq!(
            crossed_mine_held_start_time(
                true,
                false,
                Some(INVALID_SONG_TIME_NS),
                previous,
                current,
            ),
            None
        );
    }

    #[test]
    fn edge_judge_indices_use_lead_note_only() {
        assert_eq!(collect_edge_judge_indices(0, 7), None);

        let (indices, count) = collect_edge_judge_indices(3, 7).expect("row has notes");
        assert_eq!(count, 1);
        assert_eq!(indices[0], 7);
        assert!(indices[1..].iter().all(|index| *index == usize::MAX));
    }

    #[test]
    fn quantization_index_matches_note_row_subdivision() {
        assert_eq!(quantization_index_from_beat(0.0), QUANT_4TH);
        assert_eq!(quantization_index_from_beat(0.5), QUANT_8TH);
        assert_eq!(quantization_index_from_beat(1.0 / 3.0), QUANT_12TH);
        assert_eq!(quantization_index_from_beat(0.25), QUANT_16TH);
        assert_eq!(quantization_index_from_beat(1.0 / 6.0), QUANT_24TH);
        assert_eq!(quantization_index_from_beat(0.125), QUANT_32ND);
        assert_eq!(quantization_index_from_beat(1.0 / 12.0), QUANT_48TH);
        assert_eq!(quantization_index_from_beat(1.0 / 16.0), QUANT_64TH);
        assert_eq!(quantization_index_from_beat(1.0 / 48.0), QUANT_192ND);
    }

    #[test]
    fn closest_note_breaks_ties_toward_future_note() {
        let timing = test_timing(144);
        let notes = vec![
            test_note_at(NoteType::Tap, None, false, 48, 1.0),
            test_note_at(NoteType::Tap, None, false, 50, 50.0 / ROWS_PER_BEAT as f32),
        ];
        let note_indices = [0usize, 1];
        let note_times_ns = [1_000_000_000_i64, 1_020_000_000_i64];
        let (note_index, err_ns) = closest_lane_note_ns(
            &note_indices,
            &notes,
            &note_times_ns,
            &timing,
            1_010_000_000_i64,
            49,
            0,
            note_indices.len(),
        )
        .expect("expected an equidistant closest note");

        assert_eq!(note_index, 1);
        assert_eq!(err_ns, -10_000_000);
    }

    #[test]
    fn closest_note_prefers_row_distance_over_time_error() {
        let timing = test_timing(144);
        let notes = vec![
            test_note_at(NoteType::Tap, None, false, 48, 1.0),
            test_note_at(NoteType::Tap, None, false, 60, 60.0 / ROWS_PER_BEAT as f32),
        ];
        let note_indices = [0usize, 1];
        let note_times_ns = [
            song_time_ns_from_seconds(1.020),
            song_time_ns_from_seconds(1.028),
        ];
        let current_time_ns = song_time_ns_from_seconds(1.030);
        let (note_index, err_ns) = closest_lane_note_ns(
            &note_indices,
            &notes,
            &note_times_ns,
            &timing,
            current_time_ns,
            50,
            0,
            note_indices.len(),
        )
        .expect("expected the nearer row to win");

        assert_eq!(note_index, 0);
        assert_eq!(err_ns, current_time_ns - note_times_ns[note_index]);
    }

    #[test]
    fn closest_note_skips_fake_segment_taps_and_judged_mines() {
        let timing = TimingData::from_segments(
            0.0,
            0.0,
            &TimingSegments {
                fakes: vec![FakeSegment {
                    beat: 1.0,
                    length: 0.01,
                }],
                ..TimingSegments::default()
            },
            &test_row_to_beat(144),
        );
        let mut fake_segment_tap = test_note_at(NoteType::Tap, None, true, 48, 1.0);
        fake_segment_tap.can_be_judged = false;
        let mut judged_mine = test_note_at(NoteType::Mine, None, false, 49, 49.0 / 48.0);
        judged_mine.mine_result = Some(MineResult::Hit);
        let notes = vec![
            fake_segment_tap,
            judged_mine,
            test_note_at(NoteType::Tap, None, false, 60, 60.0 / 48.0),
        ];
        let note_indices = [0usize, 1, 2];
        let note_times_ns = [
            song_time_ns_from_seconds(1.000),
            song_time_ns_from_seconds(1.010),
            song_time_ns_from_seconds(1.120),
        ];

        let (note_index, _) = closest_lane_note_ns(
            &note_indices,
            &notes,
            &note_times_ns,
            &timing,
            song_time_ns_from_seconds(1.030),
            50,
            0,
            note_indices.len(),
        )
        .expect("expected the unjudged tap to remain hittable");

        assert_eq!(note_index, 2);
    }
}
