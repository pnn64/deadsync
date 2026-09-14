//! Timing text behavior and benchmarks against 0.5.1214.
use deadlib_render_core::{ClockDomainTrace, PresentModeTrace};
use deadsync_theme::views::AudioTimingView;
use deadsync_theme_simply_love::views::TimingHealth;
use std::hint::black_box;
use std::sync::Arc;
#[allow(dead_code)]
#[path = "../../../tests/support/perf.rs"]
mod perf;
mod act_macro {
    macro_rules! act {
        (quad: $($tail:tt)+) => {{
            ::deadlib_present::__act_from_builder!(
                ($($tail)+) ::deadsync_assets::present_dsl::SpriteBuilder::solid()
            )
        }};
        (text: $($tail:tt)+) => {{
            ::deadlib_present::__act_from_builder!(
                ($($tail)+) ::deadsync_assets::present_dsl::TextBuilder::new()
            )
        }};
    }
    pub(crate) use act;
}
pub(crate) use act_macro::act;
mod views {
    pub use deadsync_theme_simply_love::views::TimingHealth;
}

#[allow(dead_code, unused_imports)]
#[path = "loading_diagnostics/timing_baseline.rs"]
mod baseline;
#[allow(dead_code)]
mod current {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/src/screens/components/shared/stats_overlay.rs"
    ));
    pub(super) fn readout(timing: TimingHealth) -> Arc<str> {
        retained_timing_text(timing)
    }
    pub(super) fn reset() {
        TIMING_TEXT_CACHE.with(|c| *c.borrow_mut() = TimingTextCache::new());
    }
    pub(super) fn storage() -> (usize, usize) {
        TIMING_TEXT_CACHE.with(|c| {
            let c = c.borrow();
            (c.entries.len(), c.scratch.capacity())
        })
    }
    pub(super) fn payload_bytes() -> usize {
        TIMING_TEXT_CACHE.with(|c| c.borrow().entries.values().map(|s| s.len()).sum())
    }
    pub(super) fn metadata_bytes() -> usize {
        std::mem::size_of::<TimingTextCache>()
    }
}
fn timing(audio: bool) -> TimingHealth {
    TimingHealth {
        interval_ns: 16_666_667,
        display_error_ms: -0.42,
        display_catching_up: true,
        present_mode: PresentModeTrace::Fifo,
        display_clock: ClockDomainTrace::Device,
        host_clock: ClockDomainTrace::Monotonic,
        in_flight_images: 2,
        waited_for_image: true,
        applied_back_pressure: false,
        queue_idle_waited: false,
        suboptimal: false,
        submitted_present_id: 12_345,
        completed_present_id: 12_344,
        calibration_error_ns: 83_000,
        host_mapped: true,
        audio: audio.then_some(AudioTimingView {
            backend: "WASAPI",
            requested_output_mode: "exclusive",
            fallback_from_native: false,
            timing_clock: "device",
            timing_quality: "precise",
            sample_rate_hz: 48_000,
            device_period_ns: 2_666_667,
            stream_latency_ns: 5_333_334,
            buffer_frames: 256,
            padding_frames: 128,
            queued_frames: 384,
            estimated_output_delay_ns: 8_000_000,
            clock_fallback_count: 1,
            timing_sanity_failure_count: 2,
            underrun_count: 3,
        }),
    }
}

#[test]
fn every_timing_field_refreshes_the_exact_visible_text() {
    current::reset();
    for audio in [false, true] {
        let t = timing(audio);
        let mut cases = vec![t];
        let mut v = t;
        v.interval_ns = v.interval_ns.wrapping_add(1);
        cases.push(v);
        let mut v = t;
        v.display_catching_up = !v.display_catching_up;
        cases.push(v);
        let mut v = t;
        v.present_mode = PresentModeTrace::Immediate;
        cases.push(v);
        let mut v = t;
        v.display_clock = ClockDomainTrace::MonotonicRaw;
        cases.push(v);
        let mut v = t;
        v.host_clock = ClockDomainTrace::MonotonicRaw;
        cases.push(v);
        let mut v = t;
        v.in_flight_images = v.in_flight_images.wrapping_add(1);
        cases.push(v);
        let mut v = t;
        v.waited_for_image = !v.waited_for_image;
        cases.push(v);
        let mut v = t;
        v.applied_back_pressure = !v.applied_back_pressure;
        cases.push(v);
        let mut v = t;
        v.queue_idle_waited = !v.queue_idle_waited;
        cases.push(v);
        let mut v = t;
        v.suboptimal = !v.suboptimal;
        cases.push(v);
        let mut v = t;
        v.submitted_present_id = v.submitted_present_id.wrapping_add(1);
        cases.push(v);
        let mut v = t;
        v.completed_present_id = v.completed_present_id.wrapping_add(1);
        cases.push(v);
        let mut v = t;
        v.calibration_error_ns = v.calibration_error_ns.wrapping_add(1);
        cases.push(v);
        let mut v = t;
        v.host_mapped = !v.host_mapped;
        cases.push(v);
        let mut v = t;
        v.audio = None;
        cases.push(v);
        for value in [
            0.0,
            -0.0,
            0.005,
            -0.005,
            f32::NAN,
            f32::INFINITY,
            f32::NEG_INFINITY,
            f32::MAX,
            f32::MIN_POSITIVE,
        ] {
            let mut v = t;
            v.display_error_ms = value;
            cases.push(v);
        }
        for value in [0, 1, u64::MAX] {
            let mut v = t;
            v.interval_ns = value;
            v.calibration_error_ns = value;
            cases.push(v);
        }
        if audio {
            let mut v = t;
            let a = v.audio.as_mut().unwrap();
            a.backend = "changed field";
            cases.push(v);
            let mut v = t;
            let a = v.audio.as_mut().unwrap();
            a.requested_output_mode = "changed field";
            cases.push(v);
            let mut v = t;
            let a = v.audio.as_mut().unwrap();
            a.fallback_from_native = !a.fallback_from_native;
            cases.push(v);
            let mut v = t;
            let a = v.audio.as_mut().unwrap();
            a.timing_clock = "changed field";
            cases.push(v);
            let mut v = t;
            let a = v.audio.as_mut().unwrap();
            a.timing_quality = "changed field";
            cases.push(v);
            let mut v = t;
            let a = v.audio.as_mut().unwrap();
            a.sample_rate_hz = a.sample_rate_hz.wrapping_add(1);
            cases.push(v);
            let mut v = t;
            let a = v.audio.as_mut().unwrap();
            a.device_period_ns = a.device_period_ns.wrapping_add(1);
            cases.push(v);
            let mut v = t;
            let a = v.audio.as_mut().unwrap();
            a.stream_latency_ns = a.stream_latency_ns.wrapping_add(1);
            cases.push(v);
            let mut v = t;
            let a = v.audio.as_mut().unwrap();
            a.buffer_frames = a.buffer_frames.wrapping_add(1);
            cases.push(v);
            let mut v = t;
            let a = v.audio.as_mut().unwrap();
            a.padding_frames = a.padding_frames.wrapping_add(1);
            cases.push(v);
            let mut v = t;
            let a = v.audio.as_mut().unwrap();
            a.queued_frames = a.queued_frames.wrapping_add(1);
            cases.push(v);
            let mut v = t;
            let a = v.audio.as_mut().unwrap();
            a.estimated_output_delay_ns = a.estimated_output_delay_ns.wrapping_add(1);
            cases.push(v);
            let mut v = t;
            let a = v.audio.as_mut().unwrap();
            a.clock_fallback_count = a.clock_fallback_count.wrapping_add(1);
            cases.push(v);
            let mut v = t;
            let a = v.audio.as_mut().unwrap();
            a.timing_sanity_failure_count = a.timing_sanity_failure_count.wrapping_add(1);
            cases.push(v);
            let mut v = t;
            let a = v.audio.as_mut().unwrap();
            a.underrun_count = a.underrun_count.wrapping_add(1);
            cases.push(v);
        }
        for v in cases {
            assert_eq!(current::readout(v).as_ref(), baseline::owned(v));
        }
    }
}

#[test]
fn random_float_bits_and_large_audio_fields_match_old_formatting() {
    let mut seed = 914u64;
    for _ in 0..4096 {
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
        let mut t = timing(true);
        t.display_error_ms = f32::from_bits((seed >> 32) as u32);
        t.interval_ns = seed;
        t.calibration_error_ns = seed.rotate_left(9);
        t.submitted_present_id = seed as u32;
        t.completed_present_id = (seed >> 32) as u32;
        let a = t.audio.as_mut().unwrap();
        a.device_period_ns = seed;
        a.stream_latency_ns = seed;
        a.estimated_output_delay_ns = seed;
        a.clock_fallback_count = seed;
        a.timing_sanity_failure_count = seed;
        a.underrun_count = seed;
        assert_eq!(current::readout(t).as_ref(), baseline::owned(t));
    }
}

fn prime(old: bool) {
    if old {
        baseline::reset();
    } else {
        current::reset();
    }
    for i in 0u32..4096 {
        let mut t = timing(true);
        t.submitted_present_id = i;
        t.completed_present_id = i.saturating_sub(1);
        if old {
            black_box(baseline::readout(t));
        } else {
            black_box(current::readout(t));
        }
    }
}

#[test]
fn cache_saturation_and_external_arc_owners_preserve_text() {
    prime(false);
    let first = current::readout(timing(true));
    let expected = first.to_string();
    for i in 0..256 {
        let mut t = timing(i % 2 == 0);
        t.submitted_present_id = 80_000 + i;
        assert_eq!(current::readout(t).as_ref(), baseline::owned(t));
    }
    assert_eq!(first.as_ref(), expected);
    let last = current::readout(timing(true));
    assert!(Arc::ptr_eq(&last, &current::readout(timing(true))));
    assert_eq!(current::storage().0, 4096);
    perf::assert_no_churn(|| {
        black_box(current::readout(timing(true)));
    });
}

#[test]
fn changing_ids_use_one_allocation_and_subprecision_jitter_uses_none() {
    prime(false);
    let mut t = timing(true);
    t.submitted_present_id = 90_000;
    black_box(current::readout(t));
    t.submitted_present_id += 1;
    perf::assert_churn_budget(1, 512, || {
        black_box(current::readout(t));
    });
    t.display_error_ms = f32::from_bits(t.display_error_ms.to_bits() + 1);
    perf::assert_no_churn(|| {
        black_box(current::readout(t));
    });
}

#[test]
fn equal_text_does_not_fill_cache_with_subprecision_keys() {
    baseline::reset();
    current::reset();
    for i in 0..4096 {
        let mut t = timing(true);
        t.display_error_ms = f32::from_bits(t.display_error_ms.to_bits() + i);
        assert_eq!(baseline::readout(t), current::readout(t));
    }
    assert_eq!(baseline::storage().0, 4096);
    assert_eq!(current::storage().0, 1);
    eprintln!(
        "jitter retention: old {:?}, new ({}, {}); scratch {}; cache metadata old {} new {}",
        baseline::storage(),
        current::storage().0,
        current::payload_bytes(),
        current::storage().1,
        baseline::metadata_bytes(),
        current::metadata_bytes()
    );
}

#[test]
#[ignore = "manual release benchmark; run serially with --nocapture"]
fn benchmark_loading_diagnostics() {
    for name in [
        "stable",
        "alternating",
        "64-cached",
        "changing-ids",
        "changing-no-audio",
        "jitter",
        "saturated-stable",
    ] {
        let versions = if std::env::var_os("DEADSYNC_PERF_REVERSE").is_some() {
            [false, true]
        } else {
            [true, false]
        };
        for old in versions {
            prime(old);
            let mut tick = 0u32;
            perf::measure_sampled(
                &format!("timing/{name}/{}", if old { "old" } else { "new" }),
                20000,
                1,
                || {
                    tick = tick.wrapping_add(1);
                    let mut t = timing(name != "changing-no-audio");
                    match name {
                        "stable" => {
                            t.submitted_present_id = 4095;
                            t.completed_present_id = 4094;
                        }
                        "alternating" => {
                            t.submitted_present_id = 4094 + tick % 2;
                            t.completed_present_id = t.submitted_present_id - 1;
                        }
                        "64-cached" => {
                            t.submitted_present_id = 4000 + tick % 64;
                            t.completed_present_id = t.submitted_present_id - 1;
                        }
                        "changing-ids" | "changing-no-audio" => {
                            t.submitted_present_id = 10_000 + tick;
                            t.completed_present_id = t.submitted_present_id - 1;
                        }
                        "jitter" => {
                            t.display_error_ms =
                                f32::from_bits(t.display_error_ms.to_bits() + tick % 2048);
                        }
                        _ => {}
                    }
                    if old {
                        black_box(baseline::readout(black_box(t)));
                    } else {
                        black_box(current::readout(black_box(t)));
                    }
                },
            );
        }
    }
}
