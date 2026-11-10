#[allow(dead_code)]
#[path = "../../../tests/support/allocations.rs"]
mod perf;

#[allow(dead_code)]
mod wav {
    include!("../src/wav.rs");

    mod kernel {
        use super::*;
        use crate::perf;


        #[test]
        fn float64_packets_preserve_half_boundaries_and_nonfinite_silence() {
            let mut values = Vec::new();
            for integer in -32770..=32770 {
                let v = (f64::from(integer) + 0.5) / 32767.0;
                values.extend([
                    v.next_down().next_down(),
                    v.next_down(),
                    v,
                    v.next_up(),
                    v.next_up().next_up(),
                ]);
            }
            for exponent in 0..=2047u64 {
                for sign in [0, 1 << 63] {
                    for mantissa in [
                        0,
                        1,
                        0x7_ffff_ffff_ffff,
                        0x8_0000_0000_0000,
                        0xf_ffff_ffff_ffff,
                    ] {
                        values.push(f64::from_bits(sign | (exponent << 52) | mantissa));
                    }
                }
            }
            let mut state = 0x1234_5678_9abc_def0u64;
            for _ in 0..2_000_000 {
                state ^= state << 13;
                state ^= state >> 7;
                state ^= state << 17;
                values.push(f64::from_bits(state));
            }
            let bytes: Vec<_> = values.iter().flat_map(|v| v.to_le_bytes()).collect();
            let mut output = Vec::with_capacity(values.len());
            perf::assert_no_churn(|| {
                decode_packet_into(&bytes, Encoding::Float64, &mut output).unwrap()
            });
            for (value, actual) in values.into_iter().zip(output) {
                let expected = if value.is_finite() {
                    (value * 32767.0).round() as i16
                } else {
                    0
                };
                assert_eq!(actual, expected, "{:016x}", value.to_bits());
            }
        }

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

    }
}
