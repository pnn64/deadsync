use deadsync_chart::{SongData, SyncPref};
use deadsync_core::input::{InputSource, MAX_COLS, MAX_PLAYERS};
use deadsync_core::note::NoteType;
use deadsync_core::song_time::{
    SongTimeNs, clamp_song_time_ns, scaled_song_time_ns, song_time_ns_add_seconds,
    song_time_ns_from_seconds, song_time_ns_invalid, song_time_ns_to_seconds,
};
use deadsync_core::timing::{ROWS_PER_BEAT, beat_to_note_row};
use deadsync_rules::judgment::{self, JudgeGrade, Judgment, TimingWindow};
use deadsync_rules::note::{
    HoldData, HoldResult, MineResult, Note, TIMING_WINDOW_SECONDS_HOLD, TIMING_WINDOW_SECONDS_ROLL,
};
use deadsync_rules::timing::{TimingData, TimingProfile, TimingProfileNs};
use std::collections::VecDeque;
use std::hash::Hasher;
use std::path::PathBuf;
use std::time::Instant;
use twox_hash::XxHash64;

// ITGmania ScreenGameplay MinSecondsToStep/MinSecondsToMusic defaults.
const MIN_SECONDS_TO_STEP: f32 = 6.0;
const MIN_SECONDS_TO_MUSIC: f32 = 2.0;
// Simply Love: ScreenGameplay GiveUpSeconds=0.33.
pub const GIVE_UP_HOLD_SECONDS: f32 = 0.33;
// Mirrors ScreenGameplay::AbortGiveUpText tween duration (1/2 second).
pub const GIVE_UP_ABORT_TEXT_SECONDS: f32 = 0.5;
pub const BACK_OUT_HOLD_SECONDS: f32 = 1.0;
// Simply Love: ScreenGameplay out.lua (sleep 0.5, linear 1.0).
const GIVE_UP_OUT_FADE_DELAY_SECONDS: f32 = 0.5;
const GIVE_UP_OUT_FADE_SECONDS: f32 = 1.0;
// Simply Love: _fade out normal.lua (sleep 0.1, linear 0.4).
const BACK_OUT_FADE_DELAY_SECONDS: f32 = 0.1;
const BACK_OUT_FADE_SECONDS: f32 = 0.4;
pub const GAMEPLAY_INPUT_BACKLOG_WARN: usize = 128;
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
pub fn active_hold_is_engaged(active: &ActiveHold) -> bool {
    !active.let_go && active.life > 0.0
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
) -> f32 {
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
        return offset_s;
    }
    if count > 1
        && (count & 1) == 1
        && let Some(oldest) = oldest_in_window
    {
        sum -= oldest;
        count -= 1;
    }
    let mut avg = sum / (count.max(1) as f32);
    if count == 1 {
        avg *= 0.75;
    }
    avg
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
        let broad_avg = error_bar_average_offset_s(&mut broad, 0.5, 0.050, 400);
        assert!((broad_avg - 0.040).abs() <= 1e-6);

        let mut narrow = VecDeque::from([(0.0, 0.010), (100.0, 0.020), (200.0, 0.030)]);
        let narrow_avg = error_bar_average_offset_s(&mut narrow, 0.5, 0.050, 200);
        assert!((narrow_avg - 0.0375).abs() <= 1e-6);
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
