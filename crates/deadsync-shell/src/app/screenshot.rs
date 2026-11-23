use super::{App, AppState, CurrentScreen, MAX_PLAYERS};
use crate::ActiveTheme;
use crate::screenshot::{
    AutoScreenshotEvalResult, AutoScreenshotFrameContext, PendingScreenshotResult,
    ScreenshotFlowError, ScreenshotSongInfo, append_screenshot_overlay_actors,
    auto_screenshot_frame_plan, capture_pending_screenshot, screenshot_preview_visible,
    screenshot_song_info,
};
use deadlib_present::actors::Actor;
use deadlib_present::space;
use deadsync_profile::compat as profile;
use deadsync_theme::evaluation::ThemeEvaluation;
use deadsync_theme::views::EvaluationView;
use log::{info, warn};
use std::time::Instant;

pub(super) fn current_song_title(state: &AppState) -> Option<ScreenshotSongInfo> {
    let gameplay = state.screens.gameplay_state.as_ref().map(|gs| {
        let title = gs.gameplay.song().title.clone();
        let meter = gs
            .gameplay
            .charts()
            .iter()
            .find(|chart| chart.meter > 0)
            .map(|chart| chart.meter);
        ScreenshotSongInfo { title, meter }
    });
    let evaluation = state
        .screens
        .evaluation_stage
        .as_ref()
        .map(|stage| ScreenshotSongInfo {
            title: stage.song.title.clone(),
            meter: stage.players().next().map(|player| player.chart.meter),
        });
    screenshot_song_info(state.screens.current_screen, gameplay, evaluation)
}

/// Host-owned capture facts copied before the result is passed to presentation.
#[derive(Clone, Copy, Default)]
pub(super) struct EvaluationCapture {
    results: [Option<AutoScreenshotEvalResult>; MAX_PLAYERS],
    taken: bool,
}

impl EvaluationCapture {
    pub(super) fn from_scores<N, S>(scores: &[Option<EvaluationView<N, S>>; MAX_PLAYERS]) -> Self {
        Self {
            results: scores.each_ref().map(|score| {
                score.as_ref().map(|info| AutoScreenshotEvalResult {
                    personal_best: info.personal_record_highlight_rank == Some(1),
                    failed: info.fail_time.is_some(),
                    grade: info.grade,
                })
            }),
            taken: false,
        }
    }
}

impl App {
    pub(super) fn update_evaluation_capture(&mut self, mask: u8) {
        let screens = &mut self.state.screens;
        let on_evaluation = screens.current_screen == CurrentScreen::Evaluation;
        let overview_revealing = on_evaluation
            && screens.menu_state.theme_flag() == deadsync_config::theme::ThemeFlag::OpenItg
            && screens.evaluation_state.screen_elapsed
                < deadsync_theme_openitg::EVALUATION_OVERVIEW_SETTLED_SECONDS
            && super::evaluation::overview_pane_selected(&screens.evaluation_state);
        let capture = &mut screens.evaluation_capture;
        let plan = auto_screenshot_frame_plan(
            AutoScreenshotFrameContext {
                screen: screens.current_screen,
                already_taken: capture.taken,
                ready: on_evaluation
                    && !overview_revealing
                    && ActiveTheme::evaluation_capture_ready(&screens.evaluation_state),
                mask: if on_evaluation { mask } else { 0 },
            },
            capture.results.iter().flatten().copied(),
        );
        if plan.mark_taken {
            capture.taken = true;
        }
        if plan.request_capture {
            self.state.shell.screenshot.request(None);
        }
    }

    pub(super) fn capture_pending_screenshot(&mut self, now: Instant) {
        // Most frames have no request; skip copying the song title for them.
        if !self.state.shell.screenshot.pending() {
            return;
        }
        let song_info = current_song_title(&self.state);
        let song_info_ref = song_info
            .as_ref()
            .map(|info| (info.title.as_str(), info.meter));
        let result = capture_pending_screenshot(
            &self.screenshots_dir,
            &mut self.state.shell.screenshot,
            self.backend.as_mut(),
            &mut self.asset_manager,
            song_info_ref,
            now,
            screenshot_preview_visible(self.state.screens.current_screen),
            |side| profile::is_session_side_joined(side) && !profile::is_session_side_guest(side),
        );

        let saved_path = match result {
            Ok(PendingScreenshotResult::NoRequest | PendingScreenshotResult::NoBackend) => return,
            Ok(PendingScreenshotResult::Saved {
                path,
                preview_error,
            }) => {
                if let Some(error) = preview_error {
                    warn!("Failed to create screenshot preview texture: {error}");
                }
                path
            }
            Err(ScreenshotFlowError::Capture(error)) => {
                warn!(
                    "Screenshot capture unavailable for renderer {}: {error}",
                    self.backend_type
                );
                return;
            }
            Err(ScreenshotFlowError::Save(error)) => {
                warn!("Failed to save screenshot: {error}");
                return;
            }
            Err(ScreenshotFlowError::PreviewTexture(error)) => {
                warn!("Unexpected screenshot preview error during capture: {error}");
                return;
            }
        };

        self.ui_sfx
            .play_cue(&mut self.audio, deadsync_theme::UiCue::Screenshot);
        info!("Saved screenshot to {}", saved_path.display());
    }

    pub(super) fn append_screenshot_overlay_actors(&self, actors: &mut Vec<Actor>, now: Instant) {
        append_screenshot_overlay_actors(
            &self.state.shell.screenshot,
            screenshot_preview_visible(self.state.screens.current_screen),
            actors,
            now,
            space::screen_width(),
            space::screen_height(),
        );
    }
}
