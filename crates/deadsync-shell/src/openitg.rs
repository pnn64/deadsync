//! Connects mounted OpenITG screens to the application flow and services.

use crate::ThemeScreen as Screen;
use deadsync_theme::{AudioRequest, ThemeEffect};
use deadsync_theme_openitg::{OpenItgRuntimeRequest, OpenItgScreen};
use deadsync_theme_simply_love::{SimplyLoveEffect, SimplyLoveRuntimeRequest};

pub(crate) type Effect = ThemeEffect<OpenItgScreen, OpenItgRuntimeRequest>;

pub(crate) const fn host_screen(screen: OpenItgScreen) -> Screen {
    match screen {
        OpenItgScreen::Menu => Screen::Menu,
        OpenItgScreen::Attract => Screen::Init,
        OpenItgScreen::Play => Screen::SelectProfile,
        OpenItgScreen::Options => Screen::Options,
        OpenItgScreen::Content => Screen::ContentBrowser,
        OpenItgScreen::Sandbox => Screen::Sandbox,
        OpenItgScreen::Gameplay => Screen::Gameplay,
        OpenItgScreen::Practice => Screen::Practice,
        OpenItgScreen::SongSelection => Screen::SelectMusic,
        OpenItgScreen::CourseSelection => Screen::SelectCourse,
        OpenItgScreen::PlayerOptions => Screen::PlayerOptions,
    }
}

pub(crate) const fn screen_for_host(screen: Screen) -> OpenItgScreen {
    match screen {
        Screen::Init => OpenItgScreen::Attract,
        Screen::SelectProfile | Screen::SelectPlayMode | Screen::SelectStyle => OpenItgScreen::Play,
        Screen::Options => OpenItgScreen::Options,
        Screen::ContentBrowser => OpenItgScreen::Content,
        Screen::Sandbox => OpenItgScreen::Sandbox,
        Screen::Gameplay => OpenItgScreen::Gameplay,
        Screen::Practice => OpenItgScreen::Practice,
        Screen::SelectMusic => OpenItgScreen::SongSelection,
        Screen::SelectCourse => OpenItgScreen::CourseSelection,
        Screen::PlayerOptions => OpenItgScreen::PlayerOptions,
        _ => OpenItgScreen::Menu,
    }
}

pub(crate) fn host_effect(effect: Effect) -> SimplyLoveEffect {
    match effect {
        ThemeEffect::None => ThemeEffect::None,
        ThemeEffect::Batch(effects) => {
            ThemeEffect::batch(effects.into_iter().map(host_effect).collect())
        }
        ThemeEffect::Navigate(screen) => ThemeEffect::Navigate(host_screen(screen)),
        ThemeEffect::NavigateNoFade(screen) => ThemeEffect::NavigateNoFade(host_screen(screen)),
        ThemeEffect::LeaveEvaluation => ThemeEffect::LeaveEvaluation,
        ThemeEffect::Exit => ThemeEffect::Exit,
        ThemeEffect::Shutdown => ThemeEffect::Shutdown,
        ThemeEffect::Runtime(request) => ThemeEffect::Runtime(match request {
            OpenItgRuntimeRequest::Cue(cue) => {
                SimplyLoveRuntimeRequest::Audio(AudioRequest::PlayUiCue(cue))
            }
        }),
    }
}
