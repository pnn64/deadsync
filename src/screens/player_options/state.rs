use super::*;
pub use crate::game::profile::{
    AccelEffectsMask, AppearanceEffectsMask, ErrorBarMask, HoldsMask, InsertMask, RemoveMask,
    VisualEffectsMask,
};
use bitflags::bitflags;

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
    }
}

bitflags! {
    /// Active toggles for the FA+ Options row.
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
        const HIDE_JUDGMENTS = 1 << 0;
        const HIDE_FLASH     = 1 << 1;
    }
}

bitflags! {
    /// Active toggles for the Gameplay Extras row.
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
    pub struct GameplayExtrasMask: u8 {
        const FLASH_COLUMN_FOR_MISS = 1 << 0;
        const DENSITY_GRAPH_AT_TOP  = 1 << 1;
        const COLUMN_CUES           = 1 << 2;
        const DISPLAY_SCOREBOX      = 1 << 3;
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
    pub gameplay_extras: GameplayExtrasMask,
    pub gameplay_extras_more: GameplayExtrasMoreMask,
    pub results_extras: ResultsExtrasMask,
    pub life_bar_options: LifeBarOptionsMask,
    pub error_bar: ErrorBarMask,
    pub error_bar_options: ErrorBarOptionsMask,
    pub measure_counter_options: MeasureCounterOptionsMask,
}

impl PlayerOptionMasks {
    /// Field-wise bitwise OR of two mask sets. Used to accumulate the partial
    /// results of `apply_profile_defaults` across the Main/Advanced/Uncommon
    /// panes (each pane only populates the masks for rows it contains; the
    /// rest are left at `Default::default()` and are identity under OR).
    #[inline]
    pub fn merge(self, other: Self) -> Self {
        Self {
            scroll: self.scroll | other.scroll,
            hide: self.hide | other.hide,
            insert: self.insert | other.insert,
            remove: self.remove | other.remove,
            holds: self.holds | other.holds,
            accel_effects: self.accel_effects | other.accel_effects,
            visual_effects: self.visual_effects | other.visual_effects,
            appearance_effects: self.appearance_effects | other.appearance_effects,
            fa_plus: self.fa_plus | other.fa_plus,
            early_dw: self.early_dw | other.early_dw,
            gameplay_extras: self.gameplay_extras | other.gameplay_extras,
            gameplay_extras_more: self.gameplay_extras_more | other.gameplay_extras_more,
            results_extras: self.results_extras | other.results_extras,
            life_bar_options: self.life_bar_options | other.life_bar_options,
            error_bar: self.error_bar | other.error_bar,
            error_bar_options: self.error_bar_options | other.error_bar_options,
            measure_counter_options: self.measure_counter_options | other.measure_counter_options,
        }
    }
}

/// Loaded noteskin previews for a single player slot.
///
/// Stored as `[PlayerNoteskinPreviews; PLAYER_SLOTS]` on `State` (one entry
/// per player slot).
#[derive(Clone, Default)]
pub(super) struct PlayerNoteskinPreviews {
    pub(super) base: Option<Arc<Noteskin>>,
    pub(super) mine: Option<Arc<Noteskin>>,
    pub(super) receptor: Option<Arc<Noteskin>>,
    pub(super) tap_explosion: Option<Arc<Noteskin>>,
}

/// Per-player navigation key hold/repeat timing.
///
/// Stored as `[PlayerNavInput; PLAYER_SLOTS]` on `State`.
#[derive(Clone, Copy, Debug, Default)]
pub struct PlayerNavInput {
    pub held_direction: Option<NavDirection>,
    pub held_since: Option<Instant>,
    pub last_scrolled_at: Option<Instant>,
}

/// Per-player Start button hold/repeat timing.
///
/// Stored as `[PlayerStartInput; PLAYER_SLOTS]` on `State`.
#[derive(Clone, Copy, Debug, Default)]
pub struct PlayerStartInput {
    pub held_since: Option<Instant>,
    pub last_triggered_at: Option<Instant>,
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
    pub scroll_focus_player: usize,
    pub(super) bg: heart_bg::State,
    pub nav_input: [PlayerNavInput; PLAYER_SLOTS],
    pub start_input: [PlayerStartInput; PLAYER_SLOTS],
    pub(super) allow_per_player_global_offsets: bool,
    pub player_profiles: [crate::game::profile::Profile; PLAYER_SLOTS],
    pub(super) noteskin_cache: HashMap<String, Arc<Noteskin>>,
    pub(super) noteskin_previews: [PlayerNoteskinPreviews; PLAYER_SLOTS],
    pub(super) preview_time: f32,
    pub(super) preview_beat: f32,
    pub(super) help_anim_time: [f32; PLAYER_SLOTS],
    // Combo preview state (for Combo Font row)
    pub(super) combo_preview_count: u32,
    pub(super) combo_preview_elapsed: f32,
    pub(super) pane_transition: PaneTransition,
    pub(super) menu_lr_chord: screen_input::MenuLrChordTracker,
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
