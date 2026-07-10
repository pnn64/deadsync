use deadlib_render::{ClockDomainTrace, DrawStats, PresentModeTrace};
use deadsync_config::frame_pacing::{FixedFrameStatsRing, seconds_to_us_u32};
use deadsync_screens::Screen;

pub const STUTTER_DIAG_WINDOW_NS: u64 = 500_000_000;
pub const STUTTER_DIAG_FRAME_CAPACITY: usize = 128;
const MIN_DUMP_GAP_NS: u64 = 250_000_000;

#[derive(Clone, Copy, Debug)]
pub struct StutterDiagFrameSample {
    pub host_nanos: u64,
    pub screen: Screen,
    pub redraw_request_reason: &'static str,
    pub frame_us: u32,
    pub expected_us: u32,
    pub pre_redraw_gap_us: u32,
    pub request_to_redraw_us: u32,
    pub input_us: u32,
    pub update_us: u32,
    pub compose_us: u32,
    pub upload_us: u32,
    pub draw_us: u32,
    pub acquire_us: u32,
    pub submit_us: u32,
    pub present_us: u32,
    pub gpu_wait_us: u32,
    pub draw_setup_us: u32,
    pub draw_prepare_us: u32,
    pub draw_record_us: u32,
    pub display_error_us: i32,
    pub display_catching_up: bool,
    pub present_mode: PresentModeTrace,
    pub present_display_clock: ClockDomainTrace,
    pub present_host_clock: ClockDomainTrace,
    pub in_flight_images: u8,
    pub waited_for_image: bool,
    pub applied_back_pressure: bool,
    pub queue_idle_waited: bool,
    pub suboptimal: bool,
}

impl StutterDiagFrameSample {
    const fn empty() -> Self {
        Self {
            host_nanos: 0,
            screen: Screen::Init,
            redraw_request_reason: "none",
            frame_us: 0,
            expected_us: 0,
            pre_redraw_gap_us: 0,
            request_to_redraw_us: 0,
            input_us: 0,
            update_us: 0,
            compose_us: 0,
            upload_us: 0,
            draw_us: 0,
            acquire_us: 0,
            submit_us: 0,
            present_us: 0,
            gpu_wait_us: 0,
            draw_setup_us: 0,
            draw_prepare_us: 0,
            draw_record_us: 0,
            display_error_us: 0,
            display_catching_up: false,
            present_mode: PresentModeTrace::Unknown,
            present_display_clock: ClockDomainTrace::Unknown,
            present_host_clock: ClockDomainTrace::Unknown,
            in_flight_images: 0,
            waited_for_image: false,
            applied_back_pressure: false,
            queue_idle_waited: false,
            suboptimal: false,
        }
    }
}

pub struct StutterDiagRecorder {
    frames: FixedFrameStatsRing<StutterDiagFrameSample, STUTTER_DIAG_FRAME_CAPACITY>,
    last_audio_trigger_seq: u64,
    last_display_trigger_seq: u64,
    last_dump_host_nanos: u64,
}

impl StutterDiagRecorder {
    pub const fn new() -> Self {
        Self {
            frames: FixedFrameStatsRing::new(StutterDiagFrameSample::empty()),
            last_audio_trigger_seq: 0,
            last_display_trigger_seq: 0,
            last_dump_host_nanos: 0,
        }
    }

    #[inline(always)]
    pub fn reset_frame_clock(&mut self) {
        self.frames.clear();
        self.last_dump_host_nanos = 0;
    }

    #[allow(clippy::too_many_arguments)]
    pub fn record_frame(
        &mut self,
        host_nanos: u64,
        screen: Screen,
        frame_seconds: f32,
        expected_seconds: f32,
        pre_redraw_gap_us: u32,
        request_to_redraw_us: u32,
        redraw_request_reason: &'static str,
        input_us: u32,
        update_us: u32,
        compose_us: u32,
        upload_us: u32,
        draw_us: u32,
        draw_stats: DrawStats,
        display_error_seconds: f32,
        display_catching_up: bool,
    ) {
        let display_error_us_i64 = (f64::from(display_error_seconds) * 1_000_000.0).round() as i64;
        let display_error_us =
            display_error_us_i64.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32;
        let present = draw_stats.present_stats;
        self.frames.push(StutterDiagFrameSample {
            host_nanos,
            screen,
            redraw_request_reason,
            frame_us: seconds_to_us_u32(frame_seconds),
            expected_us: seconds_to_us_u32(expected_seconds),
            pre_redraw_gap_us,
            request_to_redraw_us,
            input_us,
            update_us,
            compose_us,
            upload_us,
            draw_us,
            acquire_us: draw_stats.acquire_us,
            submit_us: draw_stats.submit_us,
            present_us: draw_stats.present_us,
            gpu_wait_us: draw_stats.gpu_wait_us,
            draw_setup_us: draw_stats.backend_setup_us,
            draw_prepare_us: draw_stats.backend_prepare_us,
            draw_record_us: draw_stats.backend_record_us,
            display_error_us,
            display_catching_up,
            present_mode: present.mode,
            present_display_clock: present.display_clock,
            present_host_clock: present.host_clock,
            in_flight_images: present.in_flight_images,
            waited_for_image: present.waited_for_image,
            applied_back_pressure: present.applied_back_pressure,
            queue_idle_waited: present.queue_idle_waited,
            suboptimal: present.suboptimal,
        });
    }

    pub fn collect_recent(&self, now_host_nanos: u64, out: &mut Vec<StutterDiagFrameSample>) {
        self.frames
            .collect_recent_by(now_host_nanos, STUTTER_DIAG_WINDOW_NS, out, |sample| {
                sample.host_nanos
            });
    }

    pub fn take_dump_trigger(
        &mut self,
        now_host_nanos: u64,
        severity: u8,
        audio_trigger_seq: u64,
        display_trigger_seq: u64,
    ) -> Option<(bool, bool)> {
        if now_host_nanos == 0 {
            return None;
        }
        let audio_triggered = audio_trigger_seq > self.last_audio_trigger_seq;
        let display_triggered = display_trigger_seq > self.last_display_trigger_seq;
        if severity == 0 && !audio_triggered && !display_triggered {
            return None;
        }
        if now_host_nanos.saturating_sub(self.last_dump_host_nanos) < MIN_DUMP_GAP_NS {
            return None;
        }
        self.last_audio_trigger_seq = audio_trigger_seq;
        self.last_display_trigger_seq = display_trigger_seq;
        self.last_dump_host_nanos = now_host_nanos;
        Some((audio_triggered, display_triggered))
    }
}

impl Default for StutterDiagRecorder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recorder_keeps_recent_frames_and_throttles_dumps() {
        let mut recorder = StutterDiagRecorder::new();
        recorder.record_frame(
            STUTTER_DIAG_WINDOW_NS,
            Screen::Gameplay,
            0.016,
            0.008,
            100,
            50,
            "chain",
            10,
            20,
            30,
            40,
            50,
            DrawStats::default(),
            0.001,
            true,
        );
        let mut frames = Vec::new();
        recorder.collect_recent(STUTTER_DIAG_WINDOW_NS, &mut frames);
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0].display_error_us, 1_000);
        assert_eq!(
            recorder.take_dump_trigger(500_000_000, 1, 0, 0),
            Some((false, false))
        );
        assert_eq!(recorder.take_dump_trigger(600_000_000, 1, 0, 0), None);
    }
}
