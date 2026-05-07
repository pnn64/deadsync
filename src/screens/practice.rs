use crate::act;
use crate::assets::{AssetManager, FontRole, current_machine_font_key};
use crate::engine::audio;
use crate::engine::input::{InputEvent, RawKeyboardEvent, VirtualAction};
use crate::engine::present::actors::Actor;
use crate::engine::present::color;
use crate::engine::space::{
    screen_center_x, screen_center_y, screen_height, screen_width, widescale,
};
use crate::game::gameplay::{self as gameplay_core, effective_spacing_multiplier_for_player};
use crate::game::{profile, scroll::ScrollSpeedSetting};
use crate::screens::gameplay as gameplay_screen;
use crate::screens::{Screen, ScreenAction};
use winit::keyboard::KeyCode;

const LEAD_IN_SECONDS: f32 = 1.0;
const LOOP_AFTER_SECONDS: f32 = 1.0;
const BEATS_PER_MEASURE: f32 = 4.0;
const MIN_CURSOR_BEAT: f32 = 0.0;
const BEAT_EPSILON: f32 = 0.000_1;
const MARKER_Z: f32 = 2985.0;
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
const EDIT_INFO_VALUE_CHARS: usize = 28;
const EDIT_LINE_SOUND: &str = "assets/sounds/change.ogg";
const EDIT_MARKER_SOUND: &str = "assets/sounds/screen_edit_marker.ogg";
const EDIT_SNAP_SOUND: &str = "assets/sounds/screen_edit_snap.ogg";
const EDIT_INVALID_SOUND: &str = "assets/sounds/common_invalid.ogg";
const EDIT_SCROLL_SPEEDS: [f32; 7] = [1.0, 1.5, 2.0, 3.0, 4.0, 6.0, 8.0];

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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CursorHoldDir {
    Up,
    Down,
}

pub struct State {
    pub(crate) gameplay: gameplay_screen::State,
    mode: Mode,
    menu: Option<MenuState>,
    cursor_beat: f32,
    selection_anchor: Option<f32>,
    selection_end: Option<f32>,
    shift_anchor: Option<f32>,
    snap_index: usize,
    edit_scroll_speed_index: usize,
    shift_held: bool,
    ctrl_held: bool,
    cursor_hold_dir: Option<CursorHoldDir>,
    cursor_hold_up_count: u8,
    cursor_hold_down_count: u8,
    cursor_hold_delay_left: f32,
    cursor_hold_repeat_left: f32,
    flash: Option<(&'static str, f32)>,
}

#[derive(Clone, Copy, Debug)]
struct MenuState {
    kind: MenuKind,
    selected: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MenuKind {
    Main,
    Help,
}

#[derive(Clone, Copy, Debug)]
enum MenuItem {
    PlayWholeSong,
    PlayCurrentToEnd,
    PlaySelection,
    SetSelectionStart,
    SetSelectionEnd,
    EditorOptions,
    Exit,
}

const MAIN_MENU_ITEMS: [MenuItem; 7] = [
    MenuItem::PlayWholeSong,
    MenuItem::PlayCurrentToEnd,
    MenuItem::PlaySelection,
    MenuItem::SetSelectionStart,
    MenuItem::SetSelectionEnd,
    MenuItem::EditorOptions,
    MenuItem::Exit,
];

const HELP_MENU_ITEMS: [&str; 9] = [
    "Hold Up/Down: Move by current snap",
    "Semicolon/Apostrophe: Move one measure",
    "Ctrl+Up/Down: Change zoom",
    "Home/End: Move to start/end of steps",
    "Left/Right: Select note snap",
    "Space: Set area marker",
    "Shift+Navigate: Select area",
    "P: Play selection",
    "Esc/Enter/Start/Back: Main Menu",
];

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

pub fn init(mut gameplay: gameplay_screen::State) -> State {
    gameplay_core::disable_score_for_practice(&mut gameplay);
    let mut state = State {
        gameplay,
        mode: Mode::Editing,
        menu: None,
        cursor_beat: 0.0,
        selection_anchor: None,
        selection_end: None,
        shift_anchor: None,
        snap_index: 0,
        edit_scroll_speed_index: 0,
        shift_held: false,
        ctrl_held: false,
        cursor_hold_dir: None,
        cursor_hold_up_count: 0,
        cursor_hold_down_count: 0,
        cursor_hold_delay_left: 0.0,
        cursor_hold_repeat_left: EDIT_CURSOR_REPEAT_INTERVAL_SECONDS,
        flash: None,
    };
    set_cursor(&mut state, MIN_CURSOR_BEAT);
    state
}

pub fn on_enter(state: &mut State) {
    audio::stop_music();
    set_cursor(state, state.cursor_beat);
}

pub fn update(state: &mut State, delta_time: f32) -> ScreenAction {
    if let Some((_, remaining)) = state.flash.as_mut() {
        *remaining -= delta_time;
        if *remaining <= 0.0 {
            state.flash = None;
        }
    }

    let Mode::Playing {
        start_beat,
        stop_beat,
    } = state.mode
    else {
        state.gameplay.total_elapsed_in_screen += delta_time;
        update_cursor_hold(state, delta_time);
        return ScreenAction::None;
    };

    let action = gameplay_core::update(&mut state.gameplay, delta_time);
    let current_time = gameplay_core::current_music_time_seconds(&state.gameplay);
    let stop_time = gameplay_core::music_time_for_beat(&state.gameplay, stop_beat);
    if current_time >= stop_time + LOOP_AFTER_SECONDS
        || !matches!(action, gameplay_core::GameplayAction::None)
    {
        start_playback(state, start_beat, stop_beat);
    }
    ScreenAction::None
}

pub fn handle_input(state: &mut State, ev: &InputEvent) -> ScreenAction {
    if state.menu.is_some() {
        return handle_menu_input(state, ev);
    }

    if !ev.pressed {
        if matches!(state.mode, Mode::Editing)
            && let Some(dir) = cursor_hold_dir_for_action(ev.action)
        {
            release_cursor_hold_input(state, dir);
            return ScreenAction::None;
        }
        if matches!(state.mode, Mode::Playing { .. }) && ev.action.is_gameplay_arrow() {
            let _ = gameplay_core::handle_input(&mut state.gameplay, ev);
        }
        return ScreenAction::None;
    }

    match state.mode {
        Mode::Playing { .. } => match ev.action {
            VirtualAction::p1_back | VirtualAction::p2_back => {
                stop_playback(state);
                ScreenAction::None
            }
            _ => {
                let _ = gameplay_core::handle_input(&mut state.gameplay, ev);
                ScreenAction::None
            }
        },
        Mode::Editing => match ev.action {
            VirtualAction::p1_up
            | VirtualAction::p2_up
            | VirtualAction::p1_menu_up
            | VirtualAction::p2_menu_up => {
                press_cursor_hold_input(state, CursorHoldDir::Up);
                move_cursor_by_hold_dir(state, CursorHoldDir::Up);
                ScreenAction::None
            }
            VirtualAction::p1_down
            | VirtualAction::p2_down
            | VirtualAction::p1_menu_down
            | VirtualAction::p2_menu_down => {
                press_cursor_hold_input(state, CursorHoldDir::Down);
                move_cursor_by_hold_dir(state, CursorHoldDir::Down);
                ScreenAction::None
            }
            VirtualAction::p1_left | VirtualAction::p2_left | VirtualAction::p1_menu_left => {
                change_snap(state, -1);
                ScreenAction::None
            }
            VirtualAction::p1_right | VirtualAction::p2_right | VirtualAction::p1_menu_right => {
                change_snap(state, 1);
                ScreenAction::None
            }
            VirtualAction::p1_start | VirtualAction::p2_start => {
                open_main_menu(state);
                ScreenAction::None
            }
            VirtualAction::p1_back | VirtualAction::p2_back => {
                open_main_menu(state);
                ScreenAction::None
            }
            VirtualAction::p1_select | VirtualAction::p2_select => {
                set_area_marker(state);
                ScreenAction::None
            }
            _ => ScreenAction::None,
        },
    }
}

pub fn handle_raw_key_event(state: &mut State, raw_key: &RawKeyboardEvent) -> (bool, ScreenAction) {
    match raw_key.code {
        KeyCode::ShiftLeft | KeyCode::ShiftRight => {
            state.shift_held = raw_key.pressed;
            if !raw_key.pressed {
                state.shift_anchor = None;
            }
            return (true, ScreenAction::None);
        }
        KeyCode::ControlLeft | KeyCode::ControlRight => {
            state.ctrl_held = raw_key.pressed;
            return (true, ScreenAction::None);
        }
        _ => {}
    }

    if !raw_key.pressed {
        return (false, ScreenAction::None);
    }

    if matches!(state.mode, Mode::Playing { .. }) {
        return match raw_key.code {
            KeyCode::Escape | KeyCode::Enter => {
                stop_playback(state);
                (true, ScreenAction::None)
            }
            _ => (false, ScreenAction::None),
        };
    }

    if state.menu.is_some() {
        return match raw_key.code {
            KeyCode::Escape => {
                close_menu(state);
                (true, ScreenAction::None)
            }
            KeyCode::Enter => (true, activate_menu_item(state)),
            _ => (false, ScreenAction::None),
        };
    }

    match raw_key.code {
        KeyCode::Escape | KeyCode::Enter => {
            open_main_menu(state);
            (true, ScreenAction::None)
        }
        KeyCode::ArrowUp if state.ctrl_held => {
            change_edit_scroll_speed(state, 1);
            (true, ScreenAction::None)
        }
        KeyCode::ArrowDown if state.ctrl_held => {
            change_edit_scroll_speed(state, -1);
            (true, ScreenAction::None)
        }
        KeyCode::KeyP if state.ctrl_held => {
            start_playback(state, MIN_CURSOR_BEAT, max_play_beat(state));
            (true, ScreenAction::None)
        }
        KeyCode::KeyP if state.shift_held => {
            start_playback(state, state.cursor_beat, max_play_beat(state));
            (true, ScreenAction::None)
        }
        KeyCode::KeyP => {
            start_selection_like_itg(state);
            (true, ScreenAction::None)
        }
        KeyCode::Space => {
            set_area_marker(state);
            (true, ScreenAction::None)
        }
        KeyCode::Semicolon => {
            move_cursor_from_button(state, -BEATS_PER_MEASURE);
            (true, ScreenAction::None)
        }
        KeyCode::PageUp => {
            move_cursor_from_button(state, -BEATS_PER_MEASURE);
            (true, ScreenAction::None)
        }
        KeyCode::Quote => {
            move_cursor_from_button(state, BEATS_PER_MEASURE);
            (true, ScreenAction::None)
        }
        KeyCode::PageDown => {
            move_cursor_from_button(state, BEATS_PER_MEASURE);
            (true, ScreenAction::None)
        }
        KeyCode::Comma if !state.ctrl_held => {
            seek_chart_note(state, -1);
            (true, ScreenAction::None)
        }
        KeyCode::Period if !state.ctrl_held => {
            seek_chart_note(state, 1);
            (true, ScreenAction::None)
        }
        KeyCode::Home => {
            set_cursor(state, MIN_CURSOR_BEAT);
            audio::play_sfx(EDIT_LINE_SOUND);
            (true, ScreenAction::None)
        }
        KeyCode::End => {
            set_cursor(state, max_play_beat(state));
            audio::play_sfx(EDIT_LINE_SOUND);
            (true, ScreenAction::None)
        }
        KeyCode::F1 => {
            open_help_menu(state);
            (true, ScreenAction::None)
        }
        _ => (false, ScreenAction::None),
    }
}

pub fn get_actors(state: &mut State, asset_manager: &AssetManager) -> Vec<Actor> {
    let mut actors = Vec::with_capacity(128);
    let view = practice_view(state);
    gameplay_screen::push_actors(&mut actors, &mut state.gameplay, asset_manager, view);
    if matches!(state.mode, Mode::Editing) {
        append_edit_markers(state, &mut actors);
        append_edit_overlay(state, &mut actors);
    }
    if state.menu.is_some() {
        append_main_menu(state, &mut actors);
    }
    actors
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
    }
}

fn practice_notefield_view(state: &State) -> gameplay_screen::NotefieldViewOverride {
    gameplay_screen::NotefieldViewOverride {
        field_zoom: Some(practice_edit_field_zoom()),
        scroll_speed: Some(practice_edit_scroll_speed(state)),
        force_center_1player: true,
        receptor_y: Some(practice_edit_cursor_y()),
        edit_beat_bars: true,
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

fn handle_menu_input(state: &mut State, ev: &InputEvent) -> ScreenAction {
    if !ev.pressed {
        return ScreenAction::None;
    }
    match ev.action {
        VirtualAction::p1_up
        | VirtualAction::p2_up
        | VirtualAction::p1_menu_up
        | VirtualAction::p2_menu_up => {
            step_menu(state, -1);
            ScreenAction::None
        }
        VirtualAction::p1_down
        | VirtualAction::p2_down
        | VirtualAction::p1_menu_down
        | VirtualAction::p2_menu_down => {
            step_menu(state, 1);
            ScreenAction::None
        }
        VirtualAction::p1_start
        | VirtualAction::p2_start
        | VirtualAction::p1_select
        | VirtualAction::p2_select => activate_menu_item(state),
        VirtualAction::p1_back | VirtualAction::p2_back => {
            close_menu(state);
            ScreenAction::None
        }
        _ => ScreenAction::None,
    }
}

fn open_main_menu(state: &mut State) {
    clear_cursor_hold_inputs(state);
    if state.menu.is_none() {
        audio::play_sfx("assets/sounds/start.ogg");
    }
    state.menu = Some(MenuState {
        kind: MenuKind::Main,
        selected: first_enabled_menu_row(MenuKind::Main),
    });
}

fn open_help_menu(state: &mut State) {
    clear_cursor_hold_inputs(state);
    if state.menu.is_none() {
        audio::play_sfx("assets/sounds/start.ogg");
    }
    state.menu = Some(MenuState {
        kind: MenuKind::Help,
        selected: 0,
    });
}

fn close_menu(state: &mut State) {
    state.menu = None;
    audio::play_sfx("assets/sounds/start.ogg");
}

fn step_menu(state: &mut State, delta: isize) {
    let Some(menu) = state.menu else {
        return;
    };
    let len = menu_len(menu.kind) as isize;
    let mut selected = menu.selected;
    for _ in 0..len {
        selected = (selected as isize + delta).rem_euclid(len) as usize;
        if menu_row_enabled(menu.kind, selected) {
            break;
        }
    }
    state.menu = Some(MenuState {
        kind: menu.kind,
        selected,
    });
    audio::play_sfx("assets/sounds/change.ogg");
}

fn activate_menu_item(state: &mut State) -> ScreenAction {
    let Some(menu) = state.menu else {
        return ScreenAction::None;
    };
    if menu.kind == MenuKind::Help {
        close_menu(state);
        return ScreenAction::None;
    }
    if !menu_row_enabled(menu.kind, menu.selected) {
        audio::play_sfx("assets/sounds/change.ogg");
        return ScreenAction::None;
    }
    let item = MAIN_MENU_ITEMS[menu.selected.min(MAIN_MENU_ITEMS.len() - 1)];
    state.menu = None;
    clear_cursor_hold_inputs(state);
    audio::play_sfx("assets/sounds/start.ogg");
    match item {
        MenuItem::PlayWholeSong => {
            start_playback(state, MIN_CURSOR_BEAT, max_play_beat(state));
            ScreenAction::None
        }
        MenuItem::PlayCurrentToEnd => {
            start_playback(state, state.cursor_beat, max_play_beat(state));
            ScreenAction::None
        }
        MenuItem::PlaySelection => {
            start_selection_like_itg(state);
            ScreenAction::None
        }
        MenuItem::SetSelectionStart => {
            set_selection_start(state);
            ScreenAction::None
        }
        MenuItem::SetSelectionEnd => {
            set_selection_end(state);
            ScreenAction::None
        }
        MenuItem::EditorOptions => ScreenAction::Navigate(Screen::PlayerOptions),
        MenuItem::Exit => ScreenAction::Navigate(Screen::SelectMusic),
    }
}

fn menu_len(kind: MenuKind) -> usize {
    match kind {
        MenuKind::Main => MAIN_MENU_ITEMS.len(),
        MenuKind::Help => HELP_MENU_ITEMS.len(),
    }
}

fn first_enabled_menu_row(kind: MenuKind) -> usize {
    (0..menu_len(kind))
        .find(|&idx| menu_row_enabled(kind, idx))
        .unwrap_or(0)
}

fn menu_row_enabled(kind: MenuKind, idx: usize) -> bool {
    match kind {
        MenuKind::Main => MAIN_MENU_ITEMS
            .get(idx)
            .is_some_and(|item| menu_item_enabled(*item)),
        MenuKind::Help => true,
    }
}

const fn menu_item_enabled(_item: MenuItem) -> bool {
    true
}

fn start_selection_like_itg(state: &mut State) {
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
    );
}

fn start_playback(state: &mut State, start_beat: f32, stop_beat: f32) {
    clear_cursor_hold_inputs(state);
    let start_time = gameplay_core::music_time_for_beat(&state.gameplay, start_beat);
    gameplay_core::start_practice_music(
        &mut state.gameplay,
        start_time - LEAD_IN_SECONDS,
        start_time,
    );
    state.mode = Mode::Playing {
        start_beat,
        stop_beat,
    };
    state.flash = None;
}

fn stop_playback(state: &mut State) {
    clear_cursor_hold_inputs(state);
    audio::stop_music();
    let current_beat = state.gameplay.current_beat.max(MIN_CURSOR_BEAT);
    state.mode = Mode::Editing;
    set_cursor(state, current_beat);
}

const fn cursor_hold_dir_for_action(action: VirtualAction) -> Option<CursorHoldDir> {
    match action {
        VirtualAction::p1_up
        | VirtualAction::p2_up
        | VirtualAction::p1_menu_up
        | VirtualAction::p2_menu_up => Some(CursorHoldDir::Up),
        VirtualAction::p1_down
        | VirtualAction::p2_down
        | VirtualAction::p1_menu_down
        | VirtualAction::p2_menu_down => Some(CursorHoldDir::Down),
        _ => None,
    }
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

    let mut remaining = delta_time;
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

fn move_cursor_by_hold_dir(state: &mut State, dir: CursorHoldDir) {
    let snap = SNAP_BEATS[state.snap_index];
    match dir {
        CursorHoldDir::Up => move_cursor_from_button(state, -snap),
        CursorHoldDir::Down => move_cursor_from_button(state, snap),
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
        audio::play_sfx(EDIT_LINE_SOUND);
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
    let music_time = gameplay_core::music_time_for_beat(&state.gameplay, state.cursor_beat);
    gameplay_core::seek_practice_display(&mut state.gameplay, music_time);
}

fn change_snap(state: &mut State, delta: isize) {
    let next = state.snap_index as isize + delta;
    if next < 0 || next >= SNAP_LABELS.len() as isize {
        return;
    }
    state.snap_index = next as usize;
    let quantized = quantize_beat(state.cursor_beat, SNAP_BEATS[state.snap_index]);
    set_cursor(state, quantized);
    audio::play_sfx(EDIT_SNAP_SOUND);
}

fn change_edit_scroll_speed(state: &mut State, delta: isize) {
    let last = EDIT_SCROLL_SPEEDS.len() as isize - 1;
    let next = (state.edit_scroll_speed_index as isize + delta).clamp(0, last) as usize;
    if next == state.edit_scroll_speed_index {
        return;
    }
    state.edit_scroll_speed_index = next;
    state.flash = Some(("Zoom changed", 0.75));
    audio::play_sfx(EDIT_MARKER_SOUND);
}

fn seek_chart_note(state: &mut State, dir: i32) {
    let current = state.cursor_beat;
    let target = if dir < 0 {
        state
            .gameplay
            .notes
            .iter()
            .filter(|note| note.can_be_judged && note.beat < current - BEAT_EPSILON)
            .map(|note| note.beat)
            .max_by(|a, b| a.total_cmp(b))
    } else {
        state
            .gameplay
            .notes
            .iter()
            .filter(|note| note.can_be_judged && note.beat > current + BEAT_EPSILON)
            .map(|note| note.beat)
            .min_by(|a, b| a.total_cmp(b))
    };
    if let Some(beat) = target {
        set_cursor(state, beat);
        audio::play_sfx(EDIT_LINE_SOUND);
    } else {
        audio::play_sfx(EDIT_INVALID_SOUND);
    }
}

fn set_area_marker(state: &mut State) {
    audio::play_sfx(EDIT_MARKER_SOUND);
    match (state.selection_anchor, state.selection_end) {
        (None, None) => {
            state.selection_anchor = Some(state.cursor_beat);
            state.flash = Some(("Area marker start set", 0.75));
        }
        (Some(begin), None) => {
            if same_beat(begin, state.cursor_beat) {
                clear_selection(state);
            } else {
                set_marker_range(state, begin, state.cursor_beat);
                state.flash = Some(("Area marker end set", 0.75));
            }
        }
        _ => {
            state.selection_anchor = Some(state.cursor_beat);
            state.selection_end = None;
            state.shift_anchor = None;
            state.flash = Some(("Area marker start set", 0.75));
        }
    }
}

fn set_selection_start(state: &mut State) {
    if state
        .selection_end
        .is_some_and(|end| state.cursor_beat >= end)
    {
        state.flash = Some(("Invalid selection start", 0.75));
        audio::play_sfx(EDIT_INVALID_SOUND);
        return;
    }
    state.selection_anchor = Some(state.cursor_beat);
    state.flash = Some(("Selection start set", 0.75));
    audio::play_sfx(EDIT_MARKER_SOUND);
}

fn set_selection_end(state: &mut State) {
    if state
        .selection_anchor
        .is_some_and(|start| state.cursor_beat <= start)
    {
        state.flash = Some(("Invalid selection end", 0.75));
        audio::play_sfx(EDIT_INVALID_SOUND);
        return;
    }
    state.selection_end = Some(state.cursor_beat);
    state.flash = Some(("Selection end set", 0.75));
    audio::play_sfx(EDIT_MARKER_SOUND);
}

fn clear_selection(state: &mut State) {
    state.selection_anchor = None;
    state.selection_end = None;
    state.shift_anchor = None;
    state.flash = Some(("Selection cleared", 0.75));
}

fn set_marker_range(state: &mut State, a: f32, b: f32) {
    state.selection_anchor = Some(a.min(b));
    state.selection_end = Some(a.max(b));
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
    state
        .gameplay
        .player_profiles
        .first()
        .is_some_and(|p| p.scroll_option.contains(profile::ScrollOption::Reverse))
}

fn max_play_beat(state: &State) -> f32 {
    let note_beat = state
        .gameplay
        .notes
        .iter()
        .map(|note| note.hold.as_ref().map_or(note.beat, |hold| hold.end_beat))
        .fold(MIN_CURSOR_BEAT, f32::max);
    let song_beat = gameplay_core::beat_for_music_time(
        &state.gameplay,
        state.gameplay.song.music_length_seconds.max(0.0),
    );
    note_beat.max(song_beat).max(MIN_CURSOR_BEAT)
}

fn append_edit_markers(state: &State, actors: &mut Vec<Actor>) {
    let hud = profile::gameplay_hud_snapshot();
    let play_style = hud.play_style;
    let is_p2_single =
        play_style == profile::PlayStyle::Single && hud.player_side == profile::PlayerSide::P2;

    match play_style {
        profile::PlayStyle::Versus => {
            append_player_markers(state, actors, 0, MarkerPlacement::P1, play_style, false);
            append_player_markers(state, actors, 1, MarkerPlacement::P2, play_style, false);
        }
        profile::PlayStyle::Single | profile::PlayStyle::Double => {
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
    play_style: profile::PlayStyle,
    center_1player_notefield: bool,
) {
    if player_idx >= state.gameplay.num_players {
        return;
    }

    let col_start = player_idx * state.gameplay.cols_per_player;
    let col_end = (col_start + state.gameplay.cols_per_player)
        .min(state.gameplay.num_cols)
        .min(state.gameplay.column_scroll_dirs.len());
    let num_cols = col_end.saturating_sub(col_start);
    if num_cols == 0 {
        return;
    }

    let profile = &state.gameplay.player_profiles[player_idx];
    let offset_sign = match placement {
        MarkerPlacement::P1 => -1.0,
        MarkerPlacement::P2 => 1.0,
    };
    let offset_x = offset_sign * profile.note_field_offset_x.clamp(0, 50) as f32;
    let offset_y = profile.note_field_offset_y.clamp(-50, 50) as f32;
    let clamped_width = screen_width().clamp(640.0, 854.0);
    let centered_one_side = state.gameplay.num_players == 1
        && play_style == profile::PlayStyle::Single
        && center_1player_notefield;
    let centered_both_sides =
        state.gameplay.num_players == 1 && play_style == profile::PlayStyle::Double;
    let base_x = if state.gameplay.num_players == 2 {
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
    let spacing_mult = effective_spacing_multiplier_for_player(&state.gameplay, player_idx);
    let field_zoom = practice_edit_field_zoom();
    let width = (num_cols as f32 * ScrollSpeedSetting::ARROW_SPACING * spacing_mult * field_zoom)
        .max(ScrollSpeedSetting::ARROW_SPACING);
    let marker_phase = (state.gameplay.total_elapsed_in_screen * std::f32::consts::PI).sin();
    let marker_shade = 0.75 + marker_phase * 0.25;
    let cursor_y = marker_y_for_beat(state, player_idx, col_start, offset_y, state.cursor_beat);
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

fn marker_y_for_beat(
    state: &State,
    player_idx: usize,
    col_start: usize,
    offset_y: f32,
    beat: f32,
) -> f32 {
    let dir = state.gameplay.column_scroll_dirs[col_start].signum();
    let dir = if dir.abs() <= f32::EPSILON { 1.0 } else { dir };
    let receptor_y = practice_edit_cursor_y() + offset_y;
    let field_zoom = practice_edit_field_zoom();
    let timing = &state.gameplay.timing_players[player_idx];
    let current_time_ns = state.gameplay.current_music_time_visible_ns[player_idx];
    let scroll_speed = practice_edit_scroll_speed(state);
    let travel = match scroll_speed {
        ScrollSpeedSetting::CMod(c_bpm) => {
            let rate = if state.gameplay.music_rate.is_finite() && state.gameplay.music_rate > 0.0 {
                state.gameplay.music_rate
            } else {
                1.0
            };
            let pps = (c_bpm / 60.0) * ScrollSpeedSetting::ARROW_SPACING * field_zoom;
            let beat_time = timing.get_time_for_beat(beat);
            let current_time = state.gameplay.current_music_time_visible[player_idx];
            (beat_time - current_time) / rate * pps
        }
        ScrollSpeedSetting::XMod(_) | ScrollSpeedSetting::MMod(_) => {
            let current_beat = state.gameplay.current_beat_visible[player_idx];
            let speed_multiplier = timing.get_speed_multiplier_ns(current_beat, current_time_ns);
            let player_multiplier = scroll_speed.beat_multiplier(
                state.gameplay.scroll_reference_bpm,
                state.gameplay.music_rate,
            );
            (timing.get_displayed_beat(beat) - timing.get_displayed_beat(current_beat))
                * ScrollSpeedSetting::ARROW_SPACING
                * field_zoom
                * speed_multiplier
                * player_multiplier
        }
    };
    receptor_y + dir * travel
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
    actors.push(act!(text:
        font(current_machine_font_key(FontRole::Header)):
        settext("PRACTICE MODE"):
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
        "Navigating",
        "Up/Down:\n     Move One Beat\nSemicolon/Apostrophe:\n     Move One Measure\nCtrl+Up/Down:\n     Change Zoom\nHome/End:\n     Move to Start/End of Steps\nLeft/Right:\n     Select Note Snap",
        0.0,
        pc,
    );
    append_help_section(
        actors,
        "Available Menus",
        "Escape\\Enter:\n     Main Menu\nF1:\n     Open Help Menu",
        EDIT_HELP_MENU_Y,
        pc,
    );
    append_help_section(
        actors,
        "Misc.",
        "Space:\n     Set area marker\nShift+Navigate:\n     Select area",
        EDIT_HELP_MISC_Y,
        pc,
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
    if let Some((text, remaining)) = state.flash {
        let alpha = remaining.clamp(0.0, 1.0);
        actors.push(act!(text:
            font("miso"):
            settext(text):
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
    color::simply_love_rgba(state.gameplay.active_color_index)
}

fn edit_info_text(state: &State) -> String {
    let chart = &state.gameplay.charts[0];
    let song = &state.gameplay.song;
    let current_second = gameplay_core::music_time_for_beat(&state.gameplay, state.cursor_beat);
    let difficulty = color::difficulty_display_name_for_song(&chart.difficulty, &song.title, true);
    let mut status = format!(
        "Current beat:  {:.3}\nCurrent second:  {:.6}\nSnap to: {}\n",
        state.cursor_beat, current_second, SNAP_LABELS[state.snap_index]
    );
    if let Some(selection) = selection_info_text(state) {
        status.push_str(&selection);
        status.push('\n');
    }
    status.push_str(&format!("Difficulty:  {} {}\n\n", difficulty, chart.meter));
    push_info_line(&mut status, "Main title", &song.title);
    push_info_line(&mut status, "Subtitle", &song.subtitle);
    push_info_line(&mut status, "Description", &chart.description);
    push_info_line(&mut status, "Chart name", &chart.chart_name);
    push_info_line(&mut status, "Step author", &chart.step_artist);
    push_info_line(&mut status, "Chart style", &chart.chart_type);
    status.push('\n');
    status.push_str(&format!(
        "Num steps: {}\nNum jumps: {}\nNum hands: {}\nNum holds: {}\nNum mines: {}\nNum rolls: {}\nNum lifts: {}\nNum fakes: {}",
        chart.stats.total_steps,
        chart.stats.jumps,
        chart.stats.hands,
        state.gameplay.holds_total[0],
        state.gameplay.mines_total[0],
        state.gameplay.rolls_total[0],
        chart.stats.lifts,
        chart.stats.fakes,
    ));
    status
}

fn selection_info_text(state: &State) -> Option<String> {
    match (state.selection_anchor, state.selection_end) {
        (Some(start), Some(stop)) if stop > start => {
            Some(format!("Selection beat:  {start:.3}-{stop:.3}"))
        }
        (Some(start), None) => Some(format!("Selection beat:  {start:.3} ...")),
        (None, Some(stop)) => Some(format!("Selection beat:  ...-{stop:.3}")),
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
    let row_count = menu_len(menu.kind);
    let selected_color = practice_player_color(state);
    for idx in 0..row_count {
        append_menu_row(
            actors,
            idx,
            row_count,
            menu.selected == idx,
            menu_row_enabled(menu.kind, idx),
            menu_row_label(menu.kind, idx),
            selected_color,
        );
    }
}

const fn menu_item_label(item: MenuItem) -> &'static str {
    match item {
        MenuItem::PlayWholeSong => "Play whole song",
        MenuItem::PlayCurrentToEnd => "Play current beat to end",
        MenuItem::PlaySelection => "Play selection",
        MenuItem::SetSelectionStart => "Set selection start",
        MenuItem::SetSelectionEnd => "Set selection end",
        MenuItem::EditorOptions => "Editor options",
        MenuItem::Exit => "Exit Edit Mode",
    }
}

fn menu_row_label(kind: MenuKind, idx: usize) -> &'static str {
    match kind {
        MenuKind::Main => MAIN_MENU_ITEMS
            .get(idx)
            .map(|item| menu_item_label(*item))
            .unwrap_or(""),
        MenuKind::Help => HELP_MENU_ITEMS.get(idx).copied().unwrap_or(""),
    }
}

fn append_menu_row(
    actors: &mut Vec<Actor>,
    idx: usize,
    row_count: usize,
    selected: bool,
    enabled: bool,
    label: &'static str,
    selected_color: [f32; 4],
) {
    let y = menu_row_y(idx, row_count);
    let bg_x = screen_center_x();
    let bg_w = widescale(543.0, 720.0);
    let (bg, fg) = if !enabled {
        ([0.015, 0.015, 0.015, 0.95], [0.65, 0.0, 0.0, 1.0])
    } else if selected {
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
    if row_count % 2 == 0 {
        screen_center_y()
            + EDIT_MENU_ROW_HEIGHT * (idx as f32 - (row_count / 2) as f32)
            + EDIT_MENU_ROW_HEIGHT * 0.5
    } else {
        screen_center_y() + EDIT_MENU_ROW_HEIGHT * (idx as f32 - (row_count / 2) as f32)
    }
}

fn append_help_section(
    actors: &mut Vec<Actor>,
    label: &'static str,
    body: &'static str,
    y: f32,
    player_color: [f32; 4],
) {
    actors.push(act!(text:
        font(current_machine_font_key(FontRole::Header)):
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
