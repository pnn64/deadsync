//! OpenITG-inspired presentation for DeadSync's shared game and session flow.

mod demonstration;
mod evaluation;
mod gameplay;
mod gameplay_outro;
mod intro;
mod menu;
mod option_explanation;
mod play_selection;
mod player_options;
mod presentation;
mod profile_picker;
mod resources;
mod select_music;
mod system_options;

use deadlib_present::actors::Actor;
use deadsync_theme::{Theme, ThemeEffect, ThemeScreenId, ThemeTransitions, TransitionKind, UiCue};

pub use evaluation::{EVALUATION_OVERVIEW_SETTLED_SECONDS, push_evaluation_overview};
pub use demonstration::{OpenItgTrainingFeedback, push_training_underlay};
pub use gameplay::GAMEPLAY_HUD;
pub use gameplay_outro::GameplayOutro;
pub use intro::{OpenItgDemonstration, OpenItgIntro, OpenItgIntroMedia};
pub use menu::MenuState;
pub use play_selection::{PlaySelectionCarousel, push_profile_load_actors};
pub use player_options::OpenItgPlayerOptions;
pub use profile_picker::push_profile_picker_actors;
pub use select_music::{
    WHEEL_BANNER_FADE, WheelBannerFrame, WheelBannerMotion, push_select_course_actors,
    push_select_music_actors,
};
pub use system_options::OpenItgSystemOptions;

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

/// UI sound requests for the application to execute.
#[derive(Clone, Debug)]
pub enum OpenItgRuntimeRequest {
    Cue(UiCue),
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
