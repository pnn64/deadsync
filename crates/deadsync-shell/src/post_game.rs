//! Host adapter for installed session-ending presentations and game result data.

use crate::graphics::RenderAssets;
use deadlib_assets::media_path_key;
use deadlib_present::actors::Actor;
use deadsync_input::{InputEvent, VirtualAction};
use deadsync_score::{
    session_summary::{self, SessionSummary},
    stage_stats::StageSummary,
};
use deadsync_theme::app_requests::{AppEffect, ProfileRequest, RuntimeRequest};
use deadsync_theme::post_game::{
    NameEntryPage, NameEntryRecord, SessionEndPresentation, SessionSummaryPage, TimedNameEntry,
};
use deadsync_theme::{Screen, ThemeEffect, TransitionState, UiCue};
use std::sync::Arc;

pub(crate) struct SessionEndRuntime {
    session_style: deadsync_theme::select_music::SessionStyleView,
    presentation: Box<dyn SessionEndPresentation<Actor>>,
    elapsed: f32,
    summary: SessionSummary,
    banners: Vec<Arc<str>>,
    summary_return: Screen,
    editor: Option<TimedNameEntry>,
    records: [Vec<NameEntryRecord>; 2],
    diagonal: bool,
    wide: bool,
    saved: bool,
}

impl SessionEndRuntime {
    pub fn new(presentation: Box<dyn SessionEndPresentation<Actor>>) -> Self {
        Self {
            session_style: Default::default(),
            presentation,
            elapsed: 0.0,
            summary: SessionSummary::default(),
            banners: Vec::new(),
            summary_return: Screen::Initials,
            editor: None,
            records: Default::default(),
            diagonal: false,
            wide: false,
            saved: false,
        }
    }

    pub fn enter_summary(
        &mut self,
        stages: &[StageSummary],
        return_to: Screen,
        session_style: deadsync_theme::select_music::SessionStyleView,
    ) {
        self.session_style = session_style;
        self.elapsed = 0.0;
        self.summary_return = return_to;
        self.summary =
            session_summary::recent_passed(stages, self.presentation.policy().summary_history);
        self.banners = self
            .summary
            .stage_indices
            .iter()
            .map(|&index| {
                stages[index]
                    .song
                    .banner_path
                    .as_deref()
                    .map_or_else(|| Arc::from(""), media_path_key)
            })
            .collect();
    }

    pub fn enter_names(
        &mut self,
        active: [bool; 2],
        initial: [&str; 2],
        diagonal: bool,
        wide: bool,
        records: [Vec<NameEntryRecord>; 2],
    ) {
        self.elapsed = 0.0;
        self.saved = false;
        self.editor = Some(TimedNameEntry::new(
            self.presentation.policy().name_entry,
            active,
            initial,
        ));
        self.diagonal = diagonal;
        self.wide = wide;
        self.records = records;
    }

    pub fn enter_game_over(&mut self) {
        self.elapsed = 0.0;
    }

    pub fn advance(&mut self, screen: Screen, delta: f32) -> Option<AppEffect> {
        self.advance_entry(screen, delta);
        match screen {
            Screen::EvaluationSummary
                if self.summary.stage_indices.is_empty()
                    && self.summary_return == Screen::Initials =>
            {
                Some(ThemeEffect::Navigate(Screen::GameOver))
            }
            Screen::EvaluationSummary
                if self.elapsed >= self.presentation.policy().summary_seconds =>
            {
                Some(ThemeEffect::Navigate(self.summary_return))
            }
            Screen::GameOver if self.elapsed >= self.presentation.policy().game_over_seconds => {
                Some(ThemeEffect::Navigate(Screen::Init))
            }
            Screen::Initials => {
                let editor = self.editor.as_mut()?;
                if !editor.complete() || self.saved {
                    return None;
                }
                self.saved = true;
                let updates = editor.names();
                let save = if updates.iter().any(Option::is_some) {
                    ThemeEffect::Runtime(RuntimeRequest::Profile(ProfileRequest::UpdateInitials(
                        updates,
                    )))
                } else {
                    ThemeEffect::None
                };
                Some(ThemeEffect::sequence(
                    save,
                    ThemeEffect::Navigate(Screen::GameOver),
                ))
            }
            _ => None,
        }
    }

    pub fn advance_entry(&mut self, screen: Screen, delta: f32) {
        if delta.is_finite() && delta > 0.0 {
            self.elapsed += delta;
        }
        if screen == Screen::Initials
            && let Some(editor) = self.editor.as_mut()
        {
            editor.advance(delta);
        }
    }

    pub fn input(&mut self, screen: Screen, event: &InputEvent) -> AppEffect {
        if !event.pressed {
            return ThemeEffect::None;
        }
        let (side, slot, finish) = match event.action {
            VirtualAction::p1_left | VirtualAction::p1_menu_left => (0, 0, false),
            VirtualAction::p1_down | VirtualAction::p1_menu_down => (0, 1, false),
            VirtualAction::p1_up | VirtualAction::p1_menu_up => (0, 2, false),
            VirtualAction::p1_right | VirtualAction::p1_menu_right => (0, 3, false),
            VirtualAction::p2_left | VirtualAction::p2_menu_left => (1, 0, false),
            VirtualAction::p2_down | VirtualAction::p2_menu_down => (1, 1, false),
            VirtualAction::p2_up | VirtualAction::p2_menu_up => (1, 2, false),
            VirtualAction::p2_right | VirtualAction::p2_menu_right => (1, 3, false),
            VirtualAction::p1_start | VirtualAction::p1_back => (0, 0, true),
            VirtualAction::p2_start | VirtualAction::p2_back => (1, 0, true),
            VirtualAction::p1_center if self.diagonal => (0, 0, true),
            VirtualAction::p2_center if self.diagonal => (1, 0, true),
            _ => return ThemeEffect::None,
        };
        match screen {
            Screen::EvaluationSummary
                if finish && self.elapsed >= self.presentation.policy().summary_input_seconds =>
            {
                ThemeEffect::Navigate(self.summary_return)
            }
            Screen::GameOver if finish => ThemeEffect::Navigate(Screen::Init),
            Screen::Initials => {
                let Some(editor) = self.editor.as_mut() else {
                    return ThemeEffect::None;
                };
                let side = if self.wide {
                    if !finish && side != 0 {
                        return ThemeEffect::None;
                    }
                    editor
                        .players
                        .iter()
                        .position(|player| player.active)
                        .unwrap_or(side)
                } else {
                    side
                };
                let changed = if finish {
                    editor.finish(side)
                } else {
                    editor.select(side, slot)
                };
                if changed {
                    ThemeEffect::UiCue(UiCue::Confirm)
                } else {
                    ThemeEffect::None
                }
            }
            _ => ThemeEffect::None,
        }
    }

    pub fn push(
        &self,
        screen: Screen,
        actors: &mut Vec<Actor>,
        assets: &RenderAssets,
        transition: &TransitionState<Screen>,
    ) {
        match screen {
            Screen::EvaluationSummary => self.presentation.push_summary(
                actors,
                assets,
                SessionSummaryPage {
                    session_style: self.session_style,
                    results: &self.summary,
                    banners: &self.banners,
                    elapsed: self.elapsed,
                    departure_elapsed: match transition {
                        TransitionState::FadingOut { elapsed, .. }
                        | TransitionState::ActorsFadeOut { elapsed, .. } => Some(*elapsed),
                        _ => None,
                    },
                },
            ),
            Screen::Initials => {
                if let Some(editor) = &self.editor {
                    self.presentation.push_name_entry(
                        actors,
                        NameEntryPage {
                            editor,
                            diagonal: self.diagonal,
                            wide: self.wide,
                            records: self.records.each_ref().map(Vec::as_slice),
                        },
                    );
                }
            }
            Screen::GameOver => self.presentation.push_game_over(actors, self.elapsed),
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use deadsync_core::input::InputSource;
    use std::time::Instant;

    #[test]
    fn timed_name_entry_uses_arrival_time_and_saves_only_the_active_physical_side() {
        let mount = crate::theme_registry::mount(deadsync_config::theme::ThemeFlag::StepMania395);
        let mut runtime = SessionEndRuntime::new(mount.session_end.unwrap());
        runtime.enter_names([false, true], ["", "OLD"], false, false, Default::default());
        runtime.advance_entry(Screen::Initials, 1.6);
        let event = |action| {
            let now = Instant::now();
            InputEvent::new(action, 0, true, InputSource::Keyboard, now, 0, now, now)
        };
        assert!(matches!(
            runtime.input(Screen::Initials, &event(VirtualAction::p1_left)),
            ThemeEffect::None
        ));
        assert!(matches!(
            runtime.input(Screen::Initials, &event(VirtualAction::p2_left)),
            ThemeEffect::UiCue(UiCue::Confirm)
        ));
        let _ = runtime.input(Screen::Initials, &event(VirtualAction::p2_start));
        let Some(ThemeEffect::Batch(effects)) = runtime.advance(Screen::Initials, 0.0) else {
            panic!("completed ranking name should be saved before navigation");
        };
        assert!(
            matches!(&effects[0], ThemeEffect::Runtime(RuntimeRequest::Profile(ProfileRequest::UpdateInitials(names)))
            if names == &[None, Some("ALD".into())])
        );
        assert!(matches!(
            effects[1],
            ThemeEffect::Navigate(Screen::GameOver)
        ));
        assert!(runtime.advance(Screen::Initials, 0.0).is_none());
        runtime.enter_game_over();
        assert!(runtime.advance(Screen::GameOver, 5.2).is_none());
        assert!(matches!(
            runtime.advance(Screen::GameOver, 0.2),
            Some(ThemeEffect::Navigate(Screen::Init))
        ));
    }
}
