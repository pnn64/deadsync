use deadsync_config::prelude as config;
use deadsync_profile::{PlayerSide, compat as profile};
use deadsync_theme_simply_love::views::{SelectFlowPlayerView, SelectFlowRuntimeView};

fn player_view(side: PlayerSide) -> SelectFlowPlayerView {
    let player = profile::get_for_side(side);
    SelectFlowPlayerView {
        joined: profile::is_session_side_joined(side),
        guest: profile::is_session_side_guest(side),
        display_name: player.display_name,
        avatar_texture_key: player.avatar_texture_key,
    }
}

pub(crate) fn players_view() -> [SelectFlowPlayerView; 2] {
    [player_view(PlayerSide::P1), player_view(PlayerSide::P2)]
}

pub(crate) fn runtime_view() -> SelectFlowRuntimeView {
    SelectFlowRuntimeView {
        players: players_view(),
        play_style: profile::get_session_play_style(),
        play_mode: profile::get_session_play_mode(),
        color_index: config::get().simply_love_color,
    }
}
