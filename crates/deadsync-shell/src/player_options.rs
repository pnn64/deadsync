use deadsync_chart::SongData;
use deadsync_core::input::MAX_PLAYERS;
use deadsync_profile as profile_data;
use deadsync_profile::compat as profile;
use deadsync_profile_gameplay::stage_options::{StageOptions, StageScrollMode};
use deadsync_rules::scroll::ScrollSpeedSetting;
use deadsync_theme_simply_love::views::{
    JudgmentPaletteChoiceView, PlayerOptionsInitView, PlayerOptionsPlayerView,
    PlayerOptionsPolicyView,
};

pub(crate) fn init_view() -> PlayerOptionsInitView {
    let config = deadsync_config::runtime::get();
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
            golf_enabled: config.enable_golf,
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

pub(crate) fn active_profiles() -> [profile_data::Profile; MAX_PLAYERS] {
    std::array::from_fn(|idx| profile::get_for_side(profile_data::player_side_for_index(idx)))
}

pub(crate) fn gameplay_profiles(
    mut profiles: [profile_data::Profile; MAX_PLAYERS],
    song: &SongData,
    mut stage: StageOptions,
    tournament: deadsync_config::theme::TournamentModeOptions,
    play_style: profile_data::PlayStyle,
    is_replay: bool,
) -> (
    [profile_data::Profile; MAX_PLAYERS],
    [ScrollSpeedSetting; MAX_PLAYERS],
) {
    let golf_enabled = deadsync_config::runtime::get().enable_golf;
    let scroll_mode = if is_replay {
        StageScrollMode::Replay
    } else {
        StageScrollMode::Live {
            enforce_tournament_mmod: tournament.enabled && tournament.enforce_no_cmod,
        }
    };
    stage.resolve_scroll(song, play_style, scroll_mode);
    for (idx, mut options) in stage.player_options.into_iter().enumerate() {
        apply_tournament_policy(&mut options, tournament, play_style);
        apply_golf_policy(&mut options, golf_enabled);
        gameplay_profile(
            &mut profiles[idx],
            options,
            stage.judgment_palette_ids[idx].take(),
            stage.heart_rate_device_ids[idx].take(),
        );
    }
    (profiles, stage.scroll_speeds)
}

fn apply_golf_policy(options: &mut profile_data::PlayerOptionsData, enabled: bool) {
    if !enabled {
        options.show_golf_score = false;
        options.show_golf_results = false;
    }
}

fn apply_tournament_policy(
    options: &mut profile_data::PlayerOptionsData,
    tournament: deadsync_config::theme::TournamentModeOptions,
    play_style: profile_data::PlayStyle,
) {
    if !tournament.enabled {
        return;
    }

    options.show_ex_score =
        tournament.scoring_system == deadsync_config::theme::TournamentScoringSystem::Ex;
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
    profile: &mut profile_data::Profile,
    options: profile_data::PlayerOptionsData,
    judgment_palette_id: Option<String>,
    heart_rate_device_id: Option<String>,
) {
    profile.set_current_player_options(options);
    profile.judgment_palette_id = judgment_palette_id;
    profile.heart_rate_device_id = heart_rate_device_id;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gameplay_profile_preserves_identity_while_applying_stage_options() {
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

        gameplay_profile(
            &mut profile,
            options.clone(),
            Some("palette-1".to_owned()),
            Some("hrm-1".to_owned()),
        );

        assert_eq!(profile.display_name, "Alice");
        assert_eq!(profile.current_combo, 42);
        assert_eq!(profile.current_player_options(), options);
        assert_eq!(profile.judgment_palette_id.as_deref(), Some("palette-1"));
        assert_eq!(profile.heart_rate_device_id.as_deref(), Some("hrm-1"));
    }

    #[test]
    fn golf_master_switch_disables_only_the_gameplay_copy() {
        let original = profile_data::PlayerOptionsData {
            show_golf_score: true,
            show_golf_results: true,
            golf_all_charts: true,
            golf_stroke_feedback: true,
            ..Default::default()
        };
        let mut effective = original.clone();
        apply_golf_policy(&mut effective, false);
        assert!(!effective.show_golf_score && !effective.show_golf_results);
        assert!(effective.golf_all_charts && effective.golf_stroke_feedback);
        assert!(original.show_golf_score && original.show_golf_results);

        let mut effective = original.clone();
        apply_golf_policy(&mut effective, true);
        assert_eq!(effective, original);
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
            deadsync_config::theme::TournamentModeOptions {
                enabled: true,
                scoring_system: deadsync_config::theme::TournamentScoringSystem::Ex,
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
            deadsync_config::theme::TournamentModeOptions {
                enabled: true,
                scoring_system: deadsync_config::theme::TournamentScoringSystem::Itg,
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
