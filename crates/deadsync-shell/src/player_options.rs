use deadsync_core::input::MAX_PLAYERS;
use deadsync_profile as profile_data;
use deadsync_profile::compat as profile;
use deadsync_theme_simply_love::views::{
    JudgmentPaletteChoiceView, PlayerOptionsInitView, PlayerOptionsPlayerView,
    PlayerOptionsPolicyView,
};

pub(crate) fn init_view() -> PlayerOptionsInitView {
    let config = deadsync_config::prelude::get();
    let session = profile::get_session_snapshot();
    let palette_catalog = deadsync_config::judgment_palettes::runtime_catalog(
        deadsync_theme_simply_love::color::JUDGMENT_PRESET,
    );
    PlayerOptionsInitView {
        policy: PlayerOptionsPolicyView {
            allow_per_player_global_offsets: config.machine_allow_per_player_global_offsets,
            heart_rate_monitors: config.machine_enable_heart_rate_monitors,
            arcade_navigation: config.arcade_options_navigation,
            machine_font: config.machine_font,
            smx_input: config.smx_input,
            smx_panel_lights: config.smx_panel_lights,
            scorebox_available: deadsync_online::score_compat::is_gs_get_scores_service_allowed(),
            keyboard_features: config.keyboard_features,
            tournament_mode: config.tournament.enabled,
        },
        play_style: session.play_style,
        player_side: session.player_side,
        joined: std::array::from_fn(|idx| {
            session.side_joined(profile_data::player_side_for_index(idx))
        }),
        music_rate: session.music_rate,
        players: std::array::from_fn(|idx| {
            let profile = profile::get_for_side(profile_data::player_side_for_index(idx));
            PlayerOptionsPlayerView {
                options: profile.current_player_options(),
                judgment_palette_id: profile.judgment_palette_id,
                heart_rate_device_id: profile.heart_rate_device_id,
                max_heart_rate: profile.max_heart_rate,
            }
        }),
        judgment_palettes: palette_catalog
            .palettes
            .iter()
            .map(|entry| JudgmentPaletteChoiceView {
                id: entry.id.clone(),
                name: entry.name.clone(),
                palette: entry.palette,
            })
            .collect(),
    }
}

pub(crate) fn gameplay_profiles(
    options: &[profile_data::PlayerOptionsData; MAX_PLAYERS],
    judgment_palette_ids: &[Option<String>; MAX_PLAYERS],
    heart_rate_device_ids: &[Option<String>; MAX_PLAYERS],
    tournament: deadsync_config::prelude::TournamentModeOptions,
    play_style: profile_data::PlayStyle,
) -> [profile_data::Profile; MAX_PLAYERS] {
    std::array::from_fn(|idx| {
        let mut options = options[idx].clone();
        apply_tournament_policy(&mut options, tournament, play_style);
        gameplay_profile(
            profile::get_for_side(profile_data::player_side_for_index(idx)),
            options,
            judgment_palette_ids[idx].clone(),
            heart_rate_device_ids[idx].clone(),
        )
    })
}

fn apply_tournament_policy(
    options: &mut profile_data::PlayerOptionsData,
    tournament: deadsync_config::prelude::TournamentModeOptions,
    play_style: profile_data::PlayStyle,
) {
    if !tournament.enabled {
        return;
    }

    options.show_ex_score =
        tournament.scoring_system == deadsync_config::prelude::TournamentScoringSystem::Ex;
    options.show_hard_ex_score = false;
    options.show_fa_plus_pane = true;
    if tournament.show_step_stats {
        options.step_statistics = profile_data::StepStatisticsMask::all_widgets();
        if play_style.is_versus() {
            options.score_position = profile_data::ScorePosition::StepStatistics;
        }
    } else {
        options.step_statistics = profile_data::StepStatisticsMask::empty();
        options.score_position = profile_data::ScorePosition::Normal;
    }
}

fn gameplay_profile(
    mut profile: profile_data::Profile,
    options: profile_data::PlayerOptionsData,
    judgment_palette_id: Option<String>,
    heart_rate_device_id: Option<String>,
) -> profile_data::Profile {
    profile.set_current_player_options(options);
    profile.judgment_palette_id = judgment_palette_id;
    profile.heart_rate_device_id = heart_rate_device_id;
    profile
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gameplay_profile_preserves_identity_while_applying_screen_edits() {
        let mut profile = profile_data::Profile {
            display_name: "Alice".to_owned(),
            current_combo: 42,
            ..Default::default()
        };
        profile.mini_percent = 5;
        let options = profile_data::PlayerOptionsData {
            mini_percent: 37,
            ..Default::default()
        };

        let merged = gameplay_profile(
            profile,
            options.clone(),
            Some("palette-1".to_owned()),
            Some("hrm-1".to_owned()),
        );

        assert_eq!(merged.display_name, "Alice");
        assert_eq!(merged.current_combo, 42);
        assert_eq!(merged.current_player_options(), options);
        assert_eq!(merged.judgment_palette_id.as_deref(), Some("palette-1"));
        assert_eq!(merged.heart_rate_device_id.as_deref(), Some("hrm-1"));
    }

    #[test]
    fn tournament_policy_overrides_only_the_gameplay_copy() {
        let original = profile_data::PlayerOptionsData {
            show_ex_score: false,
            show_hard_ex_score: true,
            show_fa_plus_pane: false,
            score_position: profile_data::ScorePosition::Normal,
            ..Default::default()
        };
        let mut effective = original.clone();

        apply_tournament_policy(
            &mut effective,
            deadsync_config::prelude::TournamentModeOptions {
                enabled: true,
                scoring_system: deadsync_config::prelude::TournamentScoringSystem::Ex,
                show_step_stats: true,
                enforce_no_cmod: true,
            },
            profile_data::PlayStyle::Versus,
        );

        assert!(effective.show_ex_score);
        assert!(!effective.show_hard_ex_score);
        assert!(effective.show_fa_plus_pane);
        assert_eq!(
            effective.step_statistics,
            profile_data::StepStatisticsMask::all_widgets()
        );
        assert_eq!(
            effective.score_position,
            profile_data::ScorePosition::StepStatistics
        );
        assert!(!original.show_ex_score);
        assert!(original.show_hard_ex_score);
        assert!(!original.show_fa_plus_pane);
    }

    #[test]
    fn tournament_hide_stats_uses_itg_score_and_normal_position() {
        let mut effective = profile_data::PlayerOptionsData {
            show_ex_score: true,
            show_hard_ex_score: true,
            step_statistics: profile_data::StepStatisticsMask::all_widgets(),
            score_position: profile_data::ScorePosition::StepStatistics,
            ..Default::default()
        };

        apply_tournament_policy(
            &mut effective,
            deadsync_config::prelude::TournamentModeOptions {
                enabled: true,
                scoring_system: deadsync_config::prelude::TournamentScoringSystem::Itg,
                show_step_stats: false,
                enforce_no_cmod: false,
            },
            profile_data::PlayStyle::Single,
        );

        assert!(!effective.show_ex_score);
        assert!(!effective.show_hard_ex_score);
        assert!(effective.step_statistics.is_empty());
        assert_eq!(
            effective.score_position,
            profile_data::ScorePosition::Normal
        );
    }
}
