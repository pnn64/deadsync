use deadlib_render::{ClockDomainTrace, PresentModeTrace};
use deadsync_audio::OutputTimingSnapshot;

pub use deadsync_config::frame_pacing::VisibleStutterSample;

/// Number of bins in the frame-interval histogram rendered by the overlay.
pub const HISTOGRAM_BINS: usize = 32;

/// Presentation-facing timing health prepared by the process shell.
#[derive(Clone, Copy, Debug)]
pub struct TimingHealth {
    pub interval_ns: u64,
    pub display_error_ms: f32,
    pub display_catching_up: bool,
    pub present_mode: PresentModeTrace,
    pub display_clock: ClockDomainTrace,
    pub host_clock: ClockDomainTrace,
    pub in_flight_images: u8,
    pub waited_for_image: bool,
    pub applied_back_pressure: bool,
    pub queue_idle_waited: bool,
    pub suboptimal: bool,
    pub submitted_present_id: u32,
    pub completed_present_id: u32,
    pub calibration_error_ns: u64,
    pub host_mapped: bool,
    pub audio: Option<OutputTimingSnapshot>,
}

/// Which screen corner or center seam the frame-statistics overlay uses.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum OverlayAnchor {
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
    TopCenter,
    BottomCenter,
}

impl OverlayAnchor {
    /// Stable string key for persistence.
    #[inline(always)]
    pub fn to_key(self) -> &'static str {
        match self {
            Self::TopLeft => "top-left",
            Self::TopRight => "top-right",
            Self::BottomLeft => "bottom-left",
            Self::BottomRight => "bottom-right",
            Self::TopCenter => "top-center",
            Self::BottomCenter => "bottom-center",
        }
    }

    /// Parse a persisted key. Unknown values leave placement automatic.
    #[inline(always)]
    pub fn from_key(key: &str) -> Option<Self> {
        match key.trim().to_ascii_lowercase().as_str() {
            "top-left" => Some(Self::TopLeft),
            "top-right" => Some(Self::TopRight),
            "bottom-left" => Some(Self::BottomLeft),
            "bottom-right" => Some(Self::BottomRight),
            "top-center" => Some(Self::TopCenter),
            "bottom-center" => Some(Self::BottomCenter),
            _ => None,
        }
    }
}

/// Presentation detail shown by the frame-statistics overlay.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum OverlayStyle {
    Detailed,
    Minimal,
}

impl OverlayStyle {
    #[inline(always)]
    pub const fn toggle(self) -> Self {
        match self {
            Self::Detailed => Self::Minimal,
            Self::Minimal => Self::Detailed,
        }
    }

    #[inline(always)]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Detailed => "detailed",
            Self::Minimal => "minimal",
        }
    }

    #[inline(always)]
    pub fn from_key(key: &str) -> Self {
        match key.trim().to_ascii_lowercase().as_str() {
            "minimal" => Self::Minimal,
            _ => Self::Detailed,
        }
    }

    #[inline(always)]
    pub const fn show_p99(self) -> bool {
        matches!(self, Self::Detailed)
    }

    #[inline(always)]
    pub const fn show_histogram(self) -> bool {
        matches!(self, Self::Detailed)
    }
}

/// One captured frame's per-phase timing plus sync state. `Copy`, no heap.
#[derive(Clone, Copy, Debug)]
pub struct FrameStatsSample {
    pub host_nanos: u64,
    pub frame_us: u32,
    pub input_us: u32,
    pub update_us: u32,
    pub compose_us: u32,
    pub upload_us: u32,
    pub draw_us: u32,
    pub gpu_wait_us: u32,
    pub display_error_us: i32,
    pub catching_up: bool,
}

impl FrameStatsSample {
    #[inline(always)]
    pub const fn empty() -> Self {
        Self {
            host_nanos: 0,
            frame_us: 0,
            input_us: 0,
            update_us: 0,
            compose_us: 0,
            upload_us: 0,
            draw_us: 0,
            gpu_wait_us: 0,
            display_error_us: 0,
            catching_up: false,
        }
    }

    #[inline(always)]
    pub const fn is_empty(&self) -> bool {
        self.host_nanos == 0
    }

    /// Sum of the explicitly measured phases (everything that isn't idle headroom).
    #[inline(always)]
    pub const fn measured_us(&self) -> u32 {
        self.input_us
            .saturating_add(self.update_us)
            .saturating_add(self.compose_us)
            .saturating_add(self.upload_us)
            .saturating_add(self.draw_us)
            .saturating_add(self.gpu_wait_us)
    }

    #[inline(always)]
    pub const fn idle_us(&self) -> u32 {
        self.frame_us.saturating_sub(self.measured_us())
    }

    #[inline(always)]
    pub const fn cpu_work_us(&self) -> u32 {
        self.input_us
            .saturating_add(self.update_us)
            .saturating_add(self.compose_us)
            .saturating_add(self.upload_us)
            .saturating_add(self.draw_us)
    }
}

/// Precomputed sync-health readouts supplied to the overlay renderer.
#[derive(Clone, Copy, Debug)]
pub struct FrameStatsSummary {
    pub avg_frame_us: u32,
    pub p99_frame_us: u32,
    pub max_frame_us: u32,
    pub fps: f32,
    pub display_error_ms: f32,
    pub display_error_p99_ms: f32,
    pub display_catching_up: bool,
    pub in_gameplay: bool,
    pub audio_callback_gap_ms: f32,
    pub audio_underruns: u64,
    pub audio_output_delay_ms: f32,
    pub audio_queued_frames: u32,
    pub frame_jitter_us: u32,
    pub display_error_jitter_us: u32,
    pub spike_hold_us: u32,
    pub target_frame_us: u32,
    pub cpu_work_us: u32,
    pub gpu_wait_us: u32,
    pub over_budget_count: u32,
    pub catch_up_count: u32,
}

/// Fill `out` with a frame-interval histogram; the final bin absorbs overflow.
pub fn histogram(samples: &[FrameStatsSample], out: &mut [u32; HISTOGRAM_BINS], bin_width_us: u32) {
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
    fn frame_sample_preserves_phase_and_idle_math() {
        let sample = FrameStatsSample {
            host_nanos: 1,
            frame_us: 10_000,
            input_us: 100,
            update_us: 200,
            compose_us: 300,
            upload_us: 400,
            draw_us: 500,
            gpu_wait_us: 600,
            display_error_us: 0,
            catching_up: false,
        };
        assert_eq!(sample.cpu_work_us(), 1_500);
        assert_eq!(sample.measured_us(), 2_100);
        assert_eq!(sample.idle_us(), 7_900);
    }

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
        histogram(&samples, &mut bins, 1_000);
        assert_eq!(bins[1], 1);
        assert_eq!(bins[HISTOGRAM_BINS - 1], 1);
        assert_eq!(bins.iter().sum::<u32>(), 2);
    }

    #[test]
    fn overlay_keys_round_trip() {
        use OverlayAnchor::*;

        for anchor in [
            TopLeft,
            TopRight,
            BottomLeft,
            BottomRight,
            TopCenter,
            BottomCenter,
        ] {
            assert_eq!(OverlayAnchor::from_key(anchor.to_key()), Some(anchor));
        }
        assert_eq!(OverlayAnchor::from_key("auto"), None);
        assert_eq!(OverlayStyle::from_key("minimal"), OverlayStyle::Minimal);
        assert_eq!(OverlayStyle::from_key("unknown"), OverlayStyle::Detailed);
    }
}
