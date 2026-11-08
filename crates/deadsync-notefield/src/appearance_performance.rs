use super::*;
use std::{hint::black_box, time::Instant};

#[path = "../../../tests/support/kernel_bench.rs"]
mod bench;

// The original composition, before the identity return in the combined helper.
#[inline(always)]
fn original_alpha_glow(y: f32, y_offset: f32, cache: &NoteAppearanceCache) -> (f32, f32) {
    let visible = appearance_note_alpha_cached(y, y_offset, cache);
    (
        appearance_note_actor_alpha_from_alpha(visible),
        appearance_note_glow_from_alpha(visible),
    )
}

fn next(state: &mut u32) -> u32 {
    *state ^= *state << 13;
    *state ^= *state >> 17;
    *state ^= *state << 5;
    *state
}

fn value(state: &mut u32, index: usize) -> f32 {
    match index % 4 {
        0 => [
            0.0,
            -0.0,
            f32::NAN,
            f32::INFINITY,
            f32::NEG_INFINITY,
            f32::MIN_POSITIVE,
            f32::from_bits(1),
            f32::MAX,
        ][next(state) as usize % 8],
        1 => (next(state) % 10000) as f32 * 0.1 - 200.0,
        2 => (next(state) % 2000) as f32 * 0.001 - 0.5,
        _ => f32::from_bits(next(state)),
    }
}

#[test]
fn alpha_glow_matches_original_with_float_patterns_and_independent_positions() {
    let mut state = 0x1234_5678;
    for case in 0..32768 {
        let identity = case % 2 == 0;
        let [
            hidden,
            hidden_offset,
            sudden,
            sudden_offset,
            stealth,
            stealth_col,
            blink,
            random_vanish,
        ] = std::array::from_fn(|i| {
            if identity {
                if next(&mut state) & 1 == 0 { 0.0 } else { -0.0 }
            } else {
                value(&mut state, i + case)
            }
        });
        let params = NoteAlphaParams {
            hidden,
            hidden_offset,
            sudden,
            sudden_offset,
            stealth,
            stealth_col,
            blink,
            random_vanish,
            stealth_type: case & 2 != 0,
            stealth_past_receptors: case & 4 != 0,
        };
        let cache = black_box(note_appearance_cache(
            value(&mut state, case),
            value(&mut state, case + 1),
            params,
        ));
        for sample in 0..64 {
            let y = black_box(value(&mut state, sample));
            let y_offset = black_box(value(&mut state, sample + 1));
            let expected = original_alpha_glow(y, y_offset, &cache);
            let actual = appearance_note_alpha_glow_cached(y, y_offset, &cache);
            assert_eq!(
                [actual.0.to_bits(), actual.1.to_bits()],
                [expected.0.to_bits(), expected.1.to_bits()],
                "case={case}, sample={sample}, y={:08x}, y_offset={:08x}",
                y.to_bits(),
                y_offset.to_bits(),
            );
            if identity {
                assert_eq!(
                    [actual.0.to_bits(), actual.1.to_bits()],
                    [1.0f32.to_bits(), 0]
                );
            }
        }
    }
}

#[inline(never)]
fn before_appearance(ys: &[f32], cache: &NoteAppearanceCache, output: &mut [(f32, f32)]) {
    for (&y, out) in ys.iter().zip(output) {
        *out = original_alpha_glow(y, y, cache);
    }
}

#[inline(never)]
fn after_appearance(ys: &[f32], cache: &NoteAppearanceCache, output: &mut [(f32, f32)]) {
    for (&y, out) in ys.iter().zip(output) {
        *out = appearance_note_alpha_glow_cached(y, y, cache);
    }
}

fn cache_for_case(case: usize) -> NoteAppearanceCache {
    let (hidden, sudden, stealth, random_vanish) = match case % 6 {
        0 => (0.0, 0.0, 0.0, 0.0),
        1 => (1.0, 0.0, 0.0, 0.0),
        2 => (0.0, 1.0, 0.0, 0.0),
        3 => (1.0, 1.0, 0.0, 0.0),
        4 => (0.0, 0.0, 0.25, 0.0),
        _ => (0.75, 0.5, 0.1, 0.25),
    };
    note_appearance_cache(
        9.0,
        0.0,
        NoteAlphaParams {
            hidden,
            sudden,
            stealth,
            random_vanish,
            ..Default::default()
        },
    )
}

#[test]
#[ignore = "paired alpha/glow timing; run release with --nocapture --test-threads=1"]
fn measure_alpha_glow() {
    bench::pin_thread();
    let rotation = std::env::var("DEADSYNC_APPEARANCE_BENCH_ROTATION")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(0);
    let names = [
        "identity",
        "hidden",
        "sudden",
        "hidden-sudden",
        "stealth-control",
        "mixed",
        "identity-fade",
        "hidden-fade-control",
        "sudden-fade-control",
        "hidden-sudden-fade-control",
        "stealth-fade-control",
        "mixed-fade",
    ];
    println!("case,notes,round,variant,iterations,cycles,ns");
    for index in 0..names.len() {
        let case = (index + rotation % names.len()) % names.len();
        let cache = cache_for_case(case);
        let ys: Vec<_> = (0..256)
            .map(|n| {
                if case < 6 {
                    -80.0 + ((n * 73) % 256) as f32 * 2.5
                } else if case % 6 == 2 {
                    160.5 + n as f32 * 39.0 / 255.0
                } else {
                    120.5 + n as f32 * 39.0 / 255.0
                }
            })
            .collect();
        let mut old = vec![(0.0, 0.0); ys.len()];
        let mut new = old.clone();
        before_appearance(&ys, &cache, &mut old);
        after_appearance(&ys, &cache, &mut new);
        for (a, b) in old.iter().zip(&new) {
            assert_eq!(
                [a.0.to_bits(), a.1.to_bits()],
                [b.0.to_bits(), b.1.to_bits()]
            );
        }
        let iterations = 3000;
        for round in 0..10 {
            for variant in [(round + rotation % 2) % 2, 1 - (round + rotation % 2) % 2] {
                let start_cycles = bench::cycles();
                let start = Instant::now();
                for _ in 0..iterations {
                    if variant == 0 {
                        before_appearance(black_box(&ys), black_box(&cache), black_box(&mut old));
                        black_box(&old);
                    } else {
                        after_appearance(black_box(&ys), black_box(&cache), black_box(&mut new));
                        black_box(&new);
                    }
                }
                let ns = start.elapsed().as_nanos() as f64 / iterations as f64;
                let cycles = (bench::cycles() - start_cycles) as f64 / iterations as f64;
                println!(
                    "{},{},{round},{variant},{iterations},{cycles:.4},{ns:.4}",
                    names[case],
                    ys.len()
                );
            }
        }
    }
}
