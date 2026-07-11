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
use deadsync_score::Grade;
use deadsync_theme_simply_love::screens::SimplyLoveScreen as Screen;

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

pub enum PendingScreenshotResult {
    NoRequest,
    NoBackend,
    Saved {
        path: PathBuf,
        preview_error: Option<ScreenshotFlowError>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScreenshotSongInfo {
    pub title: String,
    pub meter: Option<u32>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AutoScreenshotEvalResult {
    pub personal_best: bool,
    pub failed: bool,
    pub grade: Grade,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AutoScreenshotFrameContext {
    pub screen: Screen,
    pub already_taken: bool,
    pub ready: bool,
    pub mask: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AutoScreenshotFramePlan {
    pub mark_taken: bool,
    pub request_capture: bool,
}

#[inline(always)]
pub const fn screenshot_preview_visible(screen: Screen) -> bool {
    matches!(screen, Screen::Evaluation)
}

pub fn screenshot_song_info(
    screen: Screen,
    gameplay: Option<ScreenshotSongInfo>,
    evaluation: Option<ScreenshotSongInfo>,
) -> Option<ScreenshotSongInfo> {
    let info = match screen {
        Screen::Gameplay => gameplay,
        Screen::Evaluation => evaluation,
        _ => None,
    }?;
    (!info.title.is_empty()).then_some(info)
}

pub fn auto_screenshot_eval_matches_results<I>(mask: u8, results: I) -> bool
where
    I: IntoIterator<Item = AutoScreenshotEvalResult>,
{
    if mask == 0 {
        return false;
    }
    results.into_iter().any(|result| {
        deadsync_config::theme::auto_screenshot_eval_matches(
            mask,
            result.personal_best,
            result.failed,
            matches!(result.grade, Grade::Tier01),
            matches!(result.grade, Grade::Quint),
        )
    })
}

pub fn auto_screenshot_frame_plan<I>(
    context: AutoScreenshotFrameContext,
    results: I,
) -> AutoScreenshotFramePlan
where
    I: IntoIterator<Item = AutoScreenshotEvalResult>,
{
    if context.screen != Screen::Evaluation || context.already_taken || !context.ready {
        return AutoScreenshotFramePlan {
            mark_taken: false,
            request_capture: false,
        };
    }
    AutoScreenshotFramePlan {
        mark_taken: true,
        request_capture: auto_screenshot_eval_matches_results(context.mask, results),
    }
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

pub fn capture_pending_screenshot<F>(
    state: &mut ScreenshotRuntimeState<PlayerSide>,
    backend: Option<&mut Backend>,
    asset_manager: &mut AssetManager,
    song_info: Option<(&str, Option<u32>)>,
    now: Instant,
    show_preview: bool,
    side_has_local_profile: F,
) -> Result<PendingScreenshotResult, ScreenshotFlowError>
where
    F: FnOnce(PlayerSide) -> bool,
{
    let Some(request_side) = state.take_pending_request() else {
        return Ok(PendingScreenshotResult::NoRequest);
    };
    let Some(backend) = backend else {
        return Ok(PendingScreenshotResult::NoBackend);
    };
    let saved = capture_screenshot(backend, song_info)?;

    state.mark_saved(now);
    let mut preview_error = None;
    if show_preview {
        if let Err(error) = replace_screenshot_preview_texture(asset_manager, backend, &saved.image)
        {
            state.clear_preview();
            preview_error = Some(error);
        } else {
            let side_has_local_profile = request_side.is_some_and(side_has_local_profile);
            state.set_preview(
                now,
                screenshot_preview_target(request_side, side_has_local_profile),
            );
        }
    }

    Ok(PendingScreenshotResult::Saved {
        path: saved.path,
        preview_error,
    })
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

    #[test]
    fn screenshot_song_info_uses_current_capture_screen() {
        let gameplay = ScreenshotSongInfo {
            title: "Gameplay Song".to_string(),
            meter: Some(12),
        };
        let evaluation = ScreenshotSongInfo {
            title: "Evaluation Song".to_string(),
            meter: Some(10),
        };
        assert_eq!(
            screenshot_song_info(
                Screen::Gameplay,
                Some(gameplay.clone()),
                Some(evaluation.clone())
            ),
            Some(gameplay)
        );
        assert_eq!(
            screenshot_song_info(Screen::Evaluation, None, Some(evaluation.clone())),
            Some(evaluation)
        );
        assert_eq!(
            screenshot_song_info(
                Screen::Evaluation,
                None,
                Some(ScreenshotSongInfo {
                    title: String::new(),
                    meter: Some(1),
                }),
            ),
            None
        );
        assert_eq!(
            screenshot_song_info(
                Screen::Menu,
                Some(ScreenshotSongInfo {
                    title: "Menu".to_string(),
                    meter: None,
                }),
                None,
            ),
            None
        );
    }

    #[test]
    fn auto_screenshot_eval_matches_configured_result_flags() {
        use deadsync_config::theme::{AUTO_SS_FAILS, AUTO_SS_PBS, AUTO_SS_QUADS, AUTO_SS_QUINTS};

        let results = [
            AutoScreenshotEvalResult {
                personal_best: false,
                failed: true,
                grade: Grade::Failed,
            },
            AutoScreenshotEvalResult {
                personal_best: true,
                failed: false,
                grade: Grade::Tier01,
            },
        ];

        assert!(!auto_screenshot_eval_matches_results(0, results));
        assert!(auto_screenshot_eval_matches_results(AUTO_SS_FAILS, results));
        assert!(auto_screenshot_eval_matches_results(AUTO_SS_PBS, results));
        assert!(auto_screenshot_eval_matches_results(AUTO_SS_QUADS, results));
        assert!(!auto_screenshot_eval_matches_results(
            AUTO_SS_QUINTS,
            results
        ));
        assert!(auto_screenshot_eval_matches_results(
            AUTO_SS_QUINTS,
            [AutoScreenshotEvalResult {
                personal_best: false,
                failed: false,
                grade: Grade::Quint,
            }],
        ));
    }

    #[test]
    fn auto_screenshot_frame_waits_for_evaluation_readiness() {
        use deadsync_config::theme::AUTO_SS_PBS;

        let result = [AutoScreenshotEvalResult {
            personal_best: true,
            failed: false,
            grade: Grade::Tier03,
        }];
        let ready_context = AutoScreenshotFrameContext {
            screen: Screen::Evaluation,
            already_taken: false,
            ready: true,
            mask: AUTO_SS_PBS,
        };

        assert_eq!(
            auto_screenshot_frame_plan(ready_context, result),
            AutoScreenshotFramePlan {
                mark_taken: true,
                request_capture: true,
            }
        );
        assert_eq!(
            auto_screenshot_frame_plan(
                AutoScreenshotFrameContext {
                    screen: Screen::Gameplay,
                    ..ready_context
                },
                result
            ),
            AutoScreenshotFramePlan {
                mark_taken: false,
                request_capture: false,
            }
        );
        assert_eq!(
            auto_screenshot_frame_plan(
                AutoScreenshotFrameContext {
                    ready: false,
                    ..ready_context
                },
                result
            ),
            AutoScreenshotFramePlan {
                mark_taken: false,
                request_capture: false,
            }
        );
        assert_eq!(
            auto_screenshot_frame_plan(
                AutoScreenshotFrameContext {
                    already_taken: true,
                    ..ready_context
                },
                result
            ),
            AutoScreenshotFramePlan {
                mark_taken: false,
                request_capture: false,
            }
        );
    }

    #[test]
    fn auto_screenshot_frame_marks_consumed_even_without_capture_match() {
        let plan = auto_screenshot_frame_plan(
            AutoScreenshotFrameContext {
                screen: Screen::Evaluation,
                already_taken: false,
                ready: true,
                mask: 0,
            },
            [AutoScreenshotEvalResult {
                personal_best: true,
                failed: false,
                grade: Grade::Tier01,
            }],
        );

        assert_eq!(
            plan,
            AutoScreenshotFramePlan {
                mark_taken: true,
                request_capture: false,
            }
        );
    }

    #[test]
    fn pending_screenshot_without_request_or_backend_is_nonfatal() {
        let mut state = ScreenshotRuntimeState::new();
        let no_request = capture_pending_screenshot(
            &mut state,
            None,
            &mut AssetManager::new(),
            None,
            Instant::now(),
            false,
            |_| false,
        );
        assert!(matches!(no_request, Ok(PendingScreenshotResult::NoRequest)));

        state.request(Some(PlayerSide::P1));
        let no_backend = capture_pending_screenshot(
            &mut state,
            None,
            &mut AssetManager::new(),
            None,
            Instant::now(),
            false,
            |_| false,
        );
        assert!(matches!(no_backend, Ok(PendingScreenshotResult::NoBackend)));
    }
}
