use crate::act;
use crate::assets::{FontRole, machine_font_key};
use crate::config::MachineFont;
use deadlib_present::actors::Actor;
use deadlib_present::color;
use deadlib_present::space::{screen_center_x, screen_center_y, screen_height, screen_width};
use deadsync_input::{InputEvent, VirtualAction};
use deadsync_score as score_data;

pub const REPLAY_FOCUS_TWEEN_SECONDS: f32 = 0.1;
pub const REPLAY_INPUT_LOCK_SECONDS: f32 = 0.15;
pub const REPLAY_MAX_ENTRIES: usize = 1024;

const GS_LEADERBOARD_NUM_ENTRIES: usize = 13;
const GS_LEADERBOARD_ROW_HEIGHT: f32 = 24.0;
const GS_LEADERBOARD_PANE_HEIGHT: f32 = 360.0;
const GS_LEADERBOARD_PANE_WIDTH_SINGLE: f32 = 330.0;
const GS_LEADERBOARD_PANE_CENTER_Y: f32 = -15.0;
const GS_LEADERBOARD_DIM_ALPHA: f32 = 0.875;
const GS_LEADERBOARD_Z: i16 = 1480;

#[derive(Clone, Debug)]
pub struct ReplayOverlayStateData {
    pub entries: Vec<score_data::MachineReplayEntry>,
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
    pub replay: Vec<score_data::ReplayEdge>,
    pub name: String,
    pub score: f64,
    pub replay_beat0_time_ns: i64,
}

const fn replay_total_items(state: &ReplayOverlayStateData) -> usize {
    state.entries.len() + 1
}

#[must_use]
pub const fn begin_replay_overlay(
    entries: Vec<score_data::MachineReplayEntry>,
) -> ReplayOverlayState {
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
                let (replay, name, score, replay_beat0_time_ns) = {
                    let entry = &overlay.entries[selected];
                    (
                        entry.replay.clone(),
                        entry.name.clone(),
                        entry.score,
                        entry.replay_beat0_time_ns,
                    )
                };
                *state = ReplayOverlayState::Hidden;
                ReplayInputOutcome::StartGameplay(ReplayStartPayload {
                    replay,
                    name,
                    score,
                    replay_beat0_time_ns,
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

pub fn push_replay_overlay(
    actors: &mut Vec<Actor>,
    state: &ReplayOverlayState,
    active_color_index: i32,
    machine_font: MachineFont,
) -> bool {
    let ReplayOverlayState::Visible(overlay) = state else {
        return false;
    };
    actors.reserve(8 + replay_total_items(overlay).min(GS_LEADERBOARD_NUM_ENTRIES) * 5);
    push_replay_overlay_unreserved(actors, overlay, active_color_index, machine_font);
    true
}

fn push_replay_overlay_unreserved(
    actors: &mut Vec<Actor>,
    overlay: &ReplayOverlayStateData,
    active_color_index: i32,
    machine_font: MachineFont,
) {
    let pane_width = GS_LEADERBOARD_PANE_WIDTH_SINGLE;
    let pane_cx = screen_center_x();
    let pane_cy = screen_center_y() + GS_LEADERBOARD_PANE_CENTER_Y;
    let row_center = f32::midpoint(GS_LEADERBOARD_NUM_ENTRIES as f32, 1.0);
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

    let header_y =
        GS_LEADERBOARD_ROW_HEIGHT.mul_add(0.5, GS_LEADERBOARD_PANE_HEIGHT.mul_add(-0.5, pane_cy));
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
        font(machine_font_key(machine_font, FontRole::Header)):
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
        xy(pane_cx, GS_LEADERBOARD_PANE_HEIGHT.mul_add(-0.5, pane_cy) - 24.0):
        zoom(0.8):
        diffuse(1.0, 1.0, 1.0, 1.0):
        z(GS_LEADERBOARD_Z + 6):
        horizalign(center)
    ));

    let rank_x = pane_width.mul_add(-0.5, pane_cx) + 32.0;
    let name_x = pane_width.mul_add(-0.5, pane_cx) + 100.0;
    let score_x = pane_cx + 63.0;
    let date_x = pane_width.mul_add(0.5, pane_cx) - 2.0;

    for row_slot in 0..visible_rows {
        let row_idx = window_start + row_slot;
        if row_idx >= total_items {
            break;
        }
        let y = GS_LEADERBOARD_ROW_HEIGHT.mul_add((row_slot + 1) as f32 - row_center, pane_cy);
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
                score_data::format_leaderboard_date(&entry.date),
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
}

/// Stable old/new fixture for the replay-selector actor batch.
#[cfg(any(test, feature = "bench-support"))]
pub struct ReplayOverlayAppendBenchmark {
    state: ReplayOverlayState,
}

#[cfg(any(test, feature = "bench-support"))]
impl ReplayOverlayAppendBenchmark {
    #[must_use]
    pub fn new() -> Self {
        let entries = (0..13)
            .map(|index| score_data::MachineReplayEntry {
                rank: index + 1,
                name: format!("P{:02}", index + 1),
                score: 99_500.0 - index as f64 * 125.0,
                date: format!("2026-08-{:02}", index + 1),
                is_fail: index == 11,
                replay_beat0_time_ns: 0,
                replay: Vec::new(),
            })
            .collect();
        let mut state = begin_replay_overlay(entries);
        let ReplayOverlayState::Visible(overlay) = &mut state else {
            unreachable!("replay benchmark always starts visible");
        };
        overlay.selected_index = 6;
        overlay.prev_selected_index = 5;
        Self { state }
    }

    #[must_use]
    pub fn actor_count(&self) -> usize {
        let mut actors = Vec::with_capacity(73);
        let visible = push_replay_overlay(&mut actors, &self.state, 2, MachineFont::Mega);
        debug_assert!(visible);
        actors.len()
    }

    #[must_use]
    pub fn legacy_frame(&self, out: &mut Vec<Actor>) -> u64 {
        out.clear();
        let ReplayOverlayState::Visible(overlay) = &self.state else {
            unreachable!("replay benchmark stays visible");
        };
        let mut staged = Vec::new();
        push_replay_overlay_unreserved(&mut staged, overlay, 2, MachineFont::Mega);
        out.extend(staged);
        std::hint::black_box(&*out);
        super::overlay_actor_checksum(out)
    }

    #[must_use]
    pub fn direct_frame(&self, out: &mut Vec<Actor>) -> u64 {
        out.clear();
        let visible = push_replay_overlay(out, &self.state, 2, MachineFont::Mega);
        debug_assert!(visible);
        std::hint::black_box(&*out);
        super::overlay_actor_checksum(out)
    }
}

#[cfg(any(test, feature = "bench-support"))]
impl Default for ReplayOverlayAppendBenchmark {
    fn default() -> Self {
        Self::new()
    }
}
