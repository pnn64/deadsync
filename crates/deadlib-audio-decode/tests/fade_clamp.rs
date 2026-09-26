use deadlib_audio_decode::resample::apply_fade_envelope;
use std::hint::black_box;

#[path = "fade_clamp/baseline.rs"]
mod baseline;
#[path = "../../../tests/support/perf.rs"]
#[allow(dead_code)]
mod perf;

fn random(seed: &mut u64) -> u64 {
    *seed ^= *seed << 13;
    *seed ^= *seed >> 7;
    *seed ^= *seed << 17;
    *seed
}

fn compare(input: &[i16], channels: usize, start: u64, fade: (i64, i64)) {
    let mut old = input.to_vec();
    let mut new = old.clone();
    baseline::apply_fade_envelope(&mut old, channels, start, fade);
    apply_fade_envelope(&mut new, channels, start, fade);
    assert_eq!(
        new, old,
        "channels {channels}, start {start}, fade {fade:?}"
    );
}

#[test]
fn fade_pcm_matches_before_clamp_removal() {
    let mut cases = 0;
    for channels in [0, 1, 2, 3, 6, 8] {
        for len in [0, 1, 2, 3, 7, 255, 512, 4097] {
            let input = (0..len).map(|i| (i * 7919) as i16).collect::<Vec<_>>();
            for start in [0, 1, 127, 255, 256, 511, 1 << 24, i64::MAX as u64, u64::MAX] {
                for fade in [
                    (0, 0),
                    (0, 256),
                    (256, 0),
                    (-100, 100),
                    (100, -100),
                    (i64::MIN, i64::MAX),
                    (i64::MAX - 1, i64::MAX),
                    (i64::MAX, i64::MIN),
                ] {
                    compare(&input, channels, start, fade);
                    cases += 1;
                }
            }
        }
    }
    let all_i16 = (i16::MIN..=i16::MAX).collect::<Vec<_>>();
    for channels in [1, 2, 3, 6, 8] {
        for fade in [
            (0, 65536),
            (65536, 0),
            (-100, 65536),
            (0, 2_000_000),
            (2_000_000, 0),
        ] {
            compare(&all_i16, channels, 1, fade);
            cases += 1;
        }
    }
    let mut seed = 1525;
    for case in 0..2048 {
        let input = (0..case % 513)
            .map(|_| random(&mut seed) as i16)
            .collect::<Vec<_>>();
        let start = random(&mut seed) % 100_000;
        let full = (random(&mut seed) % 200_000) as i64 - 50_000;
        let silence = (random(&mut seed) % 200_000) as i64 - 50_000;
        compare(&input, case % 9, start, (full, silence));
        cases += 1;
    }
    for start in 0..256 {
        compare(&[i16::MIN, -1, 0, 1, i16::MAX], 1, start, (0, 1_000_000));
        cases += 1;
    }
    eprintln!("{cases} PCM packet comparisons matched");
}

#[test]
fn finite_unit_interpolation_needs_no_clamp() {
    // Exercise the numeric invariant independently of the integer PCM rounding
    // that can conceal tiny differences in the interpolated volume.
    let check = |a: f32, b: f32, t: f32| {
        let value = (b - a).mul_add(t, a);
        assert_eq!(
            value.to_bits(),
            value.clamp(0.0, 1.0).to_bits(),
            "{a:?}, {b:?}, {t:?}"
        );
    };
    let edges = [
        0.0,
        -0.0,
        f32::from_bits(1),
        f32::MIN_POSITIVE,
        f32::from_bits(0x33000000),
        0.25,
        0.5,
        0.9999,
        f32::from_bits(1.0f32.to_bits() - 1),
        1.0,
    ];
    for a in edges {
        for b in edges {
            for t in edges {
                check(a, b, t);
            }
            // Float conversion can round the final index to the packet length.
            for frames in [
                1usize,
                257,
                (1 << 24) - 1,
                1 << 24,
                (1 << 24) + 1,
                usize::MAX,
            ] {
                for frame in [0, frames / 2, frames - 1] {
                    check(a, b, frame as f32 / frames as f32);
                }
            }
        }
    }
    let mut seed = 493;
    for _ in 0..1_000_000 {
        let a = f32::from_bits((random(&mut seed) % 0x3f800001) as u32);
        let b = f32::from_bits((random(&mut seed) % 0x3f800001) as u32);
        let t = f32::from_bits((random(&mut seed) % 0x3f800001) as u32);
        check(a, b, t);
        check(a, b, 1.0);
        check(a, 1.0, t);
        check(1.0, b, t);
    }
}

#[test]
fn fade_uses_no_allocation() {
    let mut input = vec![12345; 4096];
    for fade in [(0, 4096), (4096, 0), (-10, 10), (0, 0)] {
        perf::assert_no_churn(|| apply_fade_envelope(&mut input, 2, 0, fade));
    }
}

type Fade = fn(&mut [i16], usize, u64, (i64, i64));

#[test]
#[ignore = "manual paired release benchmark; --ignored --nocapture --test-threads=1"]
fn benchmark_fade_clamp() {
    for (name, frames, channels, start, fade) in [
        ("mono256", 256, 1, 0, (0, 4096)),
        ("stereo256", 256, 2, 0, (0, 4096)),
        ("stereo4096", 4096, 2, 0, (0, 4096)),
        ("surround256", 256, 6, 0, (0, 4096)),
        ("fade_in256", 256, 2, 0, (4096, 0)),
        ("cross_silence", 256, 2, 4000, (0, 4096)),
        ("unity", 256, 2, 0, (4096, 8192)),
        ("silence", 256, 2, 8192, (0, 4096)),
    ] {
        let input = (0..frames * channels)
            .map(|i| (i * 7919) as i16)
            .collect::<Vec<_>>();
        let mut variants = [
            ("before", baseline::apply_fade_envelope as Fade),
            ("after", apply_fade_envelope as Fade),
        ];
        if std::env::var_os("DEADSYNC_PERF_REVERSE").is_some() {
            variants.reverse();
        }
        for (variant, fade_fn) in variants {
            let fade_fn = black_box(fade_fn);
            let mut output = input.clone();
            // The unchanged fast paths take tens of nanoseconds; give their
            // timing batches enough work to reduce clock/scheduling noise.
            let iterations = if matches!(name, "unity" | "silence") {
                262_144
            } else {
                8192
            };
            perf::measure_sampled(&format!("{name}/{variant}"), iterations, frames, || {
                output.copy_from_slice(black_box(&input));
                fade_fn(
                    black_box(&mut output),
                    black_box(channels),
                    black_box(start),
                    black_box(fade),
                );
                black_box(&output);
            });
        }
    }
}
