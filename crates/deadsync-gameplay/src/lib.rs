use deadsync_chart::song::sync_pref_offset;
use deadsync_chart::{ChartData, ChartDisplayBpm, GameplayChartData, SongData, SyncPref};
use deadsync_core::input::{InputSource, Lane, MAX_COLS, MAX_PLAYERS};
use deadsync_core::note::NoteType;
use deadsync_core::song_time::{
    INVALID_SONG_TIME_NS, SongTimeNs, clamp_song_time_ns, normalized_song_rate,
    scaled_song_delta_ns, scaled_song_time_ns, song_time_ns_add_seconds,
    song_time_ns_delta_seconds, song_time_ns_from_seconds, song_time_ns_invalid,
    song_time_ns_span_seconds, song_time_ns_to_seconds,
};
use deadsync_core::timing::{ROWS_PER_BEAT, beat_to_note_row};
use deadsync_input::{
    INPUT_SLOT_INVALID, InputEdge as GameplayInputEdge, InputEvent, VirtualAction,
    lane_from_action, lane_from_column,
};
use deadsync_rules::combo::{self, ComboState, ComboUpdate};
use deadsync_rules::judgment::{self, JudgeGrade, Judgment, TimingWindow};
pub use deadsync_rules::note::NoteCountStat;
use deadsync_rules::note::{
    HoldData, HoldResult, MAX_HOLD_LIFE, MineResult, Note, TIMING_WINDOW_SECONDS_HOLD,
    TIMING_WINDOW_SECONDS_ROLL, advance_hold_last_held, advance_hold_life_ns,
    recompute_player_totals,
};
use deadsync_rules::scroll::ScrollSpeedSetting;
use deadsync_rules::stream::{
    StreamSegment, measure_densities, stream_sequences_threshold, zmod_stream_totals_full_measures,
};
use deadsync_rules::timing::{
    BeatInfoCache, FA_PLUS_W0_MS, FA_PLUS_W010_MS, TimingData, TimingProfile, TimingProfileNs,
    TimingSegments, WindowCounts, classify_offset_ns_with_disabled_windows,
    largest_enabled_tap_window_ns,
};
use std::collections::{BTreeMap, VecDeque};
use std::hash::Hasher;
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::{
    Arc,
    atomic::{AtomicI64, AtomicU64, Ordering},
};
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
pub const OFFSET_DELTA_EPSILON_SECONDS: f32 = 0.000_001;
// Simply Love: ScreenGameplay out.lua (sleep 0.5, linear 1.0).
const GIVE_UP_OUT_FADE_DELAY_SECONDS: f32 = 0.5;
const GIVE_UP_OUT_FADE_SECONDS: f32 = 1.0;
// Simply Love: _fade out normal.lua (sleep 0.1, linear 0.4).
const BACK_OUT_FADE_DELAY_SECONDS: f32 = 0.1;
const BACK_OUT_FADE_SECONDS: f32 = 0.4;
pub const GAMEPLAY_INPUT_BACKLOG_WARN: usize = 128;
pub const GAMEPLAY_INPUT_LATENCY_WARN_US: u32 = 2_000;
pub const ASSIST_TICK_SFX_PATH: &str = "assets/sounds/assist_tick.ogg";
pub const GAMEPLAY_TRACE_SUMMARY_INTERVAL_S: f32 = 1.0;
pub const GAMEPLAY_TRACE_SLOW_FRAME_US: u32 = 4_000;
pub const GAMEPLAY_TRACE_PHASE_SPIKE_US: u32 = 1_000;
pub const UNMAPPED_INPUT_CLOCK_WARN_INTERVAL_NS: SongTimeNs = 1_000_000_000;
pub const UNMAPPED_INPUT_CLOCK_WARN_NEVER_NS: SongTimeNs = i64::MIN;
pub const MAX_ACTIVE_INPUT_SLOTS: usize = 128;
pub const AUTOSYNC_OFFSET_SAMPLE_COUNT: usize = 24;
pub const AUTOSYNC_STDDEV_MAX_SECONDS: f32 = 0.03;
pub const M_MOD_HIGH_CAP: f32 = 600.0;
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

include!("input_state.rs");
include!("trace.rs");
include!("profile_session.rs");
include!("input_slots.rs");
include!("autosync.rs");
include!("display_clock.rs");
include!("controls.rs");
include!("raw_keys.rs");
include!("life.rs");
include!("viewport.rs");
include!("density_graph.rs");
include!("song_lua_windows.rs");
include!("mod_effects.rs");
include!("score_validity.rs");
include!("course_display.rs");
include!("score_display.rs");
include!("attacks.rs");
include!("runtime_config.rs");
include!("cues.rs");
include!("player.rs");
include!("judgment_input.rs");
include!("feedback.rs");
include!("holds.rs");
include!("note_timing.rs");
include!("rows.rs");
include!("chart_transforms.rs");
include!("mines.rs");
include!("replay.rs");
include!("error_bar.rs");
include!("runtime_state.rs");
include!("runtime_init.rs");
include!("runtime_methods.rs");
include!("runtime_update.rs");
include!("effective.rs");
include!("tests.rs");
