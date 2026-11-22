//! OpenITG-inspired presentation for DeadSync's shared game and session flow.

mod menu;

use deadlib_present::actors::Actor;
use deadsync_theme::{Theme, ThemeScreenId, ThemeTransitions, TransitionKind, UiCue};

pub use menu::MenuState;

/// Presentation identifiers for the title menu and its destinations.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OpenItgScreen {
    Menu,
    Attract,
    Play,
    Options,
    Content,
    Sandbox,
}

/// The OpenITG-inspired theme.
pub struct OpenItgTheme;

impl Theme for OpenItgTheme {
    type Screen = OpenItgScreen;
    type RuntimeRequest = UiCue;

    fn screen_id(screen: Self::Screen) -> ThemeScreenId {
        ThemeScreenId::new(match screen {
            OpenItgScreen::Menu => "ScreenTitleMenu",
            OpenItgScreen::Attract => "ScreenAttract",
            OpenItgScreen::Play => "ScreenSelectProfile",
            OpenItgScreen::Options => "ScreenOptionsMenu",
            OpenItgScreen::Content => "ScreenContentBrowser",
            OpenItgScreen::Sandbox => "ScreenSandbox",
        })
    }
}

impl ThemeTransitions for OpenItgTheme {
    type Actor = Actor;
    type EntryContext<'a> = ();

    fn transition_kind(from: Self::Screen, to: Self::Screen) -> TransitionKind {
        if from == OpenItgScreen::Menu || to == OpenItgScreen::Menu {
            TransitionKind::Actors
        } else {
            TransitionKind::Direct
        }
    }

    fn actor_entry_duration(screen: Self::Screen) -> Option<f32> {
        (screen == OpenItgScreen::Menu).then_some(menu::ENTRY_SECONDS)
    }

    fn actor_exit_duration(screen: Self::Screen) -> f32 {
        if screen == OpenItgScreen::Menu {
            menu::EXIT_SECONDS
        } else {
            0.0
        }
    }

    fn skip_global_entry_fade(_from: Self::Screen, _to: Self::Screen) -> bool {
        false
    }

    fn out_transition(from: Self::Screen, _to: Self::Screen) -> (Vec<Self::Actor>, f32) {
        // The title actors fade themselves using the shared clock.
        (Vec::new(), Self::actor_exit_duration(from))
    }

    fn in_transition(screen: Self::Screen, (): Self::EntryContext<'_>) -> (Vec<Self::Actor>, f32) {
        (
            Vec::new(),
            Self::actor_entry_duration(screen).unwrap_or_default(),
        )
    }
}
