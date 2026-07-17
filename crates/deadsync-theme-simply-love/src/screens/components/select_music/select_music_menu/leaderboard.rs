use crate::act;
use crate::assets::{FontRole, machine_font_key};
use crate::config::MachineFont;
use crate::screens::components::shared::gs_scorebox::entries_with_local_self_state;
use crate::views::{
    ScoreboxSideView, SelectMusicLeaderboardRequest, SelectMusicLeaderboardSideView,
    SelectMusicLeaderboardView,
};
use deadlib_present::actors::Actor;
use deadlib_present::color;
use deadlib_present::space::{screen_center_x, screen_center_y, screen_height, screen_width};
use deadsync_input::{InputEvent, VirtualAction};
use deadsync_score as score_data;

const GS_LEADERBOARD_NUM_ENTRIES: usize = 13;
const GS_LEADERBOARD_ROW_HEIGHT: f32 = 24.0;
const GS_LEADERBOARD_PANE_HEIGHT: f32 = 360.0;
const GS_LEADERBOARD_PANE_WIDTH_SINGLE: f32 = 330.0;
const GS_LEADERBOARD_PANE_WIDTH_MULTI: f32 = 230.0;
const GS_LEADERBOARD_PANE_SIDE_OFFSET: f32 = 160.0;
const GS_LEADERBOARD_PANE_CENTER_Y: f32 = -15.0;
const GS_LEADERBOARD_DIM_ALPHA: f32 = 0.875;
const GS_LEADERBOARD_Z: i16 = 1480;
const GS_LEADERBOARD_HEADER_BG: [f32; 4] = color::rgba_hex("#00AEEF");
const GS_LEADERBOARD_TEXT_ZOOM: f32 = 1.0;
const GS_LEADERBOARD_ERROR_TIMEOUT: &str = "Timed Out";
const GS_LEADERBOARD_ERROR_FAILED: &str = "Failed to Load 😞";
const GS_LEADERBOARD_DISABLED_TEXT: &str = "Disabled";
const GS_LEADERBOARD_NO_SCORES_TEXT: &str = "No Scores";
const GS_LEADERBOARD_LOADING_TEXT: &str = "Loading ...";
const GS_LEADERBOARD_MACHINE_BEST: &str = "Machine's  Best";
const GS_LEADERBOARD_MORE_TEXT: &str = "More Leaderboards";
const GS_LEADERBOARD_CLOSE_HINT: &str = "Press &START; to dismiss.";
const GS_LEADERBOARD_RIVAL_COLOR: [f32; 4] = color::rgba_hex("#BD94FF");
const GS_LEADERBOARD_SELF_COLOR: [f32; 4] = color::rgba_hex("#A1FF94");

#[derive(Clone, Debug, Default)]
pub struct LeaderboardSideState {
    joined: bool,
    loading: bool,
    panes: Vec<score_data::LeaderboardPane>,
    pane_index: usize,
    show_icons: bool,
    error_text: Option<String>,
    machine_pane: Option<score_data::LeaderboardPane>,
    chart_hash: Option<String>,
    scorebox: ScoreboxSideView,
}

#[derive(Debug)]
pub struct LeaderboardOverlayStateData {
    elapsed: f32,
    p1: LeaderboardSideState,
    p2: LeaderboardSideState,
}

#[derive(Debug)]
pub enum LeaderboardOverlayState {
    Hidden,
    Visible(LeaderboardOverlayStateData),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LeaderboardInputOutcome {
    None,
    ChangedPane,
    Closed,
}

fn gs_machine_pane(entries: Vec<score_data::LeaderboardEntry>) -> score_data::LeaderboardPane {
    score_data::LeaderboardPane {
        name: GS_LEADERBOARD_MACHINE_BEST.to_string(),
        entries,
        is_ex: false,
        disabled: false,
        personalized: true,
        arrowcloud_kind: None,
    }
}

#[inline(always)]
fn should_show_overlay_pane(pane: &score_data::LeaderboardPane) -> bool {
    !pane.is_arrowcloud() || pane.is_hard_ex() || pane.personalized || !pane.entries.is_empty()
}

fn gs_error_text(error: &str) -> String {
    let lower = error.to_ascii_lowercase();
    if lower.contains("timed out") || lower.contains("timeout") {
        GS_LEADERBOARD_ERROR_TIMEOUT.to_string()
    } else {
        GS_LEADERBOARD_ERROR_FAILED.to_string()
    }
}

fn apply_leaderboard_side_snapshot(
    side: &mut LeaderboardSideState,
    snapshot: score_data::CachedPlayerLeaderboardData,
) {
    let current_pane = side.panes.get(side.pane_index).map(|pane| {
        (
            pane.name.clone(),
            pane.is_ex,
            pane.is_hard_ex(),
            pane.disabled,
            pane.personalized,
        )
    });

    if snapshot.loading {
        side.loading = true;
        side.error_text = None;
        side.show_icons = false;
        return;
    }

    side.loading = false;
    if let Some(error) = snapshot.error {
        side.error_text = Some(gs_error_text(&error));
        if side.panes.is_empty()
            && let Some(machine) = side.machine_pane.clone()
        {
            side.panes.push(machine);
        }
        side.pane_index = side.pane_index.min(side.panes.len().saturating_sub(1));
        side.show_icons = false;
        return;
    }

    let mut panes = snapshot.data.map_or_else(Vec::new, |data| {
        data.panes
            .iter()
            .filter(|pane| should_show_overlay_pane(pane))
            .cloned()
            .collect()
    });
    if let Some(machine) = side.machine_pane.clone() {
        panes.push(machine);
    }
    if panes.is_empty()
        && let Some(machine) = side.machine_pane.clone()
    {
        panes.push(machine);
    }

    side.error_text = None;
    if let Some((name, is_ex, is_hard_ex, disabled, personalized)) = current_pane {
        side.pane_index = panes
            .iter()
            .position(|pane| {
                pane.name == name
                    && pane.is_ex == is_ex
                    && pane.is_hard_ex() == is_hard_ex
                    && pane.disabled == disabled
                    && pane.personalized == personalized
            })
            .unwrap_or(side.pane_index.min(panes.len().saturating_sub(1)));
    } else {
        side.pane_index = 0;
    }
    side.show_icons = panes.len() > 1;
    side.panes = panes;
}

fn apply_leaderboard_side_view(
    side: &mut LeaderboardSideState,
    view: SelectMusicLeaderboardSideView,
) {
    if side.chart_hash != view.chart_hash {
        return;
    }

    let machine = gs_machine_pane(view.machine_entries);
    side.machine_pane = Some(machine.clone());
    let Some(snapshot) = view.leaderboards else {
        side.loading = false;
        side.error_text = None;
        side.panes.clear();
        side.panes.push(machine);
        side.pane_index = 0;
        side.show_icons = false;
        return;
    };
    apply_leaderboard_side_snapshot(side, snapshot);
}

fn overlay_display_entries(
    runtime: &ScoreboxSideView,
    pane: &score_data::LeaderboardPane,
) -> Vec<score_data::LeaderboardEntry> {
    let entries = entries_with_local_self_state(runtime, pane);
    score_data::prioritized_leaderboard_entries(entries.as_slice(), GS_LEADERBOARD_NUM_ENTRIES)
}

pub fn show_leaderboard_overlay(
    chart_hash_p1: Option<String>,
    chart_hash_p2: Option<String>,
    scoreboxes: [ScoreboxSideView; 2],
) -> Option<LeaderboardOverlayState> {
    let [scorebox_p1, scorebox_p2] = scoreboxes;
    let p1_joined = scorebox_p1.joined;
    let p2_joined = scorebox_p2.joined;
    if !p1_joined && !p2_joined {
        return None;
    }

    let mut p1 = LeaderboardSideState {
        joined: p1_joined,
        loading: p1_joined && chart_hash_p1.is_some() && scorebox_p1.groovestats_active,
        machine_pane: Some(gs_machine_pane(Vec::new())),
        chart_hash: chart_hash_p1,
        scorebox: scorebox_p1,
        ..Default::default()
    };
    let mut p2 = LeaderboardSideState {
        joined: p2_joined,
        loading: p2_joined && chart_hash_p2.is_some() && scorebox_p2.groovestats_active,
        machine_pane: Some(gs_machine_pane(Vec::new())),
        chart_hash: chart_hash_p2,
        scorebox: scorebox_p2,
        ..Default::default()
    };

    if p1_joined
        && !p1.loading
        && let Some(machine) = p1.machine_pane.clone()
    {
        p1.panes.push(machine);
    }
    if p2_joined
        && !p2.loading
        && let Some(machine) = p2.machine_pane.clone()
    {
        p2.panes.push(machine);
    }

    Some(LeaderboardOverlayState::Visible(
        LeaderboardOverlayStateData {
            elapsed: 0.0,
            p1,
            p2,
        },
    ))
}

pub fn leaderboard_runtime_request(
    state: &LeaderboardOverlayState,
) -> Option<SelectMusicLeaderboardRequest> {
    let LeaderboardOverlayState::Visible(overlay) = state else {
        return None;
    };
    Some(SelectMusicLeaderboardRequest {
        chart_hashes: [overlay.p1.chart_hash.clone(), overlay.p2.chart_hash.clone()],
        max_entries: GS_LEADERBOARD_NUM_ENTRIES,
    })
}

pub fn sync_leaderboard_overlay(
    state: &mut LeaderboardOverlayState,
    view: SelectMusicLeaderboardView,
) {
    let LeaderboardOverlayState::Visible(overlay) = state else {
        return;
    };
    let [p1, p2] = view.sides;
    if overlay.p1.joined {
        apply_leaderboard_side_view(&mut overlay.p1, p1);
    }
    if overlay.p2.joined {
        apply_leaderboard_side_view(&mut overlay.p2, p2);
    }
}

#[inline(always)]
pub fn hide_leaderboard_overlay(state: &mut LeaderboardOverlayState) {
    *state = LeaderboardOverlayState::Hidden;
}

pub fn update_leaderboard_overlay(state: &mut LeaderboardOverlayState, dt: f32) {
    let LeaderboardOverlayState::Visible(overlay) = state else {
        return;
    };
    overlay.elapsed += dt.max(0.0);
}

#[inline(always)]
fn leaderboard_shift(side: &mut LeaderboardSideState, delta: isize) -> bool {
    if side.loading || side.error_text.is_some() || side.panes.len() <= 1 {
        return false;
    }
    let prev = side.pane_index;
    let len = side.panes.len() as isize;
    side.pane_index = ((side.pane_index as isize + delta).rem_euclid(len)) as usize;
    side.pane_index != prev
}

pub fn handle_leaderboard_input(
    state: &mut LeaderboardOverlayState,
    ev: &InputEvent,
) -> LeaderboardInputOutcome {
    if !ev.pressed {
        return LeaderboardInputOutcome::None;
    }
    let LeaderboardOverlayState::Visible(overlay) = state else {
        return LeaderboardInputOutcome::None;
    };

    match ev.action {
        VirtualAction::p1_left | VirtualAction::p1_menu_left => {
            if overlay.p1.joined && leaderboard_shift(&mut overlay.p1, -1) {
                return LeaderboardInputOutcome::ChangedPane;
            }
        }
        VirtualAction::p1_right | VirtualAction::p1_menu_right => {
            if overlay.p1.joined && leaderboard_shift(&mut overlay.p1, 1) {
                return LeaderboardInputOutcome::ChangedPane;
            }
        }
        VirtualAction::p2_left | VirtualAction::p2_menu_left => {
            if overlay.p2.joined && leaderboard_shift(&mut overlay.p2, -1) {
                return LeaderboardInputOutcome::ChangedPane;
            }
        }
        VirtualAction::p2_right | VirtualAction::p2_menu_right => {
            if overlay.p2.joined && leaderboard_shift(&mut overlay.p2, 1) {
                return LeaderboardInputOutcome::ChangedPane;
            }
        }
        VirtualAction::p1_start
        | VirtualAction::p2_start
        | VirtualAction::p1_back
        | VirtualAction::p2_back
        | VirtualAction::p1_select
        | VirtualAction::p2_select => {
            hide_leaderboard_overlay(state);
            return LeaderboardInputOutcome::Closed;
        }
        _ => {}
    }

    LeaderboardInputOutcome::None
}

#[inline(always)]
fn leaderboard_icon_bounce_offset(elapsed: f32, dir: f32) -> f32 {
    let t = elapsed.rem_euclid(1.0);
    let phase = if t < 0.5 {
        let u = t / 0.5;
        1.0 - (1.0 - u) * (1.0 - u)
    } else {
        let u = (t - 0.5) / 0.5;
        1.0 - u * u
    };
    dir * 10.0 * phase
}

pub fn build_leaderboard_overlay(
    state: &LeaderboardOverlayState,
    machine_font: MachineFont,
) -> Option<Vec<Actor>> {
    let LeaderboardOverlayState::Visible(overlay) = state else {
        return None;
    };

    let mut actors = Vec::new();
    let overlay_elapsed = overlay.elapsed;
    let joined_count = overlay.p1.joined as usize + overlay.p2.joined as usize;
    let pane_width = if joined_count <= 1 {
        GS_LEADERBOARD_PANE_WIDTH_SINGLE
    } else {
        GS_LEADERBOARD_PANE_WIDTH_MULTI
    };
    let show_date = joined_count <= 1;
    let pane_cy = screen_center_y() + GS_LEADERBOARD_PANE_CENTER_Y;
    let row_center = (GS_LEADERBOARD_NUM_ENTRIES as f32 + 1.0) * 0.5;

    actors.push(act!(quad:
        align(0.0, 0.0): xy(0.0, 0.0):
        zoomto(screen_width(), screen_height()):
        diffuse(0.0, 0.0, 0.0, GS_LEADERBOARD_DIM_ALPHA):
        z(GS_LEADERBOARD_Z)
    ));
    actors.push(act!(text:
        font("miso"):
        settext(GS_LEADERBOARD_CLOSE_HINT):
        align(0.5, 0.5):
        xy(screen_center_x(), screen_height() - 50.0):
        zoom(1.1):
        diffuse(1.0, 1.0, 1.0, 1.0):
        z(GS_LEADERBOARD_Z + 1):
        horizalign(center)
    ));

    let mut draw_panel = |side: &LeaderboardSideState, center_x: f32| {
        let pane = side
            .panes
            .get(side.pane_index.min(side.panes.len().saturating_sub(1)));
        let display_entries = pane.map(|pane| overlay_display_entries(&side.scorebox, pane));
        let header_text = if side.loading {
            "GrooveStats".to_string()
        } else if let Some(p) = pane {
            p.name.replace("ITL Online", "ITL")
        } else {
            "GrooveStats".to_string()
        };
        let show_ex = !side.loading
            && side.error_text.is_none()
            && pane.is_some_and(|p| p.is_ex && !p.disabled);
        let show_itg_arrowcloud = !side.loading
            && side.error_text.is_none()
            && pane
                .is_some_and(|p| p.is_arrowcloud() && !p.is_ex && !p.is_hard_ex() && !p.disabled);
        let show_hard_ex = !side.loading
            && side.error_text.is_none()
            && pane.is_some_and(|p| p.is_hard_ex() && !p.disabled);
        let is_disabled = !side.loading && pane.is_some_and(|p| p.disabled);

        actors.push(act!(quad:
            align(0.5, 0.5):
            xy(center_x, pane_cy):
            zoomto(pane_width + 2.0, GS_LEADERBOARD_PANE_HEIGHT + 2.0):
            diffuse(1.0, 1.0, 1.0, 1.0):
            z(GS_LEADERBOARD_Z + 2)
        ));
        actors.push(act!(quad:
            align(0.5, 0.5):
            xy(center_x, pane_cy):
            zoomto(pane_width, GS_LEADERBOARD_PANE_HEIGHT):
            diffuse(0.0, 0.0, 0.0, 1.0):
            z(GS_LEADERBOARD_Z + 3)
        ));

        let header_y = pane_cy - GS_LEADERBOARD_PANE_HEIGHT * 0.5 + GS_LEADERBOARD_ROW_HEIGHT * 0.5;
        actors.push(act!(quad:
            align(0.5, 0.5):
            xy(center_x, header_y):
            zoomto(pane_width + 2.0, GS_LEADERBOARD_ROW_HEIGHT + 2.0):
            diffuse(1.0, 1.0, 1.0, 1.0):
            z(GS_LEADERBOARD_Z + 4)
        ));
        actors.push(act!(quad:
            align(0.5, 0.5):
            xy(center_x, header_y):
            zoomto(pane_width, GS_LEADERBOARD_ROW_HEIGHT):
            diffuse(
                GS_LEADERBOARD_HEADER_BG[0],
                GS_LEADERBOARD_HEADER_BG[1],
                GS_LEADERBOARD_HEADER_BG[2],
                GS_LEADERBOARD_HEADER_BG[3]
            ):
            z(GS_LEADERBOARD_Z + 5)
        ));
        actors.push(act!(text:
            font(machine_font_key(machine_font, FontRole::Header)):
            settext(header_text):
            align(0.5, 0.5):
            xy(center_x, header_y):
            zoom(0.5):
            diffuse(1.0, 1.0, 1.0, 1.0):
            z(GS_LEADERBOARD_Z + 6):
            horizalign(center)
        ));
        if show_ex {
            actors.push(act!(text:
                font(machine_font_key(machine_font, FontRole::Header)):
                settext("EX"):
                align(1.0, 0.5):
                xy(center_x + pane_width * 0.5 - 16.0, header_y):
                zoom(0.5):
                diffuse(1.0, 1.0, 1.0, 1.0):
                z(GS_LEADERBOARD_Z + 6):
                horizalign(right)
            ));
        } else if show_itg_arrowcloud {
            actors.push(act!(text:
                font(machine_font_key(machine_font, FontRole::Header)):
                settext("ITG"):
                align(1.0, 0.5):
                xy(center_x + pane_width * 0.5 - 16.0, header_y):
                zoom(0.5):
                diffuse(1.0, 1.0, 1.0, 1.0):
                z(GS_LEADERBOARD_Z + 6):
                horizalign(right)
            ));
        } else if show_hard_ex {
            actors.push(act!(text:
                font(machine_font_key(machine_font, FontRole::Header)):
                settext("H.EX"):
                align(1.0, 0.5):
                xy(center_x + pane_width * 0.5 - 16.0, header_y):
                zoom(0.5):
                diffuse(1.0, 1.0, 1.0, 1.0):
                z(GS_LEADERBOARD_Z + 6):
                horizalign(right)
            ));
        }

        let rank_x = center_x - pane_width * 0.5 + 32.0;
        let name_x = center_x - pane_width * 0.5 + 100.0;
        let score_x = if show_date {
            center_x + 63.0
        } else {
            center_x + pane_width * 0.5 - 2.0
        };
        let date_x = center_x + pane_width * 0.5 - 2.0;

        for i in 0..GS_LEADERBOARD_NUM_ENTRIES {
            let y = pane_cy + GS_LEADERBOARD_ROW_HEIGHT * ((i + 1) as f32 - row_center);
            let mut rank = String::new();
            let mut name = String::new();
            let mut score = String::new();
            let mut date = String::new();
            let mut has_highlight = false;
            let mut highlight_rgb = [0.0, 0.0, 0.0];
            let mut rank_col = [1.0, 1.0, 1.0, 1.0];
            let mut name_col = [1.0, 1.0, 1.0, 1.0];
            let mut score_col = if show_ex {
                color::JUDGMENT_RGBA[0]
            } else if show_hard_ex {
                color::HARD_EX_SCORE_RGBA
            } else {
                [1.0, 1.0, 1.0, 1.0]
            };
            let mut date_col = [1.0, 1.0, 1.0, 1.0];

            if side.loading {
                if i == 0 {
                    name = GS_LEADERBOARD_LOADING_TEXT.to_string();
                }
            } else if let Some(err) = &side.error_text {
                if i == 0 {
                    name.clone_from(err);
                }
            } else if is_disabled {
                if i == 0 {
                    name = GS_LEADERBOARD_DISABLED_TEXT.to_string();
                }
            } else if pane.is_some() {
                if let Some(entry) = display_entries.as_ref().and_then(|entries| entries.get(i)) {
                    rank = format!("{}.", entry.rank);
                    name.clone_from(&entry.name);
                    score = format!("{:.2}%", entry.score / 100.0);
                    date = score_data::format_leaderboard_date(&entry.date);

                    if entry.is_rival || entry.is_self {
                        has_highlight = true;
                        if entry.is_rival {
                            highlight_rgb = [
                                GS_LEADERBOARD_RIVAL_COLOR[0],
                                GS_LEADERBOARD_RIVAL_COLOR[1],
                                GS_LEADERBOARD_RIVAL_COLOR[2],
                            ];
                        } else {
                            highlight_rgb = [
                                GS_LEADERBOARD_SELF_COLOR[0],
                                GS_LEADERBOARD_SELF_COLOR[1],
                                GS_LEADERBOARD_SELF_COLOR[2],
                            ];
                        }
                        rank_col = [0.0, 0.0, 0.0, 1.0];
                        name_col = [0.0, 0.0, 0.0, 1.0];
                        score_col = [0.0, 0.0, 0.0, 1.0];
                        date_col = [0.0, 0.0, 0.0, 1.0];
                    }
                    if entry.is_fail {
                        score_col = [1.0, 0.0, 0.0, 1.0];
                    }
                } else if i == 0
                    && display_entries
                        .as_ref()
                        .is_none_or(|entries| entries.is_empty())
                {
                    name = GS_LEADERBOARD_NO_SCORES_TEXT.to_string();
                }
            }

            if has_highlight {
                actors.push(act!(quad:
                    align(0.5, 0.5):
                    xy(center_x, y):
                    zoomto(pane_width, GS_LEADERBOARD_ROW_HEIGHT):
                    diffuse(highlight_rgb[0], highlight_rgb[1], highlight_rgb[2], 1.0):
                    z(GS_LEADERBOARD_Z + 5)
                ));
            }

            actors.push(act!(text:
                font("miso"):
                settext(rank):
                align(1.0, 0.5):
                xy(rank_x, y):
                zoom(GS_LEADERBOARD_TEXT_ZOOM):
                maxwidth(30.0):
                diffuse(rank_col[0], rank_col[1], rank_col[2], rank_col[3]):
                z(GS_LEADERBOARD_Z + 7):
                horizalign(right)
            ));
            actors.push(act!(text:
                font("miso"):
                settext(name):
                align(0.5, 0.5):
                xy(name_x, y):
                zoom(GS_LEADERBOARD_TEXT_ZOOM):
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
                zoom(GS_LEADERBOARD_TEXT_ZOOM):
                diffuse(score_col[0], score_col[1], score_col[2], score_col[3]):
                z(GS_LEADERBOARD_Z + 7):
                horizalign(right)
            ));
            if show_date {
                actors.push(act!(text:
                    font("miso"):
                    settext(date):
                    align(1.0, 0.5):
                    xy(date_x, y):
                    zoom(GS_LEADERBOARD_TEXT_ZOOM):
                    diffuse(date_col[0], date_col[1], date_col[2], date_col[3]):
                    z(GS_LEADERBOARD_Z + 7):
                    horizalign(right)
                ));
            }
        }

        if !side.loading && side.error_text.is_none() && side.show_icons {
            let icon_y =
                pane_cy + GS_LEADERBOARD_PANE_HEIGHT * 0.5 - GS_LEADERBOARD_ROW_HEIGHT * 0.5;
            let left_dx = leaderboard_icon_bounce_offset(overlay_elapsed, 1.0);
            let right_dx = leaderboard_icon_bounce_offset(overlay_elapsed, -1.0);
            actors.push(act!(text:
                font("miso"):
                settext("&MENULEFT;"):
                align(0.5, 0.5):
                xy(center_x - pane_width * 0.5 + 10.0 + left_dx, icon_y):
                zoom(1.0):
                diffuse(1.0, 1.0, 1.0, 1.0):
                z(GS_LEADERBOARD_Z + 8):
                horizalign(center)
            ));
            actors.push(act!(text:
                font("miso"):
                settext(GS_LEADERBOARD_MORE_TEXT):
                align(0.5, 0.5):
                xy(center_x, icon_y):
                zoom(1.0):
                diffuse(1.0, 1.0, 1.0, 1.0):
                z(GS_LEADERBOARD_Z + 8):
                horizalign(center)
            ));
            actors.push(act!(text:
                font("miso"):
                settext("&MENURiGHT;"):
                align(0.5, 0.5):
                xy(center_x + pane_width * 0.5 - 10.0 + right_dx, icon_y):
                zoom(1.0):
                diffuse(1.0, 1.0, 1.0, 1.0):
                z(GS_LEADERBOARD_Z + 8):
                horizalign(center)
            ));
        }
    };

    if joined_count <= 1 {
        if overlay.p1.joined {
            draw_panel(&overlay.p1, screen_center_x());
        } else if overlay.p2.joined {
            draw_panel(&overlay.p2, screen_center_x());
        }
    } else {
        draw_panel(
            &overlay.p1,
            screen_center_x() - GS_LEADERBOARD_PANE_SIDE_OFFSET,
        );
        draw_panel(
            &overlay.p2,
            screen_center_x() + GS_LEADERBOARD_PANE_SIDE_OFFSET,
        );
    }

    Some(actors)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(rank: u32, name: &str) -> score_data::LeaderboardEntry {
        score_data::LeaderboardEntry {
            rank,
            name: name.to_string(),
            machine_tag: None,
            score: 9876.0,
            date: String::new(),
            is_rival: false,
            is_self: false,
            is_fail: false,
        }
    }

    #[test]
    fn empty_arrowcloud_hard_ex_pane_is_still_shown() {
        let pane = score_data::LeaderboardPane {
            name: "ArrowCloud".to_string(),
            entries: Vec::new(),
            is_ex: false,
            disabled: false,
            personalized: false,
            arrowcloud_kind: Some(score_data::ArrowCloudPaneKind::HardEx),
        };

        assert!(should_show_overlay_pane(&pane));
    }

    #[test]
    fn visible_overlay_requests_and_applies_shell_prepared_data() {
        let mut overlay = show_leaderboard_overlay(
            Some("chart-p1".to_string()),
            None,
            [
                ScoreboxSideView {
                    joined: true,
                    groovestats_active: true,
                    ..Default::default()
                },
                Default::default(),
            ],
        )
        .expect("joined player should open the overlay");

        let request = leaderboard_runtime_request(&overlay)
            .expect("visible overlay should request prepared leaderboard data");
        assert_eq!(request.chart_hashes[0].as_deref(), Some("chart-p1"));
        assert_eq!(request.max_entries, GS_LEADERBOARD_NUM_ENTRIES);

        sync_leaderboard_overlay(
            &mut overlay,
            SelectMusicLeaderboardView {
                sides: [
                    SelectMusicLeaderboardSideView {
                        chart_hash: Some("chart-p1".to_string()),
                        machine_entries: vec![entry(1, "AAA")],
                        leaderboards: None,
                    },
                    Default::default(),
                ],
            },
        );

        let LeaderboardOverlayState::Visible(data) = overlay else {
            panic!("overlay should remain visible");
        };
        assert!(!data.p1.loading);
        assert_eq!(data.p1.panes.len(), 1);
        assert_eq!(data.p1.panes[0].entries[0].name, "AAA");
    }
}
