use deadsync_audio::OutputTimingSnapshot;
use deadsync_config::frame_pacing::{FixedFrameStatsRing, update_frame_stats_spike_hold};
use deadsync_profile::PlayStyle;
use deadsync_screens::diagnostics::{
    FrameStatsSample, FrameStatsSummary, OverlayAnchor, OverlayStyle,
};
use std::time::Duration;

const FRAME_STATS_SAMPLE_COUNT: usize = 128;

// Streaming-statistics tuning. The decaying histogram keeps a stable p99 over a long
// effective window without storing a long ring; the EWMA pair tracks a smoothed mean and
// jitter (standard deviation). All allocation-free and only touched while the overlay runs.
/// Bin count for the decaying histogram (covers up to `DHIST_BINS * DHIST_BUCKET_US`).
pub const DHIST_BINS: usize = 256;
/// Histogram bin resolution in microseconds.
pub const DHIST_BUCKET_US: u32 = 200;
/// Per-frame decay applied to every bin. `1/(1-gamma)` is the effective sample count (~1024).
pub const DHIST_GAMMA: f32 = 1023.0 / 1024.0;
/// EWMA smoothing factor for the displayed mean frame time (half-life about 34 frames).
pub const EWMA_ALPHA_MEAN: f32 = 0.02;
/// EWMA smoothing factor for the jitter (variance) estimate (slower than the mean).
pub const EWMA_ALPHA_VAR: f32 = 0.01;
/// Recompute the cached percentiles every Nth sample to keep the text steady.
pub const STATS_REFRESH_PERIOD: u8 = 20;

/// Default position used until the user explicitly moves the overlay.
pub const fn default_overlay_anchor() -> OverlayAnchor {
    OverlayAnchor::TopRight
}

/// Advance to the next anchor in the full or compact placement cycle.
pub fn next_overlay_anchor(current: OverlayAnchor, compact: bool) -> OverlayAnchor {
    use OverlayAnchor::*;
    let order: &[OverlayAnchor] = if compact {
        &[
            BottomCenter,
            TopCenter,
            BottomLeft,
            BottomRight,
            TopLeft,
            TopRight,
        ]
    } else {
        &[TopLeft, TopRight, BottomRight, BottomLeft]
    };
    let idx = order
        .iter()
        .position(|anchor| *anchor == current)
        .unwrap_or(usize::MAX);
    order[(idx.wrapping_add(1)) % order.len()]
}

#[derive(Clone, Copy, Debug)]
pub struct FrameStatsSummaryContext {
    pub fps: f32,
    pub display_error_seconds: f32,
    pub display_catching_up: bool,
    pub in_gameplay: bool,
    pub audio: OutputTimingSnapshot,
}

pub fn frame_stats_summary(
    metrics: FrameStatsMetrics,
    context: FrameStatsSummaryContext,
) -> FrameStatsSummary {
    FrameStatsSummary {
        avg_frame_us: metrics.avg_frame_us,
        p99_frame_us: metrics.p99_frame_us,
        max_frame_us: metrics.max_frame_us,
        fps: context.fps,
        display_error_ms: context.display_error_seconds * 1000.0,
        display_error_p99_ms: metrics.display_error_p99_ms,
        display_catching_up: context.display_catching_up,
        in_gameplay: context.in_gameplay,
        audio_callback_gap_ms: metrics.audio_callback_gap_ms,
        audio_underruns: context.audio.underrun_count,
        audio_output_delay_ms: context.audio.estimated_output_delay_ns as f32 / 1_000_000.0,
        audio_queued_frames: context.audio.queued_frames,
        frame_jitter_us: metrics.frame_jitter_us,
        display_error_jitter_us: metrics.display_error_jitter_us,
        spike_hold_us: metrics.spike_hold_us,
        target_frame_us: metrics.target_frame_us,
        cpu_work_us: metrics.cpu_work_us,
        gpu_wait_us: metrics.gpu_wait_us,
        over_budget_count: metrics.over_budget_count,
        catch_up_count: metrics.catch_up_count,
    }
}

pub fn frame_stats_target_us(refresh_ns: u64, fallback_interval: Option<Duration>) -> Option<u32> {
    if refresh_ns != 0 {
        return Some((refresh_ns / 1000) as u32);
    }
    fallback_interval.map(|interval| interval.as_micros().min(u128::from(u32::MAX)) as u32)
}

#[inline(always)]
pub const fn frame_stats_two_player(play_style: PlayStyle, num_players: usize) -> bool {
    matches!(play_style, PlayStyle::Versus | PlayStyle::Double) || num_players >= 2
}

/// Exponentially-decaying bucketed histogram with bounded, allocation-free storage.
#[derive(Clone, Copy)]
pub struct DecayingHist {
    bins: [f32; DHIST_BINS],
    total: f32,
}

impl DecayingHist {
    #[inline(always)]
    pub const fn new() -> Self {
        Self {
            bins: [0.0; DHIST_BINS],
            total: 0.0,
        }
    }

    #[inline(always)]
    pub fn reset(&mut self) {
        self.bins = [0.0; DHIST_BINS];
        self.total = 0.0;
    }

    /// Decay all bins and add the new `value_us` to its bucket.
    #[inline]
    pub fn update(&mut self, value_us: u32, gamma: f32, bucket_us: u32) {
        let bucket_us = bucket_us.max(1);
        let idx = (value_us / bucket_us).min(DHIST_BINS as u32 - 1) as usize;
        for bin in &mut self.bins {
            *bin *= gamma;
        }
        self.bins[idx] += 1.0;
        self.total = self.total * gamma + 1.0;
    }

    /// Weighted percentile in microseconds (bucket-quantized). Returns 0 until warmed up.
    #[inline]
    pub fn percentile_us(&self, pct: f32, bucket_us: u32) -> u32 {
        if self.total <= 0.0 {
            return 0;
        }
        let bucket_us = bucket_us.max(1);
        let target = (self.total * pct.clamp(0.0, 1.0)).max(f32::MIN_POSITIVE);
        let mut cumulative = 0.0;
        for (idx, &bin) in self.bins.iter().enumerate() {
            cumulative += bin;
            if cumulative >= target {
                return (idx as u32 + 1) * bucket_us;
            }
        }
        DHIST_BINS as u32 * bucket_us
    }

    /// Effective sample count represented by the decaying histogram.
    #[inline(always)]
    pub fn effective_n(&self) -> u32 {
        self.total.round().max(0.0) as u32
    }
}

impl Default for DecayingHist {
    fn default() -> Self {
        Self::new()
    }
}

/// Exponentially-weighted mean and variance using West's incremental EWMA.
#[derive(Clone, Copy)]
pub struct EwmaStats {
    mean: f32,
    var: f32,
    count: u32,
}

impl EwmaStats {
    #[inline(always)]
    pub const fn new() -> Self {
        Self {
            mean: 0.0,
            var: 0.0,
            count: 0,
        }
    }

    #[inline(always)]
    pub fn reset(&mut self) {
        self.mean = 0.0;
        self.var = 0.0;
        self.count = 0;
    }

    #[inline]
    pub fn update(&mut self, value: f32, alpha_mean: f32, alpha_var: f32) {
        if self.count == 0 {
            self.mean = value;
            self.var = 0.0;
        } else {
            let delta = value - self.mean;
            self.mean += alpha_mean * delta;
            self.var = (1.0 - alpha_var) * (self.var + alpha_var * delta * delta);
        }
        self.count = self.count.saturating_add(1);
    }

    #[inline(always)]
    pub fn mean(&self) -> f32 {
        self.mean
    }

    #[inline(always)]
    pub fn std_dev(&self) -> f32 {
        self.var.max(0.0).sqrt()
    }
}

impl Default for EwmaStats {
    fn default() -> Self {
        Self::new()
    }
}

/// Long-window streaming statistics for the frame-statistics overlay.
#[derive(Clone, Copy)]
pub struct FrameStatsLong {
    frame_hist: DecayingHist,
    error_hist: DecayingHist,
    frame_ewma: EwmaStats,
    error_ewma: EwmaStats,
    cpu_ewma: EwmaStats,
    gpu_ewma: EwmaStats,
    cached_p99_frame_us: u32,
    cached_p99_error_us: u32,
    refresh_counter: u8,
}

impl FrameStatsLong {
    #[inline(always)]
    pub const fn new() -> Self {
        Self {
            frame_hist: DecayingHist::new(),
            error_hist: DecayingHist::new(),
            frame_ewma: EwmaStats::new(),
            error_ewma: EwmaStats::new(),
            cpu_ewma: EwmaStats::new(),
            gpu_ewma: EwmaStats::new(),
            cached_p99_frame_us: 0,
            cached_p99_error_us: 0,
            refresh_counter: 0,
        }
    }

    #[inline(always)]
    pub fn reset(&mut self) {
        self.frame_hist.reset();
        self.error_hist.reset();
        self.frame_ewma.reset();
        self.error_ewma.reset();
        self.cpu_ewma.reset();
        self.gpu_ewma.reset();
        self.cached_p99_frame_us = 0;
        self.cached_p99_error_us = 0;
        self.refresh_counter = 0;
    }

    /// Feed one captured frame and refresh cached percentiles at a low cadence.
    #[inline]
    pub fn push(&mut self, sample: &FrameStatsSample) {
        let abs_err = sample.display_error_us.unsigned_abs();
        self.frame_hist
            .update(sample.frame_us, DHIST_GAMMA, DHIST_BUCKET_US);
        self.error_hist
            .update(abs_err, DHIST_GAMMA, DHIST_BUCKET_US);
        self.frame_ewma
            .update(sample.frame_us as f32, EWMA_ALPHA_MEAN, EWMA_ALPHA_VAR);
        self.error_ewma
            .update(abs_err as f32, EWMA_ALPHA_MEAN, EWMA_ALPHA_VAR);
        self.cpu_ewma
            .update(sample.cpu_work_us() as f32, EWMA_ALPHA_MEAN, EWMA_ALPHA_VAR);
        self.gpu_ewma
            .update(sample.gpu_wait_us as f32, EWMA_ALPHA_MEAN, EWMA_ALPHA_VAR);

        self.refresh_counter = self.refresh_counter.wrapping_add(1);
        if self.refresh_counter >= STATS_REFRESH_PERIOD {
            self.refresh_counter = 0;
            self.cached_p99_frame_us = self.frame_hist.percentile_us(0.99, DHIST_BUCKET_US);
            self.cached_p99_error_us = self.error_hist.percentile_us(0.99, DHIST_BUCKET_US);
        }
    }

    #[inline(always)]
    pub fn p99_frame_us(&self) -> u32 {
        self.cached_p99_frame_us
    }

    #[inline(always)]
    pub fn p99_error_us(&self) -> u32 {
        self.cached_p99_error_us
    }

    #[inline(always)]
    pub fn avg_frame_us(&self) -> u32 {
        self.frame_ewma.mean().max(0.0).round() as u32
    }

    #[inline(always)]
    pub fn avg_cpu_us(&self) -> u32 {
        self.cpu_ewma.mean().max(0.0).round() as u32
    }

    #[inline(always)]
    pub fn avg_gpu_us(&self) -> u32 {
        self.gpu_ewma.mean().max(0.0).round() as u32
    }

    #[inline(always)]
    pub fn frame_jitter_us(&self) -> u32 {
        self.frame_ewma.std_dev().round() as u32
    }

    #[inline(always)]
    pub fn error_jitter_us(&self) -> u32 {
        self.error_ewma.std_dev().round() as u32
    }

    #[inline(always)]
    pub fn effective_n(&self) -> u32 {
        self.frame_hist.effective_n()
    }
}

impl Default for FrameStatsLong {
    fn default() -> Self {
        Self::new()
    }
}

/// Single-pass bucketed percentile of `frame_us` across `samples`, in microseconds.
pub fn percentile_us(samples: &[FrameStatsSample], pct: f32, bucket_us: u32) -> u32 {
    const BUCKETS: usize = 256;
    let bucket_us = bucket_us.max(1);
    let mut hist = [0u32; BUCKETS];
    let mut count = 0u32;
    for sample in samples {
        if sample.is_empty() {
            continue;
        }
        let idx = (sample.frame_us / bucket_us).min(BUCKETS as u32 - 1) as usize;
        hist[idx] = hist[idx].saturating_add(1);
        count = count.saturating_add(1);
    }
    if count == 0 {
        return 0;
    }
    let target = (f64::from(count) * f64::from(pct.clamp(0.0, 1.0))).ceil() as u32;
    let target = target.max(1);
    let mut cumulative = 0u32;
    for (idx, bin) in hist.iter().enumerate() {
        cumulative = cumulative.saturating_add(*bin);
        if cumulative >= target {
            return (idx as u32 + 1) * bucket_us;
        }
    }
    BUCKETS as u32 * bucket_us
}

/// Aggregated values computed from the current frame-statistics window.
#[derive(Clone, Copy, Debug)]
pub struct FrameStatsMetrics {
    pub avg_frame_us: u32,
    pub p99_frame_us: u32,
    pub max_frame_us: u32,
    pub display_error_p99_ms: f32,
    pub frame_jitter_us: u32,
    pub display_error_jitter_us: u32,
    pub spike_hold_us: u32,
    pub target_frame_us: u32,
    pub cpu_work_us: u32,
    pub gpu_wait_us: u32,
    pub over_budget_count: u32,
    pub catch_up_count: u32,
    pub audio_callback_gap_ms: f32,
}

/// Borrowed data prepared for the screen-layer overlay renderer.
pub struct FrameStatsView<'a> {
    pub samples: &'a [FrameStatsSample],
    pub metrics: FrameStatsMetrics,
    pub anchor: OverlayAnchor,
    pub style: OverlayStyle,
}

/// Shell-owned frame-statistics collection and overlay state.
///
/// The game thread owns this single-threaded controller for the process
/// session. It uses a fixed 128-entry ring plus one reusable snapshot buffer;
/// collection starts when the overlay is enabled, and disabled state performs
/// no insertion work. New samples overwrite the oldest entry without scans,
/// disk access, or allocation. Long-window statistics are fixed-size and decay
/// in place. Resources are dropped with the shell, instrumentation is the
/// overlay itself, and per-frame work is bounded by the fixed ring and
/// histogram capacities.
pub struct FrameStatsController {
    samples: FixedFrameStatsRing<FrameStatsSample, FRAME_STATS_SAMPLE_COUNT>,
    scratch: Vec<FrameStatsSample>,
    long: FrameStatsLong,
    spike_us: u32,
    spike_ttl: u16,
    audio_gap_ms: f32,
    enabled: bool,
    anchor: OverlayAnchor,
    anchor_user_set: bool,
    style: OverlayStyle,
}

impl FrameStatsController {
    pub fn new(anchor_key: &str, style_key: &str) -> Self {
        let configured_anchor = OverlayAnchor::from_key(anchor_key);
        Self {
            samples: FixedFrameStatsRing::new(FrameStatsSample::empty()),
            scratch: Vec::with_capacity(FRAME_STATS_SAMPLE_COUNT),
            long: FrameStatsLong::new(),
            spike_us: 0,
            spike_ttl: 0,
            audio_gap_ms: 0.0,
            enabled: false,
            anchor: configured_anchor.unwrap_or(OverlayAnchor::TopLeft),
            anchor_user_set: configured_anchor.is_some(),
            style: OverlayStyle::from_key(style_key),
        }
    }

    #[inline(always)]
    pub fn enabled(&self) -> bool {
        self.enabled
    }

    pub fn toggle(&mut self) -> bool {
        self.enabled = !self.enabled;
        if !self.enabled {
            self.samples.clear();
            self.long.reset();
            self.spike_us = 0;
            self.spike_ttl = 0;
            self.audio_gap_ms = 0.0;
        }
        self.enabled
    }

    /// Apply automatic placement only while no persisted/user choice exists.
    pub fn use_default_anchor(&mut self) {
        if !self.anchor_user_set {
            self.anchor = default_overlay_anchor();
        }
    }

    pub fn cycle_anchor(&mut self, compact: bool) -> OverlayAnchor {
        self.anchor = next_overlay_anchor(self.anchor, compact);
        self.anchor_user_set = true;
        self.anchor
    }

    pub fn toggle_style(&mut self) -> OverlayStyle {
        self.style = self.style.toggle();
        self.style
    }

    /// Record one completed frame when collection is enabled.
    #[inline]
    pub fn record(&mut self, sample: FrameStatsSample) {
        if !self.enabled {
            return;
        }
        self.samples.push(sample);
        self.long.push(&sample);
        update_frame_stats_spike_hold(&mut self.spike_us, &mut self.spike_ttl, sample.frame_us);
    }

    /// Snapshot the fixed ring and prepare bounded aggregate values for drawing.
    pub fn view(
        &mut self,
        target_frame_us: Option<u32>,
        raw_audio_gap_ms: f32,
    ) -> Option<FrameStatsView<'_>> {
        if !self.enabled {
            return None;
        }
        self.samples.snapshot(&mut self.scratch);

        let mut max_frame_us = 0u32;
        for sample in &self.scratch {
            if !sample.is_empty() {
                max_frame_us = max_frame_us.max(sample.frame_us);
            }
        }
        let avg_frame_us = self.long.avg_frame_us();
        let target_frame_us = target_frame_us.unwrap_or(avg_frame_us);
        let over_budget_threshold = target_frame_us.saturating_mul(2).max(1);
        let mut over_budget_count = 0u32;
        let mut catch_up_count = 0u32;
        let mut previous_catch = false;
        for sample in &self.scratch {
            if sample.is_empty() {
                continue;
            }
            if sample.frame_us >= over_budget_threshold {
                over_budget_count = over_budget_count.saturating_add(1);
            }
            if sample.catching_up && !previous_catch {
                catch_up_count = catch_up_count.saturating_add(1);
            }
            previous_catch = sample.catching_up;
        }

        let audio_callback_gap_ms = if raw_audio_gap_ms > 0.0 {
            self.audio_gap_ms = if self.audio_gap_ms > 0.0 {
                self.audio_gap_ms + EWMA_ALPHA_MEAN * (raw_audio_gap_ms - self.audio_gap_ms)
            } else {
                raw_audio_gap_ms
            };
            self.audio_gap_ms
        } else {
            raw_audio_gap_ms
        };

        Some(FrameStatsView {
            samples: &self.scratch,
            metrics: FrameStatsMetrics {
                avg_frame_us,
                p99_frame_us: self.long.p99_frame_us(),
                max_frame_us,
                display_error_p99_ms: self.long.p99_error_us() as f32 / 1000.0,
                frame_jitter_us: self.long.frame_jitter_us(),
                display_error_jitter_us: self.long.error_jitter_us(),
                spike_hold_us: self.spike_us.max(max_frame_us),
                target_frame_us,
                cpu_work_us: self.long.avg_cpu_us(),
                gpu_wait_us: self.long.avg_gpu_us(),
                over_budget_count,
                catch_up_count,
                audio_callback_gap_ms,
            },
            anchor: self.anchor,
            style: self.style,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use deadsync_audio::{
        AudioOutputMode, OutputTelemetryBackend, OutputTelemetryClock, OutputTimingQuality,
    };

    fn audio_snapshot() -> OutputTimingSnapshot {
        OutputTimingSnapshot {
            backend: OutputTelemetryBackend::Unknown,
            requested_output_mode: AudioOutputMode::Auto,
            fallback_from_native: false,
            timing_clock: OutputTelemetryClock::Unknown,
            timing_quality: OutputTimingQuality::Unknown,
            sample_rate_hz: 0,
            device_period_ns: 0,
            stream_latency_ns: 0,
            buffer_frames: 0,
            padding_frames: 0,
            queued_frames: 0,
            estimated_output_delay_ns: 0,
            clock_fallback_count: 0,
            timing_sanity_failure_count: 0,
            underrun_count: 0,
        }
    }

    fn sample(frame_us: u32) -> FrameStatsSample {
        FrameStatsSample {
            host_nanos: 1,
            frame_us,
            ..FrameStatsSample::empty()
        }
    }

    #[test]
    fn target_frame_prefers_refresh_and_bounds_duration_fallback() {
        assert_eq!(
            frame_stats_target_us(16_666_667, Some(Duration::from_millis(8))),
            Some(16_666)
        );
        assert_eq!(
            frame_stats_target_us(0, Some(Duration::from_micros(8_333))),
            Some(8_333)
        );
        assert_eq!(frame_stats_target_us(0, None), None);
        assert_eq!(
            frame_stats_target_us(0, Some(Duration::from_secs(u64::MAX))),
            Some(u32::MAX)
        );
    }

    #[test]
    fn summary_combines_metrics_with_live_display_and_audio_health() {
        let summary = frame_stats_summary(
            FrameStatsMetrics {
                avg_frame_us: 8_000,
                p99_frame_us: 12_000,
                max_frame_us: 20_000,
                display_error_p99_ms: 2.5,
                frame_jitter_us: 300,
                display_error_jitter_us: 400,
                spike_hold_us: 20_000,
                target_frame_us: 8_333,
                cpu_work_us: 2_000,
                gpu_wait_us: 1_000,
                over_budget_count: 3,
                catch_up_count: 2,
                audio_callback_gap_ms: 4.5,
            },
            FrameStatsSummaryContext {
                fps: 120.0,
                display_error_seconds: -0.0015,
                display_catching_up: true,
                in_gameplay: true,
                audio: OutputTimingSnapshot {
                    underrun_count: 7,
                    estimated_output_delay_ns: 6_500_000,
                    queued_frames: 256,
                    ..audio_snapshot()
                },
            },
        );
        assert_eq!(summary.avg_frame_us, 8_000);
        assert_eq!(summary.p99_frame_us, 12_000);
        assert_eq!(summary.max_frame_us, 20_000);
        assert_eq!(summary.fps, 120.0);
        assert_eq!(summary.display_error_ms, -1.5);
        assert!(summary.display_catching_up && summary.in_gameplay);
        assert_eq!(summary.audio_callback_gap_ms, 4.5);
        assert_eq!(summary.audio_underruns, 7);
        assert_eq!(summary.audio_output_delay_ms, 6.5);
        assert_eq!(summary.audio_queued_frames, 256);
        assert_eq!(summary.target_frame_us, 8_333);
        assert_eq!(summary.over_budget_count, 3);
        assert_eq!(summary.catch_up_count, 2);
    }

    #[test]
    fn versus_double_and_multiple_fields_use_compact_anchor_cycle() {
        assert!(!frame_stats_two_player(PlayStyle::Single, 1));
        assert!(frame_stats_two_player(PlayStyle::Single, 2));
        assert!(frame_stats_two_player(PlayStyle::Versus, 1));
        assert!(frame_stats_two_player(PlayStyle::Double, 1));
    }

    #[test]
    fn percentile_ignores_empty_and_picks_high_bucket() {
        let mut samples = vec![FrameStatsSample::empty(); 4];
        samples.extend([
            sample(16_000),
            sample(16_000),
            sample(16_000),
            sample(50_000),
        ]);
        let p99 = percentile_us(&samples, 0.99, 1_000);
        assert!(p99 >= 50_000, "p99 was {p99}");
        let p50 = percentile_us(&samples, 0.5, 1_000);
        assert!((16_000..=17_000).contains(&p50), "p50 was {p50}");
    }

    #[test]
    fn percentile_empty_is_zero() {
        let samples = [FrameStatsSample::empty(); 3];
        assert_eq!(percentile_us(&samples, 0.99, 1_000), 0);
    }

    #[test]
    fn sample_phase_totals_are_saturating() {
        let sample = FrameStatsSample {
            host_nanos: 1,
            frame_us: 16_000,
            input_us: 1_000,
            update_us: 2_000,
            compose_us: 1_000,
            upload_us: 500,
            draw_us: 3_000,
            gpu_wait_us: 1_500,
            ..FrameStatsSample::empty()
        };
        assert_eq!(sample.measured_us(), 9_000);
        assert_eq!(sample.idle_us(), 7_000);
    }

    #[test]
    fn decaying_hist_percentile_tracks_distribution() {
        let mut hist = DecayingHist::new();
        for i in 0..2000 {
            hist.update(
                if i % 50 == 0 { 50_000 } else { 16_000 },
                DHIST_GAMMA,
                DHIST_BUCKET_US,
            );
        }
        let p50 = hist.percentile_us(0.50, DHIST_BUCKET_US);
        let p99 = hist.percentile_us(0.99, DHIST_BUCKET_US);
        assert!((16_000..=16_200).contains(&p50), "p50 was {p50}");
        assert!(p99 >= 49_800, "p99 was {p99}");
        assert!(hist.effective_n() <= 1100);
    }

    #[test]
    fn decaying_hist_is_stable_against_single_outlier() {
        let mut hist = DecayingHist::new();
        for _ in 0..2000 {
            hist.update(16_000, DHIST_GAMMA, DHIST_BUCKET_US);
        }
        let before = hist.percentile_us(0.99, DHIST_BUCKET_US);
        hist.update(80_000, DHIST_GAMMA, DHIST_BUCKET_US);
        for _ in 0..200 {
            hist.update(16_000, DHIST_GAMMA, DHIST_BUCKET_US);
        }
        assert_eq!(before, hist.percentile_us(0.99, DHIST_BUCKET_US));
    }

    #[test]
    fn ewma_converges_and_reports_jitter() {
        let mut steady = EwmaStats::new();
        for _ in 0..1000 {
            steady.update(16_000.0, EWMA_ALPHA_MEAN, EWMA_ALPHA_VAR);
        }
        assert!((steady.mean() - 16_000.0).abs() < 1.0);
        assert!(steady.std_dev() < 50.0);

        let mut jittery = EwmaStats::new();
        for i in 0..4000 {
            let value = if i % 2 == 0 { 14_000.0 } else { 18_000.0 };
            jittery.update(value, EWMA_ALPHA_MEAN, EWMA_ALPHA_VAR);
        }
        assert!((jittery.mean() - 16_000.0).abs() < 500.0);
        assert!(jittery.std_dev() > 500.0);
    }

    #[test]
    fn long_stats_cache_percentiles_at_cadence() {
        let mut stats = FrameStatsLong::new();
        let mut sample = sample(16_000);
        for _ in 0..(STATS_REFRESH_PERIOD - 1) {
            stats.push(&sample);
        }
        assert_eq!(stats.p99_frame_us(), 0);
        stats.push(&sample);
        assert!(stats.p99_frame_us() > 0);

        sample.display_error_us = 1_200;
        for _ in 0..STATS_REFRESH_PERIOD {
            stats.push(&sample);
        }
        assert!(stats.p99_error_us() > 0);
        stats.reset();
        assert_eq!(stats.p99_frame_us(), 0);
        assert_eq!(stats.effective_n(), 0);
    }

    #[test]
    fn overlay_keys_and_cycles_are_stable() {
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
        assert_eq!(next_overlay_anchor(TopLeft, false), TopRight);
        assert_eq!(next_overlay_anchor(BottomLeft, false), TopLeft);
        assert_eq!(next_overlay_anchor(BottomCenter, true), TopCenter);
        assert_eq!(next_overlay_anchor(TopCenter, false), TopLeft);
        assert_eq!(default_overlay_anchor(), TopRight);

        assert_eq!(OverlayStyle::from_key("minimal"), OverlayStyle::Minimal);
        assert_eq!(OverlayStyle::from_key("unknown"), OverlayStyle::Detailed);
        assert_eq!(OverlayStyle::Detailed.toggle(), OverlayStyle::Minimal);
        assert!(OverlayStyle::Detailed.show_p99());
        assert!(!OverlayStyle::Minimal.show_histogram());
    }

    #[test]
    fn controller_gates_collection_and_prepares_bounded_view() {
        let mut controller = FrameStatsController::new("auto", "detailed");
        controller.record(sample(20_000));
        assert!(controller.view(Some(10_000), 8.0).is_none());

        assert!(controller.toggle());
        controller.use_default_anchor();
        let mut first = sample(20_000);
        first.catching_up = true;
        controller.record(first);
        controller.record(sample(5_000));
        let view = controller
            .view(Some(10_000), 8.0)
            .expect("enabled controller produces a view");
        assert_eq!(view.samples.len(), 2);
        assert_eq!(view.metrics.max_frame_us, 20_000);
        assert_eq!(view.metrics.over_budget_count, 1);
        assert_eq!(view.metrics.catch_up_count, 1);
        assert_eq!(view.metrics.audio_callback_gap_ms, 8.0);
        assert_eq!(view.anchor, OverlayAnchor::TopRight);

        assert!(!controller.toggle());
        assert!(controller.view(None, 0.0).is_none());
    }
}
