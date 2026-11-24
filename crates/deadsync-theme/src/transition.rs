use crate::Theme;
use std::path::PathBuf;

/// Presentation transitions between screens supplied by a theme.
pub trait ThemeTransitions: Theme {
    type Actor;
    type EntryContext<'a>;

    fn transition_kind(from: Self::Screen, to: Self::Screen) -> TransitionKind;
    fn actor_entry_duration(screen: Self::Screen) -> Option<f32>;
    fn actor_exit_duration(screen: Self::Screen) -> f32;
    fn skip_global_entry_fade(from: Self::Screen, to: Self::Screen) -> bool;
    fn out_transition(from: Self::Screen, to: Self::Screen) -> (Vec<Self::Actor>, f32);
    fn in_transition(
        screen: Self::Screen,
        context: Self::EntryContext<'_>,
    ) -> (Vec<Self::Actor>, f32);
}

/// Presentation audio for a screen transition. The application owns stage audio.
pub trait ThemeTransitionAudio: Theme {
    type AudioContext;

    fn transition_audio(
        from: Self::Screen,
        to: Self::Screen,
        kind: TransitionKind,
        context: Self::AudioContext,
    ) -> TransitionMusic;

    fn stop_screen_sfx_on_exit(from: Self::Screen, to: Self::Screen) -> bool;
}

/// Music on the shared preview and presentation channel.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum TransitionMusic {
    #[default]
    Keep,
    Stop,
    Play {
        path: PathBuf,
        looping: bool,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransitionKind {
    Direct,
    Actors,
    Global,
}

/// An active presentation transition with its chosen duration and destination.
#[derive(Debug)]
pub enum TransitionState<S> {
    Idle,
    FadingOut {
        elapsed: f32,
        duration: f32,
        target: S,
    },
    FadingIn {
        elapsed: f32,
        duration: f32,
    },
    ActorsFadeOut {
        elapsed: f32,
        duration: f32,
        target: S,
    },
    ActorsFadeIn {
        elapsed: f32,
        duration: f32,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransitionCompletion<S> {
    GlobalFadeOut(S),
    ActorFadeOut(S),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TransitionFrame<S> {
    pub step_screen: bool,
    pub sync_input_capture: bool,
    pub completion: Option<TransitionCompletion<S>>,
}

impl<S> TransitionState<S> {
    #[inline]
    pub const fn is_idle_or_arriving(&self) -> bool {
        matches!(
            self,
            Self::Idle | Self::FadingIn { .. } | Self::ActorsFadeIn { .. }
        )
    }
}

impl<S: Copy> TransitionState<S> {
    /// Fade-in becomes idle for the next frame. Fade-out hands its
    /// destination to the shell, which completes the screen change.
    pub fn advance_frame(&mut self, delta: f32) -> TransitionFrame<S> {
        let mut frame = TransitionFrame {
            step_screen: false,
            sync_input_capture: false,
            completion: None,
        };
        match self {
            Self::Idle => frame.step_screen = true,
            Self::FadingOut {
                elapsed,
                duration,
                target,
            } => {
                *elapsed += delta;
                if *elapsed >= *duration {
                    frame.completion = Some(TransitionCompletion::GlobalFadeOut(*target));
                }
            }
            Self::ActorsFadeOut {
                elapsed,
                duration,
                target,
            } => {
                *elapsed += delta;
                if *elapsed >= *duration {
                    frame.completion = Some(TransitionCompletion::ActorFadeOut(*target));
                }
            }
            Self::FadingIn { elapsed, duration } | Self::ActorsFadeIn { elapsed, duration } => {
                *elapsed += delta;
                if *elapsed >= *duration {
                    *self = Self::Idle;
                    frame.sync_input_capture = true;
                }
            }
        }
        frame
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clock_preserves_an_opaque_destination_and_stored_duration() {
        let destination = crate::ThemeScreenId::new("alternate-theme/results");
        let mut transition = TransitionState::ActorsFadeOut {
            elapsed: 0.0,
            duration: 0.75,
            target: destination,
        };
        assert_eq!(transition.advance_frame(0.5).completion, None);
        let complete = transition.advance_frame(0.25);
        assert_eq!(
            complete.completion,
            Some(TransitionCompletion::ActorFadeOut(destination))
        );
        assert!(!complete.step_screen);
    }
}
