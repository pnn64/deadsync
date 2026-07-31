use deadlib_present::compose::{COMPOSE_STORAGE_NAMES, COMPOSE_STORAGE_SLOTS, ComposeStorageStats};
use deadlib_render::{
    ClockDomainTrace, DrawStats, PresentModeTrace,
    draw_prep::{DRAW_STORAGE_NAMES, DRAW_STORAGE_SLOTS, DrawStorageStats},
};
use std::time::{Duration, Instant};

const LOG_INTERVAL: Duration = Duration::from_secs(5);
const REDRAW_DELIVERY_SLOW_US: u32 = 1_000;
const REDRAW_DELIVERY_BAD_US: u32 = 2_000;
const PRESENT_SLOW_US: u32 = 1_000;
const PRESENT_SPIKE_US: u32 = 3_000;
const PHASE_COUNT: usize = Phase::BackendRecord as usize + 1;
const PHASE_FINE_BINS: usize = 1_024;
const PHASE_HIST_BINS: usize = 2_048;
const PHASE_COARSE_BUCKET_US: u32 = 16;
const STORAGE_SLOTS: usize = 1 + COMPOSE_STORAGE_SLOTS + DRAW_STORAGE_SLOTS;

#[derive(Clone, Copy, Debug, Default)]
pub struct GameplayPacingPhases {
    pub actor_build_us: u32,
    pub build_screen_us: u32,
    pub compose_us: u32,
    pub upload_us: u32,
    pub storage: GameplayStorageSample,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GameplayStorageSample {
    capacities: [u32; STORAGE_SLOTS],
}

impl GameplayStorageSample {
    pub fn new(
        actor_capacity: usize,
        compose: ComposeStorageStats,
        draw: DrawStorageStats,
    ) -> Self {
        let mut capacities = [0; STORAGE_SLOTS];
        capacities[0] = saturating_u32(actor_capacity);
        capacities[1..1 + COMPOSE_STORAGE_SLOTS].copy_from_slice(&compose.capacities);
        capacities[1 + COMPOSE_STORAGE_SLOTS..].copy_from_slice(&draw.capacities);
        Self { capacities }
    }
}

#[derive(Clone, Copy)]
struct StorageTrace {
    last: [u32; STORAGE_SLOTS],
    high: [u32; STORAGE_SLOTS],
    growths: [u32; STORAGE_SLOTS],
    initialized: bool,
}

impl StorageTrace {
    const fn new() -> Self {
        Self {
            last: [0; STORAGE_SLOTS],
            high: [0; STORAGE_SLOTS],
            growths: [0; STORAGE_SLOTS],
            initialized: false,
        }
    }

    fn record(&mut self, sample: GameplayStorageSample) {
        if !self.initialized {
            // The first traced frame is the warmed baseline, not a growth event.
            self.last = sample.capacities;
            self.high = sample.capacities;
            self.initialized = true;
            return;
        }
        for (index, capacity) in sample.capacities.into_iter().enumerate() {
            self.growths[index] =
                self.growths[index].saturating_add(u32::from(capacity > self.last[index]));
            self.last[index] = capacity;
            self.high[index] = self.high[index].max(capacity);
        }
    }

    fn reset_window(&mut self) {
        self.high = self.last;
        self.growths.fill(0);
    }

    fn total_growths(&self) -> u32 {
        self.growths.iter().copied().fold(0u32, u32::saturating_add)
    }
}

impl std::fmt::Display for StorageTrace {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "actor:{}/{}", self.high[0], self.growths[0])?;
        for (offset, name) in COMPOSE_STORAGE_NAMES.iter().enumerate() {
            let index = 1 + offset;
            write!(f, " {name}:{}/{}", self.high[index], self.growths[index])?;
        }
        for (offset, name) in DRAW_STORAGE_NAMES.iter().enumerate() {
            let index = 1 + COMPOSE_STORAGE_SLOTS + offset;
            write!(
                f,
                " draw_{name}:{}/{}",
                self.high[index], self.growths[index]
            )?;
        }
        Ok(())
    }
}

#[derive(Clone, Copy)]
enum Phase {
    Frame,
    ActorBuild,
    BuildScreen,
    Compose,
    Upload,
    Draw,
    BackendSetup,
    BackendPrepare,
    BackendRecord,
}

impl Phase {
    const fn index(self) -> usize {
        self as usize
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct PhaseTail {
    p50: u32,
    p95: u32,
    p99: u32,
    worst: u32,
}

impl std::fmt::Display for PhaseTail {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}/{}/{}/{}", self.p50, self.p95, self.p99, self.worst)
    }
}

struct PhaseHist {
    bins: [u32; PHASE_HIST_BINS],
    samples: u32,
    capped: u32,
    worst: u32,
}

impl PhaseHist {
    const fn new() -> Self {
        Self {
            bins: [0; PHASE_HIST_BINS],
            samples: 0,
            capped: 0,
            worst: 0,
        }
    }

    fn reset(&mut self) {
        self.bins.fill(0);
        self.samples = 0;
        self.capped = 0;
        self.worst = 0;
    }

    fn record(&mut self, value_us: u32) {
        let (bucket, capped) = phase_bucket(value_us);
        self.bins[bucket] = self.bins[bucket].saturating_add(1);
        self.samples = self.samples.saturating_add(1);
        self.capped = self.capped.saturating_add(u32::from(capped));
        self.worst = self.worst.max(value_us);
    }

    fn tail(&self) -> PhaseTail {
        if self.samples == 0 {
            return PhaseTail::default();
        }
        let targets =
            [50, 95, 99].map(|pct| self.samples.saturating_mul(pct).saturating_add(99) / 100);
        let mut values = [0; 3];
        let mut next = 0usize;
        let mut count = 0u32;
        for (index, bin) in self.bins.iter().copied().enumerate() {
            count = count.saturating_add(bin);
            while next < targets.len() && count >= targets[next] {
                values[next] = phase_bucket_upper(index);
                next += 1;
            }
            if next == targets.len() {
                break;
            }
        }
        PhaseTail {
            p50: values[0],
            p95: values[1],
            p99: values[2],
            worst: self.worst,
        }
    }
}

pub struct GameplayPacingTrace {
    started_at: Instant,
    frames: u32,
    chain_frames: u32,
    other_frames: u32,
    dt_sum_us: u64,
    dt_max_us: u32,
    redraw_late_sum_us: u64,
    redraw_late_max_us: u32,
    redraw_delivery_sum_us: u64,
    redraw_delivery_max_us: u32,
    redraw_delivery_over_1ms: u32,
    redraw_delivery_over_2ms: u32,
    draw_sum_us: u64,
    draw_max_us: u32,
    present_sum_us: u64,
    present_max_us: u32,
    present_over_1ms: u32,
    present_over_3ms: u32,
    draw_setup_sum_us: u64,
    draw_prepare_sum_us: u64,
    draw_record_sum_us: u64,
    display_error_abs_sum_us: u64,
    display_error_abs_max_us: u32,
    display_error_last_us: i32,
    display_catching_up_frames: u32,
    display_catching_up_last: bool,
    present_last_mode: PresentModeTrace,
    present_display_clock_last: ClockDomainTrace,
    present_host_clock_last: ClockDomainTrace,
    present_inflight_sum: u64,
    present_inflight_max: u8,
    present_image_wait_frames: u32,
    present_back_pressure_frames: u32,
    present_queue_idle_frames: u32,
    present_suboptimal_frames: u32,
    present_host_mapped_frames: u32,
    present_calibration_error_sum_ns: u64,
    present_calibration_error_max_ns: u64,
    present_interval_sum_ns: u64,
    present_interval_max_ns: u64,
    present_interval_samples: u32,
    present_margin_sum_ns: u64,
    present_margin_max_ns: u64,
    present_margin_samples: u32,
    storage: StorageTrace,
    phase_hists: Box<[PhaseHist]>,
}

impl GameplayPacingTrace {
    pub fn new(now: Instant) -> Self {
        let mut phase_hists = Vec::with_capacity(PHASE_COUNT);
        phase_hists.resize_with(PHASE_COUNT, PhaseHist::new);
        Self::with_phase_storage(now, phase_hists.into_boxed_slice())
    }

    fn with_phase_storage(now: Instant, phase_hists: Box<[PhaseHist]>) -> Self {
        Self {
            started_at: now,
            frames: 0,
            chain_frames: 0,
            other_frames: 0,
            dt_sum_us: 0,
            dt_max_us: 0,
            redraw_late_sum_us: 0,
            redraw_late_max_us: 0,
            redraw_delivery_sum_us: 0,
            redraw_delivery_max_us: 0,
            redraw_delivery_over_1ms: 0,
            redraw_delivery_over_2ms: 0,
            draw_sum_us: 0,
            draw_max_us: 0,
            present_sum_us: 0,
            present_max_us: 0,
            present_over_1ms: 0,
            present_over_3ms: 0,
            draw_setup_sum_us: 0,
            draw_prepare_sum_us: 0,
            draw_record_sum_us: 0,
            display_error_abs_sum_us: 0,
            display_error_abs_max_us: 0,
            display_error_last_us: 0,
            display_catching_up_frames: 0,
            display_catching_up_last: false,
            present_last_mode: PresentModeTrace::Unknown,
            present_display_clock_last: ClockDomainTrace::Unknown,
            present_host_clock_last: ClockDomainTrace::Unknown,
            present_inflight_sum: 0,
            present_inflight_max: 0,
            present_image_wait_frames: 0,
            present_back_pressure_frames: 0,
            present_queue_idle_frames: 0,
            present_suboptimal_frames: 0,
            present_host_mapped_frames: 0,
            present_calibration_error_sum_ns: 0,
            present_calibration_error_max_ns: 0,
            present_interval_sum_ns: 0,
            present_interval_max_ns: 0,
            present_interval_samples: 0,
            present_margin_sum_ns: 0,
            present_margin_max_ns: 0,
            present_margin_samples: 0,
            storage: StorageTrace::new(),
            phase_hists,
        }
    }

    #[inline(always)]
    pub fn reset(&mut self, now: Instant) {
        let mut phase_hists = std::mem::take(&mut self.phase_hists);
        for hist in &mut phase_hists {
            hist.reset();
        }
        *self = Self::with_phase_storage(now, phase_hists);
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "the trace records one value for each measured frame phase"
    )]
    pub fn record_frame(
        &mut self,
        now: Instant,
        gameplay: bool,
        frame_seconds: f32,
        pre_redraw_gap_us: u32,
        request_to_redraw_us: u32,
        redraw_request_reason: &'static str,
        draw_us: u32,
        draw_stats: DrawStats,
        phases: GameplayPacingPhases,
        display_error_seconds: f32,
        display_catching_up: bool,
    ) {
        self.record_frame_if_enabled(
            log::log_enabled!(log::Level::Trace),
            now,
            gameplay,
            frame_seconds,
            pre_redraw_gap_us,
            request_to_redraw_us,
            redraw_request_reason,
            draw_us,
            draw_stats,
            phases,
            display_error_seconds,
            display_catching_up,
        );
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "the trace records one value for each measured frame phase"
    )]
    fn record_frame_if_enabled(
        &mut self,
        enabled: bool,
        now: Instant,
        gameplay: bool,
        frame_seconds: f32,
        pre_redraw_gap_us: u32,
        request_to_redraw_us: u32,
        redraw_request_reason: &'static str,
        draw_us: u32,
        draw_stats: DrawStats,
        phases: GameplayPacingPhases,
        display_error_seconds: f32,
        display_catching_up: bool,
    ) {
        if !enabled {
            if self.frames != 0 || self.storage.initialized {
                self.reset(now);
            }
            return;
        }
        if !gameplay {
            self.reset(now);
            return;
        }
        if self.frames == 0 {
            self.started_at = now;
        }
        let redraw_late_us = pre_redraw_gap_us.saturating_sub(request_to_redraw_us);
        let dt_us_f = (frame_seconds * 1_000_000.0).max(0.0);
        let dt_us = if dt_us_f > u32::MAX as f32 {
            u32::MAX
        } else {
            dt_us_f as u32
        };
        self.frames = self.frames.saturating_add(1);
        if redraw_request_reason == "chain" {
            self.chain_frames = self.chain_frames.saturating_add(1);
        } else {
            self.other_frames = self.other_frames.saturating_add(1);
        }
        self.dt_sum_us = self.dt_sum_us.saturating_add(u64::from(dt_us));
        self.dt_max_us = self.dt_max_us.max(dt_us);
        self.redraw_late_sum_us = self
            .redraw_late_sum_us
            .saturating_add(u64::from(redraw_late_us));
        self.redraw_late_max_us = self.redraw_late_max_us.max(redraw_late_us);
        self.redraw_delivery_sum_us = self
            .redraw_delivery_sum_us
            .saturating_add(u64::from(request_to_redraw_us));
        self.redraw_delivery_max_us = self.redraw_delivery_max_us.max(request_to_redraw_us);
        self.redraw_delivery_over_1ms += u32::from(request_to_redraw_us >= REDRAW_DELIVERY_SLOW_US);
        self.redraw_delivery_over_2ms += u32::from(request_to_redraw_us >= REDRAW_DELIVERY_BAD_US);
        self.draw_sum_us = self.draw_sum_us.saturating_add(u64::from(draw_us));
        self.draw_max_us = self.draw_max_us.max(draw_us);
        self.present_sum_us = self
            .present_sum_us
            .saturating_add(u64::from(draw_stats.present_us));
        self.present_max_us = self.present_max_us.max(draw_stats.present_us);
        self.present_over_1ms += u32::from(draw_stats.present_us >= PRESENT_SLOW_US);
        self.present_over_3ms += u32::from(draw_stats.present_us >= PRESENT_SPIKE_US);
        self.draw_setup_sum_us = self
            .draw_setup_sum_us
            .saturating_add(u64::from(draw_stats.backend_setup_us));
        self.draw_prepare_sum_us = self
            .draw_prepare_sum_us
            .saturating_add(u64::from(draw_stats.backend_prepare_us));
        self.draw_record_sum_us = self
            .draw_record_sum_us
            .saturating_add(u64::from(draw_stats.backend_record_us));
        self.record_phase(Phase::Frame, dt_us);
        self.record_phase(Phase::ActorBuild, phases.actor_build_us);
        self.record_phase(Phase::BuildScreen, phases.build_screen_us);
        self.record_phase(Phase::Compose, phases.compose_us);
        self.record_phase(Phase::Upload, phases.upload_us);
        self.record_phase(Phase::Draw, draw_us);
        self.record_phase(Phase::BackendSetup, draw_stats.backend_setup_us);
        self.record_phase(Phase::BackendPrepare, draw_stats.backend_prepare_us);
        self.record_phase(Phase::BackendRecord, draw_stats.backend_record_us);
        self.storage.record(phases.storage);

        let error_us = (f64::from(display_error_seconds) * 1_000_000.0).round() as i64;
        let error_abs_us = error_us.unsigned_abs().min(u64::from(u32::MAX)) as u32;
        self.display_error_last_us =
            error_us.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32;
        self.display_error_abs_sum_us = self
            .display_error_abs_sum_us
            .saturating_add(u64::from(error_abs_us));
        self.display_error_abs_max_us = self.display_error_abs_max_us.max(error_abs_us);
        self.display_catching_up_frames += u32::from(display_catching_up);
        self.display_catching_up_last = display_catching_up;

        let present = draw_stats.present_stats;
        self.present_last_mode = present.mode;
        self.present_display_clock_last = present.display_clock;
        self.present_host_clock_last = present.host_clock;
        self.present_inflight_sum = self
            .present_inflight_sum
            .saturating_add(u64::from(present.in_flight_images));
        self.present_inflight_max = self.present_inflight_max.max(present.in_flight_images);
        self.present_image_wait_frames += u32::from(present.waited_for_image);
        self.present_back_pressure_frames += u32::from(present.applied_back_pressure);
        self.present_queue_idle_frames += u32::from(present.queue_idle_waited);
        self.present_suboptimal_frames += u32::from(present.suboptimal);
        self.present_host_mapped_frames += u32::from(present.host_present_ns != 0);
        self.present_calibration_error_sum_ns = self
            .present_calibration_error_sum_ns
            .saturating_add(present.calibration_error_ns);
        self.present_calibration_error_max_ns = self
            .present_calibration_error_max_ns
            .max(present.calibration_error_ns);
        if present.actual_interval_ns > 0 {
            self.present_interval_sum_ns = self
                .present_interval_sum_ns
                .saturating_add(present.actual_interval_ns);
            self.present_interval_max_ns =
                self.present_interval_max_ns.max(present.actual_interval_ns);
            self.present_interval_samples = self.present_interval_samples.saturating_add(1);
        }
        if present.completed_present_id != 0 {
            self.present_margin_sum_ns = self
                .present_margin_sum_ns
                .saturating_add(present.present_margin_ns);
            self.present_margin_max_ns = self.present_margin_max_ns.max(present.present_margin_ns);
            self.present_margin_samples = self.present_margin_samples.saturating_add(1);
        }
        if now.duration_since(self.started_at) >= LOG_INTERVAL {
            self.log_and_reset(now);
        }
    }

    fn log_and_reset(&mut self, now: Instant) {
        let frames = self.frames.max(1);
        let ms = |sum_us: u64| sum_us as f64 / frames as f64 / 1000.0;
        let interval_samples = self.present_interval_samples.max(1);
        let margin_samples = self.present_margin_samples.max(1);
        let audio = deadsync_audio_stream::get_output_timing_snapshot();
        let frame_tail = self.phase_tail(Phase::Frame);
        let actor_tail = self.phase_tail(Phase::ActorBuild);
        let build_tail = self.phase_tail(Phase::BuildScreen);
        let compose_tail = self.phase_tail(Phase::Compose);
        let upload_tail = self.phase_tail(Phase::Upload);
        let draw_tail = self.phase_tail(Phase::Draw);
        let setup_tail = self.phase_tail(Phase::BackendSetup);
        let prepare_tail = self.phase_tail(Phase::BackendPrepare);
        let record_tail = self.phase_tail(Phase::BackendRecord);
        let tail_samples = self.phase_hist(Phase::Frame).samples;
        let tail_capped = self
            .phase_hists
            .iter()
            .fold(0u32, |sum, hist| sum.saturating_add(hist.capped));
        log::trace!(
            "Gameplay frame pacing: frames={} req=[chain:{} other:{}] dt_ms=[avg:{:.3} max:{:.3}] redraw_ms=[late_avg:{:.3} late_max:{:.3} deliver_avg:{:.3} deliver_max:{:.3} >=1ms:{} >=2ms:{}] draw_ms=[avg:{:.3} max:{:.3}] present_ms=[avg:{:.3} max:{:.3} >=1ms:{} >=3ms:{}] draw_cpu_ms=[setup_avg:{:.3} prep_avg:{:.3} record_avg:{:.3}] tails_us=[order:p50/p95/p99/worst samples:{} capped:{} frame:{} actor:{} build:{} compose:{} upload:{} draw:{} setup:{} prep:{} record:{}] cpu_storage=[order:capacity/growth_events total_growth_events:{} {}] display_dbg=[err_last_ms:{:+.3} abs_avg_ms:{:.3} abs_max_ms:{:.3} catch:{} catch_last:{}] present_dbg=[mode:{} display:{} host:{} mapped:{} inflight_avg:{:.2} inflight_max:{} image_wait:{} back_pressure:{} queue_idle:{} subopt:{} interval_ms_avg:{:.3} interval_ms_max:{:.3} margin_ms_avg:{:.3} margin_ms_max:{:.3} cal_ms_avg:{:.3} cal_ms_max:{:.3}] audio_dbg=[path:{} req:{} fallback:{} clock:{} qual:{} sf:{} cf:{} rate:{} buf:{} pad:{} q:{} tick_ms:{:.3} span_ms:{:.3} out_ms:{:.3} underruns:{}]",
            frames,
            self.chain_frames,
            self.other_frames,
            ms(self.dt_sum_us),
            self.dt_max_us as f64 / 1000.0,
            ms(self.redraw_late_sum_us),
            self.redraw_late_max_us as f64 / 1000.0,
            ms(self.redraw_delivery_sum_us),
            self.redraw_delivery_max_us as f64 / 1000.0,
            self.redraw_delivery_over_1ms,
            self.redraw_delivery_over_2ms,
            ms(self.draw_sum_us),
            self.draw_max_us as f64 / 1000.0,
            ms(self.present_sum_us),
            self.present_max_us as f64 / 1000.0,
            self.present_over_1ms,
            self.present_over_3ms,
            ms(self.draw_setup_sum_us),
            ms(self.draw_prepare_sum_us),
            ms(self.draw_record_sum_us),
            tail_samples,
            tail_capped,
            frame_tail,
            actor_tail,
            build_tail,
            compose_tail,
            upload_tail,
            draw_tail,
            setup_tail,
            prepare_tail,
            record_tail,
            self.storage.total_growths(),
            self.storage,
            self.display_error_last_us as f64 / 1000.0,
            self.display_error_abs_sum_us as f64 / frames as f64 / 1000.0,
            self.display_error_abs_max_us as f64 / 1000.0,
            self.display_catching_up_frames,
            u8::from(self.display_catching_up_last),
            self.present_last_mode,
            self.present_display_clock_last,
            self.present_host_clock_last,
            self.present_host_mapped_frames,
            self.present_inflight_sum as f64 / frames as f64,
            self.present_inflight_max,
            self.present_image_wait_frames,
            self.present_back_pressure_frames,
            self.present_queue_idle_frames,
            self.present_suboptimal_frames,
            self.present_interval_sum_ns as f64 / interval_samples as f64 / 1_000_000.0,
            self.present_interval_max_ns as f64 / 1_000_000.0,
            self.present_margin_sum_ns as f64 / margin_samples as f64 / 1_000_000.0,
            self.present_margin_max_ns as f64 / 1_000_000.0,
            self.present_calibration_error_sum_ns as f64 / frames as f64 / 1_000_000.0,
            self.present_calibration_error_max_ns as f64 / 1_000_000.0,
            audio.backend,
            audio.requested_output_mode.as_str(),
            audio.fallback_from_native,
            audio.timing_clock,
            audio.timing_quality,
            audio.timing_sanity_failure_count,
            audio.clock_fallback_count,
            audio.sample_rate_hz,
            audio.buffer_frames,
            audio.padding_frames,
            audio.queued_frames,
            audio.device_period_ns as f64 / 1_000_000.0,
            audio.stream_latency_ns as f64 / 1_000_000.0,
            audio.estimated_output_delay_ns as f64 / 1_000_000.0,
            audio.underrun_count
        );
        let mut storage = self.storage;
        self.reset(now);
        storage.reset_window();
        self.storage = storage;
    }

    fn record_phase(&mut self, phase: Phase, value_us: u32) {
        self.phase_hists[phase.index()].record(value_us);
    }

    fn phase_hist(&self, phase: Phase) -> &PhaseHist {
        &self.phase_hists[phase.index()]
    }

    fn phase_tail(&self, phase: Phase) -> PhaseTail {
        self.phase_hist(phase).tail()
    }
}

fn phase_bucket(value_us: u32) -> (usize, bool) {
    if value_us < PHASE_FINE_BINS as u32 {
        return (value_us as usize, false);
    }
    let coarse = (value_us - PHASE_FINE_BINS as u32) / PHASE_COARSE_BUCKET_US;
    let max_coarse = (PHASE_HIST_BINS - PHASE_FINE_BINS - 1) as u32;
    (
        PHASE_FINE_BINS + coarse.min(max_coarse) as usize,
        coarse > max_coarse,
    )
}

const fn phase_bucket_upper(index: usize) -> u32 {
    if index < PHASE_FINE_BINS {
        index as u32
    } else {
        PHASE_FINE_BINS as u32 + (index - PHASE_FINE_BINS + 1) as u32 * PHASE_COARSE_BUCKET_US - 1
    }
}

const fn saturating_u32(value: usize) -> u32 {
    if value > u32::MAX as usize {
        u32::MAX
    } else {
        value as u32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_frame_accumulates_renderer_and_display_stats() {
        let now = Instant::now();
        let mut trace = GameplayPacingTrace::new(now);
        let draw = DrawStats {
            present_us: 1_500,
            backend_setup_us: 100,
            backend_prepare_us: 200,
            backend_record_us: 300,
            ..DrawStats::default()
        };
        trace.record_frame_if_enabled(
            true,
            now,
            true,
            0.016,
            2_000,
            500,
            "chain",
            2_500,
            draw,
            GameplayPacingPhases {
                actor_build_us: 400,
                build_screen_us: 600,
                compose_us: 1_000,
                upload_us: 50,
                storage: GameplayStorageSample::new(
                    256,
                    ComposeStorageStats {
                        capacities: [32; COMPOSE_STORAGE_SLOTS],
                    },
                    DrawStorageStats {
                        capacities: [64; DRAW_STORAGE_SLOTS],
                    },
                ),
            },
            -0.002,
            true,
        );
        assert_eq!(trace.frames, 1);
        assert_eq!(trace.chain_frames, 1);
        assert_eq!(trace.dt_sum_us, 16_000);
        assert_eq!(trace.redraw_late_sum_us, 1_500);
        assert_eq!(trace.present_over_1ms, 1);
        assert_eq!(trace.display_error_last_us, -2_000);
        assert_eq!(trace.display_catching_up_frames, 1);
        assert_eq!(trace.storage.high[0], 256);
        assert_eq!(trace.storage.high[1], 32);
        assert_eq!(trace.storage.high[1 + COMPOSE_STORAGE_SLOTS], 64);
        assert_eq!(trace.storage.total_growths(), 0);
        assert_eq!(
            trace.phase_tail(Phase::Compose),
            PhaseTail {
                p50: 1_000,
                p95: 1_000,
                p99: 1_000,
                worst: 1_000,
            }
        );
    }

    #[test]
    fn disabled_trace_stays_idle() {
        let now = Instant::now();
        let mut trace = GameplayPacingTrace::new(now);
        let mut storage = GameplayStorageSample::default();
        storage.capacities.fill(8);
        trace.storage.record(storage);

        trace.record_frame_if_enabled(
            false,
            now,
            true,
            0.016,
            2_000,
            500,
            "chain",
            2_500,
            DrawStats::default(),
            GameplayPacingPhases::default(),
            -0.002,
            true,
        );

        assert_eq!(trace.frames, 0);
        assert!(!trace.storage.initialized);
        assert_eq!(trace.phase_hist(Phase::Frame).samples, 0);
    }

    #[test]
    fn phase_hist_uses_nearest_rank_percentiles_and_reset_reuses_storage() {
        let now = Instant::now();
        let mut trace = GameplayPacingTrace::new(now);
        for value in 1..=100 {
            trace.record_phase(Phase::Compose, value);
        }

        assert_eq!(
            trace.phase_tail(Phase::Compose),
            PhaseTail {
                p50: 50,
                p95: 95,
                p99: 99,
                worst: 100,
            }
        );

        let hist_ptr = trace.phase_hists.as_ptr();
        trace.reset(now);
        assert_eq!(trace.phase_hists.as_ptr(), hist_ptr);
        assert_eq!(trace.phase_hist(Phase::Compose).samples, 0);
    }

    #[test]
    fn phase_hist_quantizes_slow_values_and_keeps_exact_worst() {
        let mut hist = PhaseHist::new();
        hist.record(PHASE_FINE_BINS as u32);
        hist.record(20_000);

        assert_eq!(
            hist.tail().p50,
            PHASE_FINE_BINS as u32 + PHASE_COARSE_BUCKET_US - 1
        );
        assert_eq!(hist.capped, 1);
        assert_eq!(hist.tail().worst, 20_000);
    }

    #[test]
    fn storage_trace_counts_capacity_growth_and_preserves_window_baseline() {
        let mut trace = StorageTrace::new();
        let mut first = GameplayStorageSample::default();
        first.capacities.fill(8);
        trace.record(first);

        let mut grown = first;
        grown.capacities[0] = 16;
        grown.capacities[1 + COMPOSE_STORAGE_SLOTS] = 32;
        trace.record(grown);

        assert_eq!(trace.total_growths(), 2);
        assert_eq!(trace.high[0], 16);
        assert_eq!(trace.high[1 + COMPOSE_STORAGE_SLOTS], 32);

        trace.reset_window();
        assert_eq!(trace.total_growths(), 0);
        assert_eq!(trace.high, grown.capacities);
    }
}
