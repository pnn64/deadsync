use super::*;
use crate::screens::pack_sync as shared_pack_sync;

pub(super) fn build_overlay(
    state: &crate::screens::pack_sync::OverlayState,
    active_color_index: i32,
) -> Option<Vec<Actor>> {
    shared_pack_sync::build_overlay(state, active_color_index)
}

pub(super) fn hide_overlay(state: &mut State) {
    shared_pack_sync::hide(&mut state.pack_sync_overlay);
}

pub(super) fn show_from_selected(state: &mut State) {
    let Some(MusicWheelEntry::PackHeader { name, .. }) = state.entries.get(state.selected_index)
    else {
        return;
    };
    let pack_name = name.clone();
    show_for_group(state, Some(pack_name.as_str()));
}

pub(super) fn poll(state: &mut State) -> bool {
    shared_pack_sync::poll(&mut state.pack_sync_overlay)
}

pub(super) fn handle_input(state: &mut State, ev: &InputEvent) -> ScreenAction {
    shared_pack_sync::handle_input(&mut state.pack_sync_overlay, ev)
}

fn preferred_difficulty_index(state: &State) -> usize {
    match (
        profile::get_session_play_style(),
        profile::get_session_player_side(),
    ) {
        (profile::PlayStyle::Versus, profile::PlayerSide::P2) => {
            state.p2_preferred_difficulty_index
        }
        _ => state.preferred_difficulty_index,
    }
}

fn show_for_group(state: &mut State, pack_group: Option<&str>) -> bool {
    let Some((pack_name, targets)) = pack_sync_targets_for_group(state, pack_group) else {
        return false;
    };

    clear_preview(state);
    state.song_search = sort_menu::SongSearchState::Hidden;
    state.leaderboard = sort_menu::LeaderboardOverlayState::Hidden;
    state.downloads_overlay = sort_menu::DownloadsOverlayState::Hidden;
    state.replay_overlay = sort_menu::ReplayOverlayState::Hidden;
    state.sync_overlay = SyncOverlayState::Hidden;
    state.profile_switch_overlay = None;
    hide_test_input_overlay(state);
    clear_menu_chord(state);
    clear_p1_ud_chord(state);
    clear_p2_ud_chord(state);
    clear_overlay_nav_hold(state);
    state.nav_key_held_direction = None;
    state.nav_key_held_since = None;
    state.last_steps_nav_dir_p1 = None;
    state.last_steps_nav_time_p1 = None;
    state.last_steps_nav_dir_p2 = None;
    state.last_steps_nav_time_p2 = None;

    shared_pack_sync::begin(&mut state.pack_sync_overlay, pack_name, targets)
}

fn pack_sync_targets_for_group(
    state: &State,
    pack_group: Option<&str>,
) -> Option<(String, Vec<shared_pack_sync::TargetSpec>)> {
    let pack_name = pack_group
        .unwrap_or(shared_pack_sync::ALL_LABEL)
        .to_string();
    let target_chart_type = profile::get_session_play_style().chart_type();
    let preferred_difficulty_index = preferred_difficulty_index(state);
    let mut current_pack_name: Option<&str> = None;
    let mut targets = Vec::new();

    for entry in &state.group_entries {
        match entry {
            MusicWheelEntry::PackHeader { name, .. } => current_pack_name = Some(name.as_str()),
            MusicWheelEntry::Song(song) => {
                if pack_group.is_some() && current_pack_name != pack_group {
                    continue;
                }
                let Some(steps_index) =
                    best_steps_index(song.as_ref(), target_chart_type, preferred_difficulty_index)
                else {
                    continue;
                };
                let Some(chart_ix) =
                    selected_chart_ix_for_sync(song.as_ref(), target_chart_type, steps_index)
                else {
                    continue;
                };
                let Some(chart) = song.charts.get(chart_ix) else {
                    continue;
                };
                targets.push(shared_pack_sync::TargetSpec {
                    simfile_path: song.simfile_path.clone(),
                    song_title: song.display_full_title(false),
                    chart_label: shared_pack_sync::chart_label(chart),
                    chart_ix,
                });
            }
        }
    }

    if targets.is_empty() {
        None
    } else {
        Some((pack_name, targets))
    }
}
