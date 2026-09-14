//! Stutter dump behavior and benchmarks against 0.5.1214.
#[allow(dead_code)]
#[path = "../../../tests/support/perf.rs"]
mod perf;
use std::hint::black_box;

use deadlib_audio_core::{OutputTimingQuality, StutterDiagAudioEvent, StutterDiagAudioEventKind};
use deadlib_render_core::{ClockDomainTrace, PresentModeTrace};
use deadsync_gameplay::{DisplayClockDiagEvent, DisplayClockDiagEventKind, DisplayClockStepEvent};
use deadsync_theme_simply_love::screens::SimplyLoveScreen as Screen;
#[allow(dead_code, unused_imports)]
#[path = "loading_diagnostics/stutter_baseline.rs"]
mod baseline;
#[allow(dead_code)]
mod current {
    include!(concat!(env!("CARGO_MANIFEST_DIR"), "/src/stutter_diag.rs"));
    pub(super) fn empty_frame() -> StutterDiagFrameSample {
        StutterDiagFrameSample::empty()
    }
}
use current::{StutterDiagDumpContext, StutterDiagFrameSample};
fn context() -> StutterDiagDumpContext {
    StutterDiagDumpContext {
        now_host_nanos: 500_000_000,
        total_elapsed: 12.5,
        screen: Screen::Gameplay,
        stutter_severity: 3,
        audio_triggered: true,
        display_triggered: true,
    }
}
fn fixtures(
    n: usize,
    events: usize,
) -> (
    Vec<StutterDiagFrameSample>,
    Vec<DisplayClockDiagEvent>,
    Vec<StutterDiagAudioEvent>,
) {
    let frames = (0..n)
        .map(|i| {
            let mut frame = current::empty_frame();
            let v = if i % 13 == 0 {
                u32::MAX
            } else {
                (i as u32) * 17
            };
            frame.host_nanos = if i % 2 == 0 {
                i as u64 * 1_000_000
            } else {
                u64::MAX
            };
            frame.screen = if i % 2 == 0 {
                Screen::Gameplay
            } else {
                Screen::Init
            };
            frame.redraw_request_reason = if i % 2 == 0 { "chain" } else { "timer" };
            frame.frame_us = v.wrapping_add(0);
            frame.expected_us = v.wrapping_add(1);
            frame.pre_redraw_gap_us = v.wrapping_add(2);
            frame.request_to_redraw_us = v.wrapping_add(3);
            frame.maintenance_us = v.wrapping_add(4);
            frame.input_us = v.wrapping_add(5);
            frame.update_us = v.wrapping_add(6);
            frame.compose_us = v.wrapping_add(7);
            frame.upload_us = v.wrapping_add(8);
            frame.draw_us = v.wrapping_add(9);
            frame.acquire_us = v.wrapping_add(10);
            frame.submit_us = v.wrapping_add(11);
            frame.present_us = v.wrapping_add(12);
            frame.gpu_wait_us = v.wrapping_add(13);
            frame.draw_setup_us = v.wrapping_add(14);
            frame.draw_prepare_us = v.wrapping_add(15);
            frame.draw_record_us = v.wrapping_add(16);
            frame.expected_us = if i % 3 == 0 { 0 } else { 8_333 };
            frame.display_error_us = if i % 2 == 0 { i32::MIN } else { i32::MAX };
            frame.display_catching_up = i % 2 == 0;
            frame.present_mode = PresentModeTrace::Fifo;
            frame.present_display_clock = ClockDomainTrace::Device;
            frame.present_host_clock = ClockDomainTrace::Qpc;
            frame.in_flight_images = (i % 256) as u8;
            frame.waited_for_image = i % 2 == 0;
            frame.applied_back_pressure = i % 3 == 0;
            frame.queue_idle_waited = i % 4 == 0;
            frame.suboptimal = i % 5 == 0;
            frame
        })
        .collect();
    let display = (0..events)
        .map(|i| {
            DisplayClockDiagEvent::from_step_event(
                i as u64 * 1_000_000,
                DisplayClockStepEvent {
                    kind: DisplayClockDiagEventKind::ClampStep,
                    target_time_sec: i as f32,
                    previous_time_sec: -0.0,
                    current_time_sec: -0.25,
                    error_seconds: -0.001,
                    step_seconds: 0.125,
                    limit_seconds: 1.0 / 60.0,
                },
            )
        })
        .collect();
    let audio = (0..events)
        .map(|i| StutterDiagAudioEvent {
            at_host_nanos: i as u64 * 1_000_000,
            kind: StutterDiagAudioEventKind::CallbackGap,
            value_ns: 5_000_000,
            sample_rate_hz: 48_000,
            buffer_frames: 512,
            padding_frames: 128,
            queued_frames: 256,
            device_period_ns: 1_000_000,
            estimated_output_delay_ns: 4_000_000,
            timing_quality: OutputTimingQuality::Trusted,
        })
        .collect();
    (frames, display, audio)
}

#[test]
fn streamed_and_owned_dumps_match_every_byte_and_line_order() {
    for (n, events) in [(0, 0), (1, 0), (0, 3), (128, 32)] {
        let (mut frames, mut display, mut audio) = fixtures(n, events);
        if let Some(frame) = frames.last_mut() {
            frame.redraw_request_reason = concat!(
                "long reason ",
                "abcdefghijklmnopqrstuvwxyzabcdefghijklmnopqrstuvwxyzabcdefghijklmnopqrstuvwxyzabcdefghijklmnopqrstuvwxyzabcdefghijklmnopqrstuvwxyzabcdefghijklmnopqrstuvwxyzabcdefghijklmnopqrstuvwxyzabcdefghijklmnopqrstuvwxyzabcdefghijklmnopqrstuvwxyzabcdefghijklmnopqrstuvwxyzabcdefghijklmnopqrstuvwxyzabcdefghijklmnopqrstuvwxyzabcdefghijklmnopqrstuvwxyzabcdefghijklmnopqrstuvwxyzabcdefghijklmnopqrstuvwxyzabcdefghijklmnopqrstuvwxyzabcdefghijklmnopqrstuvwxyzabcdefghijklmnopqrstuvwxyzabcdefghijklmnopqrstuvwxyzabcdefghijklmnopqrstuvwxyzabcdefghijklmnopqrstuvwxyzabcdefghijklmnopqrstuvwxyzabcdefghijklmnopqrstuvwxyzabcdefghijklmnopqrstuvwxyzabcdefghijklmnopqrstuvwxyzabcdefghijklmnopqrstuvwxyz"
            );
        }
        if let Some(event) = display.last_mut() {
            event.target_time_sec = f32::NAN;
            event.current_time_sec = f32::INFINITY;
            event.error_seconds = f32::NEG_INFINITY;
        }
        if let Some(event) = audio.last_mut() {
            event.value_ns = u64::MAX;
            event.at_host_nanos = u64::MAX;
        }
        for elapsed in [0.0, -0.0, 12.5, f32::NAN, f32::INFINITY] {
            let mut ctx = context();
            ctx.total_elapsed = elapsed;
            let expected = baseline::stutter_diag_dump_lines(ctx, &frames, &display, &audio);
            let mut actual = Vec::new();
            current::for_each_stutter_diag_line(ctx, &frames, &display, &audio, |line| {
                actual.push(line.to_owned())
            });
            assert_eq!(actual, expected);
            assert_eq!(
                current::stutter_diag_dump_lines(ctx, &frames, &display, &audio),
                expected
            );
        }
    }
}

#[test]
fn stream_uses_one_buffer_instead_of_allocating_per_record() {
    perf::assert_churn_budget(1, 256, || {
        current::for_each_stutter_diag_line(context(), &[], &[], &[], |s| {
            black_box(s);
        });
    });
    let (frames, display, audio) = fixtures(128, 32);
    perf::assert_churn_budget(1, 768, || {
        current::for_each_stutter_diag_line(context(), &frames, &display, &audio, |s| {
            black_box(s);
        })
    });
    perf::assert_reduced_churn(
        || {
            black_box(baseline::stutter_diag_dump_lines(
                context(),
                &frames,
                &display,
                &audio,
            ));
        },
        || {
            current::for_each_stutter_diag_line(context(), &frames, &display, &audio, |s| {
                black_box(s);
            })
        },
    );
}

#[test]
#[ignore = "manual release benchmark; run serially with --nocapture"]
fn benchmark_loading_diagnostics() {
    for (name, n, events) in [
        ("empty", 0, 0),
        ("single-frame", 1, 0),
        ("128-frames", 128, 0),
        ("mixed", 128, 32),
    ] {
        let (frames, display, audio) = fixtures(n, events);
        let versions = if std::env::var_os("DEADSYNC_PERF_REVERSE").is_some() {
            [false, true]
        } else {
            [true, false]
        };
        for old in versions {
            perf::measure_sampled(
                &format!("stutter/{name}/{}", if old { "old" } else { "new" }),
                if n < 2 { 20000 } else { 300 },
                1 + n + 2 * events,
                || {
                    let ctx = black_box(context());
                    let frames = black_box(frames.as_slice());
                    let display = black_box(display.as_slice());
                    let audio = black_box(audio.as_slice());
                    let mut sum = 0;
                    if old {
                        for line in baseline::stutter_diag_dump_lines(ctx, frames, display, audio) {
                            sum += black_box(line.as_str()).len();
                        }
                    } else {
                        current::for_each_stutter_diag_line(ctx, frames, display, audio, |line| {
                            sum += black_box(line).len()
                        });
                    }
                    sum
                },
            );
        }
    }
}
