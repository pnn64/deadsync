// Frozen from 0.5.1214 (012adc4ce) for behavior and performance comparisons.
use deadlib_audio_core::StutterDiagAudioEvent;
use deadlib_render_core::{ClockDomainTrace, DrawStats, PresentModeTrace};
use deadsync_config::frame_pacing::{FixedFrameStatsRing, seconds_to_us_u32};
use deadsync_gameplay::DisplayClockDiagEvent;
use deadsync_theme_simply_love::screens::SimplyLoveScreen as Screen;

pub const STUTTER_DIAG_WINDOW_NS: u64 = 500_000_000;
pub const STUTTER_DIAG_FRAME_CAPACITY: usize = 128;
const MIN_DUMP_GAP_NS: u64 = 250_000_000;

use super::current::{StutterDiagDumpContext, StutterDiagFrameSample};

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
            f64::from(sample.frame_us) / f64::from(sample.expected_us)
        } else {
            0.0
        };
        lines.push(format!(
            "Stutter recorder frame age_ms={:.3} screen={:?} dt_ms={:.3} expected_ms={:.3} x{:.2} req={} phases_ms=[pre:{:.3} rq:{:.3} in:{:.3} maintenance:{:.3} up:{:.3} comp:{:.3} upload:{:.3} draw:{:.3}] draw_ms=[acq:{:.3} sub:{:.3} present:{:.3} gpu_wait:{:.3} setup:{:.3} prep:{:.3} record:{:.3}] display=[err_ms:{:+.3} catch:{}] present=[mode:{} display:{} host:{} inflight:{} wait:{} back:{} idle:{} subopt:{}]",
            age_ms,
            sample.screen,
            f64::from(sample.frame_us) / 1000.0,
            f64::from(sample.expected_us) / 1000.0,
            multiple,
            sample.redraw_request_reason,
            f64::from(sample.pre_redraw_gap_us) / 1000.0,
            f64::from(sample.request_to_redraw_us) / 1000.0,
            f64::from(sample.input_us) / 1000.0,
            f64::from(sample.maintenance_us) / 1000.0,
            f64::from(sample.update_us) / 1000.0,
            f64::from(sample.compose_us) / 1000.0,
            f64::from(sample.upload_us) / 1000.0,
            f64::from(sample.draw_us) / 1000.0,
            f64::from(sample.acquire_us) / 1000.0,
            f64::from(sample.submit_us) / 1000.0,
            f64::from(sample.present_us) / 1000.0,
            f64::from(sample.gpu_wait_us) / 1000.0,
            f64::from(sample.draw_setup_us) / 1000.0,
            f64::from(sample.draw_prepare_us) / 1000.0,
            f64::from(sample.draw_record_us) / 1000.0,
            f64::from(sample.display_error_us) / 1000.0,
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
            f64::from(event.target_time_sec) * 1000.0,
            f64::from(event.previous_time_sec) * 1000.0,
            f64::from(event.current_time_sec) * 1000.0,
            f64::from(event.error_seconds) * 1000.0,
            f64::from(event.step_seconds) * 1000.0,
            f64::from(event.limit_seconds) * 1000.0,
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
