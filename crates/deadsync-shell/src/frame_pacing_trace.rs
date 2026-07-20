use deadlib_render::{ClockDomainTrace, DrawStats, PresentModeTrace};
use std::time::{Duration, Instant};

const LOG_INTERVAL: Duration = Duration::from_secs(5);
const REDRAW_DELIVERY_SLOW_US: u32 = 1_000;
const REDRAW_DELIVERY_BAD_US: u32 = 2_000;
const PRESENT_SLOW_US: u32 = 1_000;
const PRESENT_SPIKE_US: u32 = 3_000;

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
}

impl GameplayPacingTrace {
    pub fn new(now: Instant) -> Self {
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
        }
    }

    #[inline(always)]
    pub fn reset(&mut self, now: Instant) {
        *self = Self::new(now);
    }

    #[allow(clippy::too_many_arguments)]
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
            display_error_seconds,
            display_catching_up,
        );
    }

    #[allow(clippy::too_many_arguments)]
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
        display_error_seconds: f32,
        display_catching_up: bool,
    ) {
        if !enabled {
            if self.frames != 0 {
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
        log::trace!(
            "Gameplay frame pacing: frames={} req=[chain:{} other:{}] dt_ms=[avg:{:.3} max:{:.3}] redraw_ms=[late_avg:{:.3} late_max:{:.3} deliver_avg:{:.3} deliver_max:{:.3} >=1ms:{} >=2ms:{}] draw_ms=[avg:{:.3} max:{:.3}] present_ms=[avg:{:.3} max:{:.3} >=1ms:{} >=3ms:{}] draw_cpu_ms=[setup_avg:{:.3} prep_avg:{:.3} record_avg:{:.3}] display_dbg=[err_last_ms:{:+.3} abs_avg_ms:{:.3} abs_max_ms:{:.3} catch:{} catch_last:{}] present_dbg=[mode:{} display:{} host:{} mapped:{} inflight_avg:{:.2} inflight_max:{} image_wait:{} back_pressure:{} queue_idle:{} subopt:{} interval_ms_avg:{:.3} interval_ms_max:{:.3} margin_ms_avg:{:.3} margin_ms_max:{:.3} cal_ms_avg:{:.3} cal_ms_max:{:.3}] audio_dbg=[path:{} req:{} fallback:{} clock:{} qual:{} sf:{} cf:{} rate:{} buf:{} pad:{} q:{} tick_ms:{:.3} span_ms:{:.3} out_ms:{:.3} underruns:{}]",
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
        self.reset(now);
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
            true, now, true, 0.016, 2_000, 500, "chain", 2_500, draw, -0.002, true,
        );
        assert_eq!(trace.frames, 1);
        assert_eq!(trace.chain_frames, 1);
        assert_eq!(trace.dt_sum_us, 16_000);
        assert_eq!(trace.redraw_late_sum_us, 1_500);
        assert_eq!(trace.present_over_1ms, 1);
        assert_eq!(trace.display_error_last_us, -2_000);
        assert_eq!(trace.display_catching_up_frames, 1);
    }

    #[test]
    fn disabled_trace_stays_idle() {
        let now = Instant::now();
        let mut trace = GameplayPacingTrace::new(now);

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
            -0.002,
            true,
        );

        assert_eq!(trace.frames, 0);
    }
}
