use crate::screens::SimplyLoveScreen;
use crate::views::{DensityGraphView, SimplyLoveDensityGraphSlot};
use deadsync_profile::{ActiveProfile, PlayerSide};
use deadsync_simfile::sync_offset::SongOffsetSyncChange;
use deadsync_theme::{AudioRequest, GraphicsRequest, PlatformRequest};
use std::path::PathBuf;
use std::sync::Arc;

#[derive(Clone, Debug)]
pub enum SimplyLoveMediaRequest {
    Screenshot(Option<PlayerSide>),
    Banner(Option<PathBuf>),
    CdTitle(Option<PathBuf>),
    PackBanner(Option<PathBuf>),
    WheelItemBackgrounds(Vec<PathBuf>),
    DensityGraph {
        slot: SimplyLoveDensityGraphSlot,
        chart_opt: Option<DensityGraphView>,
    },
}

#[derive(Clone, Debug)]
pub enum SimplyLoveProfileRequest {
    Select {
        p1: ActiveProfile,
        p2: ActiveProfile,
        p1_joined: bool,
        p2_joined: bool,
        fast_switch: bool,
    },
    PickImportFolder {
        title: String,
    },
}

#[derive(Clone, Debug)]
pub enum SimplyLoveOnlineRequest {
    Reinitialize,
    RefreshArrowCloudStatus,
    LinkArrowCloud {
        profile_id: String,
        display_name: String,
    },
    LinkGrooveStats {
        profile_id: String,
        display_name: String,
    },
    FetchGrade(String),
}

#[derive(Clone, Debug)]
pub enum SimplyLoveSyncRequest {
    StartAnalysis {
        owner: SimplyLoveSyncOwner,
        targets: Vec<SimplyLoveSyncTarget>,
        emit_freq_delta: bool,
    },
    CancelAnalysis(SimplyLoveSyncOwner),
    ApplySongOffset {
        simfile_path: PathBuf,
        delta_seconds: f32,
    },
    ApplySongOffsetBatch {
        changes: Vec<SongOffsetSyncChange>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SimplyLoveSyncOwner {
    SelectMusicSong,
    SelectMusicPack,
    OptionsPack,
}

#[derive(Clone, Debug)]
pub struct SimplyLoveSyncTarget {
    pub song: Arc<deadsync_chart::SongData>,
    pub chart_ix: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SimplyLoveSyncKernelTarget {
    Digest,
    Accumulator,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SimplyLoveSyncKernel {
    Rising,
    Loudest,
}

#[derive(Clone, Debug, PartialEq)]
pub enum SimplyLoveSyncStreamEvent {
    Init {
        cols: usize,
        freq_rows: usize,
        planned_beats: usize,
        kernel_target: SimplyLoveSyncKernelTarget,
        kernel: SimplyLoveSyncKernel,
        times_ms: Vec<f64>,
    },
    Beat {
        beat_seq: usize,
        digest_row: Vec<f64>,
        freq_delta: Option<Vec<f64>>,
    },
    Convolution {
        rows: usize,
        post_kernel: Vec<f64>,
        convolution: Vec<f64>,
        edge_discard: usize,
    },
    Done(SimplyLoveSyncResult),
}

#[derive(Clone, Debug, PartialEq)]
pub struct SimplyLoveSyncPlotView {
    pub freq_rows: usize,
    pub digest_rows: usize,
    pub cols: usize,
    pub post_rows: usize,
    pub freq_domain: Vec<f64>,
    pub beat_digest: Vec<f64>,
    pub post_kernel: Vec<f64>,
    pub convolution: Vec<f64>,
    pub times_ms: Vec<f64>,
    pub edge_discard: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SimplyLoveSyncSongResult {
    pub estimate: SimplyLoveSyncResult,
    pub plot: SimplyLoveSyncPlotView,
}

pub enum SimplyLoveSyncEvent {
    SongStream(SimplyLoveSyncStreamEvent),
    SongFinished(Result<SimplyLoveSyncSongResult, String>),
    RowStarted {
        index: usize,
    },
    RowInit {
        index: usize,
        total_beats: usize,
    },
    RowBeat {
        index: usize,
        beats_processed: usize,
        total_beats: usize,
    },
    RowFinished {
        index: usize,
        result: Result<SimplyLoveSyncResult, String>,
    },
    Finished,
    Disconnected,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SimplyLoveSyncResult {
    pub bias_ms: f64,
    pub confidence: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SimplyLoveConfigRequest {
    ShowOverlay(u8),
    MouseCursorHidden(bool),
    PersistColor(i32),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SimplyLoveHardwareRequest {
    TestLightsAuto,
    StepTestCabinet(i8),
    StepTestButton(i8),
    AssignSmxPads {
        p1_serial: Option<String>,
        p2_serial: Option<String>,
    },
    SwapSmxPads,
    SetSmxUnderglowTheme(bool),
    SetSmxUnderglowGrb(bool),
    ApplySmxPadPreset {
        pad: usize,
        name: String,
    },
    ApplySmxPadConfig {
        pad: usize,
        profile_id: String,
        name: String,
    },
    CaptureSmxPadConfig {
        pad: usize,
        profile_id: String,
        name: String,
        set_default: bool,
        overwrite: bool,
    },
    SetSmxPlayerLights([Option<[u8; 3]>; 2]),
    ReenableSmxAutoLights,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SimplyLoveDebugRequest {
    WriteFsrDump,
}

/// Updater work requested by Simply Love and executed by the process shell.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SimplyLoveUpdaterRequest {
    CheckForUpdates,
    CheckForRollback,
    DownloadUpdate,
    ApplyUpdate,
    DismissUpdate,
    CancelUpdate,
    MoveRollback(i32),
    ConfirmRollback,
    CheckFfmpegAvailability,
    ConfirmFfmpegInstall,
    DismissFfmpeg,
    CancelFfmpegCheck,
    CancelFfmpegDownload,
}

/// Runtime work requested by Simply Love after its concrete screen logic has
/// produced a generic theme effect.
#[derive(Clone, Debug)]
pub enum SimplyLoveRuntimeRequest {
    Audio(AudioRequest),
    Media(SimplyLoveMediaRequest),
    Profile(SimplyLoveProfileRequest),
    Online(SimplyLoveOnlineRequest),
    Graphics(GraphicsRequest),
    Platform(PlatformRequest),
    Sync(SimplyLoveSyncRequest),
    Config(SimplyLoveConfigRequest),
    Hardware(SimplyLoveHardwareRequest),
    Debug(SimplyLoveDebugRequest),
    Updater(SimplyLoveUpdaterRequest),
}

pub type SimplyLoveEffect = deadsync_theme::ThemeEffect<SimplyLoveScreen, SimplyLoveRuntimeRequest>;

pub(crate) fn sfx(path: &str) -> SimplyLoveEffect {
    SimplyLoveEffect::Runtime(SimplyLoveRuntimeRequest::Audio(AudioRequest::PlaySfx(
        path.to_owned(),
    )))
}

pub(crate) fn sfx_then(path: &str, effect: SimplyLoveEffect) -> SimplyLoveEffect {
    SimplyLoveEffect::Batch(vec![sfx(path), effect])
}

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

    #[test]
    fn sfx_then_preserves_audio_before_follow_up_effect() {
        let effect = sfx_then(
            "assets/sounds/start.ogg",
            SimplyLoveEffect::Navigate(SimplyLoveScreen::SelectStyle),
        );
        let SimplyLoveEffect::Batch(effects) = effect else {
            panic!("expected batch effect");
        };
        assert_eq!(effects.len(), 2);
        assert!(matches!(
            &effects[0],
            SimplyLoveEffect::Runtime(SimplyLoveRuntimeRequest::Audio(
                AudioRequest::PlaySfx(path)
            )) if path == "assets/sounds/start.ogg"
        ));
        assert!(matches!(
            effects[1],
            SimplyLoveEffect::Navigate(SimplyLoveScreen::SelectStyle)
        ));
    }
}
