use super::Screen;
use crate::{SimplyLoveTheme, visual_styles};
use deadsync_assets::{audio_folder::random_music_path, resolve_asset_path};
use deadsync_config::theme::{SrpgVariant, VisualStyle};
use deadsync_theme::{ThemeTransitionAudio, TransitionAudio, TransitionKind, TransitionMusic};
use std::path::PathBuf;

#[derive(Clone, Copy)]
pub struct TransitionAudioContext {
    pub menu_music_enabled: bool,
    pub style: VisualStyle,
    pub variant: SrpgVariant,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Cue {
    Menu,
    Course,
    Credits,
    GameOver(&'static str),
}

impl Cue {
    fn path(self, context: TransitionAudioContext) -> PathBuf {
        match self {
            Self::Menu => visual_styles::resolve_menu_music_path(
                context.style,
                context.variant,
                random_music_path,
                resolve_asset_path,
            ),
            Self::Course => resolve_asset_path("assets/music/select_course (loop).ogg"),
            Self::Credits => resolve_asset_path("assets/music/credits.ogg"),
            Self::GameOver(path) => resolve_asset_path(path),
        }
    }
}

fn cue(screen: Screen, kind: TransitionKind, context: TransitionAudioContext) -> Option<Cue> {
    match screen {
        Screen::SelectColor | Screen::SelectStyle if context.menu_music_enabled => Some(Cue::Menu),
        Screen::SelectPlayMode if kind == TransitionKind::Global && context.menu_music_enabled => {
            Some(Cue::Menu)
        }
        Screen::SelectCourse if kind == TransitionKind::Global => Some(Cue::Course),
        Screen::Credits if kind == TransitionKind::Global => Some(Cue::Credits),
        Screen::GameOver => {
            visual_styles::gameover_music_asset_path(context.style, context.variant)
                .map(Cue::GameOver)
        }
        _ => None,
    }
}

fn transition_audio(
    from: Screen,
    to: Screen,
    kind: TransitionKind,
    context: TransitionAudioContext,
    resolve_cue: impl FnOnce(Cue, TransitionAudioContext) -> PathBuf,
) -> TransitionAudio {
    if kind == TransitionKind::Direct {
        return TransitionAudio::default();
    }
    let previous = cue(from, kind, context);
    let keep_preview = matches!(
        (from, to),
        (Screen::SelectMusic, Screen::PlayerOptions) | (Screen::PlayerOptions, Screen::SelectMusic)
    );
    let music = match cue(to, kind, context) {
        Some(cue) if previous != Some(cue) => TransitionMusic::Play {
            path: resolve_cue(cue, context),
            looping: !matches!(cue, Cue::GameOver(_)),
        },
        Some(_) => TransitionMusic::Keep,
        None if keep_preview => TransitionMusic::Keep,
        None => TransitionMusic::Stop,
    };
    TransitionAudio {
        music,
        stop_preview_on_exit: kind == TransitionKind::Global
            && matches!(from, Screen::SelectMusic | Screen::PlayerOptions)
            && !matches!(
                to,
                Screen::SelectMusic | Screen::PlayerOptions | Screen::SelectCourse
            ),
    }
}

impl ThemeTransitionAudio for SimplyLoveTheme {
    type AudioContext = TransitionAudioContext;

    fn transition_audio(
        from: Screen,
        to: Screen,
        kind: TransitionKind,
        context: TransitionAudioContext,
    ) -> TransitionAudio {
        transition_audio(from, to, kind, context, Cue::path)
    }

    fn stop_screen_sfx_on_exit(from: Screen, to: Screen) -> bool {
        from == Screen::Evaluation && to != Screen::Evaluation
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CONTEXT: TransitionAudioContext = TransitionAudioContext {
        menu_music_enabled: true,
        style: VisualStyle::Hearts,
        variant: SrpgVariant::Srpg9,
    };

    #[test]
    fn preview_and_continuing_menu_audio_keep_the_stream_without_resolving_assets() {
        for kind in [TransitionKind::Actors, TransitionKind::Global] {
            for (from, to) in [
                (Screen::SelectMusic, Screen::PlayerOptions),
                (Screen::PlayerOptions, Screen::SelectMusic),
                (Screen::SelectColor, Screen::SelectStyle),
            ] {
                let audio = transition_audio(from, to, kind, CONTEXT, |_, _| {
                    panic!("a continuing stream must reuse its current track")
                });
                assert_eq!(audio, TransitionAudio::default());
            }
        }
    }

    #[test]
    fn cue_starts_preserve_transition_kind_and_style() {
        for (target, kind, context, expected, looping) in [
            (
                Screen::SelectColor,
                TransitionKind::Actors,
                CONTEXT,
                Cue::Menu,
                true,
            ),
            (
                Screen::SelectPlayMode,
                TransitionKind::Global,
                CONTEXT,
                Cue::Menu,
                true,
            ),
            (
                Screen::SelectCourse,
                TransitionKind::Global,
                CONTEXT,
                Cue::Course,
                true,
            ),
            (
                Screen::Credits,
                TransitionKind::Global,
                CONTEXT,
                Cue::Credits,
                true,
            ),
            (
                Screen::GameOver,
                TransitionKind::Actors,
                TransitionAudioContext {
                    style: VisualStyle::Srpg9,
                    variant: SrpgVariant::Srpg10,
                    ..CONTEXT
                },
                Cue::GameOver(visual_styles::SRPG10_GAMEOVER_MUSIC),
                false,
            ),
        ] {
            let audio = transition_audio(Screen::Menu, target, kind, context, |cue, _| {
                assert_eq!(cue, expected);
                "selected.ogg".into()
            });
            assert_eq!(
                audio.music,
                TransitionMusic::Play {
                    path: "selected.ogg".into(),
                    looping
                }
            );
        }
        for target in [
            Screen::SelectPlayMode,
            Screen::Credits,
            Screen::SelectCourse,
            Screen::GameOver,
        ] {
            let audio = transition_audio(
                Screen::Menu,
                target,
                TransitionKind::Actors,
                CONTEXT,
                |_, _| panic!("this route must not start a track"),
            );
            assert_eq!(audio.music, TransitionMusic::Stop);
        }
    }

    #[test]
    fn global_wheel_exit_finishes_preview_and_results_owns_its_cue_cleanup() {
        let audio = transition_audio(
            Screen::PlayerOptions,
            Screen::Menu,
            TransitionKind::Global,
            CONTEXT,
            |_, _| unreachable!(),
        );
        assert_eq!(audio.music, TransitionMusic::Stop);
        assert!(audio.stop_preview_on_exit);
        assert!(SimplyLoveTheme::stop_screen_sfx_on_exit(
            Screen::Evaluation,
            Screen::Menu
        ));
        assert!(!SimplyLoveTheme::stop_screen_sfx_on_exit(
            Screen::Evaluation,
            Screen::Evaluation
        ));
    }
}
