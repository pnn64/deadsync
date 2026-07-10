use super::{App, CurrentScreen, ShellState};
use crate::act;
use crate::assets;
use crate::screens::evaluation;
use deadlib_platform::dirs;
use deadlib_present::actors::Actor;
use deadlib_present::space;
use deadlib_render::TextureHandleMap;
use deadsync_assets::screenshot::{self as screenshot_data, ScreenshotPreviewTarget};
use deadsync_profile as profile_data;
use deadsync_profile::compat as profile;
use log::{info, warn};
use std::error::Error;
use std::time::Instant;

const SCREENSHOT_PREVIEW_TEXTURE_KEY: &str = "__screenshot_preview";
const SCREENSHOT_PREVIEW_BORDER_PX: f32 = 4.0;
const SCREENSHOT_PREVIEW_Z: i16 = 32010;

fn current_song_title(state: &super::AppState) -> Option<(String, Option<u32>)> {
    match state.screens.current_screen {
        CurrentScreen::Gameplay => state.screens.gameplay_state.as_ref().map(|gs| {
            let title = gs.gameplay.song().title.clone();
            let meter = gs
                .gameplay
                .charts()
                .iter()
                .find(|c| c.meter > 0)
                .map(|c| c.meter);
            (title, meter)
        }),
        CurrentScreen::Evaluation => state
            .screens
            .evaluation_state
            .score_info
            .iter()
            .flatten()
            .next()
            .map(|si| (si.song.title.clone(), Some(si.chart.meter))),
        _ => None,
    }
    .filter(|(t, _)| !t.is_empty())
}

pub(super) fn should_auto_screenshot_eval(eval: &evaluation::State, mask: u8) -> bool {
    if mask == 0 {
        return false;
    }
    for info in eval.score_info.iter().flatten() {
        let is_fail = info.fail_time.is_some();
        let is_pb = info.personal_record_highlight_rank == Some(1);
        let is_quad = matches!(info.grade, deadsync_score::Grade::Tier01);
        let is_quint = matches!(info.grade, deadsync_score::Grade::Quint);
        if crate::config::auto_screenshot_eval_matches(mask, is_pb, is_fail, is_quad, is_quint) {
            return true;
        }
    }
    false
}

impl ShellState {
    #[inline(always)]
    fn screenshot_flash_alpha(&self, now: Instant) -> f32 {
        self.screenshot.flash_alpha(now)
    }
}

impl App {
    pub(super) fn capture_pending_screenshot(&mut self, now: Instant) {
        let Some(request_side) = self.state.shell.screenshot.take_pending_request() else {
            return;
        };
        let capture_result = {
            let Some(backend) = self.backend.as_mut() else {
                return;
            };
            backend.capture_frame()
        };

        match capture_result {
            Ok(mut image) => {
                // Screen captures should be opaque to avoid viewer-side alpha compositing.
                screenshot_data::set_opaque_alpha(&mut image);
                let song_info = current_song_title(&self.state);
                let song_info_ref = song_info.as_ref().map(|(t, m)| (t.as_str(), *m));
                let screenshot_root = dirs::app_dirs().screenshots_dir();
                match screenshot_data::save_screenshot_image(
                    &screenshot_root,
                    &image,
                    song_info_ref,
                ) {
                    Ok(path) => {
                        self.state.shell.screenshot.mark_saved(now);

                        if self.state.screens.current_screen == CurrentScreen::Evaluation {
                            if let Err(e) = self.replace_screenshot_preview_texture(&image) {
                                warn!("Failed to create screenshot preview texture: {e}");
                                self.state.shell.screenshot.clear_preview();
                            } else {
                                self.state.shell.screenshot.set_preview(
                                    now,
                                    Self::screenshot_preview_target(request_side),
                                );
                            }
                        }

                        deadsync_audio_stream::play_sfx("assets/sounds/screenshot.ogg");
                        info!("Saved screenshot to {}", path.display());
                    }
                    Err(e) => warn!("Failed to save screenshot: {e}"),
                }
            }
            Err(e) => warn!(
                "Screenshot capture unavailable for renderer {}: {e}",
                self.backend_type
            ),
        }
    }

    #[inline(always)]
    fn screenshot_preview_target(
        side: Option<profile_data::PlayerSide>,
    ) -> ScreenshotPreviewTarget {
        if let Some(side) = side
            && profile::is_session_side_joined(side)
            && !profile::is_session_side_guest(side)
        {
            return match side {
                profile_data::PlayerSide::P1 => ScreenshotPreviewTarget::Player1,
                profile_data::PlayerSide::P2 => ScreenshotPreviewTarget::Player2,
            };
        }
        ScreenshotPreviewTarget::Machine
    }

    fn replace_screenshot_preview_texture(
        &mut self,
        image: &image::RgbaImage,
    ) -> Result<(), Box<dyn Error>> {
        let Some(backend) = self.backend.as_mut() else {
            return Ok(());
        };

        if let Some((handle, old)) = self
            .asset_manager
            .remove_texture(SCREENSHOT_PREVIEW_TEXTURE_KEY)
        {
            let mut old_map = TextureHandleMap::default();
            old_map.insert(handle, old);
            backend.retire_textures(&mut old_map);
        }

        let texture = backend.create_texture(image, deadlib_render::SamplerDesc::default())?;
        self.asset_manager.insert_texture(
            SCREENSHOT_PREVIEW_TEXTURE_KEY.to_string(),
            texture,
            image.width(),
            image.height(),
        );
        assets::register_texture_dims(
            SCREENSHOT_PREVIEW_TEXTURE_KEY,
            image.width(),
            image.height(),
        );
        Ok(())
    }

    #[inline(always)]
    fn screenshot_preview_pose(&self, now: Instant) -> Option<(f32, f32, f32, f32)> {
        if self.state.screens.current_screen != CurrentScreen::Evaluation {
            return None;
        }
        let screen_w = space::screen_width();
        let screen_h = space::screen_height();
        let pose = self
            .state
            .shell
            .screenshot
            .preview_pose(now, screen_w, screen_h)?;
        Some((pose.x, pose.y, pose.scale, pose.glow_alpha))
    }

    pub(super) fn append_screenshot_overlay_actors(&self, actors: &mut Vec<Actor>, now: Instant) {
        let flash_alpha = self.state.shell.screenshot_flash_alpha(now);
        if flash_alpha > 0.0 {
            actors.push(act!(quad:
                align(0.0, 0.0):
                xy(0.0, 0.0):
                zoomto(space::screen_width(), space::screen_height()):
                diffuse(1.0, 1.0, 1.0, flash_alpha):
                z(32000)
            ));
        }

        let Some((x, y, scale, glow_alpha)) = self.screenshot_preview_pose(now) else {
            return;
        };
        if scale <= 0.0 {
            return;
        }

        let screen_w = space::screen_width();
        let screen_h = space::screen_height();
        let shot_w = screen_w * scale;
        let shot_h = screen_h * scale;
        if shot_w <= 0.0 || shot_h <= 0.0 {
            return;
        }

        let border = SCREENSHOT_PREVIEW_BORDER_PX;
        let outer_w = shot_w + border * 2.0;
        let outer_h = shot_h + border * 2.0;
        let edge_alpha = (0.7 + glow_alpha).clamp(0.0, 1.0);
        let z = SCREENSHOT_PREVIEW_Z;

        actors.push(act!(quad:
            align(0.5, 0.5):
            xy(x, y):
            setsize(outer_w, outer_h):
            diffuse(1.0, 1.0, 1.0, glow_alpha * 0.4):
            z(z)
        ));
        actors.push(act!(sprite(SCREENSHOT_PREVIEW_TEXTURE_KEY.to_string()):
            align(0.5, 0.5):
            xy(x, y):
            setsize(screen_w, screen_h):
            zoom(scale):
            z(z + 1)
        ));
        actors.push(act!(quad:
            align(0.5, 0.5):
            xy(x, y - shot_h * 0.5 - border * 0.5):
            setsize(outer_w, border):
            diffuse(1.0, 1.0, 1.0, edge_alpha):
            z(z + 2)
        ));
        actors.push(act!(quad:
            align(0.5, 0.5):
            xy(x, y + shot_h * 0.5 + border * 0.5):
            setsize(outer_w, border):
            diffuse(1.0, 1.0, 1.0, edge_alpha):
            z(z + 2)
        ));
        actors.push(act!(quad:
            align(0.5, 0.5):
            xy(x - shot_w * 0.5 - border * 0.5, y):
            setsize(border, outer_h):
            diffuse(1.0, 1.0, 1.0, edge_alpha):
            z(z + 2)
        ));
        actors.push(act!(quad:
            align(0.5, 0.5):
            xy(x + shot_w * 0.5 + border * 0.5, y):
            setsize(border, outer_h):
            diffuse(1.0, 1.0, 1.0, edge_alpha):
            z(z + 2)
        ));
    }
}
