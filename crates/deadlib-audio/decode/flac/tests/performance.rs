//! Release encoder timings with PCM generation, validation and file I/O excluded.
//!
//! cargo test -p deadlib-audio-decode-flac --release --test performance --
//!     --ignored --nocapture --test-threads=1
//! FLAC_BENCH_FILTER selects comma-separated label substrings; FLAC_BENCH_CPU
//! selects a Windows logical processor. FLAC_BENCH_DUMP writes exact encoded
//! outputs for cross-build comparison. First-use and warm allocations are
//! measured separately from nine timing rounds after four warmup passes.
use deadlib_audio_decode_flac::*;
use std::{hint::black_box, io::Cursor, time::Instant};

#[path = "../../../../../tests/support/allocations.rs"]
#[allow(dead_code)]
mod allocations;

#[cfg(windows)]
fn cycles() -> u64 {
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetCurrentThread() -> *mut std::ffi::c_void;
        fn QueryThreadCycleTime(thread: *mut std::ffi::c_void, cycles: *mut u64) -> i32;
    }
    let mut result = 0;
    // SAFETY: the current-thread handle is valid and result is writable.
    assert_ne!(
        unsafe { QueryThreadCycleTime(GetCurrentThread(), &mut result) },
        0
    );
    result
}

#[cfg(not(windows))]
fn cycles() -> u64 {
    0
}

fn selected(label: &str) -> bool {
    let filter = std::env::var("FLAC_BENCH_FILTER").unwrap_or_default();
    filter.is_empty() || filter.split(',').any(|part| label.contains(part))
}

fn measure(label: &str, frames: usize, mut work: impl FnMut()) {
    let (_, first) = allocations::measure(&mut work);
    for _ in 0..4 {
        work();
    }
    let (_, warm) = allocations::measure(&mut work);
    for (phase, churn) in [("first", first), ("warm", warm)] {
        println!(
            "alloc,{label},{phase},{},{},{},{}",
            churn.allocs, churn.reallocs, churn.allocated_bytes, churn.peak_bytes
        );
    }
    let iterations =
        std::env::var("FLAC_BENCH_ITERS").map_or(3, |value| value.parse::<usize>().unwrap());
    assert!(iterations != 0);
    for round in 0..9 {
        let start_cycles = cycles();
        let start = Instant::now();
        for _ in 0..iterations {
            work();
        }
        let operations = (iterations * frames) as f64;
        let ns = start.elapsed().as_nanos() as f64 / operations;
        let cpu = (cycles() - start_cycles) as f64 / operations;
        println!("sample,{label},{round},{iterations},{frames},{cpu:.3},{ns:.3}");
    }
}

fn pcm(channels: u8, samples: usize, phase: usize, noise: bool) -> Vec<Vec<i32>> {
    let mut seed = 0x9e37_79b9u32 ^ phase as u32;
    (0..channels)
        .map(|channel| {
            (0..samples)
                .map(|i| {
                    seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
                    if noise {
                        (seed as i32) >> 8
                    } else {
                        let position = (i + phase) as f64;
                        let left = (position * 0.031).sin() * 3_000_000.0
                            + (position * 0.073).sin() * 1_000_000.0;
                        (left + (position * 0.011 + f64::from(channel) * 0.013).sin() * 500_000.0)
                            as i32
                    }
                })
                .collect()
        })
        .collect()
}

fn save(label: &str, bytes: &[u8]) {
    if let Some(directory) = std::env::var_os("FLAC_BENCH_DUMP") {
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::write(
            std::path::Path::new(&directory).join(format!("{label}.bin")),
            bytes,
        )
        .unwrap();
    }
}

fn raw_case(label: &str, channels: u8, level: u8, lengths: &[usize], noise: bool, fresh: bool) {
    if !selected(label) {
        return;
    }
    let options = EncodeOptions {
        channels,
        bits_per_sample: 24,
        compression_level: level,
        variable_block_size: true,
        ..EncodeOptions::default()
    };
    let frames: Vec<_> = lengths
        .iter()
        .enumerate()
        .map(|(i, &length)| pcm(channels, length, i * 4096, noise))
        .collect();
    let references: Vec<Vec<_>> = frames
        .iter()
        .map(|frame| frame.iter().map(Vec::as_slice).collect())
        .collect();
    let mut probe = FrameEncoder::new(options).unwrap();
    let mut decoder = FrameDecoder::new(None).unwrap();
    let mut encoded = Vec::new();
    let mut number = 0;
    for planes in &references {
        let bytes = probe.encode(planes, number).unwrap();
        let mut cold = FrameEncoder::new(options).unwrap();
        assert_eq!(bytes, cold.encode(planes, number).unwrap());
        let frame = decoder.decode(bytes).unwrap();
        for (actual, expected) in frame.pcm.planes().zip(planes) {
            assert_eq!(actual, *expected);
        }
        encoded.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
        encoded.extend_from_slice(bytes);
        number += planes[0].len() as u64;
    }
    save(label, &encoded);
    println!(
        "verified,{label},{},{},{}",
        references.len(),
        number,
        encoded.len()
    );
    let mut encoder = FrameEncoder::new(options).unwrap();
    measure(label, references.len(), || {
        let mut number = 0;
        for planes in &references {
            if fresh {
                encoder = FrameEncoder::new(options).unwrap();
            }
            black_box(encoder.encode(black_box(planes), number).unwrap());
            number += planes[0].len() as u64;
        }
    });
}

fn stream_case(label: &str, level: u8) {
    if !selected(label) {
        return;
    }
    let options = EncodeOptions {
        bits_per_sample: 24,
        compression_level: level,
        ..EncodeOptions::default()
    };
    let planes = pcm(2, 4096 * 8 + 257, 0, false);
    let references: Vec<_> = planes.iter().map(Vec::as_slice).collect();
    let encode = || {
        let mut encoder = Encoder::new(Cursor::new(Vec::new()), options).unwrap();
        encoder.write_planar(black_box(&references)).unwrap();
        encoder.finish_seekable().unwrap().output.into_inner()
    };
    let encoded = encode();
    let mut decoder = Decoder::new(encoded.as_slice()).unwrap();
    let mut offset = 0;
    while let Some(frame) = decoder.read_next().unwrap() {
        for (actual, expected) in frame.pcm.planes().zip(&planes) {
            assert_eq!(actual, &expected[offset..offset + actual.len()]);
        }
        offset += frame.pcm.samples();
    }
    assert_eq!(offset, planes[0].len());
    assert_eq!(decoder.integrity(), Integrity::Verified);
    save(label, &encoded);
    println!("verified,{label},9,{offset},{}", encoded.len());
    measure(label, 9, || {
        black_box(encode());
    });
}

#[test]
#[ignore = "controlled release encoder performance measurements"]
fn frame_and_stream_encoding() {
    assert!(
        !cfg!(debug_assertions),
        "run encoder timings with --release"
    );
    #[cfg(windows)]
    {
        #[link(name = "kernel32")]
        unsafe extern "system" {
            fn GetCurrentThread() -> *mut std::ffi::c_void;
            fn SetThreadAffinityMask(thread: *mut std::ffi::c_void, mask: usize) -> usize;
        }
        let cpu = std::env::var("FLAC_BENCH_CPU").map_or(0, |n| n.parse::<u32>().unwrap());
        let mask = 1usize
            .checked_shl(cpu)
            .expect("logical CPU outside affinity mask");
        // SAFETY: the current-thread pseudo-handle is valid; the OS checks the mask.
        assert_ne!(
            unsafe { SetThreadAffinityMask(GetCurrentThread(), mask) },
            0
        );
    }
    raw_case("mono-l5", 1, 5, &[4096; 8], false, false);
    raw_case("stereo-l3", 2, 3, &[4096; 8], false, false);
    raw_case("stereo-l5", 2, 5, &[4096; 8], false, false);
    raw_case("stereo-l8", 2, 8, &[4096; 8], false, false);
    raw_case("six-channel-l5", 6, 5, &[1024; 8], false, false);
    raw_case(
        "variable-l5",
        2,
        5,
        &[4096, 4096, 257, 257, 4096, 4096],
        false,
        false,
    );
    raw_case("cold-mono-l5", 1, 5, &[4096], false, true);
    raw_case("noise-l5", 2, 5, &[4096; 8], true, false);
    raw_case("fixed-only-l0", 2, 0, &[4096; 8], false, false);
    stream_case("stream-l5", 5);
    stream_case("stream-l8", 8);
}
