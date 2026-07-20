use deadsync_core::input::MAX_PLAYERS;
use deadsync_profile::{PlayStyle, PlayerSide, player_side_index};
use deadsync_theme_simply_love::screens::SimplyLoveScreen as Screen;
use deadsync_theme_simply_love::screens::player_options::{SpeedMod, scroll_speed_for_mod};

use crate::{Command, TransitionMusicPaths, transition_audio_plan};

pub struct PlayerOptionsTransition<'a> {
    pub speed_mod: &'a [SpeedMod; MAX_PLAYERS],
    pub chart_difficulty_index: [usize; MAX_PLAYERS],
    pub music_rate: f32,
    pub play_style: PlayStyle,
    pub player_side: PlayerSide,
}

pub struct TransitionEffectContext<'a> {
    pub previous: Screen,
    pub target: Screen,
    pub menu_music_enabled: bool,
    pub gameover_music_enabled: bool,
    pub music_paths: TransitionMusicPaths,
    pub player_options: Option<PlayerOptionsTransition<'a>>,
    pub select_music_preferred_difficulty: Option<usize>,
}

pub struct TransitionEffectPlan {
    pub commands: Vec<Command>,
    pub stop_screen_sfx: bool,
    pub clear_play_background: bool,
    pub preferred_difficulty_index: Option<usize>,
}

pub fn transition_effect_plan(context: TransitionEffectContext<'_>) -> TransitionEffectPlan {
    let audio = transition_audio_plan(
        context.previous,
        context.target,
        context.menu_music_enabled,
        context.gameover_music_enabled,
        context.music_paths,
    );
    let mut commands = audio.commands;
    let mut preferred_difficulty_index = None;

    if matches!(
        context.previous,
        Screen::SelectMusic | Screen::PlayerOptions
    ) {
        if let Some(options) = context.player_options {
            match options.play_style {
                PlayStyle::Versus => {
                    for (index, side) in [(0, PlayerSide::P1), (1, PlayerSide::P2)] {
                        commands.push(Command::UpdateScrollSpeed {
                            side,
                            setting: scroll_speed_for_mod(&options.speed_mod[index]),
                        });
                    }
                }
                PlayStyle::Single | PlayStyle::Double => {
                    let index = player_side_index(options.player_side);
                    commands.push(Command::UpdateScrollSpeed {
                        side: options.player_side,
                        setting: scroll_speed_for_mod(&options.speed_mod[index]),
                    });
                }
            }
            commands.push(Command::UpdateSessionMusicRate(options.music_rate));
            let index = if options.play_style == PlayStyle::Versus {
                0
            } else {
                player_side_index(options.player_side)
            };
            let preferred = options.chart_difficulty_index[index];
            commands.push(Command::UpdatePreferredDifficulty(preferred));
            preferred_difficulty_index = Some(preferred);
        }

        if !matches!(
            context.target,
            Screen::SelectMusic | Screen::PlayerOptions | Screen::Gameplay | Screen::SelectCourse
        ) {
            commands.push(Command::StopMusic);
        }
    }

    if context.previous == Screen::SelectMusic {
        preferred_difficulty_index = context.select_music_preferred_difficulty;
    }

    TransitionEffectPlan {
        commands,
        stop_screen_sfx: audio.stop_screen_sfx,
        clear_play_background: audio.clear_play_background,
        preferred_difficulty_index,
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use deadsync_rules::scroll::ScrollSpeedSetting;
    use deadsync_theme_simply_love::screens::player_options::{SpeedMod, SpeedModType};

    use super::*;

    fn paths() -> TransitionMusicPaths {
        TransitionMusicPaths {
            menu: PathBuf::from("menu.ogg"),
            course: PathBuf::from("course.ogg"),
            credits: PathBuf::from("credits.ogg"),
            gameover: PathBuf::from("gameover.ogg"),
        }
    }

    fn options<'a>(speed_mod: &'a [SpeedMod; MAX_PLAYERS]) -> PlayerOptionsTransition<'a> {
        PlayerOptionsTransition {
            speed_mod,
            chart_difficulty_index: [2, 5],
            music_rate: 1.1,
            play_style: PlayStyle::Single,
            player_side: PlayerSide::P2,
        }
    }

    #[test]
    fn player_options_persists_settings_before_leaving_wheel_flow() {
        let speed_mod = [
            SpeedMod {
                mod_type: SpeedModType::X,
                value: 1.5,
            },
            SpeedMod {
                mod_type: SpeedModType::M,
                value: 650.0,
            },
        ];
        let plan = transition_effect_plan(TransitionEffectContext {
            previous: Screen::PlayerOptions,
            target: Screen::Menu,
            menu_music_enabled: false,
            gameover_music_enabled: false,
            music_paths: paths(),
            player_options: Some(options(&speed_mod)),
            select_music_preferred_difficulty: None,
        });

        assert_eq!(plan.preferred_difficulty_index, Some(5));
        assert!(plan.commands.iter().any(|command| matches!(
            command,
            Command::UpdateScrollSpeed {
                side: PlayerSide::P2,
                setting: ScrollSpeedSetting::MMod(650.0),
            }
        )));
        assert!(matches!(plan.commands.last(), Some(Command::StopMusic)));
    }

    #[test]
    fn versus_gameplay_handoff_keeps_music_and_persists_both_players_in_order() {
        let speed_mod = [
            SpeedMod {
                mod_type: SpeedModType::X,
                value: 1.5,
            },
            SpeedMod {
                mod_type: SpeedModType::M,
                value: 600.0,
            },
        ];
        let plan = transition_effect_plan(TransitionEffectContext {
            previous: Screen::PlayerOptions,
            target: Screen::Gameplay,
            menu_music_enabled: false,
            gameover_music_enabled: false,
            music_paths: paths(),
            player_options: Some(PlayerOptionsTransition {
                speed_mod: &speed_mod,
                chart_difficulty_index: [3, 5],
                music_rate: 1.1,
                play_style: PlayStyle::Versus,
                player_side: PlayerSide::P1,
            }),
            select_music_preferred_difficulty: None,
        });

        assert_eq!(plan.preferred_difficulty_index, Some(3));
        assert_eq!(plan.commands.len(), 4);
        assert!(
            !plan
                .commands
                .iter()
                .any(|command| matches!(command, Command::StopMusic))
        );
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
        assert!(matches!(
            plan.commands[2],
            Command::UpdateSessionMusicRate(1.1)
        ));
        assert!(matches!(
            plan.commands[3],
            Command::UpdatePreferredDifficulty(3)
        ));
    }

    #[test]
    fn select_music_handoff_uses_wheel_preference_and_forwards_cleanup() {
        let plan = transition_effect_plan(TransitionEffectContext {
            previous: Screen::SelectMusic,
            target: Screen::Menu,
            menu_music_enabled: false,
            gameover_music_enabled: false,
            music_paths: paths(),
            player_options: None,
            select_music_preferred_difficulty: Some(4),
        });

        assert_eq!(plan.preferred_difficulty_index, Some(4));
        assert_eq!(
            plan.commands
                .iter()
                .filter(|command| matches!(command, Command::StopMusic))
                .count(),
            2,
        );
    }
}
