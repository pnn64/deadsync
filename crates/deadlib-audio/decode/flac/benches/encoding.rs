//! Resident-PCM encoding; setup, verification and allocation counts are untimed.
use deadlib_audio_decode_flac::{checksum::Md5, *};
use std::{hint::black_box, io::Write, time::Instant};

#[allow(dead_code)]
#[path = "../../../../../tests/support/perf.rs"]
mod alloc;
#[path = "../../../../../tests/support/kernel_bench.rs"]
mod cpu;

fn pcm(bits: u8, channels: u8, n: usize, noise: bool) -> Vec<Vec<i32>> {
    let mut state = 9639u32;
    (0..channels)
        .map(|channel| {
            (0..n)
                .map(|i| {
                    state = state.wrapping_mul(1664525).wrapping_add(1013904223);
                    let value = if noise {
                        state as i32
                    } else {
                        ((i as f64 * 0.037 + f64::from(channel) * 0.001).sin() * 1e9
                            + (i as f64 * 0.083).sin() * 5e8) as i32
                            + ((state >> 24) as i32 - 128) * 4096
                    };
                    value >> (32 - bits)
                })
                .collect()
        })
        .collect()
}

fn verify(data: &[u8], options: EncodeOptions, planes: &[Vec<i32>]) {
    let mut decoder = FrameDecoder::new(Some(options.stream_info().unwrap())).unwrap();
    let frame = decoder.decode(data).unwrap();
    assert_eq!(frame.bytes, data.len());
    for (actual, expected) in frame.pcm.planes().zip(planes) {
        assert_eq!(actual, expected);
    }
}

fn main() {
    cpu::pin_thread();
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args[0] == "matrix" {
        // Preserve complete bytes, including model choices, Rice ties and CRCs.
        let mut output = std::fs::File::create(&args[1]).unwrap();
        for bits in 4..=32 {
            for channels in [1, 2, 8] {
                for compression_level in [0, 2, 5, 8] {
                    for (n, noise) in [(16, false), (67, true), (257, false)] {
                        let options = EncodeOptions {
                            bits_per_sample: bits,
                            channels,
                            compression_level,
                            ..Default::default()
                        };
                        let planes = pcm(bits, channels, n, noise);
                        let refs: Vec<_> = planes.iter().map(Vec::as_slice).collect();
                        let mut encoder = FrameEncoder::new(options).unwrap();
                        let data = encoder.encode(&refs, 0).unwrap();
                        verify(data, options, &planes);
                        output
                            .write_all(&(data.len() as u32).to_le_bytes())
                            .unwrap();
                        output.write_all(data).unwrap();
                    }
                }
            }
        }
        return;
    }
    let case = &args[0];
    let iterations: usize = args[1].parse().unwrap();
    assert!(iterations > 0);
    let (bits, channels, compression_level, n, noise) = match case.as_str() {
        "smooth16-l0" => (16, 2, 0, 4096, false),
        "smooth16-l2" => (16, 2, 2, 4096, false),
        "smooth16-l5" => (16, 2, 5, 4096, false),
        "smooth16-l8" => (16, 2, 8, 4096, false),
        "noise16-l2" => (16, 2, 2, 4096, true),
        "noise24-l2" => (24, 2, 2, 4096, true),
        "smooth32-l5" => (32, 1, 5, 4096, false),
        "short16-l2" => (16, 2, 2, 16, false),
        _ => panic!("unknown case: {case}"),
    };
    let options = EncodeOptions {
        bits_per_sample: bits,
        channels,
        compression_level,
        ..Default::default()
    };
    let planes = pcm(bits, channels, n, noise);
    let refs: Vec<_> = planes.iter().map(Vec::as_slice).collect();
    let mut encoder = FrameEncoder::new(options).unwrap();
    let (_, cold) = alloc::measure(|| {
        black_box(encoder.encode(&refs, 0).unwrap());
    });
    let data = encoder.encode(&refs, 0).unwrap();
    verify(data, options, &planes);
    let bytes = data.len();
    let mut digest = Md5::new();
    digest.update(data);
    let signature = digest.finalize();
    let (_, warm) = alloc::measure(|| {
        black_box(encoder.encode(&refs, 0).unwrap());
    });
    let cycles = cpu::cycles();
    let start = Instant::now();
    for _ in 0..iterations {
        black_box(encoder.encode(black_box(&refs), 0).unwrap());
    }
    let nanos = start.elapsed().as_nanos();
    let cycles = cpu::cycles() - cycles;
    println!(
        "{case}\t{iterations}\t{cycles}\t{nanos}\t{}\t{bytes}\t{}\t{}\t{}\t{}\t{}\t{:02x?}",
        n * usize::from(channels),
        cold.allocs,
        cold.reallocs,
        cold.allocated_bytes,
        warm.allocs,
        warm.reallocs,
        signature,
    );
}
