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
