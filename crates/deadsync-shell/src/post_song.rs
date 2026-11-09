use deadsync_config as config;
use deadsync_online::score_compat as scores;
use deadsync_profile::{PlayerSide, compat as profile};
use deadsync_score::stage_stats::StageSummary;
use deadsync_theme_simply_love::views::{PostSongPlayerView, PostSongRuntimeView};

fn player_view(side: PlayerSide) -> PostSongPlayerView {
    let player = profile::get_for_side(side);
    let judgment_palette = deadsync_config::judgment_palettes::runtime_catalog(
        deadsync_theme_simply_love::color::JUDGMENT_PRESET,
    )
    .resolve(player.judgment_palette_id.as_deref());
    PostSongPlayerView {
        joined: profile::is_session_side_joined(side),
        guest: profile::is_session_side_guest(side),
        name_entry_eligible: false,
        display_name: player.display_name,
        player_initials: player.player_initials,
        avatar_texture_key: player.avatar_texture_key,
        calories_burned_today: player.calories_burned_today,
        ignore_step_count_calories: player.ignore_step_count_calories,
        total_songs_played: 0,
        judgment_palette,
    }
}

pub(crate) fn runtime_view() -> PostSongRuntimeView {
    let cfg = config::runtime::get();
    let session = profile::get_session_snapshot();
    PostSongRuntimeView {
        players: [player_view(PlayerSide::P1), player_view(PlayerSide::P2)],
        play_style: session.play_style,
        player_side: session.player_side,
        machine_font: cfg.machine_font,
        translated_titles: cfg.translated_titles,
        zmod_rating_box_text: cfg.zmod_rating_box_text,
        difficulty_color_scheme: cfg.difficulty_color_scheme,
        three_key_navigation: cfg.three_key_navigation,
        srpg10_visuals: cfg.visual_style.is_srpg()
            && matches!(cfg.srpg_variant, config::theme::SrpgVariant::Srpg10),
        machine_leaderboards: std::collections::HashMap::default(),
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
    for side in [PlayerSide::P1, PlayerSide::P2] {
        let side_idx = deadsync_profile::player_side_index(side);
        let player = &view.players[side_idx];
        if !player.joined || player.guest {
            continue;
        }
        view.players[side_idx].name_entry_eligible = stages.iter().any(|stage| {
            let Some(player) = stage.players[side_idx].as_ref() else {
                return false;
            };
            let mut score = player.score_percent;
            let mut valid = player.score_valid && !player.disqualified;
            let mut failed = player.grade == deadsync_score::Grade::Failed;
            if view.play_style.is_routine() {
                // Routine saves the shared result under the master player.
                if side_idx != 0 {
                    return false;
                }
                let Some(partner) = stage.players[1].as_ref() else {
                    return false;
                };
                valid &= partner.score_valid && !partner.disqualified;
                failed |= partner.grade == deadsync_score::Grade::Failed;
                score = deadsync_rules::judgment::calculate_itg_score_percent_from_points(
                    player
                        .earned_grade_points
                        .saturating_add(partner.earned_grade_points),
                    player
                        .possible_grade_points
                        .saturating_add(partner.possible_grade_points),
                );
            }
            if !valid || !score.is_finite() || score < 0.01 {
                return false;
            }
            let hash = player.chart.short_hash.as_str();
            let machine_rank = (!failed || hash.starts_with("course-"))
                .then(|| view.machine_leaderboards.get(hash))
                .flatten()
                .and_then(|entries| deadsync_score::leaderboard_rank_for_score(entries, score));
            let personal = scores::get_personal_leaderboard_local_for_side(hash, side, usize::MAX);
            deadsync_score::name_entry_record_eligible(
                score,
                machine_rank,
                deadsync_score::leaderboard_rank_for_score(&personal, score),
            )
        });
    }
    view
}
