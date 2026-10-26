use crate::config::Config;
use crate::de::{read::Reader, DecoderImpl};
use crate::error::DecodeError;
use crate::{Decode, Encode};
use std::hint::black_box;

#[path = "../../../tests/support/kernel_bench.rs"]
mod bench;
#[path = "bulk_original.rs"]
mod original;
#[allow(dead_code)]
#[path = "../../../tests/support/perf.rs"]
mod perf;

#[derive(Clone)]
struct Probe<'a> {
    bytes: &'a [u8],
    mode: u8,
    calls: Vec<(&'static str, usize)>,
}

impl Reader for Probe<'_> {
    fn read(&mut self, output: &mut [u8]) -> Result<(), DecodeError> {
        self.calls.push(("read", output.len()));
        if output.len() > self.bytes.len() {
            return Err(DecodeError::UnexpectedEnd {
                additional: output.len() - self.bytes.len(),
            });
        }
        output.copy_from_slice(&self.bytes[..output.len()]);
        self.bytes = &self.bytes[output.len()..];
        Ok(())
    }

    fn peek_read(&mut self, n: usize) -> Option<&[u8]> {
        self.calls.push(("peek", n));
        match self.mode {
            1 => None,
            2 => self.bytes.get(..n.saturating_sub(1)),
            _ => self.bytes.get(..n),
        }
    }

    fn consume(&mut self, n: usize) {
        self.calls.push(("consume", n));
        assert_ne!(self.mode, 3, "reader consume panic");
        self.bytes = self.bytes.get(n..).unwrap_or_default();
    }
}

fn check_endian<T: Copy + Encode + Decode<()> + std::fmt::Debug, C: Config>(
    values: &[T],
    config: C,
) {
    use crate::de::Decoder;
    let mut encoded = Vec::new();
    // Encode a tuple-free payload; the helper receives an already decoded length.
    for &value in values {
        encoded.extend(crate::encode_to_vec(value, config).unwrap());
    }
    for len in [0, values.len() / 2, values.len()] {
        for mode in 0..4 {
            for floats in [false, true] {
                let mut outputs = Vec::new();
                for old in [true, false] {
                    let reader = Probe {
                        bytes: &encoded,
                        mode,
                        calls: Vec::new(),
                    };
                    let mut decoder = DecoderImpl::new(reader, config, ());
                    // Exercise appending as well as allocation reuse.
                    let mut output = values.to_vec();
                    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        if old {
                            original::decode_endian_vec_into(&mut decoder, len, &mut output, floats)
                        } else {
                            super::decode_endian_vec_into(&mut decoder, len, &mut output, floats)
                        }
                    }));
                    let result = match result {
                        Ok(value) => format!("{value:?}"),
                        Err(_) => "panic".into(),
                    };
                    let wire = crate::encode_to_vec(&output, config).unwrap();
                    let reader = decoder.reader();
                    outputs.push((result, wire, reader.calls.clone(), reader.bytes.len()));
                }
                assert_eq!(
                    outputs[0], outputs[1],
                    "mode={mode} len={len} floats={floats}"
                );
            }
        }
    }
}

#[test]
fn endian_vectors_preserve_numeric_bits_reader_calls_and_unwind_state() {
    macro_rules! check {
        ($config:expr) => {{
            let c = $config;
            check_endian(&[0u16, 1, 0x1234, u16::MAX], c);
            check_endian(&[0i16, -1, i16::MIN, i16::MAX], c);
            check_endian(&[0u32, 1, 0x12345678, u32::MAX], c);
            check_endian(&[0i32, -1, i32::MIN, i32::MAX], c);
            check_endian(&[0u64, 1, 0x123456789abcdef0, u64::MAX], c);
            check_endian(&[0i64, -1, i64::MIN, i64::MAX], c);
            check_endian(&[0u128, 1, 1 << 96, u128::MAX], c);
            check_endian(&[0i128, -1, i128::MIN, i128::MAX], c);
            check_endian(
                &[0.0f32, -0.0, f32::INFINITY, f32::from_bits(0x7f801234)],
                c,
            );
            check_endian(
                &[
                    0.0f64,
                    -0.0,
                    f64::INFINITY,
                    f64::from_bits(0x7ff0000000001234),
                ],
                c,
            );
        }};
    }
    check!(crate::config::legacy().with_big_endian());
    check!(crate::config::legacy().with_little_endian());
}

fn measure(label: &str, iterations: usize, mut work: impl FnMut()) {
    let (_, counts) = perf::measure(&mut work);
    println!("{label}: allocation {counts:?}");
    bench::measure(label, iterations, work);
}

fn measure_endian<T: Copy + Encode + Decode<()>, C: Config>(label: &str, values: &[T], config: C) {
    let encoded = crate::encode_to_vec(values, config).unwrap();
    let mut output = Vec::<T>::with_capacity(values.len());
    crate::decode_from_slice_into_vec(&encoded, &mut output, config).unwrap();
    measure(label, 128, || {
        crate::decode_from_slice_into_vec(black_box(&encoded), black_box(&mut output), config)
            .unwrap();
        black_box(&output);
    });
}

#[test]
#[ignore = "release CPU and allocation benchmark"]
fn benchmark_codec_bulk() {
    for len in [64usize, 4096, 65536] {
        let config = crate::config::standard();
        let values: Vec<_> = (0..len).map(|i| i % 3 == 0).collect();
        let encoded = crate::encode_to_vec(&values, config).unwrap();
        let mut output = Vec::<bool>::with_capacity(len);
        measure(&format!("bool/{len}"), 128, || {
            crate::decode_from_slice_into_vec(black_box(&encoded), black_box(&mut output), config)
                .unwrap();
            black_box(&output);
        });
    }
    let mut encoded = crate::encode_to_vec(vec![true; 4096], crate::config::standard()).unwrap();
    *encoded.last_mut().unwrap() = 255;
    let mut output = Vec::<bool>::with_capacity(4096);
    measure("bool/invalid_last", 128, || {
        black_box(
            crate::decode_from_slice_into_vec(
                black_box(&encoded),
                black_box(&mut output),
                crate::config::standard(),
            )
            .unwrap_err(),
        );
        black_box(&output);
    });
    for len in [8, 4096] {
        let ints: Vec<_> = (0..len)
            .map(|i| (i as u32).wrapping_mul(0x9e3779b9))
            .collect();
        let floats: Vec<_> = (0..len)
            .map(|i| f64::from_bits((i as u64).wrapping_mul(0x9e3779b97f4a7c15)))
            .collect();
        let big = crate::config::legacy().with_big_endian();
        measure_endian(&format!("endian/u32/{len}"), &ints, big);
        measure_endian(&format!("endian/f64/{len}"), &floats, big);
        if len == 4096 {
            measure_endian("native/u32", &ints, crate::config::legacy());
            let signed: Vec<_> = ints.iter().map(|&i| i as i32).collect();
            measure_endian("varint/i32", &signed, crate::config::standard());
        }
    }
}
