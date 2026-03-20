use crate::core::audio;
use crate::core::gfx::MeshVertex;
use crate::core::input::{
    InputEdge, InputEvent, InputSource, Lane, VirtualAction, lane_from_action,
};
use crate::core::space::{is_wide, screen_center_y, screen_height, screen_width};
use crate::game::chart::{ChartData, GameplayChartData};
use crate::game::judgment::{self, JudgeGrade, Judgment, TimingWindow};
use crate::game::note::{HoldData, HoldResult, MineResult, Note, NoteType};
use crate::game::parsing::noteskin::{self, Noteskin, Style};
use crate::game::scores;
use crate::game::song::SongData;
use crate::game::timing::{
    BeatInfoCache, ROWS_PER_BEAT, TimingData, TimingProfile, classify_offset_s,
};
use crate::game::{
    life::{
        LIFE_DECENT, LIFE_EXCELLENT, LIFE_FANTASTIC, LIFE_GREAT, LIFE_HELD, LIFE_HIT_MINE,
        LIFE_LET_GO, LIFE_MISS, LIFE_WAY_OFF, REGEN_COMBO_AFTER_MISS,
    },
    profile::{self, TimingTickMode as TickMode},
    scroll::ScrollSpeedSetting,
};
use crate::screens::components::shared::{
    density_graph::{self, DensityHistCache},
    noteskin_model::ModelMeshCache,
};
use crate::screens::{Screen, ScreenAction};
use crate::ui::color;
use log::{debug, trace, warn};
use rssp::streams::StreamSegment;
use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};
use std::hash::Hasher;
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};
use twox_hash::XxHash64;
use winit::keyboard::KeyCode;

// Simply Love ScreenGameplay in/default.lua keeps intro cover actors alive for 2.0s.
pub const TRANSITION_IN_DURATION: f32 = 2.0;
// Simply Love ScreenGameplay out.lua: sleep(0.5), linear(1.0).
pub const TRANSITION_OUT_DELAY: f32 = 0.5;
pub const TRANSITION_OUT_FADE_DURATION: f32 = 1.0;
pub const TRANSITION_OUT_DURATION: f32 = TRANSITION_OUT_DELAY + TRANSITION_OUT_FADE_DURATION;
pub const MAX_COLS: usize = 8;
pub const MAX_PLAYERS: usize = 2;
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
    pub sudden: f32,
    pub stealth: f32,
    pub blink: f32,
    pub random_vanish: f32,
}

impl AppearanceEffects {
    #[inline(always)]
    fn from_mask(mask: u8) -> Self {
        Self {
            hidden: f32::from((mask & APPEARANCE_MASK_BIT_HIDDEN) != 0),
            sudden: f32::from((mask & APPEARANCE_MASK_BIT_SUDDEN) != 0),
            stealth: f32::from((mask & APPEARANCE_MASK_BIT_STEALTH) != 0),
            blink: f32::from((mask & APPEARANCE_MASK_BIT_BLINK) != 0),
            random_vanish: f32::from((mask & APPEARANCE_MASK_BIT_RANDOM_VANISH) != 0),
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
    sudden: Option<f32>,
    stealth: Option<f32>,
    blink: Option<f32>,
    random_vanish: Option<f32>,
}

impl AppearanceOverrides {
    #[inline(always)]
    fn any(self) -> bool {
        self.hidden.is_some()
            || self.sudden.is_some()
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
const MAX_NOTES_AFTER_TARGETS: usize = 64;
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
const AUTOPLAY_TAP_RELEASE_SECONDS: f32 = 0.005;
const AUTOPLAY_HOLD_RELEASE_SECONDS: f32 = 0.001;
const AUTOPLAY_OFFSET_EPSILON_SECONDS: f32 = 0.000_001;
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

#[inline(always)]
const fn input_queue_cap(num_cols: usize) -> usize {
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
fn replay_edge_cap(
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
fn receptor_glow_duration_for_col(state: &State, col: usize) -> f32 {
    let player = if state.num_players <= 1 || state.cols_per_player == 0 {
        0
    } else {
        (col / state.cols_per_player).min(state.num_players.saturating_sub(1))
    };
    state.noteskin[player]
        .as_ref()
        .map(|ns| ns.receptor_glow_behavior.duration)
        .filter(|d| *d > f32::EPSILON)
        .unwrap_or(RECEPTOR_GLOW_DURATION)
}

#[inline(always)]
fn receptor_glow_behavior_for_col(state: &State, col: usize) -> noteskin::ReceptorGlowBehavior {
    let player = if state.num_players <= 1 || state.cols_per_player == 0 {
        0
    } else {
        (col / state.cols_per_player).min(state.num_players.saturating_sub(1))
    };
    state.noteskin[player]
        .as_ref()
        .map(|ns| ns.receptor_glow_behavior)
        .unwrap_or_default()
}

#[inline(always)]
fn lane_is_pressed(state: &State, col: usize) -> bool {
    lane_counts_pressed(
        state.keyboard_lane_counts[col],
        state.gamepad_lane_counts[col],
    )
}

#[inline(always)]
const fn lane_counts_pressed(keyboard_count: u8, gamepad_count: u8) -> bool {
    keyboard_count != 0 || gamepad_count != 0
}

#[inline(always)]
fn update_lane_count(count: &mut u8, pressed: bool) {
    *count = if pressed {
        (*count).saturating_add(1)
    } else {
        (*count).saturating_sub(1)
    };
}

#[inline(always)]
const fn lane_press_started(pressed: bool, was_down: bool, is_down: bool) -> bool {
    pressed && !was_down && is_down
}

#[inline(always)]
const fn lane_release_finished(pressed: bool, was_down: bool, is_down: bool) -> bool {
    !pressed && was_down && !is_down
}

#[inline(always)]
const fn lane_edge_judges_tap(pressed: bool) -> bool {
    pressed
}

#[inline(always)]
const fn lane_edge_judges_lift(pressed: bool, was_down: bool) -> bool {
    !pressed && was_down
}

#[inline(always)]
fn trigger_receptor_glow_pulse(state: &mut State, col: usize) {
    let behavior = receptor_glow_behavior_for_col(state, col);
    state.receptor_glow_press_timers[col] = 0.0;
    state.receptor_glow_lift_start_alpha[col] = behavior.press_alpha_start;
    state.receptor_glow_lift_start_zoom[col] = behavior.press_zoom_start;
    state.receptor_glow_timers[col] = receptor_glow_duration_for_col(state, col);
}

#[inline(always)]
fn start_receptor_glow_press(state: &mut State, col: usize) {
    let behavior = receptor_glow_behavior_for_col(state, col);
    state.receptor_glow_timers[col] = 0.0;
    state.receptor_glow_press_timers[col] = behavior.press_duration;
    state.receptor_glow_lift_start_alpha[col] = behavior.press_alpha_end;
    state.receptor_glow_lift_start_zoom[col] = behavior.press_zoom_end;
}

#[inline(always)]
fn release_receptor_glow(state: &mut State, col: usize) {
    let behavior = receptor_glow_behavior_for_col(state, col);
    let (alpha, zoom) = if state.receptor_glow_press_timers[col] > f32::EPSILON
        && behavior.press_duration > f32::EPSILON
    {
        behavior.sample_press(state.receptor_glow_press_timers[col])
    } else {
        (behavior.press_alpha_end, behavior.press_zoom_end)
    };
    state.receptor_glow_press_timers[col] = 0.0;
    state.receptor_glow_lift_start_alpha[col] = alpha;
    state.receptor_glow_lift_start_zoom[col] = zoom;
    state.receptor_glow_timers[col] = receptor_glow_duration_for_col(state, col);
}

#[inline(always)]
pub fn receptor_glow_visual_for_col(state: &State, col: usize) -> Option<(f32, f32)> {
    if col >= state.num_cols {
        return None;
    }
    let behavior = receptor_glow_behavior_for_col(state, col);
    if lane_is_pressed(state, col) {
        if state.receptor_glow_press_timers[col] > f32::EPSILON
            && behavior.press_duration > f32::EPSILON
        {
            return Some(behavior.sample_press(state.receptor_glow_press_timers[col]));
        }
        return Some((behavior.press_alpha_end, behavior.press_zoom_end));
    }
    if state.receptor_glow_timers[col] > f32::EPSILON {
        return Some(behavior.sample_lift(
            state.receptor_glow_timers[col],
            state.receptor_glow_lift_start_alpha[col],
            state.receptor_glow_lift_start_zoom[col],
        ));
    }
    None
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
    fn gen_range(&mut self, upper_exclusive: usize) -> usize {
        if upper_exclusive <= 1 {
            0
        } else {
            (self.next_u32() as usize) % upper_exclusive
        }
    }

    #[inline(always)]
    fn next_f32_unit(&mut self) -> f32 {
        (self.next_u32() as f32) * (1.0 / 4_294_967_296.0)
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

fn build_row_grid(
    notes: &[Note],
    note_range: (usize, usize),
    col_offset: usize,
    cols: usize,
) -> (Vec<usize>, HashMap<usize, [usize; MAX_COLS]>) {
    let (start, end) = note_range;
    let mut map: HashMap<usize, [usize; MAX_COLS]> = HashMap::new();
    for note_idx in start..end {
        let n = &notes[note_idx];
        if n.column < col_offset {
            continue;
        }
        let local = n.column - col_offset;
        if local >= cols || local >= MAX_COLS {
            continue;
        }
        let entry = map.entry(n.row_index).or_insert([usize::MAX; MAX_COLS]);
        entry[local] = note_idx;
    }
    let mut rows: Vec<usize> = map.keys().copied().collect();
    rows.sort_unstable();
    (rows, map)
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
    // ITG parity: assist clap/metronome uses *no global offset* timing.
    // TimingData::get_beat_for_time() applies global offset internally, so
    // feed (time - offset) to cancel it out.
    let beat_no_offset = state
        .timing
        .get_beat_for_time(music_time - state.global_offset_seconds);
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
    for col in 0..cols.min(MAX_COLS) {
        if let Some(end) = hold_end_row[col] {
            if row_index > end {
                hold_end_row[col] = None;
            }
        }
    }

    for col in 0..cols.min(MAX_COLS) {
        let idx = grid[col];
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
    let (rows, mut map) = build_row_grid(notes, note_range, col_offset, cols);
    let mut rng = TurnRng::new(seed);
    let mut hold_end_row: [Option<usize>; MAX_COLS] = [None; MAX_COLS];

    for &row in &rows {
        let Some(mut grid) = map.remove(&row) else {
            continue;
        };
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

                if idx2 != usize::MAX {
                    notes[idx1].column = col_offset + t2;
                    notes[idx2].column = col_offset + t1;
                    grid.swap(t1, t2);
                } else {
                    notes[idx1].column = col_offset + t2;
                    grid[t2] = idx1;
                    grid[t1] = usize::MAX;
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
    let (rows, mut map) = build_row_grid(notes, note_range, col_offset, cols);
    let mut rng = TurnRng::new(seed);
    let mut hold_end_row: [Option<usize>; MAX_COLS] = [None; MAX_COLS];

    for &row in &rows {
        let Some(grid) = map.remove(&row) else {
            continue;
        };
        for col in 0..cols {
            if let Some(end) = hold_end_row[col] {
                if row > end {
                    hold_end_row[col] = None;
                }
            }
        }

        let mut free_cols = [0usize; MAX_COLS];
        let mut free_len = 0usize;
        for col in 0..cols {
            if hold_end_row[col].is_none() {
                free_cols[free_len] = col;
                free_len += 1;
            }
        }
        if free_len == 0 {
            continue;
        }

        let mut row_notes = [usize::MAX; MAX_COLS];
        let mut notes_len = 0usize;
        for col in 0..cols {
            if hold_end_row[col].is_some() {
                continue;
            }
            let idx = grid[col];
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
        for i in 0..place_len {
            let idx = row_notes[i];
            let col = free_cols[i];
            notes[idx].column = col_offset + col;
        }

        for i in 0..place_len {
            let idx = row_notes[i];
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

    let mut row_to_indices: HashMap<usize, Vec<usize>> = HashMap::new();
    row_to_indices.reserve(notes.len());
    for (idx, note) in notes.iter().enumerate() {
        row_to_indices.entry(note.row_index).or_default().push(idx);
    }

    let mut rows: Vec<usize> = row_to_indices.keys().copied().collect();
    rows.sort_unstable();

    let mut remove_idx = vec![false; notes.len()];
    let mut active_hold_ends: [Option<usize>; MAX_COLS] = [None; MAX_COLS];
    let mut row_candidates = Vec::<(usize, usize)>::with_capacity(MAX_COLS);

    for &row in &rows {
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
        if let Some(indices) = row_to_indices.get(&row) {
            for &idx in indices {
                let note = &notes[idx];
                if note.column < col_offset {
                    continue;
                }
                let local_col = note.column - col_offset;
                if local_col >= cols || !note_counts_for_simultaneous_limit(note) {
                    continue;
                }
                row_candidates.push((local_col, idx));
            }
        }

        if row_candidates.is_empty() {
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

fn apply_uncommon_chart_transforms(
    notes: &mut Vec<Note>,
    note_ranges: &mut [(usize, usize); MAX_PLAYERS],
    cols_per_player: usize,
    num_players: usize,
    player_profiles: &[profile::Profile; MAX_PLAYERS],
    timing_players: &[Arc<TimingData>; MAX_PLAYERS],
) {
    if num_players == 0 {
        return;
    }

    let mut transformed = Vec::with_capacity(notes.len());
    let mut transformed_ranges = [(0usize, 0usize); MAX_PLAYERS];

    for player in 0..num_players {
        let (start, end) = note_ranges[player];
        let slice_end = end.min(notes.len());
        let slice_start = start.min(slice_end);
        let mut player_notes = notes[slice_start..slice_end].to_vec();
        apply_uncommon_masks_for_player(
            &mut player_notes,
            &player_profiles[player],
            timing_players[player].as_ref(),
            player.saturating_mul(cols_per_player),
            cols_per_player,
            player,
        );

        let out_start = transformed.len();
        transformed.extend(player_notes);
        transformed_ranges[player] = (out_start, transformed.len());
    }

    if num_players == 1 {
        transformed_ranges[1] = transformed_ranges[0];
    }

    *notes = transformed;
    *note_ranges = transformed_ranges;
}

#[derive(Clone, Debug)]
struct ChartAttackWindow {
    start_second: f32,
    len_seconds: f32,
    mods: String,
}

#[derive(Clone, Copy, Debug)]
struct AttackMaskWindow {
    start_second: f32,
    end_second: f32,
    clear_all: bool,
    chart: ChartAttackEffects,
    accel: AccelOverrides,
    visual: VisualOverrides,
    appearance: AppearanceOverrides,
    visibility: VisibilityOverrides,
    scroll: ScrollOverrides,
    perspective: PerspectiveOverrides,
    scroll_speed: Option<ScrollSpeedSetting>,
    mini_percent: Option<f32>,
}

#[derive(Clone, Copy, Debug)]
struct ParsedAttackMods {
    insert_mask: u8,
    remove_mask: u8,
    holds_mask: u8,
    turn_option: profile::TurnOption,
    clear_all: bool,
    accel: AccelOverrides,
    visual: VisualOverrides,
    appearance: AppearanceOverrides,
    visibility: VisibilityOverrides,
    scroll: ScrollOverrides,
    perspective: PerspectiveOverrides,
    scroll_speed: Option<ScrollSpeedSetting>,
    mini_percent: Option<f32>,
}

impl Default for ParsedAttackMods {
    fn default() -> Self {
        Self {
            insert_mask: 0,
            remove_mask: 0,
            holds_mask: 0,
            turn_option: profile::TurnOption::None,
            clear_all: false,
            accel: AccelOverrides::default(),
            visual: VisualOverrides::default(),
            appearance: AppearanceOverrides::default(),
            visibility: VisibilityOverrides::default(),
            scroll: ScrollOverrides::default(),
            perspective: PerspectiveOverrides::default(),
            scroll_speed: None,
            mini_percent: None,
        }
    }
}

impl ParsedAttackMods {
    #[inline(always)]
    fn has_chart_effect(self) -> bool {
        self.insert_mask != 0
            || self.remove_mask != 0
            || self.holds_mask != 0
            || self.turn_option != profile::TurnOption::None
    }

    #[inline(always)]
    fn has_runtime_mask_effect(self) -> bool {
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

#[inline(always)]
const fn turn_option_bits(turn: profile::TurnOption) -> u16 {
    match turn {
        profile::TurnOption::None => 0,
        profile::TurnOption::Mirror => 1 << 0,
        profile::TurnOption::Left => 1 << 1,
        profile::TurnOption::Right => 1 << 2,
        profile::TurnOption::LRMirror => 1 << 3,
        profile::TurnOption::UDMirror => 1 << 4,
        profile::TurnOption::Shuffle => 1 << 5,
        profile::TurnOption::Blender => 1 << 6,
        profile::TurnOption::Random => 1 << 7,
    }
}

fn parse_chart_attack_windows(raw: &str) -> Vec<ChartAttackWindow> {
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

fn attack_token_key(token: &str) -> String {
    let mut key = String::with_capacity(token.len());
    for ch in token.chars() {
        if ch.is_ascii_alphanumeric() {
            key.push(ch.to_ascii_lowercase());
        }
    }
    while key.as_bytes().first().is_some_and(|c| c.is_ascii_digit()) {
        key.remove(0);
    }
    key
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

fn parse_attack_mods(mods: &str) -> ParsedAttackMods {
    let mut out = ParsedAttackMods::default();
    for token in mods.split(',') {
        let token = token.trim();
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
            "mirror" => out.turn_option = profile::TurnOption::Mirror,
            "left" => out.turn_option = profile::TurnOption::Left,
            "right" => out.turn_option = profile::TurnOption::Right,
            "lrmirror" => out.turn_option = profile::TurnOption::LRMirror,
            "udmirror" => out.turn_option = profile::TurnOption::UDMirror,
            "shuffle" => out.turn_option = profile::TurnOption::Shuffle,
            "supershuffle" | "blender" => out.turn_option = profile::TurnOption::Blender,
            "hypershuffle" => out.turn_option = profile::TurnOption::Random,
            "reverse" => out.scroll.reverse = attack_level(percent_value),
            "split" => out.scroll.split = attack_level(percent_value),
            "alternate" => out.scroll.alternate = attack_level(percent_value),
            "cross" => out.scroll.cross = attack_level(percent_value),
            "centered" => out.scroll.centered = attack_level(percent_value),
            "boost" => out.accel.boost = attack_level(percent_value),
            "brake" => out.accel.brake = attack_level(percent_value),
            "wave" => out.accel.wave = attack_level(percent_value),
            "expand" => out.accel.expand = attack_level(percent_value),
            "boomerang" => out.accel.boomerang = attack_level(percent_value),
            "drunk" => out.visual.drunk = attack_level(percent_value),
            "dizzy" => out.visual.dizzy = attack_level(percent_value),
            "confusion" => out.visual.confusion = attack_level(percent_value),
            "flip" => out.visual.flip = attack_level(percent_value),
            "invert" => out.visual.invert = attack_level(percent_value),
            "tornado" => out.visual.tornado = attack_level(percent_value),
            "tipsy" => out.visual.tipsy = attack_level(percent_value),
            "bumpy" => out.visual.bumpy = attack_level(percent_value),
            "beat" => out.visual.beat = attack_level(percent_value),
            "mini" => {
                let mini = percent_value.unwrap_or(100.0);
                if mini.is_finite() {
                    out.mini_percent = Some(mini);
                }
            }
            "hidden" => out.appearance.hidden = attack_level(percent_value),
            "sudden" => out.appearance.sudden = attack_level(percent_value),
            "stealth" => out.appearance.stealth = attack_level(percent_value),
            "blink" => out.appearance.blink = attack_level(percent_value),
            "rvanish" | "randomvanish" | "reversevanish" => {
                out.appearance.random_vanish = attack_level(percent_value)
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
    out
}

#[inline(always)]
fn random_attack_seed(base_seed: u64, player: usize, attacks_len: usize) -> u64 {
    base_seed
        ^ (0xC2B2_AE3D_27D4_EB4F_u64.wrapping_mul(player as u64 + 1))
        ^ (attacks_len as u64).wrapping_mul(0x9E37_79B9_u64)
}

fn build_random_attack_windows(
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

fn build_attack_windows_for_player(
    chart_attacks: Option<&str>,
    attack_mode: profile::AttackMode,
    player: usize,
    base_seed: u64,
    song_length_seconds: f32,
) -> Vec<ChartAttackWindow> {
    match attack_mode {
        profile::AttackMode::Off => Vec::new(),
        profile::AttackMode::On => chart_attacks
            .map(parse_chart_attack_windows)
            .unwrap_or_default(),
        profile::AttackMode::Random => {
            build_random_attack_windows(song_length_seconds, player, base_seed)
        }
    }
}

fn select_attack_mods(
    attacks: &[ChartAttackWindow],
    _attack_mode: profile::AttackMode,
    _player: usize,
    _base_seed: u64,
) -> Vec<ParsedAttackMods> {
    if attacks.is_empty() {
        return Vec::new();
    }
    attacks
        .iter()
        .map(|attack| parse_attack_mods(&attack.mods))
        .collect()
}

fn build_attack_mask_windows_for_player(
    chart_attacks: Option<&str>,
    attack_mode: profile::AttackMode,
    player: usize,
    base_seed: u64,
    song_length_seconds: f32,
) -> Vec<AttackMaskWindow> {
    let attacks = build_attack_windows_for_player(
        chart_attacks,
        attack_mode,
        player,
        base_seed,
        song_length_seconds,
    );
    if attacks.is_empty() {
        return Vec::new();
    }
    let selected_mods = select_attack_mods(&attacks, attack_mode, player, base_seed);
    if selected_mods.is_empty() {
        return Vec::new();
    }
    let mut windows = Vec::with_capacity(attacks.len());
    for (attack, mods) in attacks.iter().zip(selected_mods.iter().copied()) {
        if !mods.has_runtime_mask_effect() && !mods.has_chart_effect() {
            continue;
        }
        let start_second = attack.start_second;
        let end_second = start_second + attack.len_seconds.max(0.0);
        if !start_second.is_finite() || !end_second.is_finite() || end_second <= start_second {
            continue;
        }
        windows.push(AttackMaskWindow {
            start_second,
            end_second,
            clear_all: mods.clear_all,
            chart: ChartAttackEffects {
                insert_mask: mods.insert_mask,
                remove_mask: mods.remove_mask,
                holds_mask: mods.holds_mask,
                turn_bits: turn_option_bits(mods.turn_option),
            },
            accel: mods.accel,
            visual: mods.visual,
            appearance: mods.appearance,
            visibility: mods.visibility,
            scroll: mods.scroll,
            perspective: mods.perspective,
            scroll_speed: mods.scroll_speed,
            mini_percent: mods.mini_percent,
        });
    }
    windows
}

#[inline(always)]
fn beat_to_note_row_index(beat: f32) -> usize {
    let rows_per_beat = ROWS_PER_BEAT.max(1) as f32;
    (beat.max(0.0) * rows_per_beat).round() as usize
}

fn apply_attack_turn_mod(
    notes: &mut [Note],
    col_offset: usize,
    cols: usize,
    turn_option: profile::TurnOption,
    seed: u64,
    player: usize,
) {
    if notes.is_empty() || turn_option == profile::TurnOption::None {
        return;
    }
    let note_range = (0usize, notes.len());
    match turn_option {
        profile::TurnOption::None => {}
        profile::TurnOption::Blender => {
            apply_turn_permutation(
                notes,
                note_range,
                col_offset,
                cols,
                profile::TurnOption::Shuffle,
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
        profile::TurnOption::Random => {
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

fn apply_chart_attack_window(
    notes: &mut Vec<Note>,
    timing_player: &TimingData,
    col_offset: usize,
    cols: usize,
    player: usize,
    start_row: usize,
    end_row: usize,
    mods: ParsedAttackMods,
    turn_seed: u64,
) {
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
        Some((start_row, end_row)),
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

fn apply_chart_attacks_for_player(
    notes: &mut Vec<Note>,
    chart_attacks: Option<&str>,
    attack_mode: profile::AttackMode,
    timing_player: &TimingData,
    col_offset: usize,
    cols: usize,
    player: usize,
    base_seed: u64,
    song_length_seconds: f32,
) {
    let attacks = build_attack_windows_for_player(
        chart_attacks,
        attack_mode,
        player,
        base_seed,
        song_length_seconds,
    );
    if attacks.is_empty() {
        return;
    }
    let selected_mods = select_attack_mods(&attacks, attack_mode, player, base_seed);
    if selected_mods.is_empty() {
        if attack_mode == profile::AttackMode::Random {
            debug!(
                "Player {} selected RandomAttacks, but no random attack windows were generated.",
                player + 1,
            );
        }
        return;
    }
    for (i, (attack, mods)) in attacks
        .iter()
        .zip(selected_mods.iter().copied())
        .enumerate()
    {
        if !mods.has_chart_effect() {
            continue;
        }
        let start_beat = timing_player.get_beat_for_time(attack.start_second);
        let end_beat = timing_player.get_beat_for_time(attack.start_second + attack.len_seconds);
        let start_row = beat_to_note_row_index(start_beat);
        let end_row = beat_to_note_row_index(end_beat);
        if end_row < start_row {
            continue;
        }
        let turn_seed = base_seed
            ^ (0x9E37_79B9_u64.wrapping_mul(player as u64 + 1))
            ^ ((i as u64).wrapping_mul(0xA5A5_5A5A_u64));
        apply_chart_attack_window(
            notes,
            timing_player,
            col_offset,
            cols,
            player,
            start_row,
            end_row,
            mods,
            turn_seed,
        );
    }
}

fn apply_chart_attacks_transforms(
    notes: &mut Vec<Note>,
    note_ranges: &mut [(usize, usize); MAX_PLAYERS],
    gameplay_charts: &[Arc<GameplayChartData>; MAX_PLAYERS],
    cols_per_player: usize,
    num_players: usize,
    player_profiles: &[profile::Profile; MAX_PLAYERS],
    timing_players: &[Arc<TimingData>; MAX_PLAYERS],
    base_seed: u64,
    song_length_seconds: f32,
) {
    if num_players == 0 {
        return;
    }
    let mut transformed = Vec::with_capacity(notes.len());
    let mut transformed_ranges = [(0usize, 0usize); MAX_PLAYERS];

    for player in 0..num_players {
        let (start, end) = note_ranges[player];
        let slice_end = end.min(notes.len());
        let slice_start = start.min(slice_end);
        let mut player_notes = notes[slice_start..slice_end].to_vec();
        apply_chart_attacks_for_player(
            &mut player_notes,
            gameplay_charts[player].chart_attacks.as_deref(),
            player_profiles[player].attack_mode,
            timing_players[player].as_ref(),
            player.saturating_mul(cols_per_player),
            cols_per_player,
            player,
            base_seed,
            song_length_seconds,
        );
        let out_start = transformed.len();
        transformed.extend(player_notes);
        transformed_ranges[player] = (out_start, transformed.len());
    }

    if num_players == 1 {
        transformed_ranges[1] = transformed_ranges[0];
    }

    *notes = transformed;
    *note_ranges = transformed_ranges;
}

#[inline(always)]
fn count_total_steps_for_range(notes: &[Note], note_range: (usize, usize)) -> u32 {
    let (start, end) = note_range;
    if start >= end {
        return 0;
    }
    let mut rows = Vec::<usize>::with_capacity(end - start);
    for note in &notes[start..end] {
        if note.can_be_judged && !matches!(note.note_type, NoteType::Mine) {
            rows.push(note.row_index);
        }
    }
    rows.sort_unstable();
    rows.dedup();
    rows.len() as u32
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct PlayerTotals {
    steps: u32,
    holds: u32,
    rolls: u32,
    mines: u32,
    jumps: u32,
    hands: u32,
}

#[inline(always)]
fn recompute_player_totals(notes: &[Note], note_range: (usize, usize)) -> PlayerTotals {
    let (start, end) = note_range;
    if start >= end {
        return PlayerTotals::default();
    }
    let mut totals = PlayerTotals {
        steps: count_total_steps_for_range(notes, note_range),
        ..PlayerTotals::default()
    };
    let mut row_cells: Vec<(usize, usize)> = Vec::with_capacity(end - start);
    let mut hold_starts: Vec<usize> = Vec::new();
    let mut hold_ends: Vec<usize> = Vec::new();
    for note in &notes[start..end] {
        if !note.can_be_judged {
            continue;
        }
        match note.note_type {
            NoteType::Tap => row_cells.push((note.row_index, note.column)),
            NoteType::Hold => {
                totals.holds = totals.holds.saturating_add(1);
                row_cells.push((note.row_index, note.column));
                if let Some(hold) = note.hold.as_ref() {
                    hold_starts.push(note.row_index);
                    hold_ends.push(hold.end_row_index);
                }
            }
            NoteType::Roll => {
                totals.rolls = totals.rolls.saturating_add(1);
                row_cells.push((note.row_index, note.column));
                if let Some(hold) = note.hold.as_ref() {
                    hold_starts.push(note.row_index);
                    hold_ends.push(hold.end_row_index);
                }
            }
            NoteType::Mine => totals.mines = totals.mines.saturating_add(1),
            NoteType::Lift | NoteType::Fake => {}
        }
    }

    row_cells.sort_unstable();
    hold_starts.sort_unstable();
    hold_ends.sort_unstable();

    let mut row_ix = 0usize;
    let mut hold_start_ix = 0usize;
    let mut hold_end_ix = 0usize;
    while row_ix < row_cells.len() {
        let row = row_cells[row_ix].0;
        let mut row_mask = 0u16;
        while row_ix < row_cells.len() && row_cells[row_ix].0 == row {
            row_mask |= 1u16 << row_cells[row_ix].1.min(15);
            row_ix += 1;
        }
        while hold_start_ix < hold_starts.len() && hold_starts[hold_start_ix] < row {
            hold_start_ix += 1;
        }
        while hold_end_ix < hold_ends.len() && hold_ends[hold_end_ix] < row {
            hold_end_ix += 1;
        }
        let notes_on_row = row_mask.count_ones();
        let carried_holds = hold_start_ix.saturating_sub(hold_end_ix) as u32;
        if notes_on_row >= 2 {
            totals.jumps = totals.jumps.saturating_add(1);
        }
        if notes_on_row + carried_holds >= 3 {
            totals.hands = totals.hands.saturating_add(1);
        }
    }

    totals
}

#[inline(always)]
fn chart_has_attacks(chart: &ChartData) -> bool {
    chart.has_chart_attacks
}

fn chart_has_significant_timing_changes(chart: &ChartData) -> bool {
    chart.has_significant_timing_changes
}

fn score_valid_for_chart(
    chart: &ChartData,
    profile: &profile::Profile,
    scroll_speed: ScrollSpeedSetting,
    music_rate: f32,
) -> bool {
    let rate = if music_rate.is_finite() && music_rate > 0.0 {
        music_rate
    } else {
        1.0
    };
    if rate < 1.0 {
        return false;
    }

    if matches!(scroll_speed, ScrollSpeedSetting::CMod(_))
        && chart_has_significant_timing_changes(chart)
    {
        return false;
    }

    let remove_mask = profile::normalize_remove_mask(profile.remove_active_mask);
    if (remove_mask & REMOVE_MASK_BIT_NO_HOLDS) != 0 && chart.stats.holds > 0 {
        return false;
    }
    if (remove_mask & REMOVE_MASK_BIT_NO_MINES) != 0 && chart.mines_nonfake > 0 {
        return false;
    }
    if (remove_mask & REMOVE_MASK_BIT_NO_JUMPS) != 0 && chart.stats.jumps > 0 {
        return false;
    }
    if (remove_mask & REMOVE_MASK_BIT_NO_HANDS) != 0 && chart.stats.hands > 0 {
        return false;
    }
    if (remove_mask & REMOVE_MASK_BIT_NO_QUADS) != 0 && chart.stats.hands > 0 {
        return false;
    }
    if (remove_mask & REMOVE_MASK_BIT_NO_LIFTS) != 0 && chart.stats.lifts > 0 {
        return false;
    }
    if (remove_mask & REMOVE_MASK_BIT_NO_FAKES) != 0 && chart.stats.fakes > 0 {
        return false;
    }

    let holds_mask = profile::normalize_holds_mask(profile.holds_active_mask);
    if (holds_mask & HOLDS_MASK_BIT_NO_ROLLS) != 0 && chart.stats.rolls > 0 {
        return false;
    }

    if (remove_mask & REMOVE_MASK_BIT_LITTLE) != 0 {
        return false;
    }

    let insert_mask = profile::normalize_insert_mask(profile.insert_active_mask);
    if (insert_mask & INSERT_MASK_BIT_ECHO) != 0 {
        return false;
    }

    if (holds_mask & (HOLDS_MASK_BIT_PLANTED | HOLDS_MASK_BIT_FLOORED | HOLDS_MASK_BIT_TWISTER))
        != 0
    {
        return false;
    }

    match profile.attack_mode {
        profile::AttackMode::Off => !chart_has_attacks(chart),
        profile::AttackMode::On => true,
        profile::AttackMode::Random => false,
    }
}

fn compute_end_times(
    notes: &[Note],
    note_time_cache: &[f32],
    hold_end_time_cache: &[Option<f32>],
    rate: f32,
) -> (f32, f32) {
    let mut last_judgable_second = 0.0_f32;
    let mut last_relevant_second = 0.0_f32;
    for (i, note) in notes.iter().enumerate() {
        let start = note_time_cache[i];
        let end = hold_end_time_cache[i].unwrap_or(start);
        last_relevant_second = last_relevant_second.max(end);
        if note.can_be_judged {
            last_judgable_second = last_judgable_second.max(end);
        }
    }

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
    let notes_end_time = last_judgable_second + max_step_distance;
    let music_end_time = last_relevant_second + max_step_distance;
    (notes_end_time, music_end_time)
}

#[inline(always)]
fn compute_possible_grade_points(
    notes: &[Note],
    note_range: (usize, usize),
    holds_total: u32,
    rolls_total: u32,
) -> i32 {
    let (start, end) = note_range;
    if start >= end {
        return 0;
    }

    let mut rows: Vec<usize> = Vec::with_capacity(end - start);
    for n in &notes[start..end] {
        if n.can_be_judged && !matches!(n.note_type, NoteType::Mine) {
            rows.push(n.row_index);
        }
    }
    rows.sort_unstable();
    rows.dedup();

    let num_tap_rows = rows.len() as u64;
    let pts = (num_tap_rows * 5)
        + (u64::from(holds_total) * judgment::HOLD_SCORE_HELD as u64)
        + (u64::from(rolls_total) * judgment::HOLD_SCORE_HELD as u64);
    pts as i32
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

#[derive(Clone, Debug)]
pub struct RowEntry {
    row_index: usize,
    // Non-mine, non-fake, judgable notes on this row
    nonmine_note_indices: Vec<usize>,
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
pub struct Arrow {
    #[allow(dead_code)]
    pub beat: f32,
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
const fn column_cue_is_mine(note_type: NoteType) -> Option<bool> {
    match note_type {
        NoteType::Tap | NoteType::Lift | NoteType::Hold | NoteType::Roll => Some(false),
        NoteType::Mine => Some(true),
        NoteType::Fake => None,
    }
}

fn build_column_cues_for_player(
    notes: &[Note],
    note_range: (usize, usize),
    note_time_cache: &[f32],
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
                && let Some(is_mine) = column_cue_is_mine(note.note_type)
            {
                if !has_row_time {
                    row_time = note_time_cache[i];
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

#[must_use]
fn stream_sequences_threshold(measures: &[usize], threshold: usize) -> Vec<StreamSegment> {
    let streams: Vec<_> = measures
        .iter()
        .enumerate()
        .filter(|(_, n)| **n >= threshold)
        .map(|(i, _)| i + 1)
        .collect();

    if streams.is_empty() {
        return Vec::new();
    }

    let mut segs = Vec::new();
    let first_break = streams[0].saturating_sub(1);
    if first_break >= 2 {
        segs.push(StreamSegment {
            start: 0,
            end: first_break,
            is_break: true,
        });
    }

    let (mut count, mut end) = (1usize, None);
    for (i, &cur) in streams.iter().enumerate() {
        let next = streams.get(i + 1).copied().unwrap_or(usize::MAX);
        if cur + 1 == next {
            count += 1;
            end = Some(cur + 1);
            continue;
        }

        let e = end.unwrap_or(cur);
        segs.push(StreamSegment {
            start: e - count,
            end: e,
            is_break: false,
        });

        let bstart = cur;
        let bend = if next == usize::MAX {
            measures.len()
        } else {
            next - 1
        };
        if bend >= bstart + 2 {
            segs.push(StreamSegment {
                start: bstart,
                end: bend,
                is_break: true,
            });
        }
        count = 1;
        end = None;
    }
    segs
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

#[inline(always)]
fn target_score_setting_percent(setting: profile::TargetScoreSetting) -> Option<f64> {
    use profile::TargetScoreSetting;
    match setting {
        TargetScoreSetting::CMinus => Some(50.0),
        TargetScoreSetting::C => Some(55.0),
        TargetScoreSetting::CPlus => Some(60.0),
        TargetScoreSetting::BMinus => Some(64.0),
        TargetScoreSetting::B => Some(68.0),
        TargetScoreSetting::BPlus => Some(72.0),
        TargetScoreSetting::AMinus => Some(76.0),
        TargetScoreSetting::A => Some(80.0),
        TargetScoreSetting::APlus => Some(83.0),
        TargetScoreSetting::SMinus => Some(86.0),
        TargetScoreSetting::S => Some(89.0),
        TargetScoreSetting::SPlus => Some(92.0),
        TargetScoreSetting::MachineBest | TargetScoreSetting::PersonalBest => None,
    }
}

#[inline(always)]
fn zmod_stream_density(measures: &[usize], threshold: usize, multiplier: f32) -> f32 {
    let segs = stream_sequences_threshold(measures, threshold);
    if segs.is_empty() {
        return 0.0;
    }
    let mut total_stream = 0.0_f32;
    let mut total_measures = 0.0_f32;
    for seg in &segs {
        let seg_len = ((seg.end.saturating_sub(seg.start)) as f32 * multiplier).floor();
        if seg_len <= 0.0 {
            continue;
        }
        if !seg.is_break {
            total_stream += seg_len;
        }
        total_measures += seg_len;
    }
    if total_measures <= 0.0 {
        0.0
    } else {
        total_stream / total_measures
    }
}

#[inline(always)]
fn zmod_stream_totals_full_measures(
    measures: &[usize],
    constant_bpm: bool,
) -> (Vec<StreamSegment>, f32, f32) {
    // Mirrors SL-ChartParserHelpers.lua::GetTotalStreamAndBreakMeasures(pn, true).
    let addition = 2usize;

    let mut threshold = 14 + addition;
    let mut multiplier = 1.0_f32;
    if constant_bpm {
        threshold = 30 + addition;
        multiplier = 2.0;

        let d32 = zmod_stream_density(measures, threshold, multiplier);
        if d32 < 0.2 {
            threshold = 22 + addition;
            multiplier = 1.5;
            let d24 = zmod_stream_density(measures, threshold, multiplier);
            if d24 < 0.2 {
                threshold = 18 + addition;
                multiplier = 1.25;
                let d20 = zmod_stream_density(measures, threshold, multiplier);
                if d20 < 0.2 {
                    threshold = 14 + addition;
                    multiplier = 1.0;
                }
            }
        }
    }

    let segs = stream_sequences_threshold(measures, threshold);
    if segs.is_empty() {
        return (segs, 0.0, 0.0);
    }

    let mut total_stream = 0.0_f32;
    let mut total_break = 0.0_f32;
    let mut edge_break = 0.0_f32;
    let mut last_stream = false;
    let len = segs.len();
    for (i, seg) in segs.iter().enumerate() {
        let seg_len = seg.end.saturating_sub(seg.start) as f32;
        if seg_len <= 0.0 {
            continue;
        }
        if seg.is_break && i > 0 && i + 1 < len {
            total_break += seg_len;
            last_stream = false;
        } else if seg.is_break {
            edge_break += seg_len;
            last_stream = false;
        } else {
            if last_stream {
                total_break += 1.0;
            }
            total_stream += seg_len;
            last_stream = true;
        }
    }

    if total_stream + total_break < 10.0 || total_stream + total_break < edge_break {
        total_break += edge_break;
    }

    (segs, total_stream * multiplier, total_break * multiplier)
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

#[derive(Clone, Copy, Debug)]
enum DangerAnim {
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

impl Default for DangerAnim {
    fn default() -> Self {
        Self::Hidden
    }
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

#[derive(Clone, Debug)]
pub struct PlayerRuntime {
    pub combo: u32,
    pub miss_combo: u32,
    pub full_combo_grade: Option<JudgeGrade>,
    pub current_combo_grade: Option<JudgeGrade>,
    pub first_fc_attempt_broken: bool,
    pub judgment_counts: judgment::JudgeCounts,
    pub scoring_counts: judgment::JudgeCounts,
    pub last_judgment: Option<JudgmentRenderInfo>,

    pub life: f32,
    pub combo_after_miss: u32,
    pub is_failing: bool,
    pub fail_time: Option<f32>,

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

fn init_player_runtime() -> PlayerRuntime {
    PlayerRuntime {
        combo: 0,
        miss_combo: 0,
        full_combo_grade: None,
        current_combo_grade: None,
        first_fc_attempt_broken: false,
        judgment_counts: [0; judgment::JUDGE_GRADE_COUNT],
        scoring_counts: [0; judgment::JUDGE_GRADE_COUNT],
        last_judgment: None,
        life: 0.5,
        combo_after_miss: 0,
        is_failing: false,
        fail_time: None,
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
    pub event_music_time: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct ReplayInputEdge {
    pub lane_index: u8,
    pub pressed: bool,
    pub source: InputSource,
    pub event_music_time: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct ReplayOffsetSnapshot {
    pub beat0_time_seconds: f32,
}

#[derive(Clone, Copy, Debug)]
struct SongClockSnapshot {
    song_time: f32,
    seconds_per_second: f32,
    valid_at: Instant,
    valid_at_host_nanos: u64,
}

#[derive(Clone, Copy, Debug)]
struct FrameStableDisplayClock {
    current_time_sec: f32,
    target_time_sec: f32,
    catching_up: bool,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct DisplayClockHealth {
    pub error_seconds: f32,
    pub catching_up: bool,
}

impl FrameStableDisplayClock {
    #[inline(always)]
    const fn new(time_sec: f32) -> Self {
        Self {
            current_time_sec: time_sec,
            target_time_sec: time_sec,
            catching_up: false,
        }
    }

    #[inline(always)]
    fn reset(&mut self, time_sec: f32) -> f32 {
        self.current_time_sec = time_sec;
        self.target_time_sec = time_sec;
        self.catching_up = false;
        time_sec
    }
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
    passive_miss_us: u32,
    tap_miss_us: u32,
    cull_us: u32,
    judged_rows_us: u32,
    density_us: u32,
    density_sample_us: u32,
    density_hist_mesh_us: u32,
    density_life_mesh_us: u32,
    density_clip_us: u32,
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
    summary_peak_active_arrows: usize,
    summary_peak_pending_edges: usize,
    arrow_capacity: [usize; MAX_COLS],
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
            summary_peak_active_arrows: 0,
            summary_peak_pending_edges: 0,
            arrow_capacity: [0; MAX_COLS],
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
    pub notes: Vec<Note>,
    pub note_ranges: [(usize, usize); MAX_PLAYERS],
    pub audio_lead_in_seconds: f32,
    pub current_beat: f32,
    pub current_music_time: f32,
    pub current_beat_display: f32,
    pub current_music_time_display: f32,
    display_clock: FrameStableDisplayClock,
    pub note_spawn_cursor: [usize; MAX_PLAYERS],
    pub judged_row_cursor: [usize; MAX_PLAYERS],
    pub arrows: [Vec<Arrow>; MAX_COLS],
    pub note_time_cache: Vec<f32>,
    pub note_display_beat_cache: Vec<f32>,
    pub hold_end_time_cache: Vec<Option<f32>>,
    pub hold_end_display_beat_cache: Vec<Option<f32>>,
    pub notes_end_time: f32,
    pub music_end_time: f32,
    pub music_rate: f32,
    pub play_mine_sounds: bool,
    pub global_offset_seconds: f32,
    pub initial_global_offset_seconds: f32,
    pub song_offset_seconds: f32,
    pub initial_song_offset_seconds: f32,
    pub autosync_mode: AutosyncMode,
    pub autosync_offset_samples: [f32; AUTOSYNC_OFFSET_SAMPLE_COUNT],
    pub autosync_offset_sample_count: usize,
    pub autosync_standard_deviation: f32,
    pub global_visual_delay_seconds: f32,
    pub player_visual_delay_seconds: [f32; MAX_PLAYERS],
    pub current_music_time_visible: [f32; MAX_PLAYERS],
    pub current_beat_visible: [f32; MAX_PLAYERS],
    pub next_tap_miss_cursor: [usize; MAX_PLAYERS],
    pub next_mine_avoid_cursor: [usize; MAX_PLAYERS],
    pub mine_note_ix: [Vec<usize>; MAX_PLAYERS],
    pub mine_note_time: [Vec<f32>; MAX_PLAYERS],
    pub next_mine_ix_cursor: [usize; MAX_PLAYERS],
    pub row_entries: Vec<RowEntry>,
    pub measure_counter_segments: [Vec<StreamSegment>; MAX_PLAYERS],
    pub column_cues: [Vec<ColumnCue>; MAX_PLAYERS],
    pub mini_indicator_stream_segments: [Vec<StreamSegment>; MAX_PLAYERS],
    pub mini_indicator_total_stream_measures: [f32; MAX_PLAYERS],
    pub mini_indicator_target_score_percent: [f64; MAX_PLAYERS],
    pub mini_indicator_rival_score_percent: [f64; MAX_PLAYERS],

    // Optimization: Direct array lookup instead of HashMap
    pub row_map_cache: Vec<u32>,
    // Bit flags per note index:
    // bit0 => same row contains a hold start, bit1 => same row contains a roll start.
    pub tap_row_hold_roll_flags: Vec<u8>,

    pub decaying_hold_indices: Vec<usize>,
    pub hold_decay_active: Vec<bool>,

    pub players: [PlayerRuntime; MAX_PLAYERS],
    pub hold_judgments: [Option<HoldJudgmentRenderInfo>; MAX_COLS],
    pub is_in_freeze: bool,
    pub is_in_delay: bool,

    pub possible_grade_points: [i32; MAX_PLAYERS],
    pub song_completed_naturally: bool,
    pub autoplay_enabled: bool,
    pub autoplay_used: bool,
    pub score_valid: [bool; MAX_PLAYERS],
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
    active_attack_clear_all: [bool; MAX_PLAYERS],
    active_attack_chart: [ChartAttackEffects; MAX_PLAYERS],
    active_attack_accel: [AccelOverrides; MAX_PLAYERS],
    active_attack_visual: [VisualOverrides; MAX_PLAYERS],
    active_attack_appearance: [AppearanceOverrides; MAX_PLAYERS],
    active_attack_visibility: [VisibilityOverrides; MAX_PLAYERS],
    active_attack_scroll: [ScrollOverrides; MAX_PLAYERS],
    active_attack_perspective: [PerspectiveOverrides; MAX_PLAYERS],
    active_attack_scroll_speed: [Option<ScrollSpeedSetting>; MAX_PLAYERS],
    active_attack_mini_percent: [Option<f32>; MAX_PLAYERS],
    pub noteskin: [Option<Arc<Noteskin>>; MAX_PLAYERS],
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
    pub density_graph_cache: [Option<DensityHistCache>; MAX_PLAYERS],
    pub density_graph_mesh: [Option<Arc<[MeshVertex]>>; MAX_PLAYERS],
    pub density_graph_mesh_offset_px: [i32; MAX_PLAYERS],
    pub density_graph_life_update_rate: f32,
    pub density_graph_life_next_update_elapsed: f32,
    pub density_graph_life_points: [Vec<[f32; 2]>; MAX_PLAYERS],
    pub density_graph_life_mesh: [Option<Arc<[MeshVertex]>>; MAX_PLAYERS],
    pub density_graph_life_mesh_offset_px: [i32; MAX_PLAYERS],
    pub density_graph_life_dirty: [bool; MAX_PLAYERS],
    pub density_graph_top_h: f32,
    pub density_graph_top_w: [f32; MAX_PLAYERS],
    pub density_graph_top_scale_y: [f32; MAX_PLAYERS],
    pub density_graph_top_mesh: [Option<Arc<[MeshVertex]>>; MAX_PLAYERS],

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
    pending_edges: VecDeque<InputEdge>,
    autoplay_rng: TurnRng,
    autoplay_cursor: [usize; MAX_PLAYERS],
    autoplay_pending_row: [Option<(usize, f32)>; MAX_PLAYERS],
    autoplay_lane_state: [bool; MAX_COLS],
    autoplay_hold_release_time: [Option<f32>; MAX_COLS],
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
        for col in 0..state.num_cols.min(MAX_COLS) {
            trace.arrow_capacity[col] = state.arrows[col].capacity();
        }
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
    if phases.passive_miss_us > best.1 {
        best = ("passive_miss", phases.passive_miss_us);
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
    dst.passive_miss_us = dst.passive_miss_us.max(src.passive_miss_us);
    dst.tap_miss_us = dst.tap_miss_us.max(src.tap_miss_us);
    dst.cull_us = dst.cull_us.max(src.cull_us);
    dst.judged_rows_us = dst.judged_rows_us.max(src.judged_rows_us);
    dst.density_us = dst.density_us.max(src.density_us);
    dst.density_sample_us = dst.density_sample_us.max(src.density_sample_us);
    dst.density_hist_mesh_us = dst.density_hist_mesh_us.max(src.density_hist_mesh_us);
    dst.density_life_mesh_us = dst.density_life_mesh_us.max(src.density_life_mesh_us);
    dst.density_clip_us = dst.density_clip_us.max(src.density_clip_us);
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
        .saturating_add(phases.passive_miss_us)
        .saturating_add(phases.tap_miss_us)
        .saturating_add(phases.cull_us)
        .saturating_add(phases.judged_rows_us)
        .saturating_add(phases.density_us)
        .saturating_add(phases.danger_us)
}

fn trace_capacity_growth(state: &mut State) {
    let num_cols = state.num_cols.min(MAX_COLS);
    let num_players = state.num_players.min(MAX_PLAYERS);
    let frame = state.update_trace.frame_counter;
    for col in 0..num_cols {
        let new_cap = state.arrows[col].capacity();
        let old_cap = state.update_trace.arrow_capacity[col];
        if new_cap > old_cap {
            debug!(
                "Gameplay vec growth frame={frame}: arrows[{col}] capacity {old_cap} -> {new_cap} (len={})",
                state.arrows[col].len()
            );
            state.update_trace.arrow_capacity[col] = new_cap;
        }
    }
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
    let active_arrows: usize = state.arrows.iter().map(std::vec::Vec::len).sum();
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
        trace_state.summary_peak_active_arrows =
            trace_state.summary_peak_active_arrows.max(active_arrows);
        trace_state.summary_peak_pending_edges =
            trace_state.summary_peak_pending_edges.max(pending_len);
        trace_state.frame_counter
    };

    if pending_len >= GAMEPLAY_INPUT_BACKLOG_WARN {
        debug!(
            "Gameplay input backlog: frame={}, pending_edges={}, active_arrows={}, replay_edges={}",
            frame_counter, pending_len, active_arrows, replay_edges_len
        );
    }

    let (hot_name, hot_us) = max_phase_name_and_us(&phases);
    let is_slow =
        total_us >= GAMEPLAY_TRACE_SLOW_FRAME_US || hot_us >= GAMEPLAY_TRACE_PHASE_SPIKE_US;
    if is_slow {
        state.update_trace.summary_slow_frames =
            state.update_trace.summary_slow_frames.saturating_add(1);
        debug!(
            "Gameplay slow frame={} t={:.3}s total={:.3}ms hot={}({:.3}ms) pending={} arrows={} decays={} phases_ms=[pre:{:.3} auto:{:.3} input:{:.3} held:{:.3} holds:{:.3} decay:{:.3} vis:{:.3} spawn:{:.3} mine:{:.3} pmiss:{:.3} tmiss:{:.3} cull:{:.3} judged:{:.3} density:{:.3} danger:{:.3} other:{:.3}] input_sub_ms=[queue:{:.3} state:{:.3} glow:{:.3} judge:{:.3} roll:{:.3}] density_sub_ms=[sample:{:.3} hist_mesh:{:.3} life_mesh:{:.3} clip:{:.3}]",
            frame_counter,
            music_time_sec,
            total_us as f32 / 1000.0,
            hot_name,
            hot_us as f32 / 1000.0,
            pending_len,
            active_arrows,
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
            phases.passive_miss_us as f32 / 1000.0,
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
            phases.density_sample_us as f32 / 1000.0,
            phases.density_hist_mesh_us as f32 / 1000.0,
            phases.density_life_mesh_us as f32 / 1000.0,
            phases.density_clip_us as f32 / 1000.0
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
        let summary_peak_active_arrows = state.update_trace.summary_peak_active_arrows;
        let summary_peak_pending_edges = state.update_trace.summary_peak_pending_edges;
        let (summary_hot_name, summary_hot_us) = max_phase_name_and_us(&summary_max_phase);
        trace!(
            "Gameplay trace summary: frames={} slow={} max_total={:.3}ms max_hot={}({:.3}ms) peak_arrows={} peak_pending={} input_sub_max_ms=[queue:{:.3} state:{:.3} glow:{:.3} judge:{:.3} roll:{:.3}] input_latency_us=[samples:{} cap_store_avg:{:.1} cap_store_max:{} store_emit_avg:{:.1} store_emit_max:{} emit_queue_avg:{:.1} emit_queue_max:{} queue_proc_avg:{:.1} queue_proc_max:{} cap_proc_avg:{:.1} cap_proc_max:{}] density_sub_max_ms=[sample:{:.3} hist_mesh:{:.3} life_mesh:{:.3} clip:{:.3}] other_max={:.3}",
            summary_frames,
            summary_slow_frames,
            summary_max_total_us as f32 / 1000.0,
            summary_hot_name,
            summary_hot_us as f32 / 1000.0,
            summary_peak_active_arrows,
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
            summary_max_phase.density_hist_mesh_us as f32 / 1000.0,
            summary_max_phase.density_life_mesh_us as f32 / 1000.0,
            summary_max_phase.density_clip_us as f32 / 1000.0,
            summary_max_phase.untracked_us as f32 / 1000.0
        );
        state.update_trace.summary_elapsed_s = 0.0;
        state.update_trace.summary_frames = 0;
        state.update_trace.summary_slow_frames = 0;
        state.update_trace.summary_max_total_us = 0;
        state.update_trace.summary_max_phase = GameplayUpdatePhaseTimings::default();
        state.update_trace.summary_input_latency = GameplayInputLatencyTrace::default();
        state.update_trace.summary_peak_active_arrows = 0;
        state.update_trace.summary_peak_pending_edges = 0;
    }

    trace_capacity_growth(state);
}

#[cfg(debug_assertions)]
fn debug_validate_hot_state(state: &State, delta_time: f32, music_time_sec: f32) {
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
    debug_assert_eq!(state.notes.len(), state.note_time_cache.len());
    debug_assert_eq!(state.notes.len(), state.note_display_beat_cache.len());
    debug_assert_eq!(state.notes.len(), state.hold_end_time_cache.len());
    debug_assert_eq!(state.notes.len(), state.hold_end_display_beat_cache.len());
    debug_assert_eq!(state.notes.len(), state.hold_decay_active.len());
    for player in 0..state.num_players {
        let (start, end) = state.note_ranges[player];
        debug_assert!(start <= end && end <= state.notes.len());
        debug_assert!(
            state.note_spawn_cursor[player] >= start && state.note_spawn_cursor[player] <= end
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
            state.mine_note_time[player].len()
        );
        debug_assert!(state.next_mine_ix_cursor[player] <= state.mine_note_ix[player].len());
    }
    for col in 0..state.num_cols {
        debug_assert!(state.column_scroll_dirs[col].is_finite());
        for arrow in &state.arrows[col] {
            debug_assert!(arrow.note_index < state.notes.len());
            debug_assert_eq!(state.notes[arrow.note_index].column, col);
        }
    }
}

#[cfg(not(debug_assertions))]
#[inline(always)]
fn debug_validate_hot_state(_state: &State, _delta_time: f32, _music_time_sec: f32) {}

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

fn refresh_active_attack_masks(state: &mut State) {
    for player in 0..state.num_players {
        let now = state.current_music_time_visible[player];
        let mut clear_all = false;
        let mut chart = ChartAttackEffects::default();
        let mut accel = AccelOverrides::default();
        let mut visual = VisualOverrides::default();
        let mut appearance = AppearanceOverrides::default();
        let mut visibility = VisibilityOverrides::default();
        let mut scroll = ScrollOverrides::default();
        let mut perspective = PerspectiveOverrides::default();
        let mut scroll_speed = None;
        let mut mini_percent = None;
        for window in &state.attack_mask_windows[player] {
            if now >= window.start_second && now < window.end_second {
                if window.clear_all {
                    clear_all = true;
                    accel = AccelOverrides::default();
                    visual = VisualOverrides::default();
                    appearance = AppearanceOverrides::default();
                    visibility = VisibilityOverrides::default();
                    scroll = ScrollOverrides::default();
                    perspective = PerspectiveOverrides::default();
                    scroll_speed = None;
                    mini_percent = None;
                }
                chart.insert_mask |= window.chart.insert_mask;
                chart.remove_mask |= window.chart.remove_mask;
                chart.holds_mask |= window.chart.holds_mask;
                chart.turn_bits |= window.chart.turn_bits;
                if let Some(v) = window.accel.boost {
                    accel.boost = Some(v);
                }
                if let Some(v) = window.accel.brake {
                    accel.brake = Some(v);
                }
                if let Some(v) = window.accel.wave {
                    accel.wave = Some(v);
                }
                if let Some(v) = window.accel.expand {
                    accel.expand = Some(v);
                }
                if let Some(v) = window.accel.boomerang {
                    accel.boomerang = Some(v);
                }
                if let Some(v) = window.visual.drunk {
                    visual.drunk = Some(v);
                }
                if let Some(v) = window.visual.dizzy {
                    visual.dizzy = Some(v);
                }
                if let Some(v) = window.visual.confusion {
                    visual.confusion = Some(v);
                }
                if let Some(v) = window.visual.flip {
                    visual.flip = Some(v);
                }
                if let Some(v) = window.visual.invert {
                    visual.invert = Some(v);
                }
                if let Some(v) = window.visual.tornado {
                    visual.tornado = Some(v);
                }
                if let Some(v) = window.visual.tipsy {
                    visual.tipsy = Some(v);
                }
                if let Some(v) = window.visual.bumpy {
                    visual.bumpy = Some(v);
                }
                if let Some(v) = window.visual.beat {
                    visual.beat = Some(v);
                }
                if let Some(v) = window.appearance.hidden {
                    appearance.hidden = Some(v);
                }
                if let Some(v) = window.appearance.sudden {
                    appearance.sudden = Some(v);
                }
                if let Some(v) = window.appearance.stealth {
                    appearance.stealth = Some(v);
                }
                if let Some(v) = window.appearance.blink {
                    appearance.blink = Some(v);
                }
                if let Some(v) = window.appearance.random_vanish {
                    appearance.random_vanish = Some(v);
                }
                if let Some(v) = window.visibility.dark {
                    visibility.dark = Some(v);
                }
                if let Some(v) = window.visibility.blind {
                    visibility.blind = Some(v);
                }
                if let Some(v) = window.visibility.cover {
                    visibility.cover = Some(v);
                }
                if let Some(v) = window.scroll.reverse {
                    scroll.reverse = Some(v);
                }
                if let Some(v) = window.scroll.split {
                    scroll.split = Some(v);
                }
                if let Some(v) = window.scroll.alternate {
                    scroll.alternate = Some(v);
                }
                if let Some(v) = window.scroll.cross {
                    scroll.cross = Some(v);
                }
                if let Some(v) = window.scroll.centered {
                    scroll.centered = Some(v);
                }
                if let Some(v) = window.perspective.tilt {
                    perspective.tilt = Some(v);
                }
                if let Some(v) = window.perspective.skew {
                    perspective.skew = Some(v);
                }
                if let Some(speed) = window.scroll_speed {
                    scroll_speed = Some(speed);
                }
                if let Some(mini) = window.mini_percent.filter(|v| v.is_finite()) {
                    mini_percent = Some(mini.clamp(-100.0, 150.0));
                }
            }
        }
        state.active_attack_clear_all[player] = clear_all;
        state.active_attack_chart[player] = chart;
        state.active_attack_accel[player] = accel;
        state.active_attack_visual[player] = visual;
        state.active_attack_appearance[player] = appearance;
        state.active_attack_visibility[player] = visibility;
        state.active_attack_scroll[player] = scroll;
        state.active_attack_perspective[player] = perspective;
        state.active_attack_scroll_speed[player] = scroll_speed;
        state.active_attack_mini_percent[player] = mini_percent;
    }
}

#[inline(always)]
fn merge_attack_value(base: f32, attack: Option<f32>) -> f32 {
    attack.filter(|v| v.is_finite()).unwrap_or(base)
}

#[inline(always)]
fn player_attack_base_cleared(state: &State, player_idx: usize) -> bool {
    player_idx < state.num_players && state.active_attack_clear_all[player_idx]
}

#[inline(always)]
pub fn effective_accel_effects_for_player(state: &State, player_idx: usize) -> AccelEffects {
    if player_idx >= state.num_players {
        return AccelEffects::default();
    }
    let base = if player_attack_base_cleared(state, player_idx) {
        AccelEffects::default()
    } else {
        AccelEffects::from_mask(profile::normalize_accel_effects_mask(
            state.player_profiles[player_idx].accel_effects_active_mask,
        ))
    };
    let attack = state.active_attack_accel[player_idx];
    AccelEffects {
        boost: merge_attack_value(base.boost, attack.boost),
        brake: merge_attack_value(base.brake, attack.brake),
        wave: merge_attack_value(base.wave, attack.wave),
        expand: merge_attack_value(base.expand, attack.expand),
        boomerang: merge_attack_value(base.boomerang, attack.boomerang),
    }
}

#[inline(always)]
pub fn effective_visual_effects_for_player(state: &State, player_idx: usize) -> VisualEffects {
    if player_idx >= state.num_players {
        return VisualEffects::default();
    }
    let base = if player_attack_base_cleared(state, player_idx) {
        VisualEffects::default()
    } else {
        VisualEffects::from_mask(profile::normalize_visual_effects_mask(
            state.player_profiles[player_idx].visual_effects_active_mask,
        ))
    };
    let attack = state.active_attack_visual[player_idx];
    VisualEffects {
        drunk: merge_attack_value(base.drunk, attack.drunk),
        dizzy: merge_attack_value(base.dizzy, attack.dizzy),
        confusion: merge_attack_value(base.confusion, attack.confusion),
        big: base.big,
        flip: merge_attack_value(base.flip, attack.flip),
        invert: merge_attack_value(base.invert, attack.invert),
        tornado: merge_attack_value(base.tornado, attack.tornado),
        tipsy: merge_attack_value(base.tipsy, attack.tipsy),
        bumpy: merge_attack_value(base.bumpy, attack.bumpy),
        beat: merge_attack_value(base.beat, attack.beat),
    }
}

#[inline(always)]
pub fn effective_appearance_effects_for_player(
    state: &State,
    player_idx: usize,
) -> AppearanceEffects {
    if player_idx >= state.num_players {
        return AppearanceEffects::default();
    }
    let base = if player_attack_base_cleared(state, player_idx) {
        AppearanceEffects::default()
    } else {
        AppearanceEffects::from_mask(profile::normalize_appearance_effects_mask(
            state.player_profiles[player_idx].appearance_effects_active_mask,
        ))
    };
    let attack = state.active_attack_appearance[player_idx];
    AppearanceEffects {
        hidden: merge_attack_value(base.hidden, attack.hidden),
        sudden: merge_attack_value(base.sudden, attack.sudden),
        stealth: merge_attack_value(base.stealth, attack.stealth),
        blink: merge_attack_value(base.blink, attack.blink),
        random_vanish: merge_attack_value(base.random_vanish, attack.random_vanish),
    }
}

#[inline(always)]
pub fn effective_visibility_effects_for_player(
    state: &State,
    player_idx: usize,
) -> VisibilityEffects {
    if player_idx >= state.num_players {
        return VisibilityEffects::default();
    }
    let attack = state.active_attack_visibility[player_idx];
    VisibilityEffects {
        dark: merge_attack_value(0.0, attack.dark),
        blind: merge_attack_value(0.0, attack.blind),
        cover: merge_attack_value(0.0, attack.cover),
    }
}

#[inline(always)]
pub fn active_chart_attack_effects_for_player(
    state: &State,
    player_idx: usize,
) -> ChartAttackEffects {
    if player_idx >= state.num_players {
        return ChartAttackEffects::default();
    }
    state.active_attack_chart[player_idx]
}

#[inline(always)]
pub fn effective_scroll_effects_for_player(state: &State, player_idx: usize) -> ScrollEffects {
    if player_idx >= state.num_players {
        return ScrollEffects::default();
    }
    let base = if player_attack_base_cleared(state, player_idx) {
        ScrollEffects::default()
    } else {
        ScrollEffects::from_option(state.player_profiles[player_idx].scroll_option)
    };
    let attack = state.active_attack_scroll[player_idx];
    ScrollEffects {
        reverse: merge_attack_value(base.reverse, attack.reverse),
        split: merge_attack_value(base.split, attack.split),
        alternate: merge_attack_value(base.alternate, attack.alternate),
        cross: merge_attack_value(base.cross, attack.cross),
        centered: merge_attack_value(base.centered, attack.centered),
    }
}

#[inline(always)]
pub fn effective_perspective_effects_for_player(
    state: &State,
    player_idx: usize,
) -> PerspectiveEffects {
    if player_idx >= state.num_players {
        return PerspectiveEffects::default();
    }
    let base = if player_attack_base_cleared(state, player_idx) {
        PerspectiveEffects::default()
    } else {
        PerspectiveEffects::from_perspective(state.player_profiles[player_idx].perspective)
    };
    let attack = state.active_attack_perspective[player_idx];
    PerspectiveEffects {
        tilt: merge_attack_value(base.tilt, attack.tilt),
        skew: merge_attack_value(base.skew, attack.skew),
    }
}

#[inline(always)]
pub fn effective_visual_mask_for_player(state: &State, player_idx: usize) -> u16 {
    effective_visual_effects_for_player(state, player_idx).to_mask()
}

#[inline(always)]
pub fn effective_mini_percent_for_player(state: &State, player_idx: usize) -> f32 {
    if player_idx >= state.num_players {
        return 0.0;
    }
    state.active_attack_mini_percent[player_idx]
        .filter(|v| v.is_finite())
        .unwrap_or_else(|| {
            if player_attack_base_cleared(state, player_idx) {
                0.0
            } else {
                state.player_profiles[player_idx].mini_percent as f32
            }
        })
}

#[inline(always)]
pub fn effective_scroll_speed_for_player(state: &State, player_idx: usize) -> ScrollSpeedSetting {
    if player_idx >= state.num_players {
        return ScrollSpeedSetting::default();
    }
    state.active_attack_scroll_speed[player_idx].unwrap_or_else(|| {
        if player_attack_base_cleared(state, player_idx) {
            ScrollSpeedSetting::default()
        } else {
            state.scroll_speed[player_idx]
        }
    })
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

#[inline(always)]
fn is_player_dead(p: &PlayerRuntime) -> bool {
    p.is_failing || p.life <= 0.0
}

#[inline(always)]
fn is_state_dead(state: &State, player: usize) -> bool {
    is_player_dead(&state.players[player])
}

#[inline(always)]
fn all_joined_players_failed(state: &State) -> bool {
    if state.num_players == 0 {
        return false;
    }
    for player in 0..state.num_players {
        if !is_state_dead(state, player) {
            return false;
        }
    }
    true
}

const TOGGLE_FLASH_DURATION: f32 = 1.5;
const TOGGLE_FLASH_FADE_START: f32 = 0.8;

#[inline(always)]
pub fn timing_tick_status_line(state: &State) -> Option<&'static str> {
    tick_mode_status_line(state.tick_mode)
}

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
pub fn display_clock_health(state: &State) -> DisplayClockHealth {
    DisplayClockHealth {
        error_seconds: state.display_clock.target_time_sec - state.display_clock.current_time_sec,
        catching_up: state.display_clock.catching_up,
    }
}

#[inline(always)]
fn autoplay_blocks_scoring(state: &State) -> bool {
    state.autoplay_enabled && !state.replay_mode
}

#[inline(always)]
fn autoplay_tap_offset_s(state: &mut State) -> f32 {
    let w1 = state
        .timing_profile
        .windows_s
        .first()
        .copied()
        .unwrap_or(0.0)
        .max(0.0);
    if w1 <= 0.0 {
        return 0.0;
    }

    let mut offset = (state.autoplay_rng.next_f32_unit() * 2.0 - 1.0) * w1;
    if offset.abs() < AUTOPLAY_OFFSET_EPSILON_SECONDS {
        let sign = if state.autoplay_rng.next_u32() & 1 == 0 {
            -1.0
        } else {
            1.0
        };
        offset = sign * AUTOPLAY_OFFSET_EPSILON_SECONDS.min(w1);
    }
    offset
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExitTransitionKind {
    Out,
    Cancel,
}

#[derive(Clone, Copy, Debug)]
pub struct ExitTransition {
    pub kind: ExitTransitionKind,
    pub target: Screen,
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
fn begin_exit_transition(state: &mut State, kind: ExitTransitionKind, target: Screen) {
    if state.exit_transition.is_some() {
        return;
    }
    state.hold_to_exit_key = None;
    state.hold_to_exit_start = None;
    state.hold_to_exit_aborted_at = None;
    state.exit_transition = Some(ExitTransition {
        kind,
        target,
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

fn clip_density_life_points(points: &mut Vec<[f32; 2]>, offset: f32) {
    let first_visible = points.partition_point(|p| p[0] < offset);
    if first_visible == 0 {
        return;
    }
    if first_visible >= points.len() {
        points.clear();
        return;
    }

    let a = points[first_visible - 1];
    let b = points[first_visible];
    let dx = (b[0] - a[0]).max(0.000_001_f32);
    let t = ((offset - a[0]) / dx).clamp(0.0_f32, 1.0_f32);
    points[first_visible - 1] = [offset, a[1] + (b[1] - a[1]) * t];
    points.drain(0..(first_visible - 1));
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
        for player in 0..state.num_players {
            state.density_graph_mesh[player] = None;
        }
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
    let offset = (u0 * scaled_width).clamp(0.0_f32, scaled_width);
    let offset_px = offset.floor() as i32;
    let offset_px_f = offset_px as f32;

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
                .round()
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

    for player in 0..state.num_players {
        if offset_px == state.density_graph_mesh_offset_px[player] {
            continue;
        }
        if trace_enabled {
            let started = Instant::now();
            state.density_graph_mesh_offset_px[player] = offset_px;
            let verts = state.density_graph_cache[player]
                .as_ref()
                .map_or(Vec::new(), |cache| cache.mesh(offset_px as f32, graph_w));
            state.density_graph_mesh[player] = if verts.is_empty() {
                None
            } else {
                Some(Arc::from(verts.into_boxed_slice()))
            };
            add_elapsed_us(&mut phase_timings.density_hist_mesh_us, started);
        } else {
            state.density_graph_mesh_offset_px[player] = offset_px;
            let verts = state.density_graph_cache[player]
                .as_ref()
                .map_or(Vec::new(), |cache| cache.mesh(offset_px as f32, graph_w));
            state.density_graph_mesh[player] = if verts.is_empty() {
                None
            } else {
                Some(Arc::from(verts.into_boxed_slice()))
            };
        }
    }

    for player in 0..state.num_players {
        let prev_offset_px = state.density_graph_life_mesh_offset_px[player];
        let offset_changed = offset_px != prev_offset_px;
        if !offset_changed && !state.density_graph_life_dirty[player] {
            continue;
        }
        state.density_graph_life_mesh_offset_px[player] = offset_px;
        state.density_graph_life_dirty[player] = false;
        let should_clip = offset_px > prev_offset_px;

        if trace_enabled {
            if should_clip {
                let clip_started = Instant::now();
                clip_density_life_points(&mut state.density_graph_life_points[player], offset_px_f);
                add_elapsed_us(&mut phase_timings.density_clip_us, clip_started);
            }
            if state.density_graph_life_points[player].len() < 2 {
                state.density_graph_life_mesh[player] = None;
                continue;
            }

            let mesh_started = Instant::now();
            density_graph::update_density_life_mesh(
                &mut state.density_graph_life_mesh[player],
                &state.density_graph_life_points[player],
                offset_px_f,
                graph_w,
                2.0_f32,
                [1.0_f32, 1.0_f32, 1.0_f32, 0.8_f32],
            );
            add_elapsed_us(&mut phase_timings.density_life_mesh_us, mesh_started);
        } else {
            if should_clip {
                clip_density_life_points(&mut state.density_graph_life_points[player], offset_px_f);
            }
            if state.density_graph_life_points[player].len() < 2 {
                state.density_graph_life_mesh[player] = None;
                continue;
            }
            density_graph::update_density_life_mesh(
                &mut state.density_graph_life_mesh[player],
                &state.density_graph_life_points[player],
                offset_px_f,
                graph_w,
                2.0_f32,
                [1.0_f32, 1.0_f32, 1.0_f32, 0.8_f32],
            );
        }
    }
}

#[inline(always)]
fn record_life(p: &mut PlayerRuntime, t: f32, life: f32) {
    const SHIFT: f32 = 0.003_906_25_f32; // 1/256, matches ITGmania's PlayerStageStats quirk

    let life = life.clamp(0.0_f32, 1.0_f32);
    let hist = &mut p.life_history;
    let Some(&(last_t, last_life)) = hist.last() else {
        hist.push((t, life));
        return;
    };

    if t > last_t {
        if (life - last_life).abs() > 0.000_001_f32 {
            hist.push((t, life));
        }
        return;
    }

    if (t - last_t).abs() <= 0.000_001_f32 {
        if (life - last_life).abs() <= 0.000_001_f32 {
            return;
        }
        let last_ix = hist.len() - 1;
        hist[last_ix].0 = t - SHIFT;
        hist.push((t, life));
    }
}

fn apply_life_change(p: &mut PlayerRuntime, current_music_time: f32, delta: f32) {
    if is_player_dead(p) {
        p.life = 0.0;
        p.is_failing = true;
        return;
    }

    let old_life = p.life;

    let mut final_delta = delta;
    if final_delta > 0.0 {
        if p.combo_after_miss > 0 {
            final_delta = 0.0;
            p.combo_after_miss -= 1;
        }
    } else if final_delta < 0.0 {
        p.combo_after_miss = REGEN_COMBO_AFTER_MISS;
    }

    let mut new_life = (p.life + final_delta).clamp(0.0, 1.0);

    if new_life <= 0.0 {
        if !p.is_failing {
            p.fail_time = Some(current_music_time);
        }
        new_life = 0.0;
        p.is_failing = true;
        debug!("Player has failed!");
    }

    if (new_life - old_life).abs() > 0.000_001_f32 {
        record_life(p, current_music_time, old_life);
        record_life(p, current_music_time, new_life);
    }
    p.life = new_life;
}

pub fn queue_input_edge(
    state: &mut State,
    source: InputSource,
    lane: Lane,
    pressed: bool,
    timestamp: Instant,
    timestamp_host_nanos: u64,
    stored_at: Instant,
    emitted_at: Instant,
) {
    if state.autoplay_enabled {
        return;
    }
    let play_style = profile::get_session_play_style();
    let player_side = profile::get_session_player_side();
    let lane = match (play_style, player_side, lane) {
        // Single-player: reject the "other side" entirely so only one set of bindings can play.
        (
            profile::PlayStyle::Single,
            profile::PlayerSide::P1,
            Lane::P2Left | Lane::P2Down | Lane::P2Up | Lane::P2Right,
        ) => return,
        (
            profile::PlayStyle::Single,
            profile::PlayerSide::P2,
            Lane::Left | Lane::Down | Lane::Up | Lane::Right,
        ) => return,
        // P2-only single: remap P2 lanes into the 4-col field.
        (profile::PlayStyle::Single, profile::PlayerSide::P2, Lane::P2Left) => Lane::Left,
        (profile::PlayStyle::Single, profile::PlayerSide::P2, Lane::P2Down) => Lane::Down,
        (profile::PlayStyle::Single, profile::PlayerSide::P2, Lane::P2Up) => Lane::Up,
        (profile::PlayStyle::Single, profile::PlayerSide::P2, Lane::P2Right) => Lane::Right,
        _ => lane,
    };
    if lane.index() >= state.num_cols {
        return;
    }

    let queued_at = Instant::now();
    push_input_edge_timed(
        state,
        source,
        lane,
        pressed,
        timestamp,
        timestamp_host_nanos,
        stored_at,
        emitted_at,
        queued_at,
        f32::NAN,
        state.replay_capture_enabled,
    );
}

#[inline(always)]
pub fn set_replay_capture_enabled(state: &mut State, enabled: bool) {
    state.replay_capture_enabled = enabled;
}

#[inline(always)]
pub fn replay_capture_enabled(state: &State) -> bool {
    state.replay_capture_enabled
}

#[inline(always)]
fn current_music_time_from_stream(state: &State) -> f32 {
    current_song_clock_snapshot(state).song_time
}

#[inline(always)]
fn current_song_clock_snapshot(state: &State) -> SongClockSnapshot {
    let stream_clock = audio::get_music_stream_clock_snapshot();
    let fallback_rate = if state.music_rate.is_finite() && state.music_rate > 0.0 {
        state.music_rate
    } else {
        1.0
    };
    if stream_clock.has_music_mapping {
        SongClockSnapshot {
            song_time: stream_clock.music_seconds,
            seconds_per_second: if stream_clock.music_seconds_per_second.is_finite()
                && stream_clock.music_seconds_per_second > 0.0
            {
                stream_clock.music_seconds_per_second
            } else {
                fallback_rate
            },
            valid_at: stream_clock.valid_at,
            valid_at_host_nanos: stream_clock.valid_at_host_nanos,
        }
    } else {
        SongClockSnapshot {
            song_time: stream_pos_to_music_time(state, stream_clock.stream_seconds),
            seconds_per_second: fallback_rate,
            valid_at: stream_clock.valid_at,
            valid_at_host_nanos: stream_clock.valid_at_host_nanos,
        }
    }
}

#[inline(always)]
fn music_time_from_song_clock(
    snapshot: SongClockSnapshot,
    captured_at: Instant,
    captured_host_nanos: u64,
) -> f32 {
    let slope = if snapshot.seconds_per_second.is_finite() && snapshot.seconds_per_second > 0.0 {
        snapshot.seconds_per_second
    } else {
        1.0
    };
    if snapshot.valid_at_host_nanos != 0 && captured_host_nanos != 0 {
        let dt_nanos = captured_host_nanos as i128 - snapshot.valid_at_host_nanos as i128;
        return snapshot.song_time + (dt_nanos as f64 * 1e-9 * slope as f64) as f32;
    }
    if let Some(age) = snapshot.valid_at.checked_duration_since(captured_at) {
        snapshot.song_time - age.as_secs_f32() * slope
    } else if let Some(lead) = captured_at.checked_duration_since(snapshot.valid_at) {
        snapshot.song_time + lead.as_secs_f32() * slope
    } else {
        snapshot.song_time
    }
}

const DISPLAY_CLOCK_CORRECTION_HALF_LIFE_S: f32 = 0.012;
const DISPLAY_CLOCK_MAX_LAG_S: f32 = 0.020;
const DISPLAY_CLOCK_MAX_LEAD_S: f32 = 0.006;
const DISPLAY_CLOCK_RESET_ERROR_S: f32 = 0.100;
const DISPLAY_CLOCK_MAX_STEP_S: f32 = 1.0 / 60.0;

#[inline(always)]
fn frame_stable_display_music_time(
    display_clock: &mut FrameStableDisplayClock,
    target_display_time_sec: f32,
    delta_time: f32,
    seconds_per_second: f32,
    first_update: bool,
) -> f32 {
    display_clock.target_time_sec = target_display_time_sec;
    if first_update
        || !display_clock.current_time_sec.is_finite()
        || !target_display_time_sec.is_finite()
        || !delta_time.is_finite()
        || delta_time <= 0.0
    {
        return display_clock.reset(target_display_time_sec);
    }

    let slope = if seconds_per_second.is_finite() && seconds_per_second > 0.0 {
        seconds_per_second
    } else {
        1.0
    };
    let previous_display_time_sec = display_clock.current_time_sec;
    let max_error = DISPLAY_CLOCK_RESET_ERROR_S * slope;
    if (target_display_time_sec - previous_display_time_sec).abs() > max_error {
        return display_clock.reset(target_display_time_sec);
    }

    let advanced = previous_display_time_sec + delta_time * slope;
    let correction_alpha = 1.0 - f32::exp2(-delta_time / DISPLAY_CLOCK_CORRECTION_HALF_LIFE_S);
    let mut corrected = advanced + (target_display_time_sec - advanced) * correction_alpha;
    let max_step = DISPLAY_CLOCK_MAX_STEP_S * slope;
    let step = corrected - previous_display_time_sec;
    if step.abs() > max_step * 1.2 {
        corrected = previous_display_time_sec + step.signum() * max_step;
    }
    let min_allowed = target_display_time_sec - DISPLAY_CLOCK_MAX_LAG_S * slope;
    let max_allowed = target_display_time_sec + DISPLAY_CLOCK_MAX_LEAD_S * slope;
    let corrected = corrected
        .clamp(min_allowed, max_allowed)
        .max(previous_display_time_sec);
    display_clock.current_time_sec = corrected;
    display_clock.catching_up = (target_display_time_sec - corrected).abs() > max_step * 0.5;
    corrected
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

pub fn start_stage_music(state: &State) {
    let Some(music_path) = state.song.music_path.as_ref() else {
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

#[inline(always)]
fn push_input_edge(
    state: &mut State,
    source: InputSource,
    lane: Lane,
    pressed: bool,
    event_music_time: f32,
    record_replay: bool,
) {
    let now = Instant::now();
    push_input_edge_timed(
        state,
        source,
        lane,
        pressed,
        now,
        0,
        now,
        now,
        now,
        event_music_time,
        record_replay,
    );
}

#[inline(always)]
fn push_input_edge_timed(
    state: &mut State,
    source: InputSource,
    lane: Lane,
    pressed: bool,
    captured_at: Instant,
    captured_host_nanos: u64,
    stored_at: Instant,
    emitted_at: Instant,
    queued_at: Instant,
    event_music_time: f32,
    record_replay: bool,
) {
    if lane.index() >= state.num_cols {
        return;
    }
    state.pending_edges.push_back(InputEdge {
        lane,
        pressed,
        source,
        record_replay,
        captured_at,
        captured_host_nanos,
        stored_at,
        emitted_at,
        queued_at,
        event_music_time,
    });
    if log::log_enabled!(log::Level::Debug) {
        let pending_len = state.pending_edges.len();
        if pending_len >= GAMEPLAY_INPUT_BACKLOG_WARN {
            debug!(
                "Gameplay input queue pressure: pending_edges={}, num_cols={}, music_time={:.3}",
                pending_len, state.num_cols, state.current_music_time
            );
        }
    }
}

#[inline(always)]
const fn lane_from_column(column: usize) -> Option<Lane> {
    match column {
        0 => Some(Lane::Left),
        1 => Some(Lane::Down),
        2 => Some(Lane::Up),
        3 => Some(Lane::Right),
        4 => Some(Lane::P2Left),
        5 => Some(Lane::P2Down),
        6 => Some(Lane::P2Up),
        7 => Some(Lane::P2Right),
        _ => None,
    }
}

fn get_reference_bpm_from_display_tag(display_bpm_str: &str) -> Option<f32> {
    let s = display_bpm_str.trim();
    if s.is_empty() || s == "*" {
        return None;
    }
    if let Some((_, max_str)) = s.split_once(':') {
        return max_str.trim().parse::<f32>().ok();
    }
    s.parse::<f32>().ok()
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
    let rate = if music_rate.is_finite() && music_rate > 0.0 {
        music_rate
    } else {
        1.0
    };

    let play_style = profile::get_session_play_style();
    let player_side = profile::get_session_player_side();
    let (cols_per_player, num_players, num_cols) = match play_style {
        profile::PlayStyle::Single => (4, 1, 4),
        profile::PlayStyle::Double => (8, 1, 8),
        profile::PlayStyle::Versus => (4, 2, 8),
    };
    let replay_edges = replay_edges.unwrap_or_default();
    let mut charts = charts;
    let mut gameplay_charts = gameplay_charts;
    if play_style == profile::PlayStyle::Single && player_side == profile::PlayerSide::P2 {
        scroll_speed[0] = scroll_speed[1];
        player_profiles[0] = player_profiles[1].clone();
        charts[0] = charts[1].clone();
        gameplay_charts[0] = gameplay_charts[1].clone();
        combo_carry[0] = combo_carry[1];
    }
    let player_color_index =
        if play_style == profile::PlayStyle::Single && player_side == profile::PlayerSide::P2 {
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
    let mut timing_base = gameplay_charts[0].timing.clone();
    timing_base.set_global_offset_seconds(config.global_offset_seconds);
    let timing = Arc::new(timing_base);
    let mut timing_players: [Arc<TimingData>; MAX_PLAYERS] = std::array::from_fn(|player| {
        let mut t = gameplay_charts[player].timing.clone();
        t.set_global_offset_seconds(config.global_offset_seconds);
        Arc::new(t)
    });
    if num_players == 1 {
        timing_players[1] = timing_players[0].clone();
    }
    let mut replay_input = Vec::with_capacity(replay_edges.len());
    let replay_offsets = replay_offsets.unwrap_or(ReplayOffsetSnapshot {
        beat0_time_seconds: timing_players[0].get_time_for_beat(0.0),
    });
    let mut replay_out_of_order = false;
    let mut replay_prev_time = f32::NEG_INFINITY;
    for edge in replay_edges {
        let lane = edge.lane_index as usize;
        if lane >= num_cols || !edge.event_music_time.is_finite() {
            continue;
        }
        let player = if num_players <= 1 || cols_per_player == 0 {
            0
        } else {
            (lane / cols_per_player).min(num_players.saturating_sub(1))
        };
        let replay_beat0_shift = if replay_offsets.beat0_time_seconds.is_finite() {
            timing_players[player].get_time_for_beat(0.0) - replay_offsets.beat0_time_seconds
        } else {
            0.0
        };
        let event_music_time = edge.event_music_time + replay_beat0_shift;
        if !event_music_time.is_finite() {
            continue;
        }
        if event_music_time < replay_prev_time {
            replay_out_of_order = true;
        }
        replay_prev_time = event_music_time;
        replay_input.push(RecordedLaneEdge {
            lane_index: edge.lane_index,
            pressed: edge.pressed,
            source: edge.source,
            event_music_time,
        });
    }
    if replay_out_of_order {
        replay_input.sort_by(|a, b| {
            a.event_music_time
                .partial_cmp(&b.event_music_time)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
    }
    let replay_mode = !replay_input.is_empty();
    if replay_mode {
        debug!(
            "Gameplay replay mode enabled: {} recorded edges loaded.",
            replay_input.len(),
        );
    }
    let beat_info_cache = BeatInfoCache::new(&timing);

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
    for player in 0..num_players {
        score_valid[player] = score_valid_for_chart(
            &charts[player],
            &player_profiles[player],
            scroll_speed[player],
            rate,
        );
    }

    let mut total_steps = [0u32; MAX_PLAYERS];
    let mut jumps_total = [0u32; MAX_PLAYERS];
    let mut hands_total = [0u32; MAX_PLAYERS];
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
    }

    let note_player_for_col = |col: usize| -> usize {
        if num_players <= 1 || cols_per_player == 0 {
            0
        } else {
            (col / cols_per_player).min(num_players.saturating_sub(1))
        }
    };

    let note_time_cache: Vec<f32> = notes
        .iter()
        .map(|n| timing_players[note_player_for_col(n.column)].get_time_for_beat(n.beat))
        .collect();
    let note_display_beat_cache: Vec<f32> = notes
        .iter()
        .map(|n| timing_players[note_player_for_col(n.column)].get_displayed_beat(n.beat))
        .collect();
    let hold_end_time_cache: Vec<Option<f32>> = notes
        .iter()
        .map(|n| {
            n.hold.as_ref().map(|h| {
                timing_players[note_player_for_col(n.column)].get_time_for_beat(h.end_beat)
            })
        })
        .collect();
    let hold_end_display_beat_cache: Vec<Option<f32>> = notes
        .iter()
        .map(|n| {
            n.hold.as_ref().map(|h| {
                timing_players[note_player_for_col(n.column)].get_displayed_beat(h.end_beat)
            })
        })
        .collect();

    let mut possible_grade_points = [0i32; MAX_PLAYERS];
    for player in 0..num_players {
        possible_grade_points[player] = compute_possible_grade_points(
            &notes,
            note_ranges[player],
            holds_total[player],
            rolls_total[player],
        );
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
        note_ranges[1] = note_ranges[0];
    }

    debug!("Parsed {} notes from chart data.", notes.len());

    let mut row_map: HashMap<usize, Vec<usize>> = HashMap::new();
    for (i, n) in notes.iter().enumerate() {
        if matches!(n.note_type, NoteType::Mine) {
            continue;
        }
        if !n.can_be_judged {
            continue;
        }
        row_map.entry(n.row_index).or_default().push(i);
    }
    let mut row_entries: Vec<RowEntry> = row_map
        .into_iter()
        .map(|(row_index, nonmine_note_indices)| RowEntry {
            row_index,
            nonmine_note_indices,
        })
        .collect();
    row_entries.sort_by_key(|e| e.row_index);

    // Build optimized O(1) lookup table for row entries
    let mut row_map_cache = vec![u32::MAX; max_row_index + 1];
    for (pos, entry) in row_entries.iter().enumerate() {
        if entry.row_index < row_map_cache.len() {
            row_map_cache[entry.row_index] = pos as u32;
        }
    }
    let mut row_hold_roll_flags: HashMap<usize, u8> = HashMap::new();
    for note in &notes {
        let flag = match note.note_type {
            NoteType::Hold => 0b01,
            NoteType::Roll => 0b10,
            _ => 0,
        };
        if flag != 0 {
            *row_hold_roll_flags.entry(note.row_index).or_insert(0) |= flag;
        }
    }
    let mut tap_row_hold_roll_flags = vec![0u8; notes.len()];
    for (idx, note) in notes.iter().enumerate() {
        tap_row_hold_roll_flags[idx] = row_hold_roll_flags
            .get(&note.row_index)
            .copied()
            .unwrap_or(0);
    }

    let first_second = notes
        .iter()
        .zip(&note_time_cache)
        .filter_map(|(n, &t)| n.can_be_judged.then_some(t))
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

    let mut reference_bpm =
        get_reference_bpm_from_display_tag(&song.display_bpm).unwrap_or_else(|| {
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
    let (notes_end_time, music_end_time) =
        compute_end_times(&notes, &note_time_cache, &hold_end_time_cache, rate);
    let notes_len = notes.len();
    let mut column_scroll_dirs = [1.0_f32; MAX_COLS];
    for player in 0..num_players {
        let start = player * cols_per_player;
        let end = (start + cols_per_player).min(num_cols).min(MAX_COLS);
        let local_dirs =
            compute_column_scroll_dirs(player_profiles[player].scroll_option, cols_per_player);
        for col in start..end {
            column_scroll_dirs[col] = local_dirs[col - start];
        }
    }

    let note_range_start: [usize; MAX_PLAYERS] =
        std::array::from_fn(|player| note_ranges[player].0);
    let mut mine_note_ix: [Vec<usize>; MAX_PLAYERS] = std::array::from_fn(|_| Vec::new());
    let mut mine_note_time: [Vec<f32>; MAX_PLAYERS] = std::array::from_fn(|_| Vec::new());
    for player in 0..num_players {
        let (start, end) = note_ranges[player];
        let mut mine_ix = Vec::with_capacity(mines_total[player] as usize);
        let mut mine_times = Vec::with_capacity(mines_total[player] as usize);
        for note_idx in start..end {
            if matches!(notes[note_idx].note_type, NoteType::Mine) {
                mine_ix.push(note_idx);
                mine_times.push(note_time_cache[note_idx]);
            }
        }
        mine_note_ix[player] = mine_ix;
        mine_note_time[player] = mine_times;
    }
    let next_mine_ix_cursor: [usize; MAX_PLAYERS] = [0; MAX_PLAYERS];
    let mut arrow_capacity = [0usize; MAX_COLS];
    let mut replay_cells = 0usize;
    for note in &notes {
        let col = note.column;
        if col < num_cols && col < MAX_COLS {
            arrow_capacity[col] = arrow_capacity[col].saturating_add(1);
        }
        if note.can_be_judged && !matches!(note.note_type, NoteType::Mine) {
            replay_cells = replay_cells.saturating_add(1);
        }
    }
    let pending_edges_capacity = input_queue_cap(num_cols);
    let replay_seconds = (music_end_time + start_delay).max(notes_end_time + start_delay);
    let replay_capture_enabled = !replay_mode && config.machine_enable_replays;
    let replay_edges_capacity = [
        0,
        replay_edge_cap(num_cols, replay_cells, replay_mode, replay_seconds),
    ][replay_capture_enabled as usize];
    let decaying_hold_capacity = (0..num_players).fold(0usize, |acc, player| {
        acc.saturating_add(holds_total[player] as usize + rolls_total[player] as usize)
    });

    let global_visual_delay_seconds = config.visual_delay_seconds;
    let player_visual_delay_seconds: [f32; MAX_PLAYERS] = std::array::from_fn(|player| {
        if player >= num_players {
            return 0.0;
        }
        let ms = player_profiles[player].visual_delay_ms.clamp(-100, 100);
        ms as f32 / 1000.0
    });
    let init_music_time = -start_delay;
    let init_beat = timing.get_beat_for_time(init_music_time);
    let current_music_time_visible: [f32; MAX_PLAYERS] = std::array::from_fn(|player| {
        init_music_time - global_visual_delay_seconds - player_visual_delay_seconds[player]
    });
    let current_beat_visible: [f32; MAX_PLAYERS] = std::array::from_fn(|player| {
        timing_players[player].get_beat_for_time(current_music_time_visible[player])
    });
    let attack_mask_windows: [Vec<AttackMaskWindow>; MAX_PLAYERS] = std::array::from_fn(|player| {
        if player >= num_players {
            return Vec::new();
        }
        build_attack_mask_windows_for_player(
            gameplay_charts[player].chart_attacks.as_deref(),
            player_profiles[player].attack_mode,
            player,
            song_seed,
            attack_song_length_seconds,
        )
    });
    let reverse_scroll: [bool; MAX_PLAYERS] = std::array::from_fn(|player| {
        if player >= num_players {
            return false;
        }
        player_profiles[player].reverse_scroll
    });
    let mut column_cues: [Vec<ColumnCue>; MAX_PLAYERS] = std::array::from_fn(|_| Vec::new());
    for player in 0..num_players {
        let col_start = player.saturating_mul(cols_per_player);
        let col_end = (col_start + cols_per_player).min(num_cols);
        column_cues[player] = build_column_cues_for_player(
            &notes,
            note_ranges[player],
            &note_time_cache,
            col_start,
            col_end,
            current_music_time_visible[player],
        );
    }
    if num_players == 1 {
        column_cues[1] = column_cues[0].clone();
    }

    let measure_densities: [Vec<usize>; MAX_PLAYERS] = std::array::from_fn(|p| {
        if p >= num_players {
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
    let density_graph_top_mesh: [Option<Arc<[MeshVertex]>>; MAX_PLAYERS] =
        std::array::from_fn(|p| {
            let graph_w = density_graph_top_w[p];
            let graph_h = density_graph_top_h * density_graph_top_scale_y[p].clamp(0.0, 1.0);
            if p >= num_players || graph_w <= 0.0 || graph_h <= 0.0 {
                return None;
            }
            let chart = charts[p].as_ref();
            let verts =
                crate::screens::components::shared::density_graph::build_density_histogram_mesh(
                    &chart.measure_nps_vec,
                    chart.max_nps,
                    &chart.measure_seconds_vec,
                    density_graph_first_second,
                    density_graph_last_second,
                    graph_w,
                    graph_h,
                    0.0,
                    graph_w,
                    None,
                    1.0,
                );
            if verts.is_empty() {
                None
            } else {
                Some(Arc::from(verts.into_boxed_slice()))
            }
        });

    let density_graph_cache: [Option<DensityHistCache>; MAX_PLAYERS] = std::array::from_fn(|p| {
        if !density_graph_enabled || p >= num_players {
            return None;
        }
        let chart = charts[p].as_ref();
        crate::screens::components::shared::density_graph::build_density_histogram_cache(
            &chart.measure_nps_vec,
            chart.max_nps,
            &chart.measure_seconds_vec,
            density_graph_first_second,
            density_graph_last_second,
            density_graph_scaled_width,
            density_graph_graph_h,
            None,
            1.0,
        )
    });
    let density_graph_mesh_offset_px: [i32; MAX_PLAYERS] = [0; MAX_PLAYERS];
    let density_graph_mesh: [Option<Arc<[MeshVertex]>>; MAX_PLAYERS] = std::array::from_fn(|p| {
        if !density_graph_enabled || p >= num_players {
            return None;
        }
        density_graph_cache[p].as_ref().and_then(|cache| {
            let verts = cache.mesh(0.0_f32, density_graph_graph_w);
            if verts.is_empty() {
                None
            } else {
                Some(Arc::from(verts.into_boxed_slice()))
            }
        })
    });

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
    let density_graph_life_mesh: [Option<Arc<[MeshVertex]>>; MAX_PLAYERS] =
        std::array::from_fn(|_| None);
    let density_graph_life_mesh_offset_px: [i32; MAX_PLAYERS] = [0; MAX_PLAYERS];
    let density_graph_life_dirty: [bool; MAX_PLAYERS] = [false; MAX_PLAYERS];

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
        notes,
        note_ranges,
        audio_lead_in_seconds: start_delay,
        current_beat: init_beat,
        current_music_time: init_music_time,
        current_beat_display: init_beat,
        current_music_time_display: init_music_time,
        display_clock: FrameStableDisplayClock::new(init_music_time),
        note_spawn_cursor: note_range_start,
        judged_row_cursor: [0; MAX_PLAYERS],
        arrows: std::array::from_fn(|col| {
            let cap = arrow_capacity[col];
            if cap == 0 {
                Vec::new()
            } else {
                Vec::with_capacity(cap)
            }
        }),
        note_time_cache,
        note_display_beat_cache,
        hold_end_time_cache,
        hold_end_display_beat_cache,
        notes_end_time,
        music_end_time,
        music_rate: rate,
        play_mine_sounds: config.mine_hit_sound,
        global_offset_seconds: config.global_offset_seconds,
        initial_global_offset_seconds: config.global_offset_seconds,
        song_offset_seconds,
        initial_song_offset_seconds: song_offset_seconds,
        autosync_mode: AutosyncMode::Off,
        autosync_offset_samples: [0.0; AUTOSYNC_OFFSET_SAMPLE_COUNT],
        autosync_offset_sample_count: 0,
        autosync_standard_deviation: 0.0,
        global_visual_delay_seconds,
        player_visual_delay_seconds,
        current_music_time_visible,
        current_beat_visible,
        next_tap_miss_cursor: note_range_start,
        next_mine_avoid_cursor: note_range_start,
        mine_note_ix,
        mine_note_time,
        next_mine_ix_cursor,
        row_entries,
        measure_counter_segments,
        column_cues,
        mini_indicator_stream_segments,
        mini_indicator_total_stream_measures,
        mini_indicator_target_score_percent,
        mini_indicator_rival_score_percent,
        row_map_cache,
        tap_row_hold_roll_flags,
        decaying_hold_indices: Vec::with_capacity(decaying_hold_capacity),
        hold_decay_active: vec![false; notes_len],
        players,
        hold_judgments: Default::default(),
        is_in_freeze: false,
        is_in_delay: false,
        possible_grade_points,
        song_completed_naturally: false,
        autoplay_enabled: replay_mode,
        autoplay_used: replay_mode,
        score_valid,
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
        active_attack_clear_all: [false; MAX_PLAYERS],
        active_attack_chart: [ChartAttackEffects::default(); MAX_PLAYERS],
        active_attack_accel: [AccelOverrides::default(); MAX_PLAYERS],
        active_attack_visual: [VisualOverrides::default(); MAX_PLAYERS],
        active_attack_appearance: [AppearanceOverrides::default(); MAX_PLAYERS],
        active_attack_visibility: [VisibilityOverrides::default(); MAX_PLAYERS],
        active_attack_scroll: [ScrollOverrides::default(); MAX_PLAYERS],
        active_attack_perspective: [PerspectiveOverrides::default(); MAX_PLAYERS],
        active_attack_scroll_speed: [None; MAX_PLAYERS],
        active_attack_mini_percent: [None; MAX_PLAYERS],
        noteskin,
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
        density_graph_cache,
        density_graph_mesh,
        density_graph_mesh_offset_px,
        density_graph_life_update_rate,
        density_graph_life_next_update_elapsed,
        density_graph_life_points,
        density_graph_life_mesh,
        density_graph_life_mesh_offset_px,
        density_graph_life_dirty,
        density_graph_top_h,
        density_graph_top_w,
        density_graph_top_scale_y,
        density_graph_top_mesh,
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
        pending_edges: VecDeque::with_capacity(pending_edges_capacity),
        autoplay_rng: TurnRng::new(song_seed ^ 0xA0A7_0F8A_1A2B_3C4D),
        autoplay_cursor: note_range_start,
        autoplay_pending_row: [None; MAX_PLAYERS],
        autoplay_lane_state: [false; MAX_COLS],
        autoplay_hold_release_time: [None; MAX_COLS],
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
    refresh_active_attack_masks(&mut state);
    let current_bpm = state.timing.get_bpm_for_beat(state.current_beat);
    refresh_live_notefield_options(&mut state, current_bpm);
    state
}

pub fn course_display_carry_from_state(state: &State) -> [CourseDisplayCarry; MAX_PLAYERS] {
    let mut carry = [CourseDisplayCarry::default(); MAX_PLAYERS];
    for player in 0..state.num_players.min(MAX_PLAYERS) {
        let p = &state.players[player];
        let previous = state
            .course_display_carry
            .as_ref()
            .map_or(CourseDisplayCarry::default(), |old| old[player]);
        let mut judgment_counts = [0u32; 6];
        let mut scoring_counts = [0u32; 6];
        for grade in DISPLAY_JUDGE_ORDER {
            let ix = display_judge_ix(grade);
            let stage_judgment = p.judgment_counts[ix];
            let stage_scoring = p.scoring_counts[ix];
            judgment_counts[ix] = previous.judgment_counts[ix].saturating_add(stage_judgment);
            scoring_counts[ix] = previous.scoring_counts[ix].saturating_add(stage_scoring);
        }
        let stage_window_counts = state.live_window_counts[player];
        let stage_window_counts_10ms = state.live_window_counts_10ms_blue[player];
        let stage_window_counts_display_blue = state.live_window_counts_display_blue[player];
        let window_counts = crate::game::timing::WindowCounts {
            w0: previous
                .window_counts
                .w0
                .saturating_add(stage_window_counts.w0),
            w1: previous
                .window_counts
                .w1
                .saturating_add(stage_window_counts.w1),
            w2: previous
                .window_counts
                .w2
                .saturating_add(stage_window_counts.w2),
            w3: previous
                .window_counts
                .w3
                .saturating_add(stage_window_counts.w3),
            w4: previous
                .window_counts
                .w4
                .saturating_add(stage_window_counts.w4),
            w5: previous
                .window_counts
                .w5
                .saturating_add(stage_window_counts.w5),
            miss: previous
                .window_counts
                .miss
                .saturating_add(stage_window_counts.miss),
        };
        let window_counts_10ms_blue = crate::game::timing::WindowCounts {
            w0: previous
                .window_counts_10ms_blue
                .w0
                .saturating_add(stage_window_counts_10ms.w0),
            w1: previous
                .window_counts_10ms_blue
                .w1
                .saturating_add(stage_window_counts_10ms.w1),
            w2: previous
                .window_counts_10ms_blue
                .w2
                .saturating_add(stage_window_counts_10ms.w2),
            w3: previous
                .window_counts_10ms_blue
                .w3
                .saturating_add(stage_window_counts_10ms.w3),
            w4: previous
                .window_counts_10ms_blue
                .w4
                .saturating_add(stage_window_counts_10ms.w4),
            w5: previous
                .window_counts_10ms_blue
                .w5
                .saturating_add(stage_window_counts_10ms.w5),
            miss: previous
                .window_counts_10ms_blue
                .miss
                .saturating_add(stage_window_counts_10ms.miss),
        };
        let window_counts_display_blue = crate::game::timing::WindowCounts {
            w0: previous
                .window_counts_display_blue
                .w0
                .saturating_add(stage_window_counts_display_blue.w0),
            w1: previous
                .window_counts_display_blue
                .w1
                .saturating_add(stage_window_counts_display_blue.w1),
            w2: previous
                .window_counts_display_blue
                .w2
                .saturating_add(stage_window_counts_display_blue.w2),
            w3: previous
                .window_counts_display_blue
                .w3
                .saturating_add(stage_window_counts_display_blue.w3),
            w4: previous
                .window_counts_display_blue
                .w4
                .saturating_add(stage_window_counts_display_blue.w4),
            w5: previous
                .window_counts_display_blue
                .w5
                .saturating_add(stage_window_counts_display_blue.w5),
            miss: previous
                .window_counts_display_blue
                .miss
                .saturating_add(stage_window_counts_display_blue.miss),
        };
        carry[player] = CourseDisplayCarry {
            judgment_counts,
            scoring_counts,
            window_counts,
            window_counts_10ms_blue,
            window_counts_display_blue,
            holds_held_for_score: previous
                .holds_held_for_score
                .saturating_add(p.holds_held_for_score),
            holds_let_go_for_score: previous
                .holds_let_go_for_score
                .saturating_add(p.holds_let_go_for_score),
            rolls_held_for_score: previous
                .rolls_held_for_score
                .saturating_add(p.rolls_held_for_score),
            rolls_let_go_for_score: previous
                .rolls_let_go_for_score
                .saturating_add(p.rolls_let_go_for_score),
            mines_hit_for_score: previous
                .mines_hit_for_score
                .saturating_add(p.mines_hit_for_score),
        };
    }
    if state.num_players == 1 {
        carry[1] = carry[0];
    }
    carry
}

#[inline(always)]
pub fn display_carry_for_player(state: &State, player_idx: usize) -> CourseDisplayCarry {
    if player_idx >= MAX_PLAYERS {
        return CourseDisplayCarry::default();
    }
    state
        .course_display_carry
        .as_ref()
        .map_or(CourseDisplayCarry::default(), |carry| carry[player_idx])
}

#[inline(always)]
fn default_fa_plus_window_s(state: &State) -> f32 {
    state
        .timing_profile
        .fa_plus_window_s
        .unwrap_or(state.timing_profile.windows_s[0])
}

#[inline(always)]
fn profile_custom_window_ms(profile: &profile::Profile) -> f32 {
    let ms = profile.custom_fantastic_window_ms;
    f32::from(crate::game::profile::clamp_custom_fantastic_window_ms(ms))
}

#[inline(always)]
pub fn player_fa_plus_window_s(state: &State, player_idx: usize) -> f32 {
    let base = default_fa_plus_window_s(state);
    if player_idx >= state.num_players {
        return base;
    }
    let profile = &state.player_profiles[player_idx];
    if profile.custom_fantastic_window {
        profile_custom_window_ms(profile) / 1000.0
    } else {
        base
    }
}

#[inline(always)]
pub fn player_blue_window_ms(state: &State, player_idx: usize) -> f32 {
    if player_idx >= state.num_players {
        return default_fa_plus_window_s(state) * 1000.0;
    }
    let profile = &state.player_profiles[player_idx];
    if profile.custom_fantastic_window {
        return profile_custom_window_ms(profile);
    }
    if profile.fa_plus_10ms_blue_window {
        return 10.0;
    }
    default_fa_plus_window_s(state) * 1000.0
}

#[inline(always)]
fn add_window_counts(
    lhs: crate::game::timing::WindowCounts,
    rhs: crate::game::timing::WindowCounts,
) -> crate::game::timing::WindowCounts {
    crate::game::timing::WindowCounts {
        w0: lhs.w0.saturating_add(rhs.w0),
        w1: lhs.w1.saturating_add(rhs.w1),
        w2: lhs.w2.saturating_add(rhs.w2),
        w3: lhs.w3.saturating_add(rhs.w3),
        w4: lhs.w4.saturating_add(rhs.w4),
        w5: lhs.w5.saturating_add(rhs.w5),
        miss: lhs.miss.saturating_add(rhs.miss),
    }
}

#[inline(always)]
fn normalized_blue_window_ms(ms: f32) -> f32 {
    if ms.is_finite() && ms > 0.0 {
        ms
    } else {
        crate::game::timing::FA_PLUS_W010_MS
    }
}

#[inline(always)]
fn add_judgment_to_window_counts(
    counts: &mut crate::game::timing::WindowCounts,
    judgment: &Judgment,
    blue_window_ms: f32,
) {
    let split_ms = normalized_blue_window_ms(blue_window_ms);
    match judgment.grade {
        JudgeGrade::Fantastic => {
            if judgment.time_error_ms.abs() <= split_ms {
                counts.w0 = counts.w0.saturating_add(1);
            } else {
                counts.w1 = counts.w1.saturating_add(1);
            }
        }
        JudgeGrade::Excellent => counts.w2 = counts.w2.saturating_add(1),
        JudgeGrade::Great => counts.w3 = counts.w3.saturating_add(1),
        JudgeGrade::Decent => counts.w4 = counts.w4.saturating_add(1),
        JudgeGrade::WayOff => counts.w5 = counts.w5.saturating_add(1),
        JudgeGrade::Miss => counts.miss = counts.miss.saturating_add(1),
    }
}

#[inline(always)]
fn record_display_window_counts(state: &mut State, player_idx: usize, judgment: &Judgment) {
    if player_idx >= state.num_players || player_idx >= MAX_PLAYERS {
        return;
    }
    let display_window_ms = player_blue_window_ms(state, player_idx);
    add_judgment_to_window_counts(
        &mut state.live_window_counts[player_idx],
        judgment,
        crate::game::timing::FA_PLUS_W0_MS,
    );
    add_judgment_to_window_counts(
        &mut state.live_window_counts_10ms_blue[player_idx],
        judgment,
        crate::game::timing::FA_PLUS_W010_MS,
    );
    add_judgment_to_window_counts(
        &mut state.live_window_counts_display_blue[player_idx],
        judgment,
        display_window_ms,
    );
}

#[inline(always)]
fn float_match(a: f32, b: f32) -> bool {
    (a - b).abs() <= 0.000_1
}

#[inline(always)]
pub fn display_totals_for_player(state: &State, player_idx: usize) -> CourseDisplayTotals {
    if player_idx >= MAX_PLAYERS {
        return CourseDisplayTotals::default();
    }
    if let Some(totals) = state.course_display_totals.as_ref() {
        return totals[player_idx];
    }
    CourseDisplayTotals {
        possible_grade_points: state.possible_grade_points[player_idx],
        total_steps: state.total_steps[player_idx],
        holds_total: state.holds_total[player_idx],
        rolls_total: state.rolls_total[player_idx],
        mines_total: state.mines_total[player_idx],
    }
}

pub fn display_judgment_count(state: &State, player_idx: usize, grade: JudgeGrade) -> u32 {
    if player_idx >= state.num_players {
        return 0;
    }
    let base = state.players[player_idx].judgment_counts[display_judge_ix(grade)];
    let carry = display_carry_for_player(state, player_idx);
    base.saturating_add(carry.judgment_counts[display_judge_ix(grade)])
}

pub fn display_window_counts(
    state: &State,
    player_idx: usize,
    blue_window_ms: Option<f32>,
) -> crate::game::timing::WindowCounts {
    if player_idx >= state.num_players {
        return crate::game::timing::WindowCounts::default();
    }
    let current = if let Some(ms) = blue_window_ms {
        let split_ms = normalized_blue_window_ms(ms);
        let display_split_ms = normalized_blue_window_ms(player_blue_window_ms(state, player_idx));
        if float_match(split_ms, crate::game::timing::FA_PLUS_W0_MS) {
            state.live_window_counts[player_idx]
        } else if float_match(split_ms, crate::game::timing::FA_PLUS_W010_MS) {
            state.live_window_counts_10ms_blue[player_idx]
        } else if float_match(split_ms, display_split_ms) {
            state.live_window_counts_display_blue[player_idx]
        } else {
            let (start, end) = state.note_ranges[player_idx];
            crate::game::timing::compute_window_counts_blue_ms(&state.notes[start..end], split_ms)
        }
    } else {
        state.live_window_counts[player_idx]
    };
    let carry = display_carry_for_player(state, player_idx);
    let carry_counts = if let Some(ms) = blue_window_ms {
        let split_ms = normalized_blue_window_ms(ms);
        if float_match(split_ms, crate::game::timing::FA_PLUS_W0_MS) {
            carry.window_counts
        } else if float_match(split_ms, crate::game::timing::FA_PLUS_W010_MS) {
            carry.window_counts_10ms_blue
        } else {
            carry.window_counts_display_blue
        }
    } else {
        carry.window_counts
    };
    add_window_counts(current, carry_counts)
}

#[inline(always)]
fn display_window_counts_10ms(
    state: &State,
    player_idx: usize,
) -> crate::game::timing::WindowCounts {
    if player_idx >= state.num_players {
        return crate::game::timing::WindowCounts::default();
    }
    let current = state.live_window_counts_10ms_blue[player_idx];
    let carry = display_carry_for_player(state, player_idx);
    add_window_counts(current, carry.window_counts_10ms_blue)
}

pub fn display_itg_score_percent(state: &State, player_idx: usize) -> f64 {
    if player_idx >= state.num_players {
        return 0.0;
    }
    let carry = display_carry_for_player(state, player_idx);
    let mut scoring_counts = state.players[player_idx].scoring_counts;
    for (ix, total) in scoring_counts.iter_mut().enumerate() {
        *total = total.saturating_add(carry.scoring_counts[ix]);
    }
    let holds = state.players[player_idx]
        .holds_held_for_score
        .saturating_add(carry.holds_held_for_score);
    let rolls = state.players[player_idx]
        .rolls_held_for_score
        .saturating_add(carry.rolls_held_for_score);
    let mines = state.players[player_idx]
        .mines_hit_for_score
        .saturating_add(carry.mines_hit_for_score);
    let possible = display_totals_for_player(state, player_idx).possible_grade_points;
    judgment::calculate_itg_score_percent_from_counts(
        &scoring_counts,
        holds,
        rolls,
        mines,
        possible,
    )
}

#[inline(always)]
fn scored_hold_totals_with_carry(
    held: u32,
    let_go: u32,
    carry_held: u32,
    carry_let_go: u32,
) -> (u32, u32) {
    let held_total = held.saturating_add(carry_held);
    let resolved_total = held_total
        .saturating_add(let_go)
        .saturating_add(carry_let_go);
    (held_total, resolved_total)
}

pub(crate) fn display_ex_score_data(state: &State, player_idx: usize) -> judgment::ExScoreData {
    if player_idx >= state.num_players {
        return judgment::ExScoreData::default();
    }
    let player = &state.players[player_idx];
    let carry = display_carry_for_player(state, player_idx);
    let totals = display_totals_for_player(state, player_idx);
    let (holds_held, holds_resolved) = scored_hold_totals_with_carry(
        player.holds_held_for_score,
        player.holds_let_go_for_score,
        carry.holds_held_for_score,
        carry.holds_let_go_for_score,
    );
    let (rolls_held, rolls_resolved) = scored_hold_totals_with_carry(
        player.rolls_held_for_score,
        player.rolls_let_go_for_score,
        carry.rolls_held_for_score,
        carry.rolls_let_go_for_score,
    );
    judgment::ExScoreData {
        counts: display_window_counts(state, player_idx, None),
        counts_10ms: display_window_counts_10ms(state, player_idx),
        holds_held,
        holds_resolved,
        rolls_held,
        rolls_resolved,
        mines_hit: player
            .mines_hit_for_score
            .saturating_add(carry.mines_hit_for_score),
        total_steps: totals.total_steps,
        holds_total: totals.holds_total,
        rolls_total: totals.rolls_total,
        mines_total: totals.mines_total,
    }
}

pub fn display_ex_score_percent(state: &State, player_idx: usize) -> f64 {
    judgment::ex_score_percent(&display_ex_score_data(state, player_idx))
}

pub fn display_hard_ex_score_percent(state: &State, player_idx: usize) -> f64 {
    judgment::hard_ex_score_percent(&display_ex_score_data(state, player_idx))
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
    let spawn_window = state.noteskin[player].as_ref().and_then(|ns| {
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

fn handle_mine_hit(
    state: &mut State,
    column: usize,
    arrow_list_index: usize,
    note_index: usize,
    time_error: f32,
) -> bool {
    let player = player_for_col(state, column);
    let rate = if state.music_rate.is_finite() && state.music_rate > 0.0 {
        state.music_rate
    } else {
        1.0
    };
    let abs_time_error = (time_error / rate).abs();
    let mine_window = state.timing_profile.mine_window_s;
    if abs_time_error > mine_window {
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
    if !scoring_blocked {
        state.players[player].mines_hit = state.players[player].mines_hit.saturating_add(1);
    }
    let mut updated_scoring = false;

    state.arrows[column].remove(arrow_list_index);
    if !scoring_blocked {
        apply_life_change(
            &mut state.players[player],
            state.current_music_time,
            LIFE_HIT_MINE,
        );
        if !is_state_dead(state, player) {
            state.players[player].mines_hit_for_score =
                state.players[player].mines_hit_for_score.saturating_add(1);
            updated_scoring = true;
        }
        state.players[player].combo = 0;
        state.players[player].miss_combo = state.players[player].miss_combo.saturating_add(1);
        if state.players[player].full_combo_grade.is_some() {
            state.players[player].first_fc_attempt_broken = true;
        }
        state.players[player].full_combo_grade = None;
        state.players[player].current_combo_grade = None;
    }
    state.receptor_glow_timers[column] = 0.0;
    trigger_mine_explosion(state, column);
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
    if updated_scoring {
        update_itg_grade_totals(&mut state.players[player]);
    }
    true
}

#[inline(always)]
fn try_hit_mine_while_held(state: &mut State, column: usize, current_time: f32) -> bool {
    let mine_window = state.timing_profile.mine_window_s;
    let rate = if state.music_rate.is_finite() && state.music_rate > 0.0 {
        state.music_rate
    } else {
        1.0
    };
    let search_radius = mine_window * rate;
    let start_t = current_time - search_radius;
    let end_t = current_time + search_radius;
    let player = player_for_col(state, column);
    let (note_start, note_end) = player_note_range(state, player);
    let times = &state.note_time_cache[note_start..note_end];
    let start_idx = times.partition_point(|&t| t < start_t);
    let end_idx = times.partition_point(|&t| t <= end_t);
    let mut best: Option<(usize, f32)> = None;
    for i in start_idx..end_idx {
        let idx = note_start + i;
        let note = &state.notes[idx];
        if note.column != column {
            continue;
        }
        if !matches!(note.note_type, NoteType::Mine) {
            continue;
        }
        if !note.can_be_judged {
            continue;
        }
        if note.mine_result.is_some() {
            continue;
        }
        let note_time = times[i];
        let time_error = current_time - note_time;
        let abs_err = (time_error / rate).abs();
        if abs_err <= mine_window {
            match best {
                Some((_, best_err)) if abs_err >= best_err => {}
                _ => best = Some((idx, time_error)),
            }
        }
    }
    let Some((note_index, time_error)) = best else {
        return false;
    };
    if let Some(arrow_idx) = state.arrows[column]
        .iter()
        .position(|a| a.note_index == note_index)
    {
        handle_mine_hit(state, column, arrow_idx, note_index, time_error)
    } else {
        hit_mine_timebased(state, column, note_index, time_error)
    }
}

#[inline(always)]
fn try_hit_crossed_mines_while_held(
    state: &mut State,
    column: usize,
    prev_time: f32,
    current_time: f32,
) -> bool {
    if !prev_time.is_finite() || !current_time.is_finite() || current_time <= prev_time {
        return false;
    }
    let mine_window = state.timing_profile.mine_window_s;
    let rate = if state.music_rate.is_finite() && state.music_rate > 0.0 {
        state.music_rate
    } else {
        1.0
    };
    let player = player_for_col(state, column);
    let (note_start, note_end) = player_note_range(state, player);
    // ITG checks held mines as rows are crossed. Match that by only considering
    // mines whose note time crossed between previous and current music time.
    let (start_idx, end_idx) = {
        let times = &state.note_time_cache[note_start..note_end];
        (
            times.partition_point(|&t| t <= prev_time),
            times.partition_point(|&t| t <= current_time),
        )
    };
    let mut hit_any = false;
    for i in start_idx..end_idx {
        let note_index = note_start + i;
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
        let note_time = state.note_time_cache[note_index];
        let time_error = current_time - note_time;
        let abs_err = (time_error / rate).abs();
        if abs_err > mine_window {
            continue;
        }
        if hit_mine_timebased(state, column, note_index, time_error) {
            hit_any = true;
        }
    }
    hit_any
}

#[inline(always)]
fn hit_mine_timebased(
    state: &mut State,
    column: usize,
    note_index: usize,
    time_error: f32,
) -> bool {
    let player = player_for_col(state, column);
    let rate = if state.music_rate.is_finite() && state.music_rate > 0.0 {
        state.music_rate
    } else {
        1.0
    };
    let abs_time_error = (time_error / rate).abs();
    let mine_window = state.timing_profile.mine_window_s;
    if abs_time_error > mine_window {
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
    if !scoring_blocked {
        state.players[player].mines_hit = state.players[player].mines_hit.saturating_add(1);
    }
    let mut updated_scoring = false;
    if let Some(pos) = state.arrows[column]
        .iter()
        .position(|a| a.note_index == note_index)
    {
        state.arrows[column].remove(pos);
    }
    if !scoring_blocked {
        apply_life_change(
            &mut state.players[player],
            state.current_music_time,
            LIFE_HIT_MINE,
        );
        if !is_state_dead(state, player) {
            state.players[player].mines_hit_for_score =
                state.players[player].mines_hit_for_score.saturating_add(1);
            updated_scoring = true;
        }
        state.players[player].combo = 0;
        state.players[player].miss_combo = state.players[player].miss_combo.saturating_add(1);
        if state.players[player].full_combo_grade.is_some() {
            state.players[player].first_fc_attempt_broken = true;
        }
        state.players[player].full_combo_grade = None;
        state.players[player].current_combo_grade = None;
    }
    state.receptor_glow_timers[column] = 0.0;
    trigger_mine_explosion(state, column);
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
    if updated_scoring {
        update_itg_grade_totals(&mut state.players[player]);
    }
    true
}

fn handle_hold_let_go(state: &mut State, column: usize, note_index: usize) {
    let player = player_for_col(state, column);
    let scoring_blocked = autoplay_blocks_scoring(state);
    let mut updated_possible_scoring = false;
    if let Some(hold) = state.notes[note_index].hold.as_mut() {
        if hold.result == Some(HoldResult::LetGo) {
            return;
        }
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
    if !scoring_blocked && !is_state_dead(state, player) {
        match state.notes[note_index].note_type {
            NoteType::Hold => {
                state.players[player].holds_let_go_for_score = state.players[player]
                    .holds_let_go_for_score
                    .saturating_add(1);
                updated_possible_scoring = true;
            }
            NoteType::Roll => {
                state.players[player].rolls_let_go_for_score = state.players[player]
                    .rolls_let_go_for_score
                    .saturating_add(1);
                updated_possible_scoring = true;
            }
            _ => {}
        }
    }
    if state.players[player].hands_holding_count_for_stats > 0 {
        state.players[player].hands_holding_count_for_stats -= 1;
    }
    state.hold_judgments[column] = Some(HoldJudgmentRenderInfo {
        result: HoldResult::LetGo,
        triggered_at: Instant::now(),
    });
    if !scoring_blocked {
        apply_life_change(
            &mut state.players[player],
            state.current_music_time,
            LIFE_LET_GO,
        );
    }
    if updated_possible_scoring && !is_state_dead(state, player) {
        update_itg_grade_totals(&mut state.players[player]);
    }
    if !scoring_blocked {
        state.players[player].combo = 0;
        state.players[player].miss_combo = state.players[player].miss_combo.saturating_add(1);
        if state.players[player].full_combo_grade.is_some() {
            state.players[player].first_fc_attempt_broken = true;
        }
        state.players[player].full_combo_grade = None;
        state.players[player].current_combo_grade = None;
    }
    state.receptor_glow_timers[column] = 0.0;
}

fn handle_hold_success(state: &mut State, column: usize, note_index: usize) {
    let player = player_for_col(state, column);
    let scoring_blocked = autoplay_blocks_scoring(state);
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
    if note_index < state.hold_decay_active.len() && state.hold_decay_active[note_index] {
        state.hold_decay_active[note_index] = false;
    }
    if state.players[player].hands_holding_count_for_stats > 0 {
        state.players[player].hands_holding_count_for_stats -= 1;
    }
    let mut updated_scoring = false;
    match state.notes[note_index].note_type {
        NoteType::Hold => {
            if !scoring_blocked {
                state.players[player].holds_held =
                    state.players[player].holds_held.saturating_add(1);
            }
            if !scoring_blocked && !is_state_dead(state, player) {
                state.players[player].holds_held_for_score =
                    state.players[player].holds_held_for_score.saturating_add(1);
                updated_scoring = true;
            }
        }
        NoteType::Roll => {
            if !scoring_blocked {
                state.players[player].rolls_held =
                    state.players[player].rolls_held.saturating_add(1);
            }
            if !scoring_blocked && !is_state_dead(state, player) {
                state.players[player].rolls_held_for_score =
                    state.players[player].rolls_held_for_score.saturating_add(1);
                updated_scoring = true;
            }
        }
        _ => {}
    }
    if !scoring_blocked {
        apply_life_change(
            &mut state.players[player],
            state.current_music_time,
            LIFE_HELD,
        );
    }
    if updated_scoring {
        update_itg_grade_totals(&mut state.players[player]);
    }
    if !scoring_blocked {
        state.players[player].miss_combo = 0;
    }
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

fn update_active_holds(
    state: &mut State,
    inputs: &[bool; MAX_COLS],
    current_time: f32,
    delta_time: f32,
) {
    for column in 0..state.active_holds.len() {
        let player = player_for_col(state, column);
        let timing = &state.timing_players[player];
        let current_beat = timing.get_beat_for_time(current_time);
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
                let pressed = inputs[column];
                active.is_pressed = pressed;

                if !active.let_go && active.life > 0.0 {
                    let prev_row = hold.last_held_row_index;
                    let prev_beat = hold.last_held_beat;
                    if pressed {
                        let mut current_row = timing
                            .get_row_for_beat(current_beat)
                            .unwrap_or(note_start_row);
                        current_row = current_row.clamp(note_start_row, hold.end_row_index);
                        let final_row = prev_row.max(current_row);
                        if final_row == prev_row {
                            hold.last_held_beat = prev_beat.clamp(note_start_beat, hold.end_beat);
                        } else {
                            hold.last_held_row_index = final_row;
                            let mut new_beat =
                                timing.get_beat_for_row(final_row).unwrap_or(current_beat);
                            new_beat = new_beat.clamp(note_start_beat, hold.end_beat);
                            if new_beat < prev_beat {
                                new_beat = prev_beat;
                            }
                            hold.last_held_beat = new_beat;
                        }
                    } else {
                        hold.last_held_beat = prev_beat.clamp(note_start_beat, hold.end_beat);
                    }
                }

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

pub fn judge_a_tap(state: &mut State, column: usize, current_time: f32) -> bool {
    let windows = state.timing_profile.windows_s;
    let way_off_window = windows[4];
    let mine_window = state.timing_profile.mine_window_s;
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
    let (col_start, col_end) = player_col_range(state, player);
    let way_off_window_music = way_off_window * rate;
    let mine_window_music = mine_window * rate;
    let search_window_music = way_off_window_music.max(mine_window_music);
    let search_start_time = current_time - search_window_music;
    let search_end_time = current_time + search_window_music;
    let mut best: Option<(usize, usize, f32)> = None;
    for (idx, arrow) in state.arrows[column].iter().enumerate() {
        let note_index = arrow.note_index;
        let n = &state.notes[note_index];
        if n.result.is_some() || !n.can_be_judged || n.is_fake || n.note_type == NoteType::Lift {
            continue;
        }
        let note_time = state.note_time_cache[note_index];
        if note_time < search_start_time {
            continue;
        }
        if note_time > search_end_time {
            // Arrows are emitted in chart order, so later entries are even farther ahead.
            break;
        }
        let abs_err_music = (current_time - note_time).abs();
        let window_music = if matches!(n.note_type, NoteType::Mine) {
            mine_window_music
        } else {
            way_off_window_music
        };
        if abs_err_music <= window_music {
            match best {
                Some((_, _, best_err)) if abs_err_music >= best_err => {}
                _ => best = Some((idx, note_index, abs_err_music)),
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
            if state.notes[note_index].is_fake {
                return false;
            }
            if handle_mine_hit(
                state,
                column,
                arrow_list_index,
                note_index,
                time_error_music,
            ) {
                return true;
            }
            return false;
        }
        let mine_hit_on_press = try_hit_mine_while_held(state, column, current_time);

        if abs_time_error <= way_off_window {
            let mut notes_on_row = [usize::MAX; MAX_COLS];
            let mut notes_on_row_len = 0usize;
            if let Some(&pos) = state
                .row_map_cache
                .get(note_row_index)
                .filter(|&&x| x != u32::MAX)
            {
                for &idx in &state.row_entries[pos as usize].nonmine_note_indices {
                    let col = state.notes[idx].column;
                    if col < col_start
                        || col >= col_end
                        || state.notes[idx].result.is_some()
                        || state.notes[idx].note_type == NoteType::Lift
                    {
                        continue;
                    }
                    if notes_on_row_len < MAX_COLS {
                        notes_on_row[notes_on_row_len] = idx;
                        notes_on_row_len += 1;
                    }
                }
            } else {
                for (idx, n) in state.notes.iter().enumerate() {
                    if n.row_index != note_row_index
                        || n.column < col_start
                        || n.column >= col_end
                        || matches!(n.note_type, NoteType::Mine | NoteType::Lift)
                        || n.is_fake
                        || n.result.is_some()
                    {
                        continue;
                    }
                    if notes_on_row_len < MAX_COLS {
                        notes_on_row[notes_on_row_len] = idx;
                        notes_on_row_len += 1;
                    }
                }
            }

            if notes_on_row_len == 0 {
                return false;
            }
            let all_pressed = notes_on_row[..notes_on_row_len].iter().all(|&idx| {
                let col = state.notes[idx].column;
                lane_is_pressed(state, col)
            });
            if !all_pressed {
                return false;
            }

            let mut timing_profile = state.timing_profile;
            timing_profile.fa_plus_window_s = Some(player_fa_plus_window_s(state, player));
            let (grade, window) = classify_offset_s(time_error_real, &timing_profile);
            let (song_offset_s, global_offset_s, lead_in_s, stream_pos_s) = if timing_hit_log {
                (
                    state.song_offset_seconds,
                    state.global_offset_seconds,
                    state.audio_lead_in_seconds.max(0.0),
                    audio::get_music_stream_position_seconds(),
                )
            } else {
                (0.0, 0.0, 0.0, 0.0)
            };

            if rescore_early_hits && notes_on_row_len == 1 {
                let idx = notes_on_row[0];
                let note_col = state.notes[idx].column;
                let row_note_time = state.note_time_cache[idx];
                let te_music = current_time - row_note_time;
                let te_real = te_music / rate;
                let is_early = te_real < 0.0;
                let is_bad = matches!(grade, JudgeGrade::Decent | JudgeGrade::WayOff);

                if is_early && is_bad {
                    if state.notes[idx].early_result.is_none() {
                        let judgment = Judgment {
                            time_error_ms: te_real * 1000.0,
                            grade,
                            window: Some(window),
                            miss_because_held: false,
                        };
                        state.notes[idx].early_result = Some(judgment.clone());
                        let life_delta = judge_life_delta(grade);
                        {
                            let p = &mut state.players[player];
                            if !scoring_blocked {
                                apply_life_change(p, state.current_music_time, life_delta);
                            }
                            if !hide_early_dw_judgments {
                                p.last_judgment = Some(JudgmentRenderInfo {
                                    judgment: judgment.clone(),
                                    judged_at: Instant::now(),
                                });
                            }
                        }
                        // Zmod parity: provisional early W4/W5 (with Rescore Early Hits enabled)
                        // should not add error-bar ticks before the final row judgment is known.
                        log_timing_hit_detail(
                            timing_hit_log,
                            stream_pos_s,
                            grade,
                            note_row_index,
                            note_col,
                            state.notes[idx].beat,
                            song_offset_s,
                            global_offset_s,
                            row_note_time,
                            current_time,
                            state.current_music_time,
                            rate,
                            lead_in_s,
                        );

                        if !hide_early_dw_flash {
                            trigger_receptor_glow_pulse(state, note_col);
                            trigger_tap_explosion(state, note_col, grade);
                        }

                        if let Some(end_time) = state.hold_end_time_cache[idx]
                            && matches!(state.notes[idx].note_type, NoteType::Hold | NoteType::Roll)
                        {
                            if let Some(hold) = state.notes[idx].hold.as_mut() {
                                hold.life = MAX_HOLD_LIFE;
                            }
                            state.active_holds[note_col] = Some(ActiveHold {
                                note_index: idx,
                                end_time,
                                note_type: state.notes[idx].note_type,
                                let_go: false,
                                is_pressed: true,
                                life: MAX_HOLD_LIFE,
                            });
                        }
                    }
                    return true;
                }

                if state.notes[idx].early_result.is_some()
                    && !matches!(
                        grade,
                        JudgeGrade::Fantastic | JudgeGrade::Excellent | JudgeGrade::Great
                    )
                {
                    return true;
                }

                let judgment = Judgment {
                    time_error_ms: te_real * 1000.0,
                    grade,
                    window: Some(window),
                    miss_because_held: false,
                };
                error_bar_register_tap(state, player, &judgment, current_time);
                state.notes[idx].result = Some(judgment);

                log_timing_hit_detail(
                    timing_hit_log,
                    stream_pos_s,
                    grade,
                    note_row_index,
                    note_col,
                    state.notes[idx].beat,
                    song_offset_s,
                    global_offset_s,
                    row_note_time,
                    current_time,
                    state.current_music_time,
                    rate,
                    lead_in_s,
                );

                let col_arrows = &mut state.arrows[note_col];
                if let Some(pos) = col_arrows.iter().position(|a| a.note_index == idx) {
                    col_arrows.remove(pos);
                }
                trigger_receptor_glow_pulse(state, note_col);
                trigger_tap_explosion(state, note_col, grade);
                if let Some(end_time) = state.hold_end_time_cache[idx]
                    && matches!(state.notes[idx].note_type, NoteType::Hold | NoteType::Roll)
                {
                    if let Some(hold) = state.notes[idx].hold.as_mut() {
                        hold.life = MAX_HOLD_LIFE;
                    }
                    state.active_holds[note_col] = Some(ActiveHold {
                        note_index: idx,
                        end_time,
                        note_type: state.notes[idx].note_type,
                        let_go: false,
                        is_pressed: true,
                        life: MAX_HOLD_LIFE,
                    });
                }
                return true;
            }

            for &idx in &notes_on_row[..notes_on_row_len] {
                let note_col = state.notes[idx].column;
                let row_note_time = state.note_time_cache[idx];
                let te_music = current_time - row_note_time;
                let te_real = te_music / rate;
                let judgment = Judgment {
                    time_error_ms: te_real * 1000.0,
                    grade,
                    window: Some(window),
                    miss_because_held: false,
                };
                error_bar_register_tap(state, player, &judgment, current_time);
                state.notes[idx].result = Some(judgment);

                log_timing_hit_detail(
                    timing_hit_log,
                    stream_pos_s,
                    grade,
                    note_row_index,
                    note_col,
                    state.notes[idx].beat,
                    song_offset_s,
                    global_offset_s,
                    row_note_time,
                    current_time,
                    state.current_music_time,
                    rate,
                    lead_in_s,
                );

                let col_arrows = &mut state.arrows[note_col];
                if let Some(pos) = col_arrows.iter().position(|a| a.note_index == idx) {
                    col_arrows.remove(pos);
                }
                trigger_receptor_glow_pulse(state, note_col);
                trigger_tap_explosion(state, note_col, grade);
                if let Some(end_time) = state.hold_end_time_cache[idx]
                    && matches!(state.notes[idx].note_type, NoteType::Hold | NoteType::Roll)
                {
                    if let Some(hold) = state.notes[idx].hold.as_mut() {
                        hold.life = MAX_HOLD_LIFE;
                    }
                    state.active_holds[note_col] = Some(ActiveHold {
                        note_index: idx,
                        end_time,
                        note_type: state.notes[idx].note_type,
                        let_go: false,
                        is_pressed: true,
                        life: MAX_HOLD_LIFE,
                    });
                }
            }
            return true;
        }
        return mine_hit_on_press;
    }
    try_hit_mine_while_held(state, column, current_time)
}

/// Judge lift notes on button release. Mirrors judge_a_tap but only matches
/// NoteType::Lift and judges a single note (no row-wide all-pressed check).
pub fn judge_a_lift(state: &mut State, column: usize, current_time: f32) -> bool {
    let windows = state.timing_profile.windows_s;
    let way_off_window = windows[4];
    let rate = if state.music_rate.is_finite() && state.music_rate > 0.0 {
        state.music_rate
    } else {
        1.0
    };
    let timing_hit_log = timing_hit_log_enabled();
    let player = player_for_col(state, column);
    let scoring_blocked = autoplay_blocks_scoring(state);
    let way_off_window_music = way_off_window * rate;
    let search_start_time = current_time - way_off_window_music;
    let search_end_time = current_time + way_off_window_music;

    let mut best: Option<(usize, usize, f32)> = None;
    for (idx, arrow) in state.arrows[column].iter().enumerate() {
        let note_index = arrow.note_index;
        let n = &state.notes[note_index];
        if n.result.is_some() || !n.can_be_judged || n.is_fake || n.note_type != NoteType::Lift {
            continue;
        }
        let note_time = state.note_time_cache[note_index];
        if note_time < search_start_time {
            continue;
        }
        if note_time > search_end_time {
            break;
        }
        let abs_err_music = (current_time - note_time).abs();
        if abs_err_music <= way_off_window_music {
            match best {
                Some((_, _, best_err)) if abs_err_music >= best_err => {}
                _ => best = Some((idx, note_index, abs_err_music)),
            }
        }
    }

    let Some((_arrow_list_index, note_index, _)) = best else {
        return false;
    };

    let note_time = state.note_time_cache[note_index];
    let time_error_music = current_time - note_time;
    let time_error_real = time_error_music / rate;
    let abs_time_error = time_error_real.abs();
    if abs_time_error > way_off_window {
        return false;
    }

    let mut timing_profile = state.timing_profile;
    timing_profile.fa_plus_window_s = Some(player_fa_plus_window_s(state, player));
    let (grade, window) = classify_offset_s(time_error_real, &timing_profile);
    let (song_offset_s, global_offset_s, lead_in_s, stream_pos_s) = if timing_hit_log {
        (
            state.song_offset_seconds,
            state.global_offset_seconds,
            state.audio_lead_in_seconds.max(0.0),
            audio::get_music_stream_position_seconds(),
        )
    } else {
        (0.0, 0.0, 0.0, 0.0)
    };

    let note_col = state.notes[note_index].column;
    let note_row_index = state.notes[note_index].row_index;
    let note_beat = state.notes[note_index].beat;
    let judgment = Judgment {
        time_error_ms: time_error_real * 1000.0,
        grade,
        window: Some(window),
        miss_because_held: false,
    };
    if !scoring_blocked {
        error_bar_register_tap(state, player, &judgment, current_time);
    }
    state.notes[note_index].result = Some(judgment);

    log_timing_hit_detail(
        timing_hit_log,
        stream_pos_s,
        grade,
        note_row_index,
        note_col,
        note_beat,
        song_offset_s,
        global_offset_s,
        note_time,
        current_time,
        state.current_music_time,
        rate,
        lead_in_s,
    );

    let col_arrows = &mut state.arrows[note_col];
    if let Some(pos) = col_arrows.iter().position(|a| a.note_index == note_index) {
        col_arrows.remove(pos);
    }
    trigger_receptor_glow_pulse(state, note_col);
    trigger_tap_explosion(state, note_col, grade);
    true
}

pub fn handle_input(state: &mut State, ev: &InputEvent) -> ScreenAction {
    if state.exit_transition.is_some() {
        return ScreenAction::None;
    }
    if let Some(lane) = lane_from_action(ev.action) {
        queue_input_edge(
            state,
            ev.source,
            lane,
            ev.pressed,
            ev.timestamp,
            ev.timestamp_host_nanos,
            ev.stored_at,
            ev.emitted_at,
        );
        abort_hold_to_exit(state, ev.timestamp);
        return ScreenAction::None;
    }
    let is_p2_single = profile::get_session_play_style() == profile::PlayStyle::Single
        && profile::get_session_player_side() == profile::PlayerSide::P2;
    match ev.action {
        VirtualAction::p1_start if !is_p2_single => {
            if ev.pressed {
                state.hold_to_exit_key = Some(HoldToExitKey::Start);
                state.hold_to_exit_start = Some(ev.timestamp);
                state.hold_to_exit_aborted_at = None;
            } else if state.hold_to_exit_key == Some(HoldToExitKey::Start) {
                abort_hold_to_exit(state, ev.timestamp);
            }
        }
        VirtualAction::p2_start if is_p2_single => {
            if ev.pressed {
                state.hold_to_exit_key = Some(HoldToExitKey::Start);
                state.hold_to_exit_start = Some(ev.timestamp);
                state.hold_to_exit_aborted_at = None;
            } else if state.hold_to_exit_key == Some(HoldToExitKey::Start) {
                abort_hold_to_exit(state, ev.timestamp);
            }
        }
        VirtualAction::p1_back if !is_p2_single => {
            if ev.pressed {
                state.hold_to_exit_key = Some(HoldToExitKey::Back);
                state.hold_to_exit_start = Some(ev.timestamp);
                state.hold_to_exit_aborted_at = None;
            } else if state.hold_to_exit_key == Some(HoldToExitKey::Back) {
                abort_hold_to_exit(state, ev.timestamp);
            }
        }
        VirtualAction::p2_back if is_p2_single => {
            if ev.pressed {
                state.hold_to_exit_key = Some(HoldToExitKey::Back);
                state.hold_to_exit_start = Some(ev.timestamp);
                state.hold_to_exit_aborted_at = None;
            } else if state.hold_to_exit_key == Some(HoldToExitKey::Back) {
                abort_hold_to_exit(state, ev.timestamp);
            }
        }
        _ => {}
    }
    ScreenAction::None
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
const fn next_tick_mode(mode: TickMode) -> TickMode {
    match mode {
        TickMode::Off => TickMode::Assist,
        TickMode::Assist => TickMode::Hit,
        TickMode::Hit => TickMode::Off,
    }
}

#[inline(always)]
const fn tick_mode_status_line(mode: TickMode) -> Option<&'static str> {
    match mode {
        TickMode::Off => None,
        TickMode::Assist => Some("Assist Tick"),
        TickMode::Hit => Some("Hit Tick"),
    }
}

#[inline(always)]
const fn tick_mode_debug_label(mode: TickMode) -> &'static str {
    match mode {
        TickMode::Off => "off",
        TickMode::Assist => "assist tick",
        TickMode::Hit => "hit tick",
    }
}

fn set_tick_mode(state: &mut State, mode: TickMode, now_music_time: f32) {
    if state.tick_mode == mode {
        return;
    }
    state.tick_mode = mode;
    profile::set_session_timing_tick_mode(mode);

    let song_row = assist_row_no_offset(state, now_music_time);
    state.assist_last_crossed_row = song_row;
    state.assist_clap_cursor = assist_clap_cursor_for_row(&state.assist_clap_rows, song_row);

    debug!("Timing ticks set to {} (F7).", tick_mode_debug_label(mode));
}

fn set_autoplay_enabled(state: &mut State, enabled: bool, now_music_time: f32) {
    if state.autoplay_enabled == enabled {
        return;
    }
    state.autoplay_enabled = enabled;

    if enabled {
        state.keyboard_lane_counts = [0; MAX_COLS];
        state.gamepad_lane_counts = [0; MAX_COLS];
        state.prev_inputs = [false; MAX_COLS];
        state.receptor_glow_timers = [0.0; MAX_COLS];
        state.receptor_glow_press_timers = [0.0; MAX_COLS];
        state.receptor_glow_lift_start_alpha = [0.0; MAX_COLS];
        state.receptor_glow_lift_start_zoom = [1.0; MAX_COLS];
        state.pending_edges.clear();
        state.autoplay_lane_state = [false; MAX_COLS];
        state.autoplay_hold_release_time = [None; MAX_COLS];
        state.autoplay_pending_row = [None; MAX_PLAYERS];
        for player in 0..state.num_players {
            let (note_start, note_end) = player_note_range(state, player);
            state.autoplay_cursor[player] = state.next_tap_miss_cursor[player]
                .max(note_start)
                .min(note_end);
        }
        debug!("Autoplay enabled (F8). Scores for this stage will not be saved.");
        return;
    }

    debug!("Autoplay disabled (F8).");
    for col in 0..state.num_cols {
        if !state.autoplay_lane_state[col] {
            continue;
        }
        let Some(lane) = lane_from_column(col) else {
            continue;
        };
        push_input_edge(
            state,
            InputSource::Keyboard,
            lane,
            false,
            now_music_time,
            false,
        );
        state.autoplay_lane_state[col] = false;
    }
    for t in &mut state.autoplay_hold_release_time {
        *t = None;
    }
    state.autoplay_pending_row = [None; MAX_PLAYERS];
}

fn run_autoplay(state: &mut State, now_music_time: f32) {
    if !state.autoplay_enabled {
        return;
    }

    for player in 0..state.num_players {
        let (note_start, note_end) = player_note_range(state, player);
        let mut cursor = state.autoplay_cursor[player].max(note_start);
        while cursor < note_end {
            while cursor < note_end && state.notes[cursor].result.is_some() {
                cursor += 1;
                state.autoplay_pending_row[player] = None;
            }
            if cursor >= note_end {
                break;
            }

            let row = state.notes[cursor].row_index;
            let mut row_end = cursor + 1;
            while row_end < note_end && state.notes[row_end].row_index == row {
                row_end += 1;
            }
            let row_time = state.note_time_cache[cursor];
            let row_event_time = match state.autoplay_pending_row[player] {
                Some((pending_cursor, pending_time)) if pending_cursor == cursor => pending_time,
                _ => {
                    let sampled = row_time + autoplay_tap_offset_s(state);
                    state.autoplay_pending_row[player] = Some((cursor, sampled));
                    sampled
                }
            };
            if row_event_time > now_music_time {
                break;
            }

            let mut tap_releases: [Option<f32>; MAX_COLS] = [None; MAX_COLS];
            for idx in cursor..row_end {
                let (result_is_some, is_fake, can_be_judged, note_type, col) = {
                    let note = &state.notes[idx];
                    (
                        note.result.is_some(),
                        note.is_fake,
                        note.can_be_judged,
                        note.note_type,
                        note.column,
                    )
                };
                if result_is_some
                    || is_fake
                    || !can_be_judged
                    || matches!(note_type, NoteType::Mine)
                {
                    continue;
                }

                if col >= state.num_cols {
                    continue;
                }
                let Some(lane) = lane_from_column(col) else {
                    continue;
                };

                if !state.autoplay_lane_state[col] {
                    push_input_edge(
                        state,
                        InputSource::Keyboard,
                        lane,
                        true,
                        row_event_time,
                        false,
                    );
                    state.autoplay_lane_state[col] = true;
                }

                state.autoplay_used = true;
                match note_type {
                    NoteType::Hold => {
                        let end_time = state.hold_end_time_cache[idx].unwrap_or(row_time);
                        let release_at = end_time + AUTOPLAY_HOLD_RELEASE_SECONDS;
                        match state.autoplay_hold_release_time[col] {
                            Some(prev) => {
                                if release_at > prev {
                                    state.autoplay_hold_release_time[col] = Some(release_at);
                                }
                            }
                            None => state.autoplay_hold_release_time[col] = Some(release_at),
                        }
                    }
                    NoteType::Lift => {
                        tap_releases[col] = Some(row_event_time);
                    }
                    NoteType::Roll | NoteType::Tap => {
                        tap_releases[col] = Some(row_event_time + AUTOPLAY_TAP_RELEASE_SECONDS);
                    }
                    _ => {}
                }
            }

            for (col, release_at) in tap_releases.into_iter().enumerate() {
                if release_at.is_none() || !state.autoplay_lane_state[col] {
                    continue;
                }
                let Some(lane) = lane_from_column(col) else {
                    continue;
                };
                push_input_edge(
                    state,
                    InputSource::Keyboard,
                    lane,
                    false,
                    release_at.unwrap_or(row_event_time),
                    false,
                );
                state.autoplay_lane_state[col] = false;
            }

            state.autoplay_pending_row[player] = None;
            cursor = row_end;
        }
        state.autoplay_cursor[player] = cursor;
    }

    for col in 0..state.num_cols {
        let Some(release_at) = state.autoplay_hold_release_time[col] else {
            continue;
        };
        if now_music_time < release_at {
            continue;
        }
        if state.autoplay_lane_state[col]
            && let Some(lane) = lane_from_column(col)
        {
            push_input_edge(state, InputSource::Keyboard, lane, false, release_at, false);
        }
        state.autoplay_lane_state[col] = false;
        state.autoplay_hold_release_time[col] = None;
    }

    let mut roll_cols = [usize::MAX; MAX_COLS];
    let mut roll_count = 0usize;
    for col in 0..state.num_cols {
        if state.active_holds[col]
            .as_ref()
            .is_some_and(|active| matches!(active.note_type, NoteType::Roll) && !active.let_go)
            && roll_count < MAX_COLS
        {
            roll_cols[roll_count] = col;
            roll_count += 1;
        }
    }
    for col in roll_cols.into_iter().take(roll_count) {
        refresh_roll_life_on_step(state, col);
    }
}

fn run_replay(state: &mut State, now_music_time: f32) {
    if !state.autoplay_enabled || !state.replay_mode {
        return;
    }
    while state.replay_cursor < state.replay_input.len() {
        let edge = state.replay_input[state.replay_cursor];
        if edge.event_music_time > now_music_time {
            break;
        }
        state.replay_cursor += 1;
        let col = edge.lane_index as usize;
        if col >= state.num_cols {
            continue;
        }
        let Some(lane) = lane_from_column(col) else {
            continue;
        };
        push_input_edge(
            state,
            edge.source,
            lane,
            edge.pressed,
            edge.event_music_time,
            false,
        );
        state.autoplay_used = true;
    }
}

#[inline(always)]
fn mutate_timing_arc(timing: &mut Arc<TimingData>, mut apply: impl FnMut(&mut TimingData)) {
    if let Some(inner) = Arc::get_mut(timing) {
        apply(inner);
        return;
    }
    let mut cloned = (**timing).clone();
    apply(&mut cloned);
    *timing = Arc::new(cloned);
}

#[inline(always)]
fn refresh_timing_after_offset_change(state: &mut State) {
    let num_players = state.num_players;
    let cols_per_player = state.cols_per_player;
    for (time, note) in state.note_time_cache.iter_mut().zip(&state.notes) {
        let player = if num_players <= 1 || cols_per_player == 0 {
            0
        } else {
            (note.column / cols_per_player).min(num_players.saturating_sub(1))
        };
        *time = state.timing_players[player].get_time_for_beat(note.beat);
    }
    for (time_opt, note) in state.hold_end_time_cache.iter_mut().zip(&state.notes) {
        let player = if num_players <= 1 || cols_per_player == 0 {
            0
        } else {
            (note.column / cols_per_player).min(num_players.saturating_sub(1))
        };
        *time_opt = note
            .hold
            .as_ref()
            .map(|h| state.timing_players[player].get_time_for_beat(h.end_beat));
    }
    state.beat_info_cache.reset(&state.timing);

    let (notes_end_time, music_end_time) = compute_end_times(
        &state.notes,
        &state.note_time_cache,
        &state.hold_end_time_cache,
        state.music_rate,
    );
    state.notes_end_time = notes_end_time;
    state.music_end_time = music_end_time;
}

#[inline(always)]
fn quantized_offset_change_line(label: &str, start: f32, new: f32) -> Option<String> {
    let start_q = quantize_offset_seconds(start);
    let new_q = quantize_offset_seconds(new);
    let delta_q = new_q - start_q;
    if delta_q.abs() < 0.000_1_f32 {
        return None;
    }
    let direction = if delta_q > 0.0 { "earlier" } else { "later" };
    Some(format!(
        "{label} from {start_q:+.3} to {new_q:+.3} (notes {direction})"
    ))
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
fn cycle_autosync_mode(state: &mut State) {
    let mut next = match state.autosync_mode {
        AutosyncMode::Off => AutosyncMode::Song,
        AutosyncMode::Song => AutosyncMode::Machine,
        AutosyncMode::Machine => AutosyncMode::Off,
    };
    if state.course_display_totals.is_some() && next == AutosyncMode::Song {
        next = AutosyncMode::Machine;
    }
    state.autosync_mode = next;
}

#[inline(always)]
fn autosync_mean(samples: &[f32; AUTOSYNC_OFFSET_SAMPLE_COUNT]) -> f32 {
    let mut sum = 0.0_f32;
    for value in samples {
        sum += *value;
    }
    sum / AUTOSYNC_OFFSET_SAMPLE_COUNT as f32
}

#[inline(always)]
fn autosync_stddev(samples: &[f32; AUTOSYNC_OFFSET_SAMPLE_COUNT], mean: f32) -> f32 {
    let mut dev = 0.0_f32;
    for value in samples {
        let d = *value - mean;
        dev += d * d;
    }
    (dev / AUTOSYNC_OFFSET_SAMPLE_COUNT as f32).sqrt()
}

#[inline(always)]
fn apply_autosync_offset_correction(state: &mut State, note_off_by_seconds: f32) {
    if !note_off_by_seconds.is_finite() || state.autosync_mode == AutosyncMode::Off {
        return;
    }
    let sample_ix = state
        .autosync_offset_sample_count
        .min(AUTOSYNC_OFFSET_SAMPLE_COUNT.saturating_sub(1));
    state.autosync_offset_samples[sample_ix] = note_off_by_seconds;
    state.autosync_offset_sample_count = state.autosync_offset_sample_count.saturating_add(1);
    if state.autosync_offset_sample_count < AUTOSYNC_OFFSET_SAMPLE_COUNT {
        return;
    }

    let mean = autosync_mean(&state.autosync_offset_samples);
    let stddev = autosync_stddev(&state.autosync_offset_samples, mean);
    if stddev < AUTOSYNC_STDDEV_MAX_SECONDS {
        match state.autosync_mode {
            AutosyncMode::Off => {}
            AutosyncMode::Song => {
                if state.course_display_totals.is_none() {
                    let _ = apply_song_offset_delta(state, mean);
                }
            }
            AutosyncMode::Machine => {
                let _ = apply_global_offset_delta(state, mean);
            }
        }
    }

    state.autosync_standard_deviation = stddev;
    state.autosync_offset_sample_count = 0;
}

#[inline(always)]
fn apply_autosync_for_row_hits(
    state: &mut State,
    row_entry_index: usize,
    col_start: usize,
    col_end: usize,
) {
    if state.replay_mode
        || autoplay_blocks_scoring(state)
        || state.autosync_mode == AutosyncMode::Off
    {
        return;
    }
    // ITG parity: AdjustSync::HandleAutosync() is disabled in course mode.
    if state.course_display_totals.is_some() {
        return;
    }

    let row_len = state.row_entries[row_entry_index]
        .nonmine_note_indices
        .len();
    let mut i = 0;
    while i < row_len {
        let note_index = state.row_entries[row_entry_index].nonmine_note_indices[i];
        let maybe_note_offset = {
            let note = &state.notes[note_index];
            if note.column < col_start || note.column >= col_end {
                None
            } else {
                note.result.as_ref().and_then(|judgment| {
                    if matches!(
                        judgment.grade,
                        JudgeGrade::Fantastic | JudgeGrade::Excellent | JudgeGrade::Great
                    ) {
                        // ITG's fNoteOffset is positive when stepping early.
                        Some(-judgment.time_error_ms * 0.001)
                    } else {
                        None
                    }
                })
            }
        };
        if let Some(note_off_by_seconds) = maybe_note_offset {
            apply_autosync_offset_correction(state, note_off_by_seconds);
        }
        i += 1;
    }
}

#[inline(always)]
fn refresh_sync_overlay_message(state: &mut State) {
    let mut message = String::new();
    if let Some(global_line) = quantized_offset_change_line(
        "Global Offset",
        state.initial_global_offset_seconds,
        state.global_offset_seconds,
    ) {
        message.push_str(&global_line);
    }
    if let Some(song_line) = quantized_offset_change_line(
        "Song offset",
        state.initial_song_offset_seconds,
        state.song_offset_seconds,
    ) {
        if !message.is_empty() {
            message.push('\n');
        }
        message.push_str(&song_line);
    }
    if message.is_empty() {
        state.sync_overlay_message = None;
    } else {
        state.sync_overlay_message = Some(Arc::<str>::from(message));
    }
}

#[inline(always)]
fn offset_adjust_slot(code: KeyCode) -> Option<usize> {
    match code {
        KeyCode::F11 => Some(0),
        KeyCode::F12 => Some(1),
        _ => None,
    }
}

#[inline(always)]
fn offset_adjust_delta(code: KeyCode) -> Option<f32> {
    match code {
        KeyCode::F11 => Some(-OFFSET_ADJUST_STEP_SECONDS),
        KeyCode::F12 => Some(OFFSET_ADJUST_STEP_SECONDS),
        _ => return None,
    }
}

#[inline(always)]
fn clear_offset_adjust_hold(state: &mut State, code: KeyCode) -> bool {
    let Some(slot) = offset_adjust_slot(code) else {
        return false;
    };
    state.offset_adjust_held_since[slot] = None;
    state.offset_adjust_last_at[slot] = None;
    true
}

#[inline(always)]
fn start_offset_adjust_hold(state: &mut State, code: KeyCode, at: Instant) -> Option<f32> {
    let slot = offset_adjust_slot(code)?;
    state.offset_adjust_held_since[slot] = Some(at);
    state.offset_adjust_last_at[slot] = Some(at);
    offset_adjust_delta(code)
}

#[inline(always)]
fn update_offset_adjust_hold(state: &mut State) {
    let now = Instant::now();
    for code in [KeyCode::F11, KeyCode::F12] {
        let Some(slot) = offset_adjust_slot(code) else {
            continue;
        };
        let (Some(held_since), Some(last_at)) = (
            state.offset_adjust_held_since[slot],
            state.offset_adjust_last_at[slot],
        ) else {
            continue;
        };
        if now.duration_since(held_since) < OFFSET_ADJUST_REPEAT_DELAY
            || now.duration_since(last_at) < OFFSET_ADJUST_REPEAT_INTERVAL
        {
            continue;
        }
        let Some(delta) = offset_adjust_delta(code) else {
            continue;
        };
        if state.shift_held {
            let _ = apply_global_offset_delta(state, delta);
        } else if state.course_display_totals.is_none() {
            let _ = apply_song_offset_delta(state, delta);
        }
        state.offset_adjust_last_at[slot] = Some(now);
    }
}

#[inline(always)]
fn update_raw_modifier_state(state: &mut State, code: KeyCode, pressed: bool) {
    match code {
        KeyCode::ShiftLeft | KeyCode::ShiftRight => state.shift_held = pressed,
        KeyCode::ControlLeft | KeyCode::ControlRight => state.ctrl_held = pressed,
        _ => {}
    }
}

#[inline(always)]
fn apply_global_offset_delta(state: &mut State, delta: f32) -> bool {
    let old_offset = state.global_offset_seconds;
    let new_offset = old_offset + delta;
    if (new_offset - old_offset).abs() < 0.000_001_f32 {
        return false;
    }
    mutate_timing_arc(&mut state.timing, |timing| {
        timing.set_global_offset_seconds(new_offset)
    });
    for timing in &mut state.timing_players {
        mutate_timing_arc(timing, |timing| {
            timing.set_global_offset_seconds(new_offset)
        });
    }
    refresh_timing_after_offset_change(state);
    state.global_offset_seconds = new_offset;
    refresh_sync_overlay_message(state);
    true
}

#[inline(always)]
fn apply_song_offset_delta(state: &mut State, delta: f32) -> bool {
    let old_offset = state.song_offset_seconds;
    let new_offset = old_offset + delta;
    if (new_offset - old_offset).abs() < 0.000_001_f32 {
        return false;
    }

    let timing_shift_seconds = old_offset - new_offset;
    mutate_timing_arc(&mut state.timing, |timing| {
        timing.shift_song_offset_seconds(timing_shift_seconds)
    });
    for timing in &mut state.timing_players {
        mutate_timing_arc(timing, |timing| {
            timing.shift_song_offset_seconds(timing_shift_seconds)
        });
    }
    refresh_timing_after_offset_change(state);
    state.song_offset_seconds = new_offset;
    refresh_sync_overlay_message(state);
    true
}

pub enum RawKeyAction {
    None,
    Restart,
}

pub fn handle_queued_raw_key(
    state: &mut State,
    code: KeyCode,
    pressed: bool,
    timestamp: Instant,
    allow_commands: bool,
) -> RawKeyAction {
    update_raw_modifier_state(state, code, pressed);
    if !pressed {
        let _ = clear_offset_adjust_hold(state, code);
        return RawKeyAction::None;
    }
    if !allow_commands {
        return RawKeyAction::None;
    }
    if code == KeyCode::KeyR && state.ctrl_held {
        return RawKeyAction::Restart;
    }
    if code == KeyCode::F6 {
        cycle_autosync_mode(state);
        return RawKeyAction::None;
    }

    if code == KeyCode::F7 {
        let now_music_time = current_music_time_from_stream(state);
        set_tick_mode(state, next_tick_mode(state.tick_mode), now_music_time);
        return RawKeyAction::None;
    }

    if code == KeyCode::F8 {
        let now_music_time = current_music_time_from_stream(state);
        set_autoplay_enabled(state, !state.autoplay_enabled, now_music_time);
        return RawKeyAction::None;
    }
    let Some(delta) = start_offset_adjust_hold(state, code, timestamp) else {
        return RawKeyAction::None;
    };

    if state.shift_held {
        let _ = apply_global_offset_delta(state, delta);
        return RawKeyAction::None;
    }
    if state.course_display_totals.is_none() {
        let _ = apply_song_offset_delta(state, delta);
    }
    RawKeyAction::None
}

fn finalize_row_judgment(
    state: &mut State,
    player: usize,
    row_index: usize,
    row_entry_index: usize,
    skip_life_change: bool,
) {
    let (col_start, col_end) = player_col_range(state, player);
    let row_len = state.row_entries[row_entry_index]
        .nonmine_note_indices
        .len();

    let mut row_has_miss = false;
    let mut row_has_successful_hit = false;
    let mut row_has_wayoff = false;
    let mut has_miss_winner = false;
    let mut final_judgment: Option<Judgment> = None;
    let mut i = 0;
    while i < row_len {
        let note_index = state.row_entries[row_entry_index].nonmine_note_indices[i];
        let note = &state.notes[note_index];
        if note.column < col_start || note.column >= col_end {
            i += 1;
            continue;
        }
        let Some(judgment) = note.result.as_ref() else {
            i += 1;
            continue;
        };

        row_has_miss |= judgment.grade == JudgeGrade::Miss;
        row_has_wayoff |= judgment.grade == JudgeGrade::WayOff;
        row_has_successful_hit |= matches!(
            judgment.grade,
            JudgeGrade::Fantastic | JudgeGrade::Excellent | JudgeGrade::Great
        );

        if judgment.grade == JudgeGrade::Miss {
            if !has_miss_winner {
                final_judgment = Some(judgment.clone());
                has_miss_winner = true;
            }
            i += 1;
            continue;
        }
        if has_miss_winner {
            i += 1;
            continue;
        }

        let should_replace = match final_judgment.as_ref() {
            None => true,
            Some(current) => judgment.time_error_ms.abs() > current.time_error_ms.abs(),
        };
        if should_replace {
            final_judgment = Some(judgment.clone());
        }
        i += 1;
    }

    let Some(final_judgment) = final_judgment else {
        return;
    };
    let scoring_blocked = autoplay_blocks_scoring(state);
    apply_autosync_for_row_hits(state, row_entry_index, col_start, col_end);
    let final_grade = final_judgment.grade;
    record_display_window_counts(state, player, &final_judgment);
    if scoring_blocked {
        state.players[player].last_judgment = Some(JudgmentRenderInfo {
            judgment: final_judgment,
            judged_at: Instant::now(),
        });
        return;
    }
    let p = &mut state.players[player];
    let grade_ix = display_judge_ix(final_grade);
    p.judgment_counts[grade_ix] = p.judgment_counts[grade_ix].saturating_add(1);
    if !is_player_dead(p) {
        p.scoring_counts[grade_ix] = p.scoring_counts[grade_ix].saturating_add(1);
        update_itg_grade_totals(p);
    }
    let life_delta = judge_life_delta(final_grade);
    if !skip_life_change {
        apply_life_change(p, state.current_music_time, life_delta);
    }
    p.last_judgment = Some(JudgmentRenderInfo {
        judgment: final_judgment,
        judged_at: Instant::now(),
    });
    if row_has_successful_hit {
        p.miss_combo = 0;
    }
    if row_has_miss {
        p.miss_combo = p.miss_combo.saturating_add(1);
    }
    if row_has_miss || matches!(final_grade, JudgeGrade::Decent | JudgeGrade::WayOff) {
        p.combo = 0;
        if p.full_combo_grade.is_some() {
            p.first_fc_attempt_broken = true;
        }
        p.full_combo_grade = None;
        p.current_combo_grade = None;
    } else {
        let combo_increment: u32 = state.row_entries[row_entry_index]
            .nonmine_note_indices
            .iter()
            .filter(|&&i| {
                let col = state.notes[i].column;
                col >= col_start && col < col_end
            })
            .count() as u32;
        p.combo = p.combo.saturating_add(combo_increment);
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
    if !row_has_miss && !row_has_wayoff {
        let notes_on_row_count: usize = state.row_entries[row_entry_index]
            .nonmine_note_indices
            .iter()
            .filter(|&&i| {
                let note = &state.notes[i];
                note.column >= col_start && note.column < col_end && !note.is_fake
            })
            .count();
        let carried_holds_down: usize = state.active_holds[col_start..col_end]
            .iter()
            .filter_map(|a| a.as_ref())
            .filter(|a| active_hold_is_engaged(a))
            .filter(|a| {
                let note = &state.notes[a.note_index];
                if note.row_index >= row_index {
                    return false;
                }
                if let Some(h) = note.hold.as_ref() {
                    h.last_held_row_index >= row_index
                } else {
                    false
                }
            })
            .count();
        if notes_on_row_count + carried_holds_down >= 3 {
            p.hands_achieved = p.hands_achieved.saturating_add(1);
        }
    }
}

fn update_judged_rows(state: &mut State) {
    for player in 0..state.num_players {
        let (col_start, col_end) = player_col_range(state, player);
        loop {
            let cursor = state.judged_row_cursor[player];
            if cursor >= state.row_entries.len() {
                break;
            }

            let row_index = state.row_entries[cursor].row_index;
            let row_len = state.row_entries[cursor].nonmine_note_indices.len();
            let mut has_notes_on_row = false;
            let mut is_row_complete = true;
            let mut skip_life_change = false;
            let mut i = 0;
            while i < row_len {
                let note_index = state.row_entries[cursor].nonmine_note_indices[i];
                let note = &state.notes[note_index];
                if note.column < col_start || note.column >= col_end {
                    i += 1;
                    continue;
                }
                has_notes_on_row = true;
                if note.result.is_none() {
                    is_row_complete = false;
                    break;
                }
                skip_life_change |= note.early_result.is_some();
                i += 1;
            }

            if !has_notes_on_row {
                state.judged_row_cursor[player] += 1;
                continue;
            }

            if is_row_complete {
                finalize_row_judgment(state, player, row_index, cursor, skip_life_change);
                state.judged_row_cursor[player] += 1;
            } else {
                break;
            }
        }
    }
}

#[inline(always)]
fn process_input_edges(
    state: &mut State,
    trace_enabled: bool,
    phase_timings: &mut GameplayUpdatePhaseTimings,
    song_clock: SongClockSnapshot,
) {
    if state.pending_edges.is_empty() {
        return;
    }

    let mut pending = VecDeque::new();
    if trace_enabled {
        let started = Instant::now();
        std::mem::swap(&mut pending, &mut state.pending_edges);
        add_elapsed_us(&mut phase_timings.input_queue_us, started);
    } else {
        std::mem::swap(&mut pending, &mut state.pending_edges);
    }

    while let Some(mut edge) = pending.pop_front() {
        let lane_idx = edge.lane.index();
        if lane_idx >= state.num_cols {
            continue;
        }
        if !edge.event_music_time.is_finite() {
            edge.event_music_time =
                music_time_from_song_clock(song_clock, edge.captured_at, edge.captured_host_nanos);
        }
        if edge.record_replay && edge.event_music_time.is_finite() {
            state.replay_edges.push(RecordedLaneEdge {
                lane_index: lane_idx as u8,
                pressed: edge.pressed,
                source: edge.source,
                event_music_time: edge.event_music_time,
            });
        }
        if trace_enabled {
            let processed_at = Instant::now();
            let capture_to_store_us = elapsed_us_between(edge.stored_at, edge.captured_at);
            let store_to_emit_us = elapsed_us_between(edge.emitted_at, edge.stored_at);
            let emit_to_queue_us = elapsed_us_between(edge.queued_at, edge.emitted_at);
            let capture_to_queue_us = elapsed_us_between(edge.queued_at, edge.captured_at);
            let capture_to_process_us = elapsed_us_between(processed_at, edge.captured_at);
            let queue_to_process_us = elapsed_us_between(processed_at, edge.queued_at);
            state.update_trace.summary_input_latency.record(
                capture_to_store_us,
                store_to_emit_us,
                emit_to_queue_us,
                capture_to_process_us,
                queue_to_process_us,
            );
            if capture_to_process_us >= GAMEPLAY_INPUT_LATENCY_WARN_US {
                debug!(
                    "Gameplay input latency spike: lane={} pressed={} source={:?} capture_store_us={} store_emit_us={} emit_queue_us={} queue_process_us={} capture_queue_us={} capture_process_us={} pending={} now_t={:.3} edge_t={:.3}",
                    lane_idx,
                    edge.pressed,
                    edge.source,
                    capture_to_store_us,
                    store_to_emit_us,
                    emit_to_queue_us,
                    queue_to_process_us,
                    capture_to_queue_us,
                    capture_to_process_us,
                    pending.len() + state.pending_edges.len() + 1,
                    state.current_music_time,
                    edge.event_music_time,
                );
            }
        }

        let state_started = if trace_enabled {
            Some(Instant::now())
        } else {
            None
        };
        let mut keyboard_count = state.keyboard_lane_counts[lane_idx];
        let mut gamepad_count = state.gamepad_lane_counts[lane_idx];
        let was_down = keyboard_count != 0 || gamepad_count != 0;
        match edge.source {
            InputSource::Keyboard => update_lane_count(&mut keyboard_count, edge.pressed),
            InputSource::Gamepad => update_lane_count(&mut gamepad_count, edge.pressed),
        }
        state.keyboard_lane_counts[lane_idx] = keyboard_count;
        state.gamepad_lane_counts[lane_idx] = gamepad_count;
        let is_down = keyboard_count != 0 || gamepad_count != 0;
        if let Some(started) = state_started {
            add_elapsed_us(&mut phase_timings.input_state_us, started);
        }

        let press_started = lane_press_started(edge.pressed, was_down, is_down);
        let release_finished = lane_release_finished(edge.pressed, was_down, is_down);

        if press_started {
            if trace_enabled {
                let started = Instant::now();
                start_receptor_glow_press(state, lane_idx);
                add_elapsed_us(&mut phase_timings.input_glow_us, started);
            } else {
                start_receptor_glow_press(state, lane_idx);
            }
        } else if release_finished {
            if trace_enabled {
                let started = Instant::now();
                release_receptor_glow(state, lane_idx);
                add_elapsed_us(&mut phase_timings.input_glow_us, started);
            } else {
                release_receptor_glow(state, lane_idx);
            }
        }

        if lane_edge_judges_tap(edge.pressed) {
            let event_music_time = edge.event_music_time;
            let hit_note = if trace_enabled {
                let started = Instant::now();
                let hit_note = judge_a_tap(state, lane_idx, event_music_time);
                add_elapsed_us(&mut phase_timings.input_judge_us, started);
                hit_note
            } else {
                judge_a_tap(state, lane_idx, event_music_time)
            };
            if trace_enabled {
                let started = Instant::now();
                refresh_roll_life_on_step(state, lane_idx);
                add_elapsed_us(&mut phase_timings.input_roll_us, started);
            } else {
                refresh_roll_life_on_step(state, lane_idx);
            }
            if hit_note {
                if state.tick_mode == TickMode::Hit {
                    audio::play_assist_tick(ASSIST_TICK_SFX_PATH);
                }
            } else {
                state.receptor_bop_timers[lane_idx] = 0.11;
            }
        } else if lane_edge_judges_lift(edge.pressed, was_down) {
            let event_music_time = edge.event_music_time;
            let hit_lift = judge_a_lift(state, lane_idx, event_music_time);
            if hit_lift && state.tick_mode == TickMode::Hit {
                audio::play_assist_tick(ASSIST_TICK_SFX_PATH);
            }
        }
    }

    if !state.pending_edges.is_empty() {
        if trace_enabled {
            let started = Instant::now();
            pending.append(&mut state.pending_edges);
            add_elapsed_us(&mut phase_timings.input_queue_us, started);
        } else {
            pending.append(&mut state.pending_edges);
        }
    }
    if trace_enabled {
        let started = Instant::now();
        state.pending_edges = pending;
        add_elapsed_us(&mut phase_timings.input_queue_us, started);
    } else {
        state.pending_edges = pending;
    }
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
            i += 1;
            continue;
        }
        let start_time = hold.let_go_started_at.unwrap();
        let base_life = hold.let_go_starting_life.clamp(0.0, MAX_HOLD_LIFE);
        if base_life <= 0.0 {
            hold.life = 0.0;
            i += 1;
            continue;
        }
        let rate = if state.music_rate.is_finite() && state.music_rate > 0.0 {
            state.music_rate
        } else {
            1.0
        };
        let elapsed_music = (state.current_music_time - start_time).max(0.0);
        let elapsed_real = elapsed_music / rate;
        hold.life = (base_life - elapsed_real / window).max(0.0);
        i += 1;
    }
}

#[inline(always)]
fn tick_visual_effects(state: &mut State, delta_time: f32) {
    for col in 0..state.num_cols {
        if lane_is_pressed(state, col) {
            state.receptor_glow_timers[col] = 0.0;
            state.receptor_glow_press_timers[col] =
                (state.receptor_glow_press_timers[col] - delta_time).max(0.0);
        } else {
            state.receptor_glow_press_timers[col] = 0.0;
            state.receptor_glow_timers[col] =
                (state.receptor_glow_timers[col] - delta_time).max(0.0);
        }
    }
    for timer in &mut state.receptor_bop_timers {
        *timer = (*timer - delta_time).max(0.0);
    }
    if state.toggle_flash_timer > 0.0 {
        state.toggle_flash_timer = (state.toggle_flash_timer - delta_time).max(0.0);
    }
    for player in 0..state.num_players {
        state.players[player]
            .combo_milestones
            .retain_mut(|milestone| {
                milestone.elapsed += delta_time;
                let max_duration = match milestone.kind {
                    ComboMilestoneKind::Hundred => COMBO_HUNDRED_MILESTONE_DURATION,
                    ComboMilestoneKind::Thousand => COMBO_THOUSAND_MILESTONE_DURATION,
                };
                milestone.elapsed < max_duration
            });
    }
    let num_players = state.num_players;
    let cols_per_player = state.cols_per_player;
    for (col, explosion) in state.tap_explosions.iter_mut().enumerate() {
        if let Some(active) = explosion {
            active.elapsed += delta_time;
            let player = if num_players <= 1 || cols_per_player == 0 {
                0
            } else {
                (col / cols_per_player).min(num_players.saturating_sub(1))
            };
            let lifetime = state.noteskin[player]
                .as_ref()
                .and_then(|ns| ns.tap_explosions.get(&active.window))
                .map_or(0.0, |explosion| explosion.animation.duration());
            if lifetime <= 0.0 || active.elapsed >= lifetime {
                *explosion = None;
            }
        }
    }
    for (col, explosion) in state.mine_explosions.iter_mut().enumerate() {
        if let Some(active) = explosion {
            active.elapsed += delta_time;
            let player = if num_players <= 1 || cols_per_player == 0 {
                0
            } else {
                (col / cols_per_player).min(num_players.saturating_sub(1))
            };
            let lifetime = state.noteskin[player]
                .as_ref()
                .and_then(|ns| ns.mine_hit_explosion.as_ref())
                .map_or(MINE_EXPLOSION_DURATION, |explosion| {
                    explosion.animation.duration()
                });
            if lifetime <= 0.0 || active.elapsed >= lifetime {
                *explosion = None;
            }
        }
    }
    for slot in &mut state.hold_judgments {
        if let Some(render_info) = slot
            && render_info.triggered_at.elapsed().as_secs_f32() >= HOLD_JUDGMENT_TOTAL_DURATION
        {
            *slot = None;
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
fn apply_time_based_mine_avoidance(state: &mut State, music_time_sec: f32) {
    let mine_window = state.timing_profile.mine_window_s;
    let rate = if state.music_rate.is_finite() && state.music_rate > 0.0 {
        state.music_rate
    } else {
        1.0
    };
    let cutoff_time = mine_window.mul_add(-rate, music_time_sec);
    let log_mine_avoid = mine_avoid_log_enabled();
    for player in 0..state.num_players {
        let mines_len = state.mine_note_ix[player].len();
        let mine_cursor = state.next_mine_ix_cursor[player].min(mines_len);
        let mine_end = mine_cursor
            + state.mine_note_time[player][mine_cursor..].partition_point(|&t| t <= cutoff_time);
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
fn partition_notes_before_time(note_times: &[f32], lookahead_time: f32) -> usize {
    note_times.partition_point(|note_time| *note_time < lookahead_time)
}

#[inline(always)]
fn spawn_lookahead_arrows(state: &mut State, music_time_sec: f32) {
    for player in 0..state.num_players {
        let timing = &state.timing_players[player];
        let (note_start, note_end) = player_note_range(state, player);
        let mut cursor = state.note_spawn_cursor[player].max(note_start);
        let spawn_time = music_time_sec.max(state.current_music_time_visible[player]);
        let scroll_speed = effective_scroll_speed_for_player(state, player);
        match scroll_speed {
            ScrollSpeedSetting::CMod(_) => {
                let lookahead_time = spawn_time + state.scroll_travel_time[player];
                // C-mod note travel is time-based. Beat lookahead freezes inside stops,
                // which stalls spawning until the note is effectively due.
                let spawn_limit = cursor
                    + partition_notes_before_time(
                        &state.note_time_cache[cursor..note_end],
                        lookahead_time,
                    );
                while cursor < spawn_limit {
                    let note = &state.notes[cursor];
                    if note.column < state.num_cols {
                        state.arrows[note.column].push(Arrow {
                            beat: note.beat,
                            note_type: note.note_type,
                            note_index: cursor,
                        });
                    }
                    cursor += 1;
                }
            }
            ScrollSpeedSetting::XMod(_) | ScrollSpeedSetting::MMod(_) => {
                let spawn_beat = timing.get_beat_for_time(spawn_time);
                let current_displayed_beat = timing.get_displayed_beat(spawn_beat);
                let speed_multiplier = timing.get_speed_multiplier(spawn_beat, spawn_time);
                let player_multiplier =
                    scroll_speed.beat_multiplier(state.scroll_reference_bpm, state.music_rate);
                let final_multiplier = player_multiplier * speed_multiplier;
                if final_multiplier > 0.0 {
                    let pixels_per_beat = ScrollSpeedSetting::ARROW_SPACING
                        * final_multiplier
                        * state.field_zoom[player];
                    let lookahead_in_displayed_beats =
                        state.draw_distance_before_targets[player] / pixels_per_beat;
                    let mut target_displayed_beat =
                        current_displayed_beat + lookahead_in_displayed_beats;
                    if speed_multiplier < 0.75 {
                        let cap_displayed_beat = timing.get_displayed_beat(spawn_beat + 16.0);
                        target_displayed_beat = target_displayed_beat.min(cap_displayed_beat);
                    }
                    while cursor < note_end {
                        let note_disp_beat = state.note_display_beat_cache[cursor];
                        if note_disp_beat >= target_displayed_beat {
                            break;
                        }
                        let note = &state.notes[cursor];
                        if note.column < state.num_cols {
                            state.arrows[note.column].push(Arrow {
                                beat: note.beat,
                                note_type: note.note_type,
                                note_index: cursor,
                            });
                        }
                        cursor += 1;
                    }
                }
            }
        }
        state.note_spawn_cursor[player] = cursor;
    }
}

#[inline(always)]
fn apply_passive_misses_and_mine_avoidance(state: &mut State, music_time_sec: f32) {
    let way_off_window = state.timing_profile.windows_s[4];
    let num_players = state.num_players;
    let cols_per_player = state.cols_per_player;
    let log_mine_avoid = mine_avoid_log_enabled();
    let rate = if state.music_rate.is_finite() && state.music_rate > 0.0 {
        state.music_rate
    } else {
        1.0
    };
    for (col_idx, col_arrows) in state.arrows.iter_mut().enumerate() {
        let Some(next_arrow_index) = col_arrows
            .iter()
            .position(|arrow| state.notes[arrow.note_index].result.is_none())
        else {
            continue;
        };
        let note_index = col_arrows[next_arrow_index].note_index;
        let (note_row_index, note_type) = {
            let note = &state.notes[note_index];
            (note.row_index, note.note_type)
        };
        let note_time = state.note_time_cache[note_index];

        if matches!(note_type, NoteType::Mine) {
            match state.notes[note_index].mine_result {
                Some(MineResult::Hit) => {
                    col_arrows.remove(next_arrow_index);
                }
                Some(MineResult::Avoided) => {}
                None => {
                    let mine_window = state.timing_profile.mine_window_s;
                    if music_time_sec - note_time > mine_window * rate
                        && state.notes[note_index].can_be_judged
                    {
                        state.notes[note_index].mine_result = Some(MineResult::Avoided);
                        let player = if num_players <= 1 || cols_per_player == 0 {
                            0
                        } else {
                            (col_idx / cols_per_player).min(num_players.saturating_sub(1))
                        };
                        state.players[player].mines_avoided =
                            state.players[player].mines_avoided.saturating_add(1);
                        if log_mine_avoid {
                            trace!(
                                "MINE AVOIDED: Row {note_row_index}, Col {col_idx}, Time: {music_time_sec:.2}s"
                            );
                        }
                    }
                }
            }
            continue;
        }
        if state.notes[note_index].is_fake {
            continue;
        }
        if !state.notes[note_index].can_be_judged {
            continue;
        }
        if music_time_sec - note_time > way_off_window * rate {
            let time_err_music = music_time_sec - note_time;
            let time_err_real = time_err_music / rate;
            let miss_because_held = lane_counts_pressed(
                state.keyboard_lane_counts[col_idx],
                state.gamepad_lane_counts[col_idx],
            );
            let miss = Judgment {
                time_error_ms: time_err_real * 1000.0,
                grade: JudgeGrade::Miss,
                window: None,
                miss_because_held,
            };
            let judgment = state.notes[note_index].early_result.clone().unwrap_or(miss);
            if judgment.grade == JudgeGrade::Miss
                && let Some(hold) = state.notes[note_index].hold.as_mut()
                && hold.result != Some(HoldResult::Held)
            {
                hold.result = Some(HoldResult::LetGo);
                if hold.let_go_started_at.is_none() {
                    hold.let_go_started_at = Some(music_time_sec);
                    hold.let_go_starting_life = hold.life.clamp(0.0, MAX_HOLD_LIFE);
                    if note_index < state.hold_decay_active.len()
                        && !state.hold_decay_active[note_index]
                    {
                        state.hold_decay_active[note_index] = true;
                        state.decaying_hold_indices.push(note_index);
                    }
                }
            }
            state.notes[note_index].result = Some(judgment);
            debug!("MISSED (pending): Row {note_row_index}, Col {col_idx}");
        }
    }
}

#[inline(always)]
fn apply_time_based_tap_misses(state: &mut State, music_time_sec: f32) {
    let way_off_window = state.timing_profile.windows_s[4];
    let rate = if state.music_rate.is_finite() && state.music_rate > 0.0 {
        state.music_rate
    } else {
        1.0
    };
    let cutoff_time = way_off_window.mul_add(-rate, music_time_sec);
    for player in 0..state.num_players {
        let (note_start, note_end) = player_note_range(state, player);
        let mut cursor = state.next_tap_miss_cursor[player].max(note_start);
        while cursor < note_end {
            let note_time = state.note_time_cache[cursor];
            if note_time > cutoff_time {
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
                let time_err_music = music_time_sec - note_time;
                let time_err_real = time_err_music / rate;
                let miss_because_held = (col < state.num_cols) && lane_is_pressed(state, col);
                let miss = Judgment {
                    time_error_ms: time_err_real * 1000.0,
                    grade: JudgeGrade::Miss,
                    window: None,
                    miss_because_held,
                };
                let judgment = state.notes[cursor].early_result.clone().unwrap_or(miss);
                let judgment_grade = judgment.grade;
                let judgment_time_error_ms = judgment.time_error_ms;
                if judgment_grade == JudgeGrade::Miss
                    && let Some(hold) = state.notes[cursor].hold.as_mut()
                    && hold.result != Some(HoldResult::Held)
                {
                    hold.result = Some(HoldResult::LetGo);
                    if hold.let_go_started_at.is_none() {
                        hold.let_go_started_at = Some(music_time_sec);
                        hold.let_go_starting_life = hold.life.clamp(0.0, MAX_HOLD_LIFE);
                        if cursor < state.hold_decay_active.len()
                            && !state.hold_decay_active[cursor]
                        {
                            state.hold_decay_active[cursor] = true;
                            state.decaying_hold_indices.push(cursor);
                        }
                    }
                }
                state.notes[cursor].result = Some(judgment);
                if log::log_enabled!(log::Level::Debug) {
                    let song_offset_s = state.song_offset_seconds;
                    let global_offset_s = state.global_offset_seconds;
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

#[inline(always)]
fn cull_scrolled_out_arrows(state: &mut State, music_time_sec: f32) {
    let num_players = state.num_players;
    let cols_per_player = state.cols_per_player;
    let player_scroll: [ScrollEffects; MAX_PLAYERS] = std::array::from_fn(|player| {
        if player >= num_players {
            ScrollEffects::default()
        } else {
            effective_scroll_effects_for_player(state, player)
        }
    });
    let player_offset_y: [f32; MAX_PLAYERS] = std::array::from_fn(|player| {
        if player >= num_players {
            0.0
        } else {
            state.player_profiles[player]
                .note_field_offset_y
                .clamp(-50, 50) as f32
        }
    });
    let receptor_y_normal: [f32; MAX_PLAYERS] = std::array::from_fn(|player| {
        screen_center_y() + RECEPTOR_Y_OFFSET_FROM_CENTER + player_offset_y[player]
    });
    let receptor_y_reverse: [f32; MAX_PLAYERS] = std::array::from_fn(|player| {
        screen_center_y() + RECEPTOR_Y_OFFSET_FROM_CENTER_REVERSE + player_offset_y[player]
    });
    let receptor_y_centered: [f32; MAX_PLAYERS] =
        std::array::from_fn(|player| screen_center_y() + player_offset_y[player]);
    let player_cull_time: [f32; MAX_PLAYERS] =
        std::array::from_fn(|player| music_time_sec.min(state.current_music_time_visible[player]));
    let player_cull_beat: [f32; MAX_PLAYERS] = std::array::from_fn(|player| {
        state.timing_players[player].get_beat_for_time(player_cull_time[player])
    });
    let player_curr_disp_beat: [f32; MAX_PLAYERS] = std::array::from_fn(|player| {
        state.timing_players[player].get_displayed_beat(player_cull_beat[player])
    });
    let player_speed_multiplier: [f32; MAX_PLAYERS] = std::array::from_fn(|player| {
        state.timing_players[player]
            .get_speed_multiplier(player_cull_beat[player], player_cull_time[player])
    });
    let effective_scroll_speed: [ScrollSpeedSetting; MAX_PLAYERS] = std::array::from_fn(|player| {
        if player >= num_players {
            ScrollSpeedSetting::default()
        } else {
            effective_scroll_speed_for_player(state, player)
        }
    });

    let beatmod_multiplier: [f32; MAX_PLAYERS] =
        std::array::from_fn(|player| match effective_scroll_speed[player] {
            ScrollSpeedSetting::XMod(_) | ScrollSpeedSetting::MMod(_) => {
                effective_scroll_speed[player]
                    .beat_multiplier(state.scroll_reference_bpm, state.music_rate)
                    * player_speed_multiplier[player]
            }
            ScrollSpeedSetting::CMod(_) => 0.0,
        });
    let cmod_pps_zoomed: [f32; MAX_PLAYERS] =
        std::array::from_fn(|player| match effective_scroll_speed[player] {
            ScrollSpeedSetting::CMod(c_bpm) => {
                (c_bpm / 60.0) * ScrollSpeedSetting::ARROW_SPACING * state.field_zoom[player]
            }
            _ => 0.0,
        });
    let cmod_pps_raw: [f32; MAX_PLAYERS] =
        std::array::from_fn(|player| match effective_scroll_speed[player] {
            ScrollSpeedSetting::CMod(c_bpm) => (c_bpm / 60.0) * ScrollSpeedSetting::ARROW_SPACING,
            _ => 0.0,
        });
    let column_dirs = state.column_scroll_dirs;

    // Centered receptors ignore Reverse for positioning (but not direction).
    // Apply notefield offset here too for consistency.
    let num_cols = state.num_cols;
    let column_receptor_ys: [f32; MAX_COLS] = std::array::from_fn(|i| {
        if i >= num_cols {
            return receptor_y_normal[0];
        }
        let player = if num_players <= 1 || cols_per_player == 0 {
            0
        } else {
            (i / cols_per_player).min(num_players.saturating_sub(1))
        };
        let local_col = i.saturating_sub(player.saturating_mul(cols_per_player));
        scroll_receptor_y(
            player_scroll[player].reverse_percent_for_column(local_col, cols_per_player),
            player_scroll[player].centered,
            receptor_y_normal[player],
            receptor_y_reverse[player],
            receptor_y_centered[player],
        )
    });

    let rate = if state.music_rate.is_finite() && state.music_rate > 0.0 {
        state.music_rate
    } else {
        1.0
    };

    for (col_idx, col_arrows) in state.arrows.iter_mut().enumerate() {
        let dir = column_dirs[col_idx];
        let receptor_y = column_receptor_ys[col_idx];
        let player = if num_players <= 1 || cols_per_player == 0 {
            0
        } else {
            (col_idx / cols_per_player).min(num_players.saturating_sub(1))
        };
        let cull_time = player_cull_time[player];
        let curr_disp_beat = player_curr_disp_beat[player];
        let scroll_speed = effective_scroll_speed[player];
        let beatmult = beatmod_multiplier[player];
        let cmod_zoomed = cmod_pps_zoomed[player];
        let cmod_raw = cmod_pps_raw[player];
        let cmp_sign = if dir < 0.0_f32 { -1.0_f32 } else { 1.0_f32 };

        let miss_cull_threshold =
            dir.mul_add(-state.draw_distance_after_targets[player], receptor_y);
        match scroll_speed {
            ScrollSpeedSetting::CMod(_) => {
                let cmod_raw_slope = dir * cmod_raw / rate;
                let cmod_zoomed_slope = dir * cmod_zoomed / rate;
                let cmod_raw_base = receptor_y - cull_time * cmod_raw_slope;
                let cmod_zoomed_base = receptor_y - cull_time * cmod_zoomed_slope;

                col_arrows.retain(|arrow| {
                    let note = &state.notes[arrow.note_index];
                    let use_raw_pos = if matches!(note.note_type, NoteType::Mine) {
                        if note.is_fake {
                            true
                        } else {
                            match note.mine_result {
                                Some(MineResult::Avoided) => false,
                                Some(MineResult::Hit) => return false,
                                None => return true,
                            }
                        }
                    } else if note.is_fake {
                        true
                    } else {
                        let Some(judgment) = note.result.as_ref() else {
                            return true;
                        };
                        if judgment.grade != JudgeGrade::Miss {
                            return false;
                        }
                        false
                    };

                    let note_time_chart = state.note_time_cache[arrow.note_index];
                    let y_pos = if use_raw_pos {
                        note_time_chart.mul_add(cmod_raw_slope, cmod_raw_base)
                    } else {
                        note_time_chart.mul_add(cmod_zoomed_slope, cmod_zoomed_base)
                    };
                    (y_pos - miss_cull_threshold) * cmp_sign >= 0.0_f32
                });
            }
            ScrollSpeedSetting::XMod(_) | ScrollSpeedSetting::MMod(_) => {
                let beat_slope =
                    dir * ScrollSpeedSetting::ARROW_SPACING * beatmult * state.field_zoom[player];
                let beat_base = receptor_y - curr_disp_beat * beat_slope;

                col_arrows.retain(|arrow| {
                    let note = &state.notes[arrow.note_index];
                    if matches!(note.note_type, NoteType::Mine) {
                        if !note.is_fake {
                            match note.mine_result {
                                Some(MineResult::Avoided) => {}
                                Some(MineResult::Hit) => return false,
                                None => return true,
                            }
                        }
                    } else if !note.is_fake {
                        let Some(judgment) = note.result.as_ref() else {
                            return true;
                        };
                        if judgment.grade != JudgeGrade::Miss {
                            return false;
                        }
                    }

                    let note_disp_beat = state.note_display_beat_cache[arrow.note_index];
                    let y_pos = note_disp_beat.mul_add(beat_slope, beat_base);
                    (y_pos - miss_cull_threshold) * cmp_sign >= 0.0_f32
                });
            }
        }
    }

    // ITG parity guard: cap total past-receptor arrows per player.
    for player in 0..num_players {
        let start_col = player.saturating_mul(cols_per_player);
        let end_col = (start_col + cols_per_player).min(num_cols).min(MAX_COLS);
        if start_col >= end_col {
            continue;
        }

        let cull_beat = player_cull_beat[player];
        let mut past_prefix_len = [0usize; MAX_COLS];
        let mut total_past = 0usize;
        for col_idx in start_col..end_col {
            let len = state.arrows[col_idx].partition_point(|arrow| arrow.beat <= cull_beat);
            past_prefix_len[col_idx] = len;
            total_past += len;
        }
        if total_past <= MAX_NOTES_AFTER_TARGETS {
            continue;
        }

        let mut drop_prefix = [0usize; MAX_COLS];
        let mut drop_remaining = total_past - MAX_NOTES_AFTER_TARGETS;
        while drop_remaining > 0 {
            let mut best = (usize::MAX, usize::MAX, usize::MAX);
            for col_idx in start_col..end_col {
                let arrow_idx = drop_prefix[col_idx];
                if arrow_idx >= past_prefix_len[col_idx] {
                    continue;
                }
                let note_index = state.arrows[col_idx][arrow_idx].note_index;
                let row_index = state.notes[note_index].row_index;
                let candidate = (row_index, col_idx, arrow_idx);
                if candidate < best {
                    best = candidate;
                }
            }
            if best.1 == usize::MAX {
                break;
            }
            drop_prefix[best.1] += 1;
            drop_remaining -= 1;
        }
        for col_idx in start_col..end_col {
            let drop_count = drop_prefix[col_idx];
            if drop_count == 0 {
                continue;
            }
            state.arrows[col_idx].drain(..drop_count);
        }
    }
}

pub fn update(state: &mut State, delta_time: f32) -> ScreenAction {
    if let Some(exit) = state.exit_transition {
        state.total_elapsed_in_screen += delta_time;
        if exit.started_at.elapsed().as_secs_f32() >= exit_total_seconds(exit.kind) {
            state.exit_transition = None;
            return ScreenAction::NavigateNoFade(exit.target);
        }
        return ScreenAction::None;
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
    let mut song_clock = current_song_clock_snapshot(state);
    let lead_in = state.audio_lead_in_seconds.max(0.0);
    let previous_music_time = state.current_music_time;
    let mut music_time_sec = song_clock.song_time;
    let is_first_update = state.total_elapsed_in_screen <= f32::EPSILON;
    if is_first_update {
        const STARTUP_MAX_FORWARD_JUMP_S: f32 = 1.0;
        let jump_s = music_time_sec - previous_music_time;
        if jump_s > STARTUP_MAX_FORWARD_JUMP_S {
            warn!(
                "Discarding anomalous first-frame music time jump ({jump_s:.3}s): prev={previous_music_time:.3}, now={music_time_sec:.3}, lead_in={lead_in:.3}"
            );
            music_time_sec = previous_music_time;
        }
    }
    song_clock.song_time = music_time_sec;
    state.current_music_time = music_time_sec;
    let target_display_music_time_sec = music_time_sec;
    let display_music_time_sec = frame_stable_display_music_time(
        &mut state.display_clock,
        target_display_music_time_sec,
        delta_time,
        song_clock.seconds_per_second,
        is_first_update,
    );
    state.current_music_time_display = display_music_time_sec;

    if let (Some(key), Some(start_time)) = (state.hold_to_exit_key, state.hold_to_exit_start) {
        let hold_s = match key {
            HoldToExitKey::Start => GIVE_UP_HOLD_SECONDS,
            HoldToExitKey::Back => BACK_OUT_HOLD_SECONDS,
        };
        if start_time.elapsed().as_secs_f32() >= hold_s {
            if key == HoldToExitKey::Start && music_time_sec >= state.notes_end_time {
                state.song_completed_naturally = true;
            }
            match key {
                HoldToExitKey::Start => {
                    begin_exit_transition(state, ExitTransitionKind::Out, Screen::Evaluation);
                }
                HoldToExitKey::Back => {
                    begin_exit_transition(state, ExitTransitionKind::Cancel, Screen::SelectMusic);
                }
            }
            finalize_update_trace(
                state,
                delta_time,
                music_time_sec,
                frame_trace_started,
                phase_timings,
            );
            return ScreenAction::None;
        }
    }
    state.total_elapsed_in_screen += delta_time;

    debug_validate_hot_state(state, delta_time, music_time_sec);

    let pre_notes_started = if trace_enabled {
        Some(Instant::now())
    } else {
        None
    };
    {
        let beat_info = state
            .timing
            .get_beat_info_from_time_cached(music_time_sec, &mut state.beat_info_cache);
        state.current_beat = beat_info.beat;
        state.current_beat_display = state.timing.get_beat_for_time(display_music_time_sec);
        state.is_in_freeze = beat_info.is_in_freeze;
        state.is_in_delay = beat_info.is_in_delay;
        let song_row = assist_row_no_offset(state, music_time_sec);
        run_assist_clap(state, song_row);

        for player in 0..state.num_players {
            let delay =
                state.global_visual_delay_seconds + state.player_visual_delay_seconds[player];
            let visible_time = display_music_time_sec - delay;
            state.current_music_time_visible[player] = visible_time;
            state.current_beat_visible[player] =
                state.timing_players[player].get_beat_for_time(visible_time);
        }
        refresh_active_attack_masks(state);

        let current_bpm = state.timing.get_bpm_for_beat(state.current_beat);
        refresh_live_notefield_options(state, current_bpm);
    }
    if let Some(started) = pre_notes_started {
        phase_timings.pre_notes_us = elapsed_us_since(started);
    }

    if state.current_music_time >= state.music_end_time {
        debug!("Music end time reached. Transitioning to evaluation.");
        state.song_completed_naturally = true;
        finalize_update_trace(
            state,
            delta_time,
            music_time_sec,
            frame_trace_started,
            phase_timings,
        );
        return ScreenAction::Navigate(Screen::Evaluation);
    }

    let autoplay_started = if trace_enabled {
        Some(Instant::now())
    } else {
        None
    };
    if state.replay_mode {
        run_replay(state, music_time_sec);
    } else {
        run_autoplay(state, music_time_sec);
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
    for (col, (now_down, was_down)) in current_inputs.iter().copied().zip(prev_inputs).enumerate() {
        if now_down && was_down {
            let _ =
                try_hit_crossed_mines_while_held(state, col, previous_music_time, music_time_sec);
        }
    }
    state.prev_inputs = current_inputs;
    if let Some(started) = held_mines_started {
        phase_timings.held_mines_us = elapsed_us_since(started);
    }

    let active_holds_started = if trace_enabled {
        Some(Instant::now())
    } else {
        None
    };
    update_active_holds(state, &current_inputs, music_time_sec, delta_time);
    if let Some(started) = active_holds_started {
        phase_timings.active_holds_us = elapsed_us_since(started);
    }

    let hold_decay_started = if trace_enabled {
        Some(Instant::now())
    } else {
        None
    };
    decay_let_go_hold_life(state);
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

    let spawn_started = if trace_enabled {
        Some(Instant::now())
    } else {
        None
    };
    spawn_lookahead_arrows(state, music_time_sec);
    if let Some(started) = spawn_started {
        phase_timings.spawn_arrows_us = elapsed_us_since(started);
    }

    let mine_avoid_started = if trace_enabled {
        Some(Instant::now())
    } else {
        None
    };
    apply_time_based_mine_avoidance(state, music_time_sec);
    if let Some(started) = mine_avoid_started {
        phase_timings.mine_avoid_us = elapsed_us_since(started);
    }

    let passive_miss_started = if trace_enabled {
        Some(Instant::now())
    } else {
        None
    };
    apply_passive_misses_and_mine_avoidance(state, music_time_sec);
    if let Some(started) = passive_miss_started {
        phase_timings.passive_miss_us = elapsed_us_since(started);
    }

    let tap_miss_started = if trace_enabled {
        Some(Instant::now())
    } else {
        None
    };
    apply_time_based_tap_misses(state, music_time_sec);
    if let Some(started) = tap_miss_started {
        phase_timings.tap_miss_us = elapsed_us_since(started);
    }

    let cull_started = if trace_enabled {
        Some(Instant::now())
    } else {
        None
    };
    cull_scrolled_out_arrows(state, music_time_sec);
    if let Some(started) = cull_started {
        phase_timings.cull_us = elapsed_us_since(started);
    }

    let judged_rows_started = if trace_enabled {
        Some(Instant::now())
    } else {
        None
    };
    update_judged_rows(state);
    if let Some(started) = judged_rows_started {
        phase_timings.judged_rows_us = elapsed_us_since(started);
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
        return ScreenAction::Navigate(Screen::Evaluation);
    }

    debug_validate_hot_state(state, delta_time, music_time_sec);
    finalize_update_trace(
        state,
        delta_time,
        music_time_sec,
        frame_trace_started,
        phase_timings,
    );
    ScreenAction::None
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
        FrameStableDisplayClock, GAMEPLAY_INPUT_BACKLOG_WARN, INSERT_MASK_BIT_MINES,
        REPLAY_EDGE_RATE_PER_SEC, ScrollEffects, ScrollSpeedSetting, SongClockSnapshot, TickMode,
        apply_mines_insert, build_assist_clap_rows, build_attack_mask_windows_for_player,
        frame_stable_display_music_time, input_queue_cap, lane_edge_judges_lift,
        lane_edge_judges_tap, lane_press_started, lane_release_finished,
        music_time_from_song_clock, next_tick_mode, parse_attack_mods, partition_notes_before_time,
        player_draw_scale_for_tilt_with_visual_mask, recompute_player_totals, replay_edge_cap,
        score_valid_for_chart, scored_hold_totals_with_carry, stage_music_cut,
        tick_mode_status_line, turn_option_bits, update_lane_count,
    };
    use crate::core::input::InputSource;
    use crate::game::chart::{ChartData, StaminaCounts};
    use crate::game::note::{HoldData, Note, NoteType};
    use crate::game::profile;
    use crate::game::timing::{ROWS_PER_BEAT, StopSegment, TimingData, TimingSegments};
    use rssp::{TechCounts, stats::ArrowStats};
    use std::time::{Duration, Instant};

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

    fn test_chart(
        stats: ArrowStats,
        timing_segments: TimingSegments,
        chart_attacks: Option<&str>,
    ) -> ChartData {
        let mines_nonfake = stats.mines;
        ChartData {
            chart_type: "dance-single".to_string(),
            difficulty: "Challenge".to_string(),
            description: String::new(),
            chart_name: String::new(),
            meter: 10,
            step_artist: String::new(),
            short_hash: String::new(),
            stats,
            tech_counts: TechCounts::default(),
            mines_nonfake,
            stamina_counts: StaminaCounts::default(),
            total_streams: 0,
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
            has_significant_timing_changes: {
                !timing_segments.stops.is_empty()
                    || !timing_segments.delays.is_empty()
                    || !timing_segments.warps.is_empty()
                    || !timing_segments.speeds.is_empty()
                    || !timing_segments.scrolls.is_empty()
                    || {
                        let mut min_bpm = f32::INFINITY;
                        let mut max_bpm = 0.0_f32;
                        for &(_, bpm) in &timing_segments.bpms {
                            if !bpm.is_finite() || bpm <= 0.0 {
                                continue;
                            }
                            min_bpm = min_bpm.min(bpm);
                            max_bpm = max_bpm.max(bpm);
                        }
                        min_bpm.is_finite() && max_bpm - min_bpm > 3.0
                    }
            },
            possible_grade_points: 0,
            holds_total: 0,
            rolls_total: 0,
            mines_total: 0,
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
    fn tick_status_matches_mode() {
        assert_eq!(tick_mode_status_line(TickMode::Off), None);
        assert_eq!(tick_mode_status_line(TickMode::Assist), Some("Assist Tick"));
        assert_eq!(tick_mode_status_line(TickMode::Hit), Some("Hit Tick"));
    }

    #[test]
    fn song_clock_reconstructs_past_edge_time() {
        let base = Instant::now();
        let snapshot = SongClockSnapshot {
            song_time: 120.0,
            seconds_per_second: 1.5,
            valid_at: base + Duration::from_millis(24),
            valid_at_host_nanos: 0,
        };
        let edge_time = music_time_from_song_clock(snapshot, base, 0);
        assert!((edge_time - 119.964).abs() < 0.000_5);
    }

    #[test]
    fn song_clock_handles_future_edge_time() {
        let base = Instant::now();
        let snapshot = SongClockSnapshot {
            song_time: 64.0,
            seconds_per_second: 2.0,
            valid_at: base,
            valid_at_host_nanos: 0,
        };
        let edge_time = music_time_from_song_clock(snapshot, base + Duration::from_millis(5), 0);
        assert!((edge_time - 64.01).abs() < 0.000_5);
    }

    #[test]
    fn song_clock_prefers_host_clock_when_available() {
        let snapshot = SongClockSnapshot {
            song_time: 32.0,
            seconds_per_second: 1.0,
            valid_at: Instant::now(),
            valid_at_host_nanos: 2_000_000_000,
        };
        let edge_time = music_time_from_song_clock(snapshot, Instant::now(), 1_997_000_000);
        assert!((edge_time - 31.997).abs() < 0.000_5);
    }

    #[test]
    fn display_clock_snaps_on_first_update() {
        let mut display_clock = FrameStableDisplayClock::new(10.0);
        let display_time =
            frame_stable_display_music_time(&mut display_clock, 12.5, 0.001, 1.0, true);
        assert!((display_time - 12.5).abs() < 0.000_5);
    }

    #[test]
    fn display_clock_advances_smoothly_toward_target() {
        let mut display_clock = FrameStableDisplayClock::new(100.0);
        let display_time =
            frame_stable_display_music_time(&mut display_clock, 100.004, 0.001, 1.0, false);
        assert!(display_time > 100.0);
        assert!(display_time < 100.004);
    }

    #[test]
    fn display_clock_snaps_back_when_far_from_target() {
        let mut display_clock = FrameStableDisplayClock::new(100.0);
        let display_time =
            frame_stable_display_music_time(&mut display_clock, 100.250, 0.001, 1.0, false);
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
    fn scored_hold_totals_with_carry_include_prior_let_go() {
        assert_eq!(scored_hold_totals_with_carry(3, 2, 4, 5), (7, 14));
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

        assert!(!score_valid_for_chart(
            &chart,
            &profile,
            profile.scroll_speed,
            1.0,
        ));
    }

    #[test]
    fn score_valid_keeps_turn_options_rankable() {
        let mut profile = profile::Profile::default();
        profile.turn_option = profile::TurnOption::Mirror;
        let chart = test_chart(ArrowStats::default(), TimingSegments::default(), None);

        assert!(score_valid_for_chart(
            &chart,
            &profile,
            profile.scroll_speed,
            1.0,
        ));
    }

    #[test]
    fn score_valid_rejects_cmod_on_significant_timing_changes() {
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

        assert!(!score_valid_for_chart(
            &chart,
            &profile,
            profile.scroll_speed,
            1.0,
        ));
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

        assert!(!score_valid_for_chart(
            &chart,
            &profile,
            profile.scroll_speed,
            1.0,
        ));
    }

    #[test]
    fn cmod_stop_lookahead_uses_time_not_frozen_beat() {
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

        assert!((lookahead_beat - stop_beat).abs() < 0.000_5);
        assert_eq!(partition_notes_before_time(&[note_time], lookahead_time), 1);
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
}
