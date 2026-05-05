use crate::act;
use crate::assets::AssetManager;
use crate::assets::i18n::tr;
use crate::engine::audio;
use crate::engine::input::{InputEvent, RawKeyboardEvent, VirtualAction};
use crate::engine::present::actors::Actor;
use crate::engine::present::color;
use crate::engine::space::{screen_height, screen_width};
use crate::game::profile::{self, ProfileCredentials};
use crate::screens::components::shared::screen_bar::{
    self, ScreenBarPosition, ScreenBarTitlePlacement,
};
use crate::screens::components::shared::transitions;
use crate::screens::components::shared::visual_style_bg;
use crate::screens::input as screen_input;
use crate::screens::{Screen, ScreenAction};
use std::sync::Arc;
use std::sync::Mutex;
use std::time::{Duration, Instant};
use winit::keyboard::KeyCode;

const TRANSITION_IN_DURATION: f32 = 0.4;
const TRANSITION_OUT_DURATION: f32 = 0.4;

const NAV_INITIAL_HOLD_DELAY: Duration = Duration::from_millis(300);
const NAV_REPEAT_SCROLL_INTERVAL: Duration = Duration::from_millis(50);

const FIELD_MAX_LEN: usize = 256;
const DISPLAY_NAME_MAX_LEN: usize = 32;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum NavDirection {
    Up,
    Down,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RowId {
    DisplayName,
    GsApiKey,
    GsUsername,
    GsIsPadPlayer,
    AcApiKey,
    ShowSecrets,
    Save,
    Cancel,
}

const ROW_ORDER: [RowId; 8] = [
    RowId::DisplayName,
    RowId::GsApiKey,
    RowId::GsUsername,
    RowId::GsIsPadPlayer,
    RowId::AcApiKey,
    RowId::ShowSecrets,
    RowId::Save,
    RowId::Cancel,
];

fn row_label(id: RowId) -> Arc<str> {
    let key = match id {
        RowId::DisplayName => "DisplayName",
        RowId::GsApiKey => "GsApiKey",
        RowId::GsUsername => "GsUsername",
        RowId::GsIsPadPlayer => "GsIsPadPlayer",
        RowId::AcApiKey => "AcApiKey",
        RowId::ShowSecrets => "ShowSecrets",
        RowId::Save => "Save",
        RowId::Cancel => "Cancel",
    };
    tr("EditProfile", key)
}

fn is_text_row(id: RowId) -> bool {
    matches!(
        id,
        RowId::DisplayName | RowId::GsApiKey | RowId::GsUsername | RowId::AcApiKey
    )
}

fn is_secret_row(id: RowId) -> bool {
    matches!(id, RowId::GsApiKey | RowId::AcApiKey)
}

fn is_toggle_row(id: RowId) -> bool {
    matches!(id, RowId::GsIsPadPlayer | RowId::ShowSecrets)
}

fn field_max_len(id: RowId) -> usize {
    match id {
        RowId::DisplayName => DISPLAY_NAME_MAX_LEN,
        _ => FIELD_MAX_LEN,
    }
}

#[derive(Clone, Debug)]
struct TextEditState {
    target: RowId,
    label: Arc<str>,
    value: String,
    masked: bool,
    error: Option<Arc<str>>,
    blink_t: f32,
}

pub struct State {
    pub active_color_index: i32,
    bg: visual_style_bg::State,
    profile_id: String,
    initial_credentials: ProfileCredentials,
    draft: ProfileCredentials,
    show_secrets: bool,
    selected_row: usize,
    nav_key_held_direction: Option<NavDirection>,
    nav_key_held_since: Option<Instant>,
    nav_key_last_scrolled_at: Option<Instant>,
    text_edit: Option<TextEditState>,
    save_error: Option<Arc<str>>,
    menu_lr_chord: screen_input::MenuLrChordTracker,
    menu_lr_undo: i8,
}

// One-shot transport for the profile id from the launcher screen
// (Manage Local Profiles) into our `init`.  Mirrors the pattern used by a
// few other screens that take a parameter on enter without making the
// `Screen` enum carry payload.
static PENDING_PROFILE_ID: Mutex<Option<String>> = Mutex::new(None);

pub fn set_pending_profile_id(id: String) {
    let mut slot = PENDING_PROFILE_ID.lock().expect("pending edit-profile id");
    *slot = Some(id);
}

fn take_pending_profile_id() -> Option<String> {
    let mut slot = PENDING_PROFILE_ID.lock().expect("pending edit-profile id");
    slot.take()
}

pub fn init() -> State {
    let profile_id = take_pending_profile_id().unwrap_or_default();
    let initial_credentials = if profile_id.is_empty() {
        ProfileCredentials::default()
    } else {
        profile::read_local_profile_credentials(&profile_id).unwrap_or_default()
    };
    let draft = initial_credentials.clone();
    State {
        active_color_index: color::DEFAULT_COLOR_INDEX,
        bg: visual_style_bg::State::new(),
        profile_id,
        initial_credentials,
        draft,
        show_secrets: false,
        selected_row: 0,
        nav_key_held_direction: None,
        nav_key_held_since: None,
        nav_key_last_scrolled_at: None,
        text_edit: None,
        save_error: None,
        menu_lr_chord: screen_input::MenuLrChordTracker::default(),
        menu_lr_undo: 0,
    }
}

fn current_row(state: &State) -> RowId {
    let idx = state.selected_row.min(ROW_ORDER.len() - 1);
    ROW_ORDER[idx]
}

fn row_value_for(state: &State, id: RowId) -> String {
    match id {
        RowId::DisplayName => state.draft.display_name.clone(),
        RowId::GsApiKey => render_secret(&state.draft.gs_api_key, state.show_secrets),
        RowId::GsUsername => state.draft.gs_username.clone(),
        RowId::GsIsPadPlayer => yes_no_label(state.draft.gs_is_pad_player),
        RowId::AcApiKey => render_secret(&state.draft.ac_api_key, state.show_secrets),
        RowId::ShowSecrets => yes_no_label(state.show_secrets),
        RowId::Save => String::new(),
        RowId::Cancel => String::new(),
    }
}

fn render_secret(value: &str, show: bool) -> String {
    if value.is_empty() {
        return String::new();
    }
    if show {
        return value.to_string();
    }
    let last4: String = value.chars().rev().take(4).collect::<String>().chars().rev().collect();
    if value.chars().count() <= 4 {
        // Short keys: show entirely (still partially obscured cases below).
        return value.chars().map(|_| '*').collect();
    }
    format!("****{last4}")
}

fn yes_no_label(value: bool) -> String {
    if value {
        tr("Common", "Yes").to_string()
    } else {
        tr("Common", "No").to_string()
    }
}

fn move_selected(state: &mut State, dir: NavDirection) {
    let total = ROW_ORDER.len();
    let last = total - 1;
    state.selected_row = match dir {
        NavDirection::Up => {
            if state.selected_row == 0 {
                last
            } else {
                state.selected_row - 1
            }
        }
        NavDirection::Down => {
            if state.selected_row >= last {
                0
            } else {
                state.selected_row + 1
            }
        }
    };
    state.save_error = None;
}

fn on_nav_press(state: &mut State, dir: NavDirection) {
    state.nav_key_held_direction = Some(dir);
    state.nav_key_held_since = Some(Instant::now());
    state.nav_key_last_scrolled_at = Some(Instant::now());
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

fn toggle_current_row(state: &mut State) {
    match current_row(state) {
        RowId::GsIsPadPlayer => {
            state.draft.gs_is_pad_player = !state.draft.gs_is_pad_player;
        }
        RowId::ShowSecrets => {
            state.show_secrets = !state.show_secrets;
        }
        _ => return,
    }
    audio::play_sfx("assets/sounds/change.ogg");
}

fn begin_text_edit(state: &mut State, target: RowId) {
    if !is_text_row(target) {
        return;
    }
    let value = match target {
        RowId::DisplayName => state.draft.display_name.clone(),
        RowId::GsApiKey => state.draft.gs_api_key.clone(),
        RowId::GsUsername => state.draft.gs_username.clone(),
        RowId::AcApiKey => state.draft.ac_api_key.clone(),
        _ => String::new(),
    };
    state.text_edit = Some(TextEditState {
        target,
        label: row_label(target),
        value,
        masked: is_secret_row(target) && !state.show_secrets,
        error: None,
        blink_t: 0.0,
    });
}

fn commit_text_edit(state: &mut State) {
    let Some(entry) = state.text_edit.take() else {
        return;
    };
    let trimmed = entry.value.trim();
    if matches!(entry.target, RowId::DisplayName) && trimmed.is_empty() {
        // Reopen with error.
        state.text_edit = Some(TextEditState {
            error: Some(tr("EditProfile", "DisplayNameEmpty")),
            ..entry
        });
        return;
    }
    match entry.target {
        RowId::DisplayName => state.draft.display_name = trimmed.to_string(),
        RowId::GsApiKey => state.draft.gs_api_key = trimmed.to_string(),
        RowId::GsUsername => state.draft.gs_username = trimmed.to_string(),
        RowId::AcApiKey => state.draft.ac_api_key = trimmed.to_string(),
        _ => {}
    }
    audio::play_sfx("assets/sounds/start.ogg");
}

fn cancel_text_edit(state: &mut State) {
    state.text_edit = None;
    audio::play_sfx("assets/sounds/change.ogg");
}

fn try_save(state: &mut State) -> ScreenAction {
    state.save_error = None;
    if state.profile_id.is_empty() {
        state.save_error = Some(tr("EditProfile", "SaveFailed"));
        return ScreenAction::None;
    }
    if state.draft.display_name.trim().is_empty() {
        state.save_error = Some(tr("EditProfile", "DisplayNameEmpty"));
        state.selected_row = ROW_ORDER
            .iter()
            .position(|r| matches!(r, RowId::DisplayName))
            .unwrap_or(0);
        return ScreenAction::None;
    }
    match profile::write_local_profile_credentials(&state.profile_id, &state.draft) {
        Ok(()) => {
            audio::play_sfx("assets/sounds/start.ogg");
            ScreenAction::Navigate(Screen::ManageLocalProfiles)
        }
        Err(_) => {
            state.save_error = Some(tr("EditProfile", "SaveFailed"));
            ScreenAction::None
        }
    }
}

fn cancel_screen(state: &mut State) -> ScreenAction {
    audio::play_sfx("assets/sounds/change.ogg");
    state.save_error = None;
    ScreenAction::Navigate(Screen::ManageLocalProfiles)
}

fn activate_selected_row(state: &mut State) -> ScreenAction {
    let row = current_row(state);
    match row {
        RowId::Save => try_save(state),
        RowId::Cancel => cancel_screen(state),
        RowId::GsIsPadPlayer | RowId::ShowSecrets => {
            toggle_current_row(state);
            ScreenAction::None
        }
        _ => {
            begin_text_edit(state, row);
            audio::play_sfx("assets/sounds/start.ogg");
            ScreenAction::None
        }
    }
}

pub fn update(state: &mut State, dt: f32) {
    if let Some(entry) = state.text_edit.as_mut() {
        entry.blink_t = (entry.blink_t + dt) % 1.0;
    }
    if let (Some(direction), Some(held_since), Some(last_scrolled_at)) = (
        state.nav_key_held_direction,
        state.nav_key_held_since,
        state.nav_key_last_scrolled_at,
    ) {
        let now = Instant::now();
        if now.duration_since(held_since) > NAV_INITIAL_HOLD_DELAY
            && now.duration_since(last_scrolled_at) >= NAV_REPEAT_SCROLL_INTERVAL
        {
            move_selected(state, direction);
            state.nav_key_last_scrolled_at = Some(now);
        }
    }
}

pub fn handle_input(state: &mut State, ev: &InputEvent) -> ScreenAction {
    let three_key_action = screen_input::three_key_menu_action(&mut state.menu_lr_chord, ev);

    if state.text_edit.is_some() {
        // Only confirm/cancel on virtual actions; raw typing comes via
        // handle_raw_key_event.
        if let Some((_, nav)) = three_key_action {
            match nav {
                screen_input::ThreeKeyMenuAction::Confirm => commit_text_edit(state),
                screen_input::ThreeKeyMenuAction::Cancel => cancel_text_edit(state),
                _ => {}
            }
            return ScreenAction::None;
        }
        if !ev.pressed {
            return ScreenAction::None;
        }
        match ev.action {
            VirtualAction::p1_start | VirtualAction::p2_start => commit_text_edit(state),
            VirtualAction::p1_back | VirtualAction::p2_back => cancel_text_edit(state),
            _ => {}
        }
        return ScreenAction::None;
    }

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
            return match nav {
                screen_input::ThreeKeyMenuAction::Prev => {
                    move_selected(state, NavDirection::Up);
                    on_nav_press(state, NavDirection::Up);
                    state.menu_lr_undo = 1;
                    audio::play_sfx("assets/sounds/change.ogg");
                    ScreenAction::None
                }
                screen_input::ThreeKeyMenuAction::Next => {
                    move_selected(state, NavDirection::Down);
                    on_nav_press(state, NavDirection::Down);
                    state.menu_lr_undo = -1;
                    audio::play_sfx("assets/sounds/change.ogg");
                    ScreenAction::None
                }
                screen_input::ThreeKeyMenuAction::Confirm => {
                    state.menu_lr_undo = 0;
                    activate_selected_row(state)
                }
                screen_input::ThreeKeyMenuAction::Cancel => cancel_screen(state),
            };
        }
    }

    match ev.action {
        VirtualAction::p1_back | VirtualAction::p2_back if ev.pressed => {
            return cancel_screen(state);
        }
        VirtualAction::p1_up
        | VirtualAction::p1_menu_up
        | VirtualAction::p2_up
        | VirtualAction::p2_menu_up => {
            if ev.pressed {
                move_selected(state, NavDirection::Up);
                on_nav_press(state, NavDirection::Up);
                audio::play_sfx("assets/sounds/change.ogg");
            } else {
                on_nav_release(state, NavDirection::Up);
            }
        }
        VirtualAction::p1_down
        | VirtualAction::p1_menu_down
        | VirtualAction::p2_down
        | VirtualAction::p2_menu_down => {
            if ev.pressed {
                move_selected(state, NavDirection::Down);
                on_nav_press(state, NavDirection::Down);
                audio::play_sfx("assets/sounds/change.ogg");
            } else {
                on_nav_release(state, NavDirection::Down);
            }
        }
        VirtualAction::p1_left
        | VirtualAction::p1_menu_left
        | VirtualAction::p2_left
        | VirtualAction::p2_menu_left
            if ev.pressed =>
        {
            if is_toggle_row(current_row(state)) {
                toggle_current_row(state);
            }
        }
        VirtualAction::p1_right
        | VirtualAction::p1_menu_right
        | VirtualAction::p2_right
        | VirtualAction::p2_menu_right
            if ev.pressed =>
        {
            if is_toggle_row(current_row(state)) {
                toggle_current_row(state);
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
    let Some(entry) = state.text_edit.as_mut() else {
        return ScreenAction::None;
    };
    if let Some(key_event) = key_event {
        if !key_event.pressed {
            return ScreenAction::None;
        }
        match key_event.code {
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
    let max_len = field_max_len(entry.target);
    let mut len = entry.value.chars().count();
    for ch in text.chars() {
        if ch.is_control() {
            continue;
        }
        if len >= max_len {
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

const ROW_H: f32 = 40.0;
const ROW_GAP: f32 = 4.0;
const VALUE_INDENT: f32 = 220.0;
const FORM_TOP_OFFSET: f32 = 80.0;
const LABEL_LEFT_PAD: f32 = 60.0;

pub fn get_actors(state: &State, _asset_manager: &AssetManager) -> Vec<Actor> {
    let mut actors: Vec<Actor> = Vec::with_capacity(48);

    actors.extend(state.bg.build(visual_style_bg::Params {
        active_color_index: state.active_color_index,
        backdrop_rgba: [0.0, 0.0, 0.0, 1.0],
        alpha_mul: 1.0,
    }));

    let title = if state.initial_credentials.display_name.is_empty() {
        tr("EditProfile", "Title").to_string()
    } else {
        format!(
            "{}: {}",
            tr("EditProfile", "Title"),
            state.initial_credentials.display_name
        )
    };
    actors.push(screen_bar::build(screen_bar::ScreenBarParams {
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

    let sw = screen_width();
    let accent = color::simply_love_rgba(state.active_color_index);

    for (idx, &row_id) in ROW_ORDER.iter().enumerate() {
        let y = FORM_TOP_OFFSET + (idx as f32) * (ROW_H + ROW_GAP);
        let is_selected = idx == state.selected_row;
        let row_bg = if is_selected {
            [accent[0] * 0.35, accent[1] * 0.35, accent[2] * 0.35, 0.85]
        } else {
            [0.0, 0.0, 0.0, 0.55]
        };
        actors.push(act!(quad:
            align(0.0, 0.0):
            xy(LABEL_LEFT_PAD - 8.0, y):
            zoomto(sw - 2.0 * (LABEL_LEFT_PAD - 8.0), ROW_H):
            diffuse(row_bg[0], row_bg[1], row_bg[2], row_bg[3]):
            z(110)
        ));
        if is_selected {
            actors.push(act!(quad:
                align(0.0, 0.0):
                xy(LABEL_LEFT_PAD - 8.0, y):
                zoomto(4.0, ROW_H):
                diffuse(accent[0], accent[1], accent[2], 1.0):
                z(111)
            ));
        }
        let label = row_label(row_id);
        actors.push(act!(text:
            font("miso"):
            settext(label):
            align(0.0, 0.5):
            xy(LABEL_LEFT_PAD, y + ROW_H * 0.5):
            zoom(0.95):
            horizalign(left):
            z(112)
        ));
        let value_text = row_value_for(state, row_id);
        if !value_text.is_empty() {
            actors.push(act!(text:
                font("miso"):
                settext(value_text):
                align(0.0, 0.5):
                xy(LABEL_LEFT_PAD + VALUE_INDENT, y + ROW_H * 0.5):
                zoom(0.95):
                maxwidth(sw - LABEL_LEFT_PAD - VALUE_INDENT - LABEL_LEFT_PAD):
                horizalign(left):
                z(112)
            ));
        }
    }

    if let Some(err) = &state.save_error {
        let err_y =
            FORM_TOP_OFFSET + (ROW_ORDER.len() as f32) * (ROW_H + ROW_GAP) + 12.0;
        actors.push(act!(text:
            font("miso"):
            settext(err.clone()):
            align(0.5, 0.0):
            xy(sw * 0.5, err_y):
            zoom(0.85):
            maxwidth(sw - 2.0 * LABEL_LEFT_PAD):
            horizalign(center):
            diffuse(1.0, 0.4, 0.4, 1.0):
            z(112)
        ));
    }

    let hint_text = tr("EditProfile", "Hint");
    actors.push(act!(text:
        font("miso"):
        settext(hint_text):
        align(0.5, 1.0):
        xy(sw * 0.5, screen_height() - 16.0):
        zoom(0.8):
        maxwidth(sw - 2.0 * LABEL_LEFT_PAD):
        horizalign(center):
        z(110)
    ));

    if state.text_edit.is_some() {
        push_text_edit_overlay(&mut actors, state);
    }

    actors
}

fn push_text_edit_overlay(actors: &mut Vec<Actor>, state: &State) {
    let Some(entry) = state.text_edit.as_ref() else {
        return;
    };
    let sw = screen_width();
    let sh = screen_height();
    let panel_w = (sw * 0.7).clamp(520.0, 1080.0);
    let panel_h = 220.0;
    let cx = sw * 0.5;
    let cy = sh * 0.5;
    let accent = color::simply_love_rgba(state.active_color_index);

    actors.push(act!(quad:
        align(0.0, 0.0):
        xy(0.0, 0.0):
        zoomto(sw, sh):
        diffuse(0.0, 0.0, 0.0, 0.7):
        z(310)
    ));
    actors.push(act!(quad:
        align(0.5, 0.5):
        xy(cx, cy):
        zoomto(panel_w + 6.0, panel_h + 6.0):
        diffuse(1.0, 1.0, 1.0, 1.0):
        z(311)
    ));
    actors.push(act!(quad:
        align(0.5, 0.5):
        xy(cx, cy):
        zoomto(panel_w, panel_h):
        diffuse(0.04, 0.06, 0.09, 0.97):
        z(312)
    ));
    actors.push(act!(text:
        font("miso"):
        settext(entry.label.clone()):
        align(0.5, 0.0):
        xy(cx, cy - panel_h * 0.5 + 16.0):
        zoom(1.0):
        horizalign(center):
        diffuse(accent[0], accent[1], accent[2], 1.0):
        z(313)
    ));

    let display = if entry.masked {
        render_secret(&entry.value, false)
    } else {
        entry.value.clone()
    };
    let caret = if entry.blink_t < 0.5 { "_" } else { " " };
    let value_with_caret = format!("{display}{caret}");
    actors.push(act!(text:
        font("miso"):
        settext(value_with_caret):
        align(0.5, 0.5):
        xy(cx, cy - 8.0):
        zoom(1.05):
        maxwidth(panel_w - 30.0):
        horizalign(center):
        z(313)
    ));

    if let Some(err) = &entry.error {
        actors.push(act!(text:
            font("miso"):
            settext(err.clone()):
            align(0.5, 0.5):
            xy(cx, cy + 28.0):
            zoom(0.85):
            maxwidth(panel_w - 30.0):
            horizalign(center):
            diffuse(1.0, 0.4, 0.4, 1.0):
            z(313)
        ));
    }

    let hint = tr("EditProfile", "TextEditHint");
    actors.push(act!(text:
        font("miso"):
        settext(hint):
        align(0.5, 1.0):
        xy(cx, cy + panel_h * 0.5 - 14.0):
        zoom(0.8):
        maxwidth(panel_w - 30.0):
        horizalign(center):
        z(313)
    ));
}

#[inline(always)]
pub fn reset_nav_holds(state: &mut State) {
    reset_nav_hold(state);
}
