use crate::act;
use crate::engine::input::{InputEvent, VirtualAction};
use crate::engine::present::actors::Actor;
use crate::engine::present::color;
use crate::engine::space::{screen_center_x, screen_center_y, screen_height, screen_width};
use crate::game::scores;

use super::leaderboard::format_groovestats_date;

pub const REPLAY_FOCUS_TWEEN_SECONDS: f32 = 0.1;
pub const REPLAY_INPUT_LOCK_SECONDS: f32 = 0.15;
const REPLAY_MAX_ENTRIES: usize = 1024;

const GS_LEADERBOARD_NUM_ENTRIES: usize = 13;
const GS_LEADERBOARD_ROW_HEIGHT: f32 = 24.0;
const GS_LEADERBOARD_PANE_HEIGHT: f32 = 360.0;
const GS_LEADERBOARD_PANE_WIDTH_SINGLE: f32 = 330.0;
const GS_LEADERBOARD_PANE_CENTER_Y: f32 = -15.0;
const GS_LEADERBOARD_DIM_ALPHA: f32 = 0.875;
const GS_LEADERBOARD_Z: i16 = 1480;

#[derive(Clone, Debug)]
pub struct ReplayOverlayStateData {
    pub entries: Vec<scores::MachineReplayEntry>,
    pub selected_index: usize,
    pub prev_selected_index: usize,
    pub focus_anim_elapsed: f32,
    pub input_lock: f32,
}

#[derive(Clone, Debug)]
pub enum ReplayOverlayState {
    Hidden,
    Visible(ReplayOverlayStateData),
}

#[derive(Clone, Debug)]
pub enum ReplayInputOutcome {
    None,
    ChangedSelection,
    Closed,
    StartGameplay(ReplayStartPayload),
}

#[derive(Clone, Debug)]
pub struct ReplayStartPayload {
    pub replay: Vec<scores::ReplayEdge>,
    pub name: String,
    pub score: f64,
    pub replay_beat0_time_seconds: f32,
}

fn replay_total_items(state: &ReplayOverlayStateData) -> usize {
    state.entries.len() + 1
}

pub fn begin_replay_overlay(chart_hash: &str) -> ReplayOverlayState {
    if chart_hash.trim().is_empty() {
        return ReplayOverlayState::Hidden;
    }
    let entries = scores::get_machine_replays_local(chart_hash, REPLAY_MAX_ENTRIES);
    ReplayOverlayState::Visible(ReplayOverlayStateData {
        entries,
        selected_index: 0,
        prev_selected_index: 0,
        focus_anim_elapsed: REPLAY_FOCUS_TWEEN_SECONDS,
        input_lock: REPLAY_INPUT_LOCK_SECONDS,
    })
}

pub fn update_replay_overlay(state: &mut ReplayOverlayState, dt: f32) -> bool {
    let ReplayOverlayState::Visible(overlay) = state else {
        return false;
    };
    let dt = dt.max(0.0);
    overlay.input_lock = (overlay.input_lock - dt).max(0.0);
    if overlay.focus_anim_elapsed < REPLAY_FOCUS_TWEEN_SECONDS {
        overlay.focus_anim_elapsed =
            (overlay.focus_anim_elapsed + dt).min(REPLAY_FOCUS_TWEEN_SECONDS);
    }
    true
}

pub fn handle_replay_input(state: &mut ReplayOverlayState, ev: &InputEvent) -> ReplayInputOutcome {
    if !ev.pressed {
        return ReplayInputOutcome::None;
    }
    let ReplayOverlayState::Visible(overlay) = state else {
        return ReplayInputOutcome::None;
    };

    if overlay.input_lock > 0.0 {
        return ReplayInputOutcome::None;
    }

    match ev.action {
        VirtualAction::p1_up
        | VirtualAction::p1_menu_up
        | VirtualAction::p1_left
        | VirtualAction::p1_menu_left
        | VirtualAction::p2_up
        | VirtualAction::p2_menu_up
        | VirtualAction::p2_left
        | VirtualAction::p2_menu_left => {
            let len = replay_total_items(overlay);
            if len <= 1 {
                return ReplayInputOutcome::None;
            }
            let old = overlay.selected_index.min(len - 1);
            let next = ((old as isize - 1).rem_euclid(len as isize)) as usize;
            if next == old {
                return ReplayInputOutcome::None;
            }
            overlay.prev_selected_index = old;
            overlay.selected_index = next;
            overlay.focus_anim_elapsed = 0.0;
            ReplayInputOutcome::ChangedSelection
        }
        VirtualAction::p1_down
        | VirtualAction::p1_menu_down
        | VirtualAction::p1_right
        | VirtualAction::p1_menu_right
        | VirtualAction::p2_down
        | VirtualAction::p2_menu_down
        | VirtualAction::p2_right
        | VirtualAction::p2_menu_right => {
            let len = replay_total_items(overlay);
            if len <= 1 {
                return ReplayInputOutcome::None;
            }
            let old = overlay.selected_index.min(len - 1);
            let next = ((old as isize + 1).rem_euclid(len as isize)) as usize;
            if next == old {
                return ReplayInputOutcome::None;
            }
            overlay.prev_selected_index = old;
            overlay.selected_index = next;
            overlay.focus_anim_elapsed = 0.0;
            ReplayInputOutcome::ChangedSelection
        }
        VirtualAction::p1_start | VirtualAction::p2_start => {
            let selected = overlay
                .selected_index
                .min(replay_total_items(overlay).saturating_sub(1));
            if selected >= overlay.entries.len() {
                *state = ReplayOverlayState::Hidden;
                ReplayInputOutcome::Closed
            } else {
                let (replay, name, score, replay_beat0_time_seconds) = {
                    let entry = &overlay.entries[selected];
                    (
                        entry.replay.clone(),
                        entry.name.clone(),
                        entry.score,
                        entry.replay_beat0_time_seconds,
                    )
                };
                *state = ReplayOverlayState::Hidden;
                ReplayInputOutcome::StartGameplay(ReplayStartPayload {
                    replay,
                    name,
                    score,
                    replay_beat0_time_seconds,
                })
            }
        }
        VirtualAction::p1_back
        | VirtualAction::p2_back
        | VirtualAction::p1_select
        | VirtualAction::p2_select => {
            *state = ReplayOverlayState::Hidden;
            ReplayInputOutcome::Closed
        }
        _ => ReplayInputOutcome::None,
    }
}

pub fn build_replay_overlay(
    state: &ReplayOverlayState,
    active_color_index: i32,
) -> Option<Vec<Actor>> {
    let ReplayOverlayState::Visible(overlay) = state else {
        return None;
    };

    let mut actors = Vec::new();
    let pane_width = GS_LEADERBOARD_PANE_WIDTH_SINGLE;
    let pane_cx = screen_center_x();
    let pane_cy = screen_center_y() + GS_LEADERBOARD_PANE_CENTER_Y;
    let row_center = (GS_LEADERBOARD_NUM_ENTRIES as f32 + 1.0) * 0.5;
    let selected_color = color::simply_love_rgba(active_color_index);
    let total_items = replay_total_items(overlay).max(1);
    let visible_rows = GS_LEADERBOARD_NUM_ENTRIES;
    let window_start = if total_items <= visible_rows {
        0
    } else {
        overlay
            .selected_index
            .saturating_sub(visible_rows / 2)
            .min(total_items - visible_rows)
    };

    actors.push(act!(quad:
        align(0.0, 0.0): xy(0.0, 0.0):
        zoomto(screen_width(), screen_height()):
        diffuse(0.0, 0.0, 0.0, GS_LEADERBOARD_DIM_ALPHA):
        z(GS_LEADERBOARD_Z)
    ));
    actors.push(act!(quad:
        align(0.5, 0.5):
        xy(pane_cx, pane_cy):
        zoomto(pane_width + 2.0, GS_LEADERBOARD_PANE_HEIGHT + 2.0):
        diffuse(1.0, 1.0, 1.0, 1.0):
        z(GS_LEADERBOARD_Z + 2)
    ));
    actors.push(act!(quad:
        align(0.5, 0.5):
        xy(pane_cx, pane_cy):
        zoomto(pane_width, GS_LEADERBOARD_PANE_HEIGHT):
        diffuse(0.0, 0.0, 0.0, 1.0):
        z(GS_LEADERBOARD_Z + 3)
    ));

    let header_y = pane_cy - GS_LEADERBOARD_PANE_HEIGHT * 0.5 + GS_LEADERBOARD_ROW_HEIGHT * 0.5;
    actors.push(act!(quad:
        align(0.5, 0.5):
        xy(pane_cx, header_y):
        zoomto(pane_width + 2.0, GS_LEADERBOARD_ROW_HEIGHT + 2.0):
        diffuse(1.0, 1.0, 1.0, 1.0):
        z(GS_LEADERBOARD_Z + 4)
    ));
    actors.push(act!(quad:
        align(0.5, 0.5):
        xy(pane_cx, header_y):
        zoomto(pane_width, GS_LEADERBOARD_ROW_HEIGHT):
        diffuse(0.0, 0.0, 1.0, 1.0):
        z(GS_LEADERBOARD_Z + 5)
    ));
    actors.push(act!(text:
        font("wendy"):
        settext("Play Replay"):
        align(0.5, 0.5):
        xy(pane_cx, header_y):
        zoom(0.5):
        diffuse(1.0, 1.0, 1.0, 1.0):
        z(GS_LEADERBOARD_Z + 6):
        horizalign(center)
    ));
    actors.push(act!(text:
        font("miso"):
        settext(format!("{} Local Scores", overlay.entries.len())):
        align(0.5, 0.5):
        xy(pane_cx, pane_cy - GS_LEADERBOARD_PANE_HEIGHT * 0.5 - 24.0):
        zoom(0.8):
        diffuse(1.0, 1.0, 1.0, 1.0):
        z(GS_LEADERBOARD_Z + 6):
        horizalign(center)
    ));

    let rank_x = pane_cx - pane_width * 0.5 + 32.0;
    let name_x = pane_cx - pane_width * 0.5 + 100.0;
    let score_x = pane_cx + 63.0;
    let date_x = pane_cx + pane_width * 0.5 - 2.0;

    for row_slot in 0..visible_rows {
        let row_idx = window_start + row_slot;
        if row_idx >= total_items {
            break;
        }
        let y = pane_cy + GS_LEADERBOARD_ROW_HEIGHT * ((row_slot + 1) as f32 - row_center);
        let selected = row_idx == overlay.selected_index;
        if selected {
            actors.push(act!(quad:
                align(0.5, 0.5):
                xy(pane_cx, y):
                zoomto(pane_width, GS_LEADERBOARD_ROW_HEIGHT):
                diffuse(selected_color[0], selected_color[1], selected_color[2], 1.0):
                z(GS_LEADERBOARD_Z + 5)
            ));
        }

        let (rank, name, score, date, is_fail, is_exit) = if row_idx < overlay.entries.len() {
            let entry = &overlay.entries[row_idx];
            (
                format!("{}.", entry.rank),
                entry.name.clone(),
                format!("{:.2}%", entry.score / 100.0),
                format_groovestats_date(&entry.date),
                entry.is_fail,
                false,
            )
        } else {
            (
                String::new(),
                "Exit".to_string(),
                String::new(),
                String::new(),
                false,
                true,
            )
        };

        let base = if selected {
            [0.0, 0.0, 0.0, 1.0]
        } else {
            [1.0, 1.0, 1.0, 1.0]
        };
        let name_col = if is_exit {
            if selected {
                [0.2, 0.0, 0.0, 1.0]
            } else {
                [1.0, 0.25, 0.25, 1.0]
            }
        } else {
            base
        };
        let score_col = if is_fail && !selected {
            [1.0, 0.0, 0.0, 1.0]
        } else {
            base
        };

        actors.push(act!(text:
            font("miso"):
            settext(rank):
            align(1.0, 0.5):
            xy(rank_x, y):
            zoom(0.8):
            maxwidth(30.0):
            diffuse(base[0], base[1], base[2], base[3]):
            z(GS_LEADERBOARD_Z + 7):
            horizalign(right)
        ));
        actors.push(act!(text:
            font("miso"):
            settext(name):
            align(0.5, 0.5):
            xy(name_x, y):
            zoom(0.8):
            maxwidth(130.0):
            diffuse(name_col[0], name_col[1], name_col[2], name_col[3]):
            z(GS_LEADERBOARD_Z + 7):
            horizalign(center)
        ));
        actors.push(act!(text:
            font("miso"):
            settext(score):
            align(1.0, 0.5):
            xy(score_x, y):
            zoom(0.8):
            diffuse(score_col[0], score_col[1], score_col[2], score_col[3]):
            z(GS_LEADERBOARD_Z + 7):
            horizalign(right)
        ));
        actors.push(act!(text:
            font("miso"):
            settext(date):
            align(1.0, 0.5):
            xy(date_x, y):
            zoom(0.8):
            diffuse(base[0], base[1], base[2], base[3]):
            z(GS_LEADERBOARD_Z + 7):
            horizalign(right)
        ));
    }

    actors.push(act!(text:
        font("miso"):
        settext("START: PLAY REPLAY    BACK/SELECT: CANCEL"):
        align(0.5, 0.5):
        xy(screen_center_x(), screen_height() - 50.0):
        zoom(1.1):
        diffuse(1.0, 1.0, 1.0, 1.0):
        z(GS_LEADERBOARD_Z + 8):
        horizalign(center)
    ));

    Some(actors)
}
