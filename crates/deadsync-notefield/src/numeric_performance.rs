use super::*;
use std::hint::black_box;

#[path = "../../../tests/support/perf.rs"]
#[allow(dead_code)]
mod allocations;
#[path = "../../../tests/support/kernel_bench.rs"]
mod bench;
#[path = "numeric_original.rs"]
mod original;

fn random(state: &mut u64) -> u64 {
    *state ^= *state << 13;
    *state ^= *state >> 7;
    *state ^= *state << 17;
    *state
}

fn f32_inputs() -> Vec<u32> {
    let mut bits = Vec::new();
    for center in [
        0,
        1,
        0x007f_ffff,
        f32::MIN_POSITIVE.to_bits(),
        f32::EPSILON.to_bits(),
        0.001_f32.to_bits(),
        0.5_f32.to_bits(),
        1.0_f32.to_bits(),
        32767.5_f32.to_bits(),
        32768.0_f32.to_bits(),
        f32::MAX.to_bits(),
        f32::INFINITY.to_bits(),
        0x7fc0_0000,
        0x7fff_ffff,
    ] {
        for delta in 0..=8 {
            for magnitude in [
                center.saturating_sub(delta),
                center.saturating_add(delta).min(0x7fff_ffff),
            ] {
                bits.extend([magnitude, magnitude | 0x8000_0000]);
            }
        }
    }
    for integer in 0..32768 {
        let half = (integer as f32 + 0.5).to_bits();
        for magnitude in [half - 1, half, half + 1] {
            bits.extend([magnitude, magnitude | 0x8000_0000]);
        }
    }
    let mut seed = 0x1453_aa95_3213_ffff;
    bits.extend((0..500_000).map(|_| random(&mut seed) as u32));
    bits
}

fn f64_inputs() -> Vec<u64> {
    let mut bits = Vec::new();
    for center in [
        0,
        1,
        f64::MIN_POSITIVE.to_bits(),
        0.005_f64.to_bits(),
        1.0_f64.to_bits(),
        (f64::from(i32::MAX) / 100.0).to_bits(),
        (f64::from(u32::MAX) / 100.0).to_bits(),
        f64::MAX.to_bits(),
        f64::INFINITY.to_bits(),
        0x7ff8_0000_0000_0000,
        0x7fff_ffff_ffff_ffff,
    ] {
        for delta in 0..=8 {
            for magnitude in [
                center.saturating_sub(delta),
                center.saturating_add(delta).min(0x7fff_ffff_ffff_ffff),
            ] {
                bits.extend([magnitude, magnitude | 0x8000_0000_0000_0000]);
            }
        }
    }
    let mut seed = 0x9821_3377_ffff_aa43;
    bits.extend((0..100_000).map(|_| random(&mut seed)));
    let mut boundary = |integer: f64| {
        let center = ((integer + 0.5) / 100.0).to_bits();
        for delta in 0..=2 {
            for magnitude in [center.saturating_sub(delta), center.saturating_add(delta)] {
                bits.extend([magnitude, magnitude | 0x8000_0000_0000_0000]);
            }
        }
    };
    for integer in 0..65536 {
        boundary(f64::from(integer));
    }
    for exponent in 0..=32 {
        for delta in -4..=4 {
            let integer = (1u64 << exponent) as f64 + f64::from(delta);
            if integer >= 0.0 {
                boundary(integer);
            }
        }
    }
    for _ in 0..100_000 {
        boundary(f64::from(random(&mut seed) as u32));
    }
    bits
}

#[test]
fn signed_activity_preserves_all_classification_cases() {
    for bits in f32_inputs() {
        let value = f32::from_bits(bits);
        assert_eq!(
            signed_effect_active(value),
            original::signed_effect_active(value),
            "{bits:08x}"
        );
    }
}

#[test]
fn divisor_preserves_exact_return_bits_and_nan_sign() {
    for bits in f32_inputs() {
        let value = f32::from_bits(bits);
        assert_eq!(
            mod_divisor(value).to_bits(),
            original::mod_divisor(value).to_bits(),
            "{bits:08x}"
        );
    }
}

#[test]
fn rounded_i16_preserves_every_half_boundary_and_saturation() {
    const CONSTANTS: [i16; 7] = [
        clamp_rounded_i16(0.5),
        clamp_rounded_i16(-0.5),
        clamp_rounded_i16(32767.5),
        clamp_rounded_i16(-32767.5),
        clamp_rounded_i16(f32::NAN),
        clamp_rounded_i16(f32::INFINITY),
        clamp_rounded_i16(f32::NEG_INFINITY),
    ];
    assert_eq!(CONSTANTS, [1, -1, 32767, -32768, 0, 0, 0]);
    for bits in f32_inputs() {
        let value = f32::from_bits(bits);
        assert_eq!(
            clamp_rounded_i16(value),
            original::clamp_rounded_i16(value),
            "{bits:08x}"
        );
    }
}

#[test]
fn percent_keys_preserve_scaled_overflow_and_rounding() {
    for bits in f32_inputs() {
        let value = f32::from_bits(bits);
        assert_eq!(
            mod_percent_key(value),
            original::mod_percent_key(value),
            "{bits:08x}"
        );
    }
    for integer in 0..32768 {
        let center = ((integer as f32 + 0.5) / 100.0).to_bits();
        for magnitude in [center - 1, center, center + 1] {
            for bits in [magnitude, magnitude | 0x8000_0000] {
                let value = f32::from_bits(bits);
                assert_eq!(mod_percent_key(value), original::mod_percent_key(value));
            }
        }
    }
}

#[test]
fn centi_quantizers_preserve_f64_limits_and_special_values() {
    for bits in f64_inputs() {
        let value = f64::from_bits(bits);
        assert_eq!(
            quantize_centi_i32(value),
            original::quantize_centi_i32(value)
        );
        assert_eq!(
            quantize_centi_u32(value),
            original::quantize_centi_u32(value)
        );
    }
}

#[test]
#[ignore = "archive exact float classifications and quantized keys"]
fn dump_numeric_trace() {
    let mut trace = Vec::new();
    let single = f32_inputs();
    let double = f64_inputs();
    println!("corpus: f32 {}, f64 {}", single.len(), double.len());
    trace.extend_from_slice(&(single.len() as u64).to_le_bytes());
    trace.extend_from_slice(&(double.len() as u64).to_le_bytes());
    for bits in single {
        let value = f32::from_bits(bits);
        trace.extend_from_slice(&bits.to_le_bytes());
        trace.push(u8::from(signed_effect_active(value)));
        trace.extend_from_slice(&mod_divisor(value).to_bits().to_le_bytes());
        trace.extend_from_slice(&clamp_rounded_i16(value).to_le_bytes());
        trace.extend_from_slice(&mod_percent_key(value).to_le_bytes());
    }
    for bits in double {
        let value = f64::from_bits(bits);
        trace.extend_from_slice(&bits.to_le_bytes());
        trace.extend_from_slice(&quantize_centi_i32(value).to_le_bytes());
        trace.extend_from_slice(&quantize_centi_u32(value).to_le_bytes());
    }
    std::fs::write(
        std::env::var_os("NOTEFIELD_NUMERIC_TRACE_OUT").expect("trace path"),
        trace,
    )
    .unwrap();
}

fn compare(label: &str, mut work: impl FnMut(bool)) {
    bench::compare(label, 1000, &mut work);
    for (variant, current) in [("original", false), ("current", true)] {
        let (_, churn) = allocations::measure(|| work(current));
        assert_eq!(
            churn.allocs
                + churn.reallocs
                + churn.frees
                + churn.allocated_bytes
                + churn.freed_bytes
                + churn.peak_bytes,
            0
        );
        println!("{label}/{variant}: allocation {churn:?}");
    }
}

#[test]
#[ignore = "paired CPU cycles and allocation benchmark"]
fn benchmark_numeric() {
    for mode in ["normal", "positive", "inactive", "exceptional"] {
        let specials = [
            0.0,
            -0.0,
            f32::MIN_POSITIVE,
            -f32::MIN_POSITIVE,
            f32::EPSILON,
            -f32::EPSILON,
            0.001,
            -0.001,
            0.5,
            -0.5,
            32767.5,
            -32767.5,
            f32::MAX,
            -f32::MAX,
            f32::INFINITY,
            f32::NEG_INFINITY,
            f32::from_bits(0x7f80_0001),
            f32::from_bits(0xffc0_3345),
        ];
        let values: Vec<_> = (0..512)
            .map(|index| match mode {
                "normal" => (index as f32 - 256.0) * 0.371,
                "positive" => index as f32 * 0.215,
                "inactive" => specials[index % 6],
                _ => specials[index % specials.len()],
            })
            .collect();
        compare(&format!("activity/{mode}/512"), |current| {
            let call = black_box(if current {
                signed_effect_active as fn(f32) -> bool
            } else {
                original::signed_effect_active as fn(f32) -> bool
            });
            for &value in &values {
                black_box(call(black_box(value)));
            }
        });
        compare(&format!("divisor/{mode}/512"), |current| {
            let call = black_box(if current {
                mod_divisor as fn(f32) -> f32
            } else {
                original::mod_divisor as fn(f32) -> f32
            });
            for &value in &values {
                black_box(call(black_box(value)));
            }
        });
        for key in [false, true] {
            let label = if key { "percent" } else { "round_i16" };
            compare(&format!("{label}/{mode}/512"), |current| {
                let call = black_box(match (current, key) {
                    (true, true) => mod_percent_key as fn(f32) -> i16,
                    (true, false) => clamp_rounded_i16 as fn(f32) -> i16,
                    (false, true) => original::mod_percent_key as fn(f32) -> i16,
                    (false, false) => original::clamp_rounded_i16 as fn(f32) -> i16,
                });
                for &value in &values {
                    black_box(call(black_box(value)));
                }
            });
        }
        let values: Vec<_> = values.iter().map(|&value| f64::from(value)).collect();
        compare(&format!("centi_i32/{mode}/512"), |current| {
            let call = black_box(if current {
                quantize_centi_i32 as fn(f64) -> i32
            } else {
                original::quantize_centi_i32 as fn(f64) -> i32
            });
            for &value in &values {
                black_box(call(black_box(value)));
            }
        });
        compare(&format!("centi_u32/{mode}/512"), |current| {
            let call = black_box(if current {
                quantize_centi_u32 as fn(f64) -> u32
            } else {
                original::quantize_centi_u32 as fn(f64) -> u32
            });
            for &value in &values {
                black_box(call(black_box(value)));
            }
        });
    }
}

#[test]
#[ignore = "exhaustive positive/negative f32 patterns around every i16 result"]
fn exhaustive_i16_domain() {
    let start = 0.5_f32.to_bits();
    let end = 32767.5_f32.to_bits();
    for magnitude in start..=end {
        for bits in [magnitude, magnitude | 0x8000_0000] {
            let value = f32::from_bits(black_box(bits));
            assert_eq!(
                clamp_rounded_i16(value),
                original::clamp_rounded_i16(value),
                "{bits:08x}"
            );
        }
    }
    println!(
        "exhaustive f32 patterns checked: {}",
        u64::from(end - start + 1) * 2
    );
}
