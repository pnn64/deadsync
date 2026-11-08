use super::{apply_fade_envelope, kernel_bench as bench};
use std::{hint::black_box, time::Instant};

// Frozen scalar behavior, including the independent round() conversion oracle.
fn original_fade(samples: &mut [i16], channels: usize, start: u64, fade: (i64, i64)) {
    let (full, silence) = fade;
    if samples.is_empty() || channels == 0 || full == silence {
        return;
    }
    let frames = samples.len() / channels;
    if frames == 0 {
        return;
    }
    let volume = |position: i64| {
        let full = full as f64;
        let silence = silence as f64;
        let denom = silence - full;
        if denom.abs() < f64::EPSILON {
            return if silence > full { 0.0 } else { 1.0 };
        }
        ((-(position as f64 - full) / denom) + 1.0).clamp(0.0, 1.0) as f32
    };
    let start = start.min(i64::MAX as u64) as i64;
    let end = (frames as u64).min(i64::MAX as u64) as i64;
    let a = volume(start);
    let b = volume(end.saturating_add(start));
    if a > 0.9999 && b > 0.9999 {
        return;
    }
    if a == 0.0 && b == 0.0 {
        samples[..frames * channels].fill(0);
        return;
    }
    for (index, frame) in samples.chunks_exact_mut(channels).enumerate() {
        let value = (b - a).mul_add(index as f32 / frames as f32, a);
        if (value - 1.0).abs() < 0.0001 {
            continue;
        }
        for sample in frame {
            *sample = (f32::from(*sample) * value).round() as i16;
        }
    }
}

// Exercise the production portable loop even on an FMA-capable machine.
fn portable_fade(samples: &mut [i16], channels: usize, start: u64, fade: (i64, i64)) {
    let (full, silence) = fade;
    if samples.is_empty() || channels == 0 || full == silence {
        return;
    }
    let frames = samples.len() / channels;
    if frames == 0 {
        return;
    }
    let start = super::saturating_i64_from_u64(start);
    let end = super::saturating_i64_from_u64(frames as u64).saturating_add(start);
    let a = super::volume_for_frame(start, full, silence);
    let b = super::volume_for_frame(end, full, silence);
    if a > 0.9999 && b > 0.9999 {
        return;
    }
    if a == 0.0 && b == 0.0 {
        samples[..frames * channels].fill(0);
        return;
    }
    super::fade_samples(samples, channels, frames as f32, a, b);
}

fn assert_envelope(
    input: &[i16],
    channels: usize,
    start: u64,
    envelope: (i64, i64),
    offset: usize,
) {
    let mut guarded = vec![12_345; offset];
    guarded.extend_from_slice(input);
    guarded.extend([12_345; 7]);
    let mut expected = guarded.clone();
    original_fade(
        &mut expected[offset..offset + input.len()],
        channels,
        start,
        envelope,
    );
    for apply in [apply_fade_envelope, portable_fade] {
        let mut actual = guarded.clone();
        apply(
            &mut actual[offset..offset + input.len()],
            channels,
            start,
            envelope,
        );
        assert_eq!(
            actual, expected,
            "channels={channels}, start={start}, fade={envelope:?}, offset={offset}"
        );
    }
}

#[test]
fn fade_matches_every_pcm_value_across_envelopes_and_both_paths() {
    let input: Vec<_> = (i16::MIN..=i16::MAX).collect();
    for envelope in [
        (0, 1000),
        (1000, 0),
        (-71_952, 48),
        (i64::MIN, i64::MAX),
        (i64::MAX - 1, i64::MAX),
        (i64::MIN, i64::MIN + 1),
        (0, 0),
        (0, 10_000_000),
    ] {
        for start in [0, 1, 2, 17, 48, 125, 333, 500, 750, 999, 1000, u64::MAX] {
            for channels in [1, 2, 3, 6, 8, input.len()] {
                assert_envelope(&input, channels, start, envelope, channels % 8);
            }
        }
    }
}

#[test]
fn fade_preserves_rounding_tails_and_guards_with_random_layouts() {
    let mut seed = 0x1234_5678_u32;
    for case in 0..20_000 {
        seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
        let channels = case % 17;
        let frames = seed as usize % 1025;
        let input: Vec<_> = (0..frames * channels + case % 17)
            .map(|i| (i as u32).wrapping_mul(seed) as i16)
            .collect();
        let start = u64::from(seed % 10_000);
        let stop = i64::from(seed % 20_000);
        let envelope = if case % 2 == 0 {
            (-17, stop)
        } else {
            (stop, -17)
        };
        assert_envelope(&input, channels, start, envelope, case % 8);
    }
}

#[inline(never)]
fn fade(samples: &mut [i16], channels: usize, start: u64, envelope: (i64, i64)) {
    apply_fade_envelope(samples, channels, start, envelope);
}

#[test]
#[ignore = "release benchmark; archive before/after binaries and run alone with --ignored --nocapture --test-threads=1"]
fn benchmark_fade_envelope() {
    bench::pin_thread();
    let rotation = std::env::var("FADE_BENCH_ROTATION")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(0);
    let cases = [
        ("mono-256", 1, 256, 0, 50, (0, 5000)),
        ("stereo-256", 2, 256, 0, 50, (0, 5000)),
        ("mono-4096", 1, 4096, 0, 50, (0, 5000)),
        ("stereo-4096", 2, 4096, 0, 50, (0, 5000)),
        ("six-256", 6, 256, 0, 50, (0, 5000)),
        ("six-4096", 6, 4096, 0, 50, (0, 5000)),
        ("stereo-tail", 2, 257, 1, 50, (0, 5000)),
        ("stereo-rise", 2, 256, 0, 50, (5000, 0)),
        ("full-volume", 2, 256, 0, 0, (1000, 2000)),
        ("silence", 2, 256, 0, 3000, (0, 2000)),
        ("empty", 2, 0, 0, 50, (0, 5000)),
        ("stereo-1", 2, 1, 0, 50, (0, 5000)),
        ("near-unity", 2, 1024, 0, 50, (0, 10_000_000)),
        ("stereo-4", 2, 4, 0, 50, (0, 5000)),
        ("stereo-16", 2, 16, 0, 50, (0, 5000)),
        ("mono-16", 1, 16, 0, 50, (0, 5000)),
    ];
    for i in 0..cases.len() {
        let (name, channels, frames, tail, start, envelope) = cases[(i + rotation) % cases.len()];
        let input: Vec<i16> = (0..frames * channels + tail)
            .map(|j| (j as i32 * 977 + 37) as i16)
            .collect();
        let mut output = input.clone();
        let mut expected = input.clone();
        original_fade(&mut expected, channels, start, envelope);
        fade(&mut output, channels, start, envelope);
        assert_eq!(output, expected);
        let iterations = 500_000 / frames.max(1) + 200;
        for round in 0..10 {
            let cycles = bench::cycles();
            let begin = Instant::now();
            for _ in 0..iterations {
                output.copy_from_slice(black_box(&input));
                fade(
                    black_box(&mut output),
                    black_box(channels),
                    black_box(start),
                    black_box(envelope),
                );
                black_box(&output);
            }
            let ns = begin.elapsed().as_nanos() as f64 / iterations as f64;
            let cycles = (bench::cycles() - cycles) as f64 / iterations as f64;
            println!("fade-envelope,{name},{frames},{round},{iterations},{cycles:.4},{ns:.4}");
        }
    }
}
