use super::*;
use crate::judgment::judgment_time_error_music_ns_from_ms;
use std::hint::black_box;

#[path = "timing_conversion_original.rs"]
mod original;

fn next(state: &mut u64) -> u64 {
    *state ^= *state << 13;
    *state ^= *state >> 7;
    *state ^= *state << 17;
    *state
}

#[test]
fn conversion_delta_preserves_full_integer_range() {
    let mut values = vec![i64::MIN, i64::MIN + 1, -1, 0, 1, i64::MAX - 1, i64::MAX];
    for bit in 0..63 {
        for offset in -2..=2 {
            if let Some(v) = (1_i64 << bit).checked_add(offset) {
                values.push(v);
                values.push(-v);
            }
        }
    }
    let check = |lhs, rhs| {
        assert_eq!(
            timing_ns_delta_seconds(lhs, rhs).to_bits(),
            original::delta_seconds(lhs, rhs).to_bits(),
            "{lhs} - {rhs}"
        )
    };
    for &lhs in &values {
        for &rhs in &values {
            check(lhs, rhs);
        }
    }
    let mut state = 0xd134_2543_de82_ef95;
    for _ in 0..200_000 {
        let lhs = next(&mut state) as i64;
        check(lhs, next(&mut state) as i64);
    }
}

#[test]
fn conversion_rounding_saturation_and_invalid_rates_match() {
    let mut values = vec![
        0.0,
        -0.0,
        f32::NAN,
        f32::from_bits(0xffc0_1234),
        f32::INFINITY,
        f32::NEG_INFINITY,
        f32::MAX,
        -f32::MAX,
        f32::MIN_POSITIVE,
        f32::from_bits(1),
    ];
    for base in [
        0.000_000_5_f32,
        0.0005,
        0.0015,
        0.5,
        1.0,
        2.0,
        9_223_372_000.0,
        9_223_372_000_000.0,
    ] {
        for offset in -3..=3 {
            let value = f32::from_bits(base.to_bits().wrapping_add_signed(offset));
            values.extend([value, -value]);
        }
    }
    let check = |ms, rate| {
        assert_eq!(
            judgment_time_error_music_ns_from_ms(ms, rate),
            original::judgment_ns(ms, rate),
            "ms={ms:?} rate={rate:?}"
        );
        let profile = TimingProfile {
            windows_s: [ms, -ms, 0.0, f32::NAN, 0.0215],
            fa_plus_window_s: Some(ms),
            mine_window_s: ms,
        };
        assert_eq!(
            TimingProfileNs::from_profile_scaled(&profile, rate),
            original::profile_ns(&profile, rate),
            "seconds={ms:?} rate={rate:?}"
        );
        let profile = TimingProfile {
            fa_plus_window_s: None,
            ..profile
        };
        assert_eq!(
            TimingProfileNs::from_profile_scaled(&profile, rate),
            original::profile_ns(&profile, rate)
        );
    };
    for &ms in &values {
        for &rate in &values {
            check(ms, rate);
        }
    }
    let mut state = 0x1234_5678_9abc_def0;
    for _ in 0..300_000 {
        let ms = f32::from_bits(next(&mut state) as u32);
        check(ms, f32::from_bits(next(&mut state) as u32));
    }
}

#[test]
#[ignore = "release paired CPU benchmark"]
fn benchmark_timing_conversions() {
    for extreme in [false, true] {
        let kind = if extreme { "extreme" } else { "gameplay" };
        let mut state = 0xd134_2543_de82_ef95;
        let pairs: Vec<_> = (0..256)
            .map(|i| {
                if extreme {
                    (next(&mut state) as i64, next(&mut state) as i64)
                } else {
                    (i * 1_000_000_000 + 12345, i * 1_000_000_000 - 200_000_000)
                }
            })
            .collect();
        let inputs: Vec<_> = (0..256)
            .map(|i| {
                if extreme {
                    (
                        f32::from_bits(next(&mut state) as u32),
                        f32::from_bits(next(&mut state) as u32),
                    )
                } else {
                    ((i as f32 - 128.0) * 0.75, [1.0, 1.1, 1.5, 2.0][i % 4])
                }
            })
            .collect();
        let profiles: Vec<_> = inputs
            .iter()
            .map(|&(ms, rate)| {
                let profile = if extreme {
                    TimingProfile {
                        windows_s: [ms; 5],
                        fa_plus_window_s: Some(ms),
                        mine_window_s: ms,
                    }
                } else {
                    TimingProfile::default_itg_with_fa_plus()
                };
                (profile, rate)
            })
            .collect();
        for current in [false, true] {
            let (_, alloc) = crate::perf_alloc::measure(|| {
                for &(lhs, rhs) in &pairs {
                    black_box(if current {
                        timing_ns_delta_seconds(lhs, rhs)
                    } else {
                        original::delta_seconds(lhs, rhs)
                    });
                }
                for &(ms, rate) in &inputs {
                    black_box(if current {
                        judgment_time_error_music_ns_from_ms(ms, rate)
                    } else {
                        original::judgment_ns(ms, rate)
                    });
                }
                for &(ref profile, rate) in &profiles {
                    black_box(if current {
                        TimingProfileNs::from_profile_scaled(profile, rate)
                    } else {
                        original::profile_ns(profile, rate)
                    });
                }
            });
            assert_eq!(alloc, crate::perf_alloc::Churn::default());
        }
        crate::perf_bench::compare(&format!("conversion/delta/{kind}/256"), 1000, |current| {
            let convert: fn(i64, i64) -> f32 = black_box(if current {
                timing_ns_delta_seconds
            } else {
                original::delta_seconds
            });
            for &(lhs, rhs) in black_box(&pairs) {
                black_box(convert(black_box(lhs), black_box(rhs)));
            }
        });
        crate::perf_bench::compare(
            &format!("conversion/judgment/{kind}/256"),
            1000,
            |current| {
                let convert: fn(f32, f32) -> i64 = black_box(if current {
                    judgment_time_error_music_ns_from_ms
                } else {
                    original::judgment_ns
                });
                for &(ms, rate) in black_box(&inputs) {
                    black_box(convert(black_box(ms), black_box(rate)));
                }
            },
        );
        crate::perf_bench::compare(&format!("conversion/profile/{kind}/256"), 1000, |current| {
            let convert: fn(&TimingProfile, f32) -> TimingProfileNs = black_box(if current {
                TimingProfileNs::from_profile_scaled
            } else {
                original::profile_ns
            });
            for &(ref profile, rate) in black_box(&profiles) {
                black_box(convert(black_box(profile), black_box(rate)));
            }
        });
    }
}
