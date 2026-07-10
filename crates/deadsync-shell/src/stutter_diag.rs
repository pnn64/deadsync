use deadlib_render::{ClockDomainTrace, DrawStats, PresentModeTrace};
use deadsync_audio::StutterDiagAudioEvent;
use deadsync_config::frame_pacing::{FixedFrameStatsRing, seconds_to_us_u32};
use deadsync_gameplay::DisplayClockDiagEvent;
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

#[derive(Clone, Copy, Debug)]
pub struct StutterDiagDumpContext {
    pub now_host_nanos: u64,
    pub total_elapsed: f32,
    pub screen: Screen,
    pub stutter_severity: u8,
    pub audio_triggered: bool,
    pub display_triggered: bool,
}

pub fn stutter_diag_dump_lines(
    context: StutterDiagDumpContext,
    frames: &[StutterDiagFrameSample],
    display_events: &[DisplayClockDiagEvent],
    audio_events: &[StutterDiagAudioEvent],
) -> Vec<String> {
    let mut lines = Vec::with_capacity(
        1usize
            .saturating_add(frames.len())
            .saturating_add(display_events.len())
            .saturating_add(audio_events.len()),
    );
    lines.push(format!(
        "Stutter recorder dump t={:.3}s screen={:?} reason=[stutter:{} audio:{} display:{}] window_ms={:.1} frames={} audio_events={} display_events={}",
        context.total_elapsed,
        context.screen,
        context.stutter_severity,
        u8::from(context.audio_triggered),
        u8::from(context.display_triggered),
        STUTTER_DIAG_WINDOW_NS as f64 / 1_000_000.0,
        frames.len(),
        audio_events.len(),
        display_events.len(),
    ));
    for sample in frames {
        let age_ms = context.now_host_nanos.saturating_sub(sample.host_nanos) as f64 / 1_000_000.0;
        let multiple = if sample.expected_us > 0 {
            sample.frame_us as f64 / sample.expected_us as f64
        } else {
            0.0
        };
        lines.push(format!(
            "Stutter recorder frame age_ms={:.3} screen={:?} dt_ms={:.3} expected_ms={:.3} x{:.2} req={} phases_ms=[pre:{:.3} rq:{:.3} in:{:.3} up:{:.3} comp:{:.3} upload:{:.3} draw:{:.3}] draw_ms=[acq:{:.3} sub:{:.3} present:{:.3} gpu_wait:{:.3} setup:{:.3} prep:{:.3} record:{:.3}] display=[err_ms:{:+.3} catch:{}] present=[mode:{} display:{} host:{} inflight:{} wait:{} back:{} idle:{} subopt:{}]",
            age_ms,
            sample.screen,
            sample.frame_us as f64 / 1000.0,
            sample.expected_us as f64 / 1000.0,
            multiple,
            sample.redraw_request_reason,
            sample.pre_redraw_gap_us as f64 / 1000.0,
            sample.request_to_redraw_us as f64 / 1000.0,
            sample.input_us as f64 / 1000.0,
            sample.update_us as f64 / 1000.0,
            sample.compose_us as f64 / 1000.0,
            sample.upload_us as f64 / 1000.0,
            sample.draw_us as f64 / 1000.0,
            sample.acquire_us as f64 / 1000.0,
            sample.submit_us as f64 / 1000.0,
            sample.present_us as f64 / 1000.0,
            sample.gpu_wait_us as f64 / 1000.0,
            sample.draw_setup_us as f64 / 1000.0,
            sample.draw_prepare_us as f64 / 1000.0,
            sample.draw_record_us as f64 / 1000.0,
            sample.display_error_us as f64 / 1000.0,
            u8::from(sample.display_catching_up),
            sample.present_mode,
            sample.present_display_clock,
            sample.present_host_clock,
            sample.in_flight_images,
            u8::from(sample.waited_for_image),
            u8::from(sample.applied_back_pressure),
            u8::from(sample.queue_idle_waited),
            u8::from(sample.suboptimal),
        ));
    }
    for event in display_events {
        let age_ms =
            context.now_host_nanos.saturating_sub(event.at_host_nanos) as f64 / 1_000_000.0;
        lines.push(format!(
            "Stutter recorder display age_ms={:.3} kind={} target_ms={:.3} prev_ms={:.3} curr_ms={:.3} err_ms={:+.3} step_ms={:+.3} limit_ms={:.3}",
            age_ms,
            event.kind,
            event.target_time_sec as f64 * 1000.0,
            event.previous_time_sec as f64 * 1000.0,
            event.current_time_sec as f64 * 1000.0,
            event.error_seconds as f64 * 1000.0,
            event.step_seconds as f64 * 1000.0,
            event.limit_seconds as f64 * 1000.0,
        ));
    }
    for event in audio_events {
        let age_ms =
            context.now_host_nanos.saturating_sub(event.at_host_nanos) as f64 / 1_000_000.0;
        lines.push(format!(
            "Stutter recorder audio age_ms={:.3} kind={} value_ms={:.3} rate={} buf={} pad={} q={} period_ms={:.3} out_ms={:.3} qual={}",
            age_ms,
            event.kind,
            event.value_ns as f64 / 1_000_000.0,
            event.sample_rate_hz,
            event.buffer_frames,
            event.padding_frames,
            event.queued_frames,
            event.device_period_ns as f64 / 1_000_000.0,
            event.estimated_output_delay_ns as f64 / 1_000_000.0,
            event.timing_quality,
        ));
    }
    lines
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
    use deadsync_audio::{OutputTimingQuality, StutterDiagAudioEventKind};
    use deadsync_gameplay::{DisplayClockDiagEventKind, DisplayClockStepEvent};

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

    #[test]
    fn dump_lines_preserve_frame_display_and_audio_diagnostics() {
        let now = STUTTER_DIAG_WINDOW_NS;
        let mut recorder = StutterDiagRecorder::new();
        recorder.record_frame(
            now - 2_000_000,
            Screen::Gameplay,
            0.024,
            0.008,
            100,
            200,
            "chain",
            300,
            400,
            500,
            600,
            700,
            DrawStats::default(),
            -0.0015,
            true,
        );
        let mut frames = Vec::new();
        recorder.collect_recent(now, &mut frames);
        let display = [DisplayClockDiagEvent::from_step_event(
            now - 3_000_000,
            DisplayClockStepEvent {
                kind: DisplayClockDiagEventKind::ClampStep,
                target_time_sec: 1.0,
                previous_time_sec: 0.98,
                current_time_sec: 0.99,
                error_seconds: 0.01,
                step_seconds: 0.01,
                limit_seconds: 1.0 / 60.0,
            },
        )];
        let audio = [StutterDiagAudioEvent {
            at_host_nanos: now - 4_000_000,
            kind: StutterDiagAudioEventKind::CallbackGap,
            value_ns: 5_000_000,
            sample_rate_hz: 48_000,
            buffer_frames: 512,
            padding_frames: 128,
            queued_frames: 256,
            device_period_ns: 1_000_000,
            estimated_output_delay_ns: 4_000_000,
            timing_quality: OutputTimingQuality::Trusted,
        }];

        let lines = stutter_diag_dump_lines(
            StutterDiagDumpContext {
                now_host_nanos: now,
                total_elapsed: 12.5,
                screen: Screen::Gameplay,
                stutter_severity: 3,
                audio_triggered: true,
                display_triggered: true,
            },
            &frames,
            &display,
            &audio,
        );

        assert_eq!(lines.len(), 4);
        assert!(lines[0].contains("t=12.500s screen=Gameplay"));
        assert!(lines[0].contains("frames=1 audio_events=1 display_events=1"));
        assert!(lines[1].contains("age_ms=2.000"));
        assert!(lines[1].contains("dt_ms=24.000 expected_ms=8.000 x3.00"));
        assert!(lines[1].contains("display=[err_ms:-1.500 catch:1]"));
        assert!(lines[2].contains("kind=clamp_step"));
        assert!(lines[2].contains("target_ms=1000.000"));
        assert!(lines[3].contains("kind=callback_gap value_ms=5.000 rate=48000"));
        assert!(lines[3].contains("period_ms=1.000 out_ms=4.000 qual=trusted"));
    }
}
