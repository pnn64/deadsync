use deadsync_core::input::MAX_PLAYERS;
use deadsync_profile::{PlayStyle, PlayerSide, player_side_index};
use deadsync_rules::scroll::ScrollSpeedSetting;
use deadsync_theme::{
    Screen, ThemeTransitionAudio, TransitionAudio, TransitionKind, TransitionMusic,
};

use crate::Command;

pub struct PlayerOptionsTransition {
    pub scroll_speed: [ScrollSpeedSetting; MAX_PLAYERS],
    pub chart_difficulty_index: [usize; MAX_PLAYERS],
    pub music_rate: f32,
    pub play_style: PlayStyle,
    pub player_side: PlayerSide,
}

pub struct TransitionEffectContext {
    pub previous: Screen,
    pub target: Screen,
    pub entry_music: Option<TransitionMusic>,
    pub player_options: Option<PlayerOptionsTransition>,
    pub select_music_preferred_difficulty: Option<usize>,
}

pub struct TransitionEffectPlan {
    pub commands: Vec<Command>,
    pub stop_screen_sfx: bool,
    pub clear_play_background: bool,
    pub preferred_difficulty_index: Option<usize>,
}

pub fn transition_music_command(music: TransitionMusic) -> Option<Command> {
    match music {
        TransitionMusic::Keep => None,
        TransitionMusic::Stop => Some(Command::StopMusic),
        TransitionMusic::Play { path, looping } => Some(Command::PlayMusic {
            path,
            looped: looping,
            volume: 1.0,
        }),
    }
}

pub fn transition_effect_plan<T: ThemeTransitionAudio<Screen = Screen>>(
    context: TransitionEffectContext,
    audio_context: T::AudioContext,
) -> TransitionEffectPlan {
    let entering_gameplay = context.target == Screen::Gameplay;
    let audio = if entering_gameplay {
        // The stage controller takes over the stream when its clock starts.
        TransitionAudio::default()
    } else {
        T::transition_audio(
            context.previous,
            context.target,
            TransitionKind::Global,
            audio_context,
        )
    };
    let entry_music = context.entry_music.filter(|_| {
        !entering_gameplay && context.target != Screen::Practice
    });
    let replaces_preview = matches!(entry_music, Some(TransitionMusic::Play { .. }));
    let music = if context.target == Screen::Practice {
        TransitionMusic::Stop
    } else {
        entry_music.unwrap_or(audio.music)
    };
    let previous_is_play = context.previous.is_play();
    let clear_play_background = previous_is_play && !context.target.is_play();
    let stop_stage_music = clear_play_background && !matches!(music, TransitionMusic::Play { .. });
    let mut commands = Vec::new();
    if let Some(command) = transition_music_command(music) {
        commands.push(command);
    }
    if stop_stage_music {
        commands.push(Command::StopMusic);
    }
    let mut preferred_difficulty_index = None;

    if context.previous == Screen::SelectMusic
        || context.previous == Screen::PlayerOptions
    {
        if let Some(options) = context.player_options {
            match options.play_style {
                PlayStyle::Versus
                | PlayStyle::PumpVersus
                | PlayStyle::Couple
                | PlayStyle::PumpCouple
                | PlayStyle::Routine
                | PlayStyle::PumpRoutine => {
                    for (index, side) in [(0, PlayerSide::P1), (1, PlayerSide::P2)] {
                        commands.push(Command::UpdateScrollSpeed {
                            side,
                            setting: options.scroll_speed[index],
                        });
                    }
                }
                PlayStyle::Single
                | PlayStyle::Double
                | PlayStyle::PumpSingle
                | PlayStyle::PumpDouble => {
                    let index = player_side_index(options.player_side);
                    commands.push(Command::UpdateScrollSpeed {
                        side: options.player_side,
                        setting: options.scroll_speed[index],
                    });
                }
            }
            commands.push(Command::UpdateSessionMusicRate(options.music_rate));
            let index = if options.play_style.is_versus() {
                0
            } else {
                player_side_index(options.player_side)
            };
            let preferred = options.chart_difficulty_index[index];
            commands.push(Command::UpdatePreferredDifficulty(preferred));
            preferred_difficulty_index = Some(preferred);
        }
    }
    if audio.stop_preview_on_exit && !replaces_preview {
        commands.push(Command::StopMusic);
    }

    if context.previous == Screen::SelectMusic {
        preferred_difficulty_index = context.select_music_preferred_difficulty;
    }

    TransitionEffectPlan {
        commands,
        stop_screen_sfx: context.previous == Screen::Gameplay
            || (!previous_is_play && T::stop_screen_sfx_on_exit(context.previous, context.target)),
        clear_play_background,
        preferred_difficulty_index,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ActiveTheme;
    use deadsync_config::theme::{SrpgVariant, VisualStyle};
    use deadsync_theme::{Theme, ThemeScreenId};
    use deadsync_theme_simply_love::screens::transition_audio::TransitionAudioContext;

    const AUDIO_CONTEXT: TransitionAudioContext = TransitionAudioContext {
        menu_music_enabled: false,
        style: VisualStyle::Hearts,
        variant: SrpgVariant::Srpg9,
    };

    fn plan(context: TransitionEffectContext) -> TransitionEffectPlan {
        transition_effect_plan::<ActiveTheme>(context, AUDIO_CONTEXT)
    }

    #[test]
    fn player_options_persists_settings_before_leaving_wheel_flow() {
        let plan = plan(TransitionEffectContext {
            entry_music: None,
            previous: Screen::PlayerOptions,
            target: Screen::Menu,
            player_options: Some(PlayerOptionsTransition {
                scroll_speed: [
                    ScrollSpeedSetting::XMod(1.5),
                    ScrollSpeedSetting::MMod(650.0),
                ],
                chart_difficulty_index: [2, 5],
                music_rate: 1.1,
                play_style: PlayStyle::Single,
                player_side: PlayerSide::P2,
            }),
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
        let plan = plan(TransitionEffectContext {
            entry_music: None,
            previous: Screen::PlayerOptions,
            target: Screen::Gameplay,
            player_options: Some(PlayerOptionsTransition {
                scroll_speed: [
                    ScrollSpeedSetting::XMod(1.5),
                    ScrollSpeedSetting::MMod(600.0),
                ],
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
        let plan = plan(TransitionEffectContext {
            entry_music: None,
            previous: Screen::SelectMusic,
            target: Screen::Menu,
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

    #[test]
    fn gameplay_exit_stops_outgoing_music_and_clears_stage_media() {
        let plan = plan(TransitionEffectContext {
            entry_music: None,
            previous: Screen::Gameplay,
            target: Screen::Evaluation,
            player_options: None,
            select_music_preferred_difficulty: None,
        });
        assert!(plan.clear_play_background);
        assert!(plan.stop_screen_sfx);
        assert!(matches!(
            plan.commands.as_slice(),
            [Command::StopMusic, Command::StopMusic]
        ));
    }

    #[test]
    fn evaluation_loop_replaces_song_and_preview_audio() {
        for previous in [Screen::Gameplay, Screen::SelectMusic, Screen::PlayerOptions] {
            let plan = plan(TransitionEffectContext {
                previous,
                target: Screen::Evaluation,
                entry_music: Some(TransitionMusic::Play {
                    path: "results.ogg".into(),
                    looping: true,
                }),
                player_options: None,
                select_music_preferred_difficulty: None,
            });
            assert_eq!(plan.clear_play_background, previous == Screen::Gameplay);
            assert_eq!(plan.stop_screen_sfx, previous == Screen::Gameplay);
            assert!(matches!(
                plan.commands.as_slice(),
                [Command::PlayMusic { path, looped: true, .. }] if path == std::path::Path::new("results.ogg")
            ));
        }
    }

    struct CueTheme;

    impl Theme for CueTheme {
        type Screen = Screen;
        type RuntimeRequest = ();

        fn screen_id(screen: Screen) -> ThemeScreenId {
            ActiveTheme::screen_id(screen)
        }
    }

    impl ThemeTransitionAudio for CueTheme {
        type AudioContext = ();

        fn transition_audio(
            _: Screen,
            target: Screen,
            _: TransitionKind,
            _: (),
        ) -> TransitionAudio {
            assert_ne!(target, Screen::Gameplay);
            TransitionAudio {
                music: TransitionMusic::Play {
                    path: "alternate.ogg".into(),
                    looping: true,
                },
                stop_preview_on_exit: false,
            }
        }

        fn stop_screen_sfx_on_exit(_: Screen, _: Screen) -> bool {
            false
        }
    }

    #[test]
    fn alternate_theme_music_plays_after_stage_exit_and_keeps_stage_entry_audio_owned_by_game() {
        for target in [Screen::Gameplay, Screen::Practice, Screen::Evaluation] {
            let plan = transition_effect_plan::<CueTheme>(
                TransitionEffectContext {
                    entry_music: None,
                    previous: Screen::Gameplay,
                    target,
                    player_options: None,
                    select_music_preferred_difficulty: None,
                },
                (),
            );
            assert!(plan.stop_screen_sfx);
            match target {
                Screen::Gameplay => assert!(plan.commands.is_empty()),
                Screen::Practice => {
                    assert!(matches!(plan.commands.as_slice(), [Command::StopMusic]))
                }
                Screen::Evaluation => {
                    assert!(plan.clear_play_background);
                    assert!(
                        matches!(plan.commands.as_slice(), [Command::PlayMusic { path, looped: true, .. }] if path == std::path::Path::new("alternate.ogg"))
                    );
                }
                _ => unreachable!(),
            }
        }
    }
}
