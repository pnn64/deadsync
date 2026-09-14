//! SOLA behavior and old/new measurements against 0.5.1216 (d78b945c0).
use super::*;
use crate::perf;
use std::hint::black_box;

#[allow(dead_code)]
#[path = "sola_work_baseline.rs"]
mod old;

fn random(seed: &mut u64) -> u64 {
    *seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
    *seed
}

fn samples(len: usize, seed: u64) -> Vec<f32> {
    let mut seed = seed;
    (0..len)
        .map(|_| (random(&mut seed) as i16) as f32 / 32768.0)
        .collect()
}

fn search_fixture(
    len: usize,
    distance: usize,
    exact: Option<usize>,
    seed: u64,
) -> (Vec<f32>, Vec<f32>) {
    let correlate = samples(len, seed);
    let mut buffer = samples(len + distance, seed + 137);
    if let Some(offset) = exact {
        buffer[offset..offset + len].copy_from_slice(&correlate);
    }
    (buffer, correlate)
}

#[test]
fn mono_search_preserves_first_exact_ties_and_final_candidate() {
    for len in [0, 1, 7, 8, 9, 31, 64, 360] {
        for offset in [0, 1, 12, 64, 97] {
            let (buffer, correlate) = search_fixture(len, 97, Some(offset), 1234);
            assert_eq!(
                find_closest_match(&buffer, &correlate),
                old::mono(&buffer, &correlate)
            );
            if len > 7 {
                assert_eq!(find_closest_match(&buffer, &correlate), offset);
            }
        }
    }
    let correlate = [1.0, -0.0, 3.0, 4.0];
    let buffer = [9.0, 1.0, 0.0, 3.0, 4.0, 1.0, -0.0, 3.0, 4.0];
    assert_eq!(find_closest_match(&buffer, &correlate), 1);
    assert_eq!(old::mono(&buffer, &correlate), 1);
    assert_eq!(find_closest_match(&[], &correlate), 0);
    assert_eq!(find_closest_match(&correlate, &correlate), 0);
}

#[test]
fn stereo_search_preserves_independent_first_matches() {
    for left_at in [None, Some(0), Some(1), Some(23), Some(97)] {
        for right_at in [None, Some(0), Some(2), Some(71), Some(97)] {
            for len in [0, 1, 7, 8, 9, 31, 360] {
                let (lb, lc) = search_fixture(len, 97, left_at, 91);
                let (rb, rc) = search_fixture(len, 97, right_at, 317);
                let actual = find_closest_match_stereo(&lb, &lc, &rb, &rc);
                assert_eq!(actual, old::stereo(&lb, &lc, &rb, &rc));
                assert_eq!(actual, (old::mono(&lb, &lc), old::mono(&rb, &rc)));
            }
        }
    }
}

#[test]
fn searches_match_random_and_nonfinite_samples() {
    let mut seed = 0x1217;
    for case in 0..700 {
        let len = (random(&mut seed) % 80) as usize;
        let distance = (random(&mut seed) % 80) as usize;
        let (mut lb, mut lc) = search_fixture(len, distance, None, random(&mut seed));
        let (mut rb, mut rc) = search_fixture(len, distance, None, random(&mut seed));
        if case % 3 == 0 && len > 0 {
            let exceptional = [
                f32::NAN,
                f32::INFINITY,
                f32::NEG_INFINITY,
                f32::MAX,
                -f32::MAX,
                f32::from_bits(1),
            ];
            for data in [&mut lb, &mut lc, &mut rb, &mut rc] {
                let index = random(&mut seed) as usize % data.len();
                data[index] = exceptional[(case / 3) % exceptional.len()];
            }
        }
        assert_eq!(find_closest_match(&lb, &lc), old::mono(&lb, &lc));
        assert_eq!(find_closest_match(&rb, &rc), old::mono(&rb, &rc));
        assert_eq!(
            find_closest_match_stereo(&lb, &lc, &rb, &rc),
            old::stereo(&lb, &lc, &rb, &rc)
        );
    }
}

fn assert_sample_bits(actual: &[f32], expected: &[f32]) {
    assert_eq!(actual.len(), expected.len());
    for (index, (&a, &b)) in actual.iter().zip(expected).enumerate() {
        assert!(
            a.to_bits() == b.to_bits() || (a.is_nan() && b.is_nan()),
            "sample {index}: {a:?} != {b:?}"
        );
    }
}

#[test]
fn dual_mono_preserves_signed_zero_nan_and_first_match_behavior() {
    // Equal first scores alone cannot establish equal searches. Both the
    // remaining candidates and the reference samples must match too.
    let mut left = vec![0.0; 24];
    let mut right = left.clone();
    left[8..16].fill(1.0);
    right[16..24].fill(1.0);
    for (lb, lc, rb, rc) in [
        (left.as_slice(), [1.0; 8], right.as_slice(), [1.0; 8]),
        (left.as_slice(), [1.0; 8], left.as_slice(), [-1.0; 8]),
    ] {
        assert_eq!(
            find_closest_match_stereo(lb, &lc, rb, &rc),
            old::stereo(lb, &lc, rb, &rc)
        );
    }
    for exact in [None, Some(0), Some(12), Some(180), Some(360)] {
        let (left, correlate) = search_fixture(360, 360, exact, 145);
        for exceptional in [None, Some(-0.0), Some(f32::NAN), Some(f32::INFINITY)] {
            let mut right = left.clone();
            let mut lc = correlate.clone();
            let mut rc = lc.clone();
            if let Some(value) = exceptional {
                right[37] = value;
                lc[7] = value;
                rc[7] = value;
            }
            assert_eq!(
                find_closest_match_stereo(&left, &lc, &right, &rc),
                old::stereo(&left, &lc, &right, &rc)
            );
        }
    }
    for a in [0.0, -0.0, f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        let b = if a == 0.0 { -a } else { a };
        assert_eq!(
            find_closest_match_stereo(&[a; 80], &[a; 31], &[b; 80], &[b; 31]),
            old::stereo(&[a; 80], &[a; 31], &[b; 80], &[b; 31])
        );
    }
}

fn source(frames: usize, channels: usize, periodic: bool) -> Vec<i16> {
    (0..frames * channels)
        .map(|i| {
            let frame = i / channels;
            let ch = i % channels;
            if periodic {
                (((frame % (61 + ch * 2)) * 977 + ch * 37) % 30001) as i16 - 15000
            } else {
                ((i.wrapping_mul(7919).wrapping_add(frame * 137)) % 30001) as i16 - 15000
            }
        })
        .collect()
}

#[test]
fn complete_stretching_preserves_packets_rate_changes_reset_and_eof() {
    for hz in [8000, 44100, 48000] {
        for channels in [1, 2, 6] {
            let mut actual = SolaStretcher::new(channels, hz);
            let mut expected = old::SolaStretcher::new(channels, hz);
            assert_eq!(
                std::mem::size_of_val(&actual),
                std::mem::size_of_val(&expected)
            );
            for (periodic, dual) in [(true, false), (false, false), (true, true)] {
                actual.reset();
                expected.reset();
                actual.set_speed_ratio(0.8);
                expected.set_speed_ratio(0.8);
                let input = if dual {
                    source(6000, 1, periodic)
                        .into_iter()
                        .flat_map(|sample| std::iter::repeat_n(sample, channels))
                        .collect()
                } else {
                    source(6000, channels, periodic)
                };
                let mut out = vec![Vec::new(); channels];
                let mut old_out = out.clone();
                for (packet, input) in input.chunks(317 * channels).enumerate() {
                    if packet == 9 {
                        actual.set_speed_ratio(1.3);
                        expected.set_speed_ratio(1.3);
                    }
                    actual.push_interleaved_i16(input);
                    expected.push_interleaved_i16(input);
                    for max in [0, 1, 257, 1024] {
                        let frames = actual.pull(&mut out, max);
                        assert_eq!(frames, expected.pull(&mut old_out, max));
                        assert_eq!(
                            actual.buffered_source_frames(),
                            expected.buffered_source_frames()
                        );
                        assert_eq!(
                            actual.trailing_speed_ratio().to_bits(),
                            expected.trailing_speed_ratio().to_bits()
                        );
                    }
                }
                actual.finish();
                expected.finish();
                loop {
                    let frames = actual.pull(&mut out, 257);
                    assert_eq!(frames, expected.pull(&mut old_out, 257));
                    if frames == 0 {
                        break;
                    }
                }
                for (a, b) in out.iter().zip(old_out) {
                    assert_sample_bits(a, &b);
                }
            }
        }
    }
}

#[test]
fn searches_remain_allocation_free() {
    let (a, pattern) = search_fixture(360, 360, Some(23), 901);
    let (b, other) = search_fixture(360, 360, Some(71), 801);
    let dual = a.clone();
    let dual_pattern = pattern.clone();
    perf::assert_no_churn(|| {
        black_box(find_closest_match(&a, &pattern));
        black_box(find_closest_match_stereo(&a, &pattern, &b, &other));
        black_box(find_closest_match_stereo(
            &a,
            &pattern,
            &dual,
            &dual_pattern,
        ));
    });
}

macro_rules! bench_stream {
    ($module:ident, $name:expr, $channels:expr, $periodic:expr, $dual:expr) => {{
        let mut state = $module::SolaStretcher::new($channels, 48000);
        state.set_speed_ratio(1.2);
        let input = if $dual {
            source(12_000, 1, $periodic)
                .into_iter()
                .flat_map(|sample| std::iter::repeat_n(sample, $channels))
                .collect()
        } else {
            source(12_000, $channels, $periodic)
        };
        let mut output = vec![Vec::with_capacity(16_000); $channels];
        perf::measure_sampled($name, 64, 12_000 * $channels, || {
            state.reset();
            state.set_speed_ratio(1.2);
            for ch in &mut output {
                ch.clear();
            }
            state.push_interleaved_i16(black_box(&input));
            state.finish();
            while state.pull(&mut output, 1024) != 0 {}
            black_box(&output);
        });
    }};
}

#[test]
#[ignore = "manual release comparison; run serially with --nocapture"]
fn benchmark_sola_work() {
    let versions = if std::env::var_os("DEADSYNC_PERF_REVERSE").is_some() {
        [false, true]
    } else {
        [true, false]
    };
    for (label, offset) in [
        ("first", Some(0)),
        ("early", Some(12)),
        ("middle", Some(180)),
        ("last", Some(360)),
        ("no-exact", None),
    ] {
        let (a, ac) = search_fixture(360, 360, offset, 103);
        let (b, bc) = search_fixture(360, 360, offset.map(|p| (p + 7).min(360)), 207);
        assert_eq!(find_closest_match(&a, &ac), old::mono(&a, &ac));
        assert_eq!(
            find_closest_match_stereo(&a, &ac, &b, &bc),
            old::stereo(&a, &ac, &b, &bc)
        );
        for stereo in [false, true] {
            for old in versions {
                let name = format!(
                    "search/{}/{label}/{}",
                    if stereo { "stereo" } else { "mono" },
                    if old { "old" } else { "new" }
                );
                let iterations = match label {
                    "first" => 4096,
                    "early" => 1024,
                    _ => 128,
                };
                perf::measure_sampled(&name, iterations, 1, || {
                    let (a, ac, b, bc) =
                        (black_box(&a), black_box(&ac), black_box(&b), black_box(&bc));
                    if stereo {
                        black_box(if old {
                            old::stereo(a, ac, b, bc)
                        } else {
                            find_closest_match_stereo(a, ac, b, bc)
                        });
                    } else {
                        black_box(if old {
                            old::mono(a, ac)
                        } else {
                            find_closest_match(a, ac)
                        });
                    }
                });
            }
        }
    }
    for (label, at) in [("first", Some(0)), ("early", Some(12)), ("no-exact", None)] {
        let (a, ac) = search_fixture(360, 360, at, 113);
        let (b, bc) = (a.clone(), ac.clone());
        assert_eq!(
            find_closest_match_stereo(&a, &ac, &b, &bc),
            old::stereo(&a, &ac, &b, &bc)
        );
        for old in versions {
            perf::measure_sampled(
                &format!("dual-mono/{label}/{}", if old { "old" } else { "new" }),
                match label {
                    "first" => 4096,
                    "early" => 1024,
                    _ => 128,
                },
                1,
                || {
                    let (a, ac, b, bc) =
                        (black_box(&a), black_box(&ac), black_box(&b), black_box(&bc));
                    black_box(if old {
                        old::stereo(a, ac, b, bc)
                    } else {
                        find_closest_match_stereo(a, ac, b, bc)
                    });
                },
            );
        }
    }
    for channels in [1, 2] {
        for periodic in [true, false] {
            for old in versions {
                let name = format!(
                    "stream/{}-{channels}ch/{}",
                    if periodic { "periodic" } else { "aperiodic" },
                    if old { "old" } else { "new" }
                );
                if old {
                    bench_stream!(old, &name, channels, periodic, false);
                } else {
                    bench_stream!(super, &name, channels, periodic, false);
                }
            }
        }
    }
    for periodic in [true, false] {
        for old in versions {
            let name = format!(
                "stream/dual-{}-2ch/{}",
                if periodic { "periodic" } else { "aperiodic" },
                if old { "old" } else { "new" }
            );
            if old {
                bench_stream!(old, &name, 2, periodic, true);
            } else {
                bench_stream!(super, &name, 2, periodic, true);
            }
        }
    }
}
