use crate::act;
use crate::engine::input::{InputEvent, RawKeyboardEvent, VirtualAction};
use crate::engine::present::actors::Actor;
use crate::engine::present::color;
use crate::engine::space::{screen_center_x, screen_center_y, screen_height, screen_width};
use crate::game::online::lobbies;
use winit::keyboard::KeyCode;

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
const PASSWORD_PROMPT_W: f32 = 430.0;
const PASSWORD_PROMPT_H: f32 = 170.0;
const PASSWORD_PROMPT_CURSOR_PERIOD: f32 = 0.8;
const PASSWORD_PROMPT_MAX_LEN: usize = 32;

#[derive(Clone, Debug)]
enum PasswordPromptMode {
    CreateLobby,
    JoinLobby { code: String },
}

#[derive(Clone, Debug)]
struct PasswordPromptState {
    mode: PasswordPromptMode,
    value: String,
    blink_t: f32,
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
        prompt.blink_t = (prompt.blink_t + dt.max(0.0)) % PASSWORD_PROMPT_CURSOR_PERIOD;
    }

    if overlay.notice_time_left > 0.0 {
        overlay.notice_time_left = (overlay.notice_time_left - dt.max(0.0)).max(0.0);
        if overlay.notice_time_left <= 0.0 {
            overlay.notice_text = None;
        }
    }
}

pub fn handle_input(state: &mut OverlayState, ev: &InputEvent) -> InputOutcome {
    if !ev.pressed {
        return InputOutcome::None;
    }

    let OverlayState::Visible(overlay) = state else {
        return InputOutcome::None;
    };

    let snapshot = lobbies::snapshot();
    if overlay.password_prompt.is_some() {
        return handle_password_prompt_input(overlay, ev);
    }
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
    key: Option<&RawKeyboardEvent>,
    text: Option<&str>,
) -> InputOutcome {
    let OverlayState::Visible(overlay) = state else {
        return InputOutcome::None;
    };
    if overlay.password_prompt.is_none() {
        return InputOutcome::None;
    }

    if key.is_some_and(|key| key.pressed) {
        match key.unwrap().code {
            KeyCode::Backspace => {
                clear_notice(overlay);
                let Some(prompt) = overlay.password_prompt.as_mut() else {
                    return InputOutcome::None;
                };
                password_prompt_backspace(prompt);
                return InputOutcome::None;
            }
            KeyCode::Escape => {
                clear_notice(overlay);
                overlay.password_prompt = None;
                return InputOutcome::None;
            }
            KeyCode::Enter | KeyCode::NumpadEnter => {
                clear_notice(overlay);
                return submit_password_prompt(overlay);
            }
            _ => {}
        }
    }

    if let Some(text) = text {
        clear_notice(overlay);
        let Some(prompt) = overlay.password_prompt.as_mut() else {
            return InputOutcome::None;
        };
        password_prompt_add_text(prompt, text);
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
        font("wendy"):
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
        actors.extend(build_password_prompt(center_x, center_y, prompt));
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
            font("wendy"):
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
            font("wendy"):
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
        font("wendy"):
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
            font("wendy"):
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

fn build_password_prompt(center_x: f32, center_y: f32, prompt: &PasswordPromptState) -> Vec<Actor> {
    let mut actors = Vec::new();
    let title = password_prompt_title(prompt);
    let hint = password_prompt_hint(prompt);
    let footer = password_prompt_footer();
    let value = masked_password_value(prompt);

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
    actors.push(act!(text:
        font("wendy"):
        settext(title):
        align(0.5, 0.5):
        xy(center_x, center_y - 50.0):
        zoom(0.46):
        diffuse(1.0, 1.0, 1.0, 1.0):
        z(OVERLAY_Z + 7):
        horizalign(center)
    ));
    actors.push(act!(text:
        font("miso"):
        settext(hint):
        align(0.5, 0.5):
        xy(center_x, center_y - 10.0):
        zoom(0.8):
        maxwidth(PASSWORD_PROMPT_W - 36.0):
        diffuse(0.8, 0.8, 0.8, 1.0):
        z(OVERLAY_Z + 7):
        horizalign(center)
    ));
    actors.push(act!(text:
        font("miso"):
        settext(value):
        align(0.5, 0.5):
        xy(center_x, center_y + 26.0):
        zoom(1.0):
        maxwidth(PASSWORD_PROMPT_W - 40.0):
        diffuse(0.4, 1.0, 0.4, 1.0):
        z(OVERLAY_Z + 7):
        horizalign(center)
    ));
    actors.push(act!(text:
        font("miso"):
        settext(footer):
        align(0.5, 0.5):
        xy(center_x, center_y + 58.0):
        zoom(0.78):
        maxwidth(PASSWORD_PROMPT_W - 36.0):
        diffuse(0.75, 0.75, 0.75, 1.0):
        z(OVERLAY_Z + 7):
        horizalign(center)
    ));
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

fn close_hint(overlay: &OverlayStateData, snapshot: &lobbies::Snapshot) -> &'static str {
    if overlay.password_prompt.is_some() {
        return "TYPE TO ENTER PASSWORD    ENTER/START: CONFIRM    ESC/BACK: CANCEL";
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
                "Enter a password, or leave it blank for a public lobby.".to_string()
            }
            PasswordPromptMode::JoinLobby { code } => {
                format!("Enter the password for lobby {code}.")
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
        blink_t: 0.0,
    }
}

#[inline(always)]
fn begin_password_prompt_join(code: &str) -> PasswordPromptState {
    PasswordPromptState {
        mode: PasswordPromptMode::JoinLobby {
            code: code.to_string(),
        },
        value: String::new(),
        blink_t: 0.0,
    }
}

fn handle_password_prompt_input(overlay: &mut OverlayStateData, ev: &InputEvent) -> InputOutcome {
    match ev.action {
        VirtualAction::p1_start | VirtualAction::p2_start => {
            clear_notice(overlay);
            submit_password_prompt(overlay)
        }
        VirtualAction::p1_back
        | VirtualAction::p2_back
        | VirtualAction::p1_select
        | VirtualAction::p2_select => {
            clear_notice(overlay);
            overlay.password_prompt = None;
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
                    blink_t: 0.0,
                });
                set_notice(overlay, "Enter the lobby password.");
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

fn password_prompt_add_text(prompt: &mut PasswordPromptState, text: &str) {
    let mut len = prompt.value.chars().count();
    for ch in text.chars() {
        if ch.is_control() {
            continue;
        }
        if len >= PASSWORD_PROMPT_MAX_LEN {
            break;
        }
        prompt.value.push(ch);
        len += 1;
    }
}

#[inline(always)]
fn password_prompt_backspace(prompt: &mut PasswordPromptState) {
    let _ = prompt.value.pop();
}

fn password_prompt_title(prompt: &PasswordPromptState) -> String {
    match &prompt.mode {
        PasswordPromptMode::CreateLobby => "Create Lobby".to_string(),
        PasswordPromptMode::JoinLobby { code } => format!("Join {code}"),
    }
}

fn password_prompt_hint(prompt: &PasswordPromptState) -> String {
    match &prompt.mode {
        PasswordPromptMode::CreateLobby => {
            "Type a password to lock the lobby. Leave it blank to create a public lobby."
                .to_string()
        }
        PasswordPromptMode::JoinLobby { .. } => "Type the lobby password.".to_string(),
    }
}

#[inline(always)]
fn password_prompt_footer() -> &'static str {
    "Keyboard input only. ENTER/START: CONFIRM    ESC/BACK: CANCEL"
}

fn masked_password_value(prompt: &PasswordPromptState) -> String {
    let cursor = if prompt.blink_t < PASSWORD_PROMPT_CURSOR_PERIOD * 0.5 {
        "▮"
    } else {
        " "
    };
    let mut masked = "*".repeat(prompt.value.chars().count());
    if prompt.value.chars().count() < PASSWORD_PROMPT_MAX_LEN {
        masked.push_str(cursor);
    }
    format!("> {masked}")
}
