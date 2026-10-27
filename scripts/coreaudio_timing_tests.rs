//! Standalone tests of functions and the callback block extracted from lib.rs.
use super::{
    callback_timing, frames_to_nanos, kernel_bench, nanos_to_frames, perf, scale_mach_time,
};
use std::hint::black_box;

// Frozen from b4ecd1e987608c129d1f7bdd96fd009b33e842e5.
fn original_scale(time: u64, numer: u32, denom: u32) -> u64 {
    ((u128::from(time) * u128::from(numer)) / u128::from(denom)).min(u128::from(u64::MAX)) as u64
}

fn original_frames(rate: u32, nanos: u64) -> u32 {
    ((u128::from(nanos) * u128::from(rate)) / 1_000_000_000).min(u128::from(u32::MAX)) as u32
}

fn original_callback(
    rate: u32,
    buffer: u32,
    frames: usize,
    nanos: u64,
) -> (u32, u64, u64, u32, u32, u32, u64) {
    let period = frames.max(1) as u32;
    let buffered = buffer.max(period);
    let latency_frames = buffered.saturating_add(original_frames(rate, nanos));
    let period_ns = frames_to_nanos(rate, period);
    let latency_ns = frames_to_nanos(rate, buffered).saturating_add(nanos);
    (
        rate,
        period_ns,
        latency_ns,
        buffer.max(period),
        0,
        latency_frames,
        latency_ns,
    )
}

fn random(state: &mut u64) -> u64 {
    *state ^= *state << 13;
    *state ^= *state >> 7;
    *state ^= *state << 17;
    *state
}

#[test]
fn mach_scaling_preserves_rounding_overflow_and_saturation() {
    let times = [
        0,
        1,
        2,
        3,
        24_000_000_000,
        u64::MAX / 125,
        u64::MAX / 125 + 1,
        u64::MAX / 2,
        u64::MAX - 1,
        u64::MAX,
    ];
    for numer in [0, 1, 2, 3, 125, 1_000_000_000, u32::MAX] {
        for denom in [1, 2, 3, 125, 1_000_000_000, u32::MAX] {
            for time in times {
                assert_eq!(
                    scale_mach_time(time, numer, denom),
                    original_scale(time, numer, denom)
                );
            }
        }
    }
    let mut state = 0x8912_753a_981b_ee57;
    for _ in 0..200_000 {
        let time = random(&mut state);
        let numer = random(&mut state) as u32;
        let denom = (random(&mut state) as u32).max(1);
        for value in [time, time / u64::from(numer.max(1))] {
            assert_eq!(
                scale_mach_time(value, numer, denom),
                original_scale(value, numer, denom)
            );
        }
    }
}

#[test]
#[should_panic]
fn zero_timebase_denominator_still_panics() {
    scale_mach_time(1, 1, 0);
}

#[test]
#[should_panic]
fn zero_timebase_numerator_and_denominator_still_panics() {
    scale_mach_time(1, 0, 0);
}

#[test]
fn latency_conversion_preserves_all_integer_boundaries() {
    for rate in [0, 1, 11_025, 44_100, 48_000, 96_000, 192_000, u32::MAX] {
        let divisor = u64::from(rate.max(1));
        let saturated = (u64::from(u32::MAX) + 1) * 1_000_000_000 / divisor;
        for nanos in [
            0,
            1,
            999_999_999,
            1_000_000_000,
            u64::MAX / divisor,
            (u64::MAX / divisor).saturating_add(1),
            saturated.saturating_sub(1),
            saturated,
            saturated + 1,
            u64::MAX,
        ] {
            assert_eq!(nanos_to_frames(rate, nanos), original_frames(rate, nanos));
        }
    }
    let mut state = 0x6243_abc1_6256_de34;
    for _ in 0..200_000 {
        let rate = random(&mut state) as u32;
        let nanos = random(&mut state);
        for value in [nanos, nanos % 10_000_000_000] {
            assert_eq!(nanos_to_frames(rate, value), original_frames(rate, value));
        }
    }
}

#[test]
fn callback_telemetry_matches_for_variable_periods_and_latency_changes() {
    for rate in [0, 1, 44_100, 48_000, 96_000, u32::MAX] {
        for buffer in [1, 64, 128, 512, u32::MAX] {
            for frames in [0, 1, 64, 128, 256, 1024, u32::MAX as usize, usize::MAX] {
                for nanos in [0, 1, 160_000_000, u64::MAX - 1, u64::MAX] {
                    assert_eq!(
                        callback_timing(rate, buffer, frames_to_nanos(rate, buffer), frames, nanos),
                        original_callback(rate, buffer, frames, nanos)
                    );
                }
            }
        }
    }
    let mut state = 0x1234_5678_abcd_eeff;
    for _ in 0..200_000 {
        let rate = random(&mut state) as u32;
        let buffer = (random(&mut state) as u32).max(1);
        let frames = random(&mut state) as usize;
        let nanos = random(&mut state);
        assert_eq!(
            callback_timing(rate, buffer, frames_to_nanos(rate, buffer), frames, nanos),
            original_callback(rate, buffer, frames, nanos)
        );
    }
}

#[test]
fn timing_remains_allocation_free() {
    perf::assert_no_churn(|| {
        black_box(scale_mach_time(black_box(24_000_000_000), 125, 3));
        black_box(nanos_to_frames(48_000, black_box(160_000_000)));
        black_box(callback_timing(
            48_000,
            128,
            2_666_666,
            black_box(128),
            160_000_000,
        ));
    });
}

#[test]
#[ignore = "CPU benchmark; run explicitly in release mode"]
fn benchmark_coreaudio_timing() {
    for (label, numer, denom) in [
        ("mach_identity", 1, 1),
        ("mach_fractional", 125, 3),
        ("mach_generic", 125, 7),
        ("mach_overflow", u32::MAX, 3),
        ("mach_integer_overflow", u32::MAX, 1),
    ] {
        kernel_bench::measure(label, 1024, || {
            for index in 0..256 {
                let time = if label.ends_with("overflow") {
                    u64::MAX - index
                } else {
                    24_000_000_000 + index
                };
                black_box(scale_mach_time(
                    black_box(time),
                    black_box(numer),
                    black_box(denom),
                ));
            }
        });
    }
    for (label, nanos) in [
        ("latency_normal", 160_000_000),
        ("latency_overflow", u64::MAX - 256),
    ] {
        kernel_bench::measure(label, 1024, || {
            for index in 0..256 {
                black_box(nanos_to_frames(black_box(48_000), black_box(nanos + index)));
            }
        });
    }
    for (label, frames) in [("callback_fixed", 128), ("callback_variable", 256)] {
        let rate = black_box(48_000);
        let buffer = black_box(128);
        let buffer_ns = frames_to_nanos(rate, buffer);
        let (_, churn) = perf::measure(|| {
            black_box(callback_timing(
                rate,
                buffer,
                buffer_ns,
                frames,
                160_000_000,
            ));
        });
        println!("{label}: allocation {churn:?}");
        kernel_bench::measure(label, 1024, || {
            for index in 0..256 {
                black_box(callback_timing(
                    rate,
                    buffer,
                    buffer_ns,
                    black_box(frames),
                    black_box(160_000_000 + index),
                ));
            }
        });
    }
}
