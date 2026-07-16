use deadsync_config::prelude as config;
use deadsync_online::score_compat as scores;
use deadsync_profile::{PlayerSide, compat as profile};
use deadsync_score::stage_stats::StageSummary;
use deadsync_theme_simply_love::views::{PostSongPlayerView, PostSongRuntimeView};

fn player_view(side: PlayerSide) -> PostSongPlayerView {
    let player = profile::get_for_side(side);
    PostSongPlayerView {
        joined: profile::is_session_side_joined(side),
        guest: profile::is_session_side_guest(side),
        display_name: player.display_name,
        player_initials: player.player_initials,
        avatar_texture_key: player.avatar_texture_key,
        calories_burned_today: player.calories_burned_today,
        ignore_step_count_calories: player.ignore_step_count_calories,
        total_songs_played: 0,
    }
}

pub(crate) fn runtime_view() -> PostSongRuntimeView {
    let cfg = config::get();
    let session = profile::get_session_snapshot();
    PostSongRuntimeView {
        players: [player_view(PlayerSide::P1), player_view(PlayerSide::P2)],
        play_style: session.play_style,
        player_side: session.player_side,
        machine_font: cfg.machine_font,
        translated_titles: cfg.translated_titles,
        zmod_rating_box_text: cfg.zmod_rating_box_text,
        three_key_navigation: cfg.three_key_navigation,
        machine_leaderboards: Default::default(),
    }
}

pub(crate) fn gameover_runtime_view() -> PostSongRuntimeView {
    let mut view = runtime_view();
    view.players[0].total_songs_played = scores::total_songs_played_for_side(PlayerSide::P1);
    view.players[1].total_songs_played = scores::total_songs_played_for_side(PlayerSide::P2);
    view
}

pub(crate) fn initials_runtime_view(stages: &[StageSummary]) -> PostSongRuntimeView {
    let mut view = runtime_view();
    for hash in stages.iter().flat_map(|stage| {
        stage
            .players
            .iter()
            .flatten()
            .map(|player| player.chart.short_hash.as_str())
    }) {
        view.machine_leaderboards
            .entry(hash.to_owned())
            .or_insert_with(|| scores::get_machine_leaderboard_local(hash, usize::MAX));
    }
    view
}
