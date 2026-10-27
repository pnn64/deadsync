use deadsync_core::song_time::*;
use std::hint::black_box;

#[path = "../../../tests/support/kernel_bench.rs"]
mod bench;
#[allow(dead_code)]
#[path = "support/song_time_original.rs"]
mod original;
#[allow(dead_code)]
#[path = "../../../tests/support/perf.rs"]
mod perf;

fn next(state: &mut u64) -> u64 {
    *state ^= *state << 13;
    *state ^= *state >> 7;
    *state ^= *state << 17;
    *state
}

fn float_edges() -> Vec<f32> {
    let mut values = Vec::new();
    for sign in [0, 0x8000_0000] {
        for exponent in 0..=255 {
            for mantissa in [0, 1, 0x3f_ffff, 0x40_0000, 0x7f_fffe, 0x7f_ffff] {
                values.push(f32::from_bits(sign | exponent << 23 | mantissa));
            }
        }
    }
    values
}

#[test]
fn seconds_and_rates_preserve_float_classes_and_rounding_boundaries() {
    let check = |value: f32| {
        assert_eq!(
            song_time_ns_from_seconds(value),
            original::song_time_ns_from_seconds(value),
            "seconds bits={:08x}",
            value.to_bits()
        );
        assert_eq!(
            normalized_song_rate(value).to_bits(),
            original::normalized_song_rate(value).to_bits(),
            "rate bits={:08x}",
            value.to_bits()
        );
    };
    for value in float_edges() {
        check(value);
    }
    // Neighbors of nanosecond half ties, the no-fraction f64 boundary, and i64 saturation.
    for nanos in (0..100_000)
        .map(|n| n as f64 + 0.5)
        .chain([2.0f64.powi(52), i64::MAX as f64])
    {
        let seconds = (nanos / 1e9) as f32;
        for bits in [
            seconds.to_bits() - 1,
            seconds.to_bits(),
            seconds.to_bits() + 1,
        ] {
            check(f32::from_bits(bits));
            check(-f32::from_bits(bits));
        }
    }
    let mut state = 0x1234_5678_9abc_def0;
    for _ in 0..2_000_000 {
        check(f32::from_bits(next(&mut state) as u32));
    }
}

#[test]
fn host_deltas_preserve_half_ties_integer_limits_and_extreme_rates() {
    let mut deltas = vec![i128::MIN, i128::MAX, -1, 0, 1];
    for bit in 0..127 {
        for offset in -1..=1 {
            let value = (1i128 << bit) + offset;
            deltas.extend([value, -value]);
        }
    }
    for delta in deltas {
        for rate in float_edges() {
            assert_eq!(
                scaled_song_delta_ns(delta, rate),
                original::scaled_song_delta_ns(delta, rate),
                "delta={delta}, rate bits={:08x}",
                rate.to_bits()
            );
        }
    }
    // Biasing this product without the sub-half guard would incorrectly produce one.
    for sign in [-1, 1] {
        let delta = sign * ((1i128 << 53) - 1);
        let rate = 2.0f32.powi(-54);
        assert_eq!(original::scaled_song_delta_ns(delta, rate), 0);
        assert_eq!(scaled_song_delta_ns(delta, rate), 0);
    }
    let mut state = 0x1234_5678_9abc_def0;
    for _ in 0..1_000_000 {
        let delta = ((u128::from(next(&mut state)) << 64) | u128::from(next(&mut state))) as i128;
        let rate = f32::from_bits(next(&mut state) as u32);
        assert_eq!(
            scaled_song_delta_ns(delta, rate),
            original::scaled_song_delta_ns(delta, rate)
        );
    }
}

#[test]
fn timestamp_callers_preserve_invalid_sentinels_and_saturation() {
    let mut state = 0x1234_5678_9abc_def0;
    for _ in 0..200_000 {
        let seconds = f32::from_bits(next(&mut state) as u32);
        let rate = f32::from_bits(next(&mut state) as u32);
        let timestamp = next(&mut state) as i64;
        assert_eq!(
            scaled_song_time_ns(seconds, rate),
            original::scaled_song_time_ns(seconds, rate)
        );
        for timestamp in [timestamp, INVALID_SONG_TIME_NS, i64::MAX] {
            assert_eq!(
                song_time_ns_add_seconds(timestamp, seconds),
                original::song_time_ns_add_seconds(timestamp, seconds)
            );
        }
    }
}

fn measure<T>(label: &str, mut work: impl FnMut() -> T) {
    let (_, counts) = perf::measure(&mut work);
    println!("{label}: allocation {counts:?}");
    bench::measure(label, 200, || {
        black_box(work());
    });
}

#[test]
#[ignore = "release CPU and allocation benchmark"]
fn benchmark_time_conversion() {
    let mut state = 0x1234_5678_9abc_def0;
    let seconds: Vec<_> = (0..4096)
        .map(|_| (next(&mut state) % 4_200_000) as f32 / 1000.0 - 600.0)
        .collect();
    let valid_rates: Vec<_> = (0..4096)
        .map(|_| 0.25 + (next(&mut state) % 192) as f32 / 128.0)
        .collect();
    let mut mixed_rates = valid_rates.clone();
    for (i, rate) in mixed_rates.iter_mut().enumerate().step_by(8) {
        *rate = [0.0, -0.0, -1.0, f32::INFINITY, f32::NAN][i / 8 % 5];
    }
    let deltas: Vec<_> = (0..4096)
        .map(|_| i128::from(next(&mut state) % 40_000_000) - 1_000_000)
        .collect();
    let extreme = float_edges();
    measure("seconds", || {
        for &seconds in black_box(&seconds) {
            black_box(song_time_ns_from_seconds(black_box(seconds)));
        }
    });
    measure("seconds_extreme", || {
        for &seconds in black_box(&extreme) {
            black_box(song_time_ns_from_seconds(black_box(seconds)));
        }
    });
    for (label, rates) in [("rate_valid", &valid_rates), ("rate_mixed", &mixed_rates)] {
        measure(label, || {
            for &rate in black_box(rates) {
                black_box(normalized_song_rate(black_box(rate)));
            }
        });
    }
    measure("host_delta", || {
        for (&delta, &rate) in black_box(&deltas).iter().zip(black_box(&valid_rates)) {
            black_box(scaled_song_delta_ns(black_box(delta), black_box(rate)));
        }
    });
    measure("scaled_seconds", || {
        for (&seconds, &rate) in black_box(&seconds).iter().zip(black_box(&valid_rates)) {
            black_box(scaled_song_time_ns(black_box(seconds), black_box(rate)));
        }
    });
    measure("offset_seconds", || {
        for &seconds in black_box(&seconds) {
            black_box(song_time_ns_add_seconds(
                black_box(10_000_000_000),
                black_box(seconds),
            ));
        }
    });
}
