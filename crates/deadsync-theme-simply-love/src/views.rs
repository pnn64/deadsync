use deadlib_render::{ClockDomainTrace, PresentModeTrace};
use deadsync_assets::noteskin::Noteskin;
use deadsync_audio::OutputTimingSnapshot;
use deadsync_profile::PlayerSide;
use std::sync::Arc;

pub use deadsync_config::frame_pacing::VisibleStutterSample;
pub use deadsync_theme::views::{
    CourseGraphStageView, CourseStageView, DensityGraphView, EvaluationView, FrameStatsSample,
    FrameStatsSummary, OverlayAnchor, OverlayStyle, SelectedCourseView, TimingHealthView,
};

/// Concrete evaluation view used by the Simply Love screens.
pub type ScoreInfo = EvaluationView<Arc<Noteskin>, PlayerSide>;
pub type CourseGraphStage = CourseGraphStageView;
pub type CourseStagePlan = CourseStageView;
pub type SelectedCoursePlan = SelectedCourseView;
pub type DensityGraphSource = DensityGraphView;
pub type TimingHealth = TimingHealthView<PresentModeTrace, ClockDomainTrace, OutputTimingSnapshot>;

/// Simply Love's two density-graph texture targets.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SimplyLoveDensityGraphSlot {
    SelectMusicP1,
    SelectMusicP2,
}

/// Simply Love compatibility name used inside its concrete screen modules.
pub(crate) type DensityGraphSlot = SimplyLoveDensityGraphSlot;

/// Runtime capabilities used to decide which concrete Simply Love options
/// rows are available on the current host.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SimplyLoveUpdaterCapabilities {
    pub app_update: bool,
    pub ffmpeg_install: bool,
}

/// Coarse updater failure category used by Simply Love's localized overlays.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SimplyLoveUpdateErrorKind {
    Network,
    RateLimited,
    HttpStatus,
    Parse,
    NoAssetForHost,
    Checksum,
    Io,
}

/// Release metadata needed by Simply Love's updater presentation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SimplyLoveReleaseView {
    pub tag: String,
    pub html_url: String,
    pub published_at: Option<String>,
}

/// Release-asset metadata shown by Simply Love before a download begins.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SimplyLoveReleaseAssetView {
    pub size: u64,
    pub digest: Option<String>,
}

/// Prepared app-update state rendered by Simply Love.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SimplyLoveUpdatePhase {
    Idle,
    Checking,
    ConfirmDownload {
        info: SimplyLoveReleaseView,
        asset: SimplyLoveReleaseAssetView,
    },
    UpToDate {
        tag: String,
    },
    RollbackChecking,
    RollbackPick {
        candidates: Vec<SimplyLoveReleaseView>,
        selected: usize,
    },
    RollbackEmpty,
    AvailableNoInstall {
        info: SimplyLoveReleaseView,
    },
    Downloading {
        info: SimplyLoveReleaseView,
        written: u64,
        total: Option<u64>,
        eta_secs: Option<u64>,
    },
    Ready {
        info: SimplyLoveReleaseView,
    },
    Applying {
        info: SimplyLoveReleaseView,
    },
    AppliedRestartRequired {
        info: SimplyLoveReleaseView,
        detail: String,
    },
    Error {
        kind: SimplyLoveUpdateErrorKind,
        detail: String,
    },
}

/// Prepared FFmpeg-install state rendered by Simply Love.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SimplyLoveFfmpegPhase {
    Idle,
    Checking,
    Confirm {
        version: String,
        origin: String,
        total: Option<u64>,
        already_available: bool,
    },
    Downloading {
        version: String,
        written: u64,
        total: Option<u64>,
        eta_secs: Option<u64>,
        speed_bps: Option<u64>,
    },
    Extracting {
        version: String,
    },
    Installed {
        version: String,
    },
    Unsupported,
    AlreadyAvailable,
    Error {
        kind: SimplyLoveUpdateErrorKind,
        detail: String,
    },
}

/// One shell-prepared snapshot of the updater services used by Options.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SimplyLoveUpdaterView {
    pub update: SimplyLoveUpdatePhase,
    pub ffmpeg: SimplyLoveFfmpegPhase,
}

impl Default for SimplyLoveUpdaterView {
    fn default() -> Self {
        Self {
            update: SimplyLoveUpdatePhase::Idle,
            ffmpeg: SimplyLoveFfmpegPhase::Idle,
        }
    }
}

/// Number of bins in Simply Love's frame-interval histogram.
pub const HISTOGRAM_BINS: usize = 32;

/// Fill `out` with Simply Love's frame-interval histogram. The final bin
/// absorbs overflow.
pub fn frame_histogram(
    samples: &[FrameStatsSample],
    out: &mut [u32; HISTOGRAM_BINS],
    bin_width_us: u32,
) {
    *out = [0; HISTOGRAM_BINS];
    let bin_width_us = bin_width_us.max(1);
    for sample in samples {
        if sample.is_empty() {
            continue;
        }
        let idx = (sample.frame_us / bin_width_us).min(HISTOGRAM_BINS as u32 - 1) as usize;
        out[idx] = out[idx].saturating_add(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn histogram_ignores_empty_samples_and_absorbs_overflow() {
        let samples = [
            FrameStatsSample::empty(),
            FrameStatsSample {
                host_nanos: 1,
                frame_us: 1_500,
                ..FrameStatsSample::empty()
            },
            FrameStatsSample {
                host_nanos: 2,
                frame_us: 100_000,
                ..FrameStatsSample::empty()
            },
        ];
        let mut bins = [0; HISTOGRAM_BINS];
        frame_histogram(&samples, &mut bins, 1_000);
        assert_eq!(bins[1], 1);
        assert_eq!(bins[HISTOGRAM_BINS - 1], 1);
    }
}
