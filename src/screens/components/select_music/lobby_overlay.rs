use crate::act;
use crate::assets::{FontRole, current_theme_font_key};
use crate::engine::audio;
use crate::engine::input::{InputEvent, RawKeyboardEvent, VirtualAction};
use crate::engine::present::actors::Actor;
use crate::engine::present::color;
use crate::engine::space::{screen_center_x, screen_center_y, screen_height, screen_width};
use crate::game::online::lobbies;
use std::time::{Duration, Instant};

const DIM_ALPHA: f32 = 0.875;
const OVERLAY_Z: i16 = 1480;
const PANEL_W: f32 = 760.0;
const PANEL_H: f32 = 430.0;
const LOBBY_LIST_X: f32 = -160.0;
const LOBBY_LIST_Y: f32 = -110.0;
const LOBBY_ROW_W: f32 = 360.0;
const LOBBY_ROW_H: f32 = 50.0;
const LOBBY_ROW_STEP: f32 = 58.0;
const VISIBLE_LOBBY_ROWS: usize = 5;
const ACTION_PANEL_X: f32 = 220.0;
const ACTION_BUTTON_W: f32 = 180.0;
const ACTION_BUTTON_H: f32 = 42.0;
const ACTION_BUTTON_STEP: f32 = 62.0;
const STATUS_Y: f32 = -172.0;
const TITLE_Y: f32 = -192.0;
const FOOTER_Y: f32 = 188.0;
const PASSWORD_PROMPT_W: f32 = 560.0;
const PASSWORD_PROMPT_H: f32 = 210.0;
const PASSWORD_PROMPT_VALUE_W: f32 = 430.0;
const PASSWORD_PROMPT_VALUE_H: f32 = 40.0;
const PASSWORD_PROMPT_TITLE_Y: f32 = -72.0;
const PASSWORD_PROMPT_HINT_Y: f32 = -46.0;
const PASSWORD_PROMPT_VALUE_Y: f32 = -14.0;
const PASSWORD_PROMPT_WHEEL_Y: f32 = 38.0;
const PASSWORD_PROMPT_FOOTER_Y: f32 = 90.0;
const PASSWORD_PROMPT_WHEEL_X_OFF: f32 = 52.0;
const PASSWORD_WHEEL_CHAR_WIDTH: f32 = 52.0;
const PASSWORD_WHEEL_NUM_ITEMS: usize = 7;
const PASSWORD_WHEEL_FOCUS_POS: usize = 3;
const PASSWORD_WHEEL_SLIDE_SECONDS: f32 = 0.075;
const PASSWORD_PROMPT_MAX_LEN: usize = lobbies::LOBBY_PASSWORD_MAX_LEN;
const NAV_INITIAL_HOLD_DELAY: Duration = Duration::from_millis(250);
const NAV_REPEAT_SCROLL_INTERVAL: Duration = Duration::from_nanos(66_666_667);
const PASSWORD_CHARS: [&str; 28] = [
    "&BACK;", "&OK;", "A", "B", "C", "D", "E", "F", "G", "H", "I", "J", "K", "L", "M", "N", "O",
    "P", "Q", "R", "S", "T", "U", "V", "W", "X", "Y", "Z",
];

#[derive(Clone, Copy, Debug)]
struct WheelItem {
    info_index: usize,
    x: f32,
    x0: f32,
    x1: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum NavDirection {
    Left,
    Right,
}

#[derive(Clone, Debug)]
struct PasswordWheel {
    info_pos: i32,
    anim_elapsed: Option<f32>,
    items: [WheelItem; PASSWORD_WHEEL_NUM_ITEMS],
}

#[inline(always)]
fn wrap_info_index(info_pos: i32, slot_index1: usize, len: usize) -> usize {
    let len_i = len as i32;
    let idx1 = (info_pos - 1 + slot_index1 as i32).rem_euclid(len_i) + 1;
    (idx1 - 1) as usize
}

#[inline(always)]
fn slot_x(slot_index1: usize) -> f32 {
    let center = (PASSWORD_WHEEL_NUM_ITEMS as f32 / 2.0).ceil();
    PASSWORD_WHEEL_CHAR_WIDTH * (slot_index1 as f32 - center)
}

impl PasswordWheel {
    fn new(starting_char_index1: i32) -> Self {
        let len = PASSWORD_CHARS.len();
        let start_pos = starting_char_index1 - PASSWORD_WHEEL_FOCUS_POS as i32;

        Self {
            info_pos: start_pos,
            anim_elapsed: None,
            items: std::array::from_fn(|i| {
                let slot = i + 1;
                let x = slot_x(slot);
                WheelItem {
                    info_index: wrap_info_index(start_pos, slot, len),
                    x,
                    x0: x,
                    x1: x,
                }
            }),
        }
    }

    #[inline(always)]
    fn focused_info_index(&self) -> usize {
        self.items[PASSWORD_WHEEL_FOCUS_POS - 1].info_index
    }

    fn finish_tweens(&mut self) {
        let Some(t) = self.anim_elapsed else {
            return;
        };
        let p = (t / PASSWORD_WHEEL_SLIDE_SECONDS).clamp(0.0, 1.0);
        for it in &mut self.items {
            it.x = it.x0 + (it.x1 - it.x0) * p;
            it.x0 = it.x;
        }
        self.anim_elapsed = None;
    }

    fn start_tween_to_slots(&mut self) {
        for (i, it) in self.items.iter_mut().enumerate() {
            let slot = i + 1;
            it.x0 = it.x;
            it.x1 = slot_x(slot);
        }
        self.anim_elapsed = Some(0.0);
    }

    fn sync_info_indices(&mut self) {
        let len = PASSWORD_CHARS.len();
        for (i, it) in self.items.iter_mut().enumerate() {
            let slot = i + 1;
            it.info_index = wrap_info_index(self.info_pos, slot, len);
        }
    }

    fn scroll_by(&mut self, dir: i32) {
        if dir == 0 || PASSWORD_CHARS.is_empty() {
            return;
        }

        self.finish_tweens();
        self.info_pos = self.info_pos.saturating_add(dir);
        if dir > 0 {
            self.items.rotate_left(dir as usize);
        } else {
            self.items.rotate_right((-dir) as usize);
        }
        self.sync_info_indices();

        if dir < 0 {
            let spawn_x = slot_x(1) - PASSWORD_WHEEL_CHAR_WIDTH;
            let it = &mut self.items[0];
            it.x = spawn_x;
            it.x0 = spawn_x;
            it.x1 = spawn_x;
        }
        self.start_tween_to_slots();
    }

    fn scroll_to_pos(&mut self, focused_char_index1: i32) {
        if PASSWORD_CHARS.is_empty() {
            return;
        }

        let start_pos = focused_char_index1 - PASSWORD_WHEEL_FOCUS_POS as i32;
        let shift_amount = start_pos - self.info_pos;
        if shift_amount == 0 {
            return;
        }

        self.finish_tweens();
        self.info_pos = start_pos;

        if shift_amount.abs() < PASSWORD_WHEEL_NUM_ITEMS as i32 {
            if shift_amount > 0 {
                self.items.rotate_left(shift_amount as usize);
            } else {
                self.items.rotate_right((-shift_amount) as usize);
            }
        }

        self.sync_info_indices();
        self.start_tween_to_slots();
    }

    fn update(&mut self, dt: f32) {
        let Some(t) = &mut self.anim_elapsed else {
            return;
        };
        *t = (*t + dt).max(0.0);
        let p = (*t / PASSWORD_WHEEL_SLIDE_SECONDS).clamp(0.0, 1.0);
        for it in &mut self.items {
            it.x = it.x0 + (it.x1 - it.x0) * p;
        }
        if p >= 1.0 {
            for it in &mut self.items {
                it.x = it.x1;
                it.x0 = it.x;
            }
            self.anim_elapsed = None;
        }
    }
}

#[derive(Clone, Debug)]
enum PasswordPromptMode {
    CreateLobby,
    JoinLobby { code: String },
}

#[derive(Clone, Debug)]
struct PasswordPromptState {
    mode: PasswordPromptMode,
    value: String,
    wheel: PasswordWheel,
    nav_key_held_direction: Option<NavDirection>,
    nav_key_held_since: Option<Instant>,
    nav_key_last_scrolled_at: Option<Instant>,
}

#[derive(Clone, Debug)]
pub struct OverlayStateData {
    browse_index: usize,
    browse_scroll: usize,
    joined_action_index: usize,
    password_prompt: Option<PasswordPromptState>,
    notice_text: Option<String>,
    notice_time_left: f32,
}

#[derive(Clone, Debug)]
pub enum OverlayState {
    Hidden,
    Visible(OverlayStateData),
}

#[derive(Clone, Debug)]
pub enum InputOutcome {
    None,
    ChangedSelection,
    Closed,
    ConnectRequested,
    SearchRequested,
    CreateRequested(String),
    JoinRequested { code: String, password: String },
    LeaveRequested,
}

#[inline(always)]
pub fn show_overlay() -> OverlayState {
    OverlayState::Visible(OverlayStateData {
        browse_index: 0,
        browse_scroll: 0,
        joined_action_index: 0,
        password_prompt: None,
        notice_text: None,
        notice_time_left: 0.0,
    })
}

#[inline(always)]
pub fn hide_overlay(state: &mut OverlayState) {
    *state = OverlayState::Hidden;
}

pub fn update_overlay(state: &mut OverlayState, dt: f32) {
    let OverlayState::Visible(overlay) = state else {
        return;
    };

    let snapshot = lobbies::snapshot();
    match snapshot.joined_lobby.as_ref() {
        Some(_) => {
            overlay.joined_action_index = overlay.joined_action_index.min(1);
            overlay.password_prompt = None;
        }
        None => {
            let len = browse_item_count(&snapshot).saturating_sub(1);
            overlay.browse_index = overlay.browse_index.min(len);
            clamp_browse_scroll(overlay, &snapshot);
        }
    }

    if let Some(prompt) = overlay.password_prompt.as_mut() {
        update_password_prompt_hold(prompt);
        prompt.wheel.update(dt);
    }

    if overlay.notice_time_left > 0.0 {
        overlay.notice_time_left = (overlay.notice_time_left - dt.max(0.0)).max(0.0);
        if overlay.notice_time_left <= 0.0 {
            overlay.notice_text = None;
        }
    }
}

pub fn handle_input(state: &mut OverlayState, ev: &InputEvent) -> InputOutcome {
    let OverlayState::Visible(overlay) = state else {
        return InputOutcome::None;
    };

    if overlay.password_prompt.is_some() {
        return handle_password_prompt_input(overlay, ev);
    }
    if !ev.pressed {
        return InputOutcome::None;
    }

    let snapshot = lobbies::snapshot();
    if snapshot.joined_lobby.is_some() {
        match ev.action {
            VirtualAction::p1_left
            | VirtualAction::p1_up
            | VirtualAction::p1_menu_left
            | VirtualAction::p1_menu_up
            | VirtualAction::p2_left
            | VirtualAction::p2_up
            | VirtualAction::p2_menu_left
            | VirtualAction::p2_menu_up => {
                let next = overlay.joined_action_index.saturating_sub(1);
                if next != overlay.joined_action_index {
                    overlay.joined_action_index = next;
                    return InputOutcome::ChangedSelection;
                }
            }
            VirtualAction::p1_right
            | VirtualAction::p1_down
            | VirtualAction::p1_menu_right
            | VirtualAction::p1_menu_down
            | VirtualAction::p2_right
            | VirtualAction::p2_down
            | VirtualAction::p2_menu_right
            | VirtualAction::p2_menu_down => {
                let next = (overlay.joined_action_index + 1).min(1);
                if next != overlay.joined_action_index {
                    overlay.joined_action_index = next;
                    return InputOutcome::ChangedSelection;
                }
            }
            VirtualAction::p1_start | VirtualAction::p2_start => {
                return if overlay.joined_action_index == 0 {
                    InputOutcome::LeaveRequested
                } else {
                    hide_overlay(state);
                    InputOutcome::Closed
                };
            }
            VirtualAction::p1_back
            | VirtualAction::p2_back
            | VirtualAction::p1_select
            | VirtualAction::p2_select => {
                hide_overlay(state);
                return InputOutcome::Closed;
            }
            _ => {}
        }
        return InputOutcome::None;
    }
    match snapshot.connection {
        lobbies::ConnectionState::Disconnected | lobbies::ConnectionState::Error(_) => {
            return match ev.action {
                VirtualAction::p1_start | VirtualAction::p2_start => InputOutcome::ConnectRequested,
                VirtualAction::p1_back
                | VirtualAction::p2_back
                | VirtualAction::p1_select
                | VirtualAction::p2_select => {
                    hide_overlay(state);
                    InputOutcome::Closed
                }
                _ => InputOutcome::None,
            };
        }
        lobbies::ConnectionState::Connecting => {
            return match ev.action {
                VirtualAction::p1_back
                | VirtualAction::p2_back
                | VirtualAction::p1_select
                | VirtualAction::p2_select => {
                    hide_overlay(state);
                    InputOutcome::Closed
                }
                _ => InputOutcome::None,
            };
        }
        lobbies::ConnectionState::Connected => {}
    }

    match ev.action {
        VirtualAction::p1_left
        | VirtualAction::p1_up
        | VirtualAction::p1_menu_left
        | VirtualAction::p1_menu_up
        | VirtualAction::p2_left
        | VirtualAction::p2_up
        | VirtualAction::p2_menu_left
        | VirtualAction::p2_menu_up => {
            let next = overlay.browse_index.saturating_sub(1);
            if next != overlay.browse_index {
                overlay.browse_index = next;
                clear_notice(overlay);
                clamp_browse_scroll(overlay, &snapshot);
                return InputOutcome::ChangedSelection;
            }
        }
        VirtualAction::p1_right
        | VirtualAction::p1_down
        | VirtualAction::p1_menu_right
        | VirtualAction::p1_menu_down
        | VirtualAction::p2_right
        | VirtualAction::p2_down
        | VirtualAction::p2_menu_right
        | VirtualAction::p2_menu_down => {
            let limit = browse_item_count(&snapshot).saturating_sub(1);
            let next = (overlay.browse_index + 1).min(limit);
            if next != overlay.browse_index {
                overlay.browse_index = next;
                clear_notice(overlay);
                clamp_browse_scroll(overlay, &snapshot);
                return InputOutcome::ChangedSelection;
            }
        }
        VirtualAction::p1_start | VirtualAction::p2_start => {
            clear_notice(overlay);
            return match resolve_browse_action(&snapshot, overlay.browse_index) {
                BrowseAction::Lobby(lobby) => {
                    if lobby.is_password_protected {
                        overlay.password_prompt =
                            Some(begin_password_prompt_join(lobby.code.as_str()));
                        InputOutcome::None
                    } else {
                        InputOutcome::JoinRequested {
                            code: lobby.code.clone(),
                            password: String::new(),
                        }
                    }
                }
                BrowseAction::Refresh => InputOutcome::SearchRequested,
                BrowseAction::Create => {
                    overlay.password_prompt = Some(begin_password_prompt_create());
                    InputOutcome::None
                }
                BrowseAction::Close => {
                    hide_overlay(state);
                    InputOutcome::Closed
                }
            };
        }
        VirtualAction::p1_back
        | VirtualAction::p2_back
        | VirtualAction::p1_select
        | VirtualAction::p2_select => {
            hide_overlay(state);
            return InputOutcome::Closed;
        }
        _ => {}
    }

    InputOutcome::None
}

pub fn handle_raw_key(
    state: &mut OverlayState,
    _key: Option<&RawKeyboardEvent>,
    _text: Option<&str>,
) -> InputOutcome {
    let OverlayState::Visible(overlay) = state else {
        return InputOutcome::None;
    };
    if overlay.password_prompt.is_none() {
        return InputOutcome::None;
    }

    InputOutcome::None
}

pub fn build_overlay(state: &OverlayState, active_color_index: i32) -> Option<Vec<Actor>> {
    let OverlayState::Visible(overlay) = state else {
        return None;
    };

    let snapshot = lobbies::snapshot();
    let center_x = screen_center_x();
    let center_y = screen_center_y();
    let fill = color::decorative_rgba(active_color_index);
    let select_color = color::simply_love_rgba(active_color_index);
    let mut actors = Vec::new();

    actors.push(act!(quad:
        align(0.0, 0.0): xy(0.0, 0.0):
        zoomto(screen_width(), screen_height()):
        diffuse(0.0, 0.0, 0.0, DIM_ALPHA):
        z(OVERLAY_Z)
    ));
    actors.push(act!(quad:
        align(0.5, 0.5): xy(center_x, center_y):
        zoomto(PANEL_W + 2.0, PANEL_H + 2.0):
        diffuse(1.0, 1.0, 1.0, 1.0):
        z(OVERLAY_Z + 1)
    ));
    actors.push(act!(quad:
        align(0.5, 0.5): xy(center_x, center_y):
        zoomto(PANEL_W, PANEL_H):
        diffuse(0.0, 0.0, 0.0, 0.96):
        z(OVERLAY_Z + 2)
    ));
    actors.push(act!(text:
        font(current_theme_font_key(FontRole::Header)):
        settext("Online Lobbies"):
        align(0.5, 0.5):
        xy(center_x, center_y + TITLE_Y):
        zoom(0.6):
        diffuse(1.0, 1.0, 1.0, 1.0):
        z(OVERLAY_Z + 3)
    ));
    actors.push(act!(text:
        font("miso"):
        settext(status_text(overlay, &snapshot)):
        align(0.5, 0.5):
        xy(center_x, center_y + STATUS_Y):
        zoom(0.8):
        diffuse(1.0, 1.0, 1.0, 1.0):
        z(OVERLAY_Z + 3):
        horizalign(center)
    ));
    actors.push(act!(text:
        font("miso"):
        settext(close_hint(overlay, &snapshot)):
        align(0.5, 0.5):
        xy(center_x, center_y + FOOTER_Y):
        zoom(0.9):
        diffuse(1.0, 1.0, 1.0, 1.0):
        z(OVERLAY_Z + 3):
        horizalign(center)
    ));

    if let Some(joined) = snapshot.joined_lobby.as_ref() {
        actors.extend(build_joined_overlay(
            center_x,
            center_y,
            joined,
            overlay.joined_action_index,
            &select_color,
        ));
        return Some(actors);
    }

    match snapshot.connection {
        lobbies::ConnectionState::Disconnected | lobbies::ConnectionState::Error(_) => {
            actors.push(act!(text:
                font("miso"):
                settext("Press &START; to connect."):
                align(0.5, 0.5):
                xy(center_x, center_y):
                zoom(1.2):
                diffuse(fill[0], fill[1], fill[2], 1.0):
                z(OVERLAY_Z + 3):
                horizalign(center)
            ));
            return Some(actors);
        }
        lobbies::ConnectionState::Connecting => {
            actors.push(act!(text:
                font("miso"):
                settext("Connecting..."):
                align(0.5, 0.5):
                xy(center_x, center_y):
                zoom(1.2):
                diffuse(fill[0], fill[1], fill[2], 1.0):
                z(OVERLAY_Z + 3):
                horizalign(center)
            ));
            return Some(actors);
        }
        lobbies::ConnectionState::Connected => {}
    }

    actors.extend(build_browse_overlay(
        center_x,
        center_y,
        overlay,
        &snapshot,
        &select_color,
    ));
    if let Some(prompt) = overlay.password_prompt.as_ref() {
        actors.extend(build_password_prompt(
            center_x,
            center_y,
            prompt,
            &select_color,
        ));
    }
    Some(actors)
}

fn build_browse_overlay(
    center_x: f32,
    center_y: f32,
    overlay: &OverlayStateData,
    snapshot: &lobbies::Snapshot,
    select_color: &[f32; 4],
) -> Vec<Actor> {
    let mut actors = Vec::new();

    actors.push(act!(text:
        font("miso"):
        settext("Available Lobbies"):
        align(0.0, 0.5):
        xy(center_x + LOBBY_LIST_X - 180.0, center_y - 146.0):
        zoom(0.95):
        diffuse(1.0, 1.0, 1.0, 1.0):
        z(OVERLAY_Z + 3):
        horizalign(left)
    ));

    let visible_lobbies = snapshot
        .available_lobbies
        .iter()
        .skip(overlay.browse_scroll)
        .take(VISIBLE_LOBBY_ROWS)
        .enumerate();
    for (slot, lobby) in visible_lobbies {
        let row_index = overlay.browse_scroll + slot;
        let selected = overlay.browse_index == row_index;
        let row_y = center_y + LOBBY_LIST_Y + slot as f32 * LOBBY_ROW_STEP;
        actors.extend(build_box_row(
            center_x + LOBBY_LIST_X,
            row_y,
            LOBBY_ROW_W,
            LOBBY_ROW_H,
            selected,
            select_color,
        ));
        actors.push(act!(text:
            font(current_theme_font_key(FontRole::Header)):
            settext(lobby.code.clone()):
            align(0.0, 0.5):
            xy(center_x + LOBBY_LIST_X - 164.0, row_y - 7.0):
            zoom(0.46):
            diffuse(1.0, 1.0, 1.0, 1.0):
            z(OVERLAY_Z + 4):
            horizalign(left)
        ));
        actors.push(act!(text:
            font("miso"):
            settext(format!(
                "{} player{}{}",
                lobby.player_count,
                if lobby.player_count == 1 { "" } else { "s" },
                if lobby.is_password_protected { "  LOCKED" } else { "" },
            )):
            align(0.0, 0.5):
            xy(center_x + LOBBY_LIST_X - 164.0, row_y + 12.0):
            zoom(0.82):
            diffuse(1.0, 1.0, 1.0, 1.0):
            z(OVERLAY_Z + 4):
            horizalign(left)
        ));
    }

    if snapshot.available_lobbies.is_empty() {
        actors.push(act!(text:
            font("miso"):
            settext("No lobbies found."):
            align(0.5, 0.5):
            xy(center_x + LOBBY_LIST_X, center_y - 10.0):
            zoom(1.05):
            diffuse(1.0, 1.0, 1.0, 1.0):
            z(OVERLAY_Z + 3):
            horizalign(center)
        ));
    } else {
        actors.push(act!(text:
            font("miso"):
            settext(format!(
                "{}/{}",
                overlay.browse_index.min(snapshot.available_lobbies.len().saturating_sub(1)) + 1,
                snapshot.available_lobbies.len(),
            )):
            align(1.0, 0.5):
            xy(center_x + LOBBY_LIST_X + 182.0, center_y - 146.0):
            zoom(0.85):
            diffuse(1.0, 1.0, 1.0, 1.0):
            z(OVERLAY_Z + 3):
            horizalign(right)
        ));
    }

    let actions = ["Refresh List", "Create Lobby", "Close"];
    for (slot, label) in actions.iter().enumerate() {
        let index = snapshot.available_lobbies.len() + slot;
        let selected = overlay.browse_index == index;
        let row_y = center_y - 66.0 + slot as f32 * ACTION_BUTTON_STEP;
        actors.extend(build_box_row(
            center_x + ACTION_PANEL_X,
            row_y,
            ACTION_BUTTON_W,
            ACTION_BUTTON_H,
            selected,
            select_color,
        ));
        actors.push(act!(text:
            font(current_theme_font_key(FontRole::Header)):
            settext(*label):
            align(0.5, 0.5):
            xy(center_x + ACTION_PANEL_X, row_y):
            zoom(0.46):
            diffuse(1.0, 1.0, 1.0, 1.0):
            z(OVERLAY_Z + 4):
            horizalign(center)
        ));
    }

    actors
}

fn build_joined_overlay(
    center_x: f32,
    center_y: f32,
    joined: &lobbies::JoinedLobby,
    selected_action_index: usize,
    select_color: &[f32; 4],
) -> Vec<Actor> {
    let mut actors = Vec::new();
    actors.push(act!(text:
        font("miso"):
        settext("Joined Lobby"):
        align(0.0, 0.5):
        xy(center_x - 300.0, center_y - 146.0):
        zoom(0.95):
        diffuse(1.0, 1.0, 1.0, 1.0):
        z(OVERLAY_Z + 3):
        horizalign(left)
    ));
    actors.push(act!(text:
        font(current_theme_font_key(FontRole::Header)):
        settext(format!("Lobby Code: {}", joined.code)):
        align(0.0, 0.5):
        xy(center_x - 300.0, center_y - 102.0):
        zoom(0.5):
        diffuse(1.0, 1.0, 1.0, 1.0):
        z(OVERLAY_Z + 3):
        horizalign(left)
    ));

    let mut players_y = center_y - 44.0;
    if let Some(song_info_text) = joined_song_info_text(joined) {
        actors.push(act!(text:
            font("miso"):
            settext(song_info_text):
            align(0.0, 0.0):
            xy(center_x - 300.0, center_y - 78.0):
            zoom(0.82):
            diffuse(1.0, 1.0, 1.0, 1.0):
            z(OVERLAY_Z + 3):
            horizalign(left)
        ));
        players_y = center_y + 4.0;
    }

    for (idx, player) in joined.players.iter().enumerate() {
        let row_y = players_y + idx as f32 * 32.0;
        let screen_suffix = lobby_player_screen_suffix(player);
        actors.push(act!(text:
            font("miso"):
            settext(format!(
                "{}. {}{}{}",
                idx + 1,
                player.label,
                if player.ready { "  ✔" } else { "" },
                screen_suffix,
            )):
            align(0.0, 0.5):
            xy(center_x - 300.0, row_y):
            zoom(0.92):
            diffuse(1.0, 1.0, 1.0, 1.0):
            z(OVERLAY_Z + 3):
            horizalign(left)
        ));
    }
    if joined.players.is_empty() {
        actors.push(act!(text:
            font("miso"):
            settext("Waiting for players..."):
            align(0.0, 0.5):
            xy(center_x - 300.0, players_y):
            zoom(0.92):
            diffuse(1.0, 1.0, 1.0, 1.0):
            z(OVERLAY_Z + 3):
            horizalign(left)
        ));
    }

    let actions = ["Leave Lobby", "Close"];
    for (slot, label) in actions.iter().enumerate() {
        let selected = selected_action_index == slot;
        let row_y = center_y - 24.0 + slot as f32 * ACTION_BUTTON_STEP;
        actors.extend(build_box_row(
            center_x + ACTION_PANEL_X,
            row_y,
            ACTION_BUTTON_W,
            ACTION_BUTTON_H,
            selected,
            select_color,
        ));
        actors.push(act!(text:
            font(current_theme_font_key(FontRole::Header)):
            settext(*label):
            align(0.5, 0.5):
            xy(center_x + ACTION_PANEL_X, row_y):
            zoom(0.46):
            diffuse(1.0, 1.0, 1.0, 1.0):
            z(OVERLAY_Z + 4):
            horizalign(center)
        ));
    }

    actors
}

fn joined_song_info_text(joined: &lobbies::JoinedLobby) -> Option<String> {
    let song_info = joined.song_info.as_ref()?;
    let title = song_info
        .title
        .as_deref()
        .unwrap_or(song_info.song_path.as_str());
    let title = truncate_text(title, 38);

    let mut detail = String::new();
    if let Some(chart_label) = song_info.chart_label.as_deref()
        && !chart_label.trim().is_empty()
    {
        detail.push_str(chart_label.trim());
    }
    if let Some(rate) = song_info
        .rate
        .filter(|rate| rate.is_finite() && *rate > 0.0)
    {
        if !detail.is_empty() {
            detail.push_str("  ");
        }
        detail.push_str(format!("{rate:.2}x").as_str());
    }

    if detail.is_empty() {
        Some(format!("Selected Song\n{title}"))
    } else {
        Some(format!("Selected Song\n{title}\n{detail}"))
    }
}

fn lobby_player_screen_suffix(player: &lobbies::LobbyPlayer) -> String {
    let screen = player.screen_name.trim();
    if screen.is_empty() || screen.eq_ignore_ascii_case("ScreenSelectMusic") {
        return String::new();
    }
    let screen = screen.strip_prefix("Screen").unwrap_or(screen);
    format!("  [{screen}]")
}

fn truncate_text(text: &str, max_chars: usize) -> String {
    let count = text.chars().count();
    if count <= max_chars {
        return text.to_string();
    }
    let keep = max_chars.saturating_sub(3);
    let mut out = String::with_capacity(max_chars);
    out.extend(text.chars().take(keep));
    out.push_str("...");
    out
}

fn build_password_prompt(
    center_x: f32,
    center_y: f32,
    prompt: &PasswordPromptState,
    select_color: &[f32; 4],
) -> Vec<Actor> {
    let mut actors = Vec::new();
    let title = password_prompt_title(prompt);
    let hint = password_prompt_hint(prompt);
    let value = password_prompt_value(prompt);

    actors.push(act!(quad:
        align(0.5, 0.5):
        xy(center_x, center_y):
        zoomto(PASSWORD_PROMPT_W + 2.0, PASSWORD_PROMPT_H + 2.0):
        diffuse(1.0, 1.0, 1.0, 1.0):
        z(OVERLAY_Z + 5)
    ));
    actors.push(act!(quad:
        align(0.5, 0.5):
        xy(center_x, center_y):
        zoomto(PASSWORD_PROMPT_W, PASSWORD_PROMPT_H):
        diffuse(0.08, 0.08, 0.08, 0.98):
        z(OVERLAY_Z + 6)
    ));
    actors.extend(build_box_row(
        center_x,
        center_y + PASSWORD_PROMPT_VALUE_Y,
        PASSWORD_PROMPT_VALUE_W,
        PASSWORD_PROMPT_VALUE_H,
        false,
        select_color,
    ));
    actors.push(act!(text:
        font("miso"):
        settext(title):
        align(0.5, 0.5):
        xy(center_x, center_y + PASSWORD_PROMPT_TITLE_Y):
        zoom(0.85):
        diffuse(1.0, 1.0, 1.0, 1.0):
        z(OVERLAY_Z + 7):
        horizalign(center)
    ));
    actors.push(act!(text:
        font("miso"):
        settext(hint):
        align(0.5, 0.5):
        xy(center_x, center_y + PASSWORD_PROMPT_HINT_Y):
        zoom(0.55):
        maxwidth(PASSWORD_PROMPT_W - 92.0):
        diffuse(0.8, 0.8, 0.8, 1.0):
        z(OVERLAY_Z + 7):
        horizalign(center)
    ));
    actors.push(act!(text:
        font(current_theme_font_key(FontRole::Header)):
        settext(value):
        align(0.5, 0.5):
        xy(center_x, center_y + PASSWORD_PROMPT_VALUE_Y):
        zoom(0.55):
        maxwidth(PASSWORD_PROMPT_VALUE_W - 40.0):
        diffuse(
            if prompt.value.is_empty() { 0.75 } else { 1.0 },
            if prompt.value.is_empty() { 0.75 } else { 1.0 },
            if prompt.value.is_empty() { 0.75 } else { 1.0 },
            1.0
        ):
        z(OVERLAY_Z + 7):
        horizalign(center)
    ));
    actors.push(act!(quad:
        align(0.5, 0.5):
        xy(center_x, center_y + PASSWORD_PROMPT_WHEEL_Y + 18.0):
        zoomto(48.0, 2.0):
        diffuse(select_color[0], select_color[1], select_color[2], 1.0):
        z(OVERLAY_Z + 7)
    ));
    for item_index in 1..=PASSWORD_WHEEL_NUM_ITEMS {
        let item = &prompt.wheel.items[item_index - 1];
        let visible = item_index < (PASSWORD_WHEEL_NUM_ITEMS - 1);
        let alpha = if visible { 1.0 } else { 0.0 };
        let shade = if item_index == PASSWORD_WHEEL_FOCUS_POS {
            1.0
        } else {
            0.3
        };
        actors.push(act!(text:
            font(current_theme_font_key(FontRole::Header)):
            settext(PASSWORD_CHARS[item.info_index]):
            align(0.5, 0.5):
            xy(
                center_x + PASSWORD_PROMPT_WHEEL_X_OFF + item.x,
                center_y + PASSWORD_PROMPT_WHEEL_Y
            ):
            zoom(0.5):
            diffuse(shade, shade, shade, alpha):
            z(OVERLAY_Z + 7):
            horizalign(center)
        ));
    }
    push_password_prompt_footer(&mut actors, center_x, center_y);
    actors
}

fn build_box_row(
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    selected: bool,
    select_color: &[f32; 4],
) -> Vec<Actor> {
    let border = if selected {
        [select_color[0], select_color[1], select_color[2], 1.0]
    } else {
        [1.0, 1.0, 1.0, 1.0]
    };
    vec![
        act!(quad:
            align(0.5, 0.5):
            xy(x, y):
            zoomto(width, height):
            diffuse(border[0], border[1], border[2], border[3]):
            z(OVERLAY_Z + 3)
        ),
        act!(quad:
            align(0.5, 0.5):
            xy(x, y):
            zoomto(width - 2.0, height - 2.0):
            diffuse(0.0, 0.0, 0.0, 1.0):
            z(OVERLAY_Z + 3)
        ),
    ]
}

fn push_password_prompt_footer(actors: &mut Vec<Actor>, center_x: f32, center_y: f32) {
    let y = center_y + PASSWORD_PROMPT_FOOTER_Y;
    for (icon_x, text_x, icon, label) in [
        (-110.0, -90.0, "&BACK;", "remove"),
        (18.0, 38.0, "&OK;", "confirm"),
    ] {
        actors.push(act!(text:
            font(current_theme_font_key(FontRole::Header)):
            settext(icon):
            align(0.5, 0.5):
            xy(center_x + icon_x, y):
            zoom(0.42):
            diffuse(0.75, 0.75, 0.75, 1.0):
            z(OVERLAY_Z + 7):
            horizalign(center)
        ));
        actors.push(act!(text:
            font("miso"):
            settext(label):
            align(0.0, 0.5):
            xy(center_x + text_x, y):
            zoom(0.6):
            diffuse(0.75, 0.75, 0.75, 1.0):
            z(OVERLAY_Z + 7):
            horizalign(left)
        ));
    }
}

fn close_hint(overlay: &OverlayStateData, snapshot: &lobbies::Snapshot) -> &'static str {
    if overlay.password_prompt.is_some() {
        return "&MENULEFT;/&MENURIGHT;: PICK    &START;: CHOOSE    &BACK;: CANCEL";
    }
    if snapshot.joined_lobby.is_some() {
        "START: SELECT ACTION    BACK/SELECT: CLOSE"
    } else {
        "ARROWS: MOVE    START: SELECT    BACK/SELECT: CLOSE"
    }
}

fn status_text(overlay: &OverlayStateData, snapshot: &lobbies::Snapshot) -> String {
    if let Some(text) = overlay.notice_text.as_ref() {
        return text.clone();
    }
    if let Some(prompt) = overlay.password_prompt.as_ref() {
        return match &prompt.mode {
            PasswordPromptMode::CreateLobby => {
                format!(
                    "Choose up to {} uppercase letters, or leave it blank for a public lobby.",
                    PASSWORD_PROMPT_MAX_LEN
                )
            }
            PasswordPromptMode::JoinLobby { code } => {
                format!(
                    "Enter the {}-letter uppercase password for lobby {code}.",
                    PASSWORD_PROMPT_MAX_LEN
                )
            }
        };
    }
    if let Some(text) = lobbies::reconnect_status_text() {
        return text;
    }
    if let Some(status) = snapshot.last_status.as_ref()
        && let Some(message) = status.message.as_ref()
        && !message.trim().is_empty()
    {
        return message.clone();
    }
    match &snapshot.connection {
        lobbies::ConnectionState::Disconnected => "Disconnected from online service.".to_string(),
        lobbies::ConnectionState::Connecting => "Connecting to online service...".to_string(),
        lobbies::ConnectionState::Connected => {
            if snapshot.joined_lobby.is_some() {
                "Connected to online service.".to_string()
            } else {
                "Select a lobby or create one.".to_string()
            }
        }
        lobbies::ConnectionState::Error(error) => format!("Connection error: {error}"),
    }
}

#[inline(always)]
fn browse_item_count(snapshot: &lobbies::Snapshot) -> usize {
    snapshot.available_lobbies.len() + 3
}

fn clamp_browse_scroll(overlay: &mut OverlayStateData, snapshot: &lobbies::Snapshot) {
    let max_scroll = snapshot
        .available_lobbies
        .len()
        .saturating_sub(VISIBLE_LOBBY_ROWS);
    overlay.browse_scroll = overlay.browse_scroll.min(max_scroll);
    if overlay.browse_index < overlay.browse_scroll {
        overlay.browse_scroll = overlay.browse_index;
    }
    let visible_end = overlay.browse_scroll + VISIBLE_LOBBY_ROWS;
    if overlay.browse_index >= visible_end
        && overlay.browse_index < snapshot.available_lobbies.len()
    {
        overlay.browse_scroll = overlay
            .browse_index
            .saturating_sub(VISIBLE_LOBBY_ROWS.saturating_sub(1))
            .min(max_scroll);
    }
}

enum BrowseAction<'a> {
    Lobby(&'a lobbies::PublicLobby),
    Refresh,
    Create,
    Close,
}

fn resolve_browse_action<'a>(
    snapshot: &'a lobbies::Snapshot,
    browse_index: usize,
) -> BrowseAction<'a> {
    if let Some(lobby) = snapshot.available_lobbies.get(browse_index) {
        return BrowseAction::Lobby(lobby);
    }
    match browse_index.saturating_sub(snapshot.available_lobbies.len()) {
        0 => BrowseAction::Refresh,
        1 => BrowseAction::Create,
        _ => BrowseAction::Close,
    }
}

#[inline(always)]
fn clear_notice(overlay: &mut OverlayStateData) {
    overlay.notice_text = None;
    overlay.notice_time_left = 0.0;
}

#[inline(always)]
fn set_notice(overlay: &mut OverlayStateData, text: &str) {
    overlay.notice_text = Some(text.to_string());
    overlay.notice_time_left = 3.0;
}

#[inline(always)]
fn begin_password_prompt_create() -> PasswordPromptState {
    PasswordPromptState {
        mode: PasswordPromptMode::CreateLobby,
        value: String::new(),
        wheel: PasswordWheel::new(3),
        nav_key_held_direction: None,
        nav_key_held_since: None,
        nav_key_last_scrolled_at: None,
    }
}

#[inline(always)]
fn begin_password_prompt_join(code: &str) -> PasswordPromptState {
    PasswordPromptState {
        mode: PasswordPromptMode::JoinLobby {
            code: code.to_string(),
        },
        value: String::new(),
        wheel: PasswordWheel::new(3),
        nav_key_held_direction: None,
        nav_key_held_since: None,
        nav_key_last_scrolled_at: None,
    }
}

fn reset_nav_hold(prompt: &mut PasswordPromptState) {
    prompt.nav_key_held_direction = None;
    prompt.nav_key_held_since = None;
    prompt.nav_key_last_scrolled_at = None;
}

fn on_nav_press(prompt: &mut PasswordPromptState, dir: NavDirection) {
    let now = Instant::now();
    prompt.nav_key_held_direction = Some(dir);
    prompt.nav_key_held_since = Some(now);
    prompt.nav_key_last_scrolled_at = Some(now);
}

fn on_nav_release(prompt: &mut PasswordPromptState, dir: NavDirection) {
    if prompt.nav_key_held_direction == Some(dir) {
        reset_nav_hold(prompt);
    }
}

fn update_password_prompt_hold(prompt: &mut PasswordPromptState) {
    let Some(dir) = prompt.nav_key_held_direction else {
        return;
    };
    let Some(held_since) = prompt.nav_key_held_since else {
        return;
    };
    let Some(last_at) = prompt.nav_key_last_scrolled_at else {
        return;
    };

    let now = Instant::now();
    if now.duration_since(held_since) < NAV_INITIAL_HOLD_DELAY {
        return;
    }
    if now.duration_since(last_at) < NAV_REPEAT_SCROLL_INTERVAL {
        return;
    }

    let dist = match dir {
        NavDirection::Left => -1,
        NavDirection::Right => 1,
    };
    prompt.wheel.scroll_by(dist);
    prompt.nav_key_last_scrolled_at = Some(now);
    audio::play_sfx("assets/sounds/change.ogg");
}

fn handle_password_prompt_input(overlay: &mut OverlayStateData, ev: &InputEvent) -> InputOutcome {
    match ev.action {
        VirtualAction::p1_left
        | VirtualAction::p1_up
        | VirtualAction::p1_menu_left
        | VirtualAction::p1_menu_up
        | VirtualAction::p2_left
        | VirtualAction::p2_up
        | VirtualAction::p2_menu_left
        | VirtualAction::p2_menu_up => {
            if ev.pressed {
                clear_notice(overlay);
            }
            let Some(prompt) = overlay.password_prompt.as_mut() else {
                return InputOutcome::None;
            };
            if ev.pressed {
                if prompt.nav_key_held_direction != Some(NavDirection::Left) {
                    prompt.wheel.scroll_by(-1);
                    on_nav_press(prompt, NavDirection::Left);
                    audio::play_sfx("assets/sounds/change.ogg");
                }
            } else {
                on_nav_release(prompt, NavDirection::Left);
            }
            InputOutcome::None
        }
        VirtualAction::p1_right
        | VirtualAction::p1_down
        | VirtualAction::p1_menu_right
        | VirtualAction::p1_menu_down
        | VirtualAction::p2_right
        | VirtualAction::p2_down
        | VirtualAction::p2_menu_right
        | VirtualAction::p2_menu_down => {
            if ev.pressed {
                clear_notice(overlay);
            }
            let Some(prompt) = overlay.password_prompt.as_mut() else {
                return InputOutcome::None;
            };
            if ev.pressed {
                if prompt.nav_key_held_direction != Some(NavDirection::Right) {
                    prompt.wheel.scroll_by(1);
                    on_nav_press(prompt, NavDirection::Right);
                    audio::play_sfx("assets/sounds/change.ogg");
                }
            } else {
                on_nav_release(prompt, NavDirection::Right);
            }
            InputOutcome::None
        }
        VirtualAction::p1_start | VirtualAction::p2_start if ev.pressed => {
            clear_notice(overlay);
            let selected = overlay
                .password_prompt
                .as_ref()
                .map(password_prompt_selected)
                .unwrap_or("&OK;");
            match selected {
                "&OK;" => submit_password_prompt(overlay),
                "&BACK;" => {
                    let Some(prompt) = overlay.password_prompt.as_mut() else {
                        return InputOutcome::None;
                    };
                    if password_prompt_backspace(prompt) {
                        audio::play_sfx("assets/sounds/change_value.ogg");
                    } else {
                        audio::play_sfx("assets/sounds/boom.ogg");
                    }
                    InputOutcome::None
                }
                ch => {
                    let Some(prompt) = overlay.password_prompt.as_mut() else {
                        return InputOutcome::None;
                    };
                    if password_prompt_add_char(prompt, ch) {
                        audio::play_sfx("assets/sounds/start.ogg");
                    } else {
                        audio::play_sfx("assets/sounds/boom.ogg");
                    }
                    InputOutcome::None
                }
            }
        }
        VirtualAction::p1_select | VirtualAction::p2_select if ev.pressed => {
            clear_notice(overlay);
            let Some(prompt) = overlay.password_prompt.as_mut() else {
                return InputOutcome::None;
            };
            if password_prompt_backspace(prompt) {
                audio::play_sfx("assets/sounds/change_value.ogg");
            } else {
                audio::play_sfx("assets/sounds/boom.ogg");
            }
            InputOutcome::None
        }
        VirtualAction::p1_back | VirtualAction::p2_back if ev.pressed => {
            clear_notice(overlay);
            overlay.password_prompt = None;
            audio::play_sfx("assets/sounds/change_value.ogg");
            InputOutcome::None
        }
        _ => InputOutcome::None,
    }
}

fn submit_password_prompt(overlay: &mut OverlayStateData) -> InputOutcome {
    let Some(prompt) = overlay.password_prompt.take() else {
        return InputOutcome::None;
    };
    match prompt.mode {
        PasswordPromptMode::CreateLobby => InputOutcome::CreateRequested(prompt.value),
        PasswordPromptMode::JoinLobby { code } => {
            if prompt.value.is_empty() {
                overlay.password_prompt = Some(PasswordPromptState {
                    mode: PasswordPromptMode::JoinLobby { code },
                    value: String::new(),
                    wheel: PasswordWheel::new(3),
                    nav_key_held_direction: None,
                    nav_key_held_since: None,
                    nav_key_last_scrolled_at: None,
                });
                set_notice(overlay, "Enter the lobby password.");
                audio::play_sfx("assets/sounds/boom.ogg");
                InputOutcome::None
            } else {
                InputOutcome::JoinRequested {
                    code,
                    password: prompt.value,
                }
            }
        }
    }
}

#[inline(always)]
fn password_prompt_selected(prompt: &PasswordPromptState) -> &'static str {
    PASSWORD_CHARS
        .get(prompt.wheel.focused_info_index())
        .copied()
        .unwrap_or("&OK;")
}

fn password_prompt_add_char(prompt: &mut PasswordPromptState, ch: &str) -> bool {
    if prompt.value.len() >= PASSWORD_PROMPT_MAX_LEN {
        return false;
    }
    prompt.value.push_str(ch);
    if prompt.value.len() >= PASSWORD_PROMPT_MAX_LEN {
        prompt.wheel.scroll_to_pos(2);
    }
    true
}

#[inline(always)]
fn password_prompt_backspace(prompt: &mut PasswordPromptState) -> bool {
    prompt.value.pop().is_some()
}

fn password_prompt_title(prompt: &PasswordPromptState) -> String {
    match &prompt.mode {
        PasswordPromptMode::CreateLobby => "Create Lobby Password (Optional)".to_string(),
        PasswordPromptMode::JoinLobby { code } => format!("Join {code}"),
    }
}

#[inline(always)]
fn password_prompt_hint(_prompt: &PasswordPromptState) -> &'static str {
    "Use &MENULEFT;/&MENURIGHT; to pick characters, then press &START;."
}

fn password_prompt_value(prompt: &PasswordPromptState) -> String {
    prompt.value.clone()
}
