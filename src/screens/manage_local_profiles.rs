use crate::act;
use crate::assets::AssetManager;
use crate::assets::i18n::{tr, tr_fmt};
use crate::assets::visual_styles;
use crate::engine::audio;
use crate::engine::input::{InputEvent, RawKeyboardEvent, VirtualAction};
use crate::engine::present::actors::Actor;
use crate::engine::present::color;
use crate::engine::space::{screen_height, screen_width};
use crate::game::profile;
use crate::screens::components::shared::heart_bg;
use crate::screens::components::shared::screen_bar::{
    self, ScreenBarPosition, ScreenBarTitlePlacement,
};
use crate::screens::components::shared::transitions;
use crate::screens::input as screen_input;
use crate::screens::{Screen, ScreenAction};
use std::sync::Arc;
use std::time::{Duration, Instant};
use winit::keyboard::KeyCode;

/* ---------------------------- transitions ---------------------------- */
const TRANSITION_IN_DURATION: f32 = 0.4;
const TRANSITION_OUT_DURATION: f32 = 0.4;

/* -------------------------- hold-to-scroll timing ------------------------- */
const NAV_INITIAL_HOLD_DELAY: Duration = Duration::from_millis(300);
const NAV_REPEAT_SCROLL_INTERVAL: Duration = Duration::from_millis(50);

/* --------------------------------- layout -------------------------------- */
/// Bars in `screen_bar.rs` use 32.0 px height.
const BAR_H: f32 = 32.0;
/// Screen-space margins (pixels, not scaled)
const LEFT_MARGIN_PX: f32 = 33.0;
const RIGHT_MARGIN_PX: f32 = 25.0;
const FIRST_ROW_TOP_MARGIN_PX: f32 = 18.0;
const BOTTOM_MARGIN_PX: f32 = 0.0;

const VISIBLE_ROWS: usize = 10;
const ROW_H: f32 = 33.0;
const ROW_GAP: f32 = 2.5;
const LIST_W: f32 = 509.0;
const SEP_W: f32 = 2.5;
const DESC_W: f32 = 292.0;
const DESC_H: f32 = (VISIBLE_ROWS as f32) * ROW_H + ((VISIBLE_ROWS - 1) as f32) * ROW_GAP;

const HEART_LEFT_PAD: f32 = 13.0;
const TEXT_LEFT_PAD: f32 = 40.66;
const ITEM_TEXT_ZOOM: f32 = 0.88;
const HEART_ZOOM: f32 = 0.026;

const DESC_TITLE_TOP_PAD_PX: f32 = 9.75;
const DESC_TITLE_SIDE_PAD_PX: f32 = 7.5;
const DESC_BULLET_TOP_PAD_PX: f32 = 23.25;
const DESC_BULLET_SIDE_PAD_PX: f32 = 7.5;
const DESC_BULLET_INDENT_PX: f32 = 10.0;
const DESC_TITLE_ZOOM: f32 = 1.0;
const DESC_BODY_ZOOM: f32 = 1.0;

const NAME_MAX_LEN: usize = 32;
const PROFILE_MENU_W: f32 = 450.0;
const PROFILE_MENU_HEADER_H: f32 = 56.0;
const PROFILE_MENU_ITEM_H: f32 = 44.0;
const PROFILE_MENU_BORDER: f32 = 3.0;

#[derive(Clone, Debug)]
enum RowKind {
    CreateNew,
    Profile { id: String, display_name: String },
    Exit,
}

#[derive(Clone, Debug)]
struct Row {
    kind: RowKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum NavDirection {
    Up,
    Down,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum NavWrap {
    Wrap,
    Clamp,
}

#[derive(Clone, Debug)]
struct NameEntryState {
    mode: NameEntryMode,
    value: String,
    error: Option<Arc<str>>,
    blink_t: f32,
}

#[derive(Clone, Debug)]
enum NameEntryMode {
    Create,
    Rename { id: String },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ProfileMenuAction {
    SetP1,
    SetP2,
    Rename,
    Delete,
}

fn profile_menu_action_label(action: ProfileMenuAction) -> Arc<str> {
    match action {
        ProfileMenuAction::SetP1 => tr("Profiles", "SetP1"),
        ProfileMenuAction::SetP2 => tr("Profiles", "SetP2"),
        ProfileMenuAction::Rename => tr("Profiles", "Rename"),
        ProfileMenuAction::Delete => tr("Profiles", "Delete"),
    }
}

const PROFILE_MENU_ACTIONS: [ProfileMenuAction; 4] = [
    ProfileMenuAction::SetP1,
    ProfileMenuAction::SetP2,
    ProfileMenuAction::Rename,
    ProfileMenuAction::Delete,
];

#[derive(Clone, Debug)]
struct ProfileMenuState {
    id: String,
    display_name: String,
    selected_action: usize,
}

#[derive(Clone, Debug)]
struct DeleteConfirmState {
    id: String,
    display_name: String,
    error: Option<Arc<str>>,
}

pub struct State {
    pub selected: usize,
    prev_selected: usize,
    pub active_color_index: i32,
    bg: heart_bg::State,
    rows: Vec<Row>,
    nav_key_held_direction: Option<NavDirection>,
    nav_key_held_since: Option<Instant>,
    nav_key_last_scrolled_at: Option<Instant>,
    name_entry: Option<NameEntryState>,
    profile_menu: Option<ProfileMenuState>,
    delete_confirm: Option<DeleteConfirmState>,
    menu_lr_chord: screen_input::MenuLrChordTracker,
    menu_lr_undo: i8,
}

pub fn init() -> State {
    let rows = build_rows();
    State {
        selected: 0,
        prev_selected: 0,
        active_color_index: color::DEFAULT_COLOR_INDEX,
        bg: heart_bg::State::new(),
        rows,
        nav_key_held_direction: None,
        nav_key_held_since: None,
        nav_key_last_scrolled_at: None,
        name_entry: None,
        profile_menu: None,
        delete_confirm: None,
        menu_lr_chord: screen_input::MenuLrChordTracker::default(),
        menu_lr_undo: 0,
    }
}

fn build_rows() -> Vec<Row> {
    let profiles = profile::scan_local_profiles();
    let mut out = Vec::with_capacity(profiles.len() + 2);
    out.push(Row {
        kind: RowKind::CreateNew,
    });
    for p in profiles {
        out.push(Row {
            kind: RowKind::Profile {
                id: p.id,
                display_name: p.display_name,
            },
        });
    }
    out.push(Row {
        kind: RowKind::Exit,
    });
    out
}

fn refresh_rows(state: &mut State) {
    state.rows = build_rows();
    if state.rows.is_empty() {
        state.selected = 0;
        state.prev_selected = 0;
        return;
    }
    state.selected = state.selected.min(state.rows.len() - 1);
    state.prev_selected = state.prev_selected.min(state.rows.len() - 1);
}

fn move_selected(state: &mut State, dir: NavDirection, wrap: NavWrap) {
    let total = state.rows.len();
    if total == 0 {
        state.selected = 0;
        return;
    }
    let last = total - 1;
    state.prev_selected = state.selected;
    state.selected = match dir {
        NavDirection::Up => {
            if state.selected == 0 {
                match wrap {
                    NavWrap::Wrap => last,
                    NavWrap::Clamp => 0,
                }
            } else {
                state.selected - 1
            }
        }
        NavDirection::Down => {
            if state.selected >= last {
                match wrap {
                    NavWrap::Wrap => 0,
                    NavWrap::Clamp => last,
                }
            } else {
                state.selected + 1
            }
        }
    };
}

fn on_nav_press(state: &mut State, dir: NavDirection) {
    let now = Instant::now();
    state.nav_key_held_direction = Some(dir);
    state.nav_key_held_since = Some(now);
    state.nav_key_last_scrolled_at = Some(now);
}

fn on_nav_release(state: &mut State, dir: NavDirection) {
    if state.nav_key_held_direction == Some(dir) {
        state.nav_key_held_direction = None;
        state.nav_key_held_since = None;
        state.nav_key_last_scrolled_at = None;
    }
}

fn reset_nav_hold(state: &mut State) {
    state.nav_key_held_direction = None;
    state.nav_key_held_since = None;
    state.nav_key_last_scrolled_at = None;
}

fn scroll_offset(selected: usize, total_rows: usize) -> usize {
    let anchor_row: usize = 4;
    let max_offset = total_rows.saturating_sub(VISIBLE_ROWS);
    if total_rows <= VISIBLE_ROWS {
        0
    } else {
        selected.saturating_sub(anchor_row).min(max_offset)
    }
}

fn update_hold_scroll(state: &mut State) {
    if state.name_entry.is_some() || state.profile_menu.is_some() || state.delete_confirm.is_some()
    {
        return;
    }
    let Some(dir) = state.nav_key_held_direction else {
        return;
    };
    let Some(held_since) = state.nav_key_held_since else {
        return;
    };
    let Some(last_at) = state.nav_key_last_scrolled_at else {
        return;
    };

    let now = Instant::now();
    if now.duration_since(held_since) < NAV_INITIAL_HOLD_DELAY {
        return;
    }
    if now.duration_since(last_at) < NAV_REPEAT_SCROLL_INTERVAL {
        return;
    }

    move_selected(state, dir, NavWrap::Clamp);
    state.nav_key_last_scrolled_at = Some(now);
}

fn update_name_entry_blink(state: &mut State, dt: f32) {
    let Some(entry) = state.name_entry.as_mut() else {
        return;
    };
    entry.blink_t = (entry.blink_t + dt) % 1.0;
}

pub fn update(state: &mut State, dt: f32) -> Option<ScreenAction> {
    update_hold_scroll(state);
    update_name_entry_blink(state, dt);
    None
}

fn name_conflicts(state: &State, name: &str, skip_profile_id: Option<&str>) -> bool {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return false;
    }
    for row in &state.rows {
        let RowKind::Profile { id, display_name } = &row.kind else {
            continue;
        };
        if skip_profile_id.is_some_and(|skip| skip == id) {
            continue;
        }
        if display_name.trim() == trimmed {
            return true;
        }
    }
    false
}

fn default_new_profile_name(state: &State) -> String {
    for i in 1..1000 {
        let candidate = format!("New{i:04}");
        if !name_conflicts(state, &candidate, None) {
            return candidate;
        }
    }
    "New0001".to_string()
}

fn validate_profile_name(state: &State, mode: &NameEntryMode, name: &str) -> Result<(), Arc<str>> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(tr("Profiles", "NameCannotBeBlank"));
    }

    let skip_id = match mode {
        NameEntryMode::Create => None,
        NameEntryMode::Rename { id } => Some(id.as_str()),
    };
    if name_conflicts(state, trimmed, skip_id) {
        return Err(tr("Profiles", "NameConflict"));
    }
    Ok(())
}

fn try_submit_name_entry(state: &mut State, entry: &NameEntryState) -> Result<String, Arc<str>> {
    validate_profile_name(state, &entry.mode, &entry.value)?;
    let trimmed = entry.value.trim();
    match &entry.mode {
        NameEntryMode::Create => {
            profile::create_local_profile(trimmed).map_err(|_| tr("Profiles", "CreateFailed"))
        }
        NameEntryMode::Rename { id } => profile::rename_local_profile(id, trimmed)
            .map(|()| id.clone())
            .map_err(|_| tr("Profiles", "RenameFailed")),
    }
}

fn confirm_name_entry(state: &mut State) {
    let Some(entry) = state.name_entry.take() else {
        return;
    };

    match try_submit_name_entry(state, &entry) {
        Ok(id) => {
            audio::play_sfx("assets/sounds/start.ogg");
            refresh_rows(state);
            reset_nav_hold(state);
            if let Some(pos) = state.rows.iter().position(|r| match &r.kind {
                RowKind::Profile { id: row_id, .. } => row_id == &id,
                _ => false,
            }) {
                state.selected = pos;
                state.prev_selected = pos;
            }
        }
        Err(e) => {
            state.name_entry = Some(NameEntryState {
                mode: entry.mode,
                value: entry.value,
                error: Some(e),
                blink_t: entry.blink_t,
            });
        }
    }
}

fn cancel_name_entry(state: &mut State) {
    state.name_entry = None;
    reset_nav_hold(state);
}

fn begin_name_entry_create(state: &mut State) {
    reset_nav_hold(state);
    state.name_entry = Some(NameEntryState {
        mode: NameEntryMode::Create,
        value: default_new_profile_name(state),
        error: None,
        blink_t: 0.0,
    });
}

fn begin_name_entry_rename(state: &mut State, id: &str, display_name: &str) {
    reset_nav_hold(state);
    state.name_entry = Some(NameEntryState {
        mode: NameEntryMode::Rename { id: id.to_string() },
        value: display_name.to_string(),
        error: None,
        blink_t: 0.0,
    });
}

fn begin_profile_menu(state: &mut State, id: &str, display_name: &str) {
    reset_nav_hold(state);
    state.profile_menu = Some(ProfileMenuState {
        id: id.to_string(),
        display_name: display_name.to_string(),
        selected_action: 0,
    });
}

fn cancel_profile_menu(state: &mut State) {
    state.profile_menu = None;
    reset_nav_hold(state);
}

fn move_profile_menu_selected(state: &mut State, dir: NavDirection) {
    let Some(menu) = state.profile_menu.as_mut() else {
        return;
    };
    let len = PROFILE_MENU_ACTIONS.len();
    if len == 0 {
        menu.selected_action = 0;
        return;
    }
    menu.selected_action = match dir {
        NavDirection::Up => {
            if menu.selected_action == 0 {
                len - 1
            } else {
                menu.selected_action - 1
            }
        }
        NavDirection::Down => (menu.selected_action + 1) % len,
    };
}

fn confirm_profile_menu(state: &mut State) {
    let Some(menu) = state.profile_menu.clone() else {
        return;
    };
    let Some(action) = PROFILE_MENU_ACTIONS.get(menu.selected_action).copied() else {
        return;
    };

    match action {
        ProfileMenuAction::SetP1 => {
            let _ = profile::set_active_profile_for_side(
                profile::PlayerSide::P1,
                profile::ActiveProfile::Local {
                    id: menu.id.clone(),
                },
            );
            refresh_rows(state);
            cancel_profile_menu(state);
            audio::play_sfx("assets/sounds/start.ogg");
        }
        ProfileMenuAction::SetP2 => {
            let _ = profile::set_active_profile_for_side(
                profile::PlayerSide::P2,
                profile::ActiveProfile::Local {
                    id: menu.id.clone(),
                },
            );
            refresh_rows(state);
            cancel_profile_menu(state);
            audio::play_sfx("assets/sounds/start.ogg");
        }
        ProfileMenuAction::Rename => {
            state.profile_menu = None;
            begin_name_entry_rename(state, &menu.id, &menu.display_name);
            audio::play_sfx("assets/sounds/start.ogg");
        }
        ProfileMenuAction::Delete => {
            state.profile_menu = None;
            begin_delete_confirm(state, &menu.id, &menu.display_name);
            audio::play_sfx("assets/sounds/start.ogg");
        }
    }
}

fn begin_delete_confirm(state: &mut State, id: &str, display_name: &str) {
    reset_nav_hold(state);
    state.profile_menu = None;
    state.delete_confirm = Some(DeleteConfirmState {
        id: id.to_string(),
        display_name: display_name.to_string(),
        error: None,
    });
}

#[inline(always)]
fn selected_after_delete(selected_before: usize, total_after: usize) -> usize {
    if total_after == 0 {
        return 0;
    }
    let mut selected = selected_before.min(total_after - 1);
    if selected + 1 == total_after && selected > 0 {
        selected -= 1;
    }
    selected
}

fn confirm_delete(state: &mut State) {
    let Some(confirm) = state.delete_confirm.take() else {
        return;
    };

    let selected_before = state.selected;
    match profile::delete_local_profile(&confirm.id) {
        Ok(()) => {
            audio::play_sfx("assets/sounds/start.ogg");
            refresh_rows(state);
            reset_nav_hold(state);
            let selected = selected_after_delete(selected_before, state.rows.len());
            state.selected = selected;
            state.prev_selected = selected;
        }
        Err(_) => {
            state.delete_confirm = Some(DeleteConfirmState {
                id: confirm.id,
                display_name: confirm.display_name,
                error: Some(tr("Profiles", "DeleteFailed")),
            });
        }
    }
}

fn cancel_delete_confirm(state: &mut State) {
    state.delete_confirm = None;
    reset_nav_hold(state);
}

#[inline(always)]
fn activate_selected_row(state: &mut State) -> ScreenAction {
    let total = state.rows.len();
    if total == 0 {
        return ScreenAction::None;
    }
    let sel = state.selected.min(total - 1);
    let start_row = state.rows[sel].kind.clone();
    match start_row {
        RowKind::CreateNew => {
            begin_name_entry_create(state);
            ScreenAction::None
        }
        RowKind::Exit => {
            audio::play_sfx("assets/sounds/start.ogg");
            ScreenAction::Navigate(Screen::Options)
        }
        RowKind::Profile { id, display_name } => {
            begin_profile_menu(state, &id, &display_name);
            audio::play_sfx("assets/sounds/start.ogg");
            ScreenAction::None
        }
    }
}

#[inline(always)]
fn undo_nav_move(state: &mut State, undo: i8) {
    match undo {
        1 => move_selected(state, NavDirection::Down, NavWrap::Wrap),
        -1 => move_selected(state, NavDirection::Up, NavWrap::Wrap),
        _ => {}
    }
}

#[inline(always)]
fn undo_profile_menu_move(state: &mut State, undo: i8) {
    match undo {
        1 => move_profile_menu_selected(state, NavDirection::Down),
        -1 => move_profile_menu_selected(state, NavDirection::Up),
        _ => {}
    }
}

pub fn handle_input(state: &mut State, ev: &InputEvent) -> ScreenAction {
    let three_key_action = screen_input::three_key_menu_action(&mut state.menu_lr_chord, ev);
    if screen_input::dedicated_three_key_nav_enabled() {
        match ev.action {
            VirtualAction::p1_left
            | VirtualAction::p1_menu_left
            | VirtualAction::p2_left
            | VirtualAction::p2_menu_left
                if !ev.pressed =>
            {
                state.menu_lr_undo = 0;
                on_nav_release(state, NavDirection::Up);
                return ScreenAction::None;
            }
            VirtualAction::p1_right
            | VirtualAction::p1_menu_right
            | VirtualAction::p2_right
            | VirtualAction::p2_menu_right
                if !ev.pressed =>
            {
                state.menu_lr_undo = 0;
                on_nav_release(state, NavDirection::Down);
                return ScreenAction::None;
            }
            _ => {}
        }
        if let Some((_, nav)) = three_key_action {
            if state.name_entry.is_some() {
                match nav {
                    screen_input::ThreeKeyMenuAction::Confirm => confirm_name_entry(state),
                    screen_input::ThreeKeyMenuAction::Cancel => cancel_name_entry(state),
                    _ => {}
                }
                return ScreenAction::None;
            }
            if state.delete_confirm.is_some() {
                match nav {
                    screen_input::ThreeKeyMenuAction::Confirm => confirm_delete(state),
                    screen_input::ThreeKeyMenuAction::Cancel => cancel_delete_confirm(state),
                    _ => {}
                }
                return ScreenAction::None;
            }
            if state.profile_menu.is_some() {
                return match nav {
                    screen_input::ThreeKeyMenuAction::Prev => {
                        move_profile_menu_selected(state, NavDirection::Up);
                        on_nav_press(state, NavDirection::Up);
                        state.menu_lr_undo = 1;
                        audio::play_sfx("assets/sounds/change.ogg");
                        ScreenAction::None
                    }
                    screen_input::ThreeKeyMenuAction::Next => {
                        move_profile_menu_selected(state, NavDirection::Down);
                        on_nav_press(state, NavDirection::Down);
                        state.menu_lr_undo = -1;
                        audio::play_sfx("assets/sounds/change.ogg");
                        ScreenAction::None
                    }
                    screen_input::ThreeKeyMenuAction::Confirm => {
                        state.menu_lr_undo = 0;
                        confirm_profile_menu(state);
                        ScreenAction::None
                    }
                    screen_input::ThreeKeyMenuAction::Cancel => {
                        undo_profile_menu_move(state, state.menu_lr_undo);
                        state.menu_lr_undo = 0;
                        cancel_profile_menu(state);
                        ScreenAction::None
                    }
                };
            }
            return match nav {
                screen_input::ThreeKeyMenuAction::Prev => {
                    move_selected(state, NavDirection::Up, NavWrap::Wrap);
                    on_nav_press(state, NavDirection::Up);
                    state.menu_lr_undo = 1;
                    ScreenAction::None
                }
                screen_input::ThreeKeyMenuAction::Next => {
                    move_selected(state, NavDirection::Down, NavWrap::Wrap);
                    on_nav_press(state, NavDirection::Down);
                    state.menu_lr_undo = -1;
                    ScreenAction::None
                }
                screen_input::ThreeKeyMenuAction::Confirm => {
                    state.menu_lr_undo = 0;
                    activate_selected_row(state)
                }
                screen_input::ThreeKeyMenuAction::Cancel => {
                    undo_nav_move(state, state.menu_lr_undo);
                    state.menu_lr_undo = 0;
                    ScreenAction::Navigate(Screen::Options)
                }
            };
        }
    }
    if state.name_entry.is_some() {
        match ev.action {
            VirtualAction::p1_start | VirtualAction::p2_start if ev.pressed => {
                confirm_name_entry(state)
            }
            VirtualAction::p1_back | VirtualAction::p2_back if ev.pressed => {
                cancel_name_entry(state)
            }
            _ => {}
        }
        return ScreenAction::None;
    }

    if state.delete_confirm.is_some() {
        match ev.action {
            VirtualAction::p1_start | VirtualAction::p2_start if ev.pressed => {
                confirm_delete(state)
            }
            VirtualAction::p1_back | VirtualAction::p2_back if ev.pressed => {
                cancel_delete_confirm(state)
            }
            _ => {}
        }
        return ScreenAction::None;
    }

    if state.profile_menu.is_some() {
        match ev.action {
            VirtualAction::p1_back | VirtualAction::p2_back if ev.pressed => {
                cancel_profile_menu(state)
            }
            VirtualAction::p1_up
            | VirtualAction::p1_menu_up
            | VirtualAction::p2_up
            | VirtualAction::p2_menu_up
                if ev.pressed =>
            {
                move_profile_menu_selected(state, NavDirection::Up);
                audio::play_sfx("assets/sounds/change.ogg");
            }
            VirtualAction::p1_down
            | VirtualAction::p1_menu_down
            | VirtualAction::p2_down
            | VirtualAction::p2_menu_down
                if ev.pressed =>
            {
                move_profile_menu_selected(state, NavDirection::Down);
                audio::play_sfx("assets/sounds/change.ogg");
            }
            VirtualAction::p1_start | VirtualAction::p2_start if ev.pressed => {
                confirm_profile_menu(state)
            }
            _ => {}
        }
        return ScreenAction::None;
    }

    match ev.action {
        VirtualAction::p1_back | VirtualAction::p2_back if ev.pressed => {
            return ScreenAction::Navigate(Screen::Options);
        }
        VirtualAction::p1_up
        | VirtualAction::p1_menu_up
        | VirtualAction::p2_up
        | VirtualAction::p2_menu_up => {
            if ev.pressed {
                move_selected(state, NavDirection::Up, NavWrap::Wrap);
                on_nav_press(state, NavDirection::Up);
            } else {
                on_nav_release(state, NavDirection::Up);
            }
        }
        VirtualAction::p1_down
        | VirtualAction::p1_menu_down
        | VirtualAction::p2_down
        | VirtualAction::p2_menu_down => {
            if ev.pressed {
                move_selected(state, NavDirection::Down, NavWrap::Wrap);
                on_nav_press(state, NavDirection::Down);
            } else {
                on_nav_release(state, NavDirection::Down);
            }
        }
        VirtualAction::p1_start | VirtualAction::p2_start if ev.pressed => {
            return activate_selected_row(state);
        }
        _ => {}
    }

    ScreenAction::None
}

pub fn handle_raw_key_event(
    state: &mut State,
    key_event: Option<&RawKeyboardEvent>,
    text: Option<&str>,
) -> ScreenAction {
    let Some(entry) = state.name_entry.as_mut() else {
        return ScreenAction::None;
    };
    if let Some(key_event) = key_event {
        if !key_event.pressed {
            return ScreenAction::None;
        }
        let code = key_event.code;
        match code {
            KeyCode::Backspace => {
                let _ = entry.value.pop();
                entry.error = None;
                return ScreenAction::None;
            }
            KeyCode::Escape => return ScreenAction::None,
            _ => {}
        }
    }

    let Some(text) = text else {
        return ScreenAction::None;
    };

    let mut len = entry.value.chars().count();
    for ch in text.chars() {
        if ch.is_control() {
            continue;
        }
        if len >= NAME_MAX_LEN {
            break;
        }
        entry.value.push(ch);
        len += 1;
    }
    entry.error = None;
    ScreenAction::None
}

pub fn in_transition() -> (Vec<Actor>, f32) {
    transitions::fade_in_black(TRANSITION_IN_DURATION, 1100)
}

pub fn out_transition() -> (Vec<Actor>, f32) {
    transitions::fade_out_black(TRANSITION_OUT_DURATION, 1200)
}

fn scaled_block_origin_with_margins() -> (f32, f32, f32) {
    let total_w = LIST_W + SEP_W + DESC_W;
    let total_h = DESC_H;

    let sw = screen_width();
    let sh = screen_height();

    let content_top = BAR_H;
    let content_bottom = sh - BAR_H;
    let content_h = (content_bottom - content_top).max(0.0);

    let avail_w = (sw - LEFT_MARGIN_PX - RIGHT_MARGIN_PX).max(0.0);
    let avail_h = (content_h - FIRST_ROW_TOP_MARGIN_PX - BOTTOM_MARGIN_PX).max(0.0);

    let s_w = if total_w > 0.0 {
        avail_w / total_w
    } else {
        1.0
    };
    let s_h = if total_h > 0.0 {
        avail_h / total_h
    } else {
        1.0
    };
    let s = s_w.min(s_h).max(0.0);

    let ox = LEFT_MARGIN_PX + total_w.mul_add(-s, avail_w).max(0.0);
    let oy = content_top + FIRST_ROW_TOP_MARGIN_PX;
    (s, ox, oy)
}

fn indicator_text(id: &str, p1_id: Option<&str>, p2_id: Option<&str>) -> Option<Arc<str>> {
    let is_p1 = p1_id.is_some_and(|p1| p1 == id);
    let is_p2 = p2_id.is_some_and(|p2| p2 == id);
    match (is_p1, is_p2) {
        (true, true) => Some(tr("Profiles", "P1P2Assigned")),
        (true, false) => Some(tr("Profiles", "P1Assigned")),
        (false, true) => Some(tr("Profiles", "P2Assigned")),
        (false, false) => None,
    }
}

fn help_for_selected(state: &State, p1_id: Option<&str>, p2_id: Option<&str>) -> (String, String) {
    let Some(row) = state.rows.get(state.selected) else {
        return (String::new(), String::new());
    };

    match &row.kind {
        RowKind::CreateNew => {
            let title = tr("Profiles", "CreateProfileTitle");
            let b1 = tr("Profiles", "EnterProfileNamePrompt");
            let b2 = tr("Profiles", "PressStartConfirm");
            let b3 = tr("Profiles", "PressBackCancel");
            let bullets = make_bullets(&[&b1, &b2, &b3]);
            (title.to_string(), bullets)
        }
        RowKind::Exit => (tr("Profiles", "ReturnToOptions").to_string(), String::new()),
        RowKind::Profile { id, display_name } => {
            let title =
                tr_fmt("Profiles", "LocalProfileFormat", &[("name", display_name)]).to_string();

            let assigned = match indicator_text(id, p1_id, p2_id) {
                Some(tag) => tr_fmt("Profiles", "AssignedFormat", &[("tag", &tag)]).to_string(),
                None => tr("Profiles", "AssignedNone").to_string(),
            };
            let b1 = tr_fmt("Profiles", "IdFormat", &[("id", id)]).to_string();
            let b3 = tr("Profiles", "OpenActionsPrompt");
            let bullets = make_bullets(&[&b1, &assigned, &b3]);
            (title, bullets)
        }
    }
}

fn make_bullets(lines: &[&str]) -> String {
    let mut out = String::new();
    let mut first = true;
    for line in lines {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if !first {
            out.push('\n');
        }
        out.push('•');
        out.push(' ');
        out.push_str(trimmed);
        first = false;
    }
    out
}

fn push_desc(ui: &mut Vec<Actor>, state: &State, s: f32, desc_x: f32, list_y: f32) {
    let p1 = profile::active_local_profile_id_for_side(profile::PlayerSide::P1);
    let p2 = profile::active_local_profile_id_for_side(profile::PlayerSide::P2);
    let (title, bullets) = help_for_selected(state, p1.as_deref(), p2.as_deref());

    let mut cursor_y = DESC_TITLE_TOP_PAD_PX.mul_add(s, list_y);
    let title_x = desc_x + DESC_TITLE_SIDE_PAD_PX * s;
    let max_title_w = (DESC_W - 2.0 * DESC_TITLE_SIDE_PAD_PX)
        .mul_add(s, 0.0)
        .max(0.0);
    ui.push(act!(text:
        align(0.0, 0.0):
        xy(title_x, cursor_y):
        zoom(DESC_TITLE_ZOOM):
        diffuse(1.0, 1.0, 1.0, 1.0):
        font("miso"):
        maxwidth(max_title_w):
        settext(title):
        horizalign(left)
    ));

    cursor_y += DESC_BULLET_TOP_PAD_PX * s;
    if bullets.is_empty() {
        return;
    }

    let bullet_side_pad = DESC_BULLET_SIDE_PAD_PX * s;
    let bullet_x = DESC_BULLET_INDENT_PX.mul_add(s, desc_x + bullet_side_pad);
    let max_bullet_w = (DESC_W - 2.0 * DESC_BULLET_SIDE_PAD_PX)
        .mul_add(s, 0.0)
        .max(0.0);
    ui.push(act!(text:
        align(0.0, 0.0):
        xy(bullet_x, cursor_y):
        zoom(DESC_BODY_ZOOM):
        diffuse(1.0, 1.0, 1.0, 1.0):
        font("miso"):
        maxwidth(max_bullet_w):
        settext(bullets):
        horizalign(left)
    ));
}

fn push_name_entry_overlay(ui: &mut Vec<Actor>, state: &State) {
    let Some(entry) = &state.name_entry else {
        return;
    };

    let w = screen_width();
    let h = screen_height();
    let accent = color::simply_love_rgba(state.active_color_index);
    let border = 4.0;
    let box_w = (w * 0.75).clamp(560.0, 1200.0);
    let top_h = 210.0;
    let bottom_h = 72.0;
    let box_h = top_h + bottom_h + 2.0 * border;
    let cx = w * 0.5;
    let cy = h * 0.5;
    let top_cy = cy - box_h * 0.5 + border + top_h * 0.5;
    let bottom_cy = cy + box_h * 0.5 - border - bottom_h * 0.5;

    ui.push(act!(quad:
        align(0.5, 0.5):
        xy(cx, cy):
        zoomto(box_w, box_h):
        diffuse(1.0, 1.0, 1.0, 1.0):
        z(1001)
    ));
    ui.push(act!(quad:
        align(0.5, 0.5):
        xy(cx, top_cy):
        zoomto(box_w - 2.0 * border, top_h):
        diffuse(accent[0], accent[1], accent[2], 1.0):
        z(1002)
    ));
    ui.push(act!(quad:
        align(0.5, 0.5):
        xy(cx, bottom_cy):
        zoomto(box_w - 2.0 * border, bottom_h):
        diffuse(0.0, 0.0, 0.0, 1.0):
        z(1002)
    ));

    let name_prompt = tr("Profiles", "EnterProfileNamePrompt");
    ui.push(act!(text:
        align(0.5, 0.5):
        xy(cx, top_cy):
        font("miso"):
        zoom(1.0):
        settext(name_prompt):
        diffuse(1.0, 1.0, 1.0, 1.0):
        z(1003):
        horizalign(center)
    ));

    let cursor = if entry.blink_t < 0.5 { "_" } else { " " };
    let mut value = entry.value.clone();
    if value.chars().count() < NAME_MAX_LEN {
        value.push_str(cursor);
    }
    ui.push(act!(text:
        align(0.5, 0.5):
        xy(cx, bottom_cy):
        font("miso"):
        zoom(1.55):
        maxwidth(box_w - 40.0):
        settext(value):
        diffuse(1.0, 1.0, 1.0, 1.0):
        z(1003):
        horizalign(center)
    ));

    let Some(err) = &entry.error else {
        return;
    };
    ui.push(act!(text:
        align(0.5, 0.0):
        xy(cx, cy + box_h * 0.5 + 8.0):
        font("miso"):
        zoom(0.9):
        maxwidth(box_w - 40.0):
        settext(err.clone()):
        diffuse(1.0, 0.2, 0.2, 1.0):
        z(1003):
        horizalign(center)
    ));
}

fn push_delete_confirm_overlay(ui: &mut Vec<Actor>, state: &State) {
    let Some(confirm) = &state.delete_confirm else {
        return;
    };

    let w = screen_width();
    let h = screen_height();
    let box_w = 700.0_f32.min(w * 0.92);
    let box_h = 190.0_f32;
    let cx = w * 0.5;
    let cy = h * 0.5;

    push_overlay_backdrop(ui, w, h);
    push_overlay_box(ui, cx, cy, box_w, box_h);

    let prompt = tr_fmt(
        "Profiles",
        "DeleteConfirmFormat",
        &[("name", &confirm.display_name)],
    );
    ui.push(act!(text:
        align(0.5, 0.0):
        xy(cx, cy - box_h * 0.5 + 16.0):
        font("miso"):
        zoom(1.0):
        maxwidth(box_w - 40.0):
        settext(prompt):
        diffuse(1.0, 1.0, 1.0, 1.0):
        z(1002):
        horizalign(center)
    ));
    let cannot_be_undone = tr("Profiles", "CannotBeUndone");
    ui.push(act!(text:
        align(0.5, 0.0):
        xy(cx, cy - box_h * 0.5 + 58.0):
        font("miso"):
        zoom(0.9):
        settext(cannot_be_undone):
        diffuse(1.0, 1.0, 1.0, 1.0):
        z(1002):
        horizalign(center)
    ));
    let yes_no = tr("Profiles", "YesNoPrompt");
    ui.push(act!(text:
        align(0.5, 1.0):
        xy(cx, cy + box_h * 0.5 - 10.0):
        font("miso"):
        zoom(0.9):
        settext(yes_no):
        diffuse(1.0, 1.0, 1.0, 1.0):
        z(1002):
        horizalign(center)
    ));

    push_overlay_error(ui, confirm.error.as_ref(), cx, cy, box_w, box_h);
}

fn push_overlay_backdrop(ui: &mut Vec<Actor>, w: f32, h: f32) {
    ui.push(act!(quad:
        align(0.0, 0.0):
        xy(0.0, 0.0):
        zoomto(w, h):
        diffuse(0.0, 0.0, 0.0, 0.65):
        z(1000)
    ));
}

fn push_overlay_box(ui: &mut Vec<Actor>, cx: f32, cy: f32, w: f32, h: f32) {
    ui.push(act!(quad:
        align(0.5, 0.5):
        xy(cx, cy):
        zoomto(w, h):
        diffuse(0.2, 0.2, 0.2, 1.0):
        z(1001)
    ));
}

fn push_overlay_error(
    ui: &mut Vec<Actor>,
    err: Option<&Arc<str>>,
    cx: f32,
    cy: f32,
    box_w: f32,
    box_h: f32,
) {
    let Some(err) = err else {
        return;
    };
    ui.push(act!(text:
        align(0.5, 0.0):
        xy(cx, cy + box_h * 0.5 - 46.0):
        font("miso"):
        zoom(0.9):
        maxwidth(box_w - 40.0):
        settext(err.clone()):
        diffuse(1.0, 0.2, 0.2, 1.0):
        z(1002):
        horizalign(center)
    ));
}

fn push_list_chrome(
    ui: &mut Vec<Actor>,
    col_active_bg: [f32; 4],
    s: f32,
    list_x: f32,
    list_y: f32,
) {
    let list_w = LIST_W * s;
    let sep_w = SEP_W * s;
    let desc_h = DESC_H * s;

    ui.push(act!(quad:
        align(0.0, 0.0):
        xy(list_x + list_w, list_y):
        zoomto(sep_w, desc_h):
        diffuse(col_active_bg[0], col_active_bg[1], col_active_bg[2], col_active_bg[3])
    ));

    let desc_x = list_x + list_w + sep_w;
    ui.push(act!(quad:
        align(0.0, 0.0):
        xy(desc_x, list_y):
        zoomto(DESC_W * s, desc_h):
        diffuse(col_active_bg[0], col_active_bg[1], col_active_bg[2], col_active_bg[3])
    ));
}

struct RowColors {
    active_bg: [f32; 4],
    inactive_bg: [f32; 4],
    brand_bg: [f32; 4],
    active_text: [f32; 4],
    white: [f32; 4],
    black: [f32; 4],
}

fn row_label(kind: &RowKind) -> Arc<str> {
    match kind {
        RowKind::CreateNew => tr("Profiles", "CreateProfileButton"),
        RowKind::Exit => tr("Common", "Exit"),
        RowKind::Profile { display_name, .. } => Arc::from(display_name.as_str()),
    }
}

fn row_is_exit(kind: &RowKind) -> bool {
    matches!(kind, RowKind::Exit)
}

fn row_width(list_w: f32, sep_w: f32, is_active: bool, is_exit: bool) -> f32 {
    if is_exit {
        list_w - sep_w
    } else if is_active {
        list_w
    } else {
        list_w - sep_w
    }
}

fn row_bg_color(colors: &RowColors, is_active: bool, is_exit: bool) -> [f32; 4] {
    if is_active {
        if is_exit {
            colors.brand_bg
        } else {
            colors.active_bg
        }
    } else {
        colors.inactive_bg
    }
}

fn row_text_color(colors: &RowColors, is_active: bool, is_exit: bool) -> [f32; 4] {
    if is_exit {
        if is_active {
            colors.black
        } else {
            colors.white
        }
    } else if is_active {
        colors.active_text
    } else {
        colors.white
    }
}

fn push_row(
    ui: &mut Vec<Actor>,
    kind: &RowKind,
    is_active: bool,
    row_y: f32,
    list_x: f32,
    list_w: f32,
    sep_w: f32,
    s: f32,
    colors: &RowColors,
    p1_id: Option<&str>,
    p2_id: Option<&str>,
) {
    let is_exit = row_is_exit(kind);
    let row_mid_y = (0.5 * ROW_H).mul_add(s, row_y);
    let row_w = row_width(list_w, sep_w, is_active, is_exit);
    let bg = row_bg_color(colors, is_active, is_exit);
    let text_col = row_text_color(colors, is_active, is_exit);

    ui.push(act!(quad:
        align(0.0, 0.0):
        xy(list_x, row_y):
        zoomto(row_w, ROW_H * s):
        diffuse(bg[0], bg[1], bg[2], bg[3])
    ));

    if !is_exit {
        let heart_x = HEART_LEFT_PAD.mul_add(s, list_x);
        let heart_tint = if is_active {
            colors.active_text
        } else {
            colors.white
        };
        ui.push(act!(sprite(visual_styles::select_color_texture_key()):
            align(0.0, 0.5):
            xy(heart_x, row_mid_y):
            zoom(HEART_ZOOM):
            diffuse(heart_tint[0], heart_tint[1], heart_tint[2], heart_tint[3])
        ));
    }

    let text_x = TEXT_LEFT_PAD.mul_add(s, list_x);
    ui.push(act!(text:
        align(0.0, 0.5):
        xy(text_x, row_mid_y):
        zoom(ITEM_TEXT_ZOOM):
        diffuse(text_col[0], text_col[1], text_col[2], text_col[3]):
        font("miso"):
        settext(row_label(kind)):
        horizalign(left)
    ));

    if let RowKind::Profile { id, .. } = kind
        && let Some(tag) = indicator_text(id, p1_id, p2_id)
    {
        ui.push(act!(text:
            align(1.0, 0.5):
            xy(list_x + list_w - 12.0 * s, row_mid_y):
            zoom(0.75):
            diffuse(text_col[0], text_col[1], text_col[2], text_col[3]):
            font("miso"):
            settext(tag):
            horizalign(right)
        ));
    }
}

fn push_rows(
    ui: &mut Vec<Actor>,
    state: &State,
    s: f32,
    list_x: f32,
    list_y: f32,
    col_active_bg: [f32; 4],
    col_inactive_bg: [f32; 4],
) {
    let list_w = LIST_W * s;
    let sep_w = SEP_W * s;
    let total_rows = state.rows.len();
    let offset = scroll_offset(state.selected, total_rows);
    let colors = RowColors {
        active_bg: col_active_bg,
        inactive_bg: col_inactive_bg,
        brand_bg: color::simply_love_rgba(state.active_color_index),
        active_text: color::simply_love_rgba(state.active_color_index + state.selected as i32),
        white: [1.0, 1.0, 1.0, 1.0],
        black: [0.0, 0.0, 0.0, 1.0],
    };

    let p1 = profile::active_local_profile_id_for_side(profile::PlayerSide::P1);
    let p2 = profile::active_local_profile_id_for_side(profile::PlayerSide::P2);
    let p1_id = p1.as_deref();
    let p2_id = p2.as_deref();

    for i_vis in 0..VISIBLE_ROWS {
        let row_idx = offset + i_vis;
        if row_idx >= total_rows {
            break;
        }
        let row_y = ((i_vis as f32) * (ROW_H + ROW_GAP)).mul_add(s, list_y);
        let is_active = row_idx == state.selected;
        push_row(
            ui,
            &state.rows[row_idx].kind,
            is_active,
            row_y,
            list_x,
            list_w,
            sep_w,
            s,
            &colors,
            p1_id,
            p2_id,
        );
    }
}

fn selected_row_top_y(state: &State, s: f32, list_y: f32) -> f32 {
    if state.rows.is_empty() {
        return list_y;
    }
    let offset = scroll_offset(state.selected, state.rows.len());
    let vis = state.selected.saturating_sub(offset).min(VISIBLE_ROWS - 1);
    ((vis as f32) * (ROW_H + ROW_GAP)).mul_add(s, list_y)
}

fn push_profile_menu_overlay(ui: &mut Vec<Actor>, state: &State, s: f32, list_x: f32, list_y: f32) {
    let Some(menu) = &state.profile_menu else {
        return;
    };

    let row_top = selected_row_top_y(state, s, list_y);
    let menu_w = PROFILE_MENU_W * s;
    let header_h = PROFILE_MENU_HEADER_H * s;
    let item_h = PROFILE_MENU_ITEM_H * s;
    let border = PROFILE_MENU_BORDER * s;
    let body_h = item_h * PROFILE_MENU_ACTIONS.len() as f32;
    let menu_h = header_h + body_h + 2.0 * border;
    let mut menu_x = (LIST_W * 0.52).mul_add(s, list_x);
    let mut menu_y = row_top;

    menu_x = menu_x.clamp(10.0, (screen_width() - menu_w - 10.0).max(10.0));
    menu_y = menu_y.clamp(
        BAR_H + 4.0,
        (screen_height() - BAR_H - menu_h - 4.0).max(BAR_H + 4.0),
    );

    let inner_x = menu_x + border;
    let inner_y = menu_y + border;
    let inner_w = (menu_w - 2.0 * border).max(0.0);
    let accent = color::simply_love_rgba(state.active_color_index);

    ui.push(act!(quad:
        align(0.0, 0.0):
        xy(menu_x, menu_y):
        zoomto(menu_w, menu_h):
        diffuse(1.0, 1.0, 1.0, 1.0):
        z(1004)
    ));
    ui.push(act!(quad:
        align(0.0, 0.0):
        xy(inner_x, inner_y):
        zoomto(inner_w, header_h):
        diffuse(0.92, 0.92, 0.92, 1.0):
        z(1005)
    ));
    ui.push(act!(quad:
        align(0.0, 0.0):
        xy(inner_x, inner_y + header_h):
        zoomto(inner_w, body_h):
        diffuse(0.0, 0.06, 0.10, 0.96):
        z(1005)
    ));
    ui.push(act!(text:
        align(0.0, 0.5):
        xy(14.0_f32.mul_add(s, inner_x), inner_y + header_h * 0.5):
        font("miso"):
        zoom(1.20):
        settext(menu.display_name.clone()):
        diffuse(0.0, 0.0, 0.0, 1.0):
        z(1006):
        horizalign(left)
    ));

    for (i, action) in PROFILE_MENU_ACTIONS.iter().enumerate() {
        let row_y = (i as f32).mul_add(item_h, inner_y + header_h);
        let selected = i == menu.selected_action;
        if selected {
            ui.push(act!(quad:
                align(0.0, 0.0):
                xy(inner_x, row_y):
                zoomto(inner_w, item_h):
                diffuse(0.17, 0.23, 0.28, 0.95):
                z(1005)
            ));
        }
        let text_col = if selected {
            [accent[0], accent[1], accent[2], 1.0]
        } else {
            [1.0, 1.0, 1.0, 1.0]
        };
        ui.push(act!(text:
            align(0.0, 0.5):
            xy(14.0_f32.mul_add(s, inner_x), row_y + item_h * 0.5):
            font("miso"):
            zoom(1.0):
            settext(profile_menu_action_label(*action)):
            diffuse(text_col[0], text_col[1], text_col[2], text_col[3]):
            z(1006):
            horizalign(left)
        ));
    }
}

pub fn get_actors(
    state: &State,
    _asset_manager: &AssetManager,
    alpha_multiplier: f32,
) -> Vec<Actor> {
    let mut actors: Vec<Actor> = Vec::with_capacity(220);

    actors.extend(state.bg.build(heart_bg::Params {
        active_color_index: state.active_color_index,
        backdrop_rgba: [0.0, 0.0, 0.0, 1.0],
        alpha_mul: 1.0,
    }));

    if alpha_multiplier <= 0.0 {
        return actors;
    }

    let mut ui = Vec::new();
    let title = tr("ScreenTitles", "ManageProfiles");
    ui.push(screen_bar::build(screen_bar::ScreenBarParams {
        title: &title,
        title_placement: ScreenBarTitlePlacement::Left,
        position: ScreenBarPosition::Top,
        transparent: false,
        left_text: None,
        center_text: None,
        right_text: None,
        left_avatar: None,
        right_avatar: None,
        fg_color: [1.0, 1.0, 1.0, 1.0],
    }));

    let col_active_bg = color::rgba_hex("#333333");
    let base_inactive = color::rgba_hex("#071016");
    let col_inactive_bg: [f32; 4] = [base_inactive[0], base_inactive[1], base_inactive[2], 0.8];

    let (s, list_x, list_y) = scaled_block_origin_with_margins();
    push_list_chrome(&mut ui, col_active_bg, s, list_x, list_y);
    push_rows(
        &mut ui,
        state,
        s,
        list_x,
        list_y,
        col_active_bg,
        col_inactive_bg,
    );

    let list_w = LIST_W * s;
    let sep_w = SEP_W * s;
    let desc_x = list_x + list_w + sep_w;
    push_desc(&mut ui, state, s, desc_x, list_y);
    push_profile_menu_overlay(&mut ui, state, s, list_x, list_y);
    push_name_entry_overlay(&mut ui, state);
    push_delete_confirm_overlay(&mut ui, state);

    for actor in &mut ui {
        actor.mul_alpha(alpha_multiplier);
    }
    actors.extend(ui);
    actors
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::input::InputSource;

    fn input_event(action: VirtualAction, pressed: bool) -> InputEvent {
        let now = Instant::now();
        InputEvent {
            action,
            input_slot: 0,
            pressed,
            source: InputSource::Keyboard,
            timestamp: now,
            timestamp_host_nanos: 0,
            stored_at: now,
            emitted_at: now,
        }
    }

    fn press(state: &mut State, action: VirtualAction) -> ScreenAction {
        handle_input(state, &input_event(action, true))
    }

    fn state_with_profile_row() -> State {
        let mut state = init();
        state.rows = vec![
            Row {
                kind: RowKind::CreateNew,
            },
            Row {
                kind: RowKind::Profile {
                    id: "test-profile".to_string(),
                    display_name: "Test Profile".to_string(),
                },
            },
            Row {
                kind: RowKind::Exit,
            },
        ];
        state.selected = 0;
        state.prev_selected = 0;
        state
    }

    #[test]
    fn p2_can_navigate_profile_list() {
        let mut state = state_with_profile_row();

        press(&mut state, VirtualAction::p2_down);
        assert_eq!(state.selected, 1);

        press(&mut state, VirtualAction::p2_down);
        assert_eq!(state.selected, 2);

        assert!(matches!(
            press(&mut state, VirtualAction::p2_start),
            ScreenAction::Navigate(Screen::Options)
        ));
    }

    #[test]
    fn p2_can_navigate_profile_action_menu() {
        let mut state = state_with_profile_row();

        press(&mut state, VirtualAction::p2_down);
        press(&mut state, VirtualAction::p2_start);
        assert_eq!(
            state.profile_menu.as_ref().map(|m| m.selected_action),
            Some(0)
        );

        press(&mut state, VirtualAction::p2_down);
        assert_eq!(
            state.profile_menu.as_ref().map(|m| m.selected_action),
            Some(1)
        );

        press(&mut state, VirtualAction::p2_back);
        assert!(state.profile_menu.is_none());
    }
}
