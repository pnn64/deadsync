use super::{App, CurrentScreen};
use crate::screens::evaluation;
use deadlib_present::actors::Actor;
use deadlib_present::space;
use deadsync_profile::compat as profile;
use deadsync_shell::{
    ScreenshotFlowError, append_screenshot_overlay_actors, capture_screenshot,
    replace_screenshot_preview_texture, screenshot_preview_target,
};
use log::{info, warn};
use std::time::Instant;

fn current_song_title(state: &super::AppState) -> Option<(String, Option<u32>)> {
    match state.screens.current_screen {
        CurrentScreen::Gameplay => state.screens.gameplay_state.as_ref().map(|gs| {
            let title = gs.gameplay.song().title.clone();
            let meter = gs
                .gameplay
                .charts()
                .iter()
                .find(|chart| chart.meter > 0)
                .map(|chart| chart.meter);
            (title, meter)
        }),
        CurrentScreen::Evaluation => state
            .screens
            .evaluation_state
            .score_info
            .iter()
            .flatten()
            .next()
            .map(|info| (info.song.title.clone(), Some(info.chart.meter))),
        _ => None,
    }
    .filter(|(title, _)| !title.is_empty())
}

pub(super) fn should_auto_screenshot_eval(eval: &evaluation::State, mask: u8) -> bool {
    if mask == 0 {
        return false;
    }
    eval.score_info.iter().flatten().any(|info| {
        crate::config::auto_screenshot_eval_matches(
            mask,
            info.personal_record_highlight_rank == Some(1),
            info.fail_time.is_some(),
            matches!(info.grade, deadsync_score::Grade::Tier01),
            matches!(info.grade, deadsync_score::Grade::Quint),
        )
    })
}

impl App {
    pub(super) fn capture_pending_screenshot(&mut self, now: Instant) {
        let Some(request_side) = self.state.shell.screenshot.take_pending_request() else {
            return;
        };
        let song_info = current_song_title(&self.state);
        let song_info_ref = song_info
            .as_ref()
            .map(|(title, meter)| (title.as_str(), *meter));
        let result = {
            let Some(backend) = self.backend.as_mut() else {
                return;
            };
            capture_screenshot(backend, song_info_ref)
        };

        let saved = match result {
            Ok(saved) => saved,
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

        self.state.shell.screenshot.mark_saved(now);
        if self.state.screens.current_screen == CurrentScreen::Evaluation {
            let preview_result = self.backend.as_mut().map_or(Ok(()), |backend| {
                replace_screenshot_preview_texture(&mut self.asset_manager, backend, &saved.image)
            });
            if let Err(error) = preview_result {
                warn!("Failed to create screenshot preview texture: {error}");
                self.state.shell.screenshot.clear_preview();
            } else {
                let side_has_local_profile = request_side.is_some_and(|side| {
                    profile::is_session_side_joined(side) && !profile::is_session_side_guest(side)
                });
                self.state.shell.screenshot.set_preview(
                    now,
                    screenshot_preview_target(request_side, side_has_local_profile),
                );
            }
        }

        deadsync_audio_stream::play_sfx("assets/sounds/screenshot.ogg");
        info!("Saved screenshot to {}", saved.path.display());
    }

    pub(super) fn append_screenshot_overlay_actors(&self, actors: &mut Vec<Actor>, now: Instant) {
        append_screenshot_overlay_actors(
            &self.state.shell.screenshot,
            self.state.screens.current_screen == CurrentScreen::Evaluation,
            actors,
            now,
            space::screen_width(),
            space::screen_height(),
        );
    }
}
