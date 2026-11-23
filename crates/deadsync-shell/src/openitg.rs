//! Connects mounted OpenITG screens to the application flow and services.

use std::collections::HashMap;
use std::sync::Arc;

use crate::ThemeScreen as Screen;
use crate::TransitionState;
use deadsync_gameplay::ExitTransitionKind;
use deadsync_theme::views::{AttractRecord, GameplayHudMotion};
use deadsync_theme::{AudioRequest, ThemeEffect};
use deadsync_theme_openitg::{
    OpenItgIntro, OpenItgIntroMedia, OpenItgRuntimeRequest, OpenItgScreen,
};
use deadsync_theme_simply_love::{SimplyLoveEffect, SimplyLoveRuntimeRequest};

pub(crate) type Effect = ThemeEffect<OpenItgScreen, OpenItgRuntimeRequest>;

#[derive(Default)]
pub(crate) struct IntroRuntime {
    pub timeline: OpenItgIntro,
    pub video_key: Option<Arc<str>>,
    current_media: Option<OpenItgIntroMedia>,
}

impl IntroRuntime {
    pub fn update_media(&mut self) -> Option<OpenItgIntroMedia> {
        let media = self.timeline.media();
        if self.current_media == Some(media) {
            return None;
        }
        self.current_media = Some(media);
        Some(media)
    }
}

pub(crate) fn recent_attract_records() -> Vec<AttractRecord> {
    const MAX_RECORDS: usize = 40;
    let recent = deadsync_profile::app_runtime::recent_played_chart_hashes_for_machine();
    let recent = &recent[..recent.len().min(MAX_RECORDS)];
    let mut remaining: HashMap<_, _> = recent
        .iter()
        .enumerate()
        .map(|(index, hash)| (hash.as_str(), index))
        .collect();
    let mut chart_labels = vec![None; recent.len()];
    if !remaining.is_empty() {
        let library = deadsync_simfile::runtime_cache::get_song_cache();
        'songs: for song in library.iter().flat_map(|pack| &pack.songs) {
            for chart in &song.charts {
                if let Some(index) = remaining.remove(chart.short_hash.as_str()) {
                    chart_labels[index] = Some((
                        Arc::from(song.title.as_str()),
                        Arc::from(chart.difficulty.as_str()),
                    ));
                    if remaining.is_empty() {
                        break 'songs;
                    }
                }
            }
        }
    }
    recent
        .iter()
        .zip(chart_labels)
        .filter_map(|(hash, labels)| {
            let (title, difficulty) = labels?;
            let (initials, score) = deadsync_profile::app_runtime::machine_record_local(hash)?;
            Some(AttractRecord {
                title,
                difficulty,
                initials,
                percent: score.score_percent * 100.0,
            })
        })
        .collect()
}

pub(crate) fn gameplay_hud_motion(
    state: &deadsync_theme_simply_love::screens::gameplay::State,
    transition: &TransitionState,
    restarting: bool,
) -> GameplayHudMotion {
    if let Some(out) = state.stage.song_lua.out_transition() {
        return GameplayHudMotion::Leaving {
            elapsed_seconds: out.elapsed_seconds(),
        };
    }
    if let Some(out) = state.exit_prompt_state().exit_transition
        && out.kind == ExitTransitionKind::Out
    {
        return GameplayHudMotion::Leaving {
            elapsed_seconds: out.started_at.elapsed().as_secs_f32(),
        };
    }
    match *transition {
        TransitionState::FadingOut {
            elapsed,
            target: Screen::Evaluation,
            ..
        } => GameplayHudMotion::Leaving {
            elapsed_seconds: elapsed,
        },
        TransitionState::FadingIn { elapsed, .. }
            if !restarting && !state.presentation.hide_song_intro =>
        {
            GameplayHudMotion::Entering {
                elapsed_seconds: elapsed,
            }
        }
        _ => GameplayHudMotion::Settled,
    }
}

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
