use super::*;
use bitflags::bitflags;
pub use deadsync_profile::{
    AccelEffectsMask, AppearanceEffectsMask, ColumnFlashMask, ErrorBarMask, HoldsMask, InsertMask,
    LiveTimingStatsMask, RemoveMask, StepStatisticsMask, TapExplosionMask, VisualEffectsMask,
};

pub const STEP_STATISTICS_ROW_WIDTH: u8 = 7;

#[inline(always)]
pub fn step_statistics_choice_bits(mask: StepStatisticsMask) -> u16 {
    let mut bits = 0u16;
    if mask.contains(StepStatisticsMask::DENSITY_GRAPH) {
        bits |= 1 << 0;
    }
    if mask.contains(StepStatisticsMask::SONG_BANNER) {
        bits |= 1 << 1;
    }
    if mask.contains(StepStatisticsMask::JUDGMENT_COUNTER) {
        bits |= 1 << 2;
    }
    if mask.contains(StepStatisticsMask::SONG_DURATION) {
        bits |= 1 << 3;
    }
    if mask.pack_info_enabled() {
        bits |= 1 << 4;
    }
    if mask.contains(StepStatisticsMask::STEP_COUNTS) {
        bits |= 1 << 5;
    }
    if mask.contains(StepStatisticsMask::PEAK_NPS) {
        bits |= 1 << 6;
    }
    bits
}

#[inline(always)]
pub fn step_statistics_mask_from_choice_bits(bits: u32) -> StepStatisticsMask {
    let mut mask = StepStatisticsMask::empty();
    if bits & (1 << 0) != 0 {
        mask.insert(StepStatisticsMask::DENSITY_GRAPH);
    }
    if bits & (1 << 1) != 0 {
        mask.insert(StepStatisticsMask::SONG_BANNER);
    }
    if bits & (1 << 2) != 0 {
        mask.insert(StepStatisticsMask::JUDGMENT_COUNTER);
    }
    if bits & (1 << 3) != 0 {
        mask.insert(StepStatisticsMask::SONG_DURATION);
    }
    if bits & (1 << 4) != 0 {
        mask.insert(StepStatisticsMask::PACK_BANNER);
    }
    if bits & (1 << 5) != 0 {
        mask.insert(StepStatisticsMask::STEP_COUNTS);
    }
    if bits & (1 << 6) != 0 {
        mask.insert(StepStatisticsMask::PEAK_NPS);
    }
    mask
}

bitflags! {
    /// Active modifiers for the Scroll row.
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
    pub struct ScrollMask: u8 {
        const REVERSE   = 1 << 0;
        const SPLIT     = 1 << 1;
        const ALTERNATE = 1 << 2;
        const CROSS     = 1 << 3;
        const CENTERED  = 1 << 4;
    }
}

bitflags! {
    /// Active toggles for the Hide row.
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
    pub struct HideMask: u8 {
        const TARGETS          = 1 << 0;
        const BACKGROUND       = 1 << 1;
        const COMBO            = 1 << 2;
        const LIFE             = 1 << 3;
        const SCORE            = 1 << 4;
        const DANGER           = 1 << 5;
        const COMBO_EXPLOSIONS = 1 << 6;
        const USERNAME         = 1 << 7;
    }
}

bitflags! {
    /// Active toggles for the FA+ Options rows.
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
    pub struct FaPlusMask: u8 {
        const WINDOW           = 1 << 0;
        const EX_SCORE         = 1 << 1;
        const HARD_EX_SCORE    = 1 << 2;
        const PANE             = 1 << 3;
        const BLUE_WINDOW_10MS = 1 << 4;
        const SPLIT_15_10MS    = 1 << 5;
    }
}

bitflags! {
    /// Active toggles for the Early Decent / Way Off Options row.
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
    pub struct EarlyDwMask: u8 {
        const HIDE_JUDGMENTS    = 1 << 0;
        const HIDE_FLASH        = 1 << 1;
        const HIDE_COLUMN_FLASH = 1 << 2;
    }
}

bitflags! {
    /// Active toggles for the Gameplay Extras row.
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
    pub struct GameplayExtrasMask: u16 {
        const FLASH_COLUMN_FOR_MISS = 1 << 0;
        const DENSITY_GRAPH_AT_TOP  = 1 << 1;
        const COLUMN_CUES           = 1 << 2;
        const MEASURE_CUES          = 1 << 3;
        const LIVE_TIMING_STATS     = 1 << 4;
        const COLUMN_COUNTDOWN      = 1 << 5;
        const DISPLAY_SCOREBOX      = 1 << 6;
    }
}

bitflags! {
    /// Active toggles for the Gameplay Extras (More) row.
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
    pub struct GameplayExtrasMoreMask: u8 {
        const COLUMN_CUES      = 1 << 0;
        const DISPLAY_SCOREBOX = 1 << 1;
    }
}

bitflags! {
    /// Active toggles for the Results Extras row.
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
    pub struct ResultsExtrasMask: u8 {
        const TRACK_EARLY_JUDGMENTS = 1 << 0;
        const SCALE_SCATTERPLOT     = 1 << 1;
        const DIM_POST_FAIL_SCATTER = 1 << 2;
    }
}

bitflags! {
    /// Active toggles for the Life Bar Options row.
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
    pub struct LifeBarOptionsMask: u8 {
        const RAINBOW_MAX       = 1 << 0;
        const RESPONSIVE_COLORS = 1 << 1;
        const SHOW_LIFE_PERCENT = 1 << 2;
    }
}

bitflags! {
    /// Active toggles for the Error Bar Options row.
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
    pub struct ErrorBarOptionsMask: u8 {
        const MOVE_UP    = 1 << 0;
        const MULTI_TICK = 1 << 1;
    }
}

bitflags! {
    /// Active toggles for the Measure Counter Options row.
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
    pub struct MeasureCounterOptionsMask: u8 {
        const MOVE_LEFT          = 1 << 0;
        const MOVE_UP            = 1 << 1;
        const VERTICAL_LOOKAHEAD = 1 << 2;
        const BROKEN_RUN_TOTAL   = 1 << 3;
        const RUN_TIMER          = 1 << 4;
    }
}

/// All per-player active bitmasks for option rows.
///
/// Stored as `[PlayerOptionMasks; PLAYER_SLOTS]` on `State` (one entry per
/// player slot). Adding a new mask row only requires adding one field here.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PlayerOptionMasks {
    pub scroll: ScrollMask,
    pub hide: HideMask,
    pub insert: InsertMask,
    pub remove: RemoveMask,
    pub holds: HoldsMask,
    pub accel_effects: AccelEffectsMask,
    pub visual_effects: VisualEffectsMask,
    pub appearance_effects: AppearanceEffectsMask,
    pub fa_plus: FaPlusMask,
    pub early_dw: EarlyDwMask,
    pub step_statistics: StepStatisticsMask,
    pub gameplay_extras: GameplayExtrasMask,
    pub column_flash: ColumnFlashMask,
    pub live_timing_stats: LiveTimingStatsMask,
    pub gameplay_extras_more: GameplayExtrasMoreMask,
    pub results_extras: ResultsExtrasMask,
    pub life_bar_options: LifeBarOptionsMask,
    pub error_bar: ErrorBarMask,
    pub error_bar_options: ErrorBarOptionsMask,
    pub measure_counter_options: MeasureCounterOptionsMask,
    pub tap_explosion: TapExplosionMask,
}

/// Loaded noteskin previews for a single player slot.
///
/// Stored as `[PlayerNoteskinPreviews; PLAYER_SLOTS]` on `NoteskinState` (one
/// entry per player slot).
#[derive(Clone, Default)]
pub(super) struct PlayerNoteskinPreviews {
    pub(super) base: Option<Arc<Noteskin>>,
    pub(super) mine: Option<Arc<Noteskin>>,
    pub(super) receptor: Option<Arc<Noteskin>>,
    pub(super) tap_explosion: Option<Arc<Noteskin>>,
}

/// Screen-lifetime noteskin preview cache owned by the app/game thread.
///
/// This is single-thread-only and is warmed with the complete noteskin catalog
/// before the first Player Options frame, while "Entering Options..." remains
/// visible. Its capacity is bounded by the catalog plus profile-only fallback
/// names. Normal option changes only clone cached `Arc`s; no disk access,
/// parsing, pruning, or eviction occurs on a live screen frame. Entries are
/// destroyed when the screen state is dropped; the shared loader keeps only
/// weak references, so it does not extend that lifetime. The underlying loader
/// reports failed loads through its existing warnings; cache hits need no
/// per-frame instrumentation because their worst-case work is a bounded hash
/// lookup and `Arc` clone.
pub(super) struct NoteskinState {
    pub(super) cache: HashMap<String, Arc<Noteskin>>,
    pub(super) previews: [PlayerNoteskinPreviews; PLAYER_SLOTS],
}

/// Per-player navigation key hold/repeat timing.
///
/// Stored as `[PlayerNavInput; PLAYER_SLOTS]` on `State`.
#[derive(Clone, Copy, Debug, Default)]
pub struct PlayerNavInput {
    pub held_direction: Option<NavDirection>,
    pub held_for: Duration,
    pub next_repeat_at: Duration,
}

/// Per-player Start button hold/repeat timing.
///
/// Stored as `[PlayerStartInput; PLAYER_SLOTS]` on `State`.
#[derive(Clone, Copy, Debug, Default)]
pub struct PlayerStartInput {
    pub held: bool,
    pub held_for: Duration,
    pub next_repeat_at: Duration,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct RowTween {
    pub(super) from_y: f32,
    pub(super) to_y: f32,
    pub(super) from_a: f32,
    pub(super) to_a: f32,
    pub(super) t: f32,
}

impl RowTween {
    #[inline(always)]
    pub(super) fn y(&self) -> f32 {
        (self.to_y - self.from_y).mul_add(self.t, self.from_y)
    }

    #[inline(always)]
    pub(super) fn a(&self) -> f32 {
        (self.to_a - self.from_a).mul_add(self.t, self.from_a)
    }
}

/// Position + size for the cursor ring tween. Used as both the `from` and
/// `to` endpoints of the cursor's per-player tween.
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct CursorRect {
    pub(super) x: f32,
    pub(super) y: f32,
    pub(super) w: f32,
    pub(super) h: f32,
}

impl CursorRect {
    #[inline(always)]
    pub(super) fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self { x, y, w, h }
    }

    /// Linearly interpolate between `from` and `to` by `t` (component-wise).
    #[inline(always)]
    pub(super) fn lerp(from: Self, to: Self, t: f32) -> Self {
        Self {
            x: (to.x - from.x).mul_add(t, from.x),
            y: (to.y - from.y).mul_add(t, from.y),
            w: (to.w - from.w).mul_add(t, from.w),
            h: (to.h - from.h).mul_add(t, from.h),
        }
    }
}

pub struct State {
    pub song: Arc<SongData>,
    pub return_screen: Screen,
    pub fixed_stepchart: Option<FixedStepchart>,
    pub chart_steps_index: [usize; PLAYER_SLOTS],
    pub chart_difficulty_index: [usize; PLAYER_SLOTS],
    pub(super) panes: [PaneState; OptionsPane::COUNT],
    /// All per-player option bitmasks. See `PlayerOptionMasks` for field meanings.
    pub option_masks: [PlayerOptionMasks; PLAYER_SLOTS],
    pub active_color_index: i32,
    pub speed_mod: [SpeedMod; PLAYER_SLOTS],
    pub music_rate: f32,
    pub current_pane: OptionsPane,
    pub(super) bg: visual_style_bg::State,
    pub nav_input: [PlayerNavInput; PLAYER_SLOTS],
    pub start_input: [PlayerStartInput; PLAYER_SLOTS],
    pub(super) policy: PlayerOptionsPolicyView,
    pub(super) play_style: deadsync_profile::PlayStyle,
    pub(super) active: [bool; PLAYER_SLOTS],
    pub(super) persisted_player_idx: usize,
    pub(super) cols_per_player: usize,
    pub player_options: [deadsync_profile::PlayerOptionsData; PLAYER_SLOTS],
    pub heart_rate_device_ids: [Option<String>; PLAYER_SLOTS],
    pub(super) heart_rate_choice_ids: Vec<Option<String>>,
    pub(super) heart_rate_readings: [HeartRateReadingView; PLAYER_SLOTS],
    pub(super) noteskin: NoteskinState,
    pub(super) preview_time: f32,
    pub(super) preview_beat: f32,
    pub(super) help_anim_time: [f32; PLAYER_SLOTS],
    // Combo preview state (for Combo Font row)
    pub(super) combo_preview_count: u32,
    pub(super) combo_preview_elapsed: f32,
    pub(super) pane_transition: PaneTransition,
    pub(super) menu_lr_chord: screen_input::MenuLrChordTracker,
    /// Ordered runtime work awaiting emission at the input/update boundary.
    pub(super) pending_effects: Vec<ThemeEffect>,
}

/// Per-pane state. Each pane keeps its own row map, cursor, and tween state so
/// switching panes never throws away rebuilt data. `current_pane` on `State`
/// indexes into `State::panes`.
pub struct PaneState {
    pub row_map: RowMap,
    pub selected_row: [usize; PLAYER_SLOTS],
    pub prev_selected_row: [usize; PLAYER_SLOTS],
    pub(super) inline_choice_x: [f32; PLAYER_SLOTS],
    pub(super) arcade_row_focus: [bool; PLAYER_SLOTS],
    pub(super) row_tweens: Vec<RowTween>,
    // Cursor ring tween (StopTweening/BeginTweening parity with ITGmania ScreenOptions::TweenCursor).
    pub(super) cursor_initialized: [bool; PLAYER_SLOTS],
    pub(super) cursor_from: [CursorRect; PLAYER_SLOTS],
    pub(super) cursor_to: [CursorRect; PLAYER_SLOTS],
    pub(super) cursor_t: [f32; PLAYER_SLOTS],
}

impl PaneState {
    pub(super) fn new(row_map: RowMap) -> Self {
        Self {
            row_map,
            selected_row: [0; PLAYER_SLOTS],
            prev_selected_row: [0; PLAYER_SLOTS],
            inline_choice_x: [f32::NAN; PLAYER_SLOTS],
            arcade_row_focus: [false; PLAYER_SLOTS],
            row_tweens: Vec::new(),
            cursor_initialized: [false; PLAYER_SLOTS],
            cursor_from: [CursorRect::default(); PLAYER_SLOTS],
            cursor_to: [CursorRect::default(); PLAYER_SLOTS],
            cursor_t: [1.0; PLAYER_SLOTS],
        }
    }

    /// Reset cursor + per-player navigation state, keeping `row_map` intact.
    /// Used when entering a pane: the row map persists across pane switches,
    /// but cursor position does not.
    pub(super) fn reset_cursor(&mut self) {
        self.selected_row = [0; PLAYER_SLOTS];
        self.prev_selected_row = [0; PLAYER_SLOTS];
        self.inline_choice_x = [f32::NAN; PLAYER_SLOTS];
        self.arcade_row_focus = [false; PLAYER_SLOTS];
        self.cursor_initialized = [false; PLAYER_SLOTS];
        self.cursor_from = [CursorRect::default(); PLAYER_SLOTS];
        self.cursor_to = [CursorRect::default(); PLAYER_SLOTS];
        self.cursor_t = [1.0; PLAYER_SLOTS];
    }
}

impl State {
    #[inline(always)]
    pub(crate) fn pane(&self) -> &PaneState {
        &self.panes[self.current_pane.index()]
    }

    #[inline(always)]
    pub(crate) fn pane_mut(&mut self) -> &mut PaneState {
        &mut self.panes[self.current_pane.index()]
    }
}
