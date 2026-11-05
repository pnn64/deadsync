//! Standalone paired measurements; the actual crate keeps forbid(unsafe_code).
use super::*;
use crate::font_cache_checks::{fonts, original};
use std::hint::black_box;

#[path = "font_cache_ablation.rs"]
mod chain_only;

type MetricInput = (i32, i32, f32, i32);
type MetricsFn = fn(&[MetricInput], &mut [i32]);
type CacheFn = fn(&'static str, &FontMap) -> [Option<Glyph>; 128];

fn metrics_original(inputs: &[MetricInput], output: &mut [i32]) {
    for (&(width, add, scale, extra), out) in inputs.iter().zip(output) {
        *out = original::round_half_to_even_i32((width + add) as f32 * scale) + extra;
    }
}

fn metrics_current(inputs: &[MetricInput], output: &mut [i32]) {
    for (&(width, add, scale, extra), out) in inputs.iter().zip(output) {
        *out = round_half_to_even_i32((width + add) as f32 * scale) + extra;
    }
}

fn metric_inputs() -> [(String, [MetricInput; 128]); 3] {
    [
        (
            "font".into(),
            std::array::from_fn(|i| (i as i32, -3, 1.25, 1)),
        ),
        (
            "ties".into(),
            std::array::from_fn(|i| (i as i32 * 2 - 127, 0, 0.5, 0)),
        ),
        (
            "large".into(),
            std::array::from_fn(|i| (i32::MAX - i as i32 * 128, 0, 2.0, 0)),
        ),
    ]
}

#[test]
fn metric_caller_matches_scaled_advances() {
    for (_, inputs) in metric_inputs() {
        let mut old = [0; 128];
        let mut new = [0; 128];
        metrics_original(&inputs, &mut old);
        metrics_current(&inputs, &mut new);
        assert_eq!(new, old);
    }
}

#[test]
fn chain_ablation_preserves_all_glyph_fields_and_ownership() {
    for (depth, per, unicode, default) in [
        (0, 0, 0, false),
        (1, 16, 0, false),
        (1, 128, 0, true),
        (8, 16, 0, true),
        (8, 8, 129, true),
        (8, 1, 5000, true),
    ] {
        let map = fonts(depth, per, unicode, default);
        for name in ["f0", "f7", "missing"] {
            crate::font_cache_checks::same_table(
                &chain_only::compute_ascii_glyphs(name, &map),
                &original::compute_ascii_glyphs(name, &map),
                &mut std::io::sink(),
            );
        }
    }
}

fn allocation(label: &str, work: impl FnOnce()) {
    let (_, counts) = crate::perf_alloc::measure(work);
    println!("{label}: allocation {counts:?}");
}

#[test]
#[ignore = "release CPU and allocation benchmark"]
fn benchmark_font_cache() {
    for (name, inputs) in metric_inputs() {
        let functions: [MetricsFn; 2] = [metrics_original, metrics_current];
        let mut buffers = [[0; 128]; 2];
        let label = format!("metrics/{name}");
        for (variant, f) in functions.into_iter().enumerate() {
            allocation(
                &format!("{label}/{}", ["original", "current"][variant]),
                || f(&inputs, &mut buffers[variant]),
            );
        }
        crate::perf_bench::compare(&label, 10000, |new| {
            let i = usize::from(new);
            black_box(functions[i])(black_box(&inputs), black_box(&mut buffers[i]));
            black_box(&buffers[i]);
        });
    }
    for (depth, per, unicode, default) in [
        (0, 0, 0, false),
        (1, 0, 0, true),
        (1, 16, 0, true),
        (1, 128, 0, true),
        (1, 16, 0, false),
        (1, 128, 5000, true),
        (2, 16, 0, true),
        (4, 16, 0, true),
        (8, 16, 0, true),
        (8, 8, 129, true),
        (8, 1, 5000, true),
        (8, 128, 0, true),
        (64, 1, 0, true),
    ] {
        let map = fonts(depth, per, unicode, default);
        let label = format!("ascii/{depth}/{per}/{unicode}/{default}");
        let functions: [CacheFn; 2] = [original::compute_ascii_glyphs, compute_ascii_glyphs];
        for (variant, f) in functions.into_iter().enumerate() {
            allocation(
                &format!("{label}/{}", ["original", "current"][variant]),
                || drop(black_box(f("f0", &map))),
            );
        }
        crate::perf_bench::compare(&label, 2000, |new| {
            black_box(black_box(functions[usize::from(new)])(
                black_box("f0"),
                black_box(&map),
            ));
        });
    }
    for (depth, per, unicode, default) in [
        (1, 0, 0, true),
        (1, 16, 0, true),
        (8, 128, 0, true),
        (8, 16, 0, true),
        (8, 8, 129, true),
        (8, 1, 5000, true),
        (33, 1, 0, true),
    ] {
        let mut maps = [
            fonts(depth, per, unicode, default),
            fonts(depth, per, unicode, default),
        ];
        original::refresh_chain_keys(&mut maps[0]);
        refresh_chain_keys(&mut maps[1]);
        let functions: [fn(&mut FontMap); 2] = [original::refresh_chain_keys, refresh_chain_keys];
        let label = format!("refresh/{depth}/{per}/{unicode}/{default}");
        for (variant, f) in functions.into_iter().enumerate() {
            allocation(
                &format!("{label}/{}", ["original", "current"][variant]),
                || f(&mut maps[variant]),
            );
        }
        crate::perf_bench::compare(&label, 200, |new| {
            let i = usize::from(new);
            black_box(functions[i])(black_box(&mut maps[i]));
        });
    }
    let random = crate::font_cache_checks::round_values();
    let sets = [
        (
            "font",
            (0..128).map(|i| i as f32 * 0.125 - 5.0).collect::<Vec<_>>(),
        ),
        ("halves", (0..128).map(|i| i as f32 - 63.5).collect()),
        ("arbitrary", random.iter().step_by(7919).copied().collect()),
        (
            "large",
            (0..128)
                .map(|i| f32::from_bits(0x4f00_0000u32.wrapping_add(i * 7919)))
                .collect(),
        ),
    ];
    for (name, values) in sets {
        let functions: [fn(f32) -> i32; 2] =
            [original::round_half_to_even_i32, round_half_to_even_i32];
        let label = format!("round/{name}");
        for (variant, f) in functions.into_iter().enumerate() {
            allocation(
                &format!("{label}/{}", ["original", "current"][variant]),
                || {
                    for &v in &values {
                        black_box(f(v));
                    }
                },
            );
        }
        crate::perf_bench::compare(&label, 10000, |new| {
            let f = black_box(functions[usize::from(new)]);
            for &v in &values {
                black_box(f(black_box(v)));
            }
        });
    }
}

#[test]
#[ignore = "separate CPU comparisons for chain and sparse-map changes"]
fn benchmark_ablations() {
    for (depth, per, unicode, default) in [(8, 16, 0, true), (8, 8, 129, true), (1, 16, 0, false)] {
        let map = fonts(depth, per, unicode, default);
        for (name, functions) in [
            (
                "chain",
                [
                    original::compute_ascii_glyphs
                        as fn(&'static str, &FontMap) -> [Option<Glyph>; 128],
                    chain_only::compute_ascii_glyphs,
                ],
            ),
            (
                "sparse",
                [chain_only::compute_ascii_glyphs, compute_ascii_glyphs],
            ),
        ] {
            let label = format!("{name}/{depth}/{per}/{unicode}/{default}");
            for (variant, f) in functions.into_iter().enumerate() {
                allocation(
                    &format!("{label}/{}", ["original", "current"][variant]),
                    || drop(black_box(f("f0", &map))),
                );
            }
            crate::perf_bench::compare(&label, 2000, |new| {
                black_box(black_box(functions[usize::from(new)])(
                    black_box("f0"),
                    black_box(&map),
                ));
            });
        }
    }
}
