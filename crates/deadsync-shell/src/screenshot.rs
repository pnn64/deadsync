//! Screenshot capture, preview texture, and shell-overlay flow.

use std::fmt;
use std::path::PathBuf;
use std::time::Instant;

use deadlib_platform::dirs;
use deadlib_present::actors::Actor;
use deadlib_render::{SamplerDesc, TextureHandleMap};
use deadlib_renderer::Backend;
use deadsync_assets::screenshot::{
    self as screenshot_data, ScreenshotPreviewTarget, ScreenshotRuntimeState, ScreenshotSaveError,
};
use deadsync_assets::{AssetManager, register_texture_dims};
use deadsync_profile::PlayerSide;

const SCREENSHOT_PREVIEW_TEXTURE_KEY: &str = "__screenshot_preview";
const SCREENSHOT_PREVIEW_BORDER_PX: f32 = 4.0;
const SCREENSHOT_PREVIEW_Z: i16 = 32010;

#[derive(Debug)]
pub enum ScreenshotFlowError {
    Capture(String),
    Save(ScreenshotSaveError),
    PreviewTexture(String),
}

impl fmt::Display for ScreenshotFlowError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Capture(error) | Self::PreviewTexture(error) => f.write_str(error),
            Self::Save(error) => error.fmt(f),
        }
    }
}

pub struct SavedScreenshot {
    pub image: image::RgbaImage,
    pub path: PathBuf,
}

/// Capture the current backend frame, force opaque alpha, and save it to the screenshot tree.
pub fn capture_screenshot(
    backend: &mut Backend,
    song_info: Option<(&str, Option<u32>)>,
) -> Result<SavedScreenshot, ScreenshotFlowError> {
    let mut image = backend
        .capture_frame()
        .map_err(|error| ScreenshotFlowError::Capture(error.to_string()))?;
    screenshot_data::set_opaque_alpha(&mut image);
    let path = screenshot_data::save_screenshot_image(
        &dirs::app_dirs().screenshots_dir(),
        &image,
        song_info,
    )
    .map_err(ScreenshotFlowError::Save)?;
    Ok(SavedScreenshot { image, path })
}

/// Replace the dynamic texture used by the evaluation screenshot preview.
pub fn replace_screenshot_preview_texture(
    asset_manager: &mut AssetManager,
    backend: &mut Backend,
    image: &image::RgbaImage,
) -> Result<(), ScreenshotFlowError> {
    if let Some((handle, old)) = asset_manager.remove_texture(SCREENSHOT_PREVIEW_TEXTURE_KEY) {
        let mut old_map = TextureHandleMap::default();
        old_map.insert(handle, old);
        backend.retire_textures(&mut old_map);
    }

    let texture = backend
        .create_texture(image, SamplerDesc::default())
        .map_err(|error| ScreenshotFlowError::PreviewTexture(error.to_string()))?;
    asset_manager.insert_texture(
        SCREENSHOT_PREVIEW_TEXTURE_KEY.to_string(),
        texture,
        image.width(),
        image.height(),
    );
    register_texture_dims(
        SCREENSHOT_PREVIEW_TEXTURE_KEY,
        image.width(),
        image.height(),
    );
    Ok(())
}

/// Resolve where a captured evaluation screenshot should tween after its center preview.
pub const fn screenshot_preview_target(
    side: Option<PlayerSide>,
    side_has_local_profile: bool,
) -> ScreenshotPreviewTarget {
    if side_has_local_profile {
        return match side {
            Some(PlayerSide::P1) => ScreenshotPreviewTarget::Player1,
            Some(PlayerSide::P2) => ScreenshotPreviewTarget::Player2,
            None => ScreenshotPreviewTarget::Machine,
        };
    }
    ScreenshotPreviewTarget::Machine
}

/// Append screenshot flash and evaluation-preview actors to the shell overlay.
pub fn append_screenshot_overlay_actors<RequestSide: Copy>(
    state: &ScreenshotRuntimeState<RequestSide>,
    show_preview: bool,
    actors: &mut Vec<Actor>,
    now: Instant,
    screen_w: f32,
    screen_h: f32,
) {
    let flash_alpha = state.flash_alpha(now);
    if flash_alpha > 0.0 {
        actors.push(deadlib_present::__act_from_builder!(
            (align(0.0, 0.0):
            xy(0.0, 0.0):
            zoomto(screen_w, screen_h):
            diffuse(1.0, 1.0, 1.0, flash_alpha):
            z(32000))
            deadsync_assets::present_dsl::SpriteBuilder::solid()
        ));
    }

    if !show_preview {
        return;
    }
    let Some(pose) = state.preview_pose(now, screen_w, screen_h) else {
        return;
    };
    if pose.scale <= 0.0 {
        return;
    }

    let shot_w = screen_w * pose.scale;
    let shot_h = screen_h * pose.scale;
    if shot_w <= 0.0 || shot_h <= 0.0 {
        return;
    }

    let border = SCREENSHOT_PREVIEW_BORDER_PX;
    let outer_w = shot_w + border * 2.0;
    let outer_h = shot_h + border * 2.0;
    let edge_alpha = (0.7 + pose.glow_alpha).clamp(0.0, 1.0);
    let z = SCREENSHOT_PREVIEW_Z;

    actors.push(deadlib_present::__act_from_builder!(
        (align(0.5, 0.5):
        xy(pose.x, pose.y):
        setsize(outer_w, outer_h):
        diffuse(1.0, 1.0, 1.0, pose.glow_alpha * 0.4):
        z(z))
        deadsync_assets::present_dsl::SpriteBuilder::solid()
    ));
    actors.push(deadlib_present::__act_from_builder!(
        (align(0.5, 0.5):
        xy(pose.x, pose.y):
        setsize(screen_w, screen_h):
        zoom(pose.scale):
        z(z + 1))
        deadsync_assets::present_dsl::SpriteBuilder::texture(
            SCREENSHOT_PREVIEW_TEXTURE_KEY.to_string()
        )
    ));
    for (x, y, width, height) in [
        (
            pose.x,
            pose.y - shot_h * 0.5 - border * 0.5,
            outer_w,
            border,
        ),
        (
            pose.x,
            pose.y + shot_h * 0.5 + border * 0.5,
            outer_w,
            border,
        ),
        (
            pose.x - shot_w * 0.5 - border * 0.5,
            pose.y,
            border,
            outer_h,
        ),
        (
            pose.x + shot_w * 0.5 + border * 0.5,
            pose.y,
            border,
            outer_h,
        ),
    ] {
        actors.push(deadlib_present::__act_from_builder!(
            (align(0.5, 0.5):
            xy(x, y):
            setsize(width, height):
            diffuse(1.0, 1.0, 1.0, edge_alpha):
            z(z + 2))
            deadsync_assets::present_dsl::SpriteBuilder::solid()
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preview_target_requires_a_joined_local_profile() {
        assert_eq!(
            screenshot_preview_target(Some(PlayerSide::P1), true),
            ScreenshotPreviewTarget::Player1
        );
        assert_eq!(
            screenshot_preview_target(Some(PlayerSide::P2), true),
            ScreenshotPreviewTarget::Player2
        );
        assert_eq!(
            screenshot_preview_target(Some(PlayerSide::P1), false),
            ScreenshotPreviewTarget::Machine
        );
        assert_eq!(
            screenshot_preview_target(None, false),
            ScreenshotPreviewTarget::Machine
        );
    }
}
