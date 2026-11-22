//! Canonical practice editor state, navigation, selection and hotkey decisions.
//!
//! Display easing, menu contents, sounds and localized feedback belong to the
//! presentation. Feedback is delivered immediately into the caller's existing
//! storage; editor updates do not allocate an intermediate event queue.

use crate::{
    AutosyncMode, GameplayAudioCommand, GameplayOffsetAdjustKey, GameplayProfileData,
    GameplayRawKeyInput, GameplayRuntimeState, GameplayTimingTickMode,
};
use deadsync_input::practice::{PracticeKey, PracticeKeyEvent};
use deadsync_input::{InputEvent, VirtualAction};

type Game<P, O, C, D> = GameplayRuntimeState<P, O, C, D>;

const MIN_CURSOR_BEAT: f32 = 0.0;
const BEAT_EPSILON: f32 = 0.000_1;
const BEATS_PER_MEASURE: f32 = 4.0;
const NAV_REPEAT_DELAY_SECONDS: f32 = 0.375;
const NAV_REPEAT_INTERVAL_SECONDS: f32 = 0.125;
const MAX_NAV_REPEATS_PER_FRAME: usize = 64;
const TAB_FAST_MULTIPLIER: f32 = 4.0;
const MUSIC_RATE_STEP: f32 = 0.01;
const MUSIC_RATE_MIN: f32 = 0.5;
const MUSIC_RATE_MAX: f32 = 3.0;
const RATE_REPEAT_DELAY_SECONDS: f32 = 0.375;
const RATE_REPEAT_INTERVAL_SECONDS: f32 = 0.05;
const MAX_RATE_REPEATS_PER_FRAME: usize = 64;
const SNAP_BEATS: [f32; 9] = [
    1.0,
    0.5,
    1.0 / 3.0,
    0.25,
    1.0 / 6.0,
    0.125,
    1.0 / 12.0,
    1.0 / 16.0,
    1.0 / 48.0,
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PracticeEditorConfig {
    pub only_dedicated_menu_buttons: bool,
    pub three_key_navigation: bool,
    pub tab_acceleration: bool,
}

impl Default for PracticeEditorConfig {
    fn default() -> Self {
        Self {
            only_dedicated_menu_buttons: false,
            three_key_navigation: false,
            tab_acceleration: true,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PracticeEditorSnapshot {
    pub cursor_beat: f32,
    pub selection_anchor: Option<f32>,
    pub selection_end: Option<f32>,
    pub snap_index: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PracticePlayRange {
    WholeSong,
    CurrentToEnd,
    Selection,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PracticeEditorCommand {
    Play(PracticePlayRange),
    SetSelectionStart,
    SetSelectionEnd,
    PlaceAreaMarker,
    ClearSelection,
}

/// Decisions the practice session applies to its menus, layout or playback.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum PracticeEditorRequest {
    #[default]
    None,
    OpenMainMenu,
    OpenHelpMenu,
    CloseMenu,
    ActivateMenu,
    StepMenu(isize),
    ChangeEditScrollSpeed(isize),
    StartPlayback {
        start_beat: f32,
        stop_beat: f32,
    },
    StopPlayback,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PracticeEditorInputResult {
    pub consumed: bool,
    pub request: PracticeEditorRequest,
}

impl PracticeEditorInputResult {
    const fn handled(request: PracticeEditorRequest) -> Self {
        Self {
            consumed: true,
            request,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PracticeEditorSound {
    Line,
    Marker,
    Snap,
    Invalid,
    MenuOpenClose,
    MenuMove,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PracticeEditorFlash {
    AreaMarkerStartSet,
    AreaMarkerEndSet,
    SelectionCleared,
    InvalidSelectionStart,
    SelectionStartSet,
    InvalidSelectionEnd,
    SelectionEndSet,
    MusicRate { rate: f32 },
    MusicRateLimit { rate: f32 },
    Autoplay { enabled: bool },
    TimingTick(GameplayTimingTickMode),
    Autosync(AutosyncMode),
    Offset { global: bool, seconds: f32 },
}

/// Ordered feedback with game facts; asset paths and text remain in the theme.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PracticeEditorFeedback {
    Sound(PracticeEditorSound),
    Flash(PracticeEditorFlash),
    PersistMusicRate(f32),
    SyncMusicRateText,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Direction {
    Up,
    Down,
}

impl Direction {
    const fn opposite(self) -> Self {
        match self {
            Self::Up => Self::Down,
            Self::Down => Self::Up,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum NavMode {
    GameplayButtons,
    DedicatedFiveKey,
    DedicatedThreeKey,
}

#[derive(Debug)]
struct HeldNavigation {
    dir: Option<Direction>,
    up_count: u8,
    down_count: u8,
    delay_left: f32,
    repeat_left: f32,
}

impl HeldNavigation {
    const fn new() -> Self {
        Self {
            dir: None,
            up_count: 0,
            down_count: 0,
            delay_left: 0.0,
            repeat_left: NAV_REPEAT_INTERVAL_SECONDS,
        }
    }

    const fn count(&self, dir: Direction) -> u8 {
        match dir {
            Direction::Up => self.up_count,
            Direction::Down => self.down_count,
        }
    }

    const fn count_mut(&mut self, dir: Direction) -> &mut u8 {
        match dir {
            Direction::Up => &mut self.up_count,
            Direction::Down => &mut self.down_count,
        }
    }

    const fn start(&mut self, dir: Direction) {
        self.dir = Some(dir);
        self.delay_left = NAV_REPEAT_DELAY_SECONDS;
        self.repeat_left = NAV_REPEAT_INTERVAL_SECONDS;
    }

    const fn clear_timer(&mut self) {
        self.dir = None;
        self.delay_left = 0.0;
        self.repeat_left = NAV_REPEAT_INTERVAL_SECONDS;
    }

    const fn clear(&mut self) {
        self.up_count = 0;
        self.down_count = 0;
        self.clear_timer();
    }

    const fn press(&mut self, dir: Direction) {
        let count = self.count_mut(dir);
        *count = count.saturating_add(1);
        self.start(dir);
    }

    fn release(&mut self, dir: Direction) {
        let count = self.count_mut(dir);
        *count = count.saturating_sub(1);
        if self.dir != Some(dir) || self.count(dir) != 0 {
            return;
        }
        let other = dir.opposite();
        if self.count(other) > 0 {
            self.start(other);
        } else {
            self.clear_timer();
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RateDirection {
    Lower,
    Raise,
}

impl RateDirection {
    const fn opposite(self) -> Self {
        match self {
            Self::Lower => Self::Raise,
            Self::Raise => Self::Lower,
        }
    }

    const fn delta(self) -> f32 {
        match self {
            Self::Lower => -MUSIC_RATE_STEP,
            Self::Raise => MUSIC_RATE_STEP,
        }
    }
}

/// Editor controls shared by every practice presentation.
#[derive(Debug)]
pub struct PracticeEditor {
    config: PracticeEditorConfig,
    cursor_beat: f32,
    selection_anchor: Option<f32>,
    selection_end: Option<f32>,
    shift_anchor: Option<f32>,
    snap_index: usize,
    shift_held: bool,
    ctrl_held: bool,
    tab_held: bool,
    cursor_hold: HeldNavigation,
    page_hold: HeldNavigation,
    rate_hold_dir: Option<RateDirection>,
    rate_hold_lower_count: u8,
    rate_hold_raise_count: u8,
    rate_hold_delay_left: f32,
    rate_hold_repeat_left: f32,
}

impl PracticeEditor {
    #[must_use]
    pub const fn new(config: PracticeEditorConfig) -> Self {
        Self {
            config,
            cursor_beat: 0.0,
            selection_anchor: None,
            selection_end: None,
            shift_anchor: None,
            snap_index: 0,
            shift_held: false,
            ctrl_held: false,
            tab_held: false,
            cursor_hold: HeldNavigation::new(),
            page_hold: HeldNavigation::new(),
            rate_hold_dir: None,
            rate_hold_lower_count: 0,
            rate_hold_raise_count: 0,
            rate_hold_delay_left: 0.0,
            rate_hold_repeat_left: RATE_REPEAT_INTERVAL_SECONDS,
        }
    }

    #[must_use]
    pub const fn snapshot(&self) -> PracticeEditorSnapshot {
        PracticeEditorSnapshot {
            cursor_beat: self.cursor_beat,
            selection_anchor: self.selection_anchor,
            selection_end: self.selection_end,
            snap_index: self.snap_index,
        }
    }

    #[must_use]
    pub const fn cursor_beat(&self) -> f32 {
        self.cursor_beat
    }

    #[must_use]
    pub const fn selection_anchor(&self) -> Option<f32> {
        self.selection_anchor
    }

    #[must_use]
    pub const fn selection_end(&self) -> Option<f32> {
        self.selection_end
    }

    #[must_use]
    pub const fn snap_index(&self) -> usize {
        self.snap_index
    }

    #[must_use]
    pub const fn snap_step(&self) -> f32 {
        SNAP_BEATS[self.snap_index]
    }

    #[must_use]
    pub fn selection_range(&self) -> Option<(f32, f32)> {
        let a = self.selection_anchor?;
        let b = self.selection_end?;
        Some((a.min(b), a.max(b)))
    }

    /// Restoring an editor position never restores pressed keys or repeat timers.
    pub fn restore<P: GameplayProfileData, O, C, D>(
        &mut self,
        game: &Game<P, O, C, D>,
        snapshot: PracticeEditorSnapshot,
    ) {
        self.clear_navigation_holds();
        self.shift_anchor = None;
        self.shift_held = false;
        self.ctrl_held = false;
        self.tab_held = false;
        self.clear_rate_hold_inputs();
        self.snap_index = snapshot.snap_index.min(SNAP_BEATS.len().saturating_sub(1));
        (self.selection_anchor, self.selection_end) = clamp_selection(
            snapshot.selection_anchor,
            snapshot.selection_end,
            max_play_beat(game),
        );
        self.set_cursor(game, snapshot.cursor_beat);
    }

    pub fn set_cursor<P: GameplayProfileData, O, C, D>(
        &mut self,
        game: &Game<P, O, C, D>,
        beat: f32,
    ) {
        self.cursor_beat = beat.clamp(MIN_CURSOR_BEAT, max_play_beat(game));
    }

    /// Playback transitions and menu entry retain the existing cursor-then-page
    /// cleanup order and deliberately leave music-rate holds active.
    pub const fn clear_navigation_holds(&mut self) {
        self.cursor_hold.clear();
        self.page_hold.clear();
    }

    #[must_use]
    pub fn playback_range<P: GameplayProfileData, O, C, D>(
        &self,
        game: &Game<P, O, C, D>,
        request: PracticePlayRange,
    ) -> (f32, f32) {
        match request {
            PracticePlayRange::WholeSong => (MIN_CURSOR_BEAT, max_play_beat(game)),
            PracticePlayRange::CurrentToEnd => (self.cursor_beat, max_play_beat(game)),
            PracticePlayRange::Selection => {
                let (start, stop) = self
                    .selection_range()
                    .filter(|(start, stop)| stop > start)
                    .or_else(|| {
                        self.selection_anchor
                            .map(|start| (start, max_play_beat(game)))
                    })
                    .unwrap_or_else(|| (self.cursor_beat, max_play_beat(game)));
                (start, stop.max(start + self.snap_step()))
            }
        }
    }

    pub fn execute<P: GameplayProfileData, O, C, D>(
        &mut self,
        game: &Game<P, O, C, D>,
        command: PracticeEditorCommand,
        mut feedback: impl FnMut(&Game<P, O, C, D>, PracticeEditorFeedback),
    ) -> PracticeEditorRequest {
        match command {
            PracticeEditorCommand::Play(range) => {
                let (start_beat, stop_beat) = self.playback_range(game, range);
                return PracticeEditorRequest::StartPlayback {
                    start_beat,
                    stop_beat,
                };
            }
            PracticeEditorCommand::SetSelectionStart => {
                self.set_selection_start(game, &mut feedback);
            }
            PracticeEditorCommand::SetSelectionEnd => {
                self.set_selection_end(game, &mut feedback);
            }
            PracticeEditorCommand::PlaceAreaMarker => self.place_area_marker(game, &mut feedback),
            PracticeEditorCommand::ClearSelection => self.clear_selection(game, &mut feedback),
        }
        PracticeEditorRequest::None
    }

    /// Route editor and menu logical inputs. Playback inputs have their own
    /// PracticePlayback owner and must only reach this method when editing.
    pub fn handle_input<P: GameplayProfileData, O, C, D>(
        &mut self,
        game: &Game<P, O, C, D>,
        event: &InputEvent,
        menu_open: bool,
        mut feedback: impl FnMut(&Game<P, O, C, D>, PracticeEditorFeedback),
    ) -> PracticeEditorRequest {
        if menu_open {
            if !event.pressed {
                return PracticeEditorRequest::None;
            }
            if let Some(delta) = menu_step_delta(self.nav_mode(), event.action) {
                return PracticeEditorRequest::StepMenu(delta);
            }
            return match event.action {
                VirtualAction::p1_start
                | VirtualAction::p2_start
                | VirtualAction::p1_select
                | VirtualAction::p2_select => PracticeEditorRequest::ActivateMenu,
                VirtualAction::p1_back | VirtualAction::p2_back => PracticeEditorRequest::CloseMenu,
                _ => PracticeEditorRequest::None,
            };
        }

        if !event.pressed {
            if let Some(dir) = cursor_direction(self.nav_mode(), event.action) {
                self.cursor_hold.release(dir);
            }
            return PracticeEditorRequest::None;
        }
        if let Some(dir) = cursor_direction(self.nav_mode(), event.action) {
            self.cursor_hold.press(dir);
            self.move_cursor_in_direction(game, dir, &mut feedback);
            return PracticeEditorRequest::None;
        }
        if let Some(delta) = snap_delta(self.nav_mode(), event.action) {
            self.change_snap(game, delta, &mut feedback);
            return PracticeEditorRequest::None;
        }
        match event.action {
            VirtualAction::p1_start
            | VirtualAction::p2_start
            | VirtualAction::p1_back
            | VirtualAction::p2_back => PracticeEditorRequest::OpenMainMenu,
            VirtualAction::p1_select | VirtualAction::p2_select => {
                self.place_area_marker(game, &mut feedback);
                PracticeEditorRequest::None
            }
            _ => PracticeEditorRequest::None,
        }
    }

    /// Preserve global hotkey precedence before playback, menus and edit commands.
    pub fn handle_raw_key<P: GameplayProfileData, O, C, D>(
        &mut self,
        game: &mut Game<P, O, C, D>,
        event: &PracticeKeyEvent,
        playing: bool,
        menu_open: bool,
        mut feedback: impl FnMut(&Game<P, O, C, D>, PracticeEditorFeedback),
    ) -> PracticeEditorInputResult {
        match event.key {
            PracticeKey::Shift => {
                self.shift_held = event.pressed;
                if !event.pressed {
                    self.shift_anchor = None;
                    self.clear_rate_hold_inputs();
                }
                return PracticeEditorInputResult::handled(PracticeEditorRequest::None);
            }
            PracticeKey::Control => {
                self.ctrl_held = event.pressed;
                return PracticeEditorInputResult::handled(PracticeEditorRequest::None);
            }
            PracticeKey::FastScroll => {
                self.tab_held = event.pressed;
                return PracticeEditorInputResult::default();
            }
            _ => {}
        }

        if !event.pressed {
            if let Some(dir) = page_direction(event.key) {
                self.page_hold.release(dir);
                return PracticeEditorInputResult::handled(PracticeEditorRequest::None);
            }
            if let Some(dir) = rate_direction(event.key, event.pressed, self.shift_held) {
                self.release_rate_hold(dir);
                return PracticeEditorInputResult::handled(PracticeEditorRequest::None);
            }
            if let Some(input) = gameplay_hotkey_input(event.key) {
                self.forward_hotkey(game, input, false, event.timestamp, &mut feedback);
                return PracticeEditorInputResult::handled(PracticeEditorRequest::None);
            }
            return PracticeEditorInputResult::default();
        }

        if let Some(input) = gameplay_hotkey_input(event.key) {
            if !event.repeat {
                self.forward_hotkey(game, input, true, event.timestamp, &mut feedback);
            }
            return PracticeEditorInputResult::handled(PracticeEditorRequest::None);
        }
        if let Some(dir) = rate_direction(event.key, event.pressed, self.shift_held) {
            if !event.repeat {
                self.press_rate_hold(game, dir, &mut feedback);
            }
            return PracticeEditorInputResult::handled(PracticeEditorRequest::None);
        }
        if playing {
            return match event.key {
                PracticeKey::Cancel | PracticeKey::Accept => {
                    PracticeEditorInputResult::handled(PracticeEditorRequest::StopPlayback)
                }
                _ => PracticeEditorInputResult::default(),
            };
        }
        if menu_open {
            return match event.key {
                PracticeKey::Cancel => {
                    PracticeEditorInputResult::handled(PracticeEditorRequest::CloseMenu)
                }
                PracticeKey::Accept => {
                    PracticeEditorInputResult::handled(PracticeEditorRequest::ActivateMenu)
                }
                _ => PracticeEditorInputResult::default(),
            };
        }

        let request = match event.key {
            PracticeKey::Cancel | PracticeKey::Accept => PracticeEditorRequest::OpenMainMenu,
            PracticeKey::ScrollSpeedIncrease if self.ctrl_held => {
                PracticeEditorRequest::ChangeEditScrollSpeed(1)
            }
            PracticeKey::ScrollSpeedDecrease if self.ctrl_held => {
                PracticeEditorRequest::ChangeEditScrollSpeed(-1)
            }
            PracticeKey::Play => {
                let range = if self.ctrl_held {
                    PracticePlayRange::WholeSong
                } else if self.shift_held {
                    PracticePlayRange::CurrentToEnd
                } else {
                    PracticePlayRange::Selection
                };
                let (start_beat, stop_beat) = self.playback_range(game, range);
                PracticeEditorRequest::StartPlayback {
                    start_beat,
                    stop_beat,
                }
            }
            PracticeKey::AreaMarker => {
                self.place_area_marker(game, &mut feedback);
                PracticeEditorRequest::None
            }
            PracticeKey::PageUp | PracticeKey::PageDown => {
                if !event.repeat {
                    let dir = match event.key {
                        PracticeKey::PageUp => Direction::Up,
                        _ => Direction::Down,
                    };
                    self.page_hold.press(dir);
                    self.move_cursor_by_page(game, dir, &mut feedback);
                }
                PracticeEditorRequest::None
            }
            PracticeKey::PreviousNote if !self.ctrl_held => {
                self.seek_chart_note(game, Direction::Up, &mut feedback);
                PracticeEditorRequest::None
            }
            PracticeKey::NextNote if !self.ctrl_held => {
                self.seek_chart_note(game, Direction::Down, &mut feedback);
                PracticeEditorRequest::None
            }
            PracticeKey::SongStart => {
                self.set_cursor(game, MIN_CURSOR_BEAT);
                feedback(
                    game,
                    PracticeEditorFeedback::Sound(PracticeEditorSound::Line),
                );
                PracticeEditorRequest::None
            }
            PracticeKey::SongEnd => {
                self.set_cursor(game, max_play_beat(game));
                feedback(
                    game,
                    PracticeEditorFeedback::Sound(PracticeEditorSound::Line),
                );
                PracticeEditorRequest::None
            }
            PracticeKey::Help => PracticeEditorRequest::OpenHelpMenu,
            _ => return PracticeEditorInputResult::default(),
        };
        PracticeEditorInputResult::handled(request)
    }

    /// Rate repeats run before the editor/playback branch, including while a menu
    /// is open. Tab acceleration never changes their time base.
    pub fn update_music_rate_hold<P: GameplayProfileData, O, C, D>(
        &mut self,
        game: &mut Game<P, O, C, D>,
        delta_time: f32,
        mut feedback: impl FnMut(&Game<P, O, C, D>, PracticeEditorFeedback),
    ) {
        if delta_time <= 0.0 {
            return;
        }
        let Some(dir) = self.rate_hold_dir else {
            return;
        };
        if self.rate_hold_count(dir) == 0 {
            self.clear_rate_hold_timer();
            return;
        }

        let mut remaining = delta_time;
        if self.rate_hold_delay_left > 0.0 {
            let elapsed = remaining.min(self.rate_hold_delay_left);
            self.rate_hold_delay_left -= elapsed;
            remaining -= elapsed;
            if self.rate_hold_delay_left > 0.0 {
                return;
            }
            if !self.change_music_rate(game, dir.delta(), &mut feedback) {
                self.stop_rate_hold_dir(dir);
                return;
            }
            self.rate_hold_repeat_left = RATE_REPEAT_INTERVAL_SECONDS;
        }

        self.rate_hold_repeat_left -= remaining;
        let mut repeats = 0;
        while self.rate_hold_repeat_left <= 0.0 && repeats < MAX_RATE_REPEATS_PER_FRAME {
            if !self.change_music_rate(game, dir.delta(), &mut feedback) {
                self.stop_rate_hold_dir(dir);
                return;
            }
            self.rate_hold_repeat_left += RATE_REPEAT_INTERVAL_SECONDS;
            repeats += 1;
        }
    }

    /// Run only during editing, after screen elapsed time and before display
    /// easing. Cursor repeats precede page repeats, including their feedback.
    pub fn update_navigation_holds<P: GameplayProfileData, O, C, D>(
        &mut self,
        game: &Game<P, O, C, D>,
        delta_time: f32,
        menu_open: bool,
        mut feedback: impl FnMut(&Game<P, O, C, D>, PracticeEditorFeedback),
    ) {
        self.update_cursor_hold(game, delta_time, menu_open, &mut feedback);
        self.update_page_hold(game, delta_time, menu_open, &mut feedback);
    }

    const fn nav_mode(&self) -> NavMode {
        if !self.config.only_dedicated_menu_buttons {
            NavMode::GameplayButtons
        } else if self.config.three_key_navigation {
            NavMode::DedicatedThreeKey
        } else {
            NavMode::DedicatedFiveKey
        }
    }

    fn navigation_delta_time(&self, delta_time: f32) -> f32 {
        delta_time
            * if self.tab_held && self.config.tab_acceleration {
                TAB_FAST_MULTIPLIER
            } else {
                1.0
            }
    }

    fn update_cursor_hold<P: GameplayProfileData, O, C, D>(
        &mut self,
        game: &Game<P, O, C, D>,
        delta_time: f32,
        menu_open: bool,
        feedback: &mut impl FnMut(&Game<P, O, C, D>, PracticeEditorFeedback),
    ) {
        if menu_open || self.ctrl_held || delta_time <= 0.0 {
            return;
        }
        let Some(dir) = self.cursor_hold.dir else {
            return;
        };
        if self.cursor_hold.count(dir) == 0 {
            self.cursor_hold.clear_timer();
            return;
        }

        let mut remaining = self.navigation_delta_time(delta_time);
        if self.cursor_hold.delay_left > 0.0 {
            let elapsed = remaining.min(self.cursor_hold.delay_left);
            self.cursor_hold.delay_left -= elapsed;
            remaining -= elapsed;
            if self.cursor_hold.delay_left > 0.0 {
                return;
            }
            self.move_cursor_in_direction(game, dir, feedback);
            self.cursor_hold.repeat_left = NAV_REPEAT_INTERVAL_SECONDS;
        }

        self.cursor_hold.repeat_left -= remaining;
        let mut repeats = 0;
        while self.cursor_hold.repeat_left <= 0.0 && repeats < MAX_NAV_REPEATS_PER_FRAME {
            self.move_cursor_in_direction(game, dir, feedback);
            self.cursor_hold.repeat_left += NAV_REPEAT_INTERVAL_SECONDS;
            repeats += 1;
        }
    }

    fn update_page_hold<P: GameplayProfileData, O, C, D>(
        &mut self,
        game: &Game<P, O, C, D>,
        delta_time: f32,
        menu_open: bool,
        feedback: &mut impl FnMut(&Game<P, O, C, D>, PracticeEditorFeedback),
    ) {
        if menu_open || delta_time <= 0.0 {
            return;
        }
        let Some(dir) = self.page_hold.dir else {
            return;
        };
        if self.page_hold.count(dir) == 0 {
            self.page_hold.clear_timer();
            return;
        }

        let mut remaining = self.navigation_delta_time(delta_time);
        if self.page_hold.delay_left > 0.0 {
            let elapsed = remaining.min(self.page_hold.delay_left);
            self.page_hold.delay_left -= elapsed;
            remaining -= elapsed;
            if self.page_hold.delay_left > 0.0 {
                return;
            }
            self.move_cursor_by_page(game, dir, feedback);
            self.page_hold.repeat_left = NAV_REPEAT_INTERVAL_SECONDS;
        }

        self.page_hold.repeat_left -= remaining;
        let mut repeats = 0;
        while self.page_hold.repeat_left <= 0.0 && repeats < MAX_NAV_REPEATS_PER_FRAME {
            self.move_cursor_by_page(game, dir, feedback);
            self.page_hold.repeat_left += NAV_REPEAT_INTERVAL_SECONDS;
            repeats += 1;
        }
    }

    fn move_cursor_in_direction<P: GameplayProfileData, O, C, D>(
        &mut self,
        game: &Game<P, O, C, D>,
        dir: Direction,
        feedback: &mut impl FnMut(&Game<P, O, C, D>, PracticeEditorFeedback),
    ) {
        let snap = self.snap_step();
        self.move_cursor(
            game,
            match dir {
                Direction::Up => -snap,
                Direction::Down => snap,
            },
            feedback,
        );
    }

    fn move_cursor_by_page<P: GameplayProfileData, O, C, D>(
        &mut self,
        game: &Game<P, O, C, D>,
        dir: Direction,
        feedback: &mut impl FnMut(&Game<P, O, C, D>, PracticeEditorFeedback),
    ) {
        self.move_cursor(
            game,
            match dir {
                Direction::Up => -BEATS_PER_MEASURE,
                Direction::Down => BEATS_PER_MEASURE,
            },
            feedback,
        );
    }

    fn move_cursor<P: GameplayProfileData, O, C, D>(
        &mut self,
        game: &Game<P, O, C, D>,
        delta_beats: f32,
        feedback: &mut impl FnMut(&Game<P, O, C, D>, PracticeEditorFeedback),
    ) {
        let old_beat = self.cursor_beat;
        let next_beat = quantize_beat(old_beat + delta_beats, self.snap_step());
        self.set_cursor(game, next_beat);
        if self.shift_held {
            self.extend_shift_marker(old_beat);
        } else {
            self.shift_anchor = None;
        }
        if !same_beat(old_beat, self.cursor_beat) {
            feedback(
                game,
                PracticeEditorFeedback::Sound(PracticeEditorSound::Line),
            );
        }
    }

    fn extend_shift_marker(&mut self, original_beat: f32) {
        let anchor = *self.shift_anchor.get_or_insert(original_beat);
        if same_beat(anchor, self.cursor_beat) {
            self.selection_anchor = None;
            self.selection_end = None;
        } else {
            self.set_marker_range(anchor, self.cursor_beat);
        }
    }

    fn change_snap<P: GameplayProfileData, O, C, D>(
        &mut self,
        game: &Game<P, O, C, D>,
        delta: isize,
        feedback: &mut impl FnMut(&Game<P, O, C, D>, PracticeEditorFeedback),
    ) {
        let next = self.snap_index as isize + delta;
        if next < 0 || next >= SNAP_BEATS.len() as isize {
            return;
        }
        self.snap_index = next as usize;
        let quantized = quantize_beat(self.cursor_beat, self.snap_step());
        self.set_cursor(game, quantized);
        feedback(
            game,
            PracticeEditorFeedback::Sound(PracticeEditorSound::Snap),
        );
    }

    fn seek_chart_note<P: GameplayProfileData, O, C, D>(
        &mut self,
        game: &Game<P, O, C, D>,
        dir: Direction,
        feedback: &mut impl FnMut(&Game<P, O, C, D>, PracticeEditorFeedback),
    ) {
        let current = self.cursor_beat;
        let target = match dir {
            Direction::Up => game
                .notes()
                .iter()
                .filter(|note| note.can_be_judged && note.beat < current - BEAT_EPSILON)
                .map(|note| note.beat)
                .max_by(f32::total_cmp),
            Direction::Down => game
                .notes()
                .iter()
                .filter(|note| note.can_be_judged && note.beat > current + BEAT_EPSILON)
                .map(|note| note.beat)
                .min_by(f32::total_cmp),
        };
        if let Some(beat) = target {
            self.set_cursor(game, beat);
            feedback(
                game,
                PracticeEditorFeedback::Sound(PracticeEditorSound::Line),
            );
        } else {
            feedback(
                game,
                PracticeEditorFeedback::Sound(PracticeEditorSound::Invalid),
            );
        }
    }

    fn place_area_marker<P: GameplayProfileData, O, C, D>(
        &mut self,
        game: &Game<P, O, C, D>,
        feedback: &mut impl FnMut(&Game<P, O, C, D>, PracticeEditorFeedback),
    ) {
        feedback(
            game,
            PracticeEditorFeedback::Sound(PracticeEditorSound::Marker),
        );
        match (self.selection_anchor, self.selection_end) {
            (None, None) => {
                self.selection_anchor = Some(self.cursor_beat);
                feedback(
                    game,
                    PracticeEditorFeedback::Flash(PracticeEditorFlash::AreaMarkerStartSet),
                );
            }
            (Some(begin), None) => {
                if same_beat(begin, self.cursor_beat) {
                    self.clear_selection(game, feedback);
                } else {
                    self.set_marker_range(begin, self.cursor_beat);
                    feedback(
                        game,
                        PracticeEditorFeedback::Flash(PracticeEditorFlash::AreaMarkerEndSet),
                    );
                }
            }
            _ => {
                self.selection_anchor = Some(self.cursor_beat);
                self.selection_end = None;
                self.shift_anchor = None;
                feedback(
                    game,
                    PracticeEditorFeedback::Flash(PracticeEditorFlash::AreaMarkerStartSet),
                );
            }
        }
    }

    fn set_selection_start<P: GameplayProfileData, O, C, D>(
        &mut self,
        game: &Game<P, O, C, D>,
        feedback: &mut impl FnMut(&Game<P, O, C, D>, PracticeEditorFeedback),
    ) {
        if self
            .selection_end
            .is_some_and(|end| self.cursor_beat >= end)
        {
            feedback(
                game,
                PracticeEditorFeedback::Flash(PracticeEditorFlash::InvalidSelectionStart),
            );
            feedback(
                game,
                PracticeEditorFeedback::Sound(PracticeEditorSound::Invalid),
            );
            return;
        }
        self.selection_anchor = Some(self.cursor_beat);
        feedback(
            game,
            PracticeEditorFeedback::Flash(PracticeEditorFlash::SelectionStartSet),
        );
        feedback(
            game,
            PracticeEditorFeedback::Sound(PracticeEditorSound::Marker),
        );
    }

    fn set_selection_end<P: GameplayProfileData, O, C, D>(
        &mut self,
        game: &Game<P, O, C, D>,
        feedback: &mut impl FnMut(&Game<P, O, C, D>, PracticeEditorFeedback),
    ) {
        if self
            .selection_anchor
            .is_some_and(|start| self.cursor_beat <= start)
        {
            feedback(
                game,
                PracticeEditorFeedback::Flash(PracticeEditorFlash::InvalidSelectionEnd),
            );
            feedback(
                game,
                PracticeEditorFeedback::Sound(PracticeEditorSound::Invalid),
            );
            return;
        }
        self.selection_end = Some(self.cursor_beat);
        feedback(
            game,
            PracticeEditorFeedback::Flash(PracticeEditorFlash::SelectionEndSet),
        );
        feedback(
            game,
            PracticeEditorFeedback::Sound(PracticeEditorSound::Marker),
        );
    }

    fn clear_selection<P: GameplayProfileData, O, C, D>(
        &mut self,
        game: &Game<P, O, C, D>,
        feedback: &mut impl FnMut(&Game<P, O, C, D>, PracticeEditorFeedback),
    ) {
        self.selection_anchor = None;
        self.selection_end = None;
        self.shift_anchor = None;
        feedback(
            game,
            PracticeEditorFeedback::Flash(PracticeEditorFlash::SelectionCleared),
        );
    }

    const fn set_marker_range(&mut self, a: f32, b: f32) {
        self.selection_anchor = Some(a.min(b));
        self.selection_end = Some(a.max(b));
    }

    fn forward_hotkey<P: GameplayProfileData, O, C, D>(
        &self,
        game: &mut Game<P, O, C, D>,
        input: GameplayRawKeyInput,
        pressed: bool,
        timestamp: std::time::Instant,
        feedback: &mut impl FnMut(&Game<P, O, C, D>, PracticeEditorFeedback),
    ) {
        game.set_raw_modifier_state(self.shift_held, self.ctrl_held);
        let now_music_time = game.current_music_time_seconds();
        // Practice has no offset-save prompt, so these commands are always allowed.
        let _ =
            game.handle_queued_raw_key_input(input, None, pressed, timestamp, now_music_time, true);
        if !pressed {
            return;
        }
        let flash = match input {
            GameplayRawKeyInput::Autoplay => PracticeEditorFlash::Autoplay {
                enabled: game.autoplay_enabled(),
            },
            GameplayRawKeyInput::TimingTick => PracticeEditorFlash::TimingTick(game.tick_mode()),
            GameplayRawKeyInput::Autosync => PracticeEditorFlash::Autosync(game.autosync_mode()),
            GameplayRawKeyInput::OffsetAdjust(_) => PracticeEditorFlash::Offset {
                global: self.shift_held,
                seconds: if self.shift_held {
                    game.global_offset_seconds()
                } else {
                    game.song_offset_seconds()
                },
            },
            GameplayRawKeyInput::Restart | GameplayRawKeyInput::Other => return,
        };
        feedback(game, PracticeEditorFeedback::Flash(flash));
    }

    const fn rate_hold_count(&self, dir: RateDirection) -> u8 {
        match dir {
            RateDirection::Lower => self.rate_hold_lower_count,
            RateDirection::Raise => self.rate_hold_raise_count,
        }
    }

    const fn rate_hold_count_mut(&mut self, dir: RateDirection) -> &mut u8 {
        match dir {
            RateDirection::Lower => &mut self.rate_hold_lower_count,
            RateDirection::Raise => &mut self.rate_hold_raise_count,
        }
    }

    fn press_rate_hold<P: GameplayProfileData, O, C, D>(
        &mut self,
        game: &mut Game<P, O, C, D>,
        dir: RateDirection,
        feedback: &mut impl FnMut(&Game<P, O, C, D>, PracticeEditorFeedback),
    ) {
        let count = self.rate_hold_count_mut(dir);
        if *count > 0 {
            return;
        }
        *count = 1;
        self.start_rate_hold(dir);
        if !self.change_music_rate(game, dir.delta(), feedback) {
            self.stop_rate_hold_dir(dir);
        }
    }

    fn release_rate_hold(&mut self, dir: RateDirection) {
        let count = self.rate_hold_count_mut(dir);
        if *count == 0 {
            return;
        }
        *count = 0;
        if self.rate_hold_dir != Some(dir) {
            return;
        }
        self.stop_rate_hold_dir(dir);
    }

    const fn start_rate_hold(&mut self, dir: RateDirection) {
        self.rate_hold_dir = Some(dir);
        self.rate_hold_delay_left = RATE_REPEAT_DELAY_SECONDS;
        self.rate_hold_repeat_left = RATE_REPEAT_INTERVAL_SECONDS;
    }

    const fn clear_rate_hold_timer(&mut self) {
        self.rate_hold_dir = None;
        self.rate_hold_delay_left = 0.0;
        self.rate_hold_repeat_left = RATE_REPEAT_INTERVAL_SECONDS;
    }

    const fn clear_rate_hold_inputs(&mut self) {
        self.rate_hold_lower_count = 0;
        self.rate_hold_raise_count = 0;
        self.clear_rate_hold_timer();
    }

    const fn stop_rate_hold_dir(&mut self, dir: RateDirection) {
        let other = dir.opposite();
        if self.rate_hold_count(other) > 0 {
            self.start_rate_hold(other);
        } else {
            self.clear_rate_hold_timer();
        }
    }

    fn change_music_rate<P: GameplayProfileData, O, C, D>(
        &mut self,
        game: &mut Game<P, O, C, D>,
        delta: f32,
        feedback: &mut impl FnMut(&Game<P, O, C, D>, PracticeEditorFeedback),
    ) -> bool {
        let current = game.music_rate();
        let new_rate = quantized_music_rate(current, delta);
        if (new_rate - current).abs() <= f32::EPSILON {
            feedback(
                game,
                PracticeEditorFeedback::Sound(PracticeEditorSound::Invalid),
            );
            feedback(
                game,
                PracticeEditorFeedback::Flash(PracticeEditorFlash::MusicRateLimit {
                    rate: current,
                }),
            );
            return false;
        }
        let changed = game.set_music_rate(new_rate);
        feedback(game, PracticeEditorFeedback::PersistMusicRate(new_rate));
        game.push_audio_command(GameplayAudioCommand::SetMusicRate(new_rate));
        if changed {
            feedback(game, PracticeEditorFeedback::SyncMusicRateText);
            feedback(
                game,
                PracticeEditorFeedback::Flash(PracticeEditorFlash::MusicRate { rate: new_rate }),
            );
            feedback(
                game,
                PracticeEditorFeedback::Sound(PracticeEditorSound::Line),
            );
        }
        changed
    }
}

const fn cursor_direction(mode: NavMode, action: VirtualAction) -> Option<Direction> {
    match mode {
        NavMode::GameplayButtons => match action {
            VirtualAction::p1_up
            | VirtualAction::p2_up
            | VirtualAction::p1_menu_up
            | VirtualAction::p2_menu_up => Some(Direction::Up),
            VirtualAction::p1_down
            | VirtualAction::p2_down
            | VirtualAction::p1_menu_down
            | VirtualAction::p2_menu_down => Some(Direction::Down),
            _ => None,
        },
        NavMode::DedicatedFiveKey => match action {
            VirtualAction::p1_menu_up | VirtualAction::p2_menu_up => Some(Direction::Up),
            VirtualAction::p1_menu_down | VirtualAction::p2_menu_down => Some(Direction::Down),
            _ => None,
        },
        NavMode::DedicatedThreeKey => match action {
            VirtualAction::p1_menu_left | VirtualAction::p2_menu_left => Some(Direction::Up),
            VirtualAction::p1_menu_right | VirtualAction::p2_menu_right => Some(Direction::Down),
            _ => None,
        },
    }
}

const fn snap_delta(mode: NavMode, action: VirtualAction) -> Option<isize> {
    match mode {
        NavMode::GameplayButtons => match action {
            // Preserve the existing gameplay-button map: P2's dedicated menu
            // left/right do not change snap in this mode.
            VirtualAction::p1_left | VirtualAction::p2_left | VirtualAction::p1_menu_left => {
                Some(-1)
            }
            VirtualAction::p1_right | VirtualAction::p2_right | VirtualAction::p1_menu_right => {
                Some(1)
            }
            _ => None,
        },
        NavMode::DedicatedFiveKey => match action {
            VirtualAction::p1_menu_left | VirtualAction::p2_menu_left => Some(-1),
            VirtualAction::p1_menu_right | VirtualAction::p2_menu_right => Some(1),
            _ => None,
        },
        NavMode::DedicatedThreeKey => None,
    }
}

const fn menu_step_delta(mode: NavMode, action: VirtualAction) -> Option<isize> {
    match cursor_direction(mode, action) {
        Some(Direction::Up) => Some(-1),
        Some(Direction::Down) => Some(1),
        None => None,
    }
}

const fn page_direction(key: PracticeKey) -> Option<Direction> {
    match key {
        PracticeKey::PageUp => Some(Direction::Up),
        PracticeKey::PageDown => Some(Direction::Down),
        _ => None,
    }
}

const fn rate_direction(
    key: PracticeKey,
    pressed: bool,
    shift_held: bool,
) -> Option<RateDirection> {
    if pressed && !shift_held {
        return None;
    }
    match key {
        PracticeKey::RateDecrease => Some(RateDirection::Lower),
        PracticeKey::RateIncrease => Some(RateDirection::Raise),
        _ => None,
    }
}

const fn gameplay_hotkey_input(key: PracticeKey) -> Option<GameplayRawKeyInput> {
    match key {
        PracticeKey::Autosync => Some(GameplayRawKeyInput::Autosync),
        PracticeKey::TimingTick => Some(GameplayRawKeyInput::TimingTick),
        PracticeKey::Autoplay => Some(GameplayRawKeyInput::Autoplay),
        PracticeKey::OffsetDecrease => Some(GameplayRawKeyInput::OffsetAdjust(
            GameplayOffsetAdjustKey::Decrease,
        )),
        PracticeKey::OffsetIncrease => Some(GameplayRawKeyInput::OffsetAdjust(
            GameplayOffsetAdjustKey::Increase,
        )),
        _ => None,
    }
}

fn quantized_music_rate(current: f32, delta: f32) -> f32 {
    let current_step = (current / MUSIC_RATE_STEP).round() as i32;
    let delta_step = (delta / MUSIC_RATE_STEP).round() as i32;
    let min_step = (MUSIC_RATE_MIN / MUSIC_RATE_STEP).round() as i32;
    let max_step = (MUSIC_RATE_MAX / MUSIC_RATE_STEP).round() as i32;
    (current_step + delta_step).clamp(min_step, max_step) as f32 * MUSIC_RATE_STEP
}

fn clamp_selection(
    anchor: Option<f32>,
    end: Option<f32>,
    max_beat: f32,
) -> (Option<f32>, Option<f32>) {
    let anchor = anchor.map(|beat| clamp_marker_beat(beat, max_beat));
    let end = end.map(|beat| clamp_marker_beat(beat, max_beat));
    match (anchor, end) {
        (Some(a), Some(b)) if !same_beat(a, b) => (Some(a.min(b)), Some(a.max(b))),
        (Some(a), None) => (Some(a), None),
        (None, Some(b)) => (None, Some(b)),
        _ => (None, None),
    }
}

const fn clamp_marker_beat(beat: f32, max_beat: f32) -> f32 {
    if beat.is_finite() {
        beat.clamp(MIN_CURSOR_BEAT, max_beat)
    } else {
        MIN_CURSOR_BEAT
    }
}

fn same_beat(a: f32, b: f32) -> bool {
    (a - b).abs() <= BEAT_EPSILON
}

fn quantize_beat(beat: f32, snap: f32) -> f32 {
    if snap <= BEAT_EPSILON {
        beat
    } else {
        (beat / snap).round() * snap
    }
}

/// Calculate the current editor bound from note tails and the song duration.
/// This deliberately uses current gameplay data rather than a cached bound.
#[must_use]
pub fn max_play_beat<P: GameplayProfileData, O, C, D>(game: &Game<P, O, C, D>) -> f32 {
    let note_beat = game
        .notes()
        .iter()
        .map(|note| note.hold.as_ref().map_or(note.beat, |hold| hold.end_beat))
        .fold(MIN_CURSOR_BEAT, f32::max);
    let song_beat = game.beat_for_music_time(game.song().music_length_seconds.max(0.0));
    note_beat.max(song_beat).max(MIN_CURSOR_BEAT)
}
