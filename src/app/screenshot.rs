use super::App;
use crate::screens::evaluation;
use deadlib_present::actors::Actor;
use deadlib_present::space;
use deadsync_profile::compat as profile;
use deadsync_shell::{
    AutoScreenshotEvalResult, PendingScreenshotResult, ScreenshotFlowError, ScreenshotSongInfo,
    append_screenshot_overlay_actors, capture_pending_screenshot, screenshot_preview_visible,
    screenshot_song_info,
};
use log::{info, warn};
use std::time::Instant;

fn current_song_title(state: &super::AppState) -> Option<ScreenshotSongInfo> {
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
        .evaluation_state
        .score_info
        .iter()
        .flatten()
        .next()
        .map(|info| ScreenshotSongInfo {
            title: info.song.title.clone(),
            meter: Some(info.chart.meter),
        });
    screenshot_song_info(state.screens.current_screen, gameplay, evaluation)
}

pub(super) fn auto_screenshot_eval_results(
    eval: &evaluation::State,
) -> impl Iterator<Item = AutoScreenshotEvalResult> + '_ {
    eval.score_info
        .iter()
        .flatten()
        .map(|info| AutoScreenshotEvalResult {
            personal_best: info.personal_record_highlight_rank == Some(1),
            failed: info.fail_time.is_some(),
            grade: info.grade,
        })
}

impl App {
    pub(super) fn capture_pending_screenshot(&mut self, now: Instant) {
        let song_info = current_song_title(&self.state);
        let song_info_ref = song_info
            .as_ref()
            .map(|info| (info.title.as_str(), info.meter));
        let result = capture_pending_screenshot(
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

        deadsync_audio_stream::play_sfx("assets/sounds/screenshot.ogg");
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
