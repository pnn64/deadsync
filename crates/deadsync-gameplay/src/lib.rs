use deadsync_chart::SyncPref;
use deadsync_core::input::{InputSource, MAX_PLAYERS};
use deadsync_core::note::NoteType;
use deadsync_core::song_time::SongTimeNs;
use deadsync_rules::judgment::{JudgeGrade, Judgment, TimingWindow};
use deadsync_rules::note::{HoldResult, MineResult};
use std::time::Instant;

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
    pub start_time_ns: SongTimeNs,
    pub end_time_ns: SongTimeNs,
    pub note_type: NoteType,
    pub let_go: bool,
    pub is_pressed: bool,
    pub life: f32,
    pub last_update_time_ns: SongTimeNs,
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
}
