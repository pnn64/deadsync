use crate::Command;
use deadsync_core::input::MAX_PLAYERS;
use deadsync_profile::compat as profile;
use deadsync_profile::{self as profile_data, PlayStyle, PlayerSide, player_side_index};
use deadsync_theme_simply_love::screens::player_options::{SpeedMod, scroll_speed_for_mod};
use deadsync_theme_simply_love::views::{
    PlayerOptionsInitView, PlayerOptionsPlayerView, PlayerOptionsPolicyView,
};

pub(crate) fn init_view() -> PlayerOptionsInitView {
    let config = deadsync_config::prelude::get();
    let session = profile::get_session_snapshot();
    PlayerOptionsInitView {
        policy: PlayerOptionsPolicyView {
            allow_per_player_global_offsets: config.machine_allow_per_player_global_offsets,
            heart_rate_monitors: config.machine_enable_heart_rate_monitors,
            arcade_navigation: config.arcade_options_navigation,
            dedicated_three_key_nav: config.three_key_navigation
                && config.only_dedicated_menu_buttons,
            smx_input: config.smx_input,
            smx_panel_lights: config.smx_panel_lights,
            scorebox_available: deadsync_online::score_compat::is_gs_get_scores_service_allowed(),
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
                heart_rate_device_id: profile.heart_rate_device_id,
            }
        }),
    }
}

pub(crate) fn gameplay_profiles(
    options: &[profile_data::PlayerOptionsData; MAX_PLAYERS],
    heart_rate_device_ids: &[Option<String>; MAX_PLAYERS],
) -> [profile_data::Profile; MAX_PLAYERS] {
    std::array::from_fn(|idx| {
        gameplay_profile(
            profile::get_for_side(profile_data::player_side_for_index(idx)),
            options[idx].clone(),
            heart_rate_device_ids[idx].clone(),
        )
    })
}

fn gameplay_profile(
    mut profile: profile_data::Profile,
    options: profile_data::PlayerOptionsData,
    heart_rate_device_id: Option<String>,
) -> profile_data::Profile {
    profile.set_current_player_options(options);
    profile.heart_rate_device_id = heart_rate_device_id;
    profile
}

pub struct PlayerOptionsPersistPlan {
    pub commands: Vec<Command>,
    pub preferred_difficulty_index: usize,
}

pub fn player_options_persist_plan(
    speed_mod: &[SpeedMod; MAX_PLAYERS],
    chart_difficulty_index: [usize; MAX_PLAYERS],
    music_rate: f32,
    play_style: PlayStyle,
    player_side: PlayerSide,
) -> PlayerOptionsPersistPlan {
    let mut commands = Vec::with_capacity(MAX_PLAYERS + 2);
    match play_style {
        PlayStyle::Versus => {
            for (idx, side) in [(0, PlayerSide::P1), (1, PlayerSide::P2)] {
                commands.push(Command::UpdateScrollSpeed {
                    side,
                    setting: scroll_speed_for_mod(&speed_mod[idx]),
                });
            }
        }
        PlayStyle::Single | PlayStyle::Double => {
            let idx = player_side_index(player_side);
            commands.push(Command::UpdateScrollSpeed {
                side: player_side,
                setting: scroll_speed_for_mod(&speed_mod[idx]),
            });
        }
    }

    commands.push(Command::UpdateSessionMusicRate(music_rate));
    let preferred_difficulty_index = match play_style {
        PlayStyle::Versus => chart_difficulty_index[0],
        PlayStyle::Single | PlayStyle::Double => {
            chart_difficulty_index[player_side_index(player_side)]
        }
    };
    commands.push(Command::UpdatePreferredDifficulty(
        preferred_difficulty_index,
    ));

    PlayerOptionsPersistPlan {
        commands,
        preferred_difficulty_index,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use deadsync_rules::scroll::ScrollSpeedSetting;
    use deadsync_theme_simply_love::screens::player_options::SpeedModType;

    fn speed_mod(mod_type: SpeedModType, value: f32) -> SpeedMod {
        SpeedMod { mod_type, value }
    }

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

        let merged = gameplay_profile(profile, options.clone(), Some("hrm-1".to_owned()));

        assert_eq!(merged.display_name, "Alice");
        assert_eq!(merged.current_combo, 42);
        assert_eq!(merged.current_player_options(), options);
        assert_eq!(merged.heart_rate_device_id.as_deref(), Some("hrm-1"));
    }

    #[test]
    fn versus_persists_both_sides_and_p1_difficulty() {
        let plan = player_options_persist_plan(
            &[
                speed_mod(SpeedModType::X, 1.5),
                speed_mod(SpeedModType::M, 600.0),
            ],
            [3, 5],
            1.1,
            PlayStyle::Versus,
            PlayerSide::P1,
        );
        assert_eq!(plan.commands.len(), 4);
        assert!(matches!(
            plan.commands[0],
            Command::UpdateScrollSpeed {
                side: PlayerSide::P1,
                setting: ScrollSpeedSetting::XMod(1.5),
            }
        ));
        assert!(matches!(
            plan.commands[1],
            Command::UpdateScrollSpeed {
                side: PlayerSide::P2,
                setting: ScrollSpeedSetting::MMod(600.0),
            }
        ));
        assert_eq!(plan.preferred_difficulty_index, 3);
    }

    #[test]
    fn single_p2_persists_the_physical_side_slot() {
        let plan = player_options_persist_plan(
            &[
                speed_mod(SpeedModType::C, 300.0),
                speed_mod(SpeedModType::M, 725.0),
            ],
            [2, 6],
            1.0,
            PlayStyle::Single,
            PlayerSide::P2,
        );
        assert_eq!(plan.commands.len(), 3);
        assert!(matches!(
            plan.commands[0],
            Command::UpdateScrollSpeed {
                side: PlayerSide::P2,
                setting: ScrollSpeedSetting::MMod(725.0),
            }
        ));
        assert_eq!(plan.preferred_difficulty_index, 6);
    }
}
