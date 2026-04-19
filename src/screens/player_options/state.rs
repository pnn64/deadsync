use super::*;

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
    // For Scroll row: bitmask of which options are enabled.
    // 0 => Normal scroll (no special modifier).
    pub scroll_active_mask: [u8; PLAYER_SLOTS],
    // For Hide row: bitmask of which options are enabled.
    // bit0 = Targets, bit1 = Background, bit2 = Combo, bit3 = Life,
    // bit4 = Score, bit5 = Danger, bit6 = Combo Explosions.
    pub hide_active_mask: [u8; PLAYER_SLOTS],
    // For FA+ Options row: bitmask of which options are enabled.
    // bit0 = Display FA+ Window, bit1 = Display EX Score, bit2 = Display H.EX Score,
    // bit3 = Display FA+ Pane, bit4 = 10ms Blue Window, bit5 = 15/10ms Split.
    pub fa_plus_active_mask: [u8; PLAYER_SLOTS],
    // For Early Decent/Way Off Options row: bitmask of which options are enabled.
    // bit0 = Hide Judgments, bit1 = Hide NoteField Flash.
    pub early_dw_active_mask: [u8; PLAYER_SLOTS],
    // For Gameplay Extras row: bitmask of which options are enabled.
    // bit0 = Flash Column for Miss, bit1 = Density Graph at Top,
    // bit2 = Column Cues, bit3 = Display Scorebox.
    pub gameplay_extras_active_mask: [u8; PLAYER_SLOTS],
    // For Gameplay Extras (More) row: bitmask of which options are enabled.
    // bit0 = Column Cues, bit1 = Display Scorebox.
    pub gameplay_extras_more_active_mask: [u8; PLAYER_SLOTS],
    // For Results Extras row: bitmask of which options are enabled.
    // bit0 = Track Early Judgments.
    pub results_extras_active_mask: [u8; PLAYER_SLOTS],
    // For Life Bar Options row: bitmask of which options are enabled.
    // bit0 = Rainbow Max, bit1 = Responsive Colors, bit2 = Show Life Percentage.
    pub life_bar_options_active_mask: [u8; PLAYER_SLOTS],
    // For Error Bar row: bitmask of which options are enabled.
    // bit0 = Colorful, bit1 = Monochrome, bit2 = Text, bit3 = Highlight, bit4 = Average.
    pub error_bar_active_mask: [u8; PLAYER_SLOTS],
    // For Error Bar Options row: bitmask of which options are enabled.
    // bit0 = Move Up, bit1 = Multi-Tick (Simply Love semantics).
    pub error_bar_options_active_mask: [u8; PLAYER_SLOTS],
    // For Measure Counter Options row: bitmask of which options are enabled.
    // bit0 = Move Left, bit1 = Move Up, bit2 = Vertical Lookahead,
    // bit3 = Broken Run Total, bit4 = Run Timer.
    pub measure_counter_options_active_mask: [u8; PLAYER_SLOTS],
    // For Insert row: bitmask of enabled chart insert transforms.
    // bit0 = Wide, bit1 = Big, bit2 = Quick, bit3 = BMRize,
    // bit4 = Skippy, bit5 = Echo, bit6 = Stomp.
    pub insert_active_mask: [u8; PLAYER_SLOTS],
    // For Remove row: bitmask of enabled chart removal transforms.
    // bit0 = Little, bit1 = No Mines, bit2 = No Holds, bit3 = No Jumps,
    // bit4 = No Hands, bit5 = No Quads, bit6 = No Lifts, bit7 = No Fakes.
    pub remove_active_mask: [u8; PLAYER_SLOTS],
    // For Holds row: bitmask of enabled hold transforms.
    // bit0 = Planted, bit1 = Floored, bit2 = Twister,
    // bit3 = No Rolls, bit4 = Holds To Rolls.
    pub holds_active_mask: [u8; PLAYER_SLOTS],
    // For Accel Effects row: bitmask of enabled acceleration transforms.
    // bit0 = Boost, bit1 = Brake, bit2 = Wave, bit3 = Expand, bit4 = Boomerang.
    pub accel_effects_active_mask: [u8; PLAYER_SLOTS],
    // For Visual Effects row: bitmask of enabled visual transforms.
    // bit0 = Drunk, bit1 = Dizzy, bit2 = Confusion, bit3 = Big,
    // bit4 = Flip, bit5 = Invert, bit6 = Tornado, bit7 = Tipsy,
    // bit8 = Bumpy, bit9 = Beat.
    pub visual_effects_active_mask: [u16; PLAYER_SLOTS],
    // For Appearance Effects row: bitmask of enabled appearance transforms.
    // bit0 = Hidden, bit1 = Sudden, bit2 = Stealth, bit3 = Blink, bit4 = R.Vanish.
    pub appearance_effects_active_mask: [u8; PLAYER_SLOTS],
    pub active_color_index: i32,
    pub speed_mod: [SpeedMod; PLAYER_SLOTS],
    pub music_rate: f32,
    pub current_pane: OptionsPane,
    pub scroll_focus_player: usize,
    pub(super) bg: heart_bg::State,
    pub nav_key_held_direction: [Option<NavDirection>; PLAYER_SLOTS],
    pub nav_key_held_since: [Option<Instant>; PLAYER_SLOTS],
    pub nav_key_last_scrolled_at: [Option<Instant>; PLAYER_SLOTS],
    pub start_held_since: [Option<Instant>; PLAYER_SLOTS],
    pub start_last_triggered_at: [Option<Instant>; PLAYER_SLOTS],
    pub(super) allow_per_player_global_offsets: bool,
    pub player_profiles: [crate::game::profile::Profile; PLAYER_SLOTS],
    pub(super) noteskin_cache: HashMap<String, Arc<Noteskin>>,
    pub(super) noteskin: [Option<Arc<Noteskin>>; PLAYER_SLOTS],
    pub(super) mine_noteskin: [Option<Arc<Noteskin>>; PLAYER_SLOTS],
    pub(super) receptor_noteskin: [Option<Arc<Noteskin>>; PLAYER_SLOTS],
    pub(super) tap_explosion_noteskin: [Option<Arc<Noteskin>>; PLAYER_SLOTS],
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
