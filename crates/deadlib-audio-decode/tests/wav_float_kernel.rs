#[path = "../../../tests/support/kernel_bench.rs"]
mod bench;
#[allow(dead_code)]
#[path = "../../../tests/support/perf.rs"]
mod perf;

#[allow(dead_code)]
mod wav {
    include!("../src/wav.rs");

    mod kernel {
        use super::*;
        use crate::{bench, perf};
        use std::hint::black_box;

        #[test]
        fn float32_packets_preserve_rounding_boundaries_and_nonfinite_silence() {
            let mut values = Vec::new();
            for integer in -32770..=32770 {
                let v = ((f64::from(integer) + 0.5) / 32767.0) as f32;
                for v in [
                    v.next_down().next_down(),
                    v.next_down(),
                    v,
                    v.next_up(),
                    v.next_up().next_up(),
                ] {
                    values.push(v);
                }
            }
            for exponent in 0..=255u32 {
                for sign in [0, 1 << 31] {
                    for mantissa in [0, 1, 0x3f_ffff, 0x40_0000, 0x7f_ffff] {
                        values.push(f32::from_bits(sign | (exponent << 23) | mantissa));
                    }
                }
            }
            let mut state = 0x1234_5678u32;
            for _ in 0..1_000_000 {
                state ^= state << 13;
                state ^= state >> 17;
                state ^= state << 5;
                values.push(f32::from_bits(state));
            }
            let bytes: Vec<_> = values.iter().flat_map(|v| v.to_le_bytes()).collect();
            let mut output = Vec::with_capacity(values.len());
            perf::assert_no_churn(|| {
                decode_packet_into(&bytes, Encoding::Float32, &mut output).unwrap()
            });
            for (value, actual) in values.into_iter().zip(output) {
                let expected = if value.is_finite() {
                    (f64::from(value) * 32767.0).round() as i16
                } else {
                    0
                };
                assert_eq!(actual, expected, "{:08x}", value.to_bits());
            }
        }

        #[test]
        #[ignore = "release CPU and allocation benchmark"]
        fn benchmark_wav_float() {
            for encoding in [Encoding::Float32, Encoding::Float64, Encoding::Pcm16] {
                for pattern in ["audio", "clipped", "exceptional"] {
                    for samples in [64usize, 8192] {
                        let bytes: Vec<_> = (0..samples)
                            .flat_map(|i| {
                                let value = match pattern {
                                    "audio" => f64::from(i.wrapping_mul(71) as i16) / 32768.0,
                                    "clipped" => f64::from(i.wrapping_mul(71) as i16) / 8192.0,
                                    _ => [
                                        0.0,
                                        -0.0,
                                        f64::INFINITY,
                                        f64::NEG_INFINITY,
                                        f64::NAN,
                                        f64::MAX,
                                        -f64::MAX,
                                        0.25,
                                    ][i % 8],
                                };
                                match encoding {
                                    Encoding::Float32 => (value as f32).to_le_bytes().to_vec(),
                                    Encoding::Float64 => value.to_le_bytes().to_vec(),
                                    _ => (value as i16).to_le_bytes().to_vec(),
                                }
                            })
                            .collect();
                        let mut output = Vec::with_capacity(samples);
                        let label = format!(
                            "wav/bytes={}/{pattern}/samples={samples}",
                            encoding.sample_bytes()
                        );
                        let mut work = || {
                            decode_packet_into(
                                black_box(&bytes),
                                black_box(encoding),
                                black_box(&mut output),
                            )
                            .unwrap();
                            black_box(&output);
                        };
                        let (_, counts) = perf::measure(&mut work);
                        println!("{label}: allocation {counts:?}");
                        bench::measure(&label, (1_000_000 / samples).max(100), &mut work);
                    }
                }
            }
        }
    }
}
