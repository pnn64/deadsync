use super::{Encoding, decode_packet_into};
use std::{hint::black_box, time::Instant};

#[path = "../../../tests/support/kernel_bench.rs"]
mod bench;

// Frozen pre-vectorization float32 behavior for exhaustive compatibility checks.
fn original_float32(value: f32) -> i16 {
    if !value.is_finite() {
        return 0;
    }
    let value = f64::from(value);
    (value * 32767.0 + 0.5f64.copysign(value)) as i16
}

fn reference(value: f64) -> i16 {
    if value.is_finite() {
        (value * 32767.0).round() as i16
    } else {
        0
    }
}

#[test]
fn float_packets_preserve_tails_offsets_storage_and_partial_errors() {
    let values = [
        0.0,
        -0.0,
        0.5 / 32767.0,
        -0.5 / 32767.0,
        0.25,
        -0.9,
        1.0,
        -1.0,
        1.5,
        -4.0,
        f64::MIN_POSITIVE,
        f64::from_bits(1),
        f64::MAX,
        -f64::MAX,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::NAN,
        f64::from_bits(0xfff0_0000_0000_0001),
    ];
    let mut cases = 0;
    let mut compared = 0;
    for encoding in [Encoding::Float32, Encoding::Float64] {
        for samples in (0..=97).chain([255, 256, 257, 8191, 8192, 8193]) {
            for offset in 0..32 {
                let mut bytes = vec![0xa5; offset];
                let mut expected = Vec::with_capacity(samples);
                for index in 0..samples {
                    let value = values[(index + offset) % values.len()];
                    if matches!(encoding, Encoding::Float32) {
                        bytes.extend_from_slice(&(value as f32).to_le_bytes());
                        expected.push(reference(f64::from(value as f32)));
                    } else {
                        bytes.extend_from_slice(&value.to_le_bytes());
                        expected.push(reference(value));
                    }
                }
                let end = bytes.len();
                bytes.extend_from_slice(&[0xa5; 8]);
                let mut output = Vec::with_capacity(samples.max(3));
                output.extend_from_slice(&[123; 3]);
                let storage = (output.as_ptr(), output.capacity());
                decode_packet_into(&bytes[offset..end], encoding, &mut output).unwrap();
                assert_eq!(output, expected, "samples={samples}, offset={offset}");
                assert_eq!((output.as_ptr(), output.capacity()), storage);
                assert!(bytes[..offset].iter().all(|&v| v == 0xa5));
                assert_eq!(&bytes[end..], &[0xa5; 8]);
                for extra in 1..encoding.sample_bytes() {
                    assert_eq!(
                        decode_packet_into(&bytes[offset..end + extra], encoding, &mut output),
                        Err("WAV packet ended mid-sample")
                    );
                    assert_eq!(output, expected);
                    assert_eq!((output.as_ptr(), output.capacity()), storage);
                }
                cases += 1;
                compared += samples;
            }
        }
    }
    println!("{cases} float packet/storage cases; {compared} exact PCM samples");
}

#[test]
#[ignore = "exhaustive release check of all 4294967296 f32 bit patterns"]
fn exhaustive_float32_packets_match_original() {
    let mut bytes = vec![0; 65536 * 4];
    let mut output = Vec::with_capacity(65536);
    for high in 0..=65535u32 {
        for (low, sample) in bytes.as_chunks_mut::<4>().0.iter_mut().enumerate() {
            *sample = ((high << 16) | low as u32).to_le_bytes();
        }
        decode_packet_into(&bytes, Encoding::Float32, &mut output).unwrap();
        for (low, &actual) in output.iter().enumerate() {
            let bits = (high << 16) | low as u32;
            assert_eq!(actual, original_float32(f32::from_bits(bits)), "{bits:08x}");
        }
    }
    println!("All 4294967296 f32 bit patterns match the original conversion");
}

#[inline(never)]
fn decode(bytes: &[u8], encoding: Encoding, output: &mut Vec<i16>) {
    decode_packet_into(bytes, encoding, output).unwrap();
}

struct Case {
    name: String,
    encoding: Encoding,
    samples: usize,
    bytes: Vec<u8>,
    offset: usize,
    length: usize,
    output: Vec<i16>,
}

fn fixture(name: &str, encoding: Encoding, pattern: &str, samples: usize) -> Case {
    let mut bytes = Vec::with_capacity(samples * encoding.sample_bytes() + 4096);
    let mut expected = Vec::with_capacity(samples);
    for i in 0..samples {
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
        let integer = (i as i32).wrapping_mul(0x7fa9_539d);
        match encoding {
            Encoding::Float32 => {
                bytes.extend_from_slice(&(value as f32).to_le_bytes());
                expected.push(reference(f64::from(value as f32)));
            }
            Encoding::Float64 => {
                bytes.extend_from_slice(&value.to_le_bytes());
                expected.push(reference(value));
            }
            Encoding::Pcm8 => {
                bytes.push(integer as u8);
                expected.push((i16::from(integer as u8) - 128) << 8);
            }
            Encoding::Pcm16 => {
                bytes.extend_from_slice(&(integer as i16).to_le_bytes());
                expected.push(integer as i16);
            }
            Encoding::Pcm24 => {
                bytes.extend_from_slice(&integer.to_le_bytes()[..3]);
                expected.push((integer >> 8) as i16);
            }
            Encoding::Pcm32 => {
                bytes.extend_from_slice(&integer.to_le_bytes());
                expected.push((integer >> 16) as i16);
            }
        }
    }
    let mut output = Vec::with_capacity(samples);
    let length = bytes.len();
    bytes.resize(length + 4096, 0);
    // Fix relative page offsets across before/after binaries for memcpy controls.
    let offset = (output.as_ptr() as usize)
        .wrapping_sub(bytes.as_ptr() as usize)
        .wrapping_add(128)
        & 4095;
    bytes.copy_within(..length, offset);
    decode(&bytes[offset..offset + length], encoding, &mut output);
    assert_eq!(output, expected);
    Case {
        name: format!("{name}-{pattern}-{samples}"),
        encoding,
        samples,
        bytes,
        offset,
        length,
        output,
    }
}

#[test]
#[ignore = "release CPU benchmark with raw rounds for before/after binaries"]
fn benchmark_wav_float_conversion() {
    bench::pin_thread();
    let rotation = std::env::var("WAV_FLOAT_BENCH_ROTATION")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(0);
    let mut cases = Vec::new();
    for (name, encoding) in [("f32", Encoding::Float32), ("f64", Encoding::Float64)] {
        for pattern in ["audio", "clipped", "exceptional"] {
            for samples in [64, 8192] {
                cases.push(fixture(name, encoding, pattern, samples));
            }
        }
    }
    for (name, encoding) in [
        ("pcm8", Encoding::Pcm8),
        ("pcm16", Encoding::Pcm16),
        ("pcm24", Encoding::Pcm24),
        ("pcm32", Encoding::Pcm32),
    ] {
        for samples in [64, 8192] {
            cases.push(fixture(name, encoding, "audio", samples));
        }
    }
    for index in 0..cases.len() {
        let index = (index + rotation) % cases.len();
        let Case {
            name,
            encoding,
            samples,
            bytes,
            offset,
            length,
            output,
        } = &mut cases[index];
        let bytes = &bytes[*offset..*offset + *length];
        let iterations = (8_000_000 / *samples).max(1000);
        for round in 0..10 {
            let start_cycles = bench::cycles();
            let start = Instant::now();
            for _ in 0..iterations {
                decode(black_box(bytes), black_box(*encoding), black_box(output));
                black_box(&*output);
            }
            let ns = start.elapsed().as_nanos() as f64 / iterations as f64;
            let cycles = (bench::cycles() - start_cycles) as f64 / iterations as f64;
            println!("wav-float,{name},{samples},{round},{iterations},{cycles:.4},{ns:.4}");
        }
    }
}
