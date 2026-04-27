use super::{App, CurrentScreen, ShellState};
use crate::act;
use crate::assets;
use crate::config::dirs;
use crate::engine::gfx::TextureHandleMap;
use crate::engine::present::actors::Actor;
use crate::engine::space;
use crate::game::{profile, scores};
use crate::screens::evaluation;
use chrono::{Datelike, Local};
use log::{info, warn};
use std::error::Error;
use std::path::PathBuf;
use std::time::Instant;

const SCREENSHOT_FLASH_ATTACK_SECONDS: f32 = 0.02;
const SCREENSHOT_FLASH_DECAY_SECONDS: f32 = 0.18;
const SCREENSHOT_FLASH_MAX_ALPHA: f32 = 0.7;
const SCREENSHOT_PREVIEW_TEXTURE_KEY: &str = "__screenshot_preview";
const SCREENSHOT_PREVIEW_SCALE: f32 = 0.2;
const SCREENSHOT_PREVIEW_HOLD_SECONDS: f32 = 0.4;
const SCREENSHOT_PREVIEW_MACHINE_EXTRA_HOLD_SECONDS: f32 = 0.25;
const SCREENSHOT_PREVIEW_TWEEN_SECONDS: f32 = 0.75;
const SCREENSHOT_PREVIEW_GLOW_PERIOD_SECONDS: f32 = 0.5;
const SCREENSHOT_PREVIEW_GLOW_ALPHA: f32 = 0.2;
const SCREENSHOT_PREVIEW_BORDER_PX: f32 = 4.0;
const SCREENSHOT_PREVIEW_Z: i16 = 32010;

const MONTH_NAMES: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];

fn sanitize_song_title(title: &str) -> String {
    title
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '_' })
        .collect()
}

fn current_song_title(state: &super::AppState) -> Option<(String, Option<u32>)> {
    match state.screens.current_screen {
        CurrentScreen::Gameplay => state.screens.gameplay_state.as_ref().map(|gs| {
            let title = gs.gameplay.song.title.clone();
            let meter = gs
                .gameplay
                .charts
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

#[derive(Clone, Copy)]
enum ScreenshotPreviewTarget {
    Player(profile::PlayerSide),
    Machine,
}

#[derive(Clone, Copy)]
pub(super) struct ScreenshotPreviewState {
    started_at: Instant,
    target: ScreenshotPreviewTarget,
}

pub(super) fn should_auto_screenshot_eval(eval: &evaluation::State, mask: u8) -> bool {
    if mask == 0 {
        return false;
    }
    for info in eval.score_info.iter().flatten() {
        let is_fail = info.fail_time.is_some();
        let is_pb = info.personal_record_highlight_rank == Some(1);
        let is_quad = matches!(info.grade, scores::Grade::Tier01);
        let is_quint = matches!(info.grade, scores::Grade::Quint);
        if (mask & crate::config::AUTO_SS_PBS) != 0 && is_pb {
            return true;
        }
        if (mask & crate::config::AUTO_SS_FAILS) != 0 && is_fail {
            return true;
        }
        if (mask & crate::config::AUTO_SS_CLEARS) != 0 && !is_fail {
            return true;
        }
        if (mask & crate::config::AUTO_SS_QUADS) != 0 && is_quad {
            return true;
        }
        if (mask & crate::config::AUTO_SS_QUINTS) != 0 && is_quint {
            return true;
        }
    }
    false
}

fn save_screenshot_image(
    image: &image::RgbaImage,
    song_info: Option<(&str, Option<u32>)>,
) -> Result<PathBuf, Box<dyn Error>> {
    let root = dirs::app_dirs().screenshots_dir();
    let now = Local::now();
    let month_idx = now.month0() as usize;
    let month_name = MONTH_NAMES.get(month_idx).copied().unwrap_or("Unknown");
    let dir = root
        .join(format!("{:04}", now.year()))
        .join(format!("{:02}-{}", now.month(), month_name));
    std::fs::create_dir_all(&dir)?;

    let stamp = now.format("%Y-%m-%d_%H%M%S").to_string();
    let title_part = song_info
        .map(|(title, meter)| {
            let title = sanitize_song_title(title);
            match meter {
                Some(m) if m > 0 => format!("__{m}__{title}"),
                _ => format!("__{title}"),
            }
        })
        .filter(|t| !t.is_empty())
        .unwrap_or_default();

    let mut path = dir.join(format!("{stamp}{title_part}.png"));
    let mut suffix = 1_u32;
    while path.exists() {
        path = dir.join(format!("{stamp}{title_part}-{suffix:02}.png"));
        suffix = suffix.saturating_add(1);
        if suffix > 9_999 {
            return Err(
                std::io::Error::other("Failed to allocate unique screenshot filename").into(),
            );
        }
    }

    image.save_with_format(&path, image::ImageFormat::Png)?;
    Ok(path)
}

#[inline(always)]
fn set_opaque_alpha(image: &mut image::RgbaImage) {
    for pixel in image.pixels_mut() {
        pixel.0[3] = 255;
    }
}

impl ShellState {
    #[inline(always)]
    fn screenshot_flash_alpha(&self, now: Instant) -> f32 {
        let Some(started_at) = self.screenshot_flash_started_at else {
            return 0.0;
        };
        let elapsed = now.duration_since(started_at).as_secs_f32();
        let total = SCREENSHOT_FLASH_ATTACK_SECONDS + SCREENSHOT_FLASH_DECAY_SECONDS;
        if elapsed <= 0.0 || elapsed >= total {
            return 0.0;
        }
        if elapsed <= SCREENSHOT_FLASH_ATTACK_SECONDS {
            return (elapsed / SCREENSHOT_FLASH_ATTACK_SECONDS).clamp(0.0, 1.0)
                * SCREENSHOT_FLASH_MAX_ALPHA;
        }
        let fade =
            1.0 - ((elapsed - SCREENSHOT_FLASH_ATTACK_SECONDS) / SCREENSHOT_FLASH_DECAY_SECONDS);
        fade.clamp(0.0, 1.0) * SCREENSHOT_FLASH_MAX_ALPHA
    }
}

impl App {
    pub(super) fn capture_pending_screenshot(&mut self, now: Instant) {
        if !self.state.shell.screenshot_pending {
            return;
        }
        self.state.shell.screenshot_pending = false;
        let request_side = self.state.shell.screenshot_request_side.take();
        let capture_result = {
            let Some(backend) = self.backend.as_mut() else {
                return;
            };
            backend.capture_frame()
        };

        match capture_result {
            Ok(mut image) => {
                // Screen captures should be opaque to avoid viewer-side alpha compositing.
                set_opaque_alpha(&mut image);
                let song_info = current_song_title(&self.state);
                let song_info_ref = song_info.as_ref().map(|(t, m)| (t.as_str(), *m));
                match save_screenshot_image(&image, song_info_ref) {
                    Ok(path) => {
                        self.state.shell.screenshot_flash_started_at = Some(now);

                        if self.state.screens.current_screen == CurrentScreen::Evaluation {
                            if let Err(e) = self.replace_screenshot_preview_texture(&image) {
                                warn!("Failed to create screenshot preview texture: {e}");
                                self.state.shell.screenshot_preview = None;
                            } else {
                                self.state.shell.screenshot_preview =
                                    Some(ScreenshotPreviewState {
                                        started_at: now,
                                        target: Self::screenshot_preview_target(request_side),
                                    });
                            }
                        }

                        crate::engine::audio::play_sfx("assets/sounds/screenshot.ogg");
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
    fn screenshot_preview_target(side: Option<profile::PlayerSide>) -> ScreenshotPreviewTarget {
        if let Some(side) = side
            && profile::is_session_side_joined(side)
            && !profile::is_session_side_guest(side)
        {
            return ScreenshotPreviewTarget::Player(side);
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

        let texture = backend.create_texture(image, crate::engine::gfx::SamplerDesc::default())?;
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
        let preview = self.state.shell.screenshot_preview?;
        let elapsed = now.duration_since(preview.started_at).as_secs_f32();
        if !elapsed.is_finite() || elapsed < 0.0 {
            return None;
        }

        let hold_seconds = SCREENSHOT_PREVIEW_HOLD_SECONDS
            + match preview.target {
                ScreenshotPreviewTarget::Machine => SCREENSHOT_PREVIEW_MACHINE_EXTRA_HOLD_SECONDS,
                ScreenshotPreviewTarget::Player(_) => 0.0,
            };
        let total_seconds = hold_seconds + SCREENSHOT_PREVIEW_TWEEN_SECONDS;
        if elapsed >= total_seconds {
            return None;
        }

        let screen_w = space::screen_width();
        let screen_h = space::screen_height();
        let start_x = screen_w * 0.5;
        let start_y = screen_h * 0.5;

        let (target_x, target_y) = match preview.target {
            ScreenshotPreviewTarget::Player(profile::PlayerSide::P1) => (20.0, screen_h + 10.0),
            ScreenshotPreviewTarget::Player(profile::PlayerSide::P2) => {
                (screen_w - 20.0, screen_h + 10.0)
            }
            ScreenshotPreviewTarget::Machine => (screen_w * 0.5, screen_h + 10.0),
        };

        let (x, y, scale) = if elapsed <= hold_seconds {
            (start_x, start_y, SCREENSHOT_PREVIEW_SCALE)
        } else {
            let t = ((elapsed - hold_seconds) / SCREENSHOT_PREVIEW_TWEEN_SECONDS).clamp(0.0, 1.0);
            let smooth = t * t * (3.0 - 2.0 * t);
            (
                start_x + (target_x - start_x) * smooth,
                start_y + (target_y - start_y) * smooth,
                SCREENSHOT_PREVIEW_SCALE * (1.0 - smooth),
            )
        };

        let blink_phase =
            elapsed * (std::f32::consts::TAU / SCREENSHOT_PREVIEW_GLOW_PERIOD_SECONDS);
        let glow_alpha = blink_phase.sin().mul_add(0.5, 0.5) * SCREENSHOT_PREVIEW_GLOW_ALPHA;
        Some((x, y, scale.max(0.0), glow_alpha.clamp(0.0, 1.0)))
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
