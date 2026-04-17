use crate::engine::audio;
use crate::engine::input::{InputEdge, InputSource};
use crate::engine::present::color;
use crate::engine::space::{is_wide, screen_height, screen_width};
use crate::game::chart::{ChartData, GameplayChartData};
use crate::game::judgment::{
    self, JudgeGrade, Judgment, TimingWindow, judgment_time_error_ms_from_music_ns,
};
use crate::game::note::{HoldData, HoldResult, MineResult, Note, NoteType};
use crate::game::parsing::noteskin::{self, ModelMeshCache, Noteskin, Style};
use crate::game::parsing::song_lua::SongLuaOverlayActor;
use crate::game::scores;
use crate::game::song::SongData;
use crate::game::timing::{
    BeatInfoCache, ROWS_PER_BEAT, TimingData, TimingProfile, TimingProfileNs,
};
use crate::game::{
    profile::{self, TimingTickMode as TickMode},
    scroll::ScrollSpeedSetting,
};
use log::{debug, info, trace, warn};
use rssp::streams::StreamSegment;
use std::cell::RefCell;
use std::collections::VecDeque;
use std::hash::Hasher;
use std::path::PathBuf;
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};
use twox_hash::XxHash64;

#[path = "gameplay/attacks.rs"]
mod attacks;
#[path = "gameplay/autoplay.rs"]
mod autoplay;
#[path = "gameplay/autosync.rs"]
mod autosync;
#[path = "gameplay/clock.rs"]
mod clock;
#[path = "gameplay/controls.rs"]
mod controls;
#[path = "gameplay/display.rs"]
mod display;
#[path = "gameplay/holds.rs"]
mod holds;
#[path = "gameplay/input.rs"]
mod input;
#[path = "gameplay/judging.rs"]
mod judging;
#[path = "gameplay/life.rs"]
mod life;
#[path = "gameplay/note_result.rs"]
mod note_result;
#[path = "gameplay/offset.rs"]
mod offset;
#[path = "gameplay/rows.rs"]
mod rows;
#[path = "gameplay/stats.rs"]
mod stats;
#[path = "gameplay/time.rs"]
mod time;

pub(crate) use self::attacks::song_lua_ease_factor;
#[cfg(test)]
use self::attacks::song_lua_ease_window_value;
use self::attacks::{
    AttackMaskWindow, SongLuaEaseMaskWindow, apply_chart_attacks_transforms,
    base_appearance_effects, build_attack_mask_windows_for_player, build_song_lua_runtime_windows,
    effective_visual_mask_for_player, player_changes_chart, refresh_active_attack_masks,
};
pub use self::attacks::{
    SongLuaOverlayEaseWindowRuntime, SongLuaOverlayMessageRuntime,
    active_chart_attack_effects_for_player, effective_accel_effects_for_player,
    effective_appearance_effects_for_player, effective_mini_percent_for_player,
    effective_perspective_effects_for_player, effective_scroll_effects_for_player,
    effective_scroll_speed_for_player, effective_visibility_effects_for_player,
    effective_visual_effects_for_player,
};
#[cfg(test)]
use self::attacks::{
    build_song_lua_ease_windows_for_player, build_song_lua_overlay_ease_windows, parse_attack_mods,
    parse_song_lua_runtime_mods, turn_option_bits,
};
#[cfg(test)]
use self::autoplay::live_autoplay_enabled_from_flags;
use self::autoplay::{autoplay_blocks_scoring, live_autoplay_enabled, run_autoplay, run_replay};
use self::autosync::apply_autosync_for_row_hits;
pub use self::clock::{
    DisplayClockDiagEvent, DisplayClockDiagEventKind, DisplayClockHealth,
    collect_display_clock_stutter_diag_events, display_clock_health,
    display_clock_stutter_diag_trigger_seq,
};
use self::clock::{
    DisplayClockDiagRing, FrameStableDisplayClock, SongClockSnapshot, current_song_clock_snapshot,
    frame_stable_display_music_time_ns, music_time_ns_from_song_clock,
};
pub use self::controls::{
    RawKeyAction, autosync_mode_status_line, handle_queued_raw_key, timing_tick_status_line,
};
#[cfg(test)]
use self::controls::{next_tick_mode, tick_mode_status_line};
#[cfg(test)]
use self::display::effective_ex_score_inputs;
#[cfg(test)]
use self::display::scored_hold_totals_with_carry;
use self::display::{capture_failed_ex_score_inputs, record_display_window_counts};
pub use self::display::{
    display_carry_for_player, display_ex_score_percent, display_hard_ex_score_percent,
    display_itg_score_percent, display_judgment_count, display_totals_for_player,
    display_window_counts,
};
pub(crate) use self::display::{display_ex_score_data, display_scored_ex_score_data};
#[cfg(test)]
use self::holds::{HoldLifeAdvance, advance_hold_last_held, advance_hold_life_ns};
use self::holds::{begin_hold_life_decay, start_active_hold, update_active_holds};
use self::holds::{
    handle_hold_let_go, handle_hold_success, integrate_active_hold_to_time,
    refresh_roll_life_on_step,
};
#[cfg(test)]
use self::input::{
    active_hold_counts_as_pressed, lane_edge_judges_lift, lane_edge_judges_tap, lane_press_started,
    lane_release_finished, update_lane_count,
};
pub use self::input::{
    handle_input, queue_input_edge, receptor_glow_visual_for_col, replay_capture_enabled,
    set_replay_capture_enabled,
};
use self::input::{
    input_queue_cap, lane_is_pressed, process_input_edges, replay_edge_cap,
    sync_active_hold_pressed_state, tap_explosion_noteskin_for_player, tick_visual_effects,
    trigger_receptor_glow_pulse,
};
use self::judging::{
    PlayerJudgmentTiming, build_final_note_hit_judgment, build_player_judgment_timing,
    effective_player_global_offset_seconds, note_hit_eval, player_largest_tap_window_ns,
};
pub use self::judging::{player_blue_window_ms, player_fa_plus_window_s};
use self::life::{all_joined_players_failed, apply_life_change, is_player_dead, is_state_dead};
#[cfg(test)]
use self::note_result::{add_provisional_early_score, remove_provisional_early_score};
use self::note_result::{register_provisional_early_result, set_final_note_result};
use self::offset::update_offset_adjust_hold;
#[cfg(test)]
use self::offset::{
    apply_global_offset_delta, apply_song_offset_delta, mutate_timing_arc,
    refresh_timing_after_offset_change,
};
use self::rows::update_judged_rows;
#[cfg(test)]
use self::rows::{
    advance_judged_row_cursor, finalize_row_judgment, next_ready_row_in_lookahead,
    player_row_scan_state, suppress_final_bad_rescore_visual,
};
pub use self::stats::{
    CourseDisplayTotals, course_display_carry_from_state, course_display_totals_for_chart,
    score_invalid_reason_lines_for_chart, stream_segments_for_results,
};
use self::stats::{
    compute_possible_grade_points, mini_indicator_mode, needs_stream_data, recompute_player_totals,
    stream_sequences_threshold, target_score_setting_percent, zmod_stream_totals_full_measures,
};
use self::time::{
    INVALID_SONG_TIME_NS, clamp_song_time_ns, current_music_time_s, normalized_song_rate,
    scaled_song_delta_ns, scaled_song_time_ns, song_time_ns_add_seconds,
    song_time_ns_delta_seconds, song_time_ns_span_seconds,
};
pub(crate) use self::time::{
    song_time_ns_from_seconds, song_time_ns_invalid, song_time_ns_to_seconds,
};

// Simply Love ScreenGameplay in/default.lua keeps intro cover actors alive for 2.0s.
pub const TRANSITION_IN_DURATION: f32 = 2.0;
// Simply Love ScreenGameplay out.lua: sleep(0.5), linear(1.0).
pub const TRANSITION_OUT_DELAY: f32 = 0.5;
pub const TRANSITION_OUT_FADE_DURATION: f32 = 1.0;
pub const TRANSITION_OUT_DURATION: f32 = TRANSITION_OUT_DELAY + TRANSITION_OUT_FADE_DURATION;
pub const MAX_COLS: usize = 8;
pub const MAX_PLAYERS: usize = 2;
// Match Simply Love ITG / FA+: repeated negative life events add a 5-hit lock
// back up to a 10-hit ceiling before life can regenerate again.
const REGEN_COMBO_AFTER_MISS: u32 = 5;
const MAX_REGEN_COMBO_AFTER_MISS: u32 = 10;
// Simply Love enables HarshHotLifePenalty, so negative events from a full bar
// should cost at least 10% life.
const HOT_LIFE_MIN_NEGATIVE_DELTA: f32 = -0.10;
// ITGmania _fallback and Simply Love keep mine hits from incrementing miss combo.
const MINE_HIT_INCREMENTS_MISS_COMBO: bool = false;
// In SM, life regeneration is tied to LifePercentChangeHeld. Simply Love sets
// TimingWindowSecondsHold to 0.32s, so mirror that grace window. Reference:
// itgmania/Themes/Simply Love/Scripts/SL_Init.lua
const LIFE_FANTASTIC: f32 = 0.008;
const LIFE_EXCELLENT: f32 = 0.008;
const LIFE_GREAT: f32 = 0.004;
const LIFE_DECENT: f32 = 0.0;
const LIFE_WAY_OFF: f32 = -0.050;
const LIFE_MISS: f32 = -0.100;
const LIFE_HIT_MINE: f32 = -0.050;
const LIFE_HELD: f32 = 0.008;
const LIFE_LET_GO: f32 = -0.080;
const OFFSET_ADJUST_STEP_SECONDS: f32 = 0.001;
const OFFSET_ADJUST_REPEAT_DELAY: Duration = Duration::from_millis(300);
const OFFSET_ADJUST_REPEAT_INTERVAL: Duration = Duration::from_millis(50);
const INSERT_MASK_BIT_WIDE: u8 = 1u8 << 0;
const INSERT_MASK_BIT_BIG: u8 = 1u8 << 1;
const INSERT_MASK_BIT_QUICK: u8 = 1u8 << 2;
const INSERT_MASK_BIT_BMRIZE: u8 = 1u8 << 3;
const INSERT_MASK_BIT_SKIPPY: u8 = 1u8 << 4;
const INSERT_MASK_BIT_ECHO: u8 = 1u8 << 5;
const INSERT_MASK_BIT_STOMP: u8 = 1u8 << 6;
const INSERT_MASK_BIT_MINES: u8 = 1u8 << 7;
const REMOVE_MASK_BIT_LITTLE: u8 = 1u8 << 0;
const REMOVE_MASK_BIT_NO_MINES: u8 = 1u8 << 1;
const REMOVE_MASK_BIT_NO_HOLDS: u8 = 1u8 << 2;
const REMOVE_MASK_BIT_NO_JUMPS: u8 = 1u8 << 3;
const REMOVE_MASK_BIT_NO_HANDS: u8 = 1u8 << 4;
const REMOVE_MASK_BIT_NO_QUADS: u8 = 1u8 << 5;
const REMOVE_MASK_BIT_NO_LIFTS: u8 = 1u8 << 6;
const REMOVE_MASK_BIT_NO_FAKES: u8 = 1u8 << 7;
const HOLDS_MASK_BIT_PLANTED: u8 = 1u8 << 0;
const HOLDS_MASK_BIT_FLOORED: u8 = 1u8 << 1;
const HOLDS_MASK_BIT_TWISTER: u8 = 1u8 << 2;
const HOLDS_MASK_BIT_NO_ROLLS: u8 = 1u8 << 3;
const HOLDS_MASK_BIT_HOLDS_TO_ROLLS: u8 = 1u8 << 4;
const ACCEL_MASK_BIT_BOOST: u8 = 1u8 << 0;
const ACCEL_MASK_BIT_BRAKE: u8 = 1u8 << 1;
const ACCEL_MASK_BIT_WAVE: u8 = 1u8 << 2;
const ACCEL_MASK_BIT_EXPAND: u8 = 1u8 << 3;
const ACCEL_MASK_BIT_BOOMERANG: u8 = 1u8 << 4;
const VISUAL_MASK_BIT_DRUNK: u16 = 1u16 << 0;
const VISUAL_MASK_BIT_DIZZY: u16 = 1u16 << 1;
const VISUAL_MASK_BIT_CONFUSION: u16 = 1u16 << 2;
const VISUAL_MASK_BIT_BIG: u16 = 1u16 << 3;
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
pub struct AccelEffects {
    pub boost: f32,
    pub brake: f32,
    pub wave: f32,
    pub expand: f32,
    pub boomerang: f32,
}

impl AccelEffects {
    #[inline(always)]
    fn from_mask(mask: u8) -> Self {
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
    pub big: f32,
    pub flip: f32,
    pub invert: f32,
    pub tornado: f32,
    pub tipsy: f32,
    pub bumpy: f32,
    pub beat: f32,
}

impl VisualEffects {
    #[inline(always)]
    fn from_mask(mask: u16) -> Self {
        Self {
            drunk: f32::from((mask & VISUAL_MASK_BIT_DRUNK) != 0),
            dizzy: f32::from((mask & VISUAL_MASK_BIT_DIZZY) != 0),
            confusion: f32::from((mask & VISUAL_MASK_BIT_CONFUSION) != 0),
            big: f32::from((mask & VISUAL_MASK_BIT_BIG) != 0),
            flip: f32::from((mask & VISUAL_MASK_BIT_FLIP) != 0),
            invert: f32::from((mask & VISUAL_MASK_BIT_INVERT) != 0),
            tornado: f32::from((mask & VISUAL_MASK_BIT_TORNADO) != 0),
            tipsy: f32::from((mask & VISUAL_MASK_BIT_TIPSY) != 0),
            bumpy: f32::from((mask & VISUAL_MASK_BIT_BUMPY) != 0),
            beat: f32::from((mask & VISUAL_MASK_BIT_BEAT) != 0),
        }
    }

    #[inline(always)]
    fn to_mask(self) -> u16 {
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
        if self.bumpy > f32::EPSILON {
            mask |= VISUAL_MASK_BIT_BUMPY;
        }
        if self.beat > f32::EPSILON {
            mask |= VISUAL_MASK_BIT_BEAT;
        }
        mask
    }
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
    fn from_mask(mask: u8) -> Self {
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
    fn approach_speeds() -> Self {
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
    fn from_option(scroll: profile::ScrollOption) -> Self {
        Self {
            reverse: f32::from(scroll.contains(profile::ScrollOption::Reverse)),
            split: f32::from(scroll.contains(profile::ScrollOption::Split)),
            alternate: f32::from(scroll.contains(profile::ScrollOption::Alternate)),
            cross: f32::from(scroll.contains(profile::ScrollOption::Cross)),
            centered: f32::from(scroll.contains(profile::ScrollOption::Centered)),
        }
    }

    #[inline(always)]
    pub fn reverse_percent_for_column(self, local_col: usize, num_cols: usize) -> f32 {
        if num_cols == 0 {
            return 0.0;
        }
        let mut percent = self.reverse;
        if local_col >= num_cols / 2 {
            percent += self.split;
        }
        if (local_col & 1) != 0 {
            percent += self.alternate;
        }
        let first_cross_col = num_cols / 4;
        let last_cross_col = num_cols.saturating_sub(first_cross_col + 1);
        if local_col >= first_cross_col && local_col <= last_cross_col {
            percent += self.cross;
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
    pub fn reverse_scale_for_column(self, local_col: usize, num_cols: usize) -> f32 {
        1.0 - 2.0 * self.reverse_percent_for_column(local_col, num_cols)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PerspectiveEffects {
    pub tilt: f32,
    pub skew: f32,
}

impl PerspectiveEffects {
    #[inline(always)]
    fn from_perspective(perspective: profile::Perspective) -> Self {
        let (tilt, skew) = perspective.tilt_skew();
        Self { tilt, skew }
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct AccelOverrides {
    boost: Option<f32>,
    brake: Option<f32>,
    wave: Option<f32>,
    expand: Option<f32>,
    boomerang: Option<f32>,
}

impl AccelOverrides {
    #[inline(always)]
    fn any(self) -> bool {
        self.boost.is_some()
            || self.brake.is_some()
            || self.wave.is_some()
            || self.expand.is_some()
            || self.boomerang.is_some()
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct VisualOverrides {
    drunk: Option<f32>,
    dizzy: Option<f32>,
    confusion: Option<f32>,
    flip: Option<f32>,
    invert: Option<f32>,
    tornado: Option<f32>,
    tipsy: Option<f32>,
    bumpy: Option<f32>,
    beat: Option<f32>,
}

impl VisualOverrides {
    #[inline(always)]
    fn any(self) -> bool {
        self.drunk.is_some()
            || self.dizzy.is_some()
            || self.confusion.is_some()
            || self.flip.is_some()
            || self.invert.is_some()
            || self.tornado.is_some()
            || self.tipsy.is_some()
            || self.bumpy.is_some()
            || self.beat.is_some()
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct AppearanceOverrides {
    hidden: Option<f32>,
    hidden_offset: Option<f32>,
    sudden: Option<f32>,
    sudden_offset: Option<f32>,
    stealth: Option<f32>,
    blink: Option<f32>,
    random_vanish: Option<f32>,
}

impl AppearanceOverrides {
    #[inline(always)]
    fn any(self) -> bool {
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
struct VisibilityOverrides {
    dark: Option<f32>,
    blind: Option<f32>,
    cover: Option<f32>,
}

impl VisibilityOverrides {
    #[inline(always)]
    fn any(self) -> bool {
        self.dark.is_some() || self.blind.is_some() || self.cover.is_some()
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct ScrollOverrides {
    reverse: Option<f32>,
    split: Option<f32>,
    alternate: Option<f32>,
    cross: Option<f32>,
    centered: Option<f32>,
}

impl ScrollOverrides {
    #[inline(always)]
    fn any(self) -> bool {
        self.reverse.is_some()
            || self.split.is_some()
            || self.alternate.is_some()
            || self.cross.is_some()
            || self.centered.is_some()
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct PerspectiveOverrides {
    tilt: Option<f32>,
    skew: Option<f32>,
}

impl PerspectiveOverrides {
    #[inline(always)]
    fn any(self) -> bool {
        self.tilt.is_some() || self.skew.is_some()
    }
}

// These mirror ScreenGameplay's MinSecondsToStep/MinSecondsToMusic metrics in ITGmania.
// Simply Love scales them by MusicRate, so we apply that in init().
const MIN_SECONDS_TO_STEP: f32 = 6.0;
const MIN_SECONDS_TO_MUSIC: f32 = 2.0;
const M_MOD_HIGH_CAP: f32 = 600.0;
const SCOREBOX_NUM_ENTRIES: usize = 5;
const COLUMN_CUE_MIN_SECONDS: f32 = 1.5;

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

const MAX_HOLD_LIFE: f32 = 1.0;
const INITIAL_HOLD_LIFE: f32 = 1.0;
const TIMING_WINDOW_SECONDS_HOLD: f32 = 0.32;
const TIMING_WINDOW_SECONDS_ROLL: f32 = 0.35;
// ITG's MaxInputLatencySeconds preference defaults to 0.0.
const MAX_INPUT_LATENCY_SECONDS: f32 = 0.0;
// ITGmania _fallback defaults this off, and Simply Love relies on that dance parity.
const COMBO_BREAK_ON_IMMEDIATE_HOLD_LET_GO: bool = false;

// Simply Love: ScreenGameplay GiveUpSeconds=0.33
const GIVE_UP_HOLD_SECONDS: f32 = 0.33;
// Mirrors ScreenGameplay::AbortGiveUpText tween duration (1/2 second).
const GIVE_UP_ABORT_TEXT_SECONDS: f32 = 0.5;
const BACK_OUT_HOLD_SECONDS: f32 = 1.0;
// Simply Love: ScreenGameplay out.lua (sleep 0.5, linear 1.0).
const GIVE_UP_OUT_TOTAL_SECONDS: f32 = GIVE_UP_OUT_FADE_DELAY_SECONDS + GIVE_UP_OUT_FADE_SECONDS;
const GIVE_UP_OUT_FADE_DELAY_SECONDS: f32 = 0.5;
const GIVE_UP_OUT_FADE_SECONDS: f32 = 1.0;
// Simply Love: _fade out normal.lua (sleep 0.1, linear 0.4).
const BACK_OUT_TOTAL_SECONDS: f32 = BACK_OUT_FADE_DELAY_SECONDS + BACK_OUT_FADE_SECONDS;
const BACK_OUT_FADE_DELAY_SECONDS: f32 = 0.1;
const BACK_OUT_FADE_SECONDS: f32 = 0.4;
const ASSIST_TICK_SFX_PATH: &str = "assets/sounds/assist_tick.ogg";
pub const AUTOSYNC_OFFSET_SAMPLE_COUNT: usize = 24;
const AUTOSYNC_STDDEV_MAX_SECONDS: f32 = 0.03;
const RANDOM_ATTACK_RUN_TIME_SECONDS: f32 = 6.0;
const RANDOM_ATTACK_OVERLAP_SECONDS: f32 = 0.5;
const RANDOM_ATTACK_START_SECONDS_INIT: f32 = -1.0;
const RANDOM_ATTACK_MIN_GAMEPLAY_SECONDS: f32 = 1.0;
const GAMEPLAY_TRACE_SUMMARY_INTERVAL_S: f32 = 1.0;
const GAMEPLAY_TRACE_SLOW_FRAME_US: u32 = 4_000;
const GAMEPLAY_TRACE_PHASE_SPIKE_US: u32 = 1_000;
const GAMEPLAY_INPUT_BACKLOG_WARN: usize = 128;
const GAMEPLAY_INPUT_LATENCY_WARN_US: u32 = 2_000;
const REPLAY_EDGE_FLOOR_PER_LANE: usize = 64;
const REPLAY_EDGE_RATE_PER_SEC: usize = 256;
pub type SongTimeNs = i64;

// Mirrors ITGmania Data/RandomAttacks.txt categories for mods deadsync currently supports.
const RANDOM_ATTACK_MOD_POOL: [&str; 29] = [
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

#[inline(always)]
fn effective_mini_value_with_visual_mask(
    profile: &profile::Profile,
    visual_mask: u16,
    mini_percent: f32,
) -> f32 {
    let mut mini = if mini_percent.is_finite() {
        mini_percent
    } else {
        profile.mini_percent as f32
    };
    if (visual_mask & VISUAL_MASK_BIT_BIG) != 0 {
        // ITG _fallback/ArrowCloud map Effect Big to mod,-100% mini.
        mini -= 100.0;
    }
    mini.clamp(-100.0, 150.0) / 100.0
}

#[inline(always)]
fn effective_mini_value(profile: &profile::Profile) -> f32 {
    let visual_mask = profile::normalize_visual_effects_mask(profile.visual_effects_active_mask);
    effective_mini_value_with_visual_mask(profile, visual_mask, profile.mini_percent as f32)
}

#[inline(always)]
fn player_draw_scale_for_tilt_with_visual_mask(
    tilt: f32,
    profile: &profile::Profile,
    visual_mask: u16,
    mini_percent: f32,
) -> f32 {
    let mini = effective_mini_value_with_visual_mask(profile, visual_mask, mini_percent);
    (1.0 + 0.5 * tilt.abs()) * (1.0 + mini.abs())
}

#[inline(always)]
fn player_draw_scale_with_visual_mask(
    profile: &profile::Profile,
    visual_mask: u16,
    mini_percent: f32,
) -> f32 {
    player_draw_scale_for_tilt_with_visual_mask(
        profile.perspective.tilt_skew().0,
        profile,
        visual_mask,
        mini_percent,
    )
}

#[inline(always)]
fn player_draw_scale(profile: &profile::Profile) -> f32 {
    let visual_mask = profile::normalize_visual_effects_mask(profile.visual_effects_active_mask);
    player_draw_scale_with_visual_mask(profile, visual_mask, 0.0)
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
fn quantize_offset_seconds(v: f32) -> f32 {
    let step = 0.001_f32;
    (v / step).round() * step
}

#[inline(always)]
fn counts_for_early_rescore(note_type: NoteType) -> bool {
    matches!(
        note_type,
        NoteType::Tap | NoteType::Lift | NoteType::Hold | NoteType::Roll
    )
}

#[inline(always)]
fn row_entry_index_for_cached_row(row_map_cache: &[u32], row_index: usize) -> Option<usize> {
    let pos = *row_map_cache.get(row_index)?;
    if pos == u32::MAX {
        return None;
    }
    Some(pos as usize)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct FinalizedRowOutcome {
    final_grade: JudgeGrade,
}

#[inline(always)]
fn finalized_row_outcome_for_entry(
    row_entries: &[RowEntry],
    row_entry_index: usize,
) -> Option<FinalizedRowOutcome> {
    row_entries
        .get(row_entry_index)
        .and_then(|row_entry| row_entry.final_outcome)
}

#[inline(always)]
fn finalized_row_outcome_for_cached_row(
    row_entries: &[RowEntry],
    row_map_cache: &[u32],
    row_index: usize,
) -> Option<FinalizedRowOutcome> {
    let row_entry_index = row_entry_index_for_cached_row(row_map_cache, row_index)?;
    finalized_row_outcome_for_entry(row_entries, row_entry_index)
}

#[inline(always)]
fn row_entry_for_cached_row<'a>(
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
fn completed_row_final_judgment<'a>(
    notes: &'a [Note],
    row_entry: &RowEntry,
) -> Option<&'a Judgment> {
    let mut row_judgments: [Option<&Judgment>; MAX_COLS] = [None; MAX_COLS];
    let mut row_judgment_count = 0usize;

    for &note_index in &row_entry.nonmine_note_indices {
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
const fn row_final_grade_hides_note(grade: JudgeGrade) -> bool {
    // deadsync's gameplay ruleset is ITG timing with optional FA+ visual
    // overlays, so match Simply Love ITG's MinTNSToHideNotes=W3 behavior.
    matches!(
        grade,
        JudgeGrade::Fantastic | JudgeGrade::Excellent | JudgeGrade::Great
    )
}

#[inline(always)]
fn completed_row_flash_note_indices_and_grade(
    notes: &[Note],
    row_entry: &RowEntry,
) -> Option<([usize; MAX_COLS], usize, JudgeGrade)> {
    let Some(final_judgment) = completed_row_final_judgment(notes, row_entry) else {
        return None;
    };

    let mut out = [usize::MAX; MAX_COLS];
    let mut len = 0usize;
    for &note_index in &row_entry.nonmine_note_indices {
        debug_assert!(len < out.len());
        out[len] = note_index;
        len += 1;
    }
    Some((out, len, final_judgment.grade))
}

#[inline(always)]
pub fn row_hides_completed_note(state: &State, player: usize, row_index: usize) -> bool {
    finalized_row_outcome_for_cached_row(
        &state.row_entries,
        &state.row_map_cache[player],
        row_index,
    )
    .is_some_and(|outcome| row_final_grade_hides_note(outcome.final_grade))
}

#[inline(always)]
fn trigger_completed_row_tap_explosions(state: &mut State, player: usize, row_index: usize) {
    let Some((flash_note_indices, flash_count, flash_grade)) = ({
        let Some(row_entry) =
            row_entry_for_cached_row(&state.row_entries, &state.row_map_cache[player], row_index)
        else {
            return;
        };
        completed_row_flash_note_indices_and_grade(&state.notes, row_entry)
    }) else {
        return;
    };

    for &note_index in &flash_note_indices[..flash_count] {
        let column = state.notes[note_index].column;
        trigger_tap_explosion(state, column, flash_grade);
    }
}

#[inline(always)]
fn count_rescore_tracks_on_row(row_entry: &RowEntry) -> usize {
    usize::from(row_entry.rescore_track_count)
}

fn build_row_entry(
    row_index: usize,
    nonmine_note_indices: Vec<usize>,
    notes: &[Note],
    note_time_cache_ns: &[SongTimeNs],
) -> RowEntry {
    debug_assert!(!nonmine_note_indices.is_empty());
    let time_ns = note_time_cache_ns[nonmine_note_indices[0]];
    let mut rescore_track_count = 0u8;
    let mut unresolved_count = 0u8;
    let mut unresolved_nonlift_count = 0u8;
    let mut had_provisional_early_hit = false;
    for &note_index in &nonmine_note_indices {
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
        rescore_track_count,
        unresolved_count,
        unresolved_nonlift_count,
        had_provisional_early_hit,
        final_outcome: None,
    }
}

#[inline(always)]
fn mine_window_bounds_ns(
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
fn lane_note_window_bounds_ns(
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
fn closest_lane_note_ns(
    note_indices: &[usize],
    notes: &[Note],
    note_times_ns: &[SongTimeNs],
    current_time_ns: SongTimeNs,
    search_start_idx: usize,
    search_end_idx: usize,
) -> Option<(usize, SongTimeNs)> {
    let mut best: Option<(usize, SongTimeNs)> = None;
    let mut best_signed_err = 0i128;
    for &note_index in &note_indices[search_start_idx..search_end_idx] {
        let note = &notes[note_index];
        if note.result.is_some() || !note.can_be_judged || note.is_fake {
            continue;
        }
        let signed_err_music = current_time_ns as i128 - note_times_ns[note_index] as i128;
        let abs_err_music = signed_err_music.unsigned_abs();
        match best {
            Some((_, best_err))
                if abs_err_music > (best_err as i128).unsigned_abs()
                    || (abs_err_music == (best_err as i128).unsigned_abs()
                        && signed_err_music >= best_signed_err) => {}
            _ => {
                best = Some((note_index, signed_err_music as SongTimeNs));
                best_signed_err = signed_err_music;
            }
        }
    }
    best
}

#[inline(always)]
fn build_lane_note_display_runs(
    note_indices: &[usize],
    note_display_beats: &[f32],
) -> Vec<LaneIndexRun> {
    if note_indices.is_empty() {
        return Vec::new();
    }
    let mut runs = Vec::with_capacity(1);
    let mut run_start = 0usize;
    let mut prev = note_display_beats[note_indices[0]];
    for (pos, &note_index) in note_indices.iter().enumerate().skip(1) {
        let curr = note_display_beats[note_index];
        if curr < prev {
            runs.push(LaneIndexRun {
                start: run_start,
                end: pos,
            });
            run_start = pos;
        }
        prev = curr;
    }
    runs.push(LaneIndexRun {
        start: run_start,
        end: note_indices.len(),
    });
    runs
}

#[inline(always)]
fn build_lane_hold_display_runs(
    hold_indices: &[usize],
    hold_display_beat_min_cache: &[Option<f32>],
    hold_display_beat_max_cache: &[Option<f32>],
) -> Vec<LaneIndexRun> {
    if hold_indices.is_empty() {
        return Vec::new();
    }
    let first = hold_indices[0];
    let mut runs = Vec::with_capacity(1);
    let mut run_start = 0usize;
    let mut prev_min = hold_display_beat_min_cache[first].unwrap_or(0.0);
    let mut prev_max = hold_display_beat_max_cache[first].unwrap_or(0.0);
    debug_assert!(hold_display_beat_min_cache[first].is_some());
    debug_assert!(hold_display_beat_max_cache[first].is_some());
    for (pos, &note_index) in hold_indices.iter().enumerate().skip(1) {
        let curr_min = hold_display_beat_min_cache[note_index].unwrap_or(0.0);
        let curr_max = hold_display_beat_max_cache[note_index].unwrap_or(0.0);
        debug_assert!(hold_display_beat_min_cache[note_index].is_some());
        debug_assert!(hold_display_beat_max_cache[note_index].is_some());
        if curr_min < prev_min || curr_max < prev_max {
            runs.push(LaneIndexRun {
                start: run_start,
                end: pos,
            });
            run_start = pos;
        }
        prev_min = curr_min;
        prev_max = curr_max;
    }
    runs.push(LaneIndexRun {
        start: run_start,
        end: hold_indices.len(),
    });
    runs
}

#[inline(always)]
fn crossed_mine_bounds_ns(
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
const fn lane_edge_matches_note_type(pressed: bool, note_type: NoteType) -> bool {
    match note_type {
        NoteType::Tap | NoteType::Hold | NoteType::Roll => pressed,
        NoteType::Lift => !pressed,
        NoteType::Mine | NoteType::Fake => false,
    }
}

#[inline(always)]
fn collect_edge_judge_indices(
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
fn quantization_index_from_beat(beat: f32) -> u8 {
    // Match ITG's BeatToNoteType path: round beat->row at 48 rows/beat,
    // then classify by measure-subdivision divisibility.
    let row = (beat * 48.0).round() as i32;
    if row.rem_euclid(48) == 0 {
        noteskin::Quantization::Q4th as u8
    } else if row.rem_euclid(24) == 0 {
        noteskin::Quantization::Q8th as u8
    } else if row.rem_euclid(16) == 0 {
        noteskin::Quantization::Q12th as u8
    } else if row.rem_euclid(12) == 0 {
        noteskin::Quantization::Q16th as u8
    } else if row.rem_euclid(8) == 0 {
        noteskin::Quantization::Q24th as u8
    } else if row.rem_euclid(6) == 0 {
        noteskin::Quantization::Q32nd as u8
    } else if row.rem_euclid(4) == 0 {
        noteskin::Quantization::Q48th as u8
    } else if row.rem_euclid(3) == 0 {
        noteskin::Quantization::Q64th as u8
    } else {
        noteskin::Quantization::Q192nd as u8
    }
}

#[derive(Clone, Copy, Debug)]
struct TurnRng {
    state: u64,
}

impl TurnRng {
    #[inline(always)]
    fn new(seed: u64) -> Self {
        let seed = if seed == 0 {
            0x9E37_79B9_7F4A_7C15
        } else {
            seed
        };
        Self { state: seed }
    }

    #[inline(always)]
    fn next_u32(&mut self) -> u32 {
        // xorshift64*
        let mut x = self.state;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.state = x;
        (x >> 32) as u32
    }

    #[inline(always)]
    fn next_f32_unit(&mut self) -> f32 {
        (self.next_u32() as f32) * (1.0 / 4_294_967_296.0)
    }

    #[inline(always)]
    fn gen_range(&mut self, upper_exclusive: usize) -> usize {
        if upper_exclusive <= 1 {
            0
        } else {
            (self.next_u32() as usize) % upper_exclusive
        }
    }

    fn shuffle<T>(&mut self, slice: &mut [T]) {
        if slice.len() <= 1 {
            return;
        }
        for i in (1..slice.len()).rev() {
            let j = self.gen_range(i + 1);
            slice.swap(i, j);
        }
    }
}

fn turn_seed_for_song(song: &SongData) -> u64 {
    let mut hasher = XxHash64::with_seed(0);
    hasher.write(song.simfile_path.to_string_lossy().as_bytes());
    hasher.finish()
}

fn turn_take_from(turn: profile::TurnOption, cols: usize, seed: u64) -> Option<Vec<usize>> {
    if cols == 0 {
        return None;
    }
    use profile::TurnOption;
    match (turn, cols) {
        (TurnOption::None, _) => None,
        (TurnOption::Mirror, _) => Some((0..cols).rev().collect()),
        (TurnOption::LRMirror, 4) => Some(vec![3, 1, 2, 0]),
        (TurnOption::LRMirror, 8) => Some(vec![7, 5, 6, 4, 3, 1, 2, 0]),
        (TurnOption::UDMirror, 4) => Some(vec![0, 2, 1, 3]),
        (TurnOption::UDMirror, 8) => Some(vec![0, 2, 1, 3, 4, 6, 5, 7]),
        (TurnOption::Left, 4) => Some(vec![2, 0, 3, 1]),
        (TurnOption::Left, 8) => Some(vec![2, 0, 3, 1, 6, 4, 7, 5]),
        (TurnOption::Right, 4) => Some(vec![1, 3, 0, 2]),
        (TurnOption::Right, 8) => Some(vec![1, 3, 0, 2, 5, 7, 4, 6]),
        (TurnOption::Shuffle, _) => {
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

fn apply_turn_permutation(
    notes: &mut [Note],
    note_range: (usize, usize),
    col_offset: usize,
    cols: usize,
    turn: profile::TurnOption,
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

#[derive(Clone, Copy)]
struct RowGrid {
    row_index: usize,
    note_indices: [usize; MAX_COLS],
}

#[inline(always)]
fn notes_row_sorted(notes: &[Note]) -> bool {
    notes
        .windows(2)
        .all(|pair| pair[0].row_index <= pair[1].row_index)
}

fn build_row_grids(
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
fn assist_clap_cursor_for_row(rows: &[usize], row: i32) -> usize {
    if row < 0 {
        0
    } else {
        rows.partition_point(|&r| r <= row as usize)
    }
}

#[inline(always)]
fn timing_row_floor(timing: &TimingData, beat: f32) -> usize {
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
fn assist_row_no_offset(state: &State, music_time: f32) -> i32 {
    assist_row_no_offset_ns(state, song_time_ns_from_seconds(music_time))
}

#[inline(always)]
fn assist_row_no_offset_ns(state: &State, music_time_ns: SongTimeNs) -> i32 {
    // ITG parity: assist clap/metronome uses *no global offset* timing.
    // TimingData::get_beat_for_time_ns() applies global offset internally, so
    // feed (time - offset) to cancel it out.
    let beat_no_offset = state.timing.get_beat_for_time_ns(song_time_ns_add_seconds(
        music_time_ns,
        -state.global_offset_seconds,
    ));
    timing_row_floor(&state.timing, beat_no_offset).min(i32::MAX as usize) as i32
}

fn build_assist_clap_rows(notes: &[Note], note_range: (usize, usize)) -> Vec<usize> {
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

fn update_active_holds_for_row(
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

fn apply_super_shuffle_taps(
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
        update_active_holds_for_row(notes, row, &grid, cols, &mut hold_end_row);

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

fn apply_hyper_shuffle(
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

fn apply_turn_options(
    notes: &mut [Note],
    note_ranges: [(usize, usize); MAX_PLAYERS],
    cols_per_player: usize,
    num_players: usize,
    player_profiles: &[profile::Profile; MAX_PLAYERS],
    base_seed: u64,
) {
    for player in 0..num_players {
        let turn = player_profiles[player].turn_option;
        let note_range = note_ranges[player];
        let col_offset = player * cols_per_player;
        match turn {
            profile::TurnOption::None => {}
            profile::TurnOption::Blender => {
                apply_turn_permutation(
                    notes,
                    note_range,
                    col_offset,
                    cols_per_player,
                    profile::TurnOption::Shuffle,
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
            profile::TurnOption::Random => {
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
fn note_counts_for_simultaneous_limit(note: &Note) -> bool {
    match note.note_type {
        NoteType::Tap | NoteType::Lift => !note.is_fake,
        NoteType::Hold | NoteType::Roll => true,
        NoteType::Mine | NoteType::Fake => false,
    }
}

fn enforce_max_simultaneous_notes(
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

    let mut i = 0usize;
    notes.retain(|_| {
        let keep = !remove_idx[i];
        i += 1;
        keep
    });
}

#[inline(always)]
fn local_player_col(column: usize, col_offset: usize, cols: usize) -> Option<usize> {
    if column < col_offset {
        return None;
    }
    let local = column - col_offset;
    if local < cols { Some(local) } else { None }
}

fn sort_player_notes(notes: &mut [Note]) {
    notes.sort_unstable_by_key(|note| (note.row_index, note.column));
}

fn player_rows(notes: &[Note], col_offset: usize, cols: usize) -> Vec<usize> {
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

fn count_nonempty_tracks_at_row(
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

fn count_tap_or_hold_tracks_at_row(
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

fn count_tap_tracks_at_row(notes: &[Note], row: usize, col_offset: usize, cols: usize) -> usize {
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

fn first_nonempty_track_at_row(
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

fn first_tap_track_at_row(
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

fn cell_has_any_note(notes: &[Note], row: usize, column: usize) -> bool {
    notes
        .iter()
        .any(|note| note.row_index == row && note.column == column)
}

fn cell_has_nonfake_note(notes: &[Note], row: usize, column: usize) -> bool {
    notes
        .iter()
        .any(|note| note.row_index == row && note.column == column && !note.is_fake)
}

fn remove_cell_notes(notes: &mut Vec<Note>, row: usize, column: usize) {
    notes.retain(|note| !(note.row_index == row && note.column == column));
}

fn is_hold_body_at_row(notes: &[Note], row: usize, column: usize) -> bool {
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

fn count_held_tracks_at_row(notes: &[Note], row: usize, col_offset: usize, cols: usize) -> usize {
    (0..cols)
        .filter(|local| is_hold_body_at_row(notes, row, col_offset + *local))
        .count()
}

fn set_added_tap_note(
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

fn set_added_mine_note(
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

fn convert_tap_row_to_mines(notes: &mut [Note], row: usize) {
    for note in notes.iter_mut() {
        if note.row_index == row && note.note_type == NoteType::Tap {
            note.note_type = NoteType::Mine;
            note.hold = None;
            note.mine_result = None;
        }
    }
}

fn track_range_has_any_note(
    notes: &[Note],
    column: usize,
    start_row: usize,
    end_row: usize,
) -> bool {
    notes.iter().any(|note| {
        note.column == column && note.row_index >= start_row && note.row_index <= end_row
    })
}

fn apply_mines_insert(
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

fn apply_insert_intelligent_taps(
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

fn apply_wide_insert(
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

#[inline(always)]
fn stomp_mirror_track(local_track: usize, cols: usize) -> usize {
    match cols {
        4 => [3, 2, 1, 0][local_track],
        8 => [1, 0, 3, 2, 5, 4, 7, 6][local_track],
        _ => cols.saturating_sub(1).saturating_sub(local_track),
    }
}

fn apply_stomp_insert(
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

fn apply_echo_insert(
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

fn convert_taps_to_holds(
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

fn apply_uncommon_masks_with_masks(
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

fn apply_uncommon_masks_for_player(
    notes: &mut Vec<Note>,
    player_profile: &profile::Profile,
    timing_player: &TimingData,
    col_offset: usize,
    cols: usize,
    player: usize,
) {
    apply_uncommon_masks_with_masks(
        notes,
        profile::normalize_insert_mask(player_profile.insert_active_mask),
        profile::normalize_remove_mask(player_profile.remove_active_mask),
        profile::normalize_holds_mask(player_profile.holds_active_mask),
        timing_player,
        col_offset,
        cols,
        &[],
        None,
        player,
    );
}

#[inline(always)]
fn has_uncommon_masks(profile: &profile::Profile) -> bool {
    profile::normalize_insert_mask(profile.insert_active_mask) != 0
        || profile::normalize_remove_mask(profile.remove_active_mask) != 0
        || profile::normalize_holds_mask(profile.holds_active_mask) != 0
}

fn apply_uncommon_chart_transforms(
    notes: &mut Vec<Note>,
    note_ranges: &mut [(usize, usize); MAX_PLAYERS],
    cols_per_player: usize,
    num_players: usize,
    player_profiles: &[profile::Profile; MAX_PLAYERS],
    timing_players: &[Arc<TimingData>; MAX_PLAYERS],
) {
    if num_players == 0
        || !player_profiles
            .iter()
            .take(num_players)
            .any(has_uncommon_masks)
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
        if !has_uncommon_masks(&player_profiles[player]) {
            transformed.extend_from_slice(&notes[slice_start..slice_end]);
            transformed_ranges[player] = (out_start, transformed.len());
            continue;
        }
        let mut player_notes = notes[slice_start..slice_end].to_vec();
        apply_uncommon_masks_for_player(
            &mut player_notes,
            &player_profiles[player],
            timing_players[player].as_ref(),
            player.saturating_mul(cols_per_player),
            cols_per_player,
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

fn compute_end_times_ns(
    notes: &[Note],
    note_time_cache_ns: &[SongTimeNs],
    hold_end_time_cache_ns: &[Option<SongTimeNs>],
    rate: f32,
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
        last_relevant_time_ns.saturating_add(max_step_distance_ns),
    )
}

#[inline(always)]
fn late_note_resolution_window_ns(timing_profile: &TimingProfile, rate: f32) -> SongTimeNs {
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
fn max_step_distance_ns(timing_profile: &TimingProfile, rate: f32) -> SongTimeNs {
    late_note_resolution_window_ns(timing_profile, rate)
        .saturating_add(song_time_ns_from_seconds(MAX_INPUT_LATENCY_SECONDS))
}

#[derive(Clone, Debug)]
pub struct RowEntry {
    row_index: usize,
    time_ns: SongTimeNs,
    // Non-mine, non-fake, judgable notes on this row
    nonmine_note_indices: Vec<usize>,
    rescore_track_count: u8,
    unresolved_count: u8,
    unresolved_nonlift_count: u8,
    had_provisional_early_hit: bool,
    final_outcome: Option<FinalizedRowOutcome>,
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

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LaneIndexRun {
    pub start: usize,
    pub end: usize,
}

#[derive(Clone, Debug)]
pub struct JudgmentRenderInfo {
    pub judgment: Judgment,
    pub started_at_screen_s: f32,
}

#[derive(Copy, Clone, Debug)]
pub struct HoldJudgmentRenderInfo {
    pub result: HoldResult,
    pub started_at_screen_s: f32,
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
const fn column_cue_is_mine(note: &Note) -> Option<bool> {
    if note.is_fake {
        return None;
    }
    match note.note_type {
        NoteType::Tap | NoteType::Lift | NoteType::Hold | NoteType::Roll => Some(false),
        NoteType::Mine => Some(true),
        NoteType::Fake => None,
    }
}

fn build_column_cues_for_player(
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
fn compute_column_scroll_dirs(
    scroll_option: profile::ScrollOption,
    num_cols: usize,
) -> [f32; MAX_COLS] {
    use profile::ScrollOption;
    let mut dirs = [1.0_f32; MAX_COLS];
    let n = num_cols.min(MAX_COLS);

    if scroll_option.contains(ScrollOption::Reverse) {
        for d in dirs.iter_mut().take(n) {
            *d *= -1.0;
        }
    }
    if scroll_option.contains(ScrollOption::Split) {
        for base in (0..n).step_by(4) {
            if base + 2 < n {
                dirs[base + 2] *= -1.0;
            }
            if base + 3 < n {
                dirs[base + 3] *= -1.0;
            }
        }
    }
    if scroll_option.contains(ScrollOption::Alternate) {
        for base in (0..n).step_by(4) {
            if base + 1 < n {
                dirs[base + 1] *= -1.0;
            }
            if base + 3 < n {
                dirs[base + 3] *= -1.0;
            }
        }
    }
    if scroll_option.contains(ScrollOption::Cross) {
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

#[inline(always)]
fn player_side_for_index(
    play_style: profile::PlayStyle,
    session_side: profile::PlayerSide,
    player_idx: usize,
) -> profile::PlayerSide {
    if play_style == profile::PlayStyle::Versus {
        if player_idx == 0 {
            profile::PlayerSide::P1
        } else {
            profile::PlayerSide::P2
        }
    } else {
        session_side
    }
}

#[inline(always)]
const fn single_runtime_player_is_p2(
    play_style: profile::PlayStyle,
    session_side: profile::PlayerSide,
) -> bool {
    matches!(
        (play_style, session_side),
        (
            profile::PlayStyle::Single | profile::PlayStyle::Double,
            profile::PlayerSide::P2
        )
    )
}

#[inline(always)]
const fn side_index(side: profile::PlayerSide) -> usize {
    match side {
        profile::PlayerSide::P1 => 0,
        profile::PlayerSide::P2 => 1,
    }
}

#[inline(always)]
pub fn scorebox_snapshot_for_side(
    state: &State,
    side: profile::PlayerSide,
) -> Option<&scores::CachedPlayerLeaderboardData> {
    state.scorebox_side_snapshot[side_index(side)].as_ref()
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
}

#[derive(Clone, Copy, Debug)]
pub struct OffsetIndicatorText {
    pub started_at: f32,
    pub offset_ms: f32,
    pub window: TimingWindow,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum HealthState {
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
struct DangerFx {
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
fn health_state_for_player(p: &PlayerRuntime) -> HealthState {
    if p.is_failing || p.life <= 0.0 {
        HealthState::Dead
    } else if p.life < DANGER_THRESHOLD {
        HealthState::Danger
    } else {
        HealthState::Alive
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct ExScoreInputs {
    counts: crate::game::timing::WindowCounts,
    counts_10ms: crate::game::timing::WindowCounts,
    holds_held_for_score: u32,
    holds_let_go_for_score: u32,
    rolls_held_for_score: u32,
    rolls_let_go_for_score: u32,
    mines_hit_for_score: u32,
}

#[derive(Clone, Debug)]
pub struct PlayerRuntime {
    pub combo: u32,
    pub miss_combo: u32,
    pub full_combo_grade: Option<JudgeGrade>,
    pub current_combo_grade: Option<JudgeGrade>,
    pub first_fc_attempt_broken: bool,
    pub judgment_counts: judgment::JudgeCounts,
    pub scoring_counts: judgment::JudgeCounts,
    pub provisional_scoring_counts: judgment::JudgeCounts,
    pub last_judgment: Option<JudgmentRenderInfo>,

    pub life: f32,
    pub combo_after_miss: u32,
    pub is_failing: bool,
    pub fail_time: Option<f32>,
    pub calories_burned: f32,

    pub earned_grade_points: i32,

    pub combo_milestones: Vec<ActiveComboMilestone>,
    pub hands_achieved: u32,
    pub holds_held: u32,
    pub holds_held_for_score: u32,
    pub holds_let_go_for_score: u32,
    pub rolls_held: u32,
    pub rolls_held_for_score: u32,
    pub rolls_let_go_for_score: u32,
    pub mines_hit: u32,
    pub mines_hit_for_score: u32,
    pub mines_avoided: u32,
    hands_holding_count_for_stats: i32,
    failed_ex_score_inputs: Option<ExScoreInputs>,

    pub life_history: Vec<(f32, f32)>, // (time, life_value)

    pub error_bar_mono_ticks: [Option<ErrorBarTick>; 15],
    pub error_bar_mono_next: usize,
    pub error_bar_color_ticks: [Option<ErrorBarTick>; 10],
    pub error_bar_color_next: usize,
    pub error_bar_color_bar_started_at: Option<f32>,
    pub error_bar_color_flash_early: [Option<f32>; 6],
    pub error_bar_color_flash_late: [Option<f32>; 6],
    pub error_bar_text: Option<ErrorBarText>,
    pub offset_indicator_text: Option<OffsetIndicatorText>,
    pub error_bar_avg_ticks: [Option<ErrorBarTick>; 5],
    pub error_bar_avg_next: usize,
    pub error_bar_avg_bar_started_at: Option<f32>,
    pub error_bar_avg_samples: VecDeque<(f32, f32)>,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct CourseDisplayCarry {
    pub judgment_counts: [u32; 6],
    pub scoring_counts: [u32; 6],
    // Canonical FA+ split (15ms) used for EX scoring/evaluation.
    pub window_counts: crate::game::timing::WindowCounts,
    // Canonical 10ms split used for H.EX scoring/evaluation.
    pub window_counts_10ms_blue: crate::game::timing::WindowCounts,
    // Display split used by gameplay counters (legacy 10ms or custom ms option).
    pub window_counts_display_blue: crate::game::timing::WindowCounts,
    pub holds_held_for_score: u32,
    pub holds_let_go_for_score: u32,
    pub rolls_held_for_score: u32,
    pub rolls_let_go_for_score: u32,
    pub mines_hit_for_score: u32,
}

const DISPLAY_JUDGE_ORDER: [JudgeGrade; 6] = [
    JudgeGrade::Fantastic,
    JudgeGrade::Excellent,
    JudgeGrade::Great,
    JudgeGrade::Decent,
    JudgeGrade::WayOff,
    JudgeGrade::Miss,
];

#[inline(always)]
const fn display_judge_ix(grade: JudgeGrade) -> usize {
    judgment::judge_grade_ix(grade)
}

#[inline(always)]
const fn judge_life_delta(grade: JudgeGrade) -> f32 {
    match grade {
        JudgeGrade::Fantastic => LIFE_FANTASTIC,
        JudgeGrade::Excellent => LIFE_EXCELLENT,
        JudgeGrade::Great => LIFE_GREAT,
        JudgeGrade::Decent => LIFE_DECENT,
        JudgeGrade::WayOff => LIFE_WAY_OFF,
        JudgeGrade::Miss => LIFE_MISS,
    }
}

#[inline(always)]
fn score_missed_holds_and_rolls(chart_type: &str) -> bool {
    // ITGmania _fallback metrics:
    // ScoreMissedHoldsAndRolls = not IsGame("pump") and not IsGame("dance")
    let chart_type = chart_type.trim();
    let is_dance = chart_type
        .get(..5)
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("dance"));
    let is_pump = chart_type
        .get(..4)
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("pump"));
    !(is_dance || is_pump)
}

fn init_player_runtime() -> PlayerRuntime {
    PlayerRuntime {
        combo: 0,
        miss_combo: 0,
        full_combo_grade: None,
        current_combo_grade: None,
        first_fc_attempt_broken: false,
        judgment_counts: [0; judgment::JUDGE_GRADE_COUNT],
        scoring_counts: [0; judgment::JUDGE_GRADE_COUNT],
        provisional_scoring_counts: [0; judgment::JUDGE_GRADE_COUNT],
        last_judgment: None,
        life: 0.5,
        combo_after_miss: 0,
        is_failing: false,
        fail_time: None,
        calories_burned: 0.0,
        earned_grade_points: 0,
        combo_milestones: Vec::new(),
        hands_achieved: 0,
        holds_held: 0,
        holds_held_for_score: 0,
        holds_let_go_for_score: 0,
        rolls_held: 0,
        rolls_held_for_score: 0,
        rolls_let_go_for_score: 0,
        mines_hit: 0,
        mines_hit_for_score: 0,
        mines_avoided: 0,
        hands_holding_count_for_stats: 0,
        failed_ex_score_inputs: None,
        life_history: Vec::with_capacity(10000),
        error_bar_mono_ticks: [None; 15],
        error_bar_mono_next: 0,
        error_bar_color_ticks: [None; 10],
        error_bar_color_next: 0,
        error_bar_color_bar_started_at: None,
        error_bar_color_flash_early: [None; 6],
        error_bar_color_flash_late: [None; 6],
        error_bar_text: None,
        offset_indicator_text: None,
        error_bar_avg_ticks: [None; 5],
        error_bar_avg_next: 0,
        error_bar_avg_bar_started_at: None,
        error_bar_avg_samples: VecDeque::with_capacity(64),
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

#[derive(Clone, Copy, Debug, Default)]
struct GameplayUpdatePhaseTimings {
    pre_notes_us: u32,
    autoplay_us: u32,
    input_edges_us: u32,
    input_queue_us: u32,
    input_state_us: u32,
    input_glow_us: u32,
    input_judge_us: u32,
    input_roll_us: u32,
    held_mines_us: u32,
    active_holds_us: u32,
    hold_decay_us: u32,
    visuals_us: u32,
    spawn_arrows_us: u32,
    mine_avoid_us: u32,
    tap_miss_us: u32,
    cull_us: u32,
    judged_rows_us: u32,
    density_us: u32,
    density_sample_us: u32,
    danger_us: u32,
    untracked_us: u32,
}

#[derive(Clone, Copy, Debug, Default)]
struct GameplayInputLatencyTrace {
    samples: u32,
    capture_to_store_total_us: u64,
    store_to_emit_total_us: u64,
    emit_to_queue_total_us: u64,
    capture_to_process_total_us: u64,
    queue_to_process_total_us: u64,
    capture_to_store_max_us: u32,
    store_to_emit_max_us: u32,
    emit_to_queue_max_us: u32,
    capture_to_process_max_us: u32,
    queue_to_process_max_us: u32,
}

impl GameplayInputLatencyTrace {
    #[inline(always)]
    fn record(
        &mut self,
        capture_to_store_us: u32,
        store_to_emit_us: u32,
        emit_to_queue_us: u32,
        capture_to_process_us: u32,
        queue_to_process_us: u32,
    ) {
        self.samples = self.samples.saturating_add(1);
        self.capture_to_store_total_us = self
            .capture_to_store_total_us
            .saturating_add(u64::from(capture_to_store_us));
        self.store_to_emit_total_us = self
            .store_to_emit_total_us
            .saturating_add(u64::from(store_to_emit_us));
        self.emit_to_queue_total_us = self
            .emit_to_queue_total_us
            .saturating_add(u64::from(emit_to_queue_us));
        self.capture_to_process_total_us = self
            .capture_to_process_total_us
            .saturating_add(u64::from(capture_to_process_us));
        self.queue_to_process_total_us = self
            .queue_to_process_total_us
            .saturating_add(u64::from(queue_to_process_us));
        self.capture_to_store_max_us = self.capture_to_store_max_us.max(capture_to_store_us);
        self.store_to_emit_max_us = self.store_to_emit_max_us.max(store_to_emit_us);
        self.emit_to_queue_max_us = self.emit_to_queue_max_us.max(emit_to_queue_us);
        self.capture_to_process_max_us = self.capture_to_process_max_us.max(capture_to_process_us);
        self.queue_to_process_max_us = self.queue_to_process_max_us.max(queue_to_process_us);
    }

    #[inline(always)]
    fn avg_us(total_us: u64, samples: u32) -> f32 {
        if samples == 0 {
            0.0
        } else {
            total_us as f32 / samples as f32
        }
    }
}

#[derive(Clone, Debug)]
struct GameplayUpdateTraceState {
    frame_counter: u64,
    summary_elapsed_s: f32,
    summary_frames: u32,
    summary_slow_frames: u32,
    summary_max_total_us: u32,
    summary_max_phase: GameplayUpdatePhaseTimings,
    summary_input_latency: GameplayInputLatencyTrace,
    summary_peak_pending_edges: usize,
    pending_edges_capacity: usize,
    replay_edges_capacity: usize,
    decaying_hold_capacity: usize,
    density_life_capacity: [usize; MAX_PLAYERS],
}

impl Default for GameplayUpdateTraceState {
    fn default() -> Self {
        Self {
            frame_counter: 0,
            summary_elapsed_s: 0.0,
            summary_frames: 0,
            summary_slow_frames: 0,
            summary_max_total_us: 0,
            summary_max_phase: GameplayUpdatePhaseTimings::default(),
            summary_input_latency: GameplayInputLatencyTrace::default(),
            summary_peak_pending_edges: 0,
            pending_edges_capacity: 0,
            replay_edges_capacity: 0,
            decaying_hold_capacity: 0,
            density_life_capacity: [0; MAX_PLAYERS],
        }
    }
}

pub struct State {
    pub song: Arc<SongData>,
    pub song_full_title: Arc<str>,
    pub stage_intro_text: Arc<str>,
    pub pack_group: Arc<str>,
    pub pack_banner_path: Option<PathBuf>,
    pub current_background_path: Option<PathBuf>,
    pub next_background_change_ix: usize,
    pub background_texture_key: String,
    pub charts: [Arc<ChartData>; MAX_PLAYERS],
    pub gameplay_charts: [Arc<GameplayChartData>; MAX_PLAYERS],
    pub num_cols: usize,
    pub cols_per_player: usize,
    pub num_players: usize,
    pub timing: Arc<TimingData>,
    pub timing_players: [Arc<TimingData>; MAX_PLAYERS],
    pub beat_info_cache: BeatInfoCache,
    pub timing_profile: TimingProfile,
    player_judgment_timing: [PlayerJudgmentTiming; MAX_PLAYERS],
    pub notes: Vec<Note>,
    pub note_ranges: [(usize, usize); MAX_PLAYERS],
    pub audio_lead_in_seconds: f32,
    pub current_beat: f32,
    pub current_music_time_ns: SongTimeNs,
    pub current_beat_display: f32,
    pub current_music_time_display: f32,
    display_clock: FrameStableDisplayClock,
    display_clock_diag: DisplayClockDiagRing,
    pub lane_note_indices: [Vec<usize>; MAX_COLS],
    pub lane_hold_indices: [Vec<usize>; MAX_COLS],
    pub lane_note_display_runs: [Vec<LaneIndexRun>; MAX_COLS],
    pub lane_hold_display_runs: [Vec<LaneIndexRun>; MAX_COLS],
    pub row_entry_ranges: [(usize, usize); MAX_PLAYERS],
    pub judged_row_cursor: [usize; MAX_PLAYERS],
    pub note_time_cache_ns: Vec<SongTimeNs>,
    pub note_display_beat_cache: Vec<f32>,
    pub hold_end_time_cache_ns: Vec<Option<SongTimeNs>>,
    pub hold_end_display_beat_cache: Vec<Option<f32>>,
    pub hold_display_beat_min_cache: Vec<Option<f32>>,
    pub hold_display_beat_max_cache: Vec<Option<f32>>,
    pub notes_end_time_ns: SongTimeNs,
    pub music_end_time_ns: SongTimeNs,
    pub music_rate: f32,
    pub play_mine_sounds: bool,
    pub global_offset_seconds: f32,
    pub initial_global_offset_seconds: f32,
    pub player_global_offset_shift_seconds: [f32; MAX_PLAYERS],
    pub song_offset_seconds: f32,
    pub initial_song_offset_seconds: f32,
    pub autosync_mode: AutosyncMode,
    pub autosync_offset_samples: [SongTimeNs; AUTOSYNC_OFFSET_SAMPLE_COUNT],
    pub autosync_offset_sample_count: usize,
    pub autosync_standard_deviation: f32,
    pub global_visual_delay_seconds: f32,
    pub player_visual_delay_seconds: [f32; MAX_PLAYERS],
    pub current_music_time_visible_ns: [SongTimeNs; MAX_PLAYERS],
    pub current_music_time_visible: [f32; MAX_PLAYERS],
    pub current_beat_visible: [f32; MAX_PLAYERS],
    pub next_tap_miss_cursor: [usize; MAX_PLAYERS],
    pub next_mine_avoid_cursor: [usize; MAX_PLAYERS],
    pub mine_note_ix: [Vec<usize>; MAX_PLAYERS],
    pub mine_note_time_ns: [Vec<SongTimeNs>; MAX_PLAYERS],
    pub next_mine_ix_cursor: [usize; MAX_PLAYERS],
    pub row_entries: Vec<RowEntry>,
    pub measure_counter_segments: [Vec<StreamSegment>; MAX_PLAYERS],
    pub column_cues: [Vec<ColumnCue>; MAX_PLAYERS],
    pub mini_indicator_stream_segments: [Vec<StreamSegment>; MAX_PLAYERS],
    pub mini_indicator_total_stream_measures: [f32; MAX_PLAYERS],
    pub mini_indicator_target_score_percent: [f64; MAX_PLAYERS],
    pub mini_indicator_rival_score_percent: [f64; MAX_PLAYERS],

    // Optimization: Per-player direct row lookup instead of HashMap
    pub row_map_cache: [Vec<u32>; MAX_PLAYERS],
    pub note_row_entry_indices: Vec<u32>,
    // Bit flags per note index:
    // bit0 => same row contains a hold start, bit1 => same row contains a roll start.
    pub tap_row_hold_roll_flags: Vec<u8>,

    pub decaying_hold_indices: Vec<usize>,
    pub hold_decay_active: Vec<bool>,
    pub tap_miss_held_window: Vec<bool>,
    pending_missed_hold_feedback: Vec<bool>,
    pending_missed_hold_indices: Vec<usize>,

    pub players: [PlayerRuntime; MAX_PLAYERS],
    pub hold_judgments: [Option<HoldJudgmentRenderInfo>; MAX_COLS],
    pub is_in_freeze: bool,
    pub is_in_delay: bool,

    pub possible_grade_points: [i32; MAX_PLAYERS],
    pub song_completed_naturally: bool,
    pub autoplay_enabled: bool,
    pub autoplay_used: bool,
    pub score_valid: [bool; MAX_PLAYERS],
    score_missed_holds_rolls: [bool; MAX_PLAYERS],
    replay_mode: bool,
    replay_capture_enabled: bool,
    pub course_display_carry: Option<[CourseDisplayCarry; MAX_PLAYERS]>,
    pub course_display_totals: Option<[CourseDisplayTotals; MAX_PLAYERS]>,
    pub live_window_counts: [crate::game::timing::WindowCounts; MAX_PLAYERS],
    pub live_window_counts_10ms_blue: [crate::game::timing::WindowCounts; MAX_PLAYERS],
    pub live_window_counts_display_blue: [crate::game::timing::WindowCounts; MAX_PLAYERS],

    pub player_profiles: [profile::Profile; MAX_PLAYERS],
    pub scorebox_side_snapshot: [Option<scores::CachedPlayerLeaderboardData>; MAX_PLAYERS],
    attack_mask_windows: [Vec<AttackMaskWindow>; MAX_PLAYERS],
    song_lua_ease_windows: [Vec<SongLuaEaseMaskWindow>; MAX_PLAYERS],
    pub song_lua_overlays: Vec<SongLuaOverlayActor>,
    // Gameplay-thread song-lua caches built at song load and read every frame by
    // render, so overlay evaluation stays local to each overlay.
    pub song_lua_overlay_eases: Vec<SongLuaOverlayEaseWindowRuntime>,
    pub song_lua_overlay_ease_ranges: Vec<std::ops::Range<usize>>,
    pub song_lua_overlay_events: Vec<Vec<SongLuaOverlayMessageRuntime>>,
    pub song_lua_hidden_players: [bool; MAX_PLAYERS],
    pub song_lua_screen_width: f32,
    pub song_lua_screen_height: f32,
    pub song_lua_player_rotation_z: [f32; MAX_PLAYERS],
    pub song_lua_player_rotation_y: [f32; MAX_PLAYERS],
    pub song_lua_player_skew_x: [f32; MAX_PLAYERS],
    pub song_lua_player_zoom_x: [f32; MAX_PLAYERS],
    pub song_lua_player_zoom_y: [f32; MAX_PLAYERS],
    pub song_lua_player_confusion_y_offset: [f32; MAX_PLAYERS],
    active_attack_clear_all: [bool; MAX_PLAYERS],
    active_attack_chart: [ChartAttackEffects; MAX_PLAYERS],
    active_attack_accel: [AccelOverrides; MAX_PLAYERS],
    active_attack_visual: [VisualOverrides; MAX_PLAYERS],
    attack_current_appearance: [AppearanceEffects; MAX_PLAYERS],
    attack_target_appearance: [AppearanceEffects; MAX_PLAYERS],
    attack_speed_appearance: [AppearanceEffects; MAX_PLAYERS],
    active_attack_appearance: [AppearanceEffects; MAX_PLAYERS],
    active_attack_visibility: [VisibilityOverrides; MAX_PLAYERS],
    active_attack_scroll: [ScrollOverrides; MAX_PLAYERS],
    active_attack_perspective: [PerspectiveOverrides; MAX_PLAYERS],
    active_attack_scroll_speed: [Option<ScrollSpeedSetting>; MAX_PLAYERS],
    active_attack_mini_percent: [Option<f32>; MAX_PLAYERS],
    pub noteskin: [Option<Arc<Noteskin>>; MAX_PLAYERS],
    pub mine_noteskin: [Option<Arc<Noteskin>>; MAX_PLAYERS],
    pub receptor_noteskin: [Option<Arc<Noteskin>>; MAX_PLAYERS],
    pub tap_explosion_noteskin: [Option<Arc<Noteskin>>; MAX_PLAYERS],
    pub active_color_index: i32,
    pub player_color: [f32; 4],
    pub scroll_speed: [ScrollSpeedSetting; MAX_PLAYERS],
    pub scroll_reference_bpm: f32,
    pub field_zoom: [f32; MAX_PLAYERS],
    pub(crate) notefield_model_cache: [RefCell<ModelMeshCache>; MAX_PLAYERS],
    pub scroll_pixels_per_second: [f32; MAX_PLAYERS],
    pub scroll_travel_time: [f32; MAX_PLAYERS],
    pub draw_distance_before_targets: [f32; MAX_PLAYERS],
    pub draw_distance_after_targets: [f32; MAX_PLAYERS],
    pub reverse_scroll: [bool; MAX_PLAYERS],
    pub column_scroll_dirs: [f32; MAX_COLS],
    pub receptor_glow_timers: [f32; MAX_COLS],
    receptor_glow_press_timers: [f32; MAX_COLS],
    receptor_glow_lift_start_alpha: [f32; MAX_COLS],
    receptor_glow_lift_start_zoom: [f32; MAX_COLS],
    pub receptor_bop_timers: [f32; MAX_COLS],
    pub tap_explosions: [Option<ActiveTapExplosion>; MAX_COLS],
    pub mine_explosions: [Option<ActiveMineExplosion>; MAX_COLS],
    pub active_holds: [Option<ActiveHold>; MAX_COLS],

    pub holds_total: [u32; MAX_PLAYERS],
    pub rolls_total: [u32; MAX_PLAYERS],
    pub mines_total: [u32; MAX_PLAYERS],
    pub total_steps: [u32; MAX_PLAYERS],
    #[allow(dead_code)]
    // Stored for parity with transformed radar values; no live UI reads this yet.
    pub jumps_total: [u32; MAX_PLAYERS],
    pub hands_total: [u32; MAX_PLAYERS],

    pub total_elapsed_in_screen: f32,

    pub lobby_music_started: bool,
    pub lobby_ready_p1: bool,
    pub lobby_ready_p2: bool,
    pub lobby_disconnect_hold_p1: Option<Instant>,
    pub lobby_disconnect_hold_p2: Option<Instant>,
    pub sync_overlay_message: Option<Arc<str>>,
    pub replay_status_text: Option<Arc<str>>,
    danger_fx: [DangerFx; MAX_PLAYERS],

    pub density_graph_first_second: f32,
    pub density_graph_last_second: f32,
    pub density_graph_duration: f32,
    pub density_graph_graph_w: f32,
    pub density_graph_graph_h: f32,
    pub density_graph_scaled_width: f32,
    pub density_graph_u0: f32,
    pub density_graph_u_window: f32,
    pub density_graph_life_update_rate: f32,
    pub density_graph_life_next_update_elapsed: f32,
    pub density_graph_life_points: [Vec<[f32; 2]>; MAX_PLAYERS],
    pub density_graph_life_dirty: [bool; MAX_PLAYERS],
    pub density_graph_top_h: f32,
    pub density_graph_top_w: [f32; MAX_PLAYERS],
    pub density_graph_top_scale_y: [f32; MAX_PLAYERS],

    pub hold_to_exit_key: Option<HoldToExitKey>,
    pub hold_to_exit_start: Option<Instant>,
    pub hold_to_exit_aborted_at: Option<Instant>,
    pub exit_transition: Option<ExitTransition>,
    shift_held: bool,
    ctrl_held: bool,
    offset_adjust_held_since: [Option<Instant>; 2],
    offset_adjust_last_at: [Option<Instant>; 2],
    prev_inputs: [bool; MAX_COLS],
    keyboard_lane_counts: [u8; MAX_COLS],
    gamepad_lane_counts: [u8; MAX_COLS],
    lane_pressed_since_ns: [Option<SongTimeNs>; MAX_COLS],
    pending_edges: VecDeque<InputEdge>,
    autoplay_rng: TurnRng,
    autoplay_cursor: [usize; MAX_PLAYERS],
    tick_mode: TickMode,
    assist_clap_rows: Vec<usize>,
    assist_clap_cursor: usize,
    assist_last_crossed_row: i32,
    toggle_flash_text: Option<&'static str>,
    toggle_flash_timer: f32,
    replay_input: Vec<RecordedLaneEdge>,
    replay_cursor: usize,
    pub replay_edges: Vec<RecordedLaneEdge>,

    update_trace: GameplayUpdateTraceState,
}

impl GameplayUpdateTraceState {
    #[inline(always)]
    fn from_state(state: &State) -> Self {
        let mut trace = Self::default();
        trace.pending_edges_capacity = state.pending_edges.capacity();
        trace.replay_edges_capacity = state.replay_edges.capacity();
        trace.decaying_hold_capacity = state.decaying_hold_indices.capacity();
        for player in 0..state.num_players.min(MAX_PLAYERS) {
            trace.density_life_capacity[player] =
                state.density_graph_life_points[player].capacity();
        }
        trace
    }
}

#[inline(always)]
fn elapsed_us_since(started: Instant) -> u32 {
    let elapsed = started.elapsed().as_micros();
    if elapsed > u128::from(u32::MAX) {
        u32::MAX
    } else {
        elapsed as u32
    }
}

#[inline(always)]
fn elapsed_us_between(later: Instant, earlier: Instant) -> u32 {
    let elapsed = later
        .checked_duration_since(earlier)
        .unwrap_or(Duration::ZERO)
        .as_micros();
    if elapsed > u128::from(u32::MAX) {
        u32::MAX
    } else {
        elapsed as u32
    }
}

#[inline(always)]
fn add_elapsed_us(dst: &mut u32, started: Instant) {
    *dst = dst.saturating_add(elapsed_us_since(started));
}

#[inline(always)]
fn max_phase_name_and_us(phases: &GameplayUpdatePhaseTimings) -> (&'static str, u32) {
    let mut best = ("pre_notes", phases.pre_notes_us);
    if phases.autoplay_us > best.1 {
        best = ("autoplay", phases.autoplay_us);
    }
    if phases.input_edges_us > best.1 {
        best = ("input_edges", phases.input_edges_us);
    }
    if phases.held_mines_us > best.1 {
        best = ("held_mines", phases.held_mines_us);
    }
    if phases.active_holds_us > best.1 {
        best = ("active_holds", phases.active_holds_us);
    }
    if phases.hold_decay_us > best.1 {
        best = ("hold_decay", phases.hold_decay_us);
    }
    if phases.visuals_us > best.1 {
        best = ("visuals", phases.visuals_us);
    }
    if phases.spawn_arrows_us > best.1 {
        best = ("spawn_arrows", phases.spawn_arrows_us);
    }
    if phases.mine_avoid_us > best.1 {
        best = ("mine_avoid", phases.mine_avoid_us);
    }
    if phases.tap_miss_us > best.1 {
        best = ("tap_miss", phases.tap_miss_us);
    }
    if phases.cull_us > best.1 {
        best = ("cull", phases.cull_us);
    }
    if phases.judged_rows_us > best.1 {
        best = ("judged_rows", phases.judged_rows_us);
    }
    if phases.density_us > best.1 {
        best = ("density", phases.density_us);
    }
    if phases.danger_us > best.1 {
        best = ("danger", phases.danger_us);
    }
    if phases.untracked_us > best.1 {
        best = ("untracked", phases.untracked_us);
    }
    best
}

#[inline(always)]
fn accumulate_phase_max(dst: &mut GameplayUpdatePhaseTimings, src: &GameplayUpdatePhaseTimings) {
    dst.pre_notes_us = dst.pre_notes_us.max(src.pre_notes_us);
    dst.autoplay_us = dst.autoplay_us.max(src.autoplay_us);
    dst.input_edges_us = dst.input_edges_us.max(src.input_edges_us);
    dst.input_queue_us = dst.input_queue_us.max(src.input_queue_us);
    dst.input_state_us = dst.input_state_us.max(src.input_state_us);
    dst.input_glow_us = dst.input_glow_us.max(src.input_glow_us);
    dst.input_judge_us = dst.input_judge_us.max(src.input_judge_us);
    dst.input_roll_us = dst.input_roll_us.max(src.input_roll_us);
    dst.held_mines_us = dst.held_mines_us.max(src.held_mines_us);
    dst.active_holds_us = dst.active_holds_us.max(src.active_holds_us);
    dst.hold_decay_us = dst.hold_decay_us.max(src.hold_decay_us);
    dst.visuals_us = dst.visuals_us.max(src.visuals_us);
    dst.spawn_arrows_us = dst.spawn_arrows_us.max(src.spawn_arrows_us);
    dst.mine_avoid_us = dst.mine_avoid_us.max(src.mine_avoid_us);
    dst.tap_miss_us = dst.tap_miss_us.max(src.tap_miss_us);
    dst.cull_us = dst.cull_us.max(src.cull_us);
    dst.judged_rows_us = dst.judged_rows_us.max(src.judged_rows_us);
    dst.density_us = dst.density_us.max(src.density_us);
    dst.density_sample_us = dst.density_sample_us.max(src.density_sample_us);
    dst.danger_us = dst.danger_us.max(src.danger_us);
    dst.untracked_us = dst.untracked_us.max(src.untracked_us);
}

#[inline(always)]
fn tracked_phase_total_us(phases: &GameplayUpdatePhaseTimings) -> u32 {
    phases
        .pre_notes_us
        .saturating_add(phases.autoplay_us)
        .saturating_add(phases.input_edges_us)
        .saturating_add(phases.held_mines_us)
        .saturating_add(phases.active_holds_us)
        .saturating_add(phases.hold_decay_us)
        .saturating_add(phases.visuals_us)
        .saturating_add(phases.spawn_arrows_us)
        .saturating_add(phases.mine_avoid_us)
        .saturating_add(phases.tap_miss_us)
        .saturating_add(phases.cull_us)
        .saturating_add(phases.judged_rows_us)
        .saturating_add(phases.density_us)
        .saturating_add(phases.danger_us)
}

fn trace_capacity_growth(state: &mut State) {
    let num_players = state.num_players.min(MAX_PLAYERS);
    let frame = state.update_trace.frame_counter;
    let pending_cap = state.pending_edges.capacity();
    if pending_cap > state.update_trace.pending_edges_capacity {
        debug!(
            "Gameplay vec growth frame={frame}: pending_edges capacity {} -> {} (len={})",
            state.update_trace.pending_edges_capacity,
            pending_cap,
            state.pending_edges.len()
        );
        state.update_trace.pending_edges_capacity = pending_cap;
    }
    let replay_cap = state.replay_edges.capacity();
    if replay_cap > state.update_trace.replay_edges_capacity {
        debug!(
            "Gameplay vec growth frame={frame}: replay_edges capacity {} -> {} (len={})",
            state.update_trace.replay_edges_capacity,
            replay_cap,
            state.replay_edges.len()
        );
        state.update_trace.replay_edges_capacity = replay_cap;
    }
    let decaying_cap = state.decaying_hold_indices.capacity();
    if decaying_cap > state.update_trace.decaying_hold_capacity {
        debug!(
            "Gameplay vec growth frame={frame}: decaying_hold_indices capacity {} -> {} (len={})",
            state.update_trace.decaying_hold_capacity,
            decaying_cap,
            state.decaying_hold_indices.len()
        );
        state.update_trace.decaying_hold_capacity = decaying_cap;
    }
    for player in 0..num_players {
        let new_cap = state.density_graph_life_points[player].capacity();
        let old_cap = state.update_trace.density_life_capacity[player];
        if new_cap > old_cap {
            debug!(
                "Gameplay vec growth frame={frame}: density_graph_life_points[{player}] capacity {old_cap} -> {new_cap} (len={})",
                state.density_graph_life_points[player].len()
            );
            state.update_trace.density_life_capacity[player] = new_cap;
        }
    }
}

fn trace_gameplay_update(
    state: &mut State,
    delta_time: f32,
    music_time_sec: f32,
    total_us: u32,
    mut phases: GameplayUpdatePhaseTimings,
) {
    phases.untracked_us = total_us.saturating_sub(tracked_phase_total_us(&phases));
    let pending_len = state.pending_edges.len();
    let replay_edges_len = state.replay_edges.len();
    let decaying_len = state.decaying_hold_indices.len();
    let frame_counter = {
        let trace_state = &mut state.update_trace;
        trace_state.frame_counter = trace_state.frame_counter.wrapping_add(1);
        trace_state.summary_elapsed_s += delta_time.max(0.0);
        trace_state.summary_frames = trace_state.summary_frames.saturating_add(1);
        trace_state.summary_max_total_us = trace_state.summary_max_total_us.max(total_us);
        accumulate_phase_max(&mut trace_state.summary_max_phase, &phases);
        trace_state.summary_peak_pending_edges =
            trace_state.summary_peak_pending_edges.max(pending_len);
        trace_state.frame_counter
    };

    if pending_len >= GAMEPLAY_INPUT_BACKLOG_WARN {
        debug!(
            "Gameplay input backlog: frame={}, pending_edges={}, replay_edges={}",
            frame_counter, pending_len, replay_edges_len
        );
    }

    let (hot_name, hot_us) = max_phase_name_and_us(&phases);
    let is_slow =
        total_us >= GAMEPLAY_TRACE_SLOW_FRAME_US || hot_us >= GAMEPLAY_TRACE_PHASE_SPIKE_US;
    if is_slow {
        state.update_trace.summary_slow_frames =
            state.update_trace.summary_slow_frames.saturating_add(1);
        debug!(
            "Gameplay slow frame={} t={:.3}s total={:.3}ms hot={}({:.3}ms) pending={} decays={} phases_ms=[pre:{:.3} auto:{:.3} input:{:.3} held:{:.3} holds:{:.3} decay:{:.3} vis:{:.3} spawn:{:.3} mine:{:.3} tmiss:{:.3} cull:{:.3} judged:{:.3} density:{:.3} danger:{:.3} other:{:.3}] input_sub_ms=[queue:{:.3} state:{:.3} glow:{:.3} judge:{:.3} roll:{:.3}] density_sub_ms=[sample:{:.3}]",
            frame_counter,
            music_time_sec,
            total_us as f32 / 1000.0,
            hot_name,
            hot_us as f32 / 1000.0,
            pending_len,
            decaying_len,
            phases.pre_notes_us as f32 / 1000.0,
            phases.autoplay_us as f32 / 1000.0,
            phases.input_edges_us as f32 / 1000.0,
            phases.held_mines_us as f32 / 1000.0,
            phases.active_holds_us as f32 / 1000.0,
            phases.hold_decay_us as f32 / 1000.0,
            phases.visuals_us as f32 / 1000.0,
            phases.spawn_arrows_us as f32 / 1000.0,
            phases.mine_avoid_us as f32 / 1000.0,
            phases.tap_miss_us as f32 / 1000.0,
            phases.cull_us as f32 / 1000.0,
            phases.judged_rows_us as f32 / 1000.0,
            phases.density_us as f32 / 1000.0,
            phases.danger_us as f32 / 1000.0,
            phases.untracked_us as f32 / 1000.0,
            phases.input_queue_us as f32 / 1000.0,
            phases.input_state_us as f32 / 1000.0,
            phases.input_glow_us as f32 / 1000.0,
            phases.input_judge_us as f32 / 1000.0,
            phases.input_roll_us as f32 / 1000.0,
            phases.density_sample_us as f32 / 1000.0
        );
    }

    if log::log_enabled!(log::Level::Trace)
        && state.update_trace.summary_elapsed_s >= GAMEPLAY_TRACE_SUMMARY_INTERVAL_S
    {
        let summary_frames = state.update_trace.summary_frames;
        let summary_slow_frames = state.update_trace.summary_slow_frames;
        let summary_max_total_us = state.update_trace.summary_max_total_us;
        let summary_max_phase = state.update_trace.summary_max_phase;
        let summary_input_latency = state.update_trace.summary_input_latency;
        let summary_peak_pending_edges = state.update_trace.summary_peak_pending_edges;
        let (summary_hot_name, summary_hot_us) = max_phase_name_and_us(&summary_max_phase);
        trace!(
            "Gameplay trace summary: frames={} slow={} max_total={:.3}ms max_hot={}({:.3}ms) peak_pending={} input_sub_max_ms=[queue:{:.3} state:{:.3} glow:{:.3} judge:{:.3} roll:{:.3}] input_latency_us=[samples:{} cap_store_avg:{:.1} cap_store_max:{} store_emit_avg:{:.1} store_emit_max:{} emit_queue_avg:{:.1} emit_queue_max:{} queue_proc_avg:{:.1} queue_proc_max:{} cap_proc_avg:{:.1} cap_proc_max:{}] density_sub_max_ms=[sample:{:.3}] other_max={:.3}",
            summary_frames,
            summary_slow_frames,
            summary_max_total_us as f32 / 1000.0,
            summary_hot_name,
            summary_hot_us as f32 / 1000.0,
            summary_peak_pending_edges,
            summary_max_phase.input_queue_us as f32 / 1000.0,
            summary_max_phase.input_state_us as f32 / 1000.0,
            summary_max_phase.input_glow_us as f32 / 1000.0,
            summary_max_phase.input_judge_us as f32 / 1000.0,
            summary_max_phase.input_roll_us as f32 / 1000.0,
            summary_input_latency.samples,
            GameplayInputLatencyTrace::avg_us(
                summary_input_latency.capture_to_store_total_us,
                summary_input_latency.samples,
            ),
            summary_input_latency.capture_to_store_max_us,
            GameplayInputLatencyTrace::avg_us(
                summary_input_latency.store_to_emit_total_us,
                summary_input_latency.samples,
            ),
            summary_input_latency.store_to_emit_max_us,
            GameplayInputLatencyTrace::avg_us(
                summary_input_latency.emit_to_queue_total_us,
                summary_input_latency.samples,
            ),
            summary_input_latency.emit_to_queue_max_us,
            GameplayInputLatencyTrace::avg_us(
                summary_input_latency.queue_to_process_total_us,
                summary_input_latency.samples,
            ),
            summary_input_latency.queue_to_process_max_us,
            GameplayInputLatencyTrace::avg_us(
                summary_input_latency.capture_to_process_total_us,
                summary_input_latency.samples,
            ),
            summary_input_latency.capture_to_process_max_us,
            summary_max_phase.density_sample_us as f32 / 1000.0,
            summary_max_phase.untracked_us as f32 / 1000.0
        );
        state.update_trace.summary_elapsed_s = 0.0;
        state.update_trace.summary_frames = 0;
        state.update_trace.summary_slow_frames = 0;
        state.update_trace.summary_max_total_us = 0;
        state.update_trace.summary_max_phase = GameplayUpdatePhaseTimings::default();
        state.update_trace.summary_input_latency = GameplayInputLatencyTrace::default();
        state.update_trace.summary_peak_pending_edges = 0;
    }

    trace_capacity_growth(state);
}

#[cfg(test)]
fn assert_valid_hot_state_for_tests(state: &State, delta_time: f32, music_time_sec: f32) {
    debug_assert!(
        delta_time.is_finite() && delta_time >= 0.0,
        "invalid delta_time={delta_time}"
    );
    debug_assert!(
        music_time_sec.is_finite(),
        "invalid music_time_sec={music_time_sec}"
    );
    debug_assert!(
        state.num_players > 0 && state.num_players <= MAX_PLAYERS,
        "invalid num_players={}",
        state.num_players
    );
    debug_assert!(
        state.num_cols > 0 && state.num_cols <= MAX_COLS,
        "invalid num_cols={}",
        state.num_cols
    );
    debug_assert!(
        state.cols_per_player > 0 && state.cols_per_player <= MAX_COLS,
        "invalid cols_per_player={}",
        state.cols_per_player
    );
    debug_assert_eq!(state.notes.len(), state.note_time_cache_ns.len());
    debug_assert_eq!(state.notes.len(), state.note_display_beat_cache.len());
    debug_assert_eq!(state.notes.len(), state.hold_end_time_cache_ns.len());
    debug_assert_eq!(state.notes.len(), state.hold_end_display_beat_cache.len());
    debug_assert_eq!(state.notes.len(), state.hold_display_beat_min_cache.len());
    debug_assert_eq!(state.notes.len(), state.hold_display_beat_max_cache.len());
    debug_assert_eq!(state.notes.len(), state.hold_decay_active.len());
    debug_assert_eq!(state.notes.len(), state.note_row_entry_indices.len());
    for player in 0..state.num_players {
        let (start, end) = state.note_ranges[player];
        debug_assert!(start <= end && end <= state.notes.len());
        let (row_start, row_end) = state.row_entry_ranges[player];
        debug_assert!(row_start <= row_end && row_end <= state.row_entries.len());
        debug_assert!(
            state.judged_row_cursor[player] >= row_start
                && state.judged_row_cursor[player] <= row_end
        );
        debug_assert!(
            state.next_tap_miss_cursor[player] >= start
                && state.next_tap_miss_cursor[player] <= end
        );
        debug_assert!(
            state.next_mine_avoid_cursor[player] >= start
                && state.next_mine_avoid_cursor[player] <= end
        );
        debug_assert_eq!(
            state.mine_note_ix[player].len(),
            state.mine_note_time_ns[player].len()
        );
        debug_assert!(state.next_mine_ix_cursor[player] <= state.mine_note_ix[player].len());
    }
    for player in 0..state.num_players {
        let (start, end) = state.note_ranges[player];
        debug_assert!(
            state.mine_note_time_ns[player]
                .windows(2)
                .all(|pair| pair[0] <= pair[1])
        );
        for &note_index in &state.mine_note_ix[player] {
            debug_assert!(note_index >= start && note_index < end);
            debug_assert!(matches!(state.notes[note_index].note_type, NoteType::Mine));
        }
    }
    for note in &state.notes {
        if note.can_be_judged && !matches!(note.note_type, NoteType::Mine) {
            let player = player_for_col(state, note.column);
            debug_assert!(
                row_entry_for_cached_row(
                    &state.row_entries,
                    &state.row_map_cache[player],
                    note.row_index
                )
                .is_some()
            );
        }
    }
    for (row_entry_index, row_entry) in state.row_entries.iter().enumerate() {
        let first_note_index = row_entry.nonmine_note_indices[0];
        let player = player_for_col(state, state.notes[first_note_index].column);
        debug_assert!(
            row_entry_index >= state.row_entry_ranges[player].0
                && row_entry_index < state.row_entry_ranges[player].1
        );
        debug_assert_eq!(
            state.row_map_cache[player]
                .get(row_entry.row_index)
                .copied(),
            Some(row_entry_index as u32)
        );
        for &note_index in &row_entry.nonmine_note_indices {
            debug_assert!(note_index < state.notes.len());
            debug_assert_eq!(
                state.note_row_entry_indices[note_index],
                row_entry_index as u32
            );
            let note = &state.notes[note_index];
            debug_assert_eq!(note.row_index, row_entry.row_index);
            debug_assert!(note.can_be_judged);
            debug_assert!(!note.is_fake);
            debug_assert!(!matches!(note.note_type, NoteType::Mine));
        }
    }
    for col in 0..state.num_cols {
        debug_assert!(state.column_scroll_dirs[col].is_finite());
        debug_assert!(state.lane_note_indices[col].windows(2).all(|pair| {
            let left = pair[0];
            let right = pair[1];
            left < right && state.note_time_cache_ns[left] <= state.note_time_cache_ns[right]
        }));
        let note_runs = &state.lane_note_display_runs[col];
        if state.lane_note_indices[col].is_empty() {
            debug_assert!(note_runs.is_empty());
        } else {
            debug_assert_eq!(note_runs.first().map(|run| run.start), Some(0));
            debug_assert_eq!(
                note_runs.last().map(|run| run.end),
                Some(state.lane_note_indices[col].len())
            );
            debug_assert!(note_runs.iter().all(|run| run.start < run.end));
            debug_assert!(
                note_runs
                    .windows(2)
                    .all(|pair| pair[0].end == pair[1].start)
            );
            for run in note_runs {
                debug_assert!(
                    state.lane_note_indices[col][run.start..run.end]
                        .windows(2)
                        .all(|pair| {
                            state.note_display_beat_cache[pair[0]]
                                <= state.note_display_beat_cache[pair[1]]
                        })
                );
            }
        }
        for &note_index in &state.lane_note_indices[col] {
            debug_assert!(note_index < state.notes.len());
            debug_assert_eq!(state.notes[note_index].column, col);
        }
        debug_assert!(state.lane_hold_indices[col].windows(2).all(|pair| {
            let left = pair[0];
            let right = pair[1];
            left < right && state.note_time_cache_ns[left] <= state.note_time_cache_ns[right]
        }));
        let hold_runs = &state.lane_hold_display_runs[col];
        if state.lane_hold_indices[col].is_empty() {
            debug_assert!(hold_runs.is_empty());
        } else {
            debug_assert_eq!(hold_runs.first().map(|run| run.start), Some(0));
            debug_assert_eq!(
                hold_runs.last().map(|run| run.end),
                Some(state.lane_hold_indices[col].len())
            );
            debug_assert!(hold_runs.iter().all(|run| run.start < run.end));
            debug_assert!(
                hold_runs
                    .windows(2)
                    .all(|pair| pair[0].end == pair[1].start)
            );
            for run in hold_runs {
                debug_assert!(
                    state.lane_hold_indices[col][run.start..run.end]
                        .windows(2)
                        .all(|pair| {
                            let left = pair[0];
                            let right = pair[1];
                            state.hold_display_beat_min_cache[left]
                                .zip(state.hold_display_beat_min_cache[right])
                                .is_some_and(|(lhs, rhs)| lhs <= rhs)
                                && state.hold_display_beat_max_cache[left]
                                    .zip(state.hold_display_beat_max_cache[right])
                                    .is_some_and(|(lhs, rhs)| lhs <= rhs)
                        })
                );
            }
        }
        for &note_index in &state.lane_hold_indices[col] {
            debug_assert!(note_index < state.notes.len());
            debug_assert_eq!(state.notes[note_index].column, col);
            debug_assert!(matches!(
                state.notes[note_index].note_type,
                NoteType::Hold | NoteType::Roll
            ));
        }
    }
    for col in state.num_cols..MAX_COLS {
        debug_assert!(state.lane_note_indices[col].is_empty());
        debug_assert!(state.lane_hold_indices[col].is_empty());
        debug_assert!(state.lane_note_display_runs[col].is_empty());
        debug_assert!(state.lane_hold_display_runs[col].is_empty());
    }
    let mut lane_positions = [0usize; MAX_COLS];
    for (note_index, note) in state.notes.iter().enumerate() {
        if note.column >= state.num_cols {
            continue;
        }
        let lane_pos = lane_positions[note.column];
        debug_assert_eq!(
            state.lane_note_indices[note.column].get(lane_pos).copied(),
            Some(note_index)
        );
        lane_positions[note.column] += 1;
    }
    for col in 0..state.num_cols {
        debug_assert_eq!(lane_positions[col], state.lane_note_indices[col].len());
    }
}

#[inline(always)]
fn finalize_update_trace(
    state: &mut State,
    delta_time: f32,
    music_time_sec: f32,
    frame_trace_started: Option<Instant>,
    phase_timings: GameplayUpdatePhaseTimings,
) {
    let Some(started) = frame_trace_started else {
        return;
    };
    let total_us = elapsed_us_since(started);
    trace_gameplay_update(state, delta_time, music_time_sec, total_us, phase_timings);
}

#[inline(always)]
fn approach_f32(current: &mut f32, target: f32, step: f32) {
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

#[inline(always)]
pub fn scroll_receptor_y(
    reverse_percent: f32,
    centered_percent: f32,
    normal_y: f32,
    reverse_y: f32,
    centered_y: f32,
) -> f32 {
    let reverse_y = lerp(normal_y, reverse_y, reverse_percent.clamp(0.0, 1.0));
    lerp(reverse_y, centered_y, centered_percent.clamp(0.0, 1.0))
}

fn refresh_live_notefield_options(state: &mut State, current_bpm: f32) {
    for player in 0..state.num_players {
        let scroll = effective_scroll_effects_for_player(state, player);
        state.reverse_scroll[player] =
            scroll.reverse_percent_for_column(0, state.cols_per_player) > 0.5;
        let start = player.saturating_mul(state.cols_per_player);
        let end = (start + state.cols_per_player)
            .min(state.num_cols)
            .min(MAX_COLS);
        for (local_col, col) in (start..end).enumerate() {
            state.column_scroll_dirs[col] =
                scroll.reverse_scale_for_column(local_col, state.cols_per_player);
        }
    }
    for player in 0..state.num_players {
        let scroll_speed = effective_scroll_speed_for_player(state, player);
        let mut dynamic_speed = scroll_speed.pixels_per_second(
            current_bpm,
            state.scroll_reference_bpm,
            state.music_rate,
        );
        if !dynamic_speed.is_finite() || dynamic_speed <= 0.0 {
            dynamic_speed = ScrollSpeedSetting::default().pixels_per_second(
                current_bpm,
                state.scroll_reference_bpm,
                state.music_rate,
            );
        }
        state.scroll_pixels_per_second[player] = dynamic_speed;

        let scroll = effective_scroll_effects_for_player(state, player);
        let visual_mask = effective_visual_mask_for_player(state, player);
        let mini_percent = effective_mini_percent_for_player(state, player);
        let mini = effective_mini_value_with_visual_mask(
            &state.player_profiles[player],
            visual_mask,
            mini_percent,
        );
        let mut field_zoom = 1.0 - mini * 0.5;
        if field_zoom.abs() < 0.01 {
            field_zoom = 0.01;
        }
        state.field_zoom[player] = field_zoom;

        let perspective = effective_perspective_effects_for_player(state, player);
        let draw_scale = player_draw_scale_for_tilt_with_visual_mask(
            perspective.tilt,
            &state.player_profiles[player],
            visual_mask,
            mini_percent,
        );
        state.draw_distance_before_targets[player] =
            screen_height() * DRAW_DISTANCE_BEFORE_TARGETS_MULTIPLIER * draw_scale;
        state.draw_distance_after_targets[player] = lerp(
            DRAW_DISTANCE_AFTER_TARGETS * draw_scale,
            screen_height() * 0.6 * draw_scale,
            scroll.centered.clamp(0.0, 1.0),
        );

        let mut travel_time = scroll_speed.travel_time_seconds(
            state.draw_distance_before_targets[player],
            current_bpm,
            state.scroll_reference_bpm,
            state.music_rate,
        );
        if !travel_time.is_finite() || travel_time <= 0.0 {
            travel_time =
                state.draw_distance_before_targets[player] / dynamic_speed.max(f32::EPSILON);
        }
        state.scroll_travel_time[player] = travel_time;
    }
}

const TOGGLE_FLASH_DURATION: f32 = 1.5;
const TOGGLE_FLASH_FADE_START: f32 = 0.8;

pub fn toggle_flash_text(state: &State) -> Option<(&'static str, f32)> {
    if state.toggle_flash_timer > 0.0 {
        let age = TOGGLE_FLASH_DURATION - state.toggle_flash_timer;
        let alpha = if age < TOGGLE_FLASH_FADE_START {
            1.0
        } else {
            let fade_len = TOGGLE_FLASH_DURATION - TOGGLE_FLASH_FADE_START;
            1.0 - ((age - TOGGLE_FLASH_FADE_START) / fade_len).clamp(0.0, 1.0)
        };
        state.toggle_flash_text.map(|t| (t, alpha))
    } else {
        None
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
fn autoplay_random_offset_music_ns_for_window(
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
fn live_autoplay_judgment_offset_music_ns(
    state: &mut State,
    player_idx: usize,
    window: TimingWindow,
    measured_offset_music_ns: SongTimeNs,
) -> SongTimeNs {
    if !live_autoplay_enabled(state) {
        return measured_offset_music_ns;
    }
    let timing_profile = if player_idx < state.num_players {
        state.player_judgment_timing[player_idx].profile_music_ns
    } else {
        TimingProfileNs::from_profile_scaled(&state.timing_profile, state.music_rate)
    };
    autoplay_random_offset_music_ns_for_window(&mut state.autoplay_rng, timing_profile, window)
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
const fn exit_total_seconds(kind: ExitTransitionKind) -> f32 {
    match kind {
        ExitTransitionKind::Out => GIVE_UP_OUT_TOTAL_SECONDS,
        ExitTransitionKind::Cancel => BACK_OUT_TOTAL_SECONDS,
    }
}

#[inline(always)]
pub fn exit_transition_alpha(exit: &ExitTransition) -> f32 {
    let t = exit.started_at.elapsed().as_secs_f32();
    let (delay, fade) = match exit.kind {
        ExitTransitionKind::Out => (GIVE_UP_OUT_FADE_DELAY_SECONDS, GIVE_UP_OUT_FADE_SECONDS),
        ExitTransitionKind::Cancel => (BACK_OUT_FADE_DELAY_SECONDS, BACK_OUT_FADE_SECONDS),
    };
    if fade <= 0.0 {
        return 1.0;
    }
    let a = if t <= delay { 0.0 } else { (t - delay) / fade };
    a.clamp(0.0, 1.0)
}

#[inline(always)]
fn abort_hold_to_exit(state: &mut State, at: Instant) {
    if state.hold_to_exit_start.is_some() {
        state.hold_to_exit_key = None;
        state.hold_to_exit_start = None;
        state.hold_to_exit_aborted_at = Some(at);
    }
}

#[inline(always)]
const fn gameplay_exit_for_kind(kind: ExitTransitionKind) -> GameplayExit {
    match kind {
        ExitTransitionKind::Out => GameplayExit::Complete,
        ExitTransitionKind::Cancel => GameplayExit::Cancel,
    }
}

#[inline(always)]
fn begin_exit_transition(state: &mut State, kind: ExitTransitionKind) {
    if state.exit_transition.is_some() {
        return;
    }
    state.hold_to_exit_key = None;
    state.hold_to_exit_start = None;
    state.hold_to_exit_aborted_at = None;
    state.exit_transition = Some(ExitTransition {
        kind,
        started_at: Instant::now(),
    });
    audio::stop_music();
}

pub fn danger_overlay_rgba(state: &State, player: usize) -> Option<[f32; 4]> {
    if player >= state.num_players {
        return None;
    }
    if state.player_profiles[player].hide_lifebar {
        return None;
    }
    let rgba = danger_anim_rgba(&state.danger_fx[player].anim, state.total_elapsed_in_screen);
    if rgba[3] > 0.0 { Some(rgba) } else { None }
}

#[inline(always)]
fn player_for_col(state: &State, col: usize) -> usize {
    if state.num_players <= 1 || state.cols_per_player == 0 {
        return 0;
    }
    (col / state.cols_per_player).min(state.num_players.saturating_sub(1))
}

#[inline(always)]
const fn player_col_range(state: &State, player: usize) -> (usize, usize) {
    let start = player * state.cols_per_player;
    (start, start + state.cols_per_player)
}

const STEP_CAL_JUMP_WINDOW_S: f32 = 0.25;

#[inline(always)]
fn scale_range(v: f32, in_lo: f32, in_hi: f32, out_lo: f32, out_hi: f32) -> f32 {
    if (in_hi - in_lo).abs() <= f32::EPSILON {
        return out_lo;
    }
    out_lo + (v - in_lo) * (out_hi - out_lo) / (in_hi - in_lo)
}

#[inline(always)]
fn step_calories(weight_pounds: i32, tracks_held: usize) -> f32 {
    let tracks = tracks_held.max(1) as f32;
    let cals_100 = scale_range(tracks, 1.0, 2.0, 0.023, 0.077);
    let cals_200 = scale_range(tracks, 1.0, 2.0, 0.041, 0.133);
    scale_range(
        weight_pounds.max(0) as f32,
        100.0,
        200.0,
        cals_100,
        cals_200,
    )
}

#[inline(always)]
fn recent_step_tracks(
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
fn record_step_calories(state: &mut State, lane_idx: usize, event_music_time_ns: SongTimeNs) {
    if song_time_ns_invalid(event_music_time_ns) {
        return;
    }
    let player = player_for_col(state, lane_idx);
    let (start, end) = player_col_range(state, player);
    let tracks = recent_step_tracks(
        &state.lane_pressed_since_ns,
        start,
        end,
        event_music_time_ns,
    );
    let weight_pounds = state.player_profiles[player].calculated_weight_pounds();
    state.players[player].calories_burned += step_calories(weight_pounds, tracks);
}

#[inline(always)]
fn player_note_range(state: &State, player: usize) -> (usize, usize) {
    if player >= state.num_players {
        return (0, 0);
    }
    state.note_ranges[player]
}

#[inline(always)]
fn push_density_life_point(points: &mut Vec<[f32; 2]>, x: f32, y: f32) -> bool {
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

fn update_density_graph(
    state: &mut State,
    current_music_time: f32,
    trace_enabled: bool,
    phase_timings: &mut GameplayUpdatePhaseTimings,
) {
    let graph_w = state.density_graph_graph_w;
    let graph_h = state.density_graph_graph_h;
    let scaled_width = state.density_graph_scaled_width;
    if graph_w <= 0.0_f32 || graph_h <= 0.0_f32 || scaled_width <= 0.0_f32 {
        state.density_graph_u0 = 0.0_f32;
        return;
    }

    let duration = state.density_graph_duration.max(0.001_f32);
    let u_window = state.density_graph_u_window.clamp(0.0_f32, 1.0_f32);
    let max_u0 = (1.0_f32 - u_window).max(0.0_f32);
    let mut u0 = 0.0_f32;

    if max_u0 > 0.0_f32 {
        let max_seconds = (u_window * duration).max(0.0_f32);
        if max_seconds > 0.0_f32 {
            let first_second = state.density_graph_first_second;
            let last_second = state.density_graph_last_second;
            if current_music_time > last_second - (max_seconds * 0.75_f32) {
                u0 = max_u0;
            } else {
                let seconds_past_one_fourth =
                    (current_music_time - first_second) - (max_seconds * 0.25_f32);
                if seconds_past_one_fourth > 0.0_f32 {
                    u0 = (seconds_past_one_fourth / duration).clamp(0.0_f32, max_u0);
                }
            }
        }
    }

    state.density_graph_u0 = u0;

    let next_t = state.density_graph_life_next_update_elapsed;
    if state.density_graph_life_update_rate > 0.0_f32 && state.total_elapsed_in_screen >= next_t {
        let sample_started = if trace_enabled {
            Some(Instant::now())
        } else {
            None
        };
        let rate = state.density_graph_life_update_rate;
        let elapsed = (state.total_elapsed_in_screen - next_t).max(0.0_f32);
        let mut catch_up_steps = ((elapsed / rate).floor() as u32).saturating_add(1);
        if catch_up_steps > 64 {
            catch_up_steps = 64;
        }
        state.density_graph_life_next_update_elapsed += rate * catch_up_steps as f32;

        if current_music_time > 0.0_f32 && current_music_time <= state.density_graph_last_second {
            let denom = state.density_graph_duration.max(0.001_f32);
            let x = (((current_music_time - state.density_graph_first_second) / denom)
                * state.density_graph_scaled_width)
                .clamp(0.0_f32, state.density_graph_scaled_width);
            if x.is_finite() {
                for player in 0..state.num_players {
                    let life = state.players[player].life;
                    let y = (1.0_f32 - life).clamp(0.0_f32, 1.0_f32) * graph_h;
                    let points = &mut state.density_graph_life_points[player];
                    if push_density_life_point(points, x, y) {
                        state.density_graph_life_dirty[player] = true;
                    }
                }
            }
        }
        if let Some(started) = sample_started {
            add_elapsed_us(&mut phase_timings.density_sample_us, started);
        }
    }
}

#[inline(always)]
fn stream_pos_to_music_time(state: &State, stream_pos: f32) -> f32 {
    let rate = if state.music_rate.is_finite() && state.music_rate > 0.0 {
        state.music_rate
    } else {
        1.0
    };
    let lead_in = state.audio_lead_in_seconds.max(0.0);
    let anchor = -state.global_offset_seconds;
    (stream_pos - lead_in).mul_add(rate, anchor * (1.0 - rate))
}

#[inline(always)]
fn stage_music_cut(lead_in_seconds: f32) -> audio::Cut {
    audio::Cut {
        start_sec: f64::from(-lead_in_seconds.max(0.0)),
        length_sec: f64::INFINITY,
        ..Default::default()
    }
}

fn start_stage_music_audio(state: &State) {
    let Some(music_path) = state.charts[0].music_path.as_ref() else {
        return;
    };
    let lead_in = state.audio_lead_in_seconds.max(0.0);
    let rate = if state.music_rate.is_finite() && state.music_rate > 0.0 {
        state.music_rate
    } else {
        1.0
    };
    debug!("Starting music with a preroll delay of {lead_in:.2}s");
    audio::play_music(music_path.clone(), stage_music_cut(lead_in), false, rate);
}

pub fn start_stage_music(state: &mut State) {
    let start_time = -state.audio_lead_in_seconds.max(0.0);
    state.current_music_time_ns = song_time_ns_from_seconds(start_time);
    let display_time_ns = state.display_clock.reset(state.current_music_time_ns);
    state.current_music_time_display = song_time_ns_to_seconds(display_time_ns);
    state.current_beat = state
        .timing
        .get_beat_for_time_ns(state.current_music_time_ns);
    state.current_beat_display = state.timing.get_beat_for_time_ns(display_time_ns);
    for player in 0..state.num_players {
        let delay = state.global_visual_delay_seconds + state.player_visual_delay_seconds[player];
        let visible_time_ns = song_time_ns_add_seconds(display_time_ns, -delay);
        state.current_music_time_visible_ns[player] = visible_time_ns;
        state.current_music_time_visible[player] = song_time_ns_to_seconds(visible_time_ns);
        state.current_beat_visible[player] =
            state.timing_players[player].get_beat_for_time_ns(visible_time_ns);
    }
    state.total_elapsed_in_screen = 0.0;
    start_stage_music_audio(state);
}

fn get_reference_bpm_from_display_tag(
    chart: &ChartData,
    song_display_bpm_str: &str,
) -> Option<f32> {
    // 1. Try chart-level display BPM
    match &chart.display_bpm {
        Some(crate::game::chart::ChartDisplayBpm::Specified { max, .. }) => {
            let v = *max as f32;
            if v.is_finite() && v > 0.0 {
                return Some(v);
            }
        }
        Some(crate::game::chart::ChartDisplayBpm::Random) => return None,
        None => {}
    }
    // 2. Fall back to song-level display BPM string
    let s = song_display_bpm_str.trim();
    if s.is_empty() || s == "*" {
        return None;
    }
    if let Some((_, max_str)) = s.split_once(':') {
        return max_str.trim().parse::<f32>().ok();
    }
    s.parse::<f32>().ok()
}

fn song_lua_display_bpm_pair(song: &SongData, chart: Option<&ChartData>) -> [f32; 2] {
    song.chart_display_bpm_range(chart)
        .map(|(lo, hi)| {
            let lo = lo as f32;
            let hi = hi as f32;
            if lo.is_finite() && hi.is_finite() && lo > 0.0 && hi > 0.0 {
                [lo, hi]
            } else {
                [60.0, 60.0]
            }
        })
        .unwrap_or([60.0, 60.0])
}

fn step_stats_notefield_width(
    noteskin: Option<&Noteskin>,
    cols_per_player: usize,
    field_zoom: f32,
) -> Option<f32> {
    let ns = noteskin?;
    let cols = cols_per_player
        .min(ns.column_xs.len())
        .min(ns.receptor_off.len());
    if cols == 0 {
        return None;
    }

    let mut min_x = f32::INFINITY;
    let mut max_x = f32::NEG_INFINITY;
    for x in ns.column_xs.iter().take(cols) {
        let xf = *x as f32;
        min_x = min_x.min(xf);
        max_x = max_x.max(xf);
    }

    let zoom = field_zoom.max(0.0);
    let target_arrow_px = 64.0 * zoom;
    let size = ns.receptor_off[0].size();
    let w = size[0].max(0) as f32;
    let h = size[1].max(0) as f32;
    let arrow_w = if h > 0.0 && target_arrow_px > 0.0 {
        w * (target_arrow_px / h)
    } else {
        w * zoom
    };
    Some(((max_x - min_x) * zoom) + arrow_w)
}

fn upper_density_graph_width(play_style: profile::PlayStyle) -> f32 {
    // zmod UpperNPSGraph parity:
    //   width = GetNotefieldWidth()
    //   if OnePlayerTwoSides then width = width / 2
    //   width = width - 30
    let mut width = match play_style {
        profile::PlayStyle::Double => 512.0_f32,
        profile::PlayStyle::Single | profile::PlayStyle::Versus => 256.0_f32,
    };
    if play_style == profile::PlayStyle::Double {
        width *= 0.5_f32;
    }
    (width - 30.0_f32).max(0.0_f32)
}

pub fn init(
    song: Arc<SongData>,
    charts: [Arc<ChartData>; MAX_PLAYERS],
    gameplay_charts: [Arc<GameplayChartData>; MAX_PLAYERS],
    active_color_index: i32,
    music_rate: f32,
    mut scroll_speed: [ScrollSpeedSetting; MAX_PLAYERS],
    mut player_profiles: [profile::Profile; MAX_PLAYERS],
    replay_edges: Option<Vec<ReplayInputEdge>>,
    replay_offsets: Option<ReplayOffsetSnapshot>,
    replay_status_text: Option<Arc<str>>,
    stage_intro_text: Arc<str>,
    lead_in_timing: Option<LeadInTiming>,
    course_display_carry: Option<[CourseDisplayCarry; MAX_PLAYERS]>,
    course_display_totals: Option<[CourseDisplayTotals; MAX_PLAYERS]>,
    mut combo_carry: [u32; MAX_PLAYERS],
) -> State {
    debug!("Initializing Gameplay Screen...");
    let init_started = Instant::now();
    let rate = if music_rate.is_finite() && music_rate > 0.0 {
        music_rate
    } else {
        1.0
    };

    let play_style = profile::get_session_play_style();
    let player_side = profile::get_session_player_side();
    let p2_runtime_player = single_runtime_player_is_p2(play_style, player_side);
    let (cols_per_player, num_players, num_cols) = match play_style {
        profile::PlayStyle::Single => (4, 1, 4),
        profile::PlayStyle::Double => (8, 1, 8),
        profile::PlayStyle::Versus => (4, 2, 8),
    };
    let replay_edges = replay_edges.unwrap_or_default();
    let mut charts = charts;
    let mut gameplay_charts = gameplay_charts;
    if p2_runtime_player {
        scroll_speed[0] = scroll_speed[1];
        player_profiles[0] = player_profiles[1].clone();
        charts[0] = charts[1].clone();
        gameplay_charts[0] = gameplay_charts[1].clone();
        combo_carry[0] = combo_carry[1];
    }
    let player_color_index = if p2_runtime_player {
        active_color_index - 2
    } else {
        active_color_index
    };

    let style = Style {
        num_cols: cols_per_player,
        num_players: 1,
    };

    let noteskin: [Option<Arc<Noteskin>>; MAX_PLAYERS] = std::array::from_fn(|player| {
        if player >= num_players {
            return None;
        }
        let skin = player_profiles[player].noteskin.to_string();
        noteskin::load_itg_skin_cached(&style, &skin).ok()
    });
    let mine_noteskin: [Option<Arc<Noteskin>>; MAX_PLAYERS] = std::array::from_fn(|player| {
        if player >= num_players {
            return None;
        }
        let skin = player_profiles[player].resolved_mine_noteskin().to_string();
        noteskin::load_itg_skin_cached(&style, &skin)
            .ok()
            .or_else(|| noteskin[player].clone())
    });
    let receptor_noteskin: [Option<Arc<Noteskin>>; MAX_PLAYERS] = std::array::from_fn(|player| {
        if player >= num_players {
            return None;
        }
        let skin = player_profiles[player]
            .resolved_receptor_noteskin()
            .to_string();
        noteskin::load_itg_skin_cached(&style, &skin)
            .ok()
            .or_else(|| noteskin[player].clone())
    });
    let tap_explosion_noteskin: [Option<Arc<Noteskin>>; MAX_PLAYERS] =
        std::array::from_fn(|player| {
            if player >= num_players {
                return None;
            }
            let Some(skin) = player_profiles[player].resolved_tap_explosion_noteskin() else {
                return None;
            };
            noteskin::load_itg_skin_cached(&style, skin.as_str())
                .ok()
                .or_else(|| noteskin[player].clone())
        });
    let notefield_model_cache: [RefCell<ModelMeshCache>; MAX_PLAYERS] =
        std::array::from_fn(|player| {
            RefCell::new(if player < num_players {
                ModelMeshCache::with_capacity(96)
            } else {
                ModelMeshCache::default()
            })
        });

    let field_zoom: [f32; MAX_PLAYERS] = std::array::from_fn(|player| {
        if player >= num_players {
            return 1.0;
        }
        let mini_value = effective_mini_value(&player_profiles[player]);
        let mut z = 1.0 - mini_value * 0.5;
        if z.abs() < 0.01 {
            z = 0.01;
        }
        z
    });

    let config = crate::config::get();
    let song_full_title: Arc<str> = Arc::from(song.display_full_title(config.translated_titles));
    let pack_group: Arc<str> = Arc::from(
        song.simfile_path
            .parent()
            .and_then(|p| p.parent())
            .and_then(|p| p.file_name())
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_owned(),
    );
    let pack_banner_path: Option<PathBuf> = if pack_group.is_empty() {
        None
    } else {
        crate::game::song::get_song_cache()
            .iter()
            .find(|p| p.group_name == pack_group.as_ref())
            .and_then(|p| p.banner_path.clone())
    };
    let player_global_offset_shift_seconds: [f32; MAX_PLAYERS] = std::array::from_fn(|player| {
        if !config.machine_allow_per_player_global_offsets || player >= num_players {
            return 0.0;
        }
        player_profiles[player]
            .global_offset_shift_ms
            .clamp(-100, 100) as f32
            / 1000.0
    });
    let mut timing_base = gameplay_charts[0].timing.clone();
    timing_base.set_global_offset_seconds(config.global_offset_seconds);
    let timing = Arc::new(timing_base);
    let mut timing_players: [Arc<TimingData>; MAX_PLAYERS] = std::array::from_fn(|player| {
        let mut t = gameplay_charts[player].timing.clone();
        t.set_global_offset_seconds(
            config.global_offset_seconds + player_global_offset_shift_seconds[player],
        );
        Arc::new(t)
    });
    if num_players == 1 {
        timing_players[1] = timing_players[0].clone();
    }
    let mut replay_input = Vec::with_capacity(replay_edges.len());
    let replay_offsets = replay_offsets.unwrap_or(ReplayOffsetSnapshot {
        beat0_time_ns: timing_players[0].get_time_for_beat_ns(0.0),
    });
    let mut replay_out_of_order = false;
    let mut replay_prev_time_ns = INVALID_SONG_TIME_NS;
    for edge in replay_edges {
        let lane = edge.lane_index as usize;
        if lane >= num_cols || song_time_ns_invalid(edge.event_music_time_ns) {
            continue;
        }
        let player = if num_players <= 1 || cols_per_player == 0 {
            0
        } else {
            (lane / cols_per_player).min(num_players.saturating_sub(1))
        };
        let replay_beat0_shift_ns = if song_time_ns_invalid(replay_offsets.beat0_time_ns) {
            0
        } else {
            timing_players[player]
                .get_time_for_beat_ns(0.0)
                .saturating_sub(replay_offsets.beat0_time_ns)
        };
        let event_music_time_ns = edge
            .event_music_time_ns
            .saturating_add(replay_beat0_shift_ns);
        if !song_time_ns_invalid(replay_prev_time_ns) && event_music_time_ns < replay_prev_time_ns {
            replay_out_of_order = true;
        }
        replay_prev_time_ns = event_music_time_ns;
        replay_input.push(RecordedLaneEdge {
            lane_index: edge.lane_index,
            pressed: edge.pressed,
            source: edge.source,
            event_music_time_ns,
        });
    }
    if replay_out_of_order {
        replay_input.sort_by(|a, b| a.event_music_time_ns.cmp(&b.event_music_time_ns));
    }
    let replay_mode = !replay_input.is_empty();
    if replay_mode {
        debug!(
            "Gameplay replay mode enabled: {} recorded edges loaded.",
            replay_input.len(),
        );
    }
    let beat_info_cache = BeatInfoCache::new(&timing);
    let setup_ms = init_started.elapsed().as_secs_f64() * 1000.0;

    let note_build_started = Instant::now();
    let notes_cap: usize = (0..num_players)
        .map(|player| gameplay_charts[player].parsed_notes.len())
        .sum();
    let mut notes: Vec<Note> = Vec::with_capacity(notes_cap);
    let mut note_ranges = [(0usize, 0usize); MAX_PLAYERS];
    let mut holds_total: [u32; MAX_PLAYERS] = [0; MAX_PLAYERS];
    let mut rolls_total: [u32; MAX_PLAYERS] = [0; MAX_PLAYERS];
    let mut mines_total: [u32; MAX_PLAYERS] = [0; MAX_PLAYERS];
    let mut max_row_index = 0usize;

    for player in 0..num_players {
        let timing_player = &timing_players[player];
        let parsed_notes = &gameplay_charts[player].parsed_notes;
        let start = notes.len();
        let col_offset = player.saturating_mul(cols_per_player);
        for parsed in parsed_notes {
            let row_index = parsed.row_index;
            max_row_index = max_row_index.max(row_index);

            let Some(beat) = timing_player.get_beat_for_row(row_index) else {
                continue;
            };
            let explicit_fake_tap = matches!(parsed.note_type, NoteType::Fake);
            let fake_by_segment = timing_player.is_fake_at_beat(beat);
            let is_fake = explicit_fake_tap || fake_by_segment;
            let note_type = if explicit_fake_tap {
                NoteType::Tap
            } else {
                parsed.note_type
            };

            // Pre-calculate judgability to avoid binary searches during gameplay
            let judgable_by_timing = timing_player.is_judgable_at_beat(beat);
            let can_be_judged = !is_fake && judgable_by_timing;

            if can_be_judged {
                match note_type {
                    NoteType::Hold => {
                        holds_total[player] = holds_total[player].saturating_add(1);
                    }
                    NoteType::Roll => {
                        rolls_total[player] = rolls_total[player].saturating_add(1);
                    }
                    NoteType::Mine => {
                        mines_total[player] = mines_total[player].saturating_add(1);
                    }
                    NoteType::Tap | NoteType::Lift => {}
                    NoteType::Fake => {}
                }
            }

            let hold = match (note_type, parsed.tail_row_index) {
                (NoteType::Hold | NoteType::Roll, Some(tail_row)) => timing_player
                    .get_beat_for_row(tail_row)
                    .map(|end_beat| HoldData {
                        end_row_index: tail_row,
                        end_beat,
                        result: None,
                        life: INITIAL_HOLD_LIFE,
                        let_go_started_at: None,
                        let_go_starting_life: 0.0,
                        last_held_row_index: row_index,
                        last_held_beat: beat,
                    }),
                _ => None,
            };

            let quantization_idx = quantization_index_from_beat(beat);
            notes.push(Note {
                beat,
                quantization_idx,
                column: parsed.column.saturating_add(col_offset),
                note_type,
                row_index,
                result: None,
                early_result: None,
                hold,
                mine_result: None,
                is_fake,
                can_be_judged,
            });
        }
        let end = notes.len();
        note_ranges[player] = (start, end);
    }
    let note_build_ms = note_build_started.elapsed().as_secs_f64() * 1000.0;

    let transform_started = Instant::now();
    apply_uncommon_chart_transforms(
        &mut notes,
        &mut note_ranges,
        cols_per_player,
        num_players,
        &player_profiles,
        &timing_players,
    );

    let song_seed = turn_seed_for_song(&song);
    let mut attack_song_length_seconds = song.music_length_seconds.max(song.precise_last_second());
    if !attack_song_length_seconds.is_finite() || attack_song_length_seconds <= 0.0 {
        attack_song_length_seconds = song.total_length_seconds.max(0) as f32;
    }
    apply_turn_options(
        &mut notes,
        note_ranges,
        cols_per_player,
        num_players,
        &player_profiles,
        song_seed,
    );
    apply_chart_attacks_transforms(
        &mut notes,
        &mut note_ranges,
        &gameplay_charts,
        cols_per_player,
        num_players,
        &player_profiles,
        &timing_players,
        song_seed,
        attack_song_length_seconds,
    );

    let mut score_valid = [true; MAX_PLAYERS];
    let mut score_missed_holds_rolls = [false; MAX_PLAYERS];
    for player in 0..num_players {
        let invalid_reasons = score_invalid_reason_lines_for_chart(
            &charts[player],
            &player_profiles[player],
            scroll_speed[player],
            rate,
        );
        score_valid[player] = invalid_reasons.is_empty();
        if !score_valid[player] {
            debug!(
                "Score validity disabled for player {} ({}): {}.",
                player + 1,
                charts[player].short_hash,
                invalid_reasons.join("; ")
            );
        }
        score_missed_holds_rolls[player] = score_missed_holds_and_rolls(&charts[player].chart_type);
    }

    let chart_layout_changed = (0..num_players)
        .any(|player| player_changes_chart(&gameplay_charts[player], &player_profiles[player]));
    let mut total_steps = [0u32; MAX_PLAYERS];
    let mut jumps_total = [0u32; MAX_PLAYERS];
    let mut hands_total = [0u32; MAX_PLAYERS];
    let mut possible_grade_points = [0i32; MAX_PLAYERS];
    if chart_layout_changed {
        holds_total = [0; MAX_PLAYERS];
        rolls_total = [0; MAX_PLAYERS];
        mines_total = [0; MAX_PLAYERS];
        for player in 0..num_players {
            let totals = recompute_player_totals(&notes, note_ranges[player]);
            total_steps[player] = totals.steps;
            holds_total[player] = totals.holds;
            rolls_total[player] = totals.rolls;
            mines_total[player] = totals.mines;
            jumps_total[player] = totals.jumps;
            hands_total[player] = totals.hands;
            possible_grade_points[player] = compute_possible_grade_points(
                &notes,
                note_ranges[player],
                holds_total[player],
                rolls_total[player],
            );
        }
    } else {
        for player in 0..num_players {
            total_steps[player] = charts[player].stats.total_steps;
            holds_total[player] = charts[player].holds_total;
            rolls_total[player] = charts[player].rolls_total;
            mines_total[player] = charts[player].mines_total;
            jumps_total[player] = charts[player].stats.jumps;
            hands_total[player] = charts[player].stats.hands;
            possible_grade_points[player] = charts[player].possible_grade_points;
        }
    }
    if num_players == 1 {
        possible_grade_points[1] = possible_grade_points[0];
        holds_total[1] = holds_total[0];
        rolls_total[1] = rolls_total[0];
        mines_total[1] = mines_total[0];
        total_steps[1] = total_steps[0];
        jumps_total[1] = jumps_total[0];
        hands_total[1] = hands_total[0];
        score_valid[1] = score_valid[0];
        score_missed_holds_rolls[1] = score_missed_holds_rolls[0];
        note_ranges[1] = note_ranges[0];
    }
    let transform_ms = transform_started.elapsed().as_secs_f64() * 1000.0;

    let note_player_for_col = |col: usize| -> usize {
        if num_players <= 1 || cols_per_player == 0 {
            0
        } else {
            (col / cols_per_player).min(num_players.saturating_sub(1))
        }
    };

    let cache_build_started = Instant::now();
    let mut note_time_cache_ns = Vec::with_capacity(notes.len());
    let mut note_display_beat_cache = Vec::with_capacity(notes.len());
    let mut hold_end_time_cache_ns = Vec::with_capacity(notes.len());
    let mut hold_end_display_beat_cache = Vec::with_capacity(notes.len());
    let mut hold_display_beat_min_cache = Vec::with_capacity(notes.len());
    let mut hold_display_beat_max_cache = Vec::with_capacity(notes.len());
    for note in &notes {
        let timing_player = &timing_players[note_player_for_col(note.column)];
        let note_time_ns = timing_player.get_time_for_beat_ns(note.beat);
        let note_display_beat = timing_player.get_displayed_beat(note.beat);
        note_time_cache_ns.push(note_time_ns);
        note_display_beat_cache.push(note_display_beat);
        if let Some(hold) = note.hold.as_ref() {
            let end_time_ns = timing_player.get_time_for_beat_ns(hold.end_beat);
            let end_display_beat = timing_player.get_displayed_beat(hold.end_beat);
            hold_end_time_cache_ns.push(Some(end_time_ns));
            hold_end_display_beat_cache.push(Some(end_display_beat));
            hold_display_beat_min_cache.push(Some(note_display_beat.min(end_display_beat)));
            hold_display_beat_max_cache.push(Some(note_display_beat.max(end_display_beat)));
        } else {
            hold_end_time_cache_ns.push(None);
            hold_end_display_beat_cache.push(None);
            hold_display_beat_min_cache.push(None);
            hold_display_beat_max_cache.push(None);
        }
    }

    debug!("Parsed {} notes from chart data.", notes.len());

    let mut row_entries: Vec<RowEntry> = Vec::with_capacity(notes.len() / 2);
    let mut row_entry_ranges = [(0usize, 0usize); MAX_PLAYERS];
    let mut row_map_cache: [Vec<u32>; MAX_PLAYERS] =
        std::array::from_fn(|_| vec![u32::MAX; max_row_index + 1]);
    let mut note_row_entry_indices = vec![u32::MAX; notes.len()];
    let mut tap_row_hold_roll_flags = vec![0u8; notes.len()];
    for player in 0..num_players {
        let row_range_start = row_entries.len();
        let (note_start, note_end) = note_ranges[player];
        let mut cursor = note_start;
        while cursor < note_end {
            let row_index = notes[cursor].row_index;
            let row_start = cursor;
            let mut row_flags = 0u8;
            let mut nonmine_note_indices = Vec::with_capacity(4);
            while cursor < note_end && notes[cursor].row_index == row_index {
                let note = &notes[cursor];
                match note.note_type {
                    NoteType::Hold => row_flags |= 0b01,
                    NoteType::Roll => row_flags |= 0b10,
                    _ => {}
                }
                if note.can_be_judged && !matches!(note.note_type, NoteType::Mine) {
                    nonmine_note_indices.push(cursor);
                }
                cursor += 1;
            }
            if !nonmine_note_indices.is_empty() {
                let row_entry_index = row_entries.len() as u32;
                row_map_cache[player][row_index] = row_entry_index;
                for &note_index in &nonmine_note_indices {
                    note_row_entry_indices[note_index] = row_entry_index;
                }
                row_entries.push(build_row_entry(
                    row_index,
                    nonmine_note_indices,
                    &notes,
                    &note_time_cache_ns,
                ));
            }
            tap_row_hold_roll_flags[row_start..cursor].fill(row_flags);
        }
        row_entry_ranges[player] = (row_range_start, row_entries.len());
    }
    let cache_build_ms = cache_build_started.elapsed().as_secs_f64() * 1000.0;

    let timing_prep_started = Instant::now();
    let first_second = notes
        .iter()
        .zip(&note_time_cache_ns)
        .filter_map(|(n, &t_ns)| n.can_be_judged.then_some(song_time_ns_to_seconds(t_ns)))
        .reduce(f32::min)
        .unwrap_or(0.0);
    // ITGmania's ScreenGameplay::StartPlayingSong uses theme metrics
    // MinSecondsToStep / MinSecondsToMusic. Simply Love scales both by
    // MusicRate, so we apply the same here to keep real-world lead-in time
    // consistent across rates.
    let lead_in_timing = lead_in_timing.unwrap_or_default();
    let min_time_to_notes = lead_in_timing.min_seconds_to_step.max(0.0) * rate;
    let min_time_to_music = lead_in_timing.min_seconds_to_music.max(0.0) * rate;
    let mut start_delay = min_time_to_notes - first_second;
    if start_delay < min_time_to_music {
        start_delay = min_time_to_music;
    }
    if start_delay < 0.0 {
        start_delay = 0.0;
    }

    let first_note_beat = timing.get_beat_for_time(first_second);
    let initial_bpm = timing.get_bpm_for_beat(first_note_beat);

    let mut reference_bpm = get_reference_bpm_from_display_tag(&charts[0], &song.display_bpm)
        .unwrap_or_else(|| {
            let mut actual_max = timing.get_capped_max_bpm(Some(M_MOD_HIGH_CAP));
            if !actual_max.is_finite() || actual_max <= 0.0 {
                actual_max = initial_bpm.max(120.0);
            }
            actual_max
        });
    if !reference_bpm.is_finite() || reference_bpm <= 0.0 {
        reference_bpm = initial_bpm.max(120.0);
    }

    let pixels_per_second: [f32; MAX_PLAYERS] = std::array::from_fn(|player| {
        let mut pps = scroll_speed[player].pixels_per_second(initial_bpm, reference_bpm, rate);
        if !pps.is_finite() || pps <= 0.0 {
            pps = ScrollSpeedSetting::default().pixels_per_second(initial_bpm, reference_bpm, rate);
        }
        pps
    });
    let draw_distance_before_targets: [f32; MAX_PLAYERS] = std::array::from_fn(|player| {
        if player >= num_players {
            return screen_height() * DRAW_DISTANCE_BEFORE_TARGETS_MULTIPLIER;
        }
        let draw_scale = player_draw_scale(&player_profiles[player]);
        screen_height() * DRAW_DISTANCE_BEFORE_TARGETS_MULTIPLIER * draw_scale
    });
    let draw_distance_after_targets: [f32; MAX_PLAYERS] = std::array::from_fn(|player| {
        if player >= num_players {
            return DRAW_DISTANCE_AFTER_TARGETS;
        }
        let draw_scale = player_draw_scale(&player_profiles[player]);
        if player_profiles[player]
            .scroll_option
            .contains(profile::ScrollOption::Centered)
        {
            screen_height() * 0.6 * draw_scale
        } else {
            DRAW_DISTANCE_AFTER_TARGETS * draw_scale
        }
    });

    let travel_time: [f32; MAX_PLAYERS] = std::array::from_fn(|player| {
        let mut tt = scroll_speed[player].travel_time_seconds(
            draw_distance_before_targets[player],
            initial_bpm,
            reference_bpm,
            rate,
        );
        if !tt.is_finite() || tt <= 0.0 {
            tt = draw_distance_before_targets[player] / pixels_per_second[player];
        }
        tt
    });

    let timing_profile = TimingProfile::default_itg_with_fa_plus();
    let player_judgment_timing = std::array::from_fn(|player| {
        build_player_judgment_timing(timing_profile, &player_profiles[player], rate)
    });
    let (notes_end_time_ns, music_end_time_ns) =
        compute_end_times_ns(&notes, &note_time_cache_ns, &hold_end_time_cache_ns, rate);
    let notes_len = notes.len();
    let mut column_scroll_dirs = [1.0_f32; MAX_COLS];
    for (player, player_profile) in player_profiles.iter().enumerate().take(num_players) {
        let start = player * cols_per_player;
        let end = (start + cols_per_player).min(num_cols).min(MAX_COLS);
        let local_dirs = compute_column_scroll_dirs(player_profile.scroll_option, cols_per_player);
        for (offset, column_scroll_dir) in column_scroll_dirs[start..end].iter_mut().enumerate() {
            *column_scroll_dir = local_dirs[offset];
        }
    }

    let note_range_start: [usize; MAX_PLAYERS] =
        std::array::from_fn(|player| note_ranges[player].0);
    let row_entry_range_start: [usize; MAX_PLAYERS] =
        std::array::from_fn(|player| row_entry_ranges[player].0);
    let mut mine_note_ix: [Vec<usize>; MAX_PLAYERS] = std::array::from_fn(|_| Vec::new());
    let mut mine_note_time_ns: [Vec<SongTimeNs>; MAX_PLAYERS] = std::array::from_fn(|_| Vec::new());
    for player in 0..num_players {
        let (start, end) = note_ranges[player];
        let mut mine_ix = Vec::with_capacity(mines_total[player] as usize);
        let mut mine_times_ns = Vec::with_capacity(mines_total[player] as usize);
        for note_idx in start..end {
            if matches!(notes[note_idx].note_type, NoteType::Mine) {
                mine_ix.push(note_idx);
                mine_times_ns.push(note_time_cache_ns[note_idx]);
            }
        }
        mine_note_ix[player] = mine_ix;
        mine_note_time_ns[player] = mine_times_ns;
    }
    let next_mine_ix_cursor: [usize; MAX_PLAYERS] = [0; MAX_PLAYERS];
    let mut lane_note_counts = [0usize; MAX_COLS];
    let mut lane_hold_counts = [0usize; MAX_COLS];
    let mut replay_cells = 0usize;
    for note in &notes {
        let col = note.column;
        if col < num_cols && col < MAX_COLS {
            lane_note_counts[col] = lane_note_counts[col].saturating_add(1);
            if matches!(note.note_type, NoteType::Hold | NoteType::Roll) {
                lane_hold_counts[col] = lane_hold_counts[col].saturating_add(1);
            }
        }
        if note.can_be_judged && !matches!(note.note_type, NoteType::Mine) {
            replay_cells = replay_cells.saturating_add(1);
        }
    }
    let mut lane_note_indices: [Vec<usize>; MAX_COLS] =
        std::array::from_fn(|col| Vec::with_capacity(lane_note_counts[col]));
    let mut lane_hold_indices: [Vec<usize>; MAX_COLS] =
        std::array::from_fn(|col| Vec::with_capacity(lane_hold_counts[col]));
    for (note_index, note) in notes.iter().enumerate() {
        let col = note.column;
        if col < num_cols && col < MAX_COLS {
            lane_note_indices[col].push(note_index);
            if matches!(note.note_type, NoteType::Hold | NoteType::Roll) {
                lane_hold_indices[col].push(note_index);
            }
        }
    }
    let mut lane_note_display_runs: [Vec<LaneIndexRun>; MAX_COLS] =
        std::array::from_fn(|_| Vec::new());
    let mut lane_hold_display_runs: [Vec<LaneIndexRun>; MAX_COLS] =
        std::array::from_fn(|_| Vec::new());
    for col in 0..num_cols {
        lane_note_display_runs[col] =
            build_lane_note_display_runs(&lane_note_indices[col], &note_display_beat_cache);
        lane_hold_display_runs[col] = build_lane_hold_display_runs(
            &lane_hold_indices[col],
            &hold_display_beat_min_cache,
            &hold_display_beat_max_cache,
        );
    }
    let pending_edges_capacity = input_queue_cap(num_cols);
    let replay_seconds = (song_time_ns_to_seconds(music_end_time_ns) + start_delay)
        .max(song_time_ns_to_seconds(notes_end_time_ns) + start_delay);
    let replay_capture_enabled = !replay_mode && config.machine_enable_replays;
    let replay_edges_capacity = [
        0,
        replay_edge_cap(num_cols, replay_cells, replay_mode, replay_seconds),
    ][replay_capture_enabled as usize];
    let decaying_hold_capacity = (0..num_players).fold(0usize, |acc, player| {
        acc.saturating_add(holds_total[player] as usize + rolls_total[player] as usize)
    });
    let timing_prep_ms = timing_prep_started.elapsed().as_secs_f64() * 1000.0;

    let hud_prep_started = Instant::now();
    let global_visual_delay_seconds = config.visual_delay_seconds;
    let player_visual_delay_seconds: [f32; MAX_PLAYERS] = std::array::from_fn(|player| {
        if player >= num_players {
            return 0.0;
        }
        let ms = player_profiles[player].visual_delay_ms.clamp(-100, 100);
        ms as f32 / 1000.0
    });
    let init_music_time = -start_delay;
    let init_beat = timing.get_beat_for_time_ns(song_time_ns_from_seconds(init_music_time));
    let current_music_time_visible: [f32; MAX_PLAYERS] = std::array::from_fn(|player| {
        init_music_time - global_visual_delay_seconds - player_visual_delay_seconds[player]
    });
    let current_music_time_visible_ns: [SongTimeNs; MAX_PLAYERS] =
        std::array::from_fn(|player| song_time_ns_from_seconds(current_music_time_visible[player]));
    let current_beat_visible: [f32; MAX_PLAYERS] = std::array::from_fn(|player| {
        timing_players[player].get_beat_for_time_ns(current_music_time_visible_ns[player])
    });
    let (
        song_lua_mask_windows,
        song_lua_ease_windows,
        song_lua_overlays,
        song_lua_overlay_eases,
        song_lua_overlay_ease_ranges,
        song_lua_overlay_events,
        song_lua_hidden_players,
        song_lua_screen_width,
        song_lua_screen_height,
    ) = build_song_lua_runtime_windows(
        &song,
        &charts,
        &timing_players,
        num_players,
        &player_profiles,
        &scroll_speed,
        rate,
        config.global_offset_seconds,
        &player_global_offset_shift_seconds,
    );
    let attack_mask_windows: [Vec<AttackMaskWindow>; MAX_PLAYERS] = std::array::from_fn(|player| {
        if player >= num_players {
            return Vec::new();
        }
        let mut windows = if player_profiles[player].attack_mode == profile::AttackMode::Off {
            Vec::new()
        } else {
            build_attack_mask_windows_for_player(
                gameplay_charts[player].chart_attacks.as_deref(),
                player_profiles[player].attack_mode,
                player,
                song_seed,
                attack_song_length_seconds,
            )
        };
        windows.extend(song_lua_mask_windows[player].iter().copied());
        windows
    });
    let reverse_scroll: [bool; MAX_PLAYERS] = std::array::from_fn(|player| {
        if player >= num_players {
            return false;
        }
        player_profiles[player].reverse_scroll
    });
    let mut column_cues: [Vec<ColumnCue>; MAX_PLAYERS] = std::array::from_fn(|_| Vec::new());
    for player in 0..num_players {
        if !player_profiles[player].column_cues {
            continue;
        }
        let col_start = player.saturating_mul(cols_per_player);
        let col_end = (col_start + cols_per_player).min(num_cols);
        column_cues[player] = build_column_cues_for_player(
            &notes,
            note_ranges[player],
            &note_time_cache_ns,
            col_start,
            col_end,
            current_music_time_visible[player],
        );
    }
    if num_players == 1 {
        let (first, second) = column_cues.split_at_mut(1);
        second[0].clone_from(&first[0]);
    }

    let measure_densities: [Vec<usize>; MAX_PLAYERS] = std::array::from_fn(|p| {
        if p >= num_players || !needs_stream_data(&player_profiles[p]) {
            return Vec::new();
        }
        rssp::stats::measure_densities(&gameplay_charts[p].notes, cols_per_player)
    });

    let measure_counter_segments: [Vec<StreamSegment>; MAX_PLAYERS] = std::array::from_fn(|p| {
        if p >= num_players {
            return Vec::new();
        }
        let Some(threshold) = player_profiles[p].measure_counter.notes_threshold() else {
            return Vec::new();
        };
        stream_sequences_threshold(&measure_densities[p], threshold)
    });

    let mut mini_indicator_stream_segments: [Vec<StreamSegment>; MAX_PLAYERS] =
        std::array::from_fn(|_| Vec::new());
    let mut mini_indicator_total_stream_measures = [0.0_f32; MAX_PLAYERS];
    let mut mini_indicator_target_score_percent = [89.0_f64; MAX_PLAYERS];
    let mut mini_indicator_rival_score_percent = [0.0_f64; MAX_PLAYERS];

    for p in 0..num_players {
        if mini_indicator_mode(&player_profiles[p]) == profile::MiniIndicator::None {
            continue;
        }
        let constant_bpm = !timing_players[p].has_bpm_changes();
        let (stream_segments, total_stream, _total_break) =
            zmod_stream_totals_full_measures(&measure_densities[p], constant_bpm);
        mini_indicator_total_stream_measures[p] = total_stream.max(0.0);
        mini_indicator_stream_segments[p] = stream_segments;

        let side = player_side_for_index(play_style, player_side, p);
        let chart_hash = charts[p].short_hash.as_str();
        let personal_best = scores::get_cached_score_for_side(chart_hash, side)
            .map(|s| (s.score_percent * 100.0).clamp(0.0, 100.0));
        let machine_best = scores::get_machine_record_local(chart_hash)
            .map(|(_, s)| (s.score_percent * 100.0).clamp(0.0, 100.0));

        let target = match player_profiles[p].target_score {
            profile::TargetScoreSetting::MachineBest => machine_best.or(personal_best),
            profile::TargetScoreSetting::PersonalBest => personal_best,
            setting => target_score_setting_percent(setting),
        }
        .unwrap_or(89.0);
        mini_indicator_target_score_percent[p] = target;

        mini_indicator_rival_score_percent[p] = machine_best
            .unwrap_or(0.0)
            .max(personal_best.unwrap_or(0.0));
    }

    let mut scorebox_side_snapshot: [Option<scores::CachedPlayerLeaderboardData>; MAX_PLAYERS] =
        std::array::from_fn(|_| None);
    for p in 0..num_players {
        if !player_profiles[p].display_scorebox {
            continue;
        }
        let side = player_side_for_index(play_style, player_side, p);
        if !scores::is_gs_active_for_side(side) {
            continue;
        }
        let chart_hash = charts[p].short_hash.trim();
        if chart_hash.is_empty() {
            continue;
        }
        scorebox_side_snapshot[side_index(side)] =
            scores::get_or_fetch_player_leaderboards_for_side(
                chart_hash,
                side,
                SCOREBOX_NUM_ENTRIES,
            );
    }
    let hud_prep_ms = hud_prep_started.elapsed().as_secs_f64() * 1000.0;

    let graph_prep_started = Instant::now();
    let wants_step_stats = player_profiles
        .iter()
        .take(num_players)
        .any(|p| p.data_visualizations == profile::DataVisualizations::StepStatistics);
    let wide = is_wide();
    let density_graph_enabled = wide && wants_step_stats;
    let sw = screen_width();
    let sh = screen_height().max(1.0_f32);
    let is_ultrawide = sw / sh > (21.0_f32 / 9.0_f32);
    let note_field_is_centered = num_players == 1
        && play_style == profile::PlayStyle::Single
        && config.center_1player_notefield;
    let density_graph_graph_h = if density_graph_enabled {
        105.0_f32
    } else {
        0.0_f32
    };
    let density_graph_graph_w = if density_graph_enabled {
        let mut sidepane_width = sw * 0.5_f32;
        if !is_ultrawide && note_field_is_centered && wide {
            let nf_width = step_stats_notefield_width(
                noteskin[0].as_ref().map(Arc::as_ref),
                cols_per_player,
                field_zoom[0],
            )
            .unwrap_or(256.0_f32)
            .max(1.0_f32);
            sidepane_width = ((sw - nf_width) * 0.5_f32).max(1.0_f32);
        }
        if is_ultrawide && num_players > 1 {
            sidepane_width = (sw * 0.2_f32).max(1.0_f32);
        }
        sidepane_width.round().max(1.0_f32)
    } else {
        0.0_f32
    };
    let density_graph_first_second = timing.get_time_for_beat(0.0).min(0.0_f32);
    let density_graph_last_second = song.precise_last_second();
    let density_graph_duration =
        (density_graph_last_second - density_graph_first_second).max(0.001_f32);

    const DENSITY_GRAPH_MAX_SECONDS: f32 = 4.0 * 60.0;
    let density_graph_scaled_width =
        if density_graph_enabled && density_graph_duration > DENSITY_GRAPH_MAX_SECONDS {
            (density_graph_graph_w * (density_graph_duration / DENSITY_GRAPH_MAX_SECONDS))
                .round()
                .max(density_graph_graph_w)
        } else {
            density_graph_graph_w
        };
    let density_graph_u_window =
        if density_graph_enabled && density_graph_duration > DENSITY_GRAPH_MAX_SECONDS {
            (DENSITY_GRAPH_MAX_SECONDS / density_graph_duration).clamp(0.0_f32, 1.0_f32)
        } else {
            1.0_f32
        };
    let density_graph_u0 = 0.0_f32;
    let density_graph_top_h = 30.0_f32;
    let density_graph_top_w: [f32; MAX_PLAYERS] = std::array::from_fn(|p| {
        if p >= num_players || !player_profiles[p].nps_graph_at_top {
            return 0.0;
        }
        upper_density_graph_width(play_style)
    });
    let density_graph_top_scale_y: [f32; MAX_PLAYERS] = {
        let mut scale = [1.0_f32; MAX_PLAYERS];
        if num_players == 2
            && player_profiles[0].nps_graph_at_top
            && player_profiles[1].nps_graph_at_top
        {
            let p1_peak = charts[0].max_nps as f32;
            let p2_peak = charts[1].max_nps as f32;
            if p1_peak.is_finite() && p2_peak.is_finite() && p1_peak > 0.0 && p2_peak > 0.0 {
                if p1_peak < p2_peak {
                    scale[0] = (p1_peak / p2_peak).clamp(0.0, 1.0);
                } else if p2_peak < p1_peak {
                    scale[1] = (p2_peak / p1_peak).clamp(0.0, 1.0);
                }
            }
        }
        scale
    };
    let mut density_graph_life_update_rate = 0.25_f32;
    if density_graph_enabled && !timing.has_bpm_changes() {
        let bpm = timing.first_bpm();
        if bpm.is_finite() && bpm >= 60.0_f32 {
            let interval_8th = (60.0_f32 / bpm) * 0.5_f32;
            if interval_8th.is_finite() && interval_8th > 0.0_f32 {
                density_graph_life_update_rate =
                    interval_8th * (density_graph_life_update_rate / interval_8th).ceil();
            }
        }
    }
    if !density_graph_life_update_rate.is_finite() || density_graph_life_update_rate <= 0.0_f32 {
        density_graph_life_update_rate = 0.25_f32;
    }
    let density_graph_life_next_update_elapsed = 0.0_f32;
    let density_graph_life_points: [Vec<[f32; 2]>; MAX_PLAYERS] = std::array::from_fn(|p| {
        if density_graph_enabled && p < num_players {
            Vec::with_capacity(1024)
        } else {
            Vec::new()
        }
    });
    let density_graph_life_dirty: [bool; MAX_PLAYERS] = [false; MAX_PLAYERS];
    let graph_prep_ms = graph_prep_started.elapsed().as_secs_f64() * 1000.0;

    let finalize_started = Instant::now();
    let mut players = std::array::from_fn(|_| init_player_runtime());
    for p in 0..num_players {
        if player_profiles[p].carry_combo_between_songs && !replay_mode {
            players[p].combo = combo_carry[p];
        }
        let life = players[p].life;
        players[p].life_history.push((init_music_time, life));
    }
    let assist_clap_rows = build_assist_clap_rows(&notes, note_ranges[0]);
    let song_offset_seconds = song.offset;
    let next_background_change_ix = song
        .background_changes
        .iter()
        .take_while(|change| change.start_beat <= init_beat)
        .count();
    let base_attack_appearance = std::array::from_fn(|player| {
        if player < num_players {
            base_appearance_effects(&player_profiles[player])
        } else {
            AppearanceEffects::default()
        }
    });

    let mut state = State {
        song,
        song_full_title,
        stage_intro_text,
        pack_group,
        pack_banner_path,
        current_background_path: None,
        next_background_change_ix,
        charts,
        gameplay_charts,
        background_texture_key: "__black".to_string(),
        num_cols,
        cols_per_player,
        num_players,
        timing,
        timing_players,
        beat_info_cache,
        timing_profile,
        player_judgment_timing,
        notes,
        note_ranges,
        audio_lead_in_seconds: start_delay,
        current_beat: init_beat,
        current_music_time_ns: song_time_ns_from_seconds(init_music_time),
        current_beat_display: init_beat,
        current_music_time_display: init_music_time,
        display_clock: FrameStableDisplayClock::new(song_time_ns_from_seconds(init_music_time)),
        display_clock_diag: DisplayClockDiagRing::new(),
        lane_note_indices,
        lane_hold_indices,
        lane_note_display_runs,
        lane_hold_display_runs,
        row_entry_ranges,
        judged_row_cursor: row_entry_range_start,
        note_time_cache_ns,
        note_display_beat_cache,
        hold_end_time_cache_ns,
        hold_end_display_beat_cache,
        hold_display_beat_min_cache,
        hold_display_beat_max_cache,
        notes_end_time_ns,
        music_end_time_ns,
        music_rate: rate,
        play_mine_sounds: config.mine_hit_sound,
        global_offset_seconds: config.global_offset_seconds,
        initial_global_offset_seconds: config.global_offset_seconds,
        player_global_offset_shift_seconds,
        song_offset_seconds,
        initial_song_offset_seconds: song_offset_seconds,
        autosync_mode: AutosyncMode::Off,
        autosync_offset_samples: [0; AUTOSYNC_OFFSET_SAMPLE_COUNT],
        autosync_offset_sample_count: 0,
        autosync_standard_deviation: 0.0,
        global_visual_delay_seconds,
        player_visual_delay_seconds,
        current_music_time_visible_ns,
        current_music_time_visible,
        current_beat_visible,
        next_tap_miss_cursor: note_range_start,
        next_mine_avoid_cursor: note_range_start,
        mine_note_ix,
        mine_note_time_ns,
        next_mine_ix_cursor,
        row_entries,
        measure_counter_segments,
        column_cues,
        mini_indicator_stream_segments,
        mini_indicator_total_stream_measures,
        mini_indicator_target_score_percent,
        mini_indicator_rival_score_percent,
        row_map_cache,
        note_row_entry_indices,
        tap_row_hold_roll_flags,
        decaying_hold_indices: Vec::with_capacity(decaying_hold_capacity),
        hold_decay_active: vec![false; notes_len],
        tap_miss_held_window: vec![false; notes_len],
        pending_missed_hold_feedback: vec![false; notes_len],
        pending_missed_hold_indices: Vec::new(),
        players,
        hold_judgments: Default::default(),
        is_in_freeze: false,
        is_in_delay: false,
        possible_grade_points,
        song_completed_naturally: false,
        autoplay_enabled: replay_mode,
        autoplay_used: replay_mode,
        score_valid,
        score_missed_holds_rolls,
        replay_mode,
        replay_capture_enabled,
        course_display_carry,
        course_display_totals,
        live_window_counts: [crate::game::timing::WindowCounts::default(); MAX_PLAYERS],
        live_window_counts_10ms_blue: [crate::game::timing::WindowCounts::default(); MAX_PLAYERS],
        live_window_counts_display_blue: [crate::game::timing::WindowCounts::default();
            MAX_PLAYERS],
        player_profiles,
        scorebox_side_snapshot,
        attack_mask_windows,
        song_lua_ease_windows,
        song_lua_overlays,
        song_lua_overlay_eases,
        song_lua_overlay_ease_ranges,
        song_lua_overlay_events,
        song_lua_hidden_players,
        song_lua_screen_width,
        song_lua_screen_height,
        song_lua_player_rotation_z: [0.0; MAX_PLAYERS],
        song_lua_player_rotation_y: [0.0; MAX_PLAYERS],
        song_lua_player_skew_x: [0.0; MAX_PLAYERS],
        song_lua_player_zoom_x: [1.0; MAX_PLAYERS],
        song_lua_player_zoom_y: [1.0; MAX_PLAYERS],
        song_lua_player_confusion_y_offset: [0.0; MAX_PLAYERS],
        active_attack_clear_all: [false; MAX_PLAYERS],
        active_attack_chart: [ChartAttackEffects::default(); MAX_PLAYERS],
        active_attack_accel: [AccelOverrides::default(); MAX_PLAYERS],
        active_attack_visual: [VisualOverrides::default(); MAX_PLAYERS],
        attack_current_appearance: base_attack_appearance,
        attack_target_appearance: base_attack_appearance,
        attack_speed_appearance: [AppearanceEffects::approach_speeds(); MAX_PLAYERS],
        active_attack_appearance: base_attack_appearance,
        active_attack_visibility: [VisibilityOverrides::default(); MAX_PLAYERS],
        active_attack_scroll: [ScrollOverrides::default(); MAX_PLAYERS],
        active_attack_perspective: [PerspectiveOverrides::default(); MAX_PLAYERS],
        active_attack_scroll_speed: [None; MAX_PLAYERS],
        active_attack_mini_percent: [None; MAX_PLAYERS],
        noteskin,
        mine_noteskin,
        receptor_noteskin,
        tap_explosion_noteskin,
        active_color_index,
        player_color: color::decorative_rgba(player_color_index),
        scroll_speed,
        scroll_reference_bpm: reference_bpm,
        field_zoom,
        notefield_model_cache,
        scroll_pixels_per_second: pixels_per_second,
        scroll_travel_time: travel_time,
        draw_distance_before_targets,
        draw_distance_after_targets,
        reverse_scroll,
        column_scroll_dirs,
        receptor_glow_timers: [0.0; MAX_COLS],
        receptor_glow_press_timers: [0.0; MAX_COLS],
        receptor_glow_lift_start_alpha: [0.0; MAX_COLS],
        receptor_glow_lift_start_zoom: [1.0; MAX_COLS],
        receptor_bop_timers: [0.0; MAX_COLS],
        tap_explosions: Default::default(),
        mine_explosions: Default::default(),
        active_holds: Default::default(),
        holds_total,
        rolls_total,
        mines_total,
        total_steps,
        jumps_total,
        hands_total,
        total_elapsed_in_screen: 0.0,
        lobby_music_started: false,
        lobby_ready_p1: false,
        lobby_ready_p2: false,
        lobby_disconnect_hold_p1: None,
        lobby_disconnect_hold_p2: None,
        sync_overlay_message: None,
        replay_status_text,
        danger_fx: std::array::from_fn(|_| DangerFx::default()),
        density_graph_first_second,
        density_graph_last_second,
        density_graph_duration,
        density_graph_graph_w,
        density_graph_graph_h,
        density_graph_scaled_width,
        density_graph_u0,
        density_graph_u_window,
        density_graph_life_update_rate,
        density_graph_life_next_update_elapsed,
        density_graph_life_points,
        density_graph_life_dirty,
        density_graph_top_h,
        density_graph_top_w,
        density_graph_top_scale_y,
        hold_to_exit_key: None,
        hold_to_exit_start: None,
        hold_to_exit_aborted_at: None,
        exit_transition: None,
        shift_held: false,
        ctrl_held: false,
        offset_adjust_held_since: [None; 2],
        offset_adjust_last_at: [None; 2],
        prev_inputs: [false; MAX_COLS],
        keyboard_lane_counts: [0; MAX_COLS],
        gamepad_lane_counts: [0; MAX_COLS],
        lane_pressed_since_ns: [None; MAX_COLS],
        pending_edges: VecDeque::with_capacity(pending_edges_capacity),
        autoplay_rng: TurnRng::new(song_seed ^ 0xA17F_0FF5_EED5_1EED),
        autoplay_cursor: note_range_start,
        tick_mode: profile::get_session_timing_tick_mode(),
        assist_clap_rows,
        assist_clap_cursor: 0,
        assist_last_crossed_row: -1,
        toggle_flash_text: None,
        toggle_flash_timer: 0.0,
        replay_input,
        replay_cursor: 0,
        replay_edges: Vec::with_capacity(replay_edges_capacity),
        update_trace: GameplayUpdateTraceState::default(),
    };
    state.update_trace = GameplayUpdateTraceState::from_state(&state);
    refresh_active_attack_masks(&mut state, 0.0);
    let current_bpm = state.timing.get_bpm_for_beat(state.current_beat);
    refresh_live_notefield_options(&mut state, current_bpm);
    let finalize_ms = finalize_started.elapsed().as_secs_f64() * 1000.0;
    let total_ms = init_started.elapsed().as_secs_f64() * 1000.0;
    if total_ms >= 50.0 {
        info!(
            "Gameplay init timing: song='{}' notes={} players={} density_graph={} setup_ms={setup_ms:.3} note_build_ms={note_build_ms:.3} transform_ms={transform_ms:.3} cache_ms={cache_build_ms:.3} timing_ms={timing_prep_ms:.3} hud_ms={hud_prep_ms:.3} graph_ms={graph_prep_ms:.3} finalize_ms={finalize_ms:.3} elapsed_ms={total_ms:.3}",
            state.song.title,
            state.notes.len(),
            state.num_players,
            density_graph_enabled,
        );
    } else {
        debug!(
            "Gameplay init timing: song='{}' notes={} players={} density_graph={} setup_ms={setup_ms:.3} note_build_ms={note_build_ms:.3} transform_ms={transform_ms:.3} cache_ms={cache_build_ms:.3} timing_ms={timing_prep_ms:.3} hud_ms={hud_prep_ms:.3} graph_ms={graph_prep_ms:.3} finalize_ms={finalize_ms:.3} elapsed_ms={total_ms:.3}",
            state.song.title,
            state.notes.len(),
            state.num_players,
            density_graph_enabled,
        );
    }
    state
}

fn update_itg_grade_totals(p: &mut PlayerRuntime) {
    p.earned_grade_points = judgment::calculate_itg_grade_points_from_counts(
        &p.scoring_counts,
        p.holds_held_for_score,
        p.rolls_held_for_score,
        p.mines_hit_for_score,
    );
}

const fn grade_to_window(grade: JudgeGrade) -> Option<&'static str> {
    match grade {
        JudgeGrade::Fantastic => Some("W1"),
        JudgeGrade::Excellent => Some("W2"),
        JudgeGrade::Great => Some("W3"),
        JudgeGrade::Decent => Some("W4"),
        JudgeGrade::WayOff => Some("W5"),
        JudgeGrade::Miss => None,
    }
}

#[inline(always)]
fn timing_hit_log_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| {
        std::env::var("DEADSYNC_TIMING_HIT_LOG").is_ok_and(|v| {
            let v = v.trim();
            v == "1" || v.eq_ignore_ascii_case("true") || v.eq_ignore_ascii_case("yes")
        })
    })
}

#[inline(always)]
fn log_timing_hit_detail(
    enabled: bool,
    stream_pos_s: f32,
    grade: JudgeGrade,
    row_index: usize,
    col: usize,
    beat: f32,
    song_offset_s: f32,
    global_offset_s: f32,
    note_time_s: f32,
    event_time_s: f32,
    music_now_s: f32,
    rate: f32,
    lead_in_s: f32,
) {
    if !enabled {
        return;
    }
    let expected_stream_for_note_s =
        note_time_s / rate + lead_in_s + global_offset_s * (1.0 - rate) / rate;
    let expected_stream_for_hit_s =
        event_time_s / rate + lead_in_s + global_offset_s * (1.0 - rate) / rate;
    let stream_delta_note_ms = (stream_pos_s - expected_stream_for_note_s) * 1000.0;
    let stream_delta_hit_ms = (stream_pos_s - expected_stream_for_hit_s) * 1000.0;
    debug!(
        concat!(
            "TIMING HIT: grade={:?}, row={}, col={}, beat={:.3}, ",
            "song_offset_s={:.4}, global_offset_s={:.4}, ",
            "note_time_s={:.6}, event_time_s={:.6}, music_now_s={:.6}, ",
            "offset_ms={:.2}, rate={:.3}, lead_in_s={:.4}, ",
            "stream_pos_s={:.6}, stream_note_s={:.6}, stream_delta_note_ms={:.2}, ",
            "stream_hit_s={:.6}, stream_delta_hit_ms={:.2}"
        ),
        grade,
        row_index,
        col,
        beat,
        song_offset_s,
        global_offset_s,
        note_time_s,
        event_time_s,
        music_now_s,
        ((event_time_s - note_time_s) / rate) * 1000.0,
        rate,
        lead_in_s,
        stream_pos_s,
        expected_stream_for_note_s,
        stream_delta_note_ms,
        expected_stream_for_hit_s,
        stream_delta_hit_ms,
    );
}

fn trigger_tap_explosion(state: &mut State, column: usize, grade: JudgeGrade) {
    let Some(window_key) = grade_to_window(grade) else {
        return;
    };
    let player = player_for_col(state, column);
    let spawn_window = tap_explosion_noteskin_for_player(state, player).and_then(|ns| {
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
    if state.play_mine_sounds {
        audio::play_sfx("assets/sounds/boom.ogg");
    }
}

fn trigger_combo_milestone(p: &mut PlayerRuntime, kind: ComboMilestoneKind) {
    if let Some(index) = p
        .combo_milestones
        .iter()
        .position(|milestone| milestone.kind == kind)
    {
        p.combo_milestones[index].elapsed = 0.0;
    } else {
        p.combo_milestones
            .push(ActiveComboMilestone { kind, elapsed: 0.0 });
    }
}

#[inline(always)]
const fn combo_continues_on_grade(grade: JudgeGrade) -> bool {
    matches!(
        grade,
        JudgeGrade::Fantastic | JudgeGrade::Excellent | JudgeGrade::Great
    )
}

#[inline(always)]
const fn combo_increments_miss_combo(grade: JudgeGrade) -> bool {
    matches!(grade, JudgeGrade::Miss)
}

#[inline(always)]
fn clear_full_combo_state(p: &mut PlayerRuntime) {
    if p.full_combo_grade.is_some() {
        p.first_fc_attempt_broken = true;
    }
    p.full_combo_grade = None;
}

#[inline(always)]
fn break_combo_state(p: &mut PlayerRuntime, miss_combo_delta: u32) {
    p.combo = 0;
    if miss_combo_delta > 0 {
        p.miss_combo = p.miss_combo.saturating_add(miss_combo_delta);
    }
    clear_full_combo_state(p);
    p.current_combo_grade = None;
}

#[inline(always)]
fn apply_successful_row_combo_state(
    p: &mut PlayerRuntime,
    final_grade: JudgeGrade,
    row_combo_count: u32,
) {
    p.miss_combo = 0;
    p.combo = p.combo.saturating_add(row_combo_count);
    let combo = p.combo;
    if combo > 0 && combo.is_multiple_of(1000) {
        trigger_combo_milestone(p, ComboMilestoneKind::Thousand);
        trigger_combo_milestone(p, ComboMilestoneKind::Hundred);
    } else if combo > 0 && combo.is_multiple_of(100) {
        trigger_combo_milestone(p, ComboMilestoneKind::Hundred);
    }
    if !p.first_fc_attempt_broken {
        let new_grade = if let Some(current_fc_grade) = &p.full_combo_grade {
            final_grade.max(*current_fc_grade)
        } else {
            final_grade
        };
        p.full_combo_grade = Some(new_grade);
    }
    let current_combo_grade = if let Some(curr_grade) = p.current_combo_grade {
        final_grade.max(curr_grade)
    } else {
        final_grade
    };
    p.current_combo_grade = Some(current_combo_grade);
}

#[inline(always)]
fn apply_row_combo_state(
    p: &mut PlayerRuntime,
    final_grade: JudgeGrade,
    row_combo_count: u32,
    miss_combo_count: u32,
) {
    if combo_continues_on_grade(final_grade) {
        apply_successful_row_combo_state(p, final_grade, row_combo_count);
        return;
    }

    let miss_combo_delta = if combo_increments_miss_combo(final_grade) {
        miss_combo_count
    } else {
        0
    };
    break_combo_state(p, miss_combo_delta);
}

#[inline(always)]
fn apply_mine_hit_combo_state(p: &mut PlayerRuntime) {
    if MINE_HIT_INCREMENTS_MISS_COMBO {
        break_combo_state(p, 1);
    }
}

#[inline(always)]
fn apply_hold_success_combo_state(_p: &mut PlayerRuntime) {
    // ITG dance/pump scoring does not let Held / Roll Held reset miss combo.
}

fn hit_mine(
    state: &mut State,
    column: usize,
    note_index: usize,
    time_error_music_ns: SongTimeNs,
) -> bool {
    let player = player_for_col(state, column);
    let rate = if state.music_rate.is_finite() && state.music_rate > 0.0 {
        state.music_rate
    } else {
        1.0
    };
    let mine_window_music_ns = state.player_judgment_timing[player]
        .profile_music_ns
        .mine_window_ns;
    if i128::from(time_error_music_ns).abs() > i128::from(mine_window_music_ns) {
        return false;
    }
    if state.notes[note_index].mine_result.is_some() || state.notes[note_index].is_fake {
        return false;
    }
    if !state.notes[note_index].can_be_judged {
        return false;
    }

    let scoring_blocked = autoplay_blocks_scoring(state);
    state.notes[note_index].mine_result = Some(MineResult::Hit);
    let current_music_time = current_music_time_s(state);
    if !scoring_blocked {
        state.players[player].mines_hit = state.players[player].mines_hit.saturating_add(1);
    }
    let mut updated_scoring = false;
    if !scoring_blocked {
        apply_life_change(
            &mut state.players[player],
            current_music_time,
            LIFE_HIT_MINE,
        );
        capture_failed_ex_score_inputs(state, player);
        if !is_state_dead(state, player) {
            state.players[player].mines_hit_for_score =
                state.players[player].mines_hit_for_score.saturating_add(1);
            updated_scoring = true;
        }
        apply_mine_hit_combo_state(&mut state.players[player]);
    }
    state.receptor_glow_timers[column] = 0.0;
    trigger_mine_explosion(state, column);
    let note_time_ns = state.note_time_cache_ns[note_index];
    let hit_time_ns = note_time_ns.saturating_add(time_error_music_ns);
    debug!(
        "JUDGE MINE HIT: row={}, col={}, beat={:.3}, note_time={:.4}s, hit_time={:.4}s, offset_ms={:.2}, rate={:.3}",
        state.notes[note_index].row_index,
        column,
        state.notes[note_index].beat,
        song_time_ns_to_seconds(note_time_ns),
        song_time_ns_to_seconds(hit_time_ns),
        judgment_time_error_ms_from_music_ns(time_error_music_ns, rate),
        rate
    );
    if updated_scoring {
        update_itg_grade_totals(&mut state.players[player]);
    }
    true
}

#[inline(always)]
fn try_hit_mine_while_held(state: &mut State, column: usize, current_time_ns: SongTimeNs) -> bool {
    if song_time_ns_invalid(current_time_ns) {
        return false;
    }
    let player = player_for_col(state, column);
    let mine_window_music_ns = state.player_judgment_timing[player]
        .profile_music_ns
        .mine_window_ns;
    let start_t_ns = current_time_ns.saturating_sub(mine_window_music_ns);
    let end_t_ns = current_time_ns.saturating_add(mine_window_music_ns);
    let mine_ix = &state.mine_note_ix[player];
    let mine_times_ns = &state.mine_note_time_ns[player];
    let (start_idx, end_idx) = mine_window_bounds_ns(mine_times_ns, start_t_ns, end_t_ns);
    let mut best: Option<(usize, SongTimeNs)> = None;
    for i in start_idx..end_idx {
        let idx = mine_ix[i];
        let note = &state.notes[idx];
        if note.column != column {
            continue;
        }
        if !note.can_be_judged || note.is_fake {
            continue;
        }
        if note.mine_result.is_some() {
            continue;
        }
        let signed_err_ns = current_time_ns.saturating_sub(mine_times_ns[i]);
        let abs_err_ns = (signed_err_ns as i128).unsigned_abs();
        if abs_err_ns <= i128::from(mine_window_music_ns) as u128 {
            match best {
                Some((_, best_err_ns)) if abs_err_ns >= (best_err_ns as i128).unsigned_abs() => {}
                _ => best = Some((idx, signed_err_ns)),
            }
        }
    }
    let Some((note_index, time_error_ns)) = best else {
        return false;
    };
    hit_mine(state, column, note_index, time_error_ns)
}

#[inline(always)]
fn try_hit_crossed_mines_while_held(
    state: &mut State,
    column: usize,
    prev_time_ns: SongTimeNs,
    current_time_ns: SongTimeNs,
) -> bool {
    if song_time_ns_invalid(prev_time_ns)
        || song_time_ns_invalid(current_time_ns)
        || current_time_ns <= prev_time_ns
    {
        return false;
    }
    let player = player_for_col(state, column);
    let mine_window_music_ns = state.player_judgment_timing[player]
        .profile_music_ns
        .mine_window_ns;
    // ITG checks held mines as rows are crossed. Match that by only considering
    // mines whose note time crossed between previous and current music time.
    let (start_idx, end_idx) = crossed_mine_bounds_ns(
        &state.mine_note_time_ns[player],
        prev_time_ns,
        current_time_ns,
    );
    let mut hit_any = false;
    for i in start_idx..end_idx {
        let (note_index, note_time_ns) = {
            let mine_ix = &state.mine_note_ix[player];
            let mine_times_ns = &state.mine_note_time_ns[player];
            (mine_ix[i], mine_times_ns[i])
        };
        let (is_mine, can_be_judged, already_scored, is_fake, note_column) = {
            let note = &state.notes[note_index];
            (
                matches!(note.note_type, NoteType::Mine),
                note.can_be_judged,
                note.mine_result.is_some(),
                note.is_fake,
                note.column,
            )
        };
        if !is_mine || !can_be_judged || already_scored || is_fake || note_column != column {
            continue;
        }
        let time_error_music_ns = current_time_ns.saturating_sub(note_time_ns);
        if i128::from(time_error_music_ns).abs() > i128::from(mine_window_music_ns) {
            continue;
        }
        if hit_mine(state, column, note_index, time_error_music_ns) {
            hit_any = true;
        }
    }
    hit_any
}

#[inline(always)]
const fn error_bar_window_ix(window: TimingWindow) -> usize {
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
fn error_bar_push_tick<const N: usize>(
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
fn error_bar_average_offset_s(
    samples: &mut VecDeque<(f32, f32)>,
    music_time_s: f32,
    offset_s: f32,
) -> f32 {
    let now_ms = ((music_time_s * 100.0).round() * 10.0).max(0.0);
    samples.push_back((now_ms, offset_s));

    const WINDOW_MS: f32 = 400.0;
    while let Some((t, _)) = samples.front() {
        if now_ms - *t <= WINDOW_MS {
            break;
        }
        samples.pop_front();
    }

    let mut sum = 0.0_f32;
    let mut count: usize = 0;
    let mut oldest_in_window: Option<f32> = None;
    for &(t, v) in samples.iter().rev() {
        if now_ms - t > WINDOW_MS {
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

#[inline(always)]
fn error_bar_register_tap(
    state: &mut State,
    player: usize,
    judgment: &Judgment,
    tap_music_time_s: f32,
) {
    let prof = &state.player_profiles[player];
    let mut error_bar_mask = profile::normalize_error_bar_mask(prof.error_bar_active_mask);
    if error_bar_mask == 0 {
        error_bar_mask = profile::error_bar_mask_from_style(prof.error_bar, prof.error_bar_text);
    }
    let show_text = (error_bar_mask & profile::ERROR_BAR_BIT_TEXT) != 0;
    let show_monochrome = (error_bar_mask & profile::ERROR_BAR_BIT_MONOCHROME) != 0;
    let show_colorful = (error_bar_mask & profile::ERROR_BAR_BIT_COLORFUL) != 0;
    let show_highlight = (error_bar_mask & profile::ERROR_BAR_BIT_HIGHLIGHT) != 0;
    let show_average = (error_bar_mask & profile::ERROR_BAR_BIT_AVERAGE) != 0;
    let show_fa_plus_window = prof.show_fa_plus_window;
    let fa_plus_window_s = player_fa_plus_window_s(state, player);
    let error_bar_trim = prof.error_bar_trim;
    let error_bar_multi_tick = prof.error_bar_multi_tick;
    let error_ms_display = prof.error_ms_display;
    let Some(window) = judgment.window else {
        return;
    };

    let now = state.total_elapsed_in_screen;
    let offset_s = judgment.time_error_ms / 1000.0;
    let p = &mut state.players[player];

    if error_ms_display {
        p.offset_indicator_text = Some(OffsetIndicatorText {
            started_at: now,
            offset_ms: judgment.time_error_ms,
            window,
        });
    }

    if show_text {
        let threshold_s = if show_fa_plus_window {
            fa_plus_window_s
        } else {
            state.timing_profile.windows_s[0]
        };
        if offset_s.abs() > threshold_s {
            p.error_bar_text = Some(ErrorBarText {
                started_at: now,
                early: offset_s < 0.0,
            });
        } else {
            p.error_bar_text = None;
        }
    } else {
        p.error_bar_text = None;
    }

    if !(show_monochrome || show_colorful || show_highlight || show_average) {
        return;
    }

    let max_window_ix = match error_bar_trim {
        profile::ErrorBarTrim::Off => 4,
        profile::ErrorBarTrim::Fantastic => 0,
        profile::ErrorBarTrim::Excellent => 1,
        profile::ErrorBarTrim::Great => 2,
    };
    let max_offset_s = state.timing_profile.windows_s[max_window_ix];
    let clamped_offset_s = if max_offset_s.is_finite() && max_offset_s > 0.0 {
        offset_s.clamp(-max_offset_s, max_offset_s)
    } else {
        offset_s
    };

    let tick = ErrorBarTick {
        started_at: now,
        offset_s: clamped_offset_s,
        window,
    };

    if show_monochrome {
        error_bar_push_tick(
            &mut p.error_bar_mono_ticks,
            &mut p.error_bar_mono_next,
            error_bar_multi_tick,
            tick,
        );
    }

    if show_colorful || show_highlight {
        error_bar_push_tick(
            &mut p.error_bar_color_ticks,
            &mut p.error_bar_color_next,
            error_bar_multi_tick,
            tick,
        );
        p.error_bar_color_bar_started_at = Some(now);
    }

    if show_highlight {
        let is_top = if show_fa_plus_window {
            window == TimingWindow::W0
        } else {
            window == TimingWindow::W1
        };
        let flash_window = if offset_s.abs() > max_offset_s {
            match max_window_ix {
                0 => TimingWindow::W1,
                1 => TimingWindow::W2,
                2 => TimingWindow::W3,
                3 => TimingWindow::W4,
                _ => TimingWindow::W5,
            }
        } else {
            window
        };
        let wi = error_bar_window_ix(flash_window);
        if is_top {
            p.error_bar_color_flash_early[wi] = Some(now);
            p.error_bar_color_flash_late[wi] = Some(now);
        } else if offset_s < 0.0 {
            p.error_bar_color_flash_early[wi] = Some(now);
        } else {
            p.error_bar_color_flash_late[wi] = Some(now);
        }
    }

    if show_average {
        let avg =
            error_bar_average_offset_s(&mut p.error_bar_avg_samples, tap_music_time_s, offset_s);
        let avg_clamped = if max_offset_s.is_finite() && max_offset_s > 0.0 {
            avg.clamp(-max_offset_s, max_offset_s)
        } else {
            avg
        };
        error_bar_push_tick(
            &mut p.error_bar_avg_ticks,
            &mut p.error_bar_avg_next,
            error_bar_multi_tick,
            ErrorBarTick {
                started_at: now,
                offset_s: avg_clamped,
                window,
            },
        );
        p.error_bar_avg_bar_started_at = Some(now);
    }
}

#[inline(always)]
fn set_last_judgment(state: &mut State, player: usize, judgment: Judgment) {
    state.players[player].last_judgment = Some(JudgmentRenderInfo {
        judgment,
        started_at_screen_s: state.total_elapsed_in_screen,
    });
}

#[inline(always)]
fn render_provisional_early_rescore_feedback(
    state: &mut State,
    player: usize,
    column: usize,
    judgment: &Judgment,
    current_time: f32,
    hide_early_dw_judgments: bool,
    hide_early_dw_flash: bool,
) {
    if !hide_early_dw_judgments {
        set_last_judgment(state, player, judgment.clone());
        error_bar_register_tap(state, player, judgment, current_time);
    }

    if !hide_early_dw_flash {
        trigger_receptor_glow_pulse(state, column);
        trigger_tap_explosion(state, column, judgment.grade);
    }
}

pub fn judge_a_tap(
    state: &mut State,
    column: usize,
    current_time: f32,
    current_time_ns: SongTimeNs,
) -> bool {
    let rate = if state.music_rate.is_finite() && state.music_rate > 0.0 {
        state.music_rate
    } else {
        1.0
    };
    let timing_hit_log = timing_hit_log_enabled();
    let player = player_for_col(state, column);
    let rescore_early_hits = state.player_profiles[player].rescore_early_hits;
    let hide_early_dw_judgments = state.player_profiles[player].hide_early_dw_judgments;
    let hide_early_dw_flash = state.player_profiles[player].hide_early_dw_flash;
    let scoring_blocked = autoplay_blocks_scoring(state);
    let timing = state.player_judgment_timing[player];
    let search_window_ns = timing
        .largest_tap_window_music_ns
        .max(timing.profile_music_ns.mine_window_ns);
    let search_start_time_ns = current_time_ns.saturating_sub(search_window_ns);
    let search_end_time_ns = current_time_ns.saturating_add(search_window_ns);
    let lane_notes = &state.lane_note_indices[column];
    let (search_start_idx, search_end_idx) = lane_note_window_bounds_ns(
        lane_notes,
        &state.note_time_cache_ns,
        search_start_time_ns,
        search_end_time_ns,
    );
    if let Some((note_index, _)) = closest_lane_note_ns(
        lane_notes,
        &state.notes,
        &state.note_time_cache_ns,
        current_time_ns,
        search_start_idx,
        search_end_idx,
    ) {
        let note_row_index = state.notes[note_index].row_index;
        let note_type = state.notes[note_index].note_type;
        let time_error_music_ns =
            current_time_ns.saturating_sub(state.note_time_cache_ns[note_index]);

        if matches!(note_type, NoteType::Mine) {
            if state.notes[note_index].is_fake {
                return false;
            }
            if hit_mine(state, column, note_index, time_error_music_ns) {
                return true;
            }
            return false;
        }
        let mine_hit_on_press = if live_autoplay_enabled(state) {
            false
        } else {
            try_hit_mine_while_held(state, column, current_time_ns)
        };
        if !lane_edge_matches_note_type(true, note_type) {
            return mine_hit_on_press;
        }

        let Some(hit) = note_hit_eval(
            state,
            player,
            state.note_time_cache_ns[note_index],
            current_time_ns,
        ) else {
            return mine_hit_on_press;
        };
        let Some(row_entry) = row_entry_for_cached_row(
            &state.row_entries,
            &state.row_map_cache[player],
            note_row_index,
        ) else {
            debug_assert!(false, "missing row cache for row {note_row_index}");
            return false;
        };
        let row_rescore_track_count = count_rescore_tracks_on_row(row_entry);
        let row_note_count = usize::from(row_entry.unresolved_nonlift_count);
        let (song_offset_s, global_offset_s, lead_in_s, stream_pos_s) = if timing_hit_log {
            (
                state.song_offset_seconds,
                effective_player_global_offset_seconds(state, player),
                state.audio_lead_in_seconds.max(0.0),
                audio::get_music_stream_position_seconds(),
            )
        } else {
            (0.0, 0.0, 0.0, 0.0)
        };

        if rescore_early_hits && row_rescore_track_count == 1 {
            let note_col = state.notes[note_index].column;
            let is_early = hit.measured_offset_music_ns < 0;
            let is_bad = matches!(hit.grade, JudgeGrade::Decent | JudgeGrade::WayOff);

            if is_early && is_bad {
                if state.notes[note_index].early_result.is_none() {
                    let judgment = Judgment {
                        time_error_ms: judgment_time_error_ms_from_music_ns(
                            hit.measured_offset_music_ns,
                            rate,
                        ),
                        time_error_music_ns: hit.measured_offset_music_ns,
                        grade: hit.grade,
                        window: Some(hit.window),
                        miss_because_held: false,
                    };
                    register_provisional_early_result(state, player, note_index, judgment.clone());
                    let life_delta = judge_life_delta(hit.grade);
                    let current_music_time = current_music_time_s(state);
                    {
                        let p = &mut state.players[player];
                        if !scoring_blocked {
                            apply_life_change(p, current_music_time, life_delta);
                        }
                    }
                    if !scoring_blocked {
                        capture_failed_ex_score_inputs(state, player);
                    }
                    render_provisional_early_rescore_feedback(
                        state,
                        player,
                        note_col,
                        &judgment,
                        current_time,
                        hide_early_dw_judgments,
                        hide_early_dw_flash,
                    );
                    // Zmod parity: provisional early W4/W5 (with Rescore Early Hits enabled)
                    // should immediately drive EarlyHit-style visuals, but the later finalized
                    // W4/W5 should not produce a second bad popup/tick.
                    log_timing_hit_detail(
                        timing_hit_log,
                        stream_pos_s,
                        hit.grade,
                        note_row_index,
                        note_col,
                        state.notes[note_index].beat,
                        song_offset_s,
                        global_offset_s,
                        song_time_ns_to_seconds(hit.note_time_ns),
                        current_time,
                        current_music_time_s(state),
                        rate,
                        lead_in_s,
                    );

                    if let Some(end_time_ns) = state.hold_end_time_cache_ns[note_index]
                        && matches!(
                            state.notes[note_index].note_type,
                            NoteType::Hold | NoteType::Roll
                        )
                    {
                        start_active_hold(
                            state,
                            note_col,
                            note_index,
                            hit.note_time_ns,
                            end_time_ns,
                            current_time_ns,
                        );
                    }
                }
                return true;
            }

            if state.notes[note_index].early_result.is_some()
                && !matches!(
                    hit.grade,
                    JudgeGrade::Fantastic | JudgeGrade::Excellent | JudgeGrade::Great
                )
            {
                return true;
            }

            let (judgment, judgment_event_time) =
                build_final_note_hit_judgment(state, player, hit, rate);
            set_final_note_result(state, player, note_index, judgment.clone());

            log_timing_hit_detail(
                timing_hit_log,
                stream_pos_s,
                hit.grade,
                note_row_index,
                note_col,
                state.notes[note_index].beat,
                song_offset_s,
                global_offset_s,
                song_time_ns_to_seconds(hit.note_time_ns),
                song_time_ns_to_seconds(judgment_event_time),
                current_music_time_s(state),
                rate,
                lead_in_s,
            );

            trigger_completed_row_tap_explosions(state, player, note_row_index);
            trigger_receptor_glow_pulse(state, note_col);
            if let Some(end_time_ns) = state.hold_end_time_cache_ns[note_index]
                && matches!(
                    state.notes[note_index].note_type,
                    NoteType::Hold | NoteType::Roll
                )
            {
                start_active_hold(
                    state,
                    note_col,
                    note_index,
                    hit.note_time_ns,
                    end_time_ns,
                    current_time_ns,
                );
            }
            return true;
        }

        let Some((judge_indices, judge_count)) =
            collect_edge_judge_indices(row_note_count, note_index)
        else {
            return false;
        };

        for &idx in &judge_indices[..judge_count] {
            let note_col = state.notes[idx].column;
            let Some(hit) = note_hit_eval(
                state,
                player,
                state.note_time_cache_ns[idx],
                current_time_ns,
            ) else {
                continue;
            };
            let (judgment, judgment_event_time) =
                build_final_note_hit_judgment(state, player, hit, rate);
            set_final_note_result(state, player, idx, judgment.clone());

            log_timing_hit_detail(
                timing_hit_log,
                stream_pos_s,
                hit.grade,
                note_row_index,
                note_col,
                state.notes[idx].beat,
                song_offset_s,
                global_offset_s,
                song_time_ns_to_seconds(hit.note_time_ns),
                song_time_ns_to_seconds(judgment_event_time),
                current_music_time_s(state),
                rate,
                lead_in_s,
            );

            trigger_completed_row_tap_explosions(state, player, note_row_index);
            trigger_receptor_glow_pulse(state, note_col);
            if let Some(end_time_ns) = state.hold_end_time_cache_ns[idx]
                && matches!(state.notes[idx].note_type, NoteType::Hold | NoteType::Roll)
            {
                start_active_hold(
                    state,
                    note_col,
                    idx,
                    hit.note_time_ns,
                    end_time_ns,
                    current_time_ns,
                );
            }
        }
        return true;
    }
    if live_autoplay_enabled(state) {
        false
    } else {
        try_hit_mine_while_held(state, column, current_time_ns)
    }
}

/// Judge lift notes on button release. Mirrors tap judging's per-note path but
/// only matches NoteType::Lift.
pub fn judge_a_lift(
    state: &mut State,
    column: usize,
    current_time: f32,
    current_time_ns: SongTimeNs,
) -> bool {
    let rate = if state.music_rate.is_finite() && state.music_rate > 0.0 {
        state.music_rate
    } else {
        1.0
    };
    let timing_hit_log = timing_hit_log_enabled();
    let player = player_for_col(state, column);
    let rescore_early_hits = state.player_profiles[player].rescore_early_hits;
    let hide_early_dw_judgments = state.player_profiles[player].hide_early_dw_judgments;
    let hide_early_dw_flash = state.player_profiles[player].hide_early_dw_flash;
    let scoring_blocked = autoplay_blocks_scoring(state);
    let search_window_ns = player_largest_tap_window_ns(state, player);
    let search_start_time_ns = current_time_ns.saturating_sub(search_window_ns);
    let search_end_time_ns = current_time_ns.saturating_add(search_window_ns);
    let lane_notes = &state.lane_note_indices[column];
    let (search_start_idx, search_end_idx) = lane_note_window_bounds_ns(
        lane_notes,
        &state.note_time_cache_ns,
        search_start_time_ns,
        search_end_time_ns,
    );
    let Some((note_index, _)) = closest_lane_note_ns(
        lane_notes,
        &state.notes,
        &state.note_time_cache_ns,
        current_time_ns,
        search_start_idx,
        search_end_idx,
    ) else {
        return false;
    };
    if !lane_edge_matches_note_type(false, state.notes[note_index].note_type) {
        return false;
    }

    let Some(hit) = note_hit_eval(
        state,
        player,
        state.note_time_cache_ns[note_index],
        current_time_ns,
    ) else {
        return false;
    };
    let (song_offset_s, global_offset_s, lead_in_s, stream_pos_s) = if timing_hit_log {
        (
            state.song_offset_seconds,
            effective_player_global_offset_seconds(state, player),
            state.audio_lead_in_seconds.max(0.0),
            audio::get_music_stream_position_seconds(),
        )
    } else {
        (0.0, 0.0, 0.0, 0.0)
    };

    let note_col = state.notes[note_index].column;
    let note_row_index = state.notes[note_index].row_index;
    let note_beat = state.notes[note_index].beat;

    if rescore_early_hits {
        let Some(row_entry) = row_entry_for_cached_row(
            &state.row_entries,
            &state.row_map_cache[player],
            note_row_index,
        ) else {
            debug_assert!(false, "missing row cache for row {note_row_index}");
            return false;
        };
        let row_rescore_track_count = count_rescore_tracks_on_row(row_entry);
        let is_early = hit.measured_offset_music_ns < 0;
        let is_bad = matches!(hit.grade, JudgeGrade::Decent | JudgeGrade::WayOff);

        if row_rescore_track_count == 1 && is_early && is_bad {
            if state.notes[note_index].early_result.is_none() {
                let judgment = Judgment {
                    time_error_ms: judgment_time_error_ms_from_music_ns(
                        hit.measured_offset_music_ns,
                        rate,
                    ),
                    time_error_music_ns: hit.measured_offset_music_ns,
                    grade: hit.grade,
                    window: Some(hit.window),
                    miss_because_held: false,
                };
                register_provisional_early_result(state, player, note_index, judgment.clone());
                let life_delta = judge_life_delta(hit.grade);
                let current_music_time = current_music_time_s(state);
                if !scoring_blocked {
                    let p = &mut state.players[player];
                    apply_life_change(p, current_music_time, life_delta);
                    capture_failed_ex_score_inputs(state, player);
                }
                render_provisional_early_rescore_feedback(
                    state,
                    player,
                    note_col,
                    &judgment,
                    current_time,
                    hide_early_dw_judgments,
                    hide_early_dw_flash,
                );

                log_timing_hit_detail(
                    timing_hit_log,
                    stream_pos_s,
                    hit.grade,
                    note_row_index,
                    note_col,
                    note_beat,
                    song_offset_s,
                    global_offset_s,
                    song_time_ns_to_seconds(hit.note_time_ns),
                    current_time,
                    current_music_time_s(state),
                    rate,
                    lead_in_s,
                );
            }
            return true;
        }

        if row_rescore_track_count == 1
            && state.notes[note_index].early_result.is_some()
            && !matches!(
                hit.grade,
                JudgeGrade::Fantastic | JudgeGrade::Excellent | JudgeGrade::Great
            )
        {
            return true;
        }
    }

    let (judgment, judgment_event_time) = build_final_note_hit_judgment(state, player, hit, rate);
    set_final_note_result(state, player, note_index, judgment.clone());

    log_timing_hit_detail(
        timing_hit_log,
        stream_pos_s,
        hit.grade,
        note_row_index,
        note_col,
        note_beat,
        song_offset_s,
        global_offset_s,
        song_time_ns_to_seconds(hit.note_time_ns),
        song_time_ns_to_seconds(judgment_event_time),
        current_music_time_s(state),
        rate,
        lead_in_s,
    );

    trigger_completed_row_tap_explosions(state, player, note_row_index);
    trigger_receptor_glow_pulse(state, note_col);
    true
}

#[inline(always)]
fn run_assist_clap(state: &mut State, current_row: i32) {
    let song_row = current_row.max(0);
    if song_row < state.assist_last_crossed_row {
        state.assist_last_crossed_row = song_row;
        state.assist_clap_cursor = assist_clap_cursor_for_row(&state.assist_clap_rows, song_row);
        return;
    }

    let crossed_cursor =
        assist_clap_cursor_for_row(&state.assist_clap_rows, state.assist_last_crossed_row);
    if state.tick_mode == TickMode::Assist
        && crossed_cursor < state.assist_clap_rows.len()
        && state.assist_clap_rows[crossed_cursor] <= song_row as usize
    {
        audio::play_assist_tick(ASSIST_TICK_SFX_PATH);
    }

    state.assist_clap_cursor = assist_clap_cursor_for_row(&state.assist_clap_rows, song_row);
    state.assist_last_crossed_row = song_row;
}

#[inline(always)]
fn decay_let_go_hold_life(state: &mut State) {
    let mut i = 0;
    while i < state.decaying_hold_indices.len() {
        let note_index = state.decaying_hold_indices[i];
        let Some(note) = state.notes.get_mut(note_index) else {
            state.decaying_hold_indices.swap_remove(i);
            continue;
        };
        let Some(hold) = note.hold.as_mut() else {
            state.hold_decay_active[note_index] = false;
            state.decaying_hold_indices.swap_remove(i);
            continue;
        };
        if hold.result == Some(HoldResult::Held) || hold.let_go_started_at.is_none() {
            state.hold_decay_active[note_index] = false;
            state.decaying_hold_indices.swap_remove(i);
            continue;
        }
        let window = match note.note_type {
            NoteType::Roll => TIMING_WINDOW_SECONDS_ROLL,
            _ => TIMING_WINDOW_SECONDS_HOLD,
        };
        if window <= 0.0 {
            hold.life = 0.0;
            state.hold_decay_active[note_index] = false;
            state.decaying_hold_indices.swap_remove(i);
            continue;
        }
        let start_time = hold.let_go_started_at.unwrap();
        let base_life = hold.let_go_starting_life.clamp(0.0, MAX_HOLD_LIFE);
        if base_life <= 0.0 {
            hold.life = 0.0;
            state.hold_decay_active[note_index] = false;
            state.decaying_hold_indices.swap_remove(i);
            continue;
        }
        let rate = if state.music_rate.is_finite() && state.music_rate > 0.0 {
            state.music_rate
        } else {
            1.0
        };
        let elapsed_music =
            song_time_ns_delta_seconds(state.current_music_time_ns, start_time).max(0.0);
        let elapsed_real = elapsed_music / rate;
        hold.life = (base_life - elapsed_real / window).max(0.0);
        if hold.life <= f32::EPSILON {
            state.hold_decay_active[note_index] = false;
            state.decaying_hold_indices.swap_remove(i);
            continue;
        }
        i += 1;
    }
}

#[inline(always)]
fn queue_missed_hold_feedback(state: &mut State, note_index: usize) {
    if note_index >= state.pending_missed_hold_feedback.len()
        || state.pending_missed_hold_feedback[note_index]
    {
        return;
    }
    state.pending_missed_hold_feedback[note_index] = true;
    state.pending_missed_hold_indices.push(note_index);
}

#[inline(always)]
fn emit_pending_missed_hold_feedback(state: &mut State, current_time_ns: SongTimeNs) {
    let mut i = 0usize;
    while i < state.pending_missed_hold_indices.len() {
        let note_index = state.pending_missed_hold_indices[i];
        let Some(end_time_ns) = state
            .hold_end_time_cache_ns
            .get(note_index)
            .and_then(|t| *t)
        else {
            state.pending_missed_hold_feedback[note_index] = false;
            state.pending_missed_hold_indices.swap_remove(i);
            continue;
        };
        if current_time_ns < end_time_ns {
            i += 1;
            continue;
        }
        state.pending_missed_hold_feedback[note_index] = false;
        if let Some(note) = state.notes.get(note_index)
            && note
                .hold
                .as_ref()
                .is_some_and(|hold| hold.result == Some(HoldResult::Missed))
        {
            let column = note.column;
            if column < state.num_cols {
                state.hold_judgments[column] = Some(HoldJudgmentRenderInfo {
                    result: HoldResult::Missed,
                    started_at_screen_s: state.total_elapsed_in_screen,
                });
            }
        }
        state.pending_missed_hold_indices.swap_remove(i);
    }
}

#[inline(always)]
fn note_tracks_held_miss(note_type: NoteType) -> bool {
    matches!(note_type, NoteType::Tap | NoteType::Hold | NoteType::Roll)
}

#[inline(always)]
fn track_held_miss_windows(
    state: &mut State,
    inputs: &[bool; MAX_COLS],
    music_time_ns: SongTimeNs,
) {
    for player in 0..state.num_players {
        let largest_window_ns = player_largest_tap_window_ns(state, player);
        if largest_window_ns <= 0 {
            continue;
        }
        let future_cutoff_time_ns = music_time_ns.saturating_add(largest_window_ns);
        let (col_start, col_end) = player_col_range(state, player);
        let (note_start, note_end) = player_note_range(state, player);
        let mut seen_tracks = [false; MAX_COLS];
        let mut cursor = state.next_tap_miss_cursor[player].max(note_start);
        while cursor < note_end {
            let note_time_ns = state.note_time_cache_ns[cursor];
            if note_time_ns > future_cutoff_time_ns {
                break;
            }
            let note = &state.notes[cursor];
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
                state.tap_miss_held_window[cursor] = true;
            }
            cursor += 1;
        }
    }
}

#[inline(always)]
fn mine_avoid_log_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| {
        std::env::var("DEADSYNC_MINE_AVOID_LOG").is_ok_and(|v| {
            let v = v.trim();
            v == "1" || v.eq_ignore_ascii_case("true") || v.eq_ignore_ascii_case("yes")
        })
    })
}

#[inline(always)]
fn apply_time_based_mine_avoidance(state: &mut State, music_time_ns: SongTimeNs) {
    let cutoff_time_ns = music_time_ns.saturating_sub(max_step_distance_ns(
        &state.timing_profile,
        state.music_rate,
    ));
    let music_time_sec = song_time_ns_to_seconds(music_time_ns);
    let log_mine_avoid = mine_avoid_log_enabled();
    for player in 0..state.num_players {
        let mines_len = state.mine_note_ix[player].len();
        let mine_cursor = state.next_mine_ix_cursor[player].min(mines_len);
        let mine_end = mine_cursor
            + state.mine_note_time_ns[player][mine_cursor..]
                .partition_point(|&t| t <= cutoff_time_ns);
        let mut avoided_count = 0u32;
        for cursor in mine_cursor..mine_end {
            let note_idx = state.mine_note_ix[player][cursor];
            let note = &mut state.notes[note_idx];
            if note.can_be_judged && note.mine_result.is_none() {
                let row_index = note.row_index;
                let column = note.column;
                note.mine_result = Some(MineResult::Avoided);
                avoided_count = avoided_count.saturating_add(1);
                if log_mine_avoid {
                    trace!(
                        "MINE AVOIDED: Row {row_index}, Col {column}, Time: {music_time_sec:.2}s"
                    );
                }
            }
        }
        if avoided_count > 0 {
            state.players[player].mines_avoided = state.players[player]
                .mines_avoided
                .saturating_add(avoided_count);
        }
        state.next_mine_ix_cursor[player] = mine_end;
        let (_, note_end) = player_note_range(state, player);
        state.next_mine_avoid_cursor[player] = if mine_end < mines_len {
            state.mine_note_ix[player][mine_end]
        } else {
            note_end
        };
    }
}

#[inline(always)]
fn apply_time_based_tap_misses(state: &mut State, music_time_ns: SongTimeNs) {
    let rate = if state.music_rate.is_finite() && state.music_rate > 0.0 {
        state.music_rate
    } else {
        1.0
    };
    let music_time_sec = song_time_ns_to_seconds(music_time_ns);
    let cutoff_time_ns =
        music_time_ns.saturating_sub(max_step_distance_ns(&state.timing_profile, rate));
    for player in 0..state.num_players {
        let (note_start, note_end) = player_note_range(state, player);
        let should_score_miss = state.score_missed_holds_rolls[player];
        let mut cursor = state.next_tap_miss_cursor[player].max(note_start);
        while cursor < note_end {
            let note_time_ns = state.note_time_cache_ns[cursor];
            if note_time_ns > cutoff_time_ns {
                break;
            }
            let (row, col, beat) = {
                let note = &state.notes[cursor];
                if matches!(note.note_type, NoteType::Mine)
                    || !note.can_be_judged
                    || note.result.is_some()
                {
                    cursor += 1;
                    continue;
                }
                (note.row_index, note.column, note.beat)
            };
            {
                let miss_offset_music_ns = music_time_ns.saturating_sub(note_time_ns);
                let miss_because_held = state
                    .tap_miss_held_window
                    .get(cursor)
                    .copied()
                    .unwrap_or(false);
                let miss = Judgment {
                    time_error_ms: judgment_time_error_ms_from_music_ns(miss_offset_music_ns, rate),
                    time_error_music_ns: miss_offset_music_ns,
                    grade: JudgeGrade::Miss,
                    window: None,
                    miss_because_held,
                };
                let judgment = state.notes[cursor].early_result.clone().unwrap_or(miss);
                let judgment_grade = judgment.grade;
                let judgment_time_error_ms = judgment.time_error_ms;
                let mut queue_missed_feedback = false;
                if judgment_grade == JudgeGrade::Miss
                    && let Some(hold) = state.notes[cursor].hold.as_mut()
                    && hold.result != Some(HoldResult::Held)
                {
                    if should_score_miss {
                        hold.result = Some(HoldResult::LetGo);
                    } else {
                        hold.result = Some(HoldResult::Missed);
                        queue_missed_feedback = true;
                    }
                    begin_hold_life_decay(
                        hold,
                        &mut state.hold_decay_active,
                        &mut state.decaying_hold_indices,
                        cursor,
                        music_time_ns,
                    );
                }
                if queue_missed_feedback {
                    queue_missed_hold_feedback(state, cursor);
                }
                set_final_note_result(state, player, cursor, judgment);
                if log::log_enabled!(log::Level::Debug) {
                    let note_time = song_time_ns_to_seconds(note_time_ns);
                    let song_offset_s = state.song_offset_seconds;
                    let global_offset_s = effective_player_global_offset_seconds(state, player);
                    let lead_in_s = state.audio_lead_in_seconds.max(0.0);
                    let stream_pos_s = audio::get_music_stream_position_seconds();
                    let expected_stream_for_note_s =
                        note_time / rate + lead_in_s + global_offset_s * (1.0 - rate) / rate;
                    let expected_stream_for_miss_s =
                        music_time_sec / rate + lead_in_s + global_offset_s * (1.0 - rate) / rate;
                    let stream_delta_note_ms = (stream_pos_s - expected_stream_for_note_s) * 1000.0;
                    let stream_delta_miss_ms = (stream_pos_s - expected_stream_for_miss_s) * 1000.0;

                    debug!(
                        concat!(
                            "TIMING MISS: row={}, col={}, beat={:.3}, ",
                            "song_offset_s={:.4}, global_offset_s={:.4}, ",
                            "note_time_s={:.6}, miss_time_s={:.6}, ",
                            "offset_ms={:.2}, rate={:.3}, lead_in_s={:.4}, ",
                            "stream_pos_s={:.6}, stream_note_s={:.6}, stream_delta_note_ms={:.2}, ",
                            "stream_miss_s={:.6}, stream_delta_miss_ms={:.2}"
                        ),
                        row,
                        col,
                        beat,
                        song_offset_s,
                        global_offset_s,
                        note_time,
                        music_time_sec,
                        judgment_time_error_ms,
                        rate,
                        lead_in_s,
                        stream_pos_s,
                        expected_stream_for_note_s,
                        stream_delta_note_ms,
                        expected_stream_for_miss_s,
                        stream_delta_miss_ms,
                    );
                }
                debug!("MISSED (time-based): Row {row}");
            }
            cursor += 1;
        }
        state.next_tap_miss_cursor[player] = cursor;
    }
}

pub fn update(state: &mut State, delta_time: f32) -> GameplayAction {
    if let Some(exit) = state.exit_transition {
        state.total_elapsed_in_screen += delta_time;
        if exit.started_at.elapsed().as_secs_f32() >= exit_total_seconds(exit.kind) {
            state.exit_transition = None;
            return GameplayAction::NavigateNoFade(gameplay_exit_for_kind(exit.kind));
        }
        return GameplayAction::None;
    }

    let trace_enabled = log::log_enabled!(log::Level::Trace);
    let frame_trace_started = if trace_enabled {
        Some(Instant::now())
    } else {
        None
    };
    let mut phase_timings = GameplayUpdatePhaseTimings::default();

    if let Some(at) = state.hold_to_exit_aborted_at
        && at.elapsed().as_secs_f32() >= GIVE_UP_ABORT_TEXT_SECONDS
    {
        state.hold_to_exit_aborted_at = None;
    }

    // Music time driven directly by the audio device clock, interpolated
    // between callbacks for smooth, continuous motion.
    let song_clock = current_song_clock_snapshot(state);
    let lead_in = state.audio_lead_in_seconds.max(0.0);
    let previous_music_time_ns = state.current_music_time_ns;
    let mut music_time_ns = song_clock.song_time_ns;
    let is_first_update = state.total_elapsed_in_screen <= f32::EPSILON;
    if is_first_update {
        const STARTUP_MAX_FORWARD_JUMP_NS: SongTimeNs = 1_000_000_000;
        let jump_ns = music_time_ns.saturating_sub(previous_music_time_ns);
        if jump_ns > STARTUP_MAX_FORWARD_JUMP_NS {
            let previous_music_time = song_time_ns_to_seconds(previous_music_time_ns);
            let music_time_sec = song_time_ns_to_seconds(music_time_ns);
            let jump_s = song_time_ns_delta_seconds(music_time_ns, previous_music_time_ns);
            warn!(
                "Discarding anomalous first-frame music time jump ({jump_s:.3}s): prev={previous_music_time:.3}, now={music_time_sec:.3}, lead_in={lead_in:.3}"
            );
            music_time_ns = previous_music_time_ns;
        }
    }
    let music_time_sec = song_time_ns_to_seconds(music_time_ns);
    state.current_music_time_ns = music_time_ns;
    let display_diag_host_nanos = if song_clock.valid_at_host_nanos != 0 {
        song_clock.valid_at_host_nanos
    } else {
        crate::engine::host_time::instant_nanos(Instant::now())
    };
    let display_music_time_ns = frame_stable_display_music_time_ns(
        &mut state.display_clock,
        &mut state.display_clock_diag,
        display_diag_host_nanos,
        music_time_ns,
        delta_time,
        song_clock.seconds_per_second,
        is_first_update,
    );
    state.current_music_time_display = song_time_ns_to_seconds(display_music_time_ns);

    if let (Some(key), Some(start_time)) = (state.hold_to_exit_key, state.hold_to_exit_start) {
        let hold_s = match key {
            HoldToExitKey::Start => GIVE_UP_HOLD_SECONDS,
            HoldToExitKey::Back => BACK_OUT_HOLD_SECONDS,
        };
        if start_time.elapsed().as_secs_f32() >= hold_s {
            if key == HoldToExitKey::Start && music_time_ns >= state.notes_end_time_ns {
                state.song_completed_naturally = true;
            }
            match key {
                HoldToExitKey::Start => {
                    begin_exit_transition(state, ExitTransitionKind::Out);
                }
                HoldToExitKey::Back => {
                    begin_exit_transition(state, ExitTransitionKind::Cancel);
                }
            }
            finalize_update_trace(
                state,
                delta_time,
                music_time_sec,
                frame_trace_started,
                phase_timings,
            );
            return GameplayAction::None;
        }
    }
    state.total_elapsed_in_screen += delta_time;

    let pre_notes_started = if trace_enabled {
        Some(Instant::now())
    } else {
        None
    };
    {
        let beat_info = state
            .timing
            .get_beat_info_from_time_ns_cached(music_time_ns, &mut state.beat_info_cache);
        state.current_beat = beat_info.beat;
        state.current_beat_display = state.timing.get_beat_for_time_ns(display_music_time_ns);
        state.is_in_freeze = beat_info.is_in_freeze;
        state.is_in_delay = beat_info.is_in_delay;
        let song_row = assist_row_no_offset_ns(state, music_time_ns);
        run_assist_clap(state, song_row);

        for player in 0..state.num_players {
            let delay =
                state.global_visual_delay_seconds + state.player_visual_delay_seconds[player];
            let visible_time_ns = song_time_ns_add_seconds(display_music_time_ns, -delay);
            state.current_music_time_visible_ns[player] = visible_time_ns;
            state.current_music_time_visible[player] = song_time_ns_to_seconds(visible_time_ns);
            state.current_beat_visible[player] =
                state.timing_players[player].get_beat_for_time_ns(visible_time_ns);
        }
        refresh_active_attack_masks(state, delta_time);

        let current_bpm = state.timing.get_bpm_for_beat(state.current_beat);
        refresh_live_notefield_options(state, current_bpm);
    }
    if let Some(started) = pre_notes_started {
        phase_timings.pre_notes_us = elapsed_us_since(started);
    }

    let autoplay_started = if trace_enabled {
        Some(Instant::now())
    } else {
        None
    };
    if state.replay_mode {
        run_replay(state);
    } else {
        run_autoplay(state, music_time_ns);
    }
    if let Some(started) = autoplay_started {
        phase_timings.autoplay_us = elapsed_us_since(started);
    }

    let input_started = if trace_enabled {
        Some(Instant::now())
    } else {
        None
    };
    update_offset_adjust_hold(state);
    process_input_edges(state, trace_enabled, &mut phase_timings, song_clock);
    if let Some(started) = input_started {
        phase_timings.input_edges_us = elapsed_us_since(started);
    }

    let held_mines_started = if trace_enabled {
        Some(Instant::now())
    } else {
        None
    };
    let num_cols = state.num_cols;
    let current_inputs: [bool; MAX_COLS] = std::array::from_fn(|i| {
        if i >= num_cols {
            return false;
        }
        lane_is_pressed(state, i)
    });
    let prev_inputs = state.prev_inputs;
    if !live_autoplay_enabled(state) {
        for (col, (now_down, was_down)) in
            current_inputs.iter().copied().zip(prev_inputs).enumerate()
        {
            if now_down && was_down {
                let _ = try_hit_crossed_mines_while_held(
                    state,
                    col,
                    previous_music_time_ns,
                    music_time_ns,
                );
            }
        }
    }
    track_held_miss_windows(state, &current_inputs, music_time_ns);
    state.prev_inputs = current_inputs;
    if let Some(started) = held_mines_started {
        phase_timings.held_mines_us = elapsed_us_since(started);
    }

    let active_holds_started = if trace_enabled {
        Some(Instant::now())
    } else {
        None
    };
    update_active_holds(state, &current_inputs, music_time_ns);
    if let Some(started) = active_holds_started {
        phase_timings.active_holds_us = elapsed_us_since(started);
    }

    let hold_decay_started = if trace_enabled {
        Some(Instant::now())
    } else {
        None
    };
    decay_let_go_hold_life(state);
    emit_pending_missed_hold_feedback(state, music_time_ns);
    if let Some(started) = hold_decay_started {
        phase_timings.hold_decay_us = elapsed_us_since(started);
    }

    let visuals_started = if trace_enabled {
        Some(Instant::now())
    } else {
        None
    };
    tick_visual_effects(state, delta_time);
    if let Some(started) = visuals_started {
        phase_timings.visuals_us = elapsed_us_since(started);
    }

    let mine_avoid_started = if trace_enabled {
        Some(Instant::now())
    } else {
        None
    };
    apply_time_based_mine_avoidance(state, music_time_ns);
    if let Some(started) = mine_avoid_started {
        phase_timings.mine_avoid_us = elapsed_us_since(started);
    }

    let judged_rows_started = if trace_enabled {
        Some(Instant::now())
    } else {
        None
    };
    // ITGmania resolves already-complete rows before it promotes overdue notes
    // to misses, so a later completed row can still score on the current frame
    // even if an earlier row times out immediately afterward.
    update_judged_rows(state);
    if let Some(started) = judged_rows_started {
        phase_timings.judged_rows_us = elapsed_us_since(started);
    }

    let tap_miss_started = if trace_enabled {
        Some(Instant::now())
    } else {
        None
    };
    apply_time_based_tap_misses(state, music_time_ns);
    if let Some(started) = tap_miss_started {
        phase_timings.tap_miss_us = elapsed_us_since(started);
    }

    let density_started = if trace_enabled {
        Some(Instant::now())
    } else {
        None
    };
    update_density_graph(state, music_time_sec, trace_enabled, &mut phase_timings);
    if let Some(started) = density_started {
        phase_timings.density_us = elapsed_us_since(started);
    }

    let danger_started = if trace_enabled {
        Some(Instant::now())
    } else {
        None
    };
    update_danger_fx(state);
    if let Some(started) = danger_started {
        phase_timings.danger_us = elapsed_us_since(started);
    }

    // Match ITG's end-of-song ordering: resolve the frame's late taps, hold
    // ends, and misses before leaving gameplay, otherwise the last frame can
    // cut to evaluation before final judgments land.
    if state.current_music_time_ns >= state.music_end_time_ns {
        debug!("Music end time reached. Transitioning to evaluation.");
        state.song_completed_naturally = true;
        finalize_update_trace(
            state,
            delta_time,
            music_time_sec,
            frame_trace_started,
            phase_timings,
        );
        return GameplayAction::Navigate(GameplayExit::Complete);
    }

    if matches!(
        crate::config::get().default_fail_type,
        crate::config::DefaultFailType::Immediate
    ) && all_joined_players_failed(state)
    {
        debug!("All joined players failed. Transitioning to evaluation.");
        state.song_completed_naturally = false;
        audio::stop_music();
        finalize_update_trace(
            state,
            delta_time,
            music_time_sec,
            frame_trace_started,
            phase_timings,
        );
        return GameplayAction::Navigate(GameplayExit::Complete);
    }

    finalize_update_trace(
        state,
        delta_time,
        music_time_sec,
        frame_trace_started,
        phase_timings,
    );
    GameplayAction::None
}

fn update_danger_fx(state: &mut State) {
    let now = state.total_elapsed_in_screen;
    for player in 0..state.num_players {
        if state.player_profiles[player].hide_lifebar {
            state.danger_fx[player] = DangerFx::default();
            continue;
        }

        let fx = &mut state.danger_fx[player];
        let health = health_state_for_player(&state.players[player]);
        if fx.last_health == health {
            continue;
        }

        if state.player_profiles[player].hide_danger {
            if health == HealthState::Dead {
                fx.anim = DangerAnim::Flash {
                    started_at: now,
                    rgb: [1.0, 0.0, 0.0],
                };
            }
            fx.last_health = health;
            continue;
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
}

#[cfg(test)]
mod tests {
    use super::{
        COMBO_BREAK_ON_IMMEDIATE_HOLD_LET_GO, DisplayClockDiagRing, FinalizedRowOutcome,
        FrameStableDisplayClock, GAMEPLAY_INPUT_BACKLOG_WARN, HoldJudgmentRenderInfo,
        HoldToExitKey, INSERT_MASK_BIT_MINES, LaneIndexRun, MAX_COLS, MAX_PLAYERS,
        REPLAY_EDGE_RATE_PER_SEC, RowEntry, ScrollEffects, ScrollSpeedSetting, SongClockSnapshot,
        TickMode, TurnRng, active_hold_counts_as_pressed, add_provisional_early_score,
        advance_hold_last_held, advance_hold_life_ns, advance_judged_row_cursor,
        apply_autosync_for_row_hits, apply_global_offset_delta, apply_mines_insert,
        apply_song_offset_delta, autoplay_random_offset_music_ns_for_window,
        build_assist_clap_rows, build_attack_mask_windows_for_player, build_column_cues_for_player,
        build_lane_hold_display_runs, build_lane_note_display_runs, build_player_judgment_timing,
        build_row_entry, build_row_grids, closest_lane_note_ns, collect_edge_judge_indices,
        completed_row_final_judgment, completed_row_flash_note_indices_and_grade,
        count_rescore_tracks_on_row, crossed_mine_bounds_ns,
        effective_appearance_effects_for_player, effective_player_global_offset_seconds,
        enforce_max_simultaneous_notes, finalize_row_judgment,
        finalized_row_outcome_for_cached_row, frame_stable_display_music_time_ns, handle_input,
        input_queue_cap, lane_edge_judges_lift, lane_edge_judges_tap, lane_edge_matches_note_type,
        lane_note_window_bounds_ns, lane_press_started, lane_release_finished,
        late_note_resolution_window_ns, live_autoplay_enabled_from_flags, max_step_distance_ns,
        mine_window_bounds_ns, music_time_ns_from_song_clock, mutate_timing_arc,
        next_ready_row_in_lookahead, next_tick_mode, note_hit_eval, parse_attack_mods,
        parse_song_lua_runtime_mods, player_draw_scale_for_tilt_with_visual_mask,
        player_row_scan_state, recent_step_tracks, recompute_player_totals,
        refresh_active_attack_masks, refresh_timing_after_offset_change,
        remove_provisional_early_score, replay_edge_cap, row_entry_for_cached_row,
        row_final_grade_hides_note, score_invalid_reason_lines_for_chart,
        score_missed_holds_and_rolls, scored_hold_totals_with_carry, set_final_note_result,
        single_runtime_player_is_p2, song_time_ns_from_seconds, song_time_ns_to_seconds,
        stage_music_cut, step_calories, suppress_final_bad_rescore_visual, tick_mode_status_line,
        tick_visual_effects, turn_option_bits, update_lane_count,
    };
    use crate::engine::input::{InputEvent, InputSource, VirtualAction};
    use crate::engine::present::color;
    use crate::game::chart::{ChartData, GameplayChartData, StaminaCounts};
    use crate::game::judgment::{self, JudgeGrade, Judgment, TimingWindow};
    use crate::game::note::{HoldData, HoldResult, Note, NoteType};
    use crate::game::parsing::notes::ParsedNote;
    use crate::game::profile;
    use crate::game::song::SongData;
    use crate::game::timing::{
        ROWS_PER_BEAT, StopSegment, TimingData, TimingProfile, TimingProfileNs, TimingSegments,
    };
    use rssp::{TechCounts, stats::ArrowStats};
    use std::path::PathBuf;
    use std::sync::{Arc, LazyLock, Mutex};
    use std::time::{Duration, Instant};

    static SESSION_TEST_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

    struct SessionRestore {
        play_style: profile::PlayStyle,
        player_side: profile::PlayerSide,
        p1_joined: bool,
        p2_joined: bool,
    }

    impl Drop for SessionRestore {
        fn drop(&mut self) {
            profile::set_session_play_style(self.play_style);
            profile::set_session_player_side(self.player_side);
            profile::set_session_joined(self.p1_joined, self.p2_joined);
        }
    }

    fn with_session<R>(
        play_style: profile::PlayStyle,
        player_side: profile::PlayerSide,
        p1_joined: bool,
        p2_joined: bool,
        f: impl FnOnce() -> R,
    ) -> R {
        let _lock = SESSION_TEST_LOCK.lock().expect("session test lock");
        let _restore = SessionRestore {
            play_style: profile::get_session_play_style(),
            player_side: profile::get_session_player_side(),
            p1_joined: profile::is_session_side_joined(profile::PlayerSide::P1),
            p2_joined: profile::is_session_side_joined(profile::PlayerSide::P2),
        };
        profile::set_session_play_style(play_style);
        profile::set_session_player_side(player_side);
        profile::set_session_joined(p1_joined, p2_joined);
        f()
    }

    fn test_row_to_beat(last_row: usize) -> Vec<f32> {
        (0..=last_row)
            .map(|row| row as f32 / ROWS_PER_BEAT as f32)
            .collect()
    }

    fn test_note(column: usize, row_index: usize, note_type: NoteType) -> Note {
        Note {
            beat: row_index as f32 / ROWS_PER_BEAT as f32,
            quantization_idx: 0,
            column,
            note_type,
            row_index,
            result: None,
            early_result: None,
            hold: None,
            mine_result: None,
            is_fake: false,
            can_be_judged: true,
        }
    }

    fn test_hold(column: usize, row_index: usize, end_row_index: usize) -> Note {
        let mut note = test_note(column, row_index, NoteType::Hold);
        note.hold = Some(HoldData {
            end_row_index,
            end_beat: end_row_index as f32 / ROWS_PER_BEAT as f32,
            result: None,
            life: 1.0,
            let_go_started_at: None,
            let_go_starting_life: 0.0,
            last_held_row_index: row_index,
            last_held_beat: row_index as f32 / ROWS_PER_BEAT as f32,
        });
        note
    }

    fn judged_note(column: usize, row_index: usize, note_type: NoteType) -> Note {
        let mut note = test_note(column, row_index, note_type);
        note.result = Some(Judgment {
            time_error_ms: 0.0,
            time_error_music_ns: 0,
            grade: JudgeGrade::Great,
            window: None,
            miss_because_held: false,
        });
        note
    }

    fn note_with_judgment(
        column: usize,
        row_index: usize,
        note_type: NoteType,
        grade: JudgeGrade,
        time_error_ms: f32,
    ) -> Note {
        let mut note = test_note(column, row_index, note_type);
        note.result = Some(Judgment {
            time_error_ms,
            time_error_music_ns: judgment::judgment_time_error_music_ns_from_ms(time_error_ms, 1.0),
            grade,
            window: None,
            miss_because_held: false,
        });
        note
    }

    fn gameplay_regression_chart() -> ChartData {
        ChartData {
            chart_type: "dance-double".to_string(),
            difficulty: "Challenge".to_string(),
            description: String::new(),
            chart_name: String::new(),
            meter: 12,
            step_artist: String::new(),
            music_path: None,
            short_hash: "double-p2-regression".to_string(),
            stats: ArrowStats {
                total_arrows: 2,
                left: 0,
                down: 0,
                up: 0,
                right: 0,
                total_steps: 2,
                jumps: 0,
                hands: 0,
                mines: 0,
                holds: 0,
                rolls: 0,
                lifts: 0,
                fakes: 0,
                holding: 0,
            },
            tech_counts: TechCounts::default(),
            mines_nonfake: 0,
            stamina_counts: StaminaCounts::default(),
            total_streams: 0,
            matrix_rating: 0.0,
            max_nps: 2.0,
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
            has_chart_attacks: false,
            possible_grade_points: 0,
            holds_total: 0,
            rolls_total: 0,
            mines_total: 0,
            display_bpm: None,
            min_bpm: 150.0,
            max_bpm: 150.0,
        }
    }

    fn gameplay_regression_song() -> SongData {
        SongData {
            simfile_path: PathBuf::from("songs/Tests/double-p2-regression.ssc"),
            title: "Double P2 Regression".to_string(),
            subtitle: String::new(),
            translit_title: String::new(),
            translit_subtitle: String::new(),
            artist: "Tests".to_string(),
            genre: String::new(),
            banner_path: None,
            background_path: None,
            background_changes: Vec::new(),
            foreground_lua_changes: Vec::new(),
            has_lua: false,
            cdtitle_path: None,
            music_path: None,
            display_bpm: "150".to_string(),
            offset: 0.0,
            sample_start: None,
            sample_length: None,
            min_bpm: 150.0,
            max_bpm: 150.0,
            normalized_bpms: "0.000=150.000".to_string(),
            music_length_seconds: 60.0,
            total_length_seconds: 60,
            precise_last_second_seconds: 60.0,
            charts: vec![gameplay_regression_chart()],
        }
    }

    fn gameplay_regression_payload() -> GameplayChartData {
        let parsed_notes = vec![
            ParsedNote {
                row_index: 48,
                column: 0,
                note_type: NoteType::Tap,
                tail_row_index: None,
            },
            ParsedNote {
                row_index: 96,
                column: 7,
                note_type: NoteType::Tap,
                tail_row_index: None,
            },
        ];
        let row_to_beat = test_row_to_beat(96);
        let timing_segments = TimingSegments {
            bpms: vec![(0.0, 150.0)],
            ..TimingSegments::default()
        };
        let timing = TimingData::from_segments(0.0, 0.0, &timing_segments, &row_to_beat);
        GameplayChartData {
            notes: Vec::new(),
            parsed_notes,
            row_to_beat,
            timing_segments,
            timing,
            chart_attacks: None,
        }
    }

    fn regression_state(player_profiles: [profile::Profile; MAX_PLAYERS]) -> super::State {
        let song = Arc::new(gameplay_regression_song());
        let chart = Arc::new(song.charts[0].clone());
        let charts = [chart.clone(), chart];
        let gameplay_chart = Arc::new(gameplay_regression_payload());
        let gameplay_charts = [gameplay_chart.clone(), gameplay_chart];
        super::init(
            song,
            charts,
            gameplay_charts,
            5,
            1.0,
            [
                player_profiles[0].scroll_speed,
                player_profiles[1].scroll_speed,
            ],
            player_profiles,
            None,
            None,
            None,
            Arc::from("TEST"),
            None,
            None,
            None,
            [0; MAX_PLAYERS],
        )
    }

    #[test]
    fn regression_state_passes_hot_state_audit() {
        let profiles = [profile::Profile::default(), profile::Profile::default()];
        let state = regression_state(profiles);
        super::assert_valid_hot_state_for_tests(
            &state,
            0.0,
            state.current_music_time_display,
        );
    }

    fn test_row_entry(
        notes: &[Note],
        row_index: usize,
        nonmine_note_indices: Vec<usize>,
    ) -> RowEntry {
        let note_time_cache_ns = vec![0; notes.len()];
        build_row_entry(row_index, nonmine_note_indices, notes, &note_time_cache_ns)
    }

    fn test_row_entry_with_times(
        notes: &[Note],
        note_time_cache_ns: &[super::SongTimeNs],
        row_index: usize,
        nonmine_note_indices: Vec<usize>,
    ) -> RowEntry {
        build_row_entry(row_index, nonmine_note_indices, notes, note_time_cache_ns)
    }

    fn test_input_event(action: VirtualAction) -> InputEvent {
        let now = Instant::now();
        InputEvent {
            action,
            pressed: true,
            source: InputSource::Keyboard,
            timestamp: now,
            timestamp_host_nanos: 0,
            stored_at: now,
            emitted_at: now,
        }
    }

    #[test]
    fn single_runtime_p2_helper_includes_double() {
        assert!(!single_runtime_player_is_p2(
            profile::PlayStyle::Single,
            profile::PlayerSide::P1
        ));
        assert!(single_runtime_player_is_p2(
            profile::PlayStyle::Single,
            profile::PlayerSide::P2
        ));
        assert!(!single_runtime_player_is_p2(
            profile::PlayStyle::Double,
            profile::PlayerSide::P1
        ));
        assert!(single_runtime_player_is_p2(
            profile::PlayStyle::Double,
            profile::PlayerSide::P2
        ));
        assert!(!single_runtime_player_is_p2(
            profile::PlayStyle::Versus,
            profile::PlayerSide::P2
        ));
    }

    #[test]
    fn gameplay_init_uses_p2_modifiers_for_double_p2() {
        with_session(
            profile::PlayStyle::Double,
            profile::PlayerSide::P2,
            false,
            true,
            || {
                let mut p1 = profile::Profile::default();
                p1.display_name = "P1 runtime".to_string();
                p1.scroll_speed = ScrollSpeedSetting::XMod(1.5);
                p1.perspective = profile::Perspective::Overhead;
                p1.judgment_graphic = profile::JudgmentGraphic::new("Love");

                let mut p2 = profile::Profile::default();
                p2.display_name = "P2 runtime".to_string();
                p2.scroll_speed = ScrollSpeedSetting::CMod(777.0);
                p2.perspective = profile::Perspective::Space;
                p2.judgment_graphic = profile::JudgmentGraphic::new("Bebas");

                let state = regression_state([p1, p2.clone()]);

                assert_eq!(state.num_players, 1);
                assert_eq!(state.scroll_speed[0], ScrollSpeedSetting::CMod(777.0));
                assert_eq!(state.player_profiles[0].display_name, "P2 runtime");
                assert_eq!(
                    state.player_profiles[0].perspective,
                    profile::Perspective::Space
                );
                assert_eq!(
                    state.player_profiles[0].judgment_graphic,
                    p2.judgment_graphic
                );
                assert_eq!(state.player_color, color::decorative_rgba(3));
            },
        );
    }

    #[test]
    fn gameplay_handle_input_uses_p2_menu_buttons_for_double_p2() {
        with_session(
            profile::PlayStyle::Double,
            profile::PlayerSide::P2,
            false,
            true,
            || {
                let state_profiles = [profile::Profile::default(), profile::Profile::default()];
                let mut state = regression_state(state_profiles);

                handle_input(&mut state, &test_input_event(VirtualAction::p1_start));
                assert_eq!(state.hold_to_exit_key, None);

                handle_input(&mut state, &test_input_event(VirtualAction::p2_start));
                assert_eq!(state.hold_to_exit_key, Some(HoldToExitKey::Start));
                assert!(state.hold_to_exit_start.is_some());
            },
        );
    }

    #[test]
    fn gameplay_handle_input_uses_p2_menu_buttons_for_versus() {
        with_session(
            profile::PlayStyle::Versus,
            profile::PlayerSide::P1,
            true,
            true,
            || {
                let state_profiles = [profile::Profile::default(), profile::Profile::default()];

                let mut start_state = regression_state(state_profiles.clone());
                assert_eq!(start_state.num_players, 2);
                handle_input(&mut start_state, &test_input_event(VirtualAction::p2_start));
                assert_eq!(start_state.hold_to_exit_key, Some(HoldToExitKey::Start));
                assert!(start_state.hold_to_exit_start.is_some());

                let mut back_state = regression_state(state_profiles);
                assert_eq!(back_state.num_players, 2);
                handle_input(&mut back_state, &test_input_event(VirtualAction::p2_back));
                assert_eq!(back_state.hold_to_exit_key, Some(HoldToExitKey::Back));
                assert!(back_state.hold_to_exit_start.is_some());
            },
        );
    }

    #[test]
    fn positive_song_offset_delta_moves_notes_earlier_like_global_offset() {
        let profiles = [profile::Profile::default(), profile::Profile::default()];
        let mut song_state = regression_state(profiles.clone());
        let mut global_state = regression_state(profiles);

        let song_offset_before = song_state.song_offset_seconds;
        let global_offset_before = global_state.global_offset_seconds;
        let song_before = song_state.note_time_cache_ns[0];
        let global_before = global_state.note_time_cache_ns[0];

        assert!(apply_song_offset_delta(&mut song_state, 0.010));
        assert!(apply_global_offset_delta(&mut global_state, 0.010));

        let song_after = song_state.note_time_cache_ns[0];
        let global_after = global_state.note_time_cache_ns[0];
        let expected_delta_ns = song_time_ns_from_seconds(0.010);
        let song_delta_ns = song_before - song_after;
        let global_delta_ns = global_before - global_after;

        assert!((song_state.song_offset_seconds - (song_offset_before + 0.010)).abs() <= 1e-6);
        assert!(
            (global_state.global_offset_seconds - (global_offset_before + 0.010)).abs() <= 1e-6
        );
        assert!((song_delta_ns - expected_delta_ns).abs() <= 1);
        assert!((global_delta_ns - expected_delta_ns).abs() <= 1);
        assert!((song_delta_ns - global_delta_ns).abs() <= 1);
    }

    #[test]
    fn global_offset_delta_preserves_player_shift() {
        let profiles = [profile::Profile::default(), profile::Profile::default()];
        let mut state = regression_state(profiles);
        let shift = 0.015_f32;

        state.player_global_offset_shift_seconds[0] = shift;
        mutate_timing_arc(&mut state.timing_players[0], |timing| {
            timing.set_global_offset_seconds(state.global_offset_seconds + shift)
        });
        refresh_timing_after_offset_change(&mut state);

        let machine_before = state.global_offset_seconds;
        let effective_before = effective_player_global_offset_seconds(&state, 0);
        let note_before = state.note_time_cache_ns[0];

        assert!((effective_before - (machine_before + shift)).abs() <= 1e-6);
        assert!(apply_global_offset_delta(&mut state, 0.010));

        let effective_after = effective_player_global_offset_seconds(&state, 0);
        let note_after = state.note_time_cache_ns[0];

        assert!((state.global_offset_seconds - (machine_before + 0.010)).abs() <= 1e-6);
        assert!((effective_after - (state.global_offset_seconds + shift)).abs() <= 1e-6);
        assert_eq!(note_before - note_after, song_time_ns_from_seconds(0.010));
    }

    #[test]
    fn advance_hold_last_held_keeps_progressing_after_release_while_life_remains() {
        let timing =
            TimingData::from_segments(0.0, 0.0, &TimingSegments::default(), &test_row_to_beat(96));
        let mut hold = test_hold(0, 0, 96).hold.expect("test hold has hold data");
        hold.last_held_row_index = 24;
        hold.last_held_beat = 24.0 / ROWS_PER_BEAT as f32;

        advance_hold_last_held(&mut hold, &timing, 1.0, 0, 0.0);

        assert_eq!(hold.last_held_row_index, 48);
        assert!((hold.last_held_beat - 1.0).abs() <= 1e-6);
    }

    #[test]
    fn advance_hold_last_held_keeps_exact_beat_between_rows() {
        let timing =
            TimingData::from_segments(0.0, 0.0, &TimingSegments::default(), &test_row_to_beat(96));
        let mut hold = test_hold(0, 0, 96).hold.expect("test hold has hold data");
        hold.last_held_row_index = 24;
        hold.last_held_beat = 24.0 / ROWS_PER_BEAT as f32;

        advance_hold_last_held(&mut hold, &timing, 0.99, 0, 0.0);

        assert_eq!(hold.last_held_row_index, 48);
        assert!((hold.last_held_beat - 0.99).abs() <= 1e-6);
    }

    fn test_chart(
        stats: ArrowStats,
        timing_segments: TimingSegments,
        chart_attacks: Option<&str>,
    ) -> ChartData {
        let mines_nonfake = stats.mines;
        let (raw_min_bpm, raw_max_bpm) = timing_segments.bpms.iter().fold(
            (f32::INFINITY, 0.0_f32),
            |(min_bpm, max_bpm), &(_, bpm)| {
                if !bpm.is_finite() || bpm <= 0.0 {
                    (min_bpm, max_bpm)
                } else {
                    (min_bpm.min(bpm), max_bpm.max(bpm))
                }
            },
        );
        let (min_bpm, max_bpm) = if raw_min_bpm.is_finite() {
            (raw_min_bpm as f64, raw_max_bpm as f64)
        } else {
            (0.0, 0.0)
        };
        ChartData {
            chart_type: "dance-single".to_string(),
            difficulty: "Challenge".to_string(),
            description: String::new(),
            chart_name: String::new(),
            meter: 10,
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
            has_chart_attacks: chart_attacks.is_some_and(|attacks| !attacks.trim().is_empty()),
            possible_grade_points: 0,
            holds_total: 0,
            rolls_total: 0,
            mines_total: 0,
            display_bpm: None,
            min_bpm,
            max_bpm,
        }
    }

    #[test]
    fn tick_mode_cycles() {
        let mode = next_tick_mode(TickMode::Off);
        assert_eq!(mode, TickMode::Assist);
        assert_eq!(next_tick_mode(mode), TickMode::Hit);
        assert_eq!(next_tick_mode(TickMode::Hit), TickMode::Off);
    }

    #[test]
    fn lane_press_counts_hold_until_last_alias_release() {
        let mut keyboard = 0u8;
        let mut gamepad = 0u8;

        let mut transitions = Vec::new();
        for (source, pressed) in [
            (InputSource::Keyboard, true),
            (InputSource::Keyboard, true),
            (InputSource::Keyboard, false),
            (InputSource::Keyboard, false),
            (InputSource::Gamepad, true),
            (InputSource::Keyboard, true),
            (InputSource::Gamepad, false),
            (InputSource::Keyboard, false),
        ] {
            let was_down = keyboard != 0 || gamepad != 0;
            match source {
                InputSource::Keyboard => update_lane_count(&mut keyboard, pressed),
                InputSource::Gamepad => update_lane_count(&mut gamepad, pressed),
            }
            transitions.push((was_down, keyboard != 0 || gamepad != 0));
        }

        assert_eq!(
            transitions,
            vec![
                (false, true),
                (true, true),
                (true, true),
                (true, false),
                (false, true),
                (true, true),
                (true, true),
                (true, false),
            ]
        );
    }

    #[test]
    fn physical_edges_still_judge_while_lane_is_logically_held() {
        let mut keyboard = 0u8;

        let mut tap_edges = Vec::new();
        let mut lift_edges = Vec::new();
        let mut glow_edges = Vec::new();
        for pressed in [true, true, false, false] {
            let was_down = keyboard != 0;
            update_lane_count(&mut keyboard, pressed);
            let is_down = keyboard != 0;
            tap_edges.push(lane_edge_judges_tap(pressed));
            lift_edges.push(lane_edge_judges_lift(pressed, was_down));
            glow_edges.push((
                lane_press_started(pressed, was_down, is_down),
                lane_release_finished(pressed, was_down, is_down),
            ));
        }

        assert_eq!(tap_edges, vec![true, true, false, false]);
        assert_eq!(lift_edges, vec![false, false, true, true]);
        assert_eq!(
            glow_edges,
            vec![(true, false), (false, false), (false, false), (false, true)]
        );
    }

    #[test]
    fn live_autoplay_helper_excludes_replays() {
        assert!(live_autoplay_enabled_from_flags(true, false));
        assert!(!live_autoplay_enabled_from_flags(true, true));
        assert!(!live_autoplay_enabled_from_flags(false, false));
    }

    #[test]
    fn live_autoplay_forces_active_hold_pressed_state() {
        assert!(active_hold_counts_as_pressed(true, false));
        assert!(active_hold_counts_as_pressed(true, true));
        assert!(active_hold_counts_as_pressed(false, true));
        assert!(!active_hold_counts_as_pressed(false, false));
    }

    #[test]
    fn hold_life_advance_keeps_pressed_holds_full() {
        let advanced = advance_hold_life_ns(
            NoteType::Hold,
            0.25,
            true,
            song_time_ns_from_seconds(0.2),
            1.0,
        );
        assert_eq!(
            advanced,
            super::HoldLifeAdvance {
                life_after: super::MAX_HOLD_LIFE,
                zero_elapsed_music_ns: None,
            }
        );
    }

    #[test]
    fn hold_life_advance_reports_exact_zero_cross_time() {
        let advanced = advance_hold_life_ns(
            NoteType::Hold,
            0.25,
            false,
            song_time_ns_from_seconds(0.2),
            1.0,
        );
        assert_eq!(advanced.life_after, 0.0);
        let zero_elapsed = advanced
            .zero_elapsed_music_ns
            .expect("hold should cross zero");
        assert!((song_time_ns_to_seconds(zero_elapsed) - 0.08).abs() <= 1e-6);
    }

    #[test]
    fn hold_life_advance_split_intervals_match_single_interval() {
        let whole = advance_hold_life_ns(
            NoteType::Hold,
            1.0,
            false,
            song_time_ns_from_seconds(0.16),
            1.0,
        );
        let first = advance_hold_life_ns(
            NoteType::Hold,
            1.0,
            false,
            song_time_ns_from_seconds(0.05),
            1.0,
        );
        let split = advance_hold_life_ns(
            NoteType::Hold,
            first.life_after,
            false,
            song_time_ns_from_seconds(0.11),
            1.0,
        );

        assert!((whole.life_after - split.life_after).abs() <= 1e-6);
        assert_eq!(whole.zero_elapsed_music_ns, split.zero_elapsed_music_ns);
    }

    #[test]
    fn roll_life_advance_scales_zero_cross_with_music_rate() {
        let advanced = advance_hold_life_ns(
            NoteType::Roll,
            0.5,
            false,
            song_time_ns_from_seconds(0.4),
            2.0,
        );
        assert_eq!(advanced.life_after, 0.0);
        let zero_elapsed = advanced
            .zero_elapsed_music_ns
            .expect("roll should cross zero");
        assert!((song_time_ns_to_seconds(zero_elapsed) - 0.35).abs() <= 1e-6);
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
    fn rescore_track_count_keeps_chord_rows_multi_note_after_partial_judgment() {
        let row_index = 48usize;
        let notes = vec![
            judged_note(0, row_index, NoteType::Tap),
            test_note(1, row_index, NoteType::Tap),
        ];
        let row_entry = test_row_entry(&notes, row_index, vec![0, 1]);

        assert_eq!(count_rescore_tracks_on_row(&row_entry), 2);
    }

    #[test]
    fn rescore_track_count_includes_lifts_on_row() {
        let row_index = 48usize;
        let notes = vec![
            test_note(0, row_index, NoteType::Tap),
            test_note(1, row_index, NoteType::Lift),
        ];
        let row_entry = test_row_entry(&notes, row_index, vec![0, 1]);

        assert_eq!(count_rescore_tracks_on_row(&row_entry), 2);
    }

    #[test]
    fn cached_row_entry_lookup_uses_row_map_cache() {
        let row_index = 48usize;
        let notes = vec![
            test_note(0, row_index, NoteType::Tap),
            test_note(1, row_index, NoteType::Tap),
        ];
        let row_entries = vec![test_row_entry(&notes, row_index, vec![0, 1])];
        let mut row_map_cache = vec![u32::MAX; row_index + 1];
        row_map_cache[row_index] = 0;

        let row_entry = row_entry_for_cached_row(&row_entries, &row_map_cache, row_index)
            .expect("expected cached row entry");

        assert_eq!(row_entry.row_index, row_index);
        assert_eq!(row_entry.nonmine_note_indices, vec![0, 1]);
    }

    #[test]
    fn cached_row_entry_lookup_keeps_duplicate_rows_player_specific() {
        let row_index = 48usize;
        let notes = vec![
            test_note(0, row_index, NoteType::Tap),
            test_note(1, row_index, NoteType::Tap),
            test_note(4, row_index, NoteType::Tap),
            test_note(5, row_index, NoteType::Tap),
        ];
        let row_entries = vec![
            test_row_entry(&notes, row_index, vec![0, 1]),
            test_row_entry(&notes, row_index, vec![2, 3]),
        ];
        let mut row_map_cache: [Vec<u32>; MAX_PLAYERS] =
            std::array::from_fn(|_| vec![u32::MAX; row_index + 1]);
        row_map_cache[0][row_index] = 0;
        row_map_cache[1][row_index] = 1;

        let p1 = row_entry_for_cached_row(&row_entries, &row_map_cache[0], row_index)
            .expect("expected cached p1 row entry");
        let p2 = row_entry_for_cached_row(&row_entries, &row_map_cache[1], row_index)
            .expect("expected cached p2 row entry");

        assert_eq!(p1.nonmine_note_indices, vec![0, 1]);
        assert_eq!(p2.nonmine_note_indices, vec![2, 3]);
    }

    #[test]
    fn finalized_row_outcome_lookup_uses_row_map_cache() {
        let row_index = 48usize;
        let notes = vec![test_note(0, row_index, NoteType::Tap)];
        let mut row_entries = vec![test_row_entry(&notes, row_index, vec![0])];
        row_entries[0].final_outcome = Some(FinalizedRowOutcome {
            final_grade: JudgeGrade::Great,
        });
        let mut row_map_cache = vec![u32::MAX; row_index + 1];
        row_map_cache[row_index] = 0;

        let outcome = finalized_row_outcome_for_cached_row(&row_entries, &row_map_cache, row_index)
            .expect("expected cached finalized row outcome");

        assert_eq!(outcome.final_grade, JudgeGrade::Great);
    }

    #[test]
    fn judged_row_scan_finds_later_ready_row_past_pending_middle_row() {
        let row1 = 48usize;
        let row2 = 96usize;
        let row3 = 144usize;
        let notes = vec![
            note_with_judgment(0, row1, NoteType::Tap, JudgeGrade::Great, -8.0),
            note_with_judgment(2, row1, NoteType::Tap, JudgeGrade::Great, 8.0),
            note_with_judgment(1, row2, NoteType::Tap, JudgeGrade::Great, -8.0),
            test_note(3, row2, NoteType::Tap),
            note_with_judgment(0, row3, NoteType::Tap, JudgeGrade::Great, -6.0),
            note_with_judgment(2, row3, NoteType::Tap, JudgeGrade::Excellent, 4.0),
        ];
        let note_time_cache_ns = vec![
            song_time_ns_from_seconds(1.0),
            song_time_ns_from_seconds(1.0),
            song_time_ns_from_seconds(2.0),
            song_time_ns_from_seconds(2.0),
            song_time_ns_from_seconds(3.0),
            song_time_ns_from_seconds(3.0),
        ];
        let mut row_entries = vec![
            test_row_entry_with_times(&notes, &note_time_cache_ns, row1, vec![0, 1]),
            test_row_entry_with_times(&notes, &note_time_cache_ns, row2, vec![2, 3]),
            test_row_entry_with_times(&notes, &note_time_cache_ns, row3, vec![4, 5]),
        ];
        row_entries[0].final_outcome = Some(FinalizedRowOutcome {
            final_grade: JudgeGrade::Great,
        });

        let cursor = advance_judged_row_cursor(0, row_entries.len(), |idx| {
            player_row_scan_state(&row_entries, idx, song_time_ns_from_seconds(3.5))
        });
        assert_eq!(cursor, 1);

        let ready = next_ready_row_in_lookahead(cursor, row_entries.len(), |idx| {
            player_row_scan_state(&row_entries, idx, song_time_ns_from_seconds(3.5))
        });
        assert_eq!(ready, Some((2, row3, false)));
    }

    #[test]
    fn judged_row_cursor_stays_on_earliest_pending_row_until_it_finishes() {
        let row1 = 48usize;
        let row2 = 96usize;
        let row3 = 144usize;
        let notes = vec![
            note_with_judgment(0, row1, NoteType::Tap, JudgeGrade::Great, -8.0),
            note_with_judgment(2, row1, NoteType::Tap, JudgeGrade::Great, 8.0),
            note_with_judgment(1, row2, NoteType::Tap, JudgeGrade::Great, -8.0),
            test_note(3, row2, NoteType::Tap),
            note_with_judgment(0, row3, NoteType::Tap, JudgeGrade::Great, -6.0),
            note_with_judgment(2, row3, NoteType::Tap, JudgeGrade::Excellent, 4.0),
        ];
        let note_time_cache_ns = vec![
            song_time_ns_from_seconds(1.0),
            song_time_ns_from_seconds(1.0),
            song_time_ns_from_seconds(2.0),
            song_time_ns_from_seconds(2.0),
            song_time_ns_from_seconds(3.0),
            song_time_ns_from_seconds(3.0),
        ];
        let mut row_entries = vec![
            test_row_entry_with_times(&notes, &note_time_cache_ns, row1, vec![0, 1]),
            test_row_entry_with_times(&notes, &note_time_cache_ns, row2, vec![2, 3]),
            test_row_entry_with_times(&notes, &note_time_cache_ns, row3, vec![4, 5]),
        ];
        row_entries[0].final_outcome = Some(FinalizedRowOutcome {
            final_grade: JudgeGrade::Great,
        });
        row_entries[2].final_outcome = Some(FinalizedRowOutcome {
            final_grade: JudgeGrade::Great,
        });

        let pending_cursor = advance_judged_row_cursor(0, row_entries.len(), |idx| {
            player_row_scan_state(&row_entries, idx, song_time_ns_from_seconds(3.5))
        });
        assert_eq!(pending_cursor, 1);

        row_entries[1].final_outcome = Some(FinalizedRowOutcome {
            final_grade: JudgeGrade::Great,
        });
        let advanced_cursor = advance_judged_row_cursor(0, row_entries.len(), |idx| {
            player_row_scan_state(&row_entries, idx, song_time_ns_from_seconds(3.5))
        });
        assert_eq!(advanced_cursor, 3);
    }

    #[test]
    fn completed_row_final_judgment_waits_for_full_jump() {
        let row_index = 48usize;
        let notes = vec![
            note_with_judgment(0, row_index, NoteType::Tap, JudgeGrade::Great, -12.0),
            test_note(1, row_index, NoteType::Tap),
        ];
        let row_entry = test_row_entry(&notes, row_index, vec![0, 1]);

        assert!(completed_row_final_judgment(&notes, &row_entry).is_none());
    }

    #[test]
    fn completed_row_final_judgment_uses_last_hit_on_jump() {
        let row_index = 48usize;
        let notes = vec![
            note_with_judgment(0, row_index, NoteType::Tap, JudgeGrade::Great, -12.0),
            note_with_judgment(1, row_index, NoteType::Tap, JudgeGrade::Excellent, 8.0),
        ];
        let row_entry = test_row_entry(&notes, row_index, vec![0, 1]);

        let judgment = completed_row_final_judgment(&notes, &row_entry)
            .expect("completed jump should have a final row judgment");

        assert_eq!(judgment.grade, JudgeGrade::Excellent);
        assert!(row_final_grade_hides_note(judgment.grade));
    }

    #[test]
    fn completed_row_final_judgment_keeps_w4_w5_rows_visible() {
        let row_index = 48usize;
        let decent_notes = vec![
            note_with_judgment(0, row_index, NoteType::Tap, JudgeGrade::Great, -12.0),
            note_with_judgment(1, row_index, NoteType::Tap, JudgeGrade::Decent, 96.0),
        ];
        let wayoff_notes = vec![
            note_with_judgment(0, row_index, NoteType::Tap, JudgeGrade::Great, -12.0),
            note_with_judgment(1, row_index, NoteType::Tap, JudgeGrade::WayOff, 140.0),
        ];
        let row_entry = test_row_entry(&decent_notes, row_index, vec![0, 1]);

        let decent = completed_row_final_judgment(&decent_notes, &row_entry)
            .expect("completed row should produce a final Decent");
        let wayoff = completed_row_final_judgment(&wayoff_notes, &row_entry)
            .expect("completed row should produce a final Way Off");

        assert_eq!(decent.grade, JudgeGrade::Decent);
        assert_eq!(wayoff.grade, JudgeGrade::WayOff);
        assert!(!row_final_grade_hides_note(decent.grade));
        assert!(!row_final_grade_hides_note(wayoff.grade));
    }

    #[test]
    fn jump_row_finalization_uses_row_judgment_for_error_bar_hud() {
        with_session(
            profile::PlayStyle::Single,
            profile::PlayerSide::P1,
            true,
            false,
            || {
                let mut p1 = profile::Profile::default();
                p1.error_ms_display = true;
                p1.error_bar_text = true;
                p1.error_bar_active_mask = profile::ERROR_BAR_BIT_TEXT;

                let mut state = regression_state([p1, profile::Profile::default()]);
                let row_index = 48usize;
                state.notes = vec![
                    test_note(0, row_index, NoteType::Tap),
                    test_note(1, row_index, NoteType::Tap),
                ];
                state.note_time_cache_ns = vec![
                    song_time_ns_from_seconds(1.0),
                    song_time_ns_from_seconds(1.0),
                ];
                state.row_entries = vec![test_row_entry_with_times(
                    &state.notes,
                    &state.note_time_cache_ns,
                    row_index,
                    vec![0, 1],
                )];
                state.row_entry_ranges = [(0, 1), (0, 0)];
                state.row_map_cache = std::array::from_fn(|_| vec![u32::MAX; row_index + 1]);
                state.row_map_cache[0][row_index] = 0;
                state.note_row_entry_indices = vec![0, 0];
                state.judged_row_cursor = [0; MAX_PLAYERS];
                state.current_music_time_ns = song_time_ns_from_seconds(1.096);
                state.total_elapsed_in_screen = 12.0;

                set_final_note_result(
                    &mut state,
                    0,
                    0,
                    Judgment {
                        time_error_ms: -12.0,
                        time_error_music_ns: judgment::judgment_time_error_music_ns_from_ms(
                            -12.0, 1.0,
                        ),
                        grade: JudgeGrade::Great,
                        window: Some(TimingWindow::W3),
                        miss_because_held: false,
                    },
                );
                set_final_note_result(
                    &mut state,
                    0,
                    1,
                    Judgment {
                        time_error_ms: 96.0,
                        time_error_music_ns: judgment::judgment_time_error_music_ns_from_ms(
                            96.0, 1.0,
                        ),
                        grade: JudgeGrade::Decent,
                        window: Some(TimingWindow::W4),
                        miss_because_held: false,
                    },
                );

                assert!(state.players[0].offset_indicator_text.is_none());
                assert!(state.players[0].error_bar_text.is_none());

                finalize_row_judgment(&mut state, 0, row_index, 0, false);

                let offset = state.players[0]
                    .offset_indicator_text
                    .expect("row-final judgment should drive the offset indicator");
                assert_eq!(offset.started_at, 12.0);
                assert_eq!(offset.offset_ms, 96.0);
                assert_eq!(offset.window, TimingWindow::W4);

                let early_late = state.players[0]
                    .error_bar_text
                    .expect("row-final judgment should drive the early/late text");
                assert_eq!(early_late.started_at, 12.0);
                assert!(!early_late.early);

                let last = state.players[0]
                    .last_judgment
                    .as_ref()
                    .expect("row-final judgment should update the judgment sprite");
                assert_eq!(last.judgment.grade, JudgeGrade::Decent);
                assert_eq!(last.judgment.time_error_ms, 96.0);
                assert_eq!(last.started_at_screen_s, 12.0);
            },
        );
    }

    #[test]
    fn autosync_row_hits_use_music_time_offsets_at_rate() {
        let mut state =
            regression_state([profile::Profile::default(), profile::Profile::default()]);
        let row_index = 48usize;
        let autosync_offset_ns = song_time_ns_from_seconds(0.015);

        state.music_rate = 1.5;
        state.autosync_mode = super::AutosyncMode::Song;
        state.notes = vec![test_note(0, row_index, NoteType::Tap)];
        state.notes[0].result = Some(Judgment {
            time_error_ms: -10.0,
            time_error_music_ns: -autosync_offset_ns,
            grade: JudgeGrade::Great,
            window: Some(TimingWindow::W3),
            miss_because_held: false,
        });
        state.note_time_cache_ns = vec![song_time_ns_from_seconds(1.0)];
        state.row_entries = vec![test_row_entry_with_times(
            &state.notes,
            &state.note_time_cache_ns,
            row_index,
            vec![0],
        )];
        state.autosync_offset_samples = [autosync_offset_ns; super::AUTOSYNC_OFFSET_SAMPLE_COUNT];
        state.autosync_offset_sample_count = super::AUTOSYNC_OFFSET_SAMPLE_COUNT - 1;

        apply_autosync_for_row_hits(&mut state, 0);

        assert!((state.song_offset_seconds - 0.015).abs() <= 1e-6);
        assert_eq!(state.autosync_offset_sample_count, 0);
    }

    #[test]
    fn hold_judgment_cleanup_uses_screen_time_boundary() {
        let mut state =
            regression_state([profile::Profile::default(), profile::Profile::default()]);
        state.total_elapsed_in_screen = 5.0;
        state.hold_judgments[0] = Some(HoldJudgmentRenderInfo {
            result: HoldResult::Held,
            started_at_screen_s: 4.201,
        });
        tick_visual_effects(&mut state, 0.0);
        assert!(state.hold_judgments[0].is_some());

        state.hold_judgments[0] = Some(HoldJudgmentRenderInfo {
            result: HoldResult::Held,
            started_at_screen_s: 4.2,
        });
        tick_visual_effects(&mut state, 0.0);
        assert!(state.hold_judgments[0].is_none());
    }

    #[test]
    fn completed_row_hidden_note_indices_wait_for_full_jump() {
        let row_index = 48usize;
        let notes = vec![
            note_with_judgment(0, row_index, NoteType::Tap, JudgeGrade::Great, -12.0),
            test_note(1, row_index, NoteType::Tap),
        ];
        let row_entry = test_row_entry(&notes, row_index, vec![0, 1]);

        assert!(completed_row_flash_note_indices_and_grade(&notes, &row_entry).is_none());
    }

    #[test]
    fn completed_row_hidden_note_indices_hide_whole_jump_on_great_or_better() {
        let row_index = 48usize;
        let notes = vec![
            note_with_judgment(0, row_index, NoteType::Tap, JudgeGrade::Great, -12.0),
            note_with_judgment(1, row_index, NoteType::Tap, JudgeGrade::Excellent, 8.0),
        ];
        let row_entry = test_row_entry(&notes, row_index, vec![0, 1]);

        let (hide_indices, hide_count, final_grade) =
            completed_row_flash_note_indices_and_grade(&notes, &row_entry)
                .expect("completed jump should produce a row-final grade");

        assert!(row_final_grade_hides_note(final_grade));
        assert_eq!(hide_count, 2);
        assert_eq!(hide_indices[0], 0);
        assert_eq!(hide_indices[1], 1);
    }

    #[test]
    fn completed_row_hidden_note_indices_keep_w4_w5_rows_visible() {
        let row_index = 48usize;
        let notes = vec![
            note_with_judgment(0, row_index, NoteType::Tap, JudgeGrade::Great, -12.0),
            note_with_judgment(1, row_index, NoteType::Tap, JudgeGrade::Decent, 96.0),
        ];
        let row_entry = test_row_entry(&notes, row_index, vec![0, 1]);

        let (_, _, final_grade) = completed_row_flash_note_indices_and_grade(&notes, &row_entry)
            .expect("completed jump should produce a row-final grade");
        assert!(!row_final_grade_hides_note(final_grade));
    }

    #[test]
    fn completed_row_flash_note_indices_use_final_jump_grade_for_all_lanes() {
        let row_index = 2002usize;
        let notes = vec![
            note_with_judgment(0, row_index, NoteType::Tap, JudgeGrade::Decent, -96.0),
            note_with_judgment(1, row_index, NoteType::Tap, JudgeGrade::Great, 42.0),
        ];
        let row_entry = test_row_entry(&notes, row_index, vec![0, 1]);

        let (flash_indices, flash_count, flash_grade) =
            completed_row_flash_note_indices_and_grade(&notes, &row_entry)
                .expect("completed jump should flash every lane with the final row grade");

        assert_eq!(flash_grade, JudgeGrade::Great);
        assert_eq!(flash_count, 2);
        assert_eq!(flash_indices[0], 0);
        assert_eq!(flash_indices[1], 1);

        assert!(row_final_grade_hides_note(flash_grade));
    }

    #[test]
    fn edge_judge_indices_only_use_the_triggering_note_on_jumps() {
        let (judge_indices, judge_count) = collect_edge_judge_indices(2, 1)
            .expect("jump rows should still judge the triggering note");

        assert_eq!(judge_count, 1);
        assert_eq!(judge_indices[0], 1);
        assert_eq!(judge_indices[1], usize::MAX);
    }

    #[test]
    fn tick_status_matches_mode() {
        assert_eq!(tick_mode_status_line(TickMode::Off), None);
        assert_eq!(tick_mode_status_line(TickMode::Assist), Some("Assist Tick"));
        assert_eq!(tick_mode_status_line(TickMode::Hit), Some("Hit Tick"));
    }

    #[test]
    fn song_clock_reconstructs_past_edge_time() {
        let base = Instant::now();
        let snapshot = SongClockSnapshot {
            song_time_ns: song_time_ns_from_seconds(120.0),
            seconds_per_second: 1.5,
            valid_at: base + Duration::from_millis(24),
            valid_at_host_nanos: 0,
        };
        let edge_time = song_time_ns_to_seconds(music_time_ns_from_song_clock(snapshot, base, 0));
        assert!((edge_time - 119.964).abs() < 0.000_5);
    }

    #[test]
    fn song_clock_handles_future_edge_time() {
        let base = Instant::now();
        let snapshot = SongClockSnapshot {
            song_time_ns: song_time_ns_from_seconds(64.0),
            seconds_per_second: 2.0,
            valid_at: base,
            valid_at_host_nanos: 0,
        };
        let edge_time = song_time_ns_to_seconds(music_time_ns_from_song_clock(
            snapshot,
            base + Duration::from_millis(5),
            0,
        ));
        assert!((edge_time - 64.01).abs() < 0.000_5);
    }

    #[test]
    fn song_clock_prefers_host_clock_when_available() {
        let snapshot = SongClockSnapshot {
            song_time_ns: song_time_ns_from_seconds(32.0),
            seconds_per_second: 1.0,
            valid_at: Instant::now(),
            valid_at_host_nanos: 2_000_000_000,
        };
        let edge_time = song_time_ns_to_seconds(music_time_ns_from_song_clock(
            snapshot,
            Instant::now(),
            1_997_000_000,
        ));
        assert!((edge_time - 31.997).abs() < 0.000_5);
    }

    #[test]
    fn display_clock_snaps_on_first_update() {
        let mut display_clock = FrameStableDisplayClock::new(song_time_ns_from_seconds(10.0));
        let mut diag = DisplayClockDiagRing::new();
        let display_time = song_time_ns_to_seconds(frame_stable_display_music_time_ns(
            &mut display_clock,
            &mut diag,
            1,
            song_time_ns_from_seconds(12.5),
            0.001,
            1.0,
            true,
        ));
        assert!((display_time - 12.5).abs() < 0.000_5);
    }

    #[test]
    fn display_clock_advances_smoothly_toward_target() {
        let mut display_clock = FrameStableDisplayClock::new(song_time_ns_from_seconds(100.0));
        let mut diag = DisplayClockDiagRing::new();
        let display_time = song_time_ns_to_seconds(frame_stable_display_music_time_ns(
            &mut display_clock,
            &mut diag,
            1,
            song_time_ns_from_seconds(100.004),
            0.001,
            1.0,
            false,
        ));
        assert!(display_time > 100.0);
        assert!(display_time < 100.004);
    }

    #[test]
    fn display_clock_snaps_back_when_far_from_target() {
        let mut display_clock = FrameStableDisplayClock::new(song_time_ns_from_seconds(100.0));
        let mut diag = DisplayClockDiagRing::new();
        let display_time = song_time_ns_to_seconds(frame_stable_display_music_time_ns(
            &mut display_clock,
            &mut diag,
            1,
            song_time_ns_from_seconds(100.250),
            0.001,
            1.0,
            false,
        ));
        assert!((display_time - 100.250).abs() < 0.000_5);
    }

    #[test]
    fn assist_clap_rows_include_lifts() {
        let notes = vec![Note {
            beat: 1.0,
            quantization_idx: 0,
            column: 0,
            note_type: NoteType::Lift,
            row_index: 48,
            result: None,
            early_result: None,
            hold: None,
            mine_result: None,
            is_fake: false,
            can_be_judged: true,
        }];
        assert_eq!(build_assist_clap_rows(&notes, (0, 1)), vec![48]);
    }

    #[test]
    fn row_grids_group_sorted_rows_and_ignore_out_of_range_columns() {
        let notes = vec![
            test_note(2, 48, NoteType::Tap),
            test_note(0, 48, NoteType::Lift),
            test_note(3, 96, NoteType::Tap),
            test_note(5, 96, NoteType::Tap),
        ];

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
    fn max_simultaneous_counts_active_holds_before_row_taps() {
        let mut notes = vec![
            test_hold(0, 0, 96),
            test_note(1, 48, NoteType::Tap),
            test_note(2, 48, NoteType::Tap),
        ];

        enforce_max_simultaneous_notes(&mut notes, 2, 0, 4);

        assert_eq!(notes.len(), 2);
        assert_eq!(notes[0].column, 0);
        assert_eq!(notes[0].row_index, 0);
        assert_eq!(notes[1].column, 2);
        assert_eq!(notes[1].row_index, 48);
    }

    #[test]
    fn scored_hold_totals_with_carry_include_prior_let_go() {
        assert_eq!(scored_hold_totals_with_carry(3, 2, 4, 5), (7, 14));
    }

    #[test]
    fn immediate_hold_let_go_does_not_break_combo_by_default() {
        assert!(!COMBO_BREAK_ON_IMMEDIATE_HOLD_LET_GO);
    }

    #[test]
    fn late_note_resolution_window_matches_itg_max_step_distance_window() {
        let timing_profile = TimingProfile::default_itg_with_fa_plus();

        assert!(
            (song_time_ns_to_seconds(late_note_resolution_window_ns(&timing_profile, 1.0)) - 0.35)
                .abs()
                <= 1e-6
        );
    }

    #[test]
    fn max_step_distance_scales_with_music_rate() {
        let timing_profile = TimingProfile::default_itg_with_fa_plus();

        assert!(
            (song_time_ns_to_seconds(max_step_distance_ns(&timing_profile, 1.5)) - 0.525).abs()
                <= 1e-6
        );
    }

    #[test]
    fn mine_hits_preserve_combo_when_miss_combo_metric_is_disabled() {
        let mut player = super::init_player_runtime();
        player.combo = 50;
        player.miss_combo = 3;
        player.full_combo_grade = Some(JudgeGrade::Great);
        player.current_combo_grade = Some(JudgeGrade::Great);

        super::apply_mine_hit_combo_state(&mut player);

        assert_eq!(player.combo, 50);
        assert_eq!(player.miss_combo, 3);
        assert_eq!(player.full_combo_grade, Some(JudgeGrade::Great));
        assert_eq!(player.current_combo_grade, Some(JudgeGrade::Great));
        assert!(!player.first_fc_attempt_broken);
    }

    #[test]
    fn hold_success_preserves_existing_miss_combo() {
        let mut player = super::init_player_runtime();
        player.miss_combo = 4;

        super::apply_hold_success_combo_state(&mut player);

        assert_eq!(player.miss_combo, 4);
    }

    #[test]
    fn successful_rows_clear_miss_combo_and_extend_combo() {
        let mut player = super::init_player_runtime();
        player.combo = 20;
        player.miss_combo = 4;

        super::apply_row_combo_state(&mut player, JudgeGrade::Great, 2, 1);

        assert_eq!(player.combo, 22);
        assert_eq!(player.miss_combo, 0);
        assert_eq!(player.full_combo_grade, Some(JudgeGrade::Great));
        assert_eq!(player.current_combo_grade, Some(JudgeGrade::Great));
    }

    #[test]
    fn decent_rows_break_combo_without_clearing_existing_miss_combo() {
        let mut player = super::init_player_runtime();
        player.combo = 20;
        player.miss_combo = 4;
        player.full_combo_grade = Some(JudgeGrade::Great);
        player.current_combo_grade = Some(JudgeGrade::Great);

        super::apply_row_combo_state(&mut player, JudgeGrade::Decent, 2, 1);

        assert_eq!(player.combo, 0);
        assert_eq!(player.miss_combo, 4);
        assert!(player.full_combo_grade.is_none());
        assert!(player.current_combo_grade.is_none());
        assert!(player.first_fc_attempt_broken);
    }

    #[test]
    fn miss_rows_increment_existing_miss_combo() {
        let mut player = super::init_player_runtime();
        player.combo = 20;
        player.miss_combo = 4;

        super::apply_row_combo_state(&mut player, JudgeGrade::Miss, 2, 1);

        assert_eq!(player.combo, 0);
        assert_eq!(player.miss_combo, 5);
    }

    #[test]
    fn zero_life_events_burn_down_regen_lock() {
        let mut player = super::init_player_runtime();
        player.life = 0.5;
        player.combo_after_miss = super::REGEN_COMBO_AFTER_MISS;

        for _ in 0..super::REGEN_COMBO_AFTER_MISS {
            super::apply_life_change(&mut player, 0.0, super::LIFE_DECENT);
        }

        assert_eq!(player.combo_after_miss, 0);
        assert!((player.life - 0.5).abs() <= 1e-6);

        super::apply_life_change(&mut player, 0.0, super::LIFE_GREAT);

        assert!((player.life - 0.504).abs() <= 1e-6);
    }

    #[test]
    fn repeated_negative_life_events_stack_regen_lock_to_maximum() {
        let mut player = super::init_player_runtime();
        player.combo_after_miss = super::REGEN_COMBO_AFTER_MISS;

        super::apply_life_change(&mut player, 0.0, super::LIFE_HIT_MINE);
        assert_eq!(player.combo_after_miss, super::MAX_REGEN_COMBO_AFTER_MISS);

        super::apply_life_change(&mut player, 0.0, super::LIFE_HIT_MINE);
        assert_eq!(player.combo_after_miss, super::MAX_REGEN_COMBO_AFTER_MISS);
    }

    #[test]
    fn hot_life_penalty_clamps_negative_events_to_ten_percent() {
        let mut player = super::init_player_runtime();
        player.life = 1.0;

        super::apply_life_change(&mut player, 0.0, super::LIFE_HIT_MINE);

        assert!((player.life - 0.9).abs() <= 1e-6);
    }

    #[test]
    fn final_bad_rescore_visuals_are_suppressed_only_for_bad_rows() {
        assert!(suppress_final_bad_rescore_visual(true, JudgeGrade::Decent));
        assert!(suppress_final_bad_rescore_visual(true, JudgeGrade::WayOff));
        assert!(!suppress_final_bad_rescore_visual(true, JudgeGrade::Great));
        assert!(!suppress_final_bad_rescore_visual(
            false,
            JudgeGrade::Decent
        ));
    }

    #[test]
    fn provisional_early_score_counts_round_trip() {
        let mut player = super::init_player_runtime();
        add_provisional_early_score(&mut player, JudgeGrade::WayOff);
        add_provisional_early_score(&mut player, JudgeGrade::Decent);
        add_provisional_early_score(&mut player, JudgeGrade::WayOff);

        assert_eq!(
            player.provisional_scoring_counts
                [crate::game::judgment::judge_grade_ix(JudgeGrade::Decent)],
            1
        );
        assert_eq!(
            player.provisional_scoring_counts
                [crate::game::judgment::judge_grade_ix(JudgeGrade::WayOff)],
            2
        );

        remove_provisional_early_score(&mut player, JudgeGrade::WayOff);
        remove_provisional_early_score(&mut player, JudgeGrade::WayOff);
        remove_provisional_early_score(&mut player, JudgeGrade::WayOff);

        assert_eq!(
            player.provisional_scoring_counts
                [crate::game::judgment::judge_grade_ix(JudgeGrade::Decent)],
            1
        );
        assert_eq!(
            player.provisional_scoring_counts
                [crate::game::judgment::judge_grade_ix(JudgeGrade::WayOff)],
            0
        );
    }

    #[test]
    fn effective_ex_score_inputs_use_live_values_before_fail() {
        let player = super::init_player_runtime();
        let live = super::ExScoreInputs {
            counts: crate::game::timing::WindowCounts {
                w1: 3,
                ..crate::game::timing::WindowCounts::default()
            },
            counts_10ms: crate::game::timing::WindowCounts {
                w0: 2,
                ..crate::game::timing::WindowCounts::default()
            },
            holds_held_for_score: 4,
            holds_let_go_for_score: 1,
            rolls_held_for_score: 2,
            rolls_let_go_for_score: 1,
            mines_hit_for_score: 5,
        };

        let selected = super::effective_ex_score_inputs(&player, live);

        assert_eq!(selected.counts.w1, 3);
        assert_eq!(selected.counts_10ms.w0, 2);
        assert_eq!(selected.holds_held_for_score, 4);
        assert_eq!(selected.mines_hit_for_score, 5);
    }

    #[test]
    fn effective_ex_score_inputs_freeze_on_fail_snapshot() {
        let mut player = super::init_player_runtime();
        player.failed_ex_score_inputs = Some(super::ExScoreInputs {
            counts: crate::game::timing::WindowCounts {
                w2: 7,
                ..crate::game::timing::WindowCounts::default()
            },
            counts_10ms: crate::game::timing::WindowCounts {
                w0: 1,
                ..crate::game::timing::WindowCounts::default()
            },
            holds_held_for_score: 6,
            holds_let_go_for_score: 2,
            rolls_held_for_score: 4,
            rolls_let_go_for_score: 1,
            mines_hit_for_score: 3,
        });
        let live = super::ExScoreInputs {
            counts: crate::game::timing::WindowCounts {
                w2: 9,
                ..crate::game::timing::WindowCounts::default()
            },
            counts_10ms: crate::game::timing::WindowCounts {
                w0: 5,
                ..crate::game::timing::WindowCounts::default()
            },
            holds_held_for_score: 10,
            holds_let_go_for_score: 4,
            rolls_held_for_score: 8,
            rolls_let_go_for_score: 2,
            mines_hit_for_score: 7,
        };

        let selected = super::effective_ex_score_inputs(&player, live);

        assert_eq!(selected.counts.w2, 7);
        assert_eq!(selected.counts_10ms.w0, 1);
        assert_eq!(selected.holds_held_for_score, 6);
        assert_eq!(selected.holds_let_go_for_score, 2);
        assert_eq!(selected.rolls_held_for_score, 4);
        assert_eq!(selected.mines_hit_for_score, 3);
    }

    #[test]
    fn missed_holds_and_rolls_are_not_scored_for_dance_or_pump() {
        assert!(!score_missed_holds_and_rolls("dance-single"));
        assert!(!score_missed_holds_and_rolls("pump-single"));
        assert!(!score_missed_holds_and_rolls(" Dance-single "));
        assert!(score_missed_holds_and_rolls("kb7-single"));
    }

    #[test]
    fn recompute_totals_count_three_note_row_as_jump_and_hand() {
        let notes = vec![
            test_note(0, 48, NoteType::Tap),
            test_note(1, 48, NoteType::Tap),
            test_note(2, 48, NoteType::Tap),
        ];

        let totals = recompute_player_totals(&notes, (0, notes.len()));

        assert_eq!(totals.steps, 1);
        assert_eq!(totals.jumps, 1);
        assert_eq!(totals.hands, 1);
    }

    #[test]
    fn recompute_totals_count_hold_assisted_hand_without_losing_jump() {
        let notes = vec![
            test_hold(0, 0, 96),
            test_note(1, 48, NoteType::Tap),
            test_note(2, 48, NoteType::Tap),
        ];

        let totals = recompute_player_totals(&notes, (0, notes.len()));

        assert_eq!(totals.holds, 1);
        assert_eq!(totals.steps, 2);
        assert_eq!(totals.jumps, 1);
        assert_eq!(totals.hands, 1);
    }

    #[test]
    fn score_valid_rejects_nohands_when_chart_has_hands() {
        let mut profile = profile::Profile::default();
        profile.remove_active_mask = super::REMOVE_MASK_BIT_NO_HANDS;
        let chart = test_chart(
            ArrowStats {
                hands: 4,
                ..ArrowStats::default()
            },
            TimingSegments::default(),
            None,
        );

        assert!(
            !score_invalid_reason_lines_for_chart(&chart, &profile, profile.scroll_speed, 1.0)
                .is_empty()
        );
    }

    #[test]
    fn score_valid_keeps_turn_options_rankable() {
        let mut profile = profile::Profile::default();
        profile.turn_option = profile::TurnOption::Mirror;
        let chart = test_chart(ArrowStats::default(), TimingSegments::default(), None);

        assert!(
            score_invalid_reason_lines_for_chart(&chart, &profile, profile.scroll_speed, 1.0)
                .is_empty()
        );
    }

    #[test]
    fn score_valid_keeps_cmod_rankable_on_timing_changes() {
        let mut profile = profile::Profile::default();
        profile.scroll_speed = ScrollSpeedSetting::CMod(600.0);
        let chart = test_chart(
            ArrowStats::default(),
            TimingSegments {
                bpms: vec![(0.0, 120.0), (32.0, 128.5)],
                ..TimingSegments::default()
            },
            None,
        );

        assert!(
            score_invalid_reason_lines_for_chart(&chart, &profile, profile.scroll_speed, 1.0)
                .is_empty()
        );
    }

    #[test]
    fn score_valid_rejects_disabled_chart_attacks() {
        let mut profile = profile::Profile::default();
        profile.attack_mode = profile::AttackMode::Off;
        let chart = test_chart(
            ArrowStats::default(),
            TimingSegments::default(),
            Some("TIME=1.0:LEN=2.0:MODS=mirror"),
        );

        assert!(
            !score_invalid_reason_lines_for_chart(&chart, &profile, profile.scroll_speed, 1.0)
                .is_empty()
        );
    }

    #[test]
    fn cmod_stop_lane_window_uses_time_not_frozen_beat() {
        let timing = TimingData::from_segments(
            0.0,
            0.0,
            &TimingSegments {
                bpms: vec![(0.0, 185.0)],
                stops: vec![StopSegment {
                    beat: 32.0,
                    duration: 0.973,
                }],
                ..TimingSegments::default()
            },
            &[],
        );
        let stop_beat = 32.0;
        let note_time = timing.get_time_for_beat(stop_beat);
        let lookahead_time = note_time + 0.5;
        let lookahead_beat = timing.get_beat_for_time(lookahead_time);
        let note_times_ns = [song_time_ns_from_seconds(note_time)];
        let note_indices = [0usize];

        assert!((lookahead_beat - stop_beat).abs() < 0.000_5);
        assert_eq!(
            lane_note_window_bounds_ns(
                &note_indices,
                &note_times_ns,
                0,
                song_time_ns_from_seconds(lookahead_time),
            ),
            (0, 1)
        );
        assert!(!(stop_beat < lookahead_beat));
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
    fn attack_mod_parser_accepts_stepmania_speed_strings() {
        let mods = parse_attack_mods("C600,150% drunk,200% expand");
        assert_eq!(mods.scroll_speed, Some(ScrollSpeedSetting::CMod(600.0)));
        assert_eq!(mods.visual.drunk, Some(1.5));
        assert_eq!(mods.accel.expand, Some(2.0));
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
    fn attack_mod_parser_handles_star_prefix_offsets() {
        let mods =
            parse_attack_mods("*1000 sudden,*1000 -125% suddenoffset,*2.4 150% hiddenoffset");
        assert_eq!(mods.appearance.sudden, Some(1.0));
        assert_eq!(mods.appearance.sudden_offset, Some(-1.25));
        assert_eq!(mods.appearance.hidden_offset, Some(1.5));
        assert_eq!(mods.appearance_speed.sudden, Some(1000.0));
        assert_eq!(mods.appearance_speed.sudden_offset, Some(1000.0));
        assert_eq!(mods.appearance_speed.hidden_offset, Some(2.4));
    }

    #[test]
    fn chart_attack_sudden_offset_approaches_instead_of_snapping() {
        let mut state = regression_state(std::array::from_fn(|_| profile::Profile::default()));
        state.attack_mask_windows[0] = build_attack_mask_windows_for_player(
            Some(
                "TIME=0.000:LEN=3.000:MODS=*1000 sudden,*1000 -125% suddenoffset\
                 :TIME=0.083:LEN=3.000:MODS=*2.4 150% suddenoffset",
            ),
            profile::AttackMode::On,
            0,
            0x1234,
            10.0,
        );

        state.current_music_time_visible[0] = 0.01;
        refresh_active_attack_masks(&mut state, 0.01);
        let start = effective_appearance_effects_for_player(&state, 0);
        assert!((start.sudden - 1.0).abs() <= 1e-6);
        assert!((start.sudden_offset + 1.25).abs() <= 1e-6);

        state.current_music_time_visible[0] = 0.10;
        refresh_active_attack_masks(&mut state, 0.09);
        let mid = effective_appearance_effects_for_player(&state, 0);
        assert!(mid.sudden_offset > -1.25);
        assert!(mid.sudden_offset < 1.5);

        state.current_music_time_visible[0] = 1.10;
        refresh_active_attack_masks(&mut state, 1.0);
        let late = effective_appearance_effects_for_player(&state, 0);
        assert!(late.sudden_offset > mid.sudden_offset);
        assert!(late.sudden_offset < 1.5);
    }

    #[test]
    fn attack_mod_parser_accepts_scroll_and_perspective_overrides() {
        let mods = parse_attack_mods("30% reverse,centered,50% incoming,dark,50% blind,75% cover");
        assert_eq!(mods.scroll.reverse, Some(0.3));
        assert_eq!(mods.scroll.centered, Some(1.0));
        assert_eq!(mods.perspective.tilt, Some(-0.5));
        assert_eq!(mods.perspective.skew, Some(0.5));
        assert_eq!(mods.visibility.dark, Some(1.0));
        assert_eq!(mods.visibility.blind, Some(0.5));
        assert_eq!(mods.visibility.cover, Some(0.75));
    }

    #[test]
    fn song_lua_mod_parser_accepts_star_prefix_and_aliases() {
        let mods = parse_song_lua_runtime_mods(
            "*9999 25 invert,*9999 no hidden,*9999 3x,*9999 -25 tiny,*9999 50 incoming,*9999 15 bumpy3",
        );
        assert_eq!(mods.visual.invert, Some(0.25));
        assert_eq!(mods.appearance.hidden, Some(0.0));
        assert_eq!(mods.scroll_speed, Some(ScrollSpeedSetting::XMod(3.0)));
        assert_eq!(mods.mini_percent, Some(-25.0));
        assert_eq!(mods.perspective.tilt, Some(-0.5));
        assert_eq!(mods.perspective.skew, Some(0.5));
        assert_eq!(mods.visual.bumpy, Some(0.15));
    }

    #[test]
    fn song_lua_overlay_eases_stop_after_later_message_blocks() {
        let timing_segments = TimingSegments {
            bpms: vec![(0.0, 60.0)],
            ..TimingSegments::default()
        };
        let timing =
            TimingData::from_segments(0.0, 0.0, &timing_segments, &test_row_to_beat(8 * 48));
        let compiled = crate::game::parsing::song_lua::CompiledSongLua {
            overlays: vec![crate::game::parsing::song_lua::SongLuaOverlayActor {
                kind: crate::game::parsing::song_lua::SongLuaOverlayKind::Quad,
                name: None,
                parent_index: None,
                initial_state: crate::game::parsing::song_lua::SongLuaOverlayState::default(),
                message_commands: vec![
                    crate::game::parsing::song_lua::SongLuaOverlayMessageCommand {
                        message: "ResetBlack".to_string(),
                        blocks: vec![crate::game::parsing::song_lua::SongLuaOverlayCommandBlock {
                            start: 0.0,
                            duration: 0.0,
                            easing: None,
                            opt1: None,
                            opt2: None,
                            delta: crate::game::parsing::song_lua::SongLuaOverlayStateDelta {
                                diffuse: Some([1.0, 1.0, 1.0, 0.0]),
                                ..Default::default()
                            },
                        }],
                    },
                ],
            }],
            overlay_eases: vec![crate::game::parsing::song_lua::SongLuaOverlayEase {
                overlay_index: 0,
                unit: crate::game::parsing::song_lua::SongLuaTimeUnit::Beat,
                start: 0.0,
                limit: 8.0,
                span_mode: crate::game::parsing::song_lua::SongLuaSpanMode::Len,
                from: crate::game::parsing::song_lua::SongLuaOverlayStateDelta {
                    diffuse: Some([1.0, 1.0, 1.0, 0.0]),
                    ..Default::default()
                },
                to: crate::game::parsing::song_lua::SongLuaOverlayStateDelta {
                    diffuse: Some([1.0, 1.0, 1.0, 1.0]),
                    ..Default::default()
                },
                easing: Some("linear".to_string()),
                sustain: None,
                opt1: None,
                opt2: None,
            }],
            messages: vec![crate::game::parsing::song_lua::SongLuaMessageEvent {
                beat: 4.0,
                message: "ResetBlack".to_string(),
                persists: true,
            }],
            ..Default::default()
        };

        let windows = super::build_song_lua_overlay_ease_windows(&compiled, &timing, 0.0);

        assert_eq!(windows.len(), 1);
        assert_eq!(windows[0].cutoff_second, Some(4.0));
        assert_eq!(windows[0].end_second, 8.0);
    }

    #[test]
    fn song_lua_overlay_eases_ignore_same_timestamp_setup_blocks() {
        let timing_segments = TimingSegments {
            bpms: vec![(0.0, 60.0)],
            ..TimingSegments::default()
        };
        let timing =
            TimingData::from_segments(0.0, 0.0, &timing_segments, &test_row_to_beat(8 * 48));
        let compiled = crate::game::parsing::song_lua::CompiledSongLua {
            overlays: vec![crate::game::parsing::song_lua::SongLuaOverlayActor {
                kind: crate::game::parsing::song_lua::SongLuaOverlayKind::ActorFrame,
                name: None,
                parent_index: None,
                initial_state: crate::game::parsing::song_lua::SongLuaOverlayState::default(),
                message_commands: vec![
                    crate::game::parsing::song_lua::SongLuaOverlayMessageCommand {
                        message: "SetupZoom".to_string(),
                        blocks: vec![crate::game::parsing::song_lua::SongLuaOverlayCommandBlock {
                            start: 0.0,
                            duration: 0.0,
                            easing: None,
                            opt1: None,
                            opt2: None,
                            delta: crate::game::parsing::song_lua::SongLuaOverlayStateDelta {
                                zoom: Some(1.5),
                                ..Default::default()
                            },
                        }],
                    },
                ],
            }],
            overlay_eases: vec![crate::game::parsing::song_lua::SongLuaOverlayEase {
                overlay_index: 0,
                unit: crate::game::parsing::song_lua::SongLuaTimeUnit::Beat,
                start: 0.0,
                limit: 8.0,
                span_mode: crate::game::parsing::song_lua::SongLuaSpanMode::Len,
                from: crate::game::parsing::song_lua::SongLuaOverlayStateDelta {
                    zoom: Some(1.5),
                    ..Default::default()
                },
                to: crate::game::parsing::song_lua::SongLuaOverlayStateDelta {
                    zoom: Some(1.0),
                    ..Default::default()
                },
                easing: Some("linear".to_string()),
                sustain: None,
                opt1: None,
                opt2: None,
            }],
            messages: vec![crate::game::parsing::song_lua::SongLuaMessageEvent {
                beat: 0.0,
                message: "SetupZoom".to_string(),
                persists: true,
            }],
            ..Default::default()
        };

        let windows = super::build_song_lua_overlay_ease_windows(&compiled, &timing, 0.0);

        assert_eq!(windows.len(), 1);
        assert_eq!(windows[0].cutoff_second, None);
        assert_eq!(windows[0].end_second, 8.0);
    }

    #[test]
    fn song_lua_overlay_eases_stop_persisting_after_later_reset_messages() {
        let timing_segments = TimingSegments {
            bpms: vec![(0.0, 60.0)],
            ..TimingSegments::default()
        };
        let timing =
            TimingData::from_segments(0.0, 0.0, &timing_segments, &test_row_to_beat(8 * 48));
        let compiled = crate::game::parsing::song_lua::CompiledSongLua {
            overlays: vec![crate::game::parsing::song_lua::SongLuaOverlayActor {
                kind: crate::game::parsing::song_lua::SongLuaOverlayKind::Quad,
                name: None,
                parent_index: None,
                initial_state: crate::game::parsing::song_lua::SongLuaOverlayState::default(),
                message_commands: vec![
                    crate::game::parsing::song_lua::SongLuaOverlayMessageCommand {
                        message: "ResetBlack".to_string(),
                        blocks: vec![crate::game::parsing::song_lua::SongLuaOverlayCommandBlock {
                            start: 0.0,
                            duration: 0.0,
                            easing: None,
                            opt1: None,
                            opt2: None,
                            delta: crate::game::parsing::song_lua::SongLuaOverlayStateDelta {
                                diffuse: Some([0.0, 0.0, 0.0, 0.0]),
                                ..Default::default()
                            },
                        }],
                    },
                ],
            }],
            overlay_eases: vec![crate::game::parsing::song_lua::SongLuaOverlayEase {
                overlay_index: 0,
                unit: crate::game::parsing::song_lua::SongLuaTimeUnit::Beat,
                start: 0.0,
                limit: 2.0,
                span_mode: crate::game::parsing::song_lua::SongLuaSpanMode::Len,
                from: crate::game::parsing::song_lua::SongLuaOverlayStateDelta {
                    diffuse: Some([0.0, 0.0, 0.0, 0.0]),
                    ..Default::default()
                },
                to: crate::game::parsing::song_lua::SongLuaOverlayStateDelta {
                    diffuse: Some([0.0, 0.0, 0.0, 1.0]),
                    ..Default::default()
                },
                easing: Some("linear".to_string()),
                sustain: None,
                opt1: None,
                opt2: None,
            }],
            messages: vec![crate::game::parsing::song_lua::SongLuaMessageEvent {
                beat: 4.0,
                message: "ResetBlack".to_string(),
                persists: true,
            }],
            ..Default::default()
        };

        let windows = super::build_song_lua_overlay_ease_windows(&compiled, &timing, 0.0);

        assert_eq!(windows.len(), 1);
        assert_eq!(windows[0].end_second, 2.0);
        assert_eq!(windows[0].cutoff_second, Some(4.0));
    }

    #[test]
    fn song_lua_player_transform_eases_persist_until_later_override() {
        let timing_segments = TimingSegments {
            bpms: vec![(0.0, 60.0)],
            ..TimingSegments::default()
        };
        let timing =
            TimingData::from_segments(0.0, 0.0, &timing_segments, &test_row_to_beat(16 * 48));
        let compiled = crate::game::parsing::song_lua::CompiledSongLua {
            eases: vec![
                crate::game::parsing::song_lua::SongLuaEaseWindow {
                    player: Some(1),
                    unit: crate::game::parsing::song_lua::SongLuaTimeUnit::Beat,
                    start: 0.0,
                    limit: 4.0,
                    span_mode: crate::game::parsing::song_lua::SongLuaSpanMode::Len,
                    target: crate::game::parsing::song_lua::SongLuaEaseTarget::PlayerZoomY,
                    from: 1.0,
                    to: 0.0,
                    easing: Some("linear".to_string()),
                    sustain: None,
                    opt1: None,
                    opt2: None,
                },
                crate::game::parsing::song_lua::SongLuaEaseWindow {
                    player: Some(1),
                    unit: crate::game::parsing::song_lua::SongLuaTimeUnit::Beat,
                    start: 8.0,
                    limit: 4.0,
                    span_mode: crate::game::parsing::song_lua::SongLuaSpanMode::Len,
                    target: crate::game::parsing::song_lua::SongLuaEaseTarget::PlayerZoomY,
                    from: 0.0,
                    to: 1.0,
                    easing: Some("linear".to_string()),
                    sustain: None,
                    opt1: None,
                    opt2: None,
                },
            ],
            ..Default::default()
        };

        let (windows, unsupported) =
            super::build_song_lua_ease_windows_for_player(&compiled, &timing, 0, 0.0);

        assert_eq!(unsupported, 0);
        assert_eq!(windows.len(), 2);
        assert_eq!(windows[0].sustain_end_second, 8.0);
        assert!(
            super::song_lua_ease_window_value(&windows[0], 6.0)
                .is_some_and(|value| (value - 0.0).abs() <= 0.000_1)
        );
        assert_eq!(windows[1].sustain_end_second, f32::MAX);
        assert!(
            super::song_lua_ease_window_value(&windows[1], 20.0)
                .is_some_and(|value| (value - 1.0).abs() <= 0.000_1)
        );
    }

    #[test]
    fn attack_windows_keep_chart_only_effects_for_live_state() {
        let windows = build_attack_mask_windows_for_player(
            Some("TIME=1.0:LEN=2.0:MODS=mirror,mines"),
            profile::AttackMode::On,
            0,
            123,
            10.0,
        );
        assert_eq!(windows.len(), 1);
        assert_eq!(windows[0].chart.insert_mask, INSERT_MASK_BIT_MINES);
        assert_eq!(
            windows[0].chart.turn_bits,
            turn_option_bits(profile::TurnOption::Mirror)
        );
    }

    #[test]
    fn scroll_effects_reverse_percent_matches_itg_column_rules() {
        let scroll = ScrollEffects {
            reverse: 1.0,
            split: 1.0,
            alternate: 1.0,
            cross: 0.0,
            centered: 0.0,
        };
        assert!((scroll.reverse_percent_for_column(0, 4) - 1.0).abs() <= 1e-6);
        assert!(scroll.reverse_percent_for_column(1, 4).abs() <= 1e-6);
        assert!(scroll.reverse_percent_for_column(2, 4).abs() <= 1e-6);
        assert!((scroll.reverse_percent_for_column(3, 4) - 1.0).abs() <= 1e-6);
    }

    #[test]
    fn player_draw_scale_helper_uses_supplied_tilt() {
        let profile = crate::game::profile::Profile::default();
        let base = player_draw_scale_for_tilt_with_visual_mask(0.0, &profile, 0, 0.0);
        let tilted = player_draw_scale_for_tilt_with_visual_mask(-1.0, &profile, 0, 0.0);
        assert!((base - 1.0).abs() <= 1e-6);
        assert!((tilted - 1.5).abs() <= 1e-6);
    }

    #[test]
    fn mines_insert_converts_every_sixth_nonempty_row() {
        let timing = TimingData::from_segments(
            0.0,
            0.0,
            &TimingSegments::default(),
            &test_row_to_beat(5 * 48),
        );
        let mut notes = (0..6)
            .map(|i| {
                let row = i * 48;
                Note {
                    beat: row as f32 / 48.0,
                    quantization_idx: 0,
                    column: 0,
                    note_type: NoteType::Tap,
                    row_index: row,
                    result: None,
                    early_result: None,
                    hold: None,
                    mine_result: None,
                    is_fake: false,
                    can_be_judged: true,
                }
            })
            .collect::<Vec<_>>();
        apply_mines_insert(&mut notes, &[], &timing, 0, 4, 0, 5 * 48);
        assert!(
            notes
                .iter()
                .any(|note| note.row_index == 5 * 48 && note.note_type == NoteType::Mine)
        );
    }

    #[test]
    fn mines_insert_adds_mine_half_beat_after_hold_end() {
        let timing =
            TimingData::from_segments(0.0, 0.0, &TimingSegments::default(), &test_row_to_beat(144));
        let mut notes = vec![Note {
            beat: 0.0,
            quantization_idx: 0,
            column: 1,
            note_type: NoteType::Hold,
            row_index: 0,
            result: None,
            early_result: None,
            hold: Some(HoldData {
                end_row_index: 96,
                end_beat: 2.0,
                result: None,
                life: 1.0,
                let_go_started_at: None,
                let_go_starting_life: 0.0,
                last_held_row_index: 0,
                last_held_beat: 0.0,
            }),
            mine_result: None,
            is_fake: false,
            can_be_judged: true,
        }];
        apply_mines_insert(&mut notes, &[], &timing, 0, 4, 0, 144);
        assert!(notes.iter().any(|note| note.row_index == 120
            && note.column == 1
            && note.note_type == NoteType::Mine));
    }

    #[test]
    fn mine_window_bounds_exclude_left_edge_and_include_right_edge() {
        let mine_times_ns = [
            song_time_ns_from_seconds(1.0),
            song_time_ns_from_seconds(1.5),
            song_time_ns_from_seconds(2.0),
            song_time_ns_from_seconds(2.5),
        ];
        assert_eq!(
            mine_window_bounds_ns(
                &mine_times_ns,
                song_time_ns_from_seconds(1.5),
                song_time_ns_from_seconds(2.0),
            ),
            (1, 3)
        );
    }

    #[test]
    fn crossed_mine_bounds_skip_previous_frame_boundary() {
        let mine_times_ns = [
            song_time_ns_from_seconds(1.0),
            song_time_ns_from_seconds(1.5),
            song_time_ns_from_seconds(2.0),
            song_time_ns_from_seconds(2.5),
        ];
        assert_eq!(
            crossed_mine_bounds_ns(
                &mine_times_ns,
                song_time_ns_from_seconds(1.5),
                song_time_ns_from_seconds(2.0),
            ),
            (2, 3)
        );
    }

    #[test]
    fn lane_note_window_bounds_exclude_left_edge_and_include_right_edge() {
        let note_indices = [4usize, 9, 15];
        let mut note_times_ns = [0; 16];
        note_times_ns[4] = song_time_ns_from_seconds(1.0);
        note_times_ns[9] = song_time_ns_from_seconds(1.5);
        note_times_ns[15] = song_time_ns_from_seconds(2.0);
        assert_eq!(
            lane_note_window_bounds_ns(
                &note_indices,
                &note_times_ns,
                song_time_ns_from_seconds(1.5),
                song_time_ns_from_seconds(2.0),
            ),
            (1, 3)
        );
    }

    #[test]
    fn lane_note_display_runs_split_nonmonotonic_display_order() {
        let note_indices = [0usize, 1, 2, 3, 4];
        let note_display_beats = [10.0f32, 40.0, 20.0, 45.0, 30.0];
        assert_eq!(
            build_lane_note_display_runs(&note_indices, &note_display_beats),
            vec![
                LaneIndexRun { start: 0, end: 2 },
                LaneIndexRun { start: 2, end: 4 },
                LaneIndexRun { start: 4, end: 5 },
            ]
        );
    }

    #[test]
    fn lane_hold_display_runs_split_when_interval_bounds_decrease() {
        let hold_indices = [0usize, 1, 2, 3];
        let hold_display_beat_min_cache = [Some(10.0f32), Some(15.0), Some(12.0), Some(40.0)];
        let hold_display_beat_max_cache = [Some(20.0f32), Some(30.0), Some(32.0), Some(50.0)];
        assert_eq!(
            build_lane_hold_display_runs(
                &hold_indices,
                &hold_display_beat_min_cache,
                &hold_display_beat_max_cache,
            ),
            vec![
                LaneIndexRun { start: 0, end: 2 },
                LaneIndexRun { start: 2, end: 4 },
            ]
        );
    }

    #[test]
    fn closest_lane_note_keeps_nearer_lift_visible_to_press_edges() {
        let notes = vec![
            test_note(0, 48, NoteType::Lift),
            test_note(0, 49, NoteType::Tap),
        ];
        let note_indices = [0usize, 1];
        let note_times_ns = [
            song_time_ns_from_seconds(1.000),
            song_time_ns_from_seconds(1.012),
        ];
        let (start_idx, end_idx) = lane_note_window_bounds_ns(
            &note_indices,
            &note_times_ns,
            song_time_ns_from_seconds(0.9),
            song_time_ns_from_seconds(1.1),
        );
        let (note_index, _) = closest_lane_note_ns(
            &note_indices,
            &notes,
            &note_times_ns,
            song_time_ns_from_seconds(1.004),
            start_idx,
            end_idx,
        )
        .expect("expected a closest note");

        assert_eq!(note_index, 0);
        assert!(!lane_edge_matches_note_type(
            true,
            notes[note_index].note_type
        ));
    }

    #[test]
    fn closest_lane_note_keeps_nearer_tap_visible_to_release_edges() {
        let notes = vec![
            test_note(0, 48, NoteType::Tap),
            test_note(0, 49, NoteType::Lift),
        ];
        let note_indices = [0usize, 1];
        let note_times_ns = [
            song_time_ns_from_seconds(1.000),
            song_time_ns_from_seconds(1.012),
        ];
        let (start_idx, end_idx) = lane_note_window_bounds_ns(
            &note_indices,
            &note_times_ns,
            song_time_ns_from_seconds(0.9),
            song_time_ns_from_seconds(1.1),
        );
        let (note_index, _) = closest_lane_note_ns(
            &note_indices,
            &notes,
            &note_times_ns,
            song_time_ns_from_seconds(1.004),
            start_idx,
            end_idx,
        )
        .expect("expected a closest note");

        assert_eq!(note_index, 0);
        assert!(!lane_edge_matches_note_type(
            false,
            notes[note_index].note_type
        ));
    }

    #[test]
    fn closest_lane_note_breaks_exact_tie_toward_future_note() {
        let notes = vec![
            test_note(0, 48, NoteType::Tap),
            test_note(0, 49, NoteType::Tap),
        ];
        let note_indices = [0usize, 1];
        let note_times_ns = [1_000_000_000_i64, 1_020_000_000_i64];
        let (start_idx, end_idx) = lane_note_window_bounds_ns(
            &note_indices,
            &note_times_ns,
            song_time_ns_from_seconds(0.9),
            song_time_ns_from_seconds(1.1),
        );
        let (note_index, abs_err_ns) = closest_lane_note_ns(
            &note_indices,
            &notes,
            &note_times_ns,
            1_010_000_000_i64,
            start_idx,
            end_idx,
        )
        .expect("expected an equidistant closest note");

        assert_eq!(note_index, 1);
        assert!((song_time_ns_to_seconds(abs_err_ns.abs()) - 0.010).abs() <= 1e-6);
    }

    #[test]
    fn closest_lane_note_prefers_nearer_time_over_nearer_row() {
        let notes = vec![
            test_note(0, 48, NoteType::Tap),
            test_note(0, 60, NoteType::Tap),
        ];
        let note_indices = [0usize, 1];
        let note_times_ns = [
            song_time_ns_from_seconds(1.020),
            song_time_ns_from_seconds(1.028),
        ];
        let (start_idx, end_idx) = lane_note_window_bounds_ns(
            &note_indices,
            &note_times_ns,
            song_time_ns_from_seconds(1.0),
            song_time_ns_from_seconds(1.1),
        );
        let (note_index, abs_err_ns) = closest_lane_note_ns(
            &note_indices,
            &notes,
            &note_times_ns,
            song_time_ns_from_seconds(1.030),
            start_idx,
            end_idx,
        )
        .expect("expected the nearer note in time to win");

        assert_eq!(note_index, 1);
        assert!((song_time_ns_to_seconds(abs_err_ns.abs()) - 0.002).abs() <= 1e-6);
    }
    #[test]
    fn input_queue_cap_scales_with_fields() {
        assert_eq!(input_queue_cap(0), GAMEPLAY_INPUT_BACKLOG_WARN);
        assert_eq!(input_queue_cap(4), GAMEPLAY_INPUT_BACKLOG_WARN);
        assert_eq!(input_queue_cap(5), GAMEPLAY_INPUT_BACKLOG_WARN * 2);
        assert_eq!(input_queue_cap(8), GAMEPLAY_INPUT_BACKLOG_WARN * 2);
    }

    #[test]
    fn stage_music_cut_uses_negative_lead_in() {
        let cut = stage_music_cut(2.5);
        assert!((cut.start_sec + 2.5).abs() <= 1e-9);
        assert!(cut.length_sec.is_infinite());
        assert_eq!(cut.fade_in_sec, 0.0);
        assert_eq!(cut.fade_out_sec, 0.0);

        let clamped = stage_music_cut(-1.0);
        assert_eq!(clamped.start_sec, 0.0);
    }

    #[test]
    fn replay_edge_cap_scales_with_chart_and_skips_replay_mode() {
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
    fn step_calories_matches_itg_formula() {
        assert!((step_calories(120, 1) - 0.0266).abs() <= 1e-6);
        assert!((step_calories(120, 2) - 0.0882).abs() <= 1e-6);
        assert!((step_calories(120, 3) - 0.1498).abs() <= 1e-6);
    }

    #[test]
    fn recent_step_tracks_counts_current_press_inside_jump_window() {
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
    }

    #[test]
    fn column_cues_ignore_notes_marked_fake_by_timing() {
        let mut fake_note = test_note(1, 96, NoteType::Tap);
        fake_note.is_fake = true;
        fake_note.can_be_judged = false;

        let notes = vec![
            test_note(0, 48, NoteType::Tap),
            fake_note,
            test_note(2, 192, NoteType::Tap),
        ];
        let note_time_cache_ns = [1_000_000_000_i64, 2_000_000_000, 4_000_000_000];

        let cues =
            build_column_cues_for_player(&notes, (0, notes.len()), &note_time_cache_ns, 0, 4, 0.0);

        assert_eq!(cues.len(), 2);
        assert!((cues[0].start_time - 0.0).abs() <= 1e-6);
        assert!((cues[0].duration - 1.0).abs() <= 1e-6);
        assert_eq!(cues[0].columns.len(), 1);
        assert_eq!(cues[0].columns[0].column, 0);
        assert!((cues[1].start_time - 1.0).abs() <= 1e-6);
        assert!((cues[1].duration - 3.0).abs() <= 1e-6);
        assert_eq!(cues[1].columns.len(), 1);
        assert_eq!(cues[1].columns[0].column, 2);
    }

    #[test]
    fn note_hit_eval_scales_windows_in_music_time_ns() {
        let mut state =
            regression_state([profile::Profile::default(), profile::Profile::default()]);
        state.music_rate = 1.5;
        state.player_judgment_timing = std::array::from_fn(|player| {
            build_player_judgment_timing(
                state.timing_profile,
                &state.player_profiles[player],
                state.music_rate,
            )
        });

        let note_time_ns = song_time_ns_from_seconds(2.0);
        let great_edge_ns = state.player_judgment_timing[0].profile_music_ns.windows_ns[2];
        let way_off_edge_ns = state.player_judgment_timing[0].profile_music_ns.windows_ns[4];

        let on_great_edge = note_hit_eval(&state, 0, note_time_ns, note_time_ns + great_edge_ns)
            .expect("great edge should still judge");
        assert_eq!(on_great_edge.grade, JudgeGrade::Great);
        assert_eq!(on_great_edge.window, TimingWindow::W3);

        assert!(
            note_hit_eval(&state, 0, note_time_ns, note_time_ns + way_off_edge_ns + 1).is_none(),
            "offsets beyond the scaled way-off edge should miss",
        );
    }

    #[test]
    fn note_hit_eval_matches_tap_and_lift_zero_offsets() {
        let state = regression_state([profile::Profile::default(), profile::Profile::default()]);
        let tap_time_ns = song_time_ns_from_seconds(1.0);
        let lift_time_ns = song_time_ns_from_seconds(2.0);

        let tap_hit =
            note_hit_eval(&state, 0, tap_time_ns, tap_time_ns).expect("tap hit should judge");
        let lift_hit =
            note_hit_eval(&state, 0, lift_time_ns, lift_time_ns).expect("lift hit should judge");

        assert_eq!(tap_hit.grade, lift_hit.grade);
        assert_eq!(tap_hit.window, lift_hit.window);
        assert_eq!(tap_hit.measured_offset_music_ns, 0);
        assert_eq!(lift_hit.measured_offset_music_ns, 0);
    }
}
