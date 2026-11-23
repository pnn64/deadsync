//! OpenITG-inspired presentation for DeadSync's shared game and session flow.

mod gameplay;
mod menu;
mod player_options;
mod presentation;
mod select_music;

use deadlib_present::actors::Actor;
use deadsync_profile::requests::ProfileRequest;
use deadsync_theme::{
    AudioRequest, Theme, ThemeEffect, ThemeScreenId, ThemeTransitions, TransitionKind, UiCue,
};

pub use gameplay::{GAMEPLAY_HUD, TAP_RESULTS_PNG};
pub use menu::MenuState;
pub use player_options::PlayerOptionsState;
pub use select_music::push_select_music_actors;

type Effect = ThemeEffect<OpenItgScreen, OpenItgRuntimeRequest>;

/// Presentation identifiers for the OpenITG screens and their destinations.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OpenItgScreen {
    Menu,
    Attract,
    Play,
    Options,
    Content,
    Sandbox,
    Gameplay,
    Practice,
    SongSelection,
    CourseSelection,
    PlayerOptions,
}

/// Audio and profile changes for the application to execute.
#[derive(Clone, Debug)]
pub enum OpenItgRuntimeRequest {
    Cue(UiCue),
    Audio(AudioRequest),
    Profile(ProfileRequest),
}

/// The OpenITG-inspired theme.
pub struct OpenItgTheme;

impl Theme for OpenItgTheme {
    type Screen = OpenItgScreen;
    type RuntimeRequest = OpenItgRuntimeRequest;

    fn screen_id(screen: Self::Screen) -> ThemeScreenId {
        ThemeScreenId::new(match screen {
            OpenItgScreen::Menu => "ScreenTitleMenu",
            OpenItgScreen::Attract => "ScreenAttract",
            OpenItgScreen::Play => "ScreenSelectProfile",
            OpenItgScreen::Options => "ScreenOptionsMenu",
            OpenItgScreen::Content => "ScreenContentBrowser",
            OpenItgScreen::Sandbox => "ScreenSandbox",
            OpenItgScreen::Gameplay => "ScreenGameplay",
            OpenItgScreen::Practice => "ScreenPractice",
            OpenItgScreen::SongSelection => "ScreenSelectMusic",
            OpenItgScreen::CourseSelection => "ScreenSelectCourse",
            OpenItgScreen::PlayerOptions => "ScreenPlayerOptions",
        })
    }
}

impl ThemeTransitions for OpenItgTheme {
    type Actor = Actor;
    type EntryContext<'a> = ();

    fn transition_kind(from: Self::Screen, to: Self::Screen) -> TransitionKind {
        if matches!(from, OpenItgScreen::Menu | OpenItgScreen::PlayerOptions)
            || matches!(to, OpenItgScreen::Menu | OpenItgScreen::PlayerOptions)
        {
            TransitionKind::Actors
        } else {
            TransitionKind::Direct
        }
    }

    fn actor_entry_duration(screen: Self::Screen) -> Option<f32> {
        match screen {
            OpenItgScreen::Menu => Some(menu::ENTRY_SECONDS),
            OpenItgScreen::PlayerOptions => Some(player_options::TRANSITION_SECONDS),
            _ => None,
        }
    }

    fn actor_exit_duration(screen: Self::Screen) -> f32 {
        match screen {
            OpenItgScreen::Menu => menu::EXIT_SECONDS,
            OpenItgScreen::PlayerOptions => player_options::TRANSITION_SECONDS,
            _ => 0.0,
        }
    }

    fn skip_global_entry_fade(_from: Self::Screen, _to: Self::Screen) -> bool {
        false
    }

    fn out_transition(from: Self::Screen, _to: Self::Screen) -> (Vec<Self::Actor>, f32) {
        let duration = Self::actor_exit_duration(from);
        let actors = if from == OpenItgScreen::PlayerOptions {
            vec![presentation::black_fade(duration, false)]
        } else {
            // The title actors fade themselves using the shared clock.
            Vec::new()
        };
        (actors, duration)
    }

    fn in_transition(screen: Self::Screen, (): Self::EntryContext<'_>) -> (Vec<Self::Actor>, f32) {
        let duration = Self::actor_entry_duration(screen).unwrap_or_default();
        let actors = if screen == OpenItgScreen::PlayerOptions {
            vec![presentation::black_fade(duration, true)]
        } else {
            Vec::new()
        };
        (actors, duration)
    }
}
