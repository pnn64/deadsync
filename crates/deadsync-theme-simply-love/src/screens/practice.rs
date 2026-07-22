use crate::act;
use crate::assets::i18n::{self, LookupKey, lookup_key};
use crate::assets::{AssetManager, FontRole, machine_font_key};
use crate::screens::gameplay as gameplay_screen;
use crate::screens::{Screen, ThemeEffect};
use crate::views::PracticeRuntimeView;
use deadlib_present::actors::Actor;
use deadlib_present::color;
use deadlib_present::space::{
    screen_center_x, screen_center_y, screen_height, screen_width, widescale,
};
use deadsync_gameplay::{
    AutosyncMode, GameplayAction, GameplayAudioCommand, GameplayAudioSnapshot,
    GameplayOffsetAdjustKey, GameplayRawKeyInput, GameplayTimingTickMode, handle_core_input,
    spacing_multiplier_for_percent, update_core,
};
use deadsync_input::KeyCode;
use deadsync_input::RawKeyboardEvent;
use deadsync_input::{InputEvent, VirtualAction};
use deadsync_profile as profile_data;
use deadsync_rules::scroll::ScrollSpeedSetting;
use deadsync_rules::timing::{SpeedSegment, SpeedUnit, TimingSegments};
use std::path::Path;
use std::sync::Arc;

const LEAD_IN_SECONDS: f32 = 1.0;
const LOOP_AFTER_SECONDS: f32 = 1.0;
const BEATS_PER_MEASURE: f32 = 4.0;
const MIN_CURSOR_BEAT: f32 = 0.0;
const BEAT_EPSILON: f32 = 0.000_1;
const MARKER_Z: f32 = 2985.0;
const EDIT_TIMING_LABEL_Z: f32 = MARKER_Z;
const EDIT_FIELD_CURSOR_TEX: &str = "practice/snap_display_icon_9x1 (doubleres).png";
const EDIT_FIELD_CURSOR_Z: f32 = MARKER_Z + 1.0;
const EDIT_MENU_ROW_HEIGHT: f32 = 32.0;
const EDIT_MENU_ROW_BG_HEIGHT: f32 = 30.0;
const EDIT_MENU_TITLE_X_OFFSET: f32 = 200.0;
const EDIT_MENU_TEXT_ZOOM: f32 = 1.0;
const EDIT_HELP_HEADER_ZOOM: f32 = 0.26;
const EDIT_HELP_MENU_Y: f32 = 150.0;
const EDIT_HELP_MISC_Y: f32 = 224.0;
const EDIT_FIELD_ZOOM_AT_480P: f32 = 0.5;
const EDIT_FIELD_HEIGHT_AT_480P: f32 = 360.0;
const EDIT_SNAP_CURSOR_ZOOM: f32 = 0.5;
const EDIT_CURSOR_REPEAT_DELAY_SECONDS: f32 = 0.375;
const EDIT_CURSOR_REPEAT_INTERVAL_SECONDS: f32 = 0.125;
const MAX_EDIT_CURSOR_REPEATS_PER_FRAME: usize = 64;
const TAB_FAST_MULTIPLIER: f32 = 4.0;
/// Time constant for the exponential easing that scrolls the displayed chart
/// position toward the cursor. Small enough to keep up with the key-repeat
/// interval, large enough to read as a smooth scroll rather than a jump.
const DISPLAY_SCROLL_TIME_CONSTANT: f32 = 0.07;
/// Once the display is within this many beats of the cursor, settle exactly on
/// it so the chart doesn't creep forever on sub-pixel differences.
const DISPLAY_SCROLL_SNAP_EPSILON: f32 = 0.001;
/// Jumps larger than this snap instantly instead of animating, so Home/End and
/// other long seeks stay responsive instead of blurring across the whole song.
const DISPLAY_SCROLL_MAX_SMOOTH_BEATS: f32 = 64.0;
const EDIT_INFO_VALUE_CHARS: usize = 28;
const EDIT_LINE_SOUND: &str = "assets/sounds/change.ogg";
const EDIT_MARKER_SOUND: &str = "assets/sounds/screen_edit_marker.ogg";
const EDIT_SNAP_SOUND: &str = "assets/sounds/screen_edit_snap.ogg";
const EDIT_INVALID_SOUND: &str = "assets/sounds/common_invalid.ogg";
const EDIT_SCROLL_SPEEDS: [f32; 7] = [1.0, 1.5, 2.0, 3.0, 4.0, 6.0, 8.0];
const MUSIC_RATE_HOTKEY_STEP: f32 = 0.01;
const MUSIC_RATE_HOTKEY_MIN: f32 = 0.5;
const MUSIC_RATE_HOTKEY_MAX: f32 = 3.0;
const MUSIC_RATE_REPEAT_DELAY_SECONDS: f32 = 0.375;
const MUSIC_RATE_REPEAT_INTERVAL_SECONDS: f32 = 0.05;
const MAX_MUSIC_RATE_REPEATS_PER_FRAME: usize = 64;
const FLASH_DURATION_SECS: f32 = 0.75;

pub type MusicStartSnap = fn(&Path, f64) -> f64;

#[derive(Clone, Copy)]
struct TimingLabelStyle {
    color: [f32; 4],
    left_side: bool,
    offset_x: f32,
}

// PARITY[ITGmania NoteField]: Simply Love inherits these timing label
// colors, sides, and offsets from `_fallback/metrics.ini` `[NoteField]`.
const BPM_LABEL_STYLE: TimingLabelStyle = TimingLabelStyle {
    color: [1.0, 0.0, 0.0, 1.0],
    left_side: true,
    offset_x: 60.0,
};
const STOP_LABEL_STYLE: TimingLabelStyle = TimingLabelStyle {
    color: [0.8, 0.8, 0.0, 1.0],
    left_side: true,
    offset_x: 50.0,
};
const DELAY_LABEL_STYLE: TimingLabelStyle = TimingLabelStyle {
    color: [0.0, 0.8, 0.8, 1.0],
    left_side: true,
    offset_x: 120.0,
};
const WARP_LABEL_STYLE: TimingLabelStyle = TimingLabelStyle {
    color: [1.0, 0.0, 0.5, 1.0],
    left_side: false,
    offset_x: 90.0,
};
const TIME_SIG_LABEL_STYLE: TimingLabelStyle = TimingLabelStyle {
    color: [1.0, 0.55, 0.0, 1.0],
    left_side: true,
    offset_x: 30.0,
};
const SPEED_LABEL_STYLE: TimingLabelStyle = TimingLabelStyle {
    color: [0.5, 1.0, 1.0, 1.0],
    left_side: false,
    offset_x: 30.0,
};
const SCROLL_LABEL_STYLE: TimingLabelStyle = TimingLabelStyle {
    color: [0.3, 0.8, 1.0, 1.0],
    left_side: true,
    offset_x: 100.0,
};
const FAKE_LABEL_STYLE: TimingLabelStyle = TimingLabelStyle {
    color: [1.0, 1.0, 0.5, 1.0],
    left_side: true,
    offset_x: 90.0,
};

#[derive(Clone, Copy, Debug)]
enum Mode {
    Editing,
    Playing { start_beat: f32, stop_beat: f32 },
}

#[derive(Clone, Copy, Debug)]
enum MarkerPlacement {
    P1,
    P2,
}

#[derive(Clone, Copy)]
struct PracticeFieldGeom {
    player_idx: usize,
    col_start: usize,
    center_x: f32,
    offset_y: f32,
    width: f32,
    zoom: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CursorHoldDir {
    Up,
    Down,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PageHoldDir {
    Up,
    Down,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MusicRateHoldDir {
    Lower,
    Raise,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PracticeNavMode {
    GameplayButtons,
    DedicatedFiveKey,
    DedicatedThreeKey,
}

pub struct State {
    pub gameplay: gameplay_screen::State,
    runtime: PracticeRuntimeView,
    mode: Mode,
    menu: Option<MenuState>,
    cursor_beat: f32,
    display_beat: f32,
    selection_anchor: Option<f32>,
    selection_end: Option<f32>,
    shift_anchor: Option<f32>,
    snap_index: usize,
    edit_scroll_speed_index: usize,
    shift_held: bool,
    ctrl_held: bool,
    tab_held: bool,
    cursor_hold_dir: Option<CursorHoldDir>,
    cursor_hold_up_count: u8,
    cursor_hold_down_count: u8,
    cursor_hold_delay_left: f32,
    cursor_hold_repeat_left: f32,
    page_hold_dir: Option<PageHoldDir>,
    page_hold_up_count: u8,
    page_hold_down_count: u8,
    page_hold_delay_left: f32,
    page_hold_repeat_left: f32,
    music_rate_hold_dir: Option<MusicRateHoldDir>,
    music_rate_hold_lower_count: u8,
    music_rate_hold_raise_count: u8,
    music_rate_hold_delay_left: f32,
    music_rate_hold_repeat_left: f32,
    flash: Option<(String, f32)>,
    pending_sfx: Vec<&'static str>,
    pending_profile: Vec<crate::SimplyLoveProfileRequest>,
}

#[derive(Clone, Copy, Debug)]
pub struct EditSnapshot {
    cursor_beat: f32,
    selection_anchor: Option<f32>,
    selection_end: Option<f32>,
    snap_index: usize,
    edit_scroll_speed_index: usize,
}

#[derive(Clone, Copy)]
struct MenuState {
    def: &'static MenuDef,
    selected: usize,
}

type MenuAction = fn(&mut State, MusicStartSnap) -> ThemeEffect;

struct MenuRow {
    label: LookupKey,
    /// `None` means the row is display-only; activating it just closes the menu.
    action: Option<MenuAction>,
}

struct MenuDef {
    rows: &'static [MenuRow],
}

impl MenuDef {
    fn first_actionable_row(&self) -> usize {
        self.rows
            .iter()
            .position(|r| r.action.is_some())
            .unwrap_or(0)
    }
}

const MAIN_MENU: MenuDef = MenuDef {
    rows: &[
        MenuRow {
            label: lookup_key("Practice", "MenuPlayWholeSong"),
            action: Some(action_play_whole_song),
        },
        MenuRow {
            label: lookup_key("Practice", "MenuPlayCurrentToEnd"),
            action: Some(action_play_current_to_end),
        },
        MenuRow {
            label: lookup_key("Practice", "MenuPlaySelection"),
            action: Some(action_play_selection),
        },
        MenuRow {
            label: lookup_key("Practice", "MenuSetSelectionStart"),
            action: Some(action_set_selection_start),
        },
        MenuRow {
            label: lookup_key("Practice", "MenuSetSelectionEnd"),
            action: Some(action_set_selection_end),
        },
        MenuRow {
            label: lookup_key("Practice", "MenuPracticeModeOptions"),
            action: Some(action_editor_options),
        },
        MenuRow {
            label: lookup_key("Practice", "MenuExit"),
            action: Some(action_exit_practice),
        },
    ],
};

const HELP_MENU: MenuDef = MenuDef {
    rows: &[
        MenuRow {
            label: lookup_key("Practice", "HelpHoldUpDown"),
            action: None,
        },
        MenuRow {
            label: lookup_key("Practice", "HelpSemicolonApostrophe"),
            action: None,
        },
        MenuRow {
            label: lookup_key("Practice", "HelpCtrlUpDown"),
            action: None,
        },
        MenuRow {
            label: lookup_key("Practice", "HelpHomeEnd"),
            action: None,
        },
        MenuRow {
            label: lookup_key("Practice", "HelpLeftRight"),
            action: None,
        },
        MenuRow {
            label: lookup_key("Practice", "HelpBracketKeys"),
            action: None,
        },
        MenuRow {
            label: lookup_key("Practice", "HelpSpace"),
            action: None,
        },
        MenuRow {
            label: lookup_key("Practice", "HelpShiftNavigate"),
            action: None,
        },
        MenuRow {
            label: lookup_key("Practice", "HelpP"),
            action: None,
        },
        MenuRow {
            label: lookup_key("Practice", "HelpF7Ticks"),
            action: None,
        },
        MenuRow {
            label: lookup_key("Practice", "HelpF8Autoplay"),
            action: None,
        },
        MenuRow {
            label: lookup_key("Practice", "HelpF6Autosync"),
            action: None,
        },
        MenuRow {
            label: lookup_key("Practice", "HelpOffsetKeys"),
            action: None,
        },
        MenuRow {
            label: lookup_key("Practice", "HelpEscEnter"),
            action: None,
        },
    ],
};

const SNAP_LABELS: [&str; 9] = [
    "4th", "8th", "12th", "16th", "24th", "32nd", "48th", "64th", "192nd",
];
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

pub fn init(mut gameplay: gameplay_screen::State, runtime: PracticeRuntimeView) -> State {
    gameplay.disable_score_for_practice();
    let mut state = State {
        gameplay,
        runtime,
        mode: Mode::Editing,
        menu: None,
        cursor_beat: 0.0,
        display_beat: 0.0,
        selection_anchor: None,
        selection_end: None,
        shift_anchor: None,
        snap_index: 0,
        edit_scroll_speed_index: 0,
        shift_held: false,
        ctrl_held: false,
        tab_held: false,
        cursor_hold_dir: None,
        cursor_hold_up_count: 0,
        cursor_hold_down_count: 0,
        cursor_hold_delay_left: 0.0,
        cursor_hold_repeat_left: EDIT_CURSOR_REPEAT_INTERVAL_SECONDS,
        page_hold_dir: None,
        page_hold_up_count: 0,
        page_hold_down_count: 0,
        page_hold_delay_left: 0.0,
        page_hold_repeat_left: EDIT_CURSOR_REPEAT_INTERVAL_SECONDS,
        music_rate_hold_dir: None,
        music_rate_hold_lower_count: 0,
        music_rate_hold_raise_count: 0,
        music_rate_hold_delay_left: 0.0,
        music_rate_hold_repeat_left: MUSIC_RATE_REPEAT_INTERVAL_SECONDS,
        flash: None,
        pending_sfx: Vec::with_capacity(8),
        pending_profile: Vec::with_capacity(4),
    };
    set_cursor(&mut state, MIN_CURSOR_BEAT);
    snap_display_to_cursor(&mut state);
    state
}

#[inline(always)]
fn queue_sfx(state: &mut State, path: &'static str) {
    state.pending_sfx.push(path);
}

fn prepend_pending_sfx(pending_sfx: &mut Vec<&'static str>, effect: ThemeEffect) -> ThemeEffect {
    let sound_count = pending_sfx.len();
    if sound_count == 0 {
        return effect;
    }

    let has_effect = !matches!(effect, ThemeEffect::None);
    let mut effects = Vec::with_capacity(sound_count + usize::from(has_effect));
    effects.extend(pending_sfx.drain(..).map(crate::effects::sfx));
    if has_effect {
        effects.push(effect);
    }
    if effects.len() == 1 {
        effects.pop().expect("one queued Practice effect")
    } else {
        ThemeEffect::Batch(effects)
    }
}

fn prepend_pending_effects(
    pending_profile: &mut Vec<crate::SimplyLoveProfileRequest>,
    pending_sfx: &mut Vec<&'static str>,
    effect: ThemeEffect,
) -> ThemeEffect {
    let trailing = prepend_pending_sfx(pending_sfx, effect);
    if pending_profile.is_empty() {
        return trailing;
    }

    let mut effects = Vec::with_capacity(pending_profile.len() + 2);
    effects.extend(
        pending_profile
            .drain(..)
            .map(|request| ThemeEffect::Runtime(crate::SimplyLoveRuntimeRequest::Profile(request))),
    );
    match trailing {
        ThemeEffect::None => {}
        ThemeEffect::Batch(mut trailing) => effects.append(&mut trailing),
        trailing => effects.push(trailing),
    }
    if effects.len() == 1 {
        effects.pop().expect("one queued Practice effect")
    } else {
        ThemeEffect::Batch(effects)
    }
}

fn finish_effect(state: &mut State, effect: ThemeEffect) -> ThemeEffect {
    prepend_pending_effects(&mut state.pending_profile, &mut state.pending_sfx, effect)
}

pub fn edit_snapshot(state: &State) -> EditSnapshot {
    EditSnapshot {
        cursor_beat: state.cursor_beat,
        selection_anchor: state.selection_anchor,
        selection_end: state.selection_end,
        snap_index: state.snap_index,
        edit_scroll_speed_index: state.edit_scroll_speed_index,
    }
}

pub fn restore_edit_snapshot(state: &mut State, snapshot: EditSnapshot) {
    clear_cursor_hold_inputs(state);
    clear_page_hold_inputs(state);
    state.mode = Mode::Editing;
    state.menu = None;
    state.shift_anchor = None;
    state.shift_held = false;
    state.ctrl_held = false;
    state.tab_held = false;
    clear_music_rate_hold_inputs(state);
    state.snap_index = snapshot.snap_index.min(SNAP_LABELS.len().saturating_sub(1));
    state.edit_scroll_speed_index = snapshot
        .edit_scroll_speed_index
        .min(EDIT_SCROLL_SPEEDS.len().saturating_sub(1));
    let (selection_anchor, selection_end) = clamp_selection(
        snapshot.selection_anchor,
        snapshot.selection_end,
        max_play_beat(state),
    );
    state.selection_anchor = selection_anchor;
    state.selection_end = selection_end;
    set_cursor(state, snapshot.cursor_beat);
    snap_display_to_cursor(state);
}

pub fn on_enter(state: &mut State) {
    state
        .gameplay
        .push_audio_command(GameplayAudioCommand::StopMusic);
    set_cursor(state, state.cursor_beat);
    snap_display_to_cursor(state);
}

pub fn update(
    state: &mut State,
    delta_time: f32,
    audio_snapshot: GameplayAudioSnapshot,
    fallback_host_nanos: impl FnOnce() -> u64,
    snap_music_start: MusicStartSnap,
) -> ThemeEffect {
    let effect = update_inner(
        state,
        delta_time,
        audio_snapshot,
        fallback_host_nanos,
        snap_music_start,
    );
    finish_effect(state, effect)
}

fn update_inner(
    state: &mut State,
    delta_time: f32,
    audio_snapshot: GameplayAudioSnapshot,
    fallback_host_nanos: impl FnOnce() -> u64,
    snap_music_start: MusicStartSnap,
) -> ThemeEffect {
    if let Some((_, remaining)) = state.flash.as_mut() {
        *remaining -= delta_time;
        if *remaining <= 0.0 {
            state.flash = None;
        }
    }
    update_music_rate_hold(state, delta_time);

    let Mode::Playing {
        start_beat,
        stop_beat,
    } = state.mode
    else {
        state.gameplay.advance_screen_elapsed(delta_time);
        update_cursor_hold(state, delta_time);
        update_page_hold(state, delta_time);
        update_display_scroll(state, delta_time);
        return ThemeEffect::None;
    };

    let action = update_core(
        &mut state.gameplay,
        delta_time,
        audio_snapshot,
        fallback_host_nanos,
    );
    let current_time = state.gameplay.current_music_time_seconds();
    let stop_time = state.gameplay.music_time_for_beat(stop_beat);
    if current_time >= stop_time + LOOP_AFTER_SECONDS || !matches!(action, GameplayAction::None) {
        start_playback(state, start_beat, stop_beat, snap_music_start);
    }
    ThemeEffect::None
}

pub fn handle_input(
    state: &mut State,
    ev: &InputEvent,
    snap_music_start: MusicStartSnap,
) -> ThemeEffect {
    let effect = handle_input_inner(state, ev, snap_music_start);
    finish_effect(state, effect)
}

fn handle_input_inner(
    state: &mut State,
    ev: &InputEvent,
    snap_music_start: MusicStartSnap,
) -> ThemeEffect {
    if state.menu.is_some() {
        return handle_menu_input(state, ev, snap_music_start);
    }

    if !ev.pressed {
        if matches!(state.mode, Mode::Editing)
            && let Some(dir) = edit_cursor_hold_dir_for_action(state, ev.action)
        {
            release_cursor_hold_input(state, dir);
            return ThemeEffect::None;
        }
        if matches!(state.mode, Mode::Playing { .. }) && ev.action.is_gameplay_arrow() {
            let _ = handle_core_input(&mut state.gameplay, ev);
        }
        return ThemeEffect::None;
    }

    match state.mode {
        Mode::Playing { .. } => match ev.action {
            VirtualAction::p1_back | VirtualAction::p2_back => {
                stop_playback(state);
                ThemeEffect::None
            }
            _ => {
                let _ = handle_core_input(&mut state.gameplay, ev);
                ThemeEffect::None
            }
        },
        Mode::Editing => handle_edit_input(state, ev),
    }
}

fn handle_edit_input(state: &mut State, ev: &InputEvent) -> ThemeEffect {
    if let Some(dir) = edit_cursor_hold_dir_for_action(state, ev.action) {
        press_cursor_hold_input(state, dir);
        move_cursor_by_hold_dir(state, dir);
        return ThemeEffect::None;
    }
    if let Some(delta) = edit_snap_delta_for_action(state, ev.action) {
        change_snap(state, delta);
        return ThemeEffect::None;
    }
    match ev.action {
        VirtualAction::p1_start | VirtualAction::p2_start => {
            open_main_menu(state);
            ThemeEffect::None
        }
        VirtualAction::p1_back | VirtualAction::p2_back => {
            open_main_menu(state);
            ThemeEffect::None
        }
        VirtualAction::p1_select | VirtualAction::p2_select => {
            set_area_marker(state);
            ThemeEffect::None
        }
        _ => ThemeEffect::None,
    }
}

pub fn handle_raw_key_event(
    state: &mut State,
    raw_key: &RawKeyboardEvent,
    snap_music_start: MusicStartSnap,
) -> (bool, ThemeEffect) {
    let (consumed, effect) = handle_raw_key_event_inner(state, raw_key, snap_music_start);
    (consumed, finish_effect(state, effect))
}

fn handle_raw_key_event_inner(
    state: &mut State,
    raw_key: &RawKeyboardEvent,
    snap_music_start: MusicStartSnap,
) -> (bool, ThemeEffect) {
    match raw_key.code {
        KeyCode::ShiftLeft | KeyCode::ShiftRight => {
            state.shift_held = raw_key.pressed;
            if !raw_key.pressed {
                state.shift_anchor = None;
            }
            return (true, ThemeEffect::None);
        }
        KeyCode::ControlLeft | KeyCode::ControlRight => {
            state.ctrl_held = raw_key.pressed;
            return (true, ThemeEffect::None);
        }
        KeyCode::Tab => {
            state.tab_held = raw_key.pressed;
            return (false, ThemeEffect::None);
        }
        _ => {}
    }

    if !raw_key.pressed {
        if let Some(dir) = page_hold_dir_for_key(raw_key.code) {
            release_page_hold_input(state, dir);
            return (true, ThemeEffect::None);
        }
        if let Some(dir) = music_rate_hold_dir_for_key(raw_key.code) {
            release_music_rate_hold_input(state, dir);
            return (true, ThemeEffect::None);
        }
        // Forward gameplay function-key releases (e.g. F11/F12) so the runtime
        // can clear its offset-adjust hold state.
        if let Some(input) = gameplay_hotkey_input(raw_key.code) {
            forward_gameplay_hotkey(state, input, false, raw_key.timestamp);
            return (true, ThemeEffect::None);
        }
        return (false, ThemeEffect::None);
    }

    // Gameplay function keys work everywhere in practice mode (editing,
    // playback, or with the menu open), mirroring how they behave in gameplay.
    // Autoplay (F8), assist/hit ticks (F7), AutoSync (F6), and sync-offset
    // adjustment (F11/F12) are forwarded to the embedded gameplay runtime.
    if let Some(input) = gameplay_hotkey_input(raw_key.code) {
        if !raw_key.repeat {
            forward_gameplay_hotkey(state, input, true, raw_key.timestamp);
        }
        return (true, ThemeEffect::None);
    }

    // Music rate hotkeys are global within practice mode: they work whether
    // the user is editing, mid-loop playback, or has the menu open.
    if let Some(dir) = music_rate_hold_dir_for_key(raw_key.code) {
        if !raw_key.repeat {
            press_music_rate_hold_input(state, dir);
        }
        return (true, ThemeEffect::None);
    }

    if matches!(state.mode, Mode::Playing { .. }) {
        return match raw_key.code {
            KeyCode::Escape | KeyCode::Enter => {
                stop_playback(state);
                (true, ThemeEffect::None)
            }
            _ => (false, ThemeEffect::None),
        };
    }

    if state.menu.is_some() {
        return match raw_key.code {
            KeyCode::Escape => {
                close_menu(state);
                (true, ThemeEffect::None)
            }
            KeyCode::Enter => (true, activate_menu_item(state, snap_music_start)),
            _ => (false, ThemeEffect::None),
        };
    }

    match raw_key.code {
        KeyCode::Escape | KeyCode::Enter => {
            open_main_menu(state);
            (true, ThemeEffect::None)
        }
        KeyCode::ArrowUp if state.ctrl_held => {
            change_edit_scroll_speed(state, 1);
            (true, ThemeEffect::None)
        }
        KeyCode::ArrowDown if state.ctrl_held => {
            change_edit_scroll_speed(state, -1);
            (true, ThemeEffect::None)
        }
        KeyCode::KeyP if state.ctrl_held => {
            start_playback(
                state,
                MIN_CURSOR_BEAT,
                max_play_beat(state),
                snap_music_start,
            );
            (true, ThemeEffect::None)
        }
        KeyCode::KeyP if state.shift_held => {
            start_playback(
                state,
                state.cursor_beat,
                max_play_beat(state),
                snap_music_start,
            );
            (true, ThemeEffect::None)
        }
        KeyCode::KeyP => {
            start_selection_like_itg(state, snap_music_start);
            (true, ThemeEffect::None)
        }
        KeyCode::Space => {
            set_area_marker(state);
            (true, ThemeEffect::None)
        }
        KeyCode::Semicolon => {
            press_page_hold_input_for_key(state, PageHoldDir::Up, raw_key.repeat);
            (true, ThemeEffect::None)
        }
        KeyCode::PageUp => {
            press_page_hold_input_for_key(state, PageHoldDir::Up, raw_key.repeat);
            (true, ThemeEffect::None)
        }
        KeyCode::Quote => {
            press_page_hold_input_for_key(state, PageHoldDir::Down, raw_key.repeat);
            (true, ThemeEffect::None)
        }
        KeyCode::PageDown => {
            press_page_hold_input_for_key(state, PageHoldDir::Down, raw_key.repeat);
            (true, ThemeEffect::None)
        }
        KeyCode::Comma if !state.ctrl_held => {
            seek_chart_note(state, -1);
            (true, ThemeEffect::None)
        }
        KeyCode::Period if !state.ctrl_held => {
            seek_chart_note(state, 1);
            (true, ThemeEffect::None)
        }
        KeyCode::Home => {
            set_cursor(state, MIN_CURSOR_BEAT);
            queue_sfx(state, EDIT_LINE_SOUND);
            (true, ThemeEffect::None)
        }
        KeyCode::End => {
            set_cursor(state, max_play_beat(state));
            queue_sfx(state, EDIT_LINE_SOUND);
            (true, ThemeEffect::None)
        }
        KeyCode::F1 => {
            open_help_menu(state);
            (true, ThemeEffect::None)
        }
        _ => (false, ThemeEffect::None),
    }
}

/// Maps the gameplay function keys that Practice forwards to its embedded
/// gameplay runtime. Mirrors the gameplay screen bindings: F6 AutoSync,
/// F7 assist/hit ticks, F8 autoplay, F11/F12 sync-offset adjustment.
fn gameplay_hotkey_input(code: KeyCode) -> Option<GameplayRawKeyInput> {
    match code {
        KeyCode::F6 => Some(GameplayRawKeyInput::Autosync),
        KeyCode::F7 => Some(GameplayRawKeyInput::TimingTick),
        KeyCode::F8 => Some(GameplayRawKeyInput::Autoplay),
        KeyCode::F11 => Some(GameplayRawKeyInput::OffsetAdjust(
            GameplayOffsetAdjustKey::Decrease,
        )),
        KeyCode::F12 => Some(GameplayRawKeyInput::OffsetAdjust(
            GameplayOffsetAdjustKey::Increase,
        )),
        _ => None,
    }
}

fn forward_gameplay_hotkey(
    state: &mut State,
    input: GameplayRawKeyInput,
    pressed: bool,
    timestamp: std::time::Instant,
) {
    state
        .gameplay
        .set_raw_modifier_state(state.shift_held, state.ctrl_held);
    let now_music_time = state.gameplay.current_music_time_seconds();
    // No offset-save prompt exists in practice mode, so commands are always
    // allowed.
    let _ = state.gameplay.handle_queued_raw_key_input(
        input,
        None,
        pressed,
        timestamp,
        now_music_time,
        true,
    );
    if pressed {
        set_gameplay_hotkey_flash(state, input);
    }
}

fn set_gameplay_hotkey_flash(state: &mut State, input: GameplayRawKeyInput) {
    match input {
        GameplayRawKeyInput::Autoplay => {
            let key = if state.gameplay.autoplay_enabled() {
                "FlashAutoplayOn"
            } else {
                "FlashAutoplayOff"
            };
            set_flash_tr(state, key);
        }
        GameplayRawKeyInput::TimingTick => {
            let key = match state.gameplay.tick_mode() {
                GameplayTimingTickMode::Off => "FlashTicksOff",
                GameplayTimingTickMode::Assist => "FlashTicksAssist",
                GameplayTimingTickMode::Hit => "FlashTicksHit",
            };
            set_flash_tr(state, key);
        }
        GameplayRawKeyInput::Autosync => {
            let key = match state.gameplay.autosync_mode() {
                AutosyncMode::Off => "FlashAutosyncOff",
                AutosyncMode::Song => "FlashAutosyncSong",
                AutosyncMode::Machine => "FlashAutosyncMachine",
            };
            set_flash_tr(state, key);
        }
        GameplayRawKeyInput::OffsetAdjust(_) => set_offset_adjust_flash(state),
        GameplayRawKeyInput::Restart | GameplayRawKeyInput::Other => {}
    }
}

fn set_offset_adjust_flash(state: &mut State) {
    // Shift adjusts the machine (global) offset; otherwise the song offset,
    // matching `offset_adjust_target` for a non-course practice session.
    let (key, seconds) = if state.shift_held {
        ("FlashGlobalOffset", state.gameplay.global_offset_seconds())
    } else {
        ("FlashSongOffset", state.gameplay.song_offset_seconds())
    };
    let ms = format!("{:+.0}", seconds * 1000.0);
    let text = i18n::tr_fmt("Practice", key, &[("ms", &ms)]).replace("\\n", "\n");
    state.flash = Some((text, FLASH_DURATION_SECS));
}

pub fn push_actors(
    actors: &mut Vec<Actor>,
    state: &mut State,
    asset_manager: &AssetManager,
    arrow_effect_time_s: f32,
    visual_policy: crate::views::SimplyLoveVisualPolicyView,
) {
    actors.reserve(128);
    let view = practice_view(state);
    gameplay_screen::push_actors(
        actors,
        &mut state.gameplay,
        asset_manager,
        view,
        arrow_effect_time_s,
        visual_policy,
    );
    if matches!(state.mode, Mode::Editing) {
        append_edit_markers(state, actors);
        append_edit_overlay(state, actors);
    }
    if state.menu.is_some() {
        append_main_menu(state, actors);
    }
    // Render any active flash text regardless of mode so music-rate changes
    // (and other transient feedback) are visible during loop playback as well.
    append_flash_overlay(state, actors);
}

fn practice_view(state: &State) -> gameplay_screen::ActorViewOverride {
    let mut notefield = if matches!(state.mode, Mode::Editing) {
        practice_notefield_view(state)
    } else {
        gameplay_screen::NotefieldViewOverride::default()
    };
    notefield.hide_display_mods = true;
    gameplay_screen::ActorViewOverride {
        notefield,
        hide_gameplay_hud: true,
        ..Default::default()
    }
}

fn practice_notefield_view(state: &State) -> gameplay_screen::NotefieldViewOverride {
    gameplay_screen::NotefieldViewOverride {
        field_zoom: Some(practice_edit_field_zoom()),
        scroll_speed: Some(practice_edit_scroll_speed(state)),
        force_center_1player: true,
        receptor_y: Some(practice_edit_cursor_y()),
        edit_beat_bars: true,
        hide_combo: true,
        ..gameplay_screen::NotefieldViewOverride::default()
    }
}

fn practice_edit_field_zoom() -> f32 {
    screen_height() / 480.0 * EDIT_FIELD_ZOOM_AT_480P
}

fn practice_edit_cursor_y() -> f32 {
    screen_center_y() - screen_height() / 480.0 * EDIT_FIELD_HEIGHT_AT_480P * 0.5
}

fn practice_edit_scroll_speed(state: &State) -> ScrollSpeedSetting {
    ScrollSpeedSetting::XMod(EDIT_SCROLL_SPEEDS[state.edit_scroll_speed_index])
}

fn practice_marker_bar_height() -> f32 {
    ScrollSpeedSetting::ARROW_SPACING * practice_edit_field_zoom()
}

fn handle_menu_input(
    state: &mut State,
    ev: &InputEvent,
    snap_music_start: MusicStartSnap,
) -> ThemeEffect {
    if !ev.pressed {
        return ThemeEffect::None;
    }
    if let Some(delta) = menu_step_delta_for_action(state, ev.action) {
        step_menu(state, delta);
        return ThemeEffect::None;
    }
    match ev.action {
        VirtualAction::p1_start
        | VirtualAction::p2_start
        | VirtualAction::p1_select
        | VirtualAction::p2_select => activate_menu_item(state, snap_music_start),
        VirtualAction::p1_back | VirtualAction::p2_back => {
            close_menu(state);
            ThemeEffect::None
        }
        _ => ThemeEffect::None,
    }
}

fn open_main_menu(state: &mut State) {
    clear_cursor_hold_inputs(state);
    clear_page_hold_inputs(state);
    if state.menu.is_none() {
        queue_sfx(state, "assets/sounds/start.ogg");
    }
    state.menu = Some(MenuState {
        def: &MAIN_MENU,
        selected: MAIN_MENU.first_actionable_row(),
    });
}

fn open_help_menu(state: &mut State) {
    clear_cursor_hold_inputs(state);
    clear_page_hold_inputs(state);
    if state.menu.is_none() {
        queue_sfx(state, "assets/sounds/start.ogg");
    }
    state.menu = Some(MenuState {
        def: &HELP_MENU,
        selected: 0,
    });
}

fn close_menu(state: &mut State) {
    state.menu = None;
    queue_sfx(state, "assets/sounds/start.ogg");
}

fn step_menu(state: &mut State, delta: isize) {
    let Some(menu) = state.menu else {
        return;
    };
    let len = menu.def.rows.len() as isize;
    if len == 0 {
        return;
    }
    let selected = (menu.selected as isize + delta).rem_euclid(len) as usize;
    state.menu = Some(MenuState {
        def: menu.def,
        selected,
    });
    queue_sfx(state, "assets/sounds/change.ogg");
}

fn activate_menu_item(state: &mut State, snap_music_start: MusicStartSnap) -> ThemeEffect {
    let Some(menu) = state.menu else {
        return ThemeEffect::None;
    };
    let Some(row) = menu.def.rows.get(menu.selected) else {
        return ThemeEffect::None;
    };
    let Some(action) = row.action else {
        // Display-only row (e.g. Help): treat Enter as close.
        close_menu(state);
        return ThemeEffect::None;
    };
    state.menu = None;
    clear_cursor_hold_inputs(state);
    clear_page_hold_inputs(state);
    queue_sfx(state, "assets/sounds/start.ogg");
    action(state, snap_music_start)
}

fn action_play_whole_song(state: &mut State, snap_music_start: MusicStartSnap) -> ThemeEffect {
    start_playback(
        state,
        MIN_CURSOR_BEAT,
        max_play_beat(state),
        snap_music_start,
    );
    ThemeEffect::None
}

fn action_play_current_to_end(state: &mut State, snap_music_start: MusicStartSnap) -> ThemeEffect {
    let cursor = state.cursor_beat;
    start_playback(state, cursor, max_play_beat(state), snap_music_start);
    ThemeEffect::None
}

fn action_play_selection(state: &mut State, snap_music_start: MusicStartSnap) -> ThemeEffect {
    start_selection_like_itg(state, snap_music_start);
    ThemeEffect::None
}

fn action_set_selection_start(state: &mut State, _snap_music_start: MusicStartSnap) -> ThemeEffect {
    set_selection_start(state);
    ThemeEffect::None
}

fn action_set_selection_end(state: &mut State, _snap_music_start: MusicStartSnap) -> ThemeEffect {
    set_selection_end(state);
    ThemeEffect::None
}

fn action_editor_options(_state: &mut State, _snap_music_start: MusicStartSnap) -> ThemeEffect {
    ThemeEffect::Navigate(Screen::PlayerOptions)
}

fn action_exit_practice(_state: &mut State, _snap_music_start: MusicStartSnap) -> ThemeEffect {
    ThemeEffect::Navigate(Screen::SelectMusic)
}

fn practice_nav_mode(state: &State) -> PracticeNavMode {
    practice_nav_mode_from_config(
        state.runtime.only_dedicated_menu_buttons,
        state.runtime.three_key_navigation,
    )
}

const fn practice_nav_mode_from_config(
    only_dedicated_menu_buttons: bool,
    three_key_navigation: bool,
) -> PracticeNavMode {
    if !only_dedicated_menu_buttons {
        PracticeNavMode::GameplayButtons
    } else if three_key_navigation {
        PracticeNavMode::DedicatedThreeKey
    } else {
        PracticeNavMode::DedicatedFiveKey
    }
}

fn edit_cursor_hold_dir_for_action(state: &State, action: VirtualAction) -> Option<CursorHoldDir> {
    edit_cursor_hold_dir_for_action_in_mode(practice_nav_mode(state), action)
}

const fn edit_cursor_hold_dir_for_action_in_mode(
    mode: PracticeNavMode,
    action: VirtualAction,
) -> Option<CursorHoldDir> {
    match mode {
        PracticeNavMode::GameplayButtons => match action {
            VirtualAction::p1_up
            | VirtualAction::p2_up
            | VirtualAction::p1_menu_up
            | VirtualAction::p2_menu_up => Some(CursorHoldDir::Up),
            VirtualAction::p1_down
            | VirtualAction::p2_down
            | VirtualAction::p1_menu_down
            | VirtualAction::p2_menu_down => Some(CursorHoldDir::Down),
            _ => None,
        },
        PracticeNavMode::DedicatedFiveKey => match action {
            VirtualAction::p1_menu_up | VirtualAction::p2_menu_up => Some(CursorHoldDir::Up),
            VirtualAction::p1_menu_down | VirtualAction::p2_menu_down => Some(CursorHoldDir::Down),
            _ => None,
        },
        PracticeNavMode::DedicatedThreeKey => match action {
            VirtualAction::p1_menu_left | VirtualAction::p2_menu_left => Some(CursorHoldDir::Up),
            VirtualAction::p1_menu_right | VirtualAction::p2_menu_right => {
                Some(CursorHoldDir::Down)
            }
            _ => None,
        },
    }
}

fn edit_snap_delta_for_action(state: &State, action: VirtualAction) -> Option<isize> {
    edit_snap_delta_for_action_in_mode(practice_nav_mode(state), action)
}

const fn edit_snap_delta_for_action_in_mode(
    mode: PracticeNavMode,
    action: VirtualAction,
) -> Option<isize> {
    match mode {
        PracticeNavMode::GameplayButtons => match action {
            VirtualAction::p1_left | VirtualAction::p2_left | VirtualAction::p1_menu_left => {
                Some(-1)
            }
            VirtualAction::p1_right | VirtualAction::p2_right | VirtualAction::p1_menu_right => {
                Some(1)
            }
            _ => None,
        },
        PracticeNavMode::DedicatedFiveKey => match action {
            VirtualAction::p1_menu_left | VirtualAction::p2_menu_left => Some(-1),
            VirtualAction::p1_menu_right | VirtualAction::p2_menu_right => Some(1),
            _ => None,
        },
        PracticeNavMode::DedicatedThreeKey => None,
    }
}

fn menu_step_delta_for_action(state: &State, action: VirtualAction) -> Option<isize> {
    menu_step_delta_for_action_in_mode(practice_nav_mode(state), action)
}

const fn menu_step_delta_for_action_in_mode(
    mode: PracticeNavMode,
    action: VirtualAction,
) -> Option<isize> {
    match mode {
        PracticeNavMode::GameplayButtons => match action {
            VirtualAction::p1_up
            | VirtualAction::p2_up
            | VirtualAction::p1_menu_up
            | VirtualAction::p2_menu_up => Some(-1),
            VirtualAction::p1_down
            | VirtualAction::p2_down
            | VirtualAction::p1_menu_down
            | VirtualAction::p2_menu_down => Some(1),
            _ => None,
        },
        PracticeNavMode::DedicatedFiveKey => match action {
            VirtualAction::p1_menu_up | VirtualAction::p2_menu_up => Some(-1),
            VirtualAction::p1_menu_down | VirtualAction::p2_menu_down => Some(1),
            _ => None,
        },
        PracticeNavMode::DedicatedThreeKey => match action {
            VirtualAction::p1_menu_left | VirtualAction::p2_menu_left => Some(-1),
            VirtualAction::p1_menu_right | VirtualAction::p2_menu_right => Some(1),
            _ => None,
        },
    }
}

fn start_selection_like_itg(state: &mut State, snap_music_start: MusicStartSnap) {
    let (start_beat, stop_beat) = selection_range(state)
        .filter(|(start, stop)| stop > start)
        .or_else(|| {
            state
                .selection_anchor
                .map(|start| (start, max_play_beat(state)))
        })
        .unwrap_or((state.cursor_beat, max_play_beat(state)));
    start_playback(
        state,
        start_beat,
        stop_beat.max(start_beat + SNAP_BEATS[state.snap_index]),
        snap_music_start,
    );
}

fn snapped_playback_music_time(
    state: &State,
    playback_music_time: f32,
    snap_music_start: MusicStartSnap,
) -> f32 {
    let Some(music_path) = state.gameplay.charts()[0].music_path.as_ref() else {
        return playback_music_time;
    };
    snap_music_start(music_path, f64::from(playback_music_time)) as f32
}

fn start_playback(
    state: &mut State,
    start_beat: f32,
    stop_beat: f32,
    snap_music_start: MusicStartSnap,
) {
    clear_cursor_hold_inputs(state);
    clear_page_hold_inputs(state);
    let start_time = state.gameplay.music_time_for_beat(start_beat);
    let playback_time =
        snapped_playback_music_time(state, start_time - LEAD_IN_SECONDS, snap_music_start);
    state
        .gameplay
        .start_practice_music_at(playback_time, start_time);
    state.mode = Mode::Playing {
        start_beat,
        stop_beat,
    };
    state.flash = None;
}

fn stop_playback(state: &mut State) {
    clear_cursor_hold_inputs(state);
    clear_page_hold_inputs(state);
    state
        .gameplay
        .push_audio_command(GameplayAudioCommand::StopMusic);
    let current_beat = state.gameplay.current_beat().max(MIN_CURSOR_BEAT);
    let current_time = state.gameplay.music_time_for_beat(current_beat);
    // Practice hits mutate note results, which the edit notefield uses for hide logic.
    state.gameplay.reset_practice_playback(current_time);
    state.mode = Mode::Editing;
    set_cursor(state, current_beat);
    snap_display_to_cursor(state);
}

const fn opposite_cursor_hold_dir(dir: CursorHoldDir) -> CursorHoldDir {
    match dir {
        CursorHoldDir::Up => CursorHoldDir::Down,
        CursorHoldDir::Down => CursorHoldDir::Up,
    }
}

fn cursor_hold_count(state: &State, dir: CursorHoldDir) -> u8 {
    match dir {
        CursorHoldDir::Up => state.cursor_hold_up_count,
        CursorHoldDir::Down => state.cursor_hold_down_count,
    }
}

fn cursor_hold_count_mut(state: &mut State, dir: CursorHoldDir) -> &mut u8 {
    match dir {
        CursorHoldDir::Up => &mut state.cursor_hold_up_count,
        CursorHoldDir::Down => &mut state.cursor_hold_down_count,
    }
}

fn press_cursor_hold_input(state: &mut State, dir: CursorHoldDir) {
    let count = cursor_hold_count_mut(state, dir);
    *count = count.saturating_add(1);
    start_cursor_hold(state, dir);
}

fn release_cursor_hold_input(state: &mut State, dir: CursorHoldDir) {
    let count = cursor_hold_count_mut(state, dir);
    *count = count.saturating_sub(1);
    if state.cursor_hold_dir != Some(dir) || cursor_hold_count(state, dir) != 0 {
        return;
    }

    let other = opposite_cursor_hold_dir(dir);
    if cursor_hold_count(state, other) > 0 {
        start_cursor_hold(state, other);
    } else {
        clear_cursor_hold_timer(state);
    }
}

fn start_cursor_hold(state: &mut State, dir: CursorHoldDir) {
    state.cursor_hold_dir = Some(dir);
    state.cursor_hold_delay_left = EDIT_CURSOR_REPEAT_DELAY_SECONDS;
    state.cursor_hold_repeat_left = EDIT_CURSOR_REPEAT_INTERVAL_SECONDS;
}

fn clear_cursor_hold_timer(state: &mut State) {
    state.cursor_hold_dir = None;
    state.cursor_hold_delay_left = 0.0;
    state.cursor_hold_repeat_left = EDIT_CURSOR_REPEAT_INTERVAL_SECONDS;
}

fn clear_cursor_hold_inputs(state: &mut State) {
    state.cursor_hold_up_count = 0;
    state.cursor_hold_down_count = 0;
    clear_cursor_hold_timer(state);
}

const fn opposite_page_hold_dir(dir: PageHoldDir) -> PageHoldDir {
    match dir {
        PageHoldDir::Up => PageHoldDir::Down,
        PageHoldDir::Down => PageHoldDir::Up,
    }
}

const fn page_hold_dir_for_key(code: KeyCode) -> Option<PageHoldDir> {
    match code {
        KeyCode::Semicolon | KeyCode::PageUp => Some(PageHoldDir::Up),
        KeyCode::Quote | KeyCode::PageDown => Some(PageHoldDir::Down),
        _ => None,
    }
}

fn page_hold_count(state: &State, dir: PageHoldDir) -> u8 {
    match dir {
        PageHoldDir::Up => state.page_hold_up_count,
        PageHoldDir::Down => state.page_hold_down_count,
    }
}

fn page_hold_count_mut(state: &mut State, dir: PageHoldDir) -> &mut u8 {
    match dir {
        PageHoldDir::Up => &mut state.page_hold_up_count,
        PageHoldDir::Down => &mut state.page_hold_down_count,
    }
}

fn press_page_hold_input_for_key(state: &mut State, dir: PageHoldDir, repeat: bool) {
    if repeat {
        return;
    }
    press_page_hold_input(state, dir);
    move_cursor_by_page_dir(state, dir);
}

fn press_page_hold_input(state: &mut State, dir: PageHoldDir) {
    let count = page_hold_count_mut(state, dir);
    *count = count.saturating_add(1);
    start_page_hold(state, dir);
}

fn release_page_hold_input(state: &mut State, dir: PageHoldDir) {
    let count = page_hold_count_mut(state, dir);
    *count = count.saturating_sub(1);
    if state.page_hold_dir != Some(dir) || page_hold_count(state, dir) != 0 {
        return;
    }

    let other = opposite_page_hold_dir(dir);
    if page_hold_count(state, other) > 0 {
        start_page_hold(state, other);
    } else {
        clear_page_hold_timer(state);
    }
}

fn start_page_hold(state: &mut State, dir: PageHoldDir) {
    state.page_hold_dir = Some(dir);
    state.page_hold_delay_left = EDIT_CURSOR_REPEAT_DELAY_SECONDS;
    state.page_hold_repeat_left = EDIT_CURSOR_REPEAT_INTERVAL_SECONDS;
}

fn clear_page_hold_timer(state: &mut State) {
    state.page_hold_dir = None;
    state.page_hold_delay_left = 0.0;
    state.page_hold_repeat_left = EDIT_CURSOR_REPEAT_INTERVAL_SECONDS;
}

fn clear_page_hold_inputs(state: &mut State) {
    state.page_hold_up_count = 0;
    state.page_hold_down_count = 0;
    clear_page_hold_timer(state);
}

const fn opposite_music_rate_hold_dir(dir: MusicRateHoldDir) -> MusicRateHoldDir {
    match dir {
        MusicRateHoldDir::Lower => MusicRateHoldDir::Raise,
        MusicRateHoldDir::Raise => MusicRateHoldDir::Lower,
    }
}

const fn music_rate_delta_for_dir(dir: MusicRateHoldDir) -> f32 {
    match dir {
        MusicRateHoldDir::Lower => -MUSIC_RATE_HOTKEY_STEP,
        MusicRateHoldDir::Raise => MUSIC_RATE_HOTKEY_STEP,
    }
}

const fn music_rate_hold_dir_for_key(code: KeyCode) -> Option<MusicRateHoldDir> {
    match code {
        KeyCode::BracketLeft => Some(MusicRateHoldDir::Lower),
        KeyCode::BracketRight => Some(MusicRateHoldDir::Raise),
        _ => None,
    }
}

fn music_rate_hold_count(state: &State, dir: MusicRateHoldDir) -> u8 {
    match dir {
        MusicRateHoldDir::Lower => state.music_rate_hold_lower_count,
        MusicRateHoldDir::Raise => state.music_rate_hold_raise_count,
    }
}

fn music_rate_hold_count_mut(state: &mut State, dir: MusicRateHoldDir) -> &mut u8 {
    match dir {
        MusicRateHoldDir::Lower => &mut state.music_rate_hold_lower_count,
        MusicRateHoldDir::Raise => &mut state.music_rate_hold_raise_count,
    }
}

fn press_music_rate_hold_input(state: &mut State, dir: MusicRateHoldDir) {
    let count = music_rate_hold_count_mut(state, dir);
    if *count > 0 {
        return;
    }
    *count = 1;
    start_music_rate_hold(state, dir);
    if !change_music_rate_by_hold_dir(state, dir) {
        stop_music_rate_hold_dir(state, dir);
    }
}

fn release_music_rate_hold_input(state: &mut State, dir: MusicRateHoldDir) {
    let count = music_rate_hold_count_mut(state, dir);
    if *count == 0 {
        return;
    }
    *count = 0;
    if state.music_rate_hold_dir != Some(dir) {
        return;
    }
    stop_music_rate_hold_dir(state, dir);
}

fn start_music_rate_hold(state: &mut State, dir: MusicRateHoldDir) {
    state.music_rate_hold_dir = Some(dir);
    state.music_rate_hold_delay_left = MUSIC_RATE_REPEAT_DELAY_SECONDS;
    state.music_rate_hold_repeat_left = MUSIC_RATE_REPEAT_INTERVAL_SECONDS;
}

fn clear_music_rate_hold_timer(state: &mut State) {
    state.music_rate_hold_dir = None;
    state.music_rate_hold_delay_left = 0.0;
    state.music_rate_hold_repeat_left = MUSIC_RATE_REPEAT_INTERVAL_SECONDS;
}

fn clear_music_rate_hold_inputs(state: &mut State) {
    state.music_rate_hold_lower_count = 0;
    state.music_rate_hold_raise_count = 0;
    clear_music_rate_hold_timer(state);
}

fn stop_music_rate_hold_dir(state: &mut State, dir: MusicRateHoldDir) {
    let other = opposite_music_rate_hold_dir(dir);
    if music_rate_hold_count(state, other) > 0 {
        start_music_rate_hold(state, other);
    } else {
        clear_music_rate_hold_timer(state);
    }
}

fn update_cursor_hold(state: &mut State, delta_time: f32) {
    if state.menu.is_some() || state.ctrl_held || delta_time <= 0.0 {
        return;
    }
    let Some(dir) = state.cursor_hold_dir else {
        return;
    };
    if cursor_hold_count(state, dir) == 0 {
        clear_cursor_hold_timer(state);
        return;
    }

    let mut remaining = edit_scroll_hold_delta_time(state, delta_time);
    if state.cursor_hold_delay_left > 0.0 {
        let elapsed = remaining.min(state.cursor_hold_delay_left);
        state.cursor_hold_delay_left -= elapsed;
        remaining -= elapsed;
        if state.cursor_hold_delay_left > 0.0 {
            return;
        }
        move_cursor_by_hold_dir(state, dir);
        state.cursor_hold_repeat_left = EDIT_CURSOR_REPEAT_INTERVAL_SECONDS;
    }

    state.cursor_hold_repeat_left -= remaining;
    let mut repeats = 0;
    while state.cursor_hold_repeat_left <= 0.0 && repeats < MAX_EDIT_CURSOR_REPEATS_PER_FRAME {
        move_cursor_by_hold_dir(state, dir);
        state.cursor_hold_repeat_left += EDIT_CURSOR_REPEAT_INTERVAL_SECONDS;
        repeats += 1;
    }
}

fn update_page_hold(state: &mut State, delta_time: f32) {
    if state.menu.is_some() || delta_time <= 0.0 {
        return;
    }
    let Some(dir) = state.page_hold_dir else {
        return;
    };
    if page_hold_count(state, dir) == 0 {
        clear_page_hold_timer(state);
        return;
    }

    let mut remaining = edit_scroll_hold_delta_time(state, delta_time);
    if state.page_hold_delay_left > 0.0 {
        let elapsed = remaining.min(state.page_hold_delay_left);
        state.page_hold_delay_left -= elapsed;
        remaining -= elapsed;
        if state.page_hold_delay_left > 0.0 {
            return;
        }
        move_cursor_by_page_dir(state, dir);
        state.page_hold_repeat_left = EDIT_CURSOR_REPEAT_INTERVAL_SECONDS;
    }

    state.page_hold_repeat_left -= remaining;
    let mut repeats = 0;
    while state.page_hold_repeat_left <= 0.0 && repeats < MAX_EDIT_CURSOR_REPEATS_PER_FRAME {
        move_cursor_by_page_dir(state, dir);
        state.page_hold_repeat_left += EDIT_CURSOR_REPEAT_INTERVAL_SECONDS;
        repeats += 1;
    }
}

fn edit_scroll_hold_delta_time(state: &State, delta_time: f32) -> f32 {
    delta_time * edit_scroll_hold_rate(state.tab_held, state.runtime.tab_acceleration)
}

const fn edit_scroll_hold_rate(tab_held: bool, tab_acceleration: bool) -> f32 {
    if tab_held && tab_acceleration {
        TAB_FAST_MULTIPLIER
    } else {
        1.0
    }
}

fn update_music_rate_hold(state: &mut State, delta_time: f32) {
    if delta_time <= 0.0 {
        return;
    }
    let Some(dir) = state.music_rate_hold_dir else {
        return;
    };
    if music_rate_hold_count(state, dir) == 0 {
        clear_music_rate_hold_timer(state);
        return;
    }

    let mut remaining = delta_time;
    if state.music_rate_hold_delay_left > 0.0 {
        let elapsed = remaining.min(state.music_rate_hold_delay_left);
        state.music_rate_hold_delay_left -= elapsed;
        remaining -= elapsed;
        if state.music_rate_hold_delay_left > 0.0 {
            return;
        }
        if !change_music_rate_by_hold_dir(state, dir) {
            stop_music_rate_hold_dir(state, dir);
            return;
        }
        state.music_rate_hold_repeat_left = MUSIC_RATE_REPEAT_INTERVAL_SECONDS;
    }

    state.music_rate_hold_repeat_left -= remaining;
    let mut repeats = 0;
    while state.music_rate_hold_repeat_left <= 0.0 && repeats < MAX_MUSIC_RATE_REPEATS_PER_FRAME {
        if !change_music_rate_by_hold_dir(state, dir) {
            stop_music_rate_hold_dir(state, dir);
            return;
        }
        state.music_rate_hold_repeat_left += MUSIC_RATE_REPEAT_INTERVAL_SECONDS;
        repeats += 1;
    }
}

fn change_music_rate_by_hold_dir(state: &mut State, dir: MusicRateHoldDir) -> bool {
    change_music_rate(state, music_rate_delta_for_dir(dir))
}

fn move_cursor_by_hold_dir(state: &mut State, dir: CursorHoldDir) {
    let snap = SNAP_BEATS[state.snap_index];
    match dir {
        CursorHoldDir::Up => move_cursor_from_button(state, -snap),
        CursorHoldDir::Down => move_cursor_from_button(state, snap),
    }
}

fn move_cursor_by_page_dir(state: &mut State, dir: PageHoldDir) {
    match dir {
        PageHoldDir::Up => move_cursor_from_button(state, -BEATS_PER_MEASURE),
        PageHoldDir::Down => move_cursor_from_button(state, BEATS_PER_MEASURE),
    }
}

fn move_cursor_from_button(state: &mut State, delta_beats: f32) {
    let delta = if edit_reverse_scroll(state) {
        -delta_beats
    } else {
        delta_beats
    };
    move_cursor(state, delta);
}

fn move_cursor(state: &mut State, delta_beats: f32) {
    let old_beat = state.cursor_beat;
    let next_beat = quantize_beat(old_beat + delta_beats, SNAP_BEATS[state.snap_index]);
    set_cursor(state, next_beat);
    if state.shift_held {
        extend_shift_marker(state, old_beat);
    } else {
        state.shift_anchor = None;
    }
    if !same_beat(old_beat, state.cursor_beat) {
        queue_sfx(state, EDIT_LINE_SOUND);
    }
}

fn extend_shift_marker(state: &mut State, original_beat: f32) {
    let anchor = *state.shift_anchor.get_or_insert(original_beat);
    if same_beat(anchor, state.cursor_beat) {
        state.selection_anchor = None;
        state.selection_end = None;
    } else {
        set_marker_range(state, anchor, state.cursor_beat);
    }
}

fn set_cursor(state: &mut State, beat: f32) {
    let max_beat = max_play_beat(state);
    state.cursor_beat = beat.clamp(MIN_CURSOR_BEAT, max_beat);
}

/// Immediately move the displayed scroll position to the cursor with no
/// smoothing. Used when entering or leaving the edit field, where an animated
/// scroll would be jarring rather than helpful.
fn snap_display_to_cursor(state: &mut State) {
    state.display_beat = state.cursor_beat;
    let music_time = state.gameplay.music_time_for_beat(state.display_beat);
    state.gameplay.seek_practice_display(music_time);
}

/// Ease the displayed scroll position toward the cursor so seeking through the
/// chart scrolls smoothly instead of redrawing into the next position. Large
/// jumps snap instantly to stay responsive.
fn update_display_scroll(state: &mut State, delta_time: f32) {
    if delta_time <= 0.0 || same_beat(state.display_beat, state.cursor_beat) {
        return;
    }
    let next = next_display_beat(state.display_beat, state.cursor_beat, delta_time);
    if same_beat(next, state.cursor_beat) {
        snap_display_to_cursor(state);
        return;
    }
    state.display_beat = next;
    let music_time = state.gameplay.music_time_for_beat(state.display_beat);
    state.gameplay.seek_practice_display(music_time);
}

/// Pure easing step for [`update_display_scroll`], split out for testing.
/// Returns the cursor directly when the gap is tiny (settle) or huge (snap),
/// otherwise an exponential step toward it.
fn next_display_beat(display_beat: f32, cursor_beat: f32, delta_time: f32) -> f32 {
    let diff = cursor_beat - display_beat;
    if diff.abs() <= DISPLAY_SCROLL_SNAP_EPSILON || diff.abs() > DISPLAY_SCROLL_MAX_SMOOTH_BEATS {
        return cursor_beat;
    }
    let t = 1.0 - (-delta_time / DISPLAY_SCROLL_TIME_CONSTANT).exp();
    display_beat + diff * t
}

fn change_snap(state: &mut State, delta: isize) {
    let next = state.snap_index as isize + delta;
    if next < 0 || next >= SNAP_LABELS.len() as isize {
        return;
    }
    state.snap_index = next as usize;
    let quantized = quantize_beat(state.cursor_beat, SNAP_BEATS[state.snap_index]);
    set_cursor(state, quantized);
    queue_sfx(state, EDIT_SNAP_SOUND);
}

fn change_edit_scroll_speed(state: &mut State, delta: isize) {
    let last = EDIT_SCROLL_SPEEDS.len() as isize - 1;
    let next = (state.edit_scroll_speed_index as isize + delta).clamp(0, last) as usize;
    if next == state.edit_scroll_speed_index {
        return;
    }
    state.edit_scroll_speed_index = next;
    set_flash_tr(state, "FlashZoomChanged");
    queue_sfx(state, EDIT_MARKER_SOUND);
}

fn quantized_music_rate(current: f32, delta: f32) -> f32 {
    let current_step = (current / MUSIC_RATE_HOTKEY_STEP).round() as i32;
    let delta_step = (delta / MUSIC_RATE_HOTKEY_STEP).round() as i32;
    let min_step = (MUSIC_RATE_HOTKEY_MIN / MUSIC_RATE_HOTKEY_STEP).round() as i32;
    let max_step = (MUSIC_RATE_HOTKEY_MAX / MUSIC_RATE_HOTKEY_STEP).round() as i32;
    (current_step + delta_step).clamp(min_step, max_step) as f32 * MUSIC_RATE_HOTKEY_STEP
}

fn change_music_rate(state: &mut State, delta: f32) -> bool {
    let current = state.gameplay.music_rate();
    let new_rate = quantized_music_rate(current, delta);
    if (new_rate - current).abs() <= f32::EPSILON {
        queue_sfx(state, EDIT_INVALID_SOUND);
        set_music_rate_flash(state, "FlashMusicRateLimit", current);
        return false;
    }
    let changed = state.gameplay.set_music_rate(new_rate);
    state
        .pending_profile
        .push(crate::SimplyLoveProfileRequest::SetMusicRate(new_rate));
    state
        .gameplay
        .push_audio_command(GameplayAudioCommand::SetMusicRate(new_rate));
    if changed {
        set_music_rate_flash(state, "FlashMusicRate", new_rate);
        queue_sfx(state, EDIT_LINE_SOUND);
    }
    changed
}

fn set_music_rate_flash(state: &mut State, key: &str, rate: f32) {
    let bpm_str = effective_bpm_str(state, rate);
    let text = i18n::tr_fmt(
        "Practice",
        key,
        &[("rate", &fmt_music_rate(rate)), ("bpm", &bpm_str)],
    )
    .replace("\\n", "\n");
    state.flash = Some((text, FLASH_DURATION_SECS));
}

fn effective_bpm_str(state: &State, rate: f32) -> String {
    let song = state.gameplay.song();
    let chart = state.gameplay.charts().first().map(|c| c.as_ref());
    let is_random = chart
        .is_some_and(|c| matches!(c.display_bpm, Some(deadsync_chart::ChartDisplayBpm::Random)));
    if is_random {
        return "???".to_string();
    }
    let reference_bpm = song
        .chart_display_bpm_range(chart)
        .map(|(_, hi)| hi as f32)
        .unwrap_or(song.max_bpm as f32);
    let reference_bpm = if reference_bpm.is_finite() && reference_bpm > 0.0 {
        reference_bpm
    } else {
        120.0
    };
    let effective_bpm = f64::from(reference_bpm) * f64::from(rate);
    if (effective_bpm - effective_bpm.round()).abs() < 0.05 {
        format!("{}", effective_bpm.round() as i32)
    } else {
        format!("{effective_bpm:.1}")
    }
}

fn set_flash_tr(state: &mut State, key: &str) {
    state.flash = Some((i18n::tr("Practice", key).to_string(), FLASH_DURATION_SECS));
}

fn fmt_music_rate(rate: f32) -> String {
    let scaled = (rate * 100.0).round() as i32;
    let int_part = scaled / 100;
    let frac2 = (scaled % 100).abs();
    if frac2 == 0 {
        format!("{int_part}")
    } else if frac2 % 10 == 0 {
        format!("{}.{}", int_part, frac2 / 10)
    } else {
        format!("{int_part}.{frac2:02}")
    }
}

fn seek_chart_note(state: &mut State, dir: i32) {
    let current = state.cursor_beat;
    let target = if dir < 0 {
        state
            .gameplay
            .notes()
            .iter()
            .filter(|note| note.can_be_judged && note.beat < current - BEAT_EPSILON)
            .map(|note| note.beat)
            .max_by(|a, b| a.total_cmp(b))
    } else {
        state
            .gameplay
            .notes()
            .iter()
            .filter(|note| note.can_be_judged && note.beat > current + BEAT_EPSILON)
            .map(|note| note.beat)
            .min_by(|a, b| a.total_cmp(b))
    };
    if let Some(beat) = target {
        set_cursor(state, beat);
        queue_sfx(state, EDIT_LINE_SOUND);
    } else {
        queue_sfx(state, EDIT_INVALID_SOUND);
    }
}

fn set_area_marker(state: &mut State) {
    queue_sfx(state, EDIT_MARKER_SOUND);
    match (state.selection_anchor, state.selection_end) {
        (None, None) => {
            state.selection_anchor = Some(state.cursor_beat);
            set_flash_tr(state, "FlashAreaMarkerStartSet");
        }
        (Some(begin), None) => {
            if same_beat(begin, state.cursor_beat) {
                clear_selection(state);
            } else {
                set_marker_range(state, begin, state.cursor_beat);
                set_flash_tr(state, "FlashAreaMarkerEndSet");
            }
        }
        _ => {
            state.selection_anchor = Some(state.cursor_beat);
            state.selection_end = None;
            state.shift_anchor = None;
            set_flash_tr(state, "FlashAreaMarkerStartSet");
        }
    }
}

fn set_selection_start(state: &mut State) {
    if state
        .selection_end
        .is_some_and(|end| state.cursor_beat >= end)
    {
        set_flash_tr(state, "FlashInvalidSelectionStart");
        queue_sfx(state, EDIT_INVALID_SOUND);
        return;
    }
    state.selection_anchor = Some(state.cursor_beat);
    set_flash_tr(state, "FlashSelectionStartSet");
    queue_sfx(state, EDIT_MARKER_SOUND);
}

fn set_selection_end(state: &mut State) {
    if state
        .selection_anchor
        .is_some_and(|start| state.cursor_beat <= start)
    {
        set_flash_tr(state, "FlashInvalidSelectionEnd");
        queue_sfx(state, EDIT_INVALID_SOUND);
        return;
    }
    state.selection_end = Some(state.cursor_beat);
    set_flash_tr(state, "FlashSelectionEndSet");
    queue_sfx(state, EDIT_MARKER_SOUND);
}

fn clear_selection(state: &mut State) {
    state.selection_anchor = None;
    state.selection_end = None;
    state.shift_anchor = None;
    set_flash_tr(state, "FlashSelectionCleared");
}

fn set_marker_range(state: &mut State, a: f32, b: f32) {
    state.selection_anchor = Some(a.min(b));
    state.selection_end = Some(a.max(b));
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

fn clamp_marker_beat(beat: f32, max_beat: f32) -> f32 {
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

fn selection_range(state: &State) -> Option<(f32, f32)> {
    let a = state.selection_anchor?;
    let b = state.selection_end?;
    Some((a.min(b), a.max(b)))
}

fn edit_reverse_scroll(state: &State) -> bool {
    state.gameplay.profiles().first().is_some_and(|p| {
        p.scroll_option
            .contains(profile_data::ScrollOption::Reverse)
    })
}

fn max_play_beat(state: &State) -> f32 {
    let note_beat = state
        .gameplay
        .notes()
        .iter()
        .map(|note| note.hold.as_ref().map_or(note.beat, |hold| hold.end_beat))
        .fold(MIN_CURSOR_BEAT, f32::max);
    let song_beat = state
        .gameplay
        .beat_for_music_time(state.gameplay.song().music_length_seconds.max(0.0));
    note_beat.max(song_beat).max(MIN_CURSOR_BEAT)
}

fn append_edit_markers(state: &State, actors: &mut Vec<Actor>) {
    let hud = &state.runtime.hud;
    let play_style = hud.play_style;
    let is_p2_single = profile_data::is_single_p2_side(play_style, hud.player_side);

    match play_style {
        profile_data::PlayStyle::Versus => {
            append_player_markers(state, actors, 0, MarkerPlacement::P1, play_style, false);
            append_player_markers(state, actors, 1, MarkerPlacement::P2, play_style, false);
        }
        profile_data::PlayStyle::Single | profile_data::PlayStyle::Double => {
            let placement = if is_p2_single {
                MarkerPlacement::P2
            } else {
                MarkerPlacement::P1
            };
            append_player_markers(state, actors, 0, placement, play_style, true);
        }
    }
}

fn append_player_markers(
    state: &State,
    actors: &mut Vec<Actor>,
    player_idx: usize,
    placement: MarkerPlacement,
    play_style: profile_data::PlayStyle,
    center_1player_notefield: bool,
) {
    if player_idx >= state.gameplay.num_players() {
        return;
    }

    let col_start = player_idx * state.gameplay.cols_per_player();
    let col_end = (col_start + state.gameplay.cols_per_player())
        .min(state.gameplay.num_cols())
        .min(state.gameplay.notefield_column_scroll_dir_count());
    let num_cols = col_end.saturating_sub(col_start);
    if num_cols == 0 {
        return;
    }

    let profile = &state.gameplay.profiles()[player_idx];
    let offset_sign = match placement {
        MarkerPlacement::P1 => -1.0,
        MarkerPlacement::P2 => 1.0,
    };
    let offset_x = offset_sign * profile.note_field_offset_x.clamp(0, 50) as f32;
    let offset_y = profile.note_field_offset_y.clamp(-50, 50) as f32;
    let clamped_width = screen_width().clamp(640.0, 854.0);
    let centered_one_side = state.gameplay.num_players() == 1
        && play_style == profile_data::PlayStyle::Single
        && center_1player_notefield;
    let centered_both_sides =
        state.gameplay.num_players() == 1 && play_style == profile_data::PlayStyle::Double;
    let base_x = if state.gameplay.num_players() == 2 {
        match placement {
            MarkerPlacement::P1 => screen_center_x() - clamped_width * 0.25,
            MarkerPlacement::P2 => screen_center_x() + clamped_width * 0.25,
        }
    } else if centered_one_side || centered_both_sides {
        screen_center_x()
    } else {
        match placement {
            MarkerPlacement::P1 => screen_center_x() - clamped_width * 0.25,
            MarkerPlacement::P2 => screen_center_x() + clamped_width * 0.25,
        }
    };
    let center_x = base_x + offset_x;
    let spacing_mult = if player_idx < state.gameplay.num_players() {
        spacing_multiplier_for_percent(state.gameplay.profiles()[player_idx].spacing_percent)
    } else {
        1.0
    };
    let field_zoom = practice_edit_field_zoom();
    let width = (num_cols as f32 * ScrollSpeedSetting::ARROW_SPACING * spacing_mult * field_zoom)
        .max(ScrollSpeedSetting::ARROW_SPACING);
    let geom = PracticeFieldGeom {
        player_idx,
        col_start,
        center_x,
        offset_y,
        width,
        zoom: field_zoom,
    };
    let marker_phase = (state.gameplay.total_elapsed_in_screen() * std::f32::consts::PI).sin();
    let marker_shade = 0.75 + marker_phase * 0.25;
    let cursor_y = marker_y_for_beat(state, player_idx, col_start, offset_y, state.cursor_beat);
    append_timing_segment_labels(state, actors, geom);
    append_field_cursor(
        actors,
        center_x,
        cursor_y,
        width,
        field_zoom,
        state.snap_index,
    );

    match (state.selection_anchor, state.selection_end) {
        (Some(begin), Some(end)) if end > begin => {
            let y1 = marker_y_for_beat(state, player_idx, col_start, offset_y, begin);
            let y2 = marker_y_for_beat(state, player_idx, col_start, offset_y, end);
            append_marker_area(actors, center_x, y1, y2, width);
        }
        (Some(begin), _) => {
            let y = marker_y_for_beat(state, player_idx, col_start, offset_y, begin);
            append_marker_bar(actors, center_x, y, width, marker_shade);
        }
        (None, Some(end)) => {
            let y = marker_y_for_beat(state, player_idx, col_start, offset_y, end);
            append_marker_bar(actors, center_x, y, width, marker_shade);
        }
        (None, None) => {}
    }
}

fn append_timing_segment_labels(state: &State, actors: &mut Vec<Actor>, geom: PracticeFieldGeom) {
    let Some(gameplay_chart) = state.gameplay.gameplay_chart(geom.player_idx) else {
        return;
    };
    let timing = &gameplay_chart.timing_segments;
    let glow_alpha = timing_label_glow_alpha(state.gameplay.total_elapsed_in_screen());
    append_timing_labels_from_segments(state, actors, geom, timing, glow_alpha);
}

fn append_timing_labels_from_segments(
    state: &State,
    actors: &mut Vec<Actor>,
    geom: PracticeFieldGeom,
    timing: &TimingSegments,
    glow_alpha: f32,
) {
    // PARITY[ITGmania NoteField::DrawPrimitives]: segment text draw order.
    for seg in &timing.scrolls {
        append_timing_segment_label(
            state,
            actors,
            geom,
            SCROLL_LABEL_STYLE,
            fmt_itg_float(seg.ratio),
            seg.beat,
            glow_alpha,
        );
    }
    for &(beat, bpm) in &timing.bpms {
        append_timing_segment_label(
            state,
            actors,
            geom,
            BPM_LABEL_STYLE,
            fmt_itg_float(bpm),
            beat,
            glow_alpha,
        );
    }
    for seg in &timing.stops {
        append_timing_segment_label(
            state,
            actors,
            geom,
            STOP_LABEL_STYLE,
            fmt_itg_float(seg.duration),
            seg.beat,
            glow_alpha,
        );
    }
    for seg in &timing.delays {
        append_timing_segment_label(
            state,
            actors,
            geom,
            DELAY_LABEL_STYLE,
            fmt_itg_float(seg.duration),
            seg.beat,
            glow_alpha,
        );
    }
    for seg in &timing.warps {
        append_timing_segment_label(
            state,
            actors,
            geom,
            WARP_LABEL_STYLE,
            fmt_itg_float(seg.length),
            seg.beat,
            glow_alpha,
        );
    }
    for seg in &timing.time_signatures {
        append_timing_segment_label(
            state,
            actors,
            geom,
            TIME_SIG_LABEL_STYLE,
            format!("{}\n--\n{}", seg.numerator, seg.denominator),
            seg.beat,
            glow_alpha,
        );
    }
    for seg in &timing.speeds {
        append_timing_segment_label(
            state,
            actors,
            geom,
            SPEED_LABEL_STYLE,
            timing_speed_label(*seg),
            seg.beat,
            glow_alpha,
        );
    }
    for seg in &timing.fakes {
        append_timing_segment_label(
            state,
            actors,
            geom,
            FAKE_LABEL_STYLE,
            fmt_itg_float(seg.length),
            seg.beat,
            glow_alpha,
        );
    }
}

fn append_timing_segment_label(
    state: &State,
    actors: &mut Vec<Actor>,
    geom: PracticeFieldGeom,
    style: TimingLabelStyle,
    text: String,
    beat: f32,
    glow_alpha: f32,
) {
    let y = marker_y_for_beat(state, geom.player_idx, geom.col_start, geom.offset_y, beat);
    if !timing_label_y_is_visible(y) {
        return;
    }
    let x = timing_label_x(geom.center_x, geom.width, geom.zoom, style);
    let align_x = if style.left_side { 1.0 } else { 0.0 };
    let color = style.color;
    actors.push(act!(text:
        font("miso"):
        settext(text):
        align(align_x, 0.5):
        xy(x, y):
        zoom(geom.zoom):
        wrapwidthpixels(300.0):
        diffuse(color[0], color[1], color[2], color[3]):
        glow(1.0, 1.0, 1.0, glow_alpha):
        shadowlength(2.0):
        z(EDIT_TIMING_LABEL_Z)
    ));
}

fn timing_label_y_is_visible(y: f32) -> bool {
    let margin = practice_marker_bar_height();
    y.is_finite() && y >= -margin && y <= screen_height() + margin
}

fn timing_label_x(center_x: f32, width: f32, zoom: f32, style: TimingLabelStyle) -> f32 {
    let side = if style.left_side { -1.0 } else { 1.0 };
    center_x + side * (width * 0.5 + style.offset_x * zoom)
}

fn timing_label_glow_alpha(elapsed: f32) -> f32 {
    let phase = elapsed * std::f32::consts::TAU / 6.0;
    (phase.cos() * 0.5 + 0.5).clamp(0.0, 1.0)
}

fn timing_speed_label(seg: SpeedSegment) -> String {
    let unit = match seg.unit {
        SpeedUnit::Seconds => "S",
        SpeedUnit::Beats => "B",
    };
    format!(
        "{}\n{}\n{}",
        fmt_itg_float(seg.ratio),
        unit,
        fmt_itg_float(seg.delay)
    )
}

fn fmt_itg_float(value: f32) -> String {
    format!("{value:.6}")
}

fn marker_y_for_beat(
    state: &State,
    player_idx: usize,
    col_start: usize,
    offset_y: f32,
    beat: f32,
) -> f32 {
    let dir = state
        .gameplay
        .notefield_column_scroll_dir(col_start)
        .signum();
    let dir = if dir.abs() <= f32::EPSILON { 1.0 } else { dir };
    let receptor_y = practice_edit_cursor_y() + offset_y;
    let field_zoom = practice_edit_field_zoom();
    let scroll_speed = practice_edit_scroll_speed(state);
    let current_beat = state.gameplay.visible_beat(player_idx);
    let travel = practice_edit_beat_travel(
        beat,
        current_beat,
        field_zoom,
        scroll_speed,
        state.gameplay.scroll_reference_bpm(),
        state.gameplay.music_rate(),
    );
    receptor_y + dir * travel
}

fn practice_edit_beat_travel(
    beat: f32,
    current_beat: f32,
    field_zoom: f32,
    scroll_speed: ScrollSpeedSetting,
    reference_bpm: f32,
    music_rate: f32,
) -> f32 {
    let player_multiplier = scroll_speed.beat_multiplier(reference_bpm, music_rate);
    (beat - current_beat) * ScrollSpeedSetting::ARROW_SPACING * field_zoom * player_multiplier
}

fn append_marker_area(actors: &mut Vec<Actor>, center_x: f32, y1: f32, y2: f32, width: f32) {
    if !y1.is_finite() || !y2.is_finite() {
        return;
    }
    let marker_height = practice_marker_bar_height();
    let top = y1.min(y2).max(-marker_height);
    let bottom = y1.max(y2).min(screen_height() + marker_height);
    if bottom - top <= 1.0 {
        return;
    }
    actors.push(act!(quad:
        align(0.5, 0.0):
        xy(center_x, top):
        zoomto(width, bottom - top):
        diffuse(1.0, 0.0, 0.0, 0.3):
        z(MARKER_Z - 1.0)
    ));
}

fn append_marker_bar(actors: &mut Vec<Actor>, center_x: f32, y: f32, width: f32, shade: f32) {
    let marker_height = practice_marker_bar_height();
    if !y.is_finite() || y < -marker_height || y > screen_height() + marker_height {
        return;
    }
    actors.push(act!(quad:
        align(0.5, 0.5):
        xy(center_x, y):
        zoomto(width, marker_height):
        diffuse(shade, shade, shade, 0.5):
        z(MARKER_Z)
    ));
}

fn append_field_cursor(
    actors: &mut Vec<Actor>,
    center_x: f32,
    y: f32,
    width: f32,
    zoom: f32,
    snap_index: usize,
) {
    let marker_height = practice_marker_bar_height();
    if !y.is_finite() || y < -marker_height || y > screen_height() + marker_height {
        return;
    }
    let side_gap = ScrollSpeedSetting::ARROW_SPACING * 0.5 * zoom;
    let frame = snap_index.min(SNAP_LABELS.len().saturating_sub(1)) as u32;
    append_snap_cursor_heart(actors, center_x - width * 0.5 - side_gap, y, zoom, frame);
    append_snap_cursor_heart(actors, center_x + width * 0.5 + side_gap, y, zoom, frame);
}

fn append_snap_cursor_heart(actors: &mut Vec<Actor>, x: f32, y: f32, zoom: f32, frame: u32) {
    actors.push(act!(sprite(EDIT_FIELD_CURSOR_TEX):
        align(0.5, 0.5):
        xy(x, y):
        zoom(zoom * EDIT_SNAP_CURSOR_ZOOM):
        setstate(frame):
        diffuse(1.0, 1.0, 1.0, 1.0):
        z(EDIT_FIELD_CURSOR_Z)
    ));
}

fn append_edit_overlay(state: &State, actors: &mut Vec<Actor>) {
    let pc = practice_player_color(state);
    let header_font = machine_font_key(state.gameplay.machine_font(), FontRole::Header);
    actors.push(act!(text:
        font(header_font):
        settext(i18n::tr("Practice", "TitlePracticeMode")):
        align(1.0, 0.5):
        xy(screen_width() - 35.0, 10.0):
        zoom(EDIT_HELP_HEADER_ZOOM):
        diffuse(pc[0], pc[1], pc[2], 1.0):
        z(3000)
    ));
    actors.push(act!(quad:
        align(1.0, 0.5):
        xy(screen_width(), 10.0):
        zoomto(30.0, 1.0):
        diffuse(1.0, 1.0, 1.0, 0.75):
        z(2999)
    ));

    let status = edit_info_text(state);
    append_help_section(
        actors,
        i18n::tr("Practice", "HelpSidebarNavigatingTitle"),
        i18n::tr("Practice", "HelpSidebarNavigatingBody"),
        0.0,
        pc,
        header_font,
    );
    append_help_section(
        actors,
        i18n::tr("Practice", "HelpSidebarMenusTitle"),
        i18n::tr("Practice", "HelpSidebarMenusBody"),
        EDIT_HELP_MENU_Y,
        pc,
        header_font,
    );
    append_help_section(
        actors,
        i18n::tr("Practice", "HelpSidebarMiscTitle"),
        i18n::tr("Practice", "HelpSidebarMiscBody"),
        EDIT_HELP_MISC_Y,
        pc,
        header_font,
    );
    actors.push(act!(text:
        font("miso"):
        settext(status):
        align(0.0, 0.0):
        xy(screen_width() - 150.0, 20.0):
        zoom(0.625):
        maxwidth(145.0):
        shadowlength(1.0):
        z(3000)
    ));
}

fn append_flash_overlay(state: &State, actors: &mut Vec<Actor>) {
    if let Some((text, remaining)) = state.flash.as_ref() {
        let alpha = remaining.clamp(0.0, 1.0);
        let label = text.clone();
        actors.push(act!(text:
            font("miso"):
            settext(label):
            align(0.5, 0.5):
            xy(screen_width() * 0.5, screen_height() - 80.0):
            zoom(0.9):
            diffuse(1.0, 1.0, 1.0, alpha):
            shadowlength(2.0):
            z(3001)
        ));
    }
}

fn practice_player_color(state: &State) -> [f32; 4] {
    color::simply_love_rgba(state.gameplay.active_color_index())
}

fn edit_info_text(state: &State) -> String {
    let chart = &state.gameplay.charts()[0];
    let song = state.gameplay.song();
    let current_second = state.gameplay.music_time_for_beat(state.cursor_beat);
    let difficulty = color::difficulty_display_name_for_song(&chart.difficulty, &song.title, true);
    let snap = SNAP_LABELS[state.snap_index];
    let mut status = String::new();
    status.push_str(&i18n::tr_fmt(
        "Practice",
        "InfoCurrentBeat",
        &[("beat", &format!("{:.3}", state.cursor_beat))],
    ));
    status.push('\n');
    status.push_str(&i18n::tr_fmt(
        "Practice",
        "InfoCurrentSecond",
        &[("sec", &format!("{current_second:.6}"))],
    ));
    status.push('\n');
    status.push_str(&i18n::tr_fmt("Practice", "InfoSnapTo", &[("snap", snap)]));
    status.push('\n');
    if let Some(selection) = selection_info_text(state) {
        status.push_str(&selection);
        status.push('\n');
    }
    status.push_str(&i18n::tr_fmt(
        "Practice",
        "InfoDifficulty",
        &[
            ("difficulty", difficulty),
            ("meter", &chart.meter.to_string()),
        ],
    ));
    status.push_str("\n\n");
    push_info_line(
        &mut status,
        &i18n::tr("Practice", "InfoMainTitle"),
        &song.title,
    );
    push_info_line(
        &mut status,
        &i18n::tr("Practice", "InfoSubtitle"),
        &song.subtitle,
    );
    push_info_line(
        &mut status,
        &i18n::tr("Practice", "InfoDescription"),
        &chart.description,
    );
    push_info_line(
        &mut status,
        &i18n::tr("Practice", "InfoChartName"),
        &chart.chart_name,
    );
    push_info_line(
        &mut status,
        &i18n::tr("Practice", "InfoStepAuthor"),
        &chart.step_artist,
    );
    push_info_line(
        &mut status,
        &i18n::tr("Practice", "InfoChartStyle"),
        &chart.chart_type,
    );
    status.push('\n');
    let totals = state.gameplay.display_totals_for_player(0);
    let stat_lines: [(&str, String); 8] = [
        ("InfoNumSteps", chart.stats.total_steps.to_string()),
        ("InfoNumJumps", chart.stats.jumps.to_string()),
        ("InfoNumHands", chart.stats.hands.to_string()),
        ("InfoNumHolds", totals.holds_total.to_string()),
        ("InfoNumMines", totals.mines_total.to_string()),
        ("InfoNumRolls", totals.rolls_total.to_string()),
        ("InfoNumLifts", chart.stats.lifts.to_string()),
        ("InfoNumFakes", chart.stats.fakes.to_string()),
    ];
    for (idx, (key, count)) in stat_lines.iter().enumerate() {
        if idx > 0 {
            status.push('\n');
        }
        status.push_str(&i18n::tr_fmt("Practice", key, &[("count", count)]));
    }
    status
}

fn selection_info_text(state: &State) -> Option<String> {
    match (state.selection_anchor, state.selection_end) {
        (Some(start), Some(stop)) if stop > start => Some(
            i18n::tr_fmt(
                "Practice",
                "InfoSelectionBeatRange",
                &[
                    ("start", &format!("{start:.3}")),
                    ("stop", &format!("{stop:.3}")),
                ],
            )
            .to_string(),
        ),
        (Some(start), None) => Some(
            i18n::tr_fmt(
                "Practice",
                "InfoSelectionBeatStart",
                &[("start", &format!("{start:.3}"))],
            )
            .to_string(),
        ),
        (None, Some(stop)) => Some(
            i18n::tr_fmt(
                "Practice",
                "InfoSelectionBeatEnd",
                &[("stop", &format!("{stop:.3}"))],
            )
            .to_string(),
        ),
        _ => None,
    }
}

fn push_info_line(status: &mut String, label: &str, value: &str) {
    if value.is_empty() {
        return;
    }
    status.push_str(label);
    status.push_str(":\n  ");
    push_clipped(status, value, EDIT_INFO_VALUE_CHARS);
    status.push('\n');
}

fn push_clipped(status: &mut String, value: &str, max_chars: usize) {
    for (idx, ch) in value.chars().enumerate() {
        if idx == max_chars {
            status.push_str("...");
            return;
        }
        status.push(ch);
    }
}

fn append_main_menu(state: &State, actors: &mut Vec<Actor>) {
    let Some(menu) = state.menu else {
        return;
    };
    let row_count = menu.def.rows.len();
    let selected_color = practice_player_color(state);
    for (idx, row) in menu.def.rows.iter().enumerate() {
        append_menu_row(
            actors,
            idx,
            row_count,
            menu.selected == idx,
            row.label.get(),
            selected_color,
        );
    }
}

fn append_menu_row(
    actors: &mut Vec<Actor>,
    idx: usize,
    row_count: usize,
    selected: bool,
    label: Arc<str>,
    selected_color: [f32; 4],
) {
    let y = menu_row_y(idx, row_count);
    let bg_x = screen_center_x();
    let bg_w = widescale(543.0, 720.0);
    let (bg, fg) = if selected {
        ([0.161, 0.196, 0.22, 0.95], selected_color)
    } else {
        ([0.027, 0.063, 0.086, 0.95], [1.0, 1.0, 1.0, 1.0])
    };
    actors.push(act!(quad:
        align(0.5, 0.5):
        xy(bg_x, y):
        zoomto(bg_w, EDIT_MENU_ROW_BG_HEIGHT):
        diffuse(bg[0], bg[1], bg[2], bg[3]):
        z(3100)
    ));
    actors.push(act!(text:
        font("miso"):
        settext(label):
        align(0.0, 0.5):
        xy(screen_center_x() - EDIT_MENU_TITLE_X_OFFSET, y):
        zoom(EDIT_MENU_TEXT_ZOOM):
        diffuse(fg[0], fg[1], fg[2], fg[3]):
        shadowlength(1.0):
        z(3101)
    ));
}

fn menu_row_y(idx: usize, row_count: usize) -> f32 {
    if row_count.is_multiple_of(2) {
        screen_center_y()
            + EDIT_MENU_ROW_HEIGHT * (idx as f32 - (row_count / 2) as f32)
            + EDIT_MENU_ROW_HEIGHT * 0.5
    } else {
        screen_center_y() + EDIT_MENU_ROW_HEIGHT * (idx as f32 - (row_count / 2) as f32)
    }
}

fn append_help_section(
    actors: &mut Vec<Actor>,
    label: Arc<str>,
    body: Arc<str>,
    y: f32,
    player_color: [f32; 4],
    header_font: &'static str,
) {
    actors.push(act!(text:
        font(header_font):
        settext(label):
        align(0.0, 0.5):
        xy(35.0, y + 10.0):
        zoom(EDIT_HELP_HEADER_ZOOM):
        diffuse(player_color[0], player_color[1], player_color[2], 1.0):
        z(3000)
    ));
    actors.push(act!(quad:
        align(0.0, 0.5):
        xy(0.0, y + 10.0):
        zoomto(30.0, 1.0):
        diffuse(1.0, 1.0, 1.0, 0.75):
        z(2999)
    ));
    actors.push(act!(text:
        font("miso"):
        settext(body):
        align(0.0, 0.0):
        xy(10.0, y + 20.0):
        zoom(0.55):
        diffuse(1.0, 1.0, 1.0, 1.0):
        shadowlength(1.0):
        z(3000)
    ));
}

#[cfg(test)]
mod tests {
    use super::{
        BPM_LABEL_STYLE, CursorHoldDir, DISPLAY_SCROLL_MAX_SMOOTH_BEATS,
        DISPLAY_SCROLL_SNAP_EPSILON, HELP_MENU, MAIN_MENU, MUSIC_RATE_HOTKEY_MAX,
        MUSIC_RATE_HOTKEY_MIN, MUSIC_RATE_HOTKEY_STEP, MenuDef, MusicRateHoldDir, PageHoldDir,
        PracticeNavMode, SPEED_LABEL_STYLE, TAB_FAST_MULTIPLIER, clamp_selection,
        edit_cursor_hold_dir_for_action_in_mode, edit_scroll_hold_rate,
        edit_snap_delta_for_action_in_mode, fmt_itg_float, fmt_music_rate, gameplay_hotkey_input,
        menu_step_delta_for_action_in_mode, music_rate_delta_for_dir, music_rate_hold_dir_for_key,
        next_display_beat, page_hold_dir_for_key, practice_edit_beat_travel,
        practice_nav_mode_from_config, prepend_pending_effects, prepend_pending_sfx,
        quantized_music_rate, timing_label_glow_alpha, timing_label_x, timing_speed_label,
    };
    use crate::SimplyLoveRuntimeRequest;
    use crate::assets::i18n;
    use crate::screens::{Screen, ThemeEffect};
    use deadsync_gameplay::{GameplayOffsetAdjustKey, GameplayRawKeyInput};
    use deadsync_input::KeyCode;
    use deadsync_input::VirtualAction;
    use deadsync_rules::scroll::ScrollSpeedSetting;
    use deadsync_rules::timing::{SpeedSegment, SpeedUnit};
    use deadsync_theme::AudioRequest;

    /// Every i18n key the practice screen looks up at runtime, outside of the
    /// menu definitions (which already have their own coverage tests). Keep
    /// this in sync with call sites — the test below proves each key resolves.
    const PRACTICE_RUNTIME_KEYS: &[&str] = &[
        "TitlePracticeMode",
        "HelpSidebarNavigatingTitle",
        "HelpSidebarNavigatingBody",
        "HelpSidebarMenusTitle",
        "HelpSidebarMenusBody",
        "HelpSidebarMiscTitle",
        "HelpSidebarMiscBody",
        "FlashZoomChanged",
        "FlashMusicRate",
        "FlashMusicRateLimit",
        "FlashAreaMarkerStartSet",
        "FlashAreaMarkerEndSet",
        "FlashInvalidSelectionStart",
        "FlashSelectionStartSet",
        "FlashInvalidSelectionEnd",
        "FlashSelectionEndSet",
        "FlashSelectionCleared",
        "FlashAutoplayOn",
        "FlashAutoplayOff",
        "FlashTicksAssist",
        "FlashTicksHit",
        "FlashTicksOff",
        "FlashAutosyncOff",
        "FlashAutosyncSong",
        "FlashAutosyncMachine",
        "FlashGlobalOffset",
        "FlashSongOffset",
        "InfoCurrentBeat",
        "InfoCurrentSecond",
        "InfoSnapTo",
        "InfoDifficulty",
        "InfoSelectionBeatRange",
        "InfoSelectionBeatStart",
        "InfoSelectionBeatEnd",
        "InfoMainTitle",
        "InfoSubtitle",
        "InfoDescription",
        "InfoChartName",
        "InfoStepAuthor",
        "InfoChartStyle",
        "InfoNumSteps",
        "InfoNumJumps",
        "InfoNumHands",
        "InfoNumHolds",
        "InfoNumMines",
        "InfoNumRolls",
        "InfoNumLifts",
        "InfoNumFakes",
    ];

    #[test]
    fn practice_nav_mode_follows_dedicated_menu_config() {
        assert_eq!(
            practice_nav_mode_from_config(false, false),
            PracticeNavMode::GameplayButtons
        );
        assert_eq!(
            practice_nav_mode_from_config(true, false),
            PracticeNavMode::DedicatedFiveKey
        );
        assert_eq!(
            practice_nav_mode_from_config(true, true),
            PracticeNavMode::DedicatedThreeKey
        );
    }

    #[test]
    fn dedicated_five_key_ignores_gameplay_arrows_in_practice_editing() {
        assert_eq!(
            edit_cursor_hold_dir_for_action_in_mode(
                PracticeNavMode::DedicatedFiveKey,
                VirtualAction::p1_up,
            ),
            None
        );
        assert_eq!(
            edit_cursor_hold_dir_for_action_in_mode(
                PracticeNavMode::DedicatedFiveKey,
                VirtualAction::p1_menu_up,
            ),
            Some(CursorHoldDir::Up)
        );
        assert_eq!(
            edit_snap_delta_for_action_in_mode(
                PracticeNavMode::DedicatedFiveKey,
                VirtualAction::p1_left,
            ),
            None
        );
        assert_eq!(
            edit_snap_delta_for_action_in_mode(
                PracticeNavMode::DedicatedFiveKey,
                VirtualAction::p1_menu_left,
            ),
            Some(-1)
        );
    }

    #[test]
    fn dedicated_three_key_uses_menu_left_right_for_practice_navigation() {
        assert_eq!(
            edit_cursor_hold_dir_for_action_in_mode(
                PracticeNavMode::DedicatedThreeKey,
                VirtualAction::p1_menu_left,
            ),
            Some(CursorHoldDir::Up)
        );
        assert_eq!(
            edit_cursor_hold_dir_for_action_in_mode(
                PracticeNavMode::DedicatedThreeKey,
                VirtualAction::p1_menu_right,
            ),
            Some(CursorHoldDir::Down)
        );
        assert_eq!(
            menu_step_delta_for_action_in_mode(
                PracticeNavMode::DedicatedThreeKey,
                VirtualAction::p1_menu_left,
            ),
            Some(-1)
        );
        assert_eq!(
            menu_step_delta_for_action_in_mode(
                PracticeNavMode::DedicatedThreeKey,
                VirtualAction::p1_up,
            ),
            None
        );
    }

    #[test]
    fn gameplay_hotkey_input_maps_gameplay_function_keys() {
        assert_eq!(
            gameplay_hotkey_input(KeyCode::F6),
            Some(GameplayRawKeyInput::Autosync)
        );
        assert_eq!(
            gameplay_hotkey_input(KeyCode::F7),
            Some(GameplayRawKeyInput::TimingTick)
        );
        assert_eq!(
            gameplay_hotkey_input(KeyCode::F8),
            Some(GameplayRawKeyInput::Autoplay)
        );
        assert_eq!(
            gameplay_hotkey_input(KeyCode::F11),
            Some(GameplayRawKeyInput::OffsetAdjust(
                GameplayOffsetAdjustKey::Decrease
            ))
        );
        assert_eq!(
            gameplay_hotkey_input(KeyCode::F12),
            Some(GameplayRawKeyInput::OffsetAdjust(
                GameplayOffsetAdjustKey::Increase
            ))
        );
        // F1 opens the practice help menu and must not be treated as a gameplay
        // hotkey; unrelated keys are ignored too.
        assert_eq!(gameplay_hotkey_input(KeyCode::F1), None);
        assert_eq!(gameplay_hotkey_input(KeyCode::Space), None);
    }

    #[test]
    fn music_rate_hotkey_increment_is_quantized_and_clamped() {
        assert!((quantized_music_rate(1.0, MUSIC_RATE_HOTKEY_STEP) - 1.01).abs() < 1e-5);
        assert!((quantized_music_rate(1.0, -MUSIC_RATE_HOTKEY_STEP) - 0.99).abs() < 1e-5);
        assert!((quantized_music_rate(0.93, -MUSIC_RATE_HOTKEY_STEP) - 0.92).abs() < 1e-5);
        assert!((quantized_music_rate(0.93, MUSIC_RATE_HOTKEY_STEP) - 0.94).abs() < 1e-5);
        assert!(
            (quantized_music_rate(MUSIC_RATE_HOTKEY_MAX, MUSIC_RATE_HOTKEY_STEP)
                - MUSIC_RATE_HOTKEY_MAX)
                .abs()
                < 1e-5
        );
        assert!(
            (quantized_music_rate(MUSIC_RATE_HOTKEY_MIN, -MUSIC_RATE_HOTKEY_STEP)
                - MUSIC_RATE_HOTKEY_MIN)
                .abs()
                < 1e-5
        );
    }

    #[test]
    fn pending_practice_sfx_keep_order_before_navigation() {
        let mut pending = vec!["assets/sounds/change.ogg", "assets/sounds/start.ogg"];
        let effect =
            prepend_pending_sfx(&mut pending, ThemeEffect::Navigate(Screen::PlayerOptions));

        let ThemeEffect::Batch(effects) = effect else {
            panic!("multiple Practice effects should be batched");
        };
        let [first, second, ThemeEffect::Navigate(Screen::PlayerOptions)] = effects.as_slice()
        else {
            panic!("Practice SFX should precede navigation");
        };
        for (effect, expected) in [
            (first, "assets/sounds/change.ogg"),
            (second, "assets/sounds/start.ogg"),
        ] {
            let ThemeEffect::Runtime(SimplyLoveRuntimeRequest::Audio(AudioRequest::PlaySfx(path))) =
                effect
            else {
                panic!("queued Practice sound should become an audio request");
            };
            assert_eq!(path, expected);
        }
        assert!(pending.is_empty());
    }

    #[test]
    fn practice_rate_persistence_precedes_sound_and_navigation() {
        let mut profiles = vec![crate::SimplyLoveProfileRequest::SetMusicRate(1.25)];
        let mut sounds = vec!["assets/sounds/change.ogg"];
        let ThemeEffect::Batch(effects) = prepend_pending_effects(
            &mut profiles,
            &mut sounds,
            ThemeEffect::Navigate(Screen::PlayerOptions),
        ) else {
            panic!("Practice rate change should preserve ordered effects");
        };

        assert!(matches!(
            &effects[0],
            ThemeEffect::Runtime(SimplyLoveRuntimeRequest::Profile(
                crate::SimplyLoveProfileRequest::SetMusicRate(rate)
            )) if (*rate - 1.25).abs() < f32::EPSILON
        ));
        assert!(matches!(
            &effects[1],
            ThemeEffect::Runtime(SimplyLoveRuntimeRequest::Audio(AudioRequest::PlaySfx(path)))
                if path == "assets/sounds/change.ogg"
        ));
        assert!(matches!(
            effects[2],
            ThemeEffect::Navigate(Screen::PlayerOptions)
        ));
        assert!(profiles.is_empty());
        assert!(sounds.is_empty());
    }

    #[test]
    fn display_scroll_settles_and_snaps() {
        // Tiny gaps settle exactly on the cursor.
        assert_eq!(next_display_beat(10.0, 10.0 + 0.0005, 0.016), 10.0 + 0.0005);
        // Huge jumps snap instantly so Home/End stay responsive.
        let far = 10.0 + DISPLAY_SCROLL_MAX_SMOOTH_BEATS + 1.0;
        assert_eq!(next_display_beat(10.0, far, 0.016), far);
    }

    #[test]
    fn display_scroll_eases_toward_cursor() {
        let start = 0.0;
        let target = 4.0;
        let step = next_display_beat(start, target, 0.016);
        // A single frame moves partway, never overshoots, and heads the right way.
        assert!(step > start && step < target);
        // Repeated stepping converges onto the cursor.
        let mut beat = start;
        for _ in 0..240 {
            beat = next_display_beat(beat, target, 0.016);
        }
        assert!((beat - target).abs() <= DISPLAY_SCROLL_SNAP_EPSILON);
    }

    #[test]
    fn music_rate_brackets_map_to_hold_deltas() {
        assert_eq!(
            music_rate_hold_dir_for_key(KeyCode::BracketLeft),
            Some(MusicRateHoldDir::Lower)
        );
        assert_eq!(
            music_rate_hold_dir_for_key(KeyCode::BracketRight),
            Some(MusicRateHoldDir::Raise)
        );
        assert_eq!(music_rate_hold_dir_for_key(KeyCode::KeyP), None);
        assert_eq!(
            music_rate_delta_for_dir(MusicRateHoldDir::Lower),
            -MUSIC_RATE_HOTKEY_STEP
        );
        assert_eq!(
            music_rate_delta_for_dir(MusicRateHoldDir::Raise),
            MUSIC_RATE_HOTKEY_STEP
        );
    }

    #[test]
    fn page_keys_map_to_measure_hold_dirs() {
        assert_eq!(
            page_hold_dir_for_key(KeyCode::PageUp),
            Some(PageHoldDir::Up)
        );
        assert_eq!(
            page_hold_dir_for_key(KeyCode::Semicolon),
            Some(PageHoldDir::Up)
        );
        assert_eq!(
            page_hold_dir_for_key(KeyCode::PageDown),
            Some(PageHoldDir::Down)
        );
        assert_eq!(
            page_hold_dir_for_key(KeyCode::Quote),
            Some(PageHoldDir::Down)
        );
        assert_eq!(page_hold_dir_for_key(KeyCode::Home), None);
    }

    #[test]
    fn tab_accelerates_practice_edit_scroll_holds_when_enabled() {
        assert_eq!(edit_scroll_hold_rate(false, true), 1.0);
        assert_eq!(edit_scroll_hold_rate(true, false), 1.0);
        assert_eq!(edit_scroll_hold_rate(true, true), TAB_FAST_MULTIPLIER);
    }

    #[test]
    fn fmt_music_rate_matches_player_options_format() {
        assert_eq!(fmt_music_rate(1.0), "1");
        assert_eq!(fmt_music_rate(1.5), "1.5");
        assert_eq!(fmt_music_rate(0.85), "0.85");
        assert_eq!(fmt_music_rate(2.05), "2.05");
        assert_eq!(fmt_music_rate(0.5), "0.5");
    }

    #[test]
    fn timing_label_numbers_match_itg_to_string_shape() {
        assert_eq!(fmt_itg_float(120.0), "120.000000");
        assert_eq!(fmt_itg_float(0.5), "0.500000");
    }

    #[test]
    fn timing_speed_label_matches_itg_multiline_value() {
        let beats = SpeedSegment {
            beat: 16.0,
            ratio: 2.0,
            delay: 0.5,
            unit: SpeedUnit::Beats,
        };
        let seconds = SpeedSegment {
            unit: SpeedUnit::Seconds,
            ..beats
        };
        assert_eq!(timing_speed_label(beats), "2.000000\nB\n0.500000");
        assert_eq!(timing_speed_label(seconds), "2.000000\nS\n0.500000");
    }

    #[test]
    fn timing_label_x_uses_inherited_side_offsets() {
        assert_eq!(timing_label_x(400.0, 160.0, 0.5, BPM_LABEL_STYLE), 290.0);
        assert_eq!(timing_label_x(400.0, 160.0, 0.5, SPEED_LABEL_STYLE), 495.0);
    }

    #[test]
    fn timing_label_glow_uses_six_second_cycle() {
        assert_eq!(timing_label_glow_alpha(0.0), 1.0);
        assert!((timing_label_glow_alpha(3.0) - 0.0).abs() < 0.000_001);
        assert!((timing_label_glow_alpha(6.0) - 1.0).abs() < 0.000_001);
    }

    #[test]
    fn practice_edit_travel_uses_step_editor_beat_spacing() {
        let travel =
            practice_edit_beat_travel(44.0, 40.0, 0.5, ScrollSpeedSetting::XMod(1.5), 180.0, 1.0);

        assert!((travel - 4.0 * 64.0 * 0.5 * 1.5).abs() <= 0.001);
    }

    #[test]
    fn practice_selection_restore_preserves_valid_range() {
        assert_eq!(
            clamp_selection(Some(64.0), Some(32.0), 128.0),
            (Some(32.0), Some(64.0))
        );
    }

    #[test]
    fn practice_selection_restore_drops_collapsed_range() {
        assert_eq!(clamp_selection(Some(96.0), Some(128.0), 64.0), (None, None));
    }

    #[test]
    fn help_menu_item_keys_resolve_through_i18n() {
        i18n::init_for_tests();
        assert_menu_labels_localized(&HELP_MENU);
    }

    #[test]
    fn main_menu_item_keys_resolve_through_i18n() {
        i18n::init_for_tests();
        assert_menu_labels_localized(&MAIN_MENU);
    }

    #[test]
    fn help_menu_rows_have_no_actions_main_menu_rows_all_have_actions() {
        assert!(
            HELP_MENU.rows.iter().all(|r| r.action.is_none()),
            "help rows are display-only"
        );
        assert!(
            MAIN_MENU.rows.iter().all(|r| r.action.is_some()),
            "every main row must dispatch an action"
        );
    }

    #[test]
    fn all_practice_runtime_keys_resolve_through_i18n() {
        i18n::init_for_tests();
        for key in PRACTICE_RUNTIME_KEYS {
            let resolved = i18n::tr("Practice", key);
            assert_ne!(
                resolved.as_ref(),
                format!("Practice.{key}").as_str(),
                "missing i18n entry for Practice.{key}"
            );
        }
    }

    #[test]
    fn placeholder_keys_substitute_named_args() {
        i18n::init_for_tests();
        let rate = i18n::tr_fmt("Practice", "FlashMusicRate", &[("rate", "1.5")]);
        assert!(
            rate.contains("1.5") && !rate.contains("{rate}"),
            "FlashMusicRate did not substitute placeholder: {rate}"
        );
        let beat = i18n::tr_fmt("Practice", "InfoCurrentBeat", &[("beat", "3.000")]);
        assert!(
            beat.contains("3.000") && !beat.contains("{beat}"),
            "InfoCurrentBeat did not substitute placeholder: {beat}"
        );
    }

    fn assert_menu_labels_localized(def: &MenuDef) {
        for row in def.rows {
            let resolved = row.label.get();
            let fallback = format!("{}.{}", row.label.section, row.label.key);
            assert_ne!(
                resolved.as_ref(),
                fallback.as_str(),
                "missing i18n entry for {fallback}"
            );
        }
    }
}
