use crate::screens::SimplyLoveScreen;
use crate::views::{DensityGraphView, SimplyLoveDensityGraphSlot};
use deadlib_render::{BackendType, PresentModePolicy};
use deadsync_config::app_config::DisplayMode;
use deadsync_profile::{ActiveProfile, PlayerSide};
use deadsync_simfile::sync_offset::SongOffsetSyncChange;
use std::path::PathBuf;

/// Runtime work requested by Simply Love after its concrete screen logic has
/// produced a generic theme effect.
#[derive(Clone, Debug)]
pub enum SimplyLoveRuntimeRequest {
    SelectProfiles {
        p1: ActiveProfile,
        p2: ActiveProfile,
    },
    LinkArrowCloud {
        profile_id: String,
        display_name: String,
    },
    LinkGrooveStats {
        profile_id: String,
        display_name: String,
    },
    RequestScreenshot(Option<PlayerSide>),
    RequestBanner(Option<PathBuf>),
    RequestCdTitle(Option<PathBuf>),
    RequestPackBanner(Option<PathBuf>),
    RequestWheelItemBackgrounds(Vec<PathBuf>),
    RequestDensityGraph {
        slot: SimplyLoveDensityGraphSlot,
        chart_opt: Option<DensityGraphView>,
    },
    ApplySongOffsetSync {
        simfile_path: PathBuf,
        delta_seconds: f32,
    },
    ApplySongOffsetSyncBatch {
        changes: Vec<SongOffsetSyncChange>,
    },
    FetchOnlineGrade(String),
    WriteFsrDump,
    ChangeGraphics {
        renderer: Option<BackendType>,
        display_mode: Option<DisplayMode>,
        monitor: Option<usize>,
        resolution: Option<(u32, u32)>,
        vsync: Option<bool>,
        present_mode_policy: Option<PresentModePolicy>,
        max_fps: Option<u16>,
        high_dpi: Option<bool>,
    },
    UpdateShowOverlay(u8),
    UpdateMouseCursorHidden(bool),
    TestLightsSetAuto,
    TestLightsStepCabinet(i8),
    TestLightsStepButton(i8),
}

pub type SimplyLoveEffect = deadsync_theme::ThemeEffect<SimplyLoveScreen, SimplyLoveRuntimeRequest>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SimplyLoveEffectRouteContext {
    pub current_screen: SimplyLoveScreen,
    pub restart_pending: bool,
    pub course_active: bool,
    pub course_has_next_stage: bool,
    pub gameplay_failed: bool,
}

#[derive(Clone, Debug)]
pub struct SimplyLoveEffectRoutePlan {
    pub action: SimplyLoveEffect,
    pub clear_restart_pending: bool,
}

/// Apply Simply Love's gameplay and course redirects before the shell executes
/// an effect.
pub fn resolve_effect_route(
    effect: SimplyLoveEffect,
    context: SimplyLoveEffectRouteContext,
) -> SimplyLoveEffectRoutePlan {
    let (effect, clear_restart_pending) = match effect {
        // SL/zmod parity: a restart-triggered Cancel exit returns to the wheel.
        // Redirect it to Gameplay so the player skips the wheel round-trip.
        SimplyLoveEffect::NavigateNoFade(SimplyLoveScreen::SelectMusic)
            if context.restart_pending && context.current_screen == SimplyLoveScreen::Gameplay =>
        {
            (
                SimplyLoveEffect::NavigateNoFade(SimplyLoveScreen::Gameplay),
                true,
            )
        }
        SimplyLoveEffect::Navigate(SimplyLoveScreen::Evaluation)
            if context.current_screen == SimplyLoveScreen::Gameplay
                && context.course_has_next_stage
                && !context.gameplay_failed =>
        {
            (
                SimplyLoveEffect::Navigate(SimplyLoveScreen::Gameplay),
                false,
            )
        }
        SimplyLoveEffect::Navigate(SimplyLoveScreen::SelectMusic)
            if context.current_screen == SimplyLoveScreen::Gameplay && context.course_active =>
        {
            (
                SimplyLoveEffect::Navigate(SimplyLoveScreen::SelectCourse),
                false,
            )
        }
        SimplyLoveEffect::NavigateNoFade(SimplyLoveScreen::SelectMusic)
            if context.current_screen == SimplyLoveScreen::Gameplay && context.course_active =>
        {
            (
                SimplyLoveEffect::NavigateNoFade(SimplyLoveScreen::SelectCourse),
                false,
            )
        }
        effect => (effect, false),
    };
    SimplyLoveEffectRoutePlan {
        action: effect,
        clear_restart_pending,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn course_and_restart_redirects_are_theme_owned() {
        let restart = resolve_effect_route(
            SimplyLoveEffect::NavigateNoFade(SimplyLoveScreen::SelectMusic),
            SimplyLoveEffectRouteContext {
                current_screen: SimplyLoveScreen::Gameplay,
                restart_pending: true,
                course_active: false,
                course_has_next_stage: false,
                gameplay_failed: false,
            },
        );
        assert!(matches!(
            restart.action,
            SimplyLoveEffect::NavigateNoFade(SimplyLoveScreen::Gameplay)
        ));
        assert!(restart.clear_restart_pending);

        let course = resolve_effect_route(
            SimplyLoveEffect::Navigate(SimplyLoveScreen::SelectMusic),
            SimplyLoveEffectRouteContext {
                current_screen: SimplyLoveScreen::Gameplay,
                restart_pending: false,
                course_active: true,
                course_has_next_stage: false,
                gameplay_failed: false,
            },
        );
        assert!(matches!(
            course.action,
            SimplyLoveEffect::Navigate(SimplyLoveScreen::SelectCourse)
        ));
    }
}
