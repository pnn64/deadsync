use super::*;
use crate::perf::measure;
use crate::pipelines_support::compare;
use std::hint::black_box;

fn chart(difficulty: &str, doubles: bool) -> PreviewChart {
    PreviewChart {
        difficulty: difficulty.into(),
        doubles,
        meter: 12,
        lanes: if doubles { 8 } else { 4 },
        notes: Arc::from([]),
    }
}

fn fixture(count: usize, top: Option<bool>) -> Vec<PreviewChart> {
    let mut charts: Vec<_> = (0..count).map(|i| chart("Hard", i % 2 != 0)).collect();
    if let Some(doubles) = top
        && let Some(last) = charts.last_mut()
    {
        *last = chart("cHaLlEnGe", doubles);
    }
    charts
}

#[test]
fn preview_selection_keeps_priority_case_and_fallbacks() {
    for word in ["challenge", "expert"] {
        for mask in 0..1 << word.len() {
            let mixed: String = word
                .bytes()
                .enumerate()
                .map(|(i, byte)| {
                    char::from(if mask & (1 << i) != 0 {
                        byte.to_ascii_uppercase()
                    } else {
                        byte
                    })
                })
                .collect();
            for doubles in [false, true] {
                let charts = [
                    chart("Hard", false),
                    chart(&mixed, doubles),
                    chart("Edit", false),
                ];
                assert_eq!(default_chart(&charts), Some(1));
                assert_eq!(
                    default_chart(&charts),
                    pipelines_original::default_chart(&charts)
                );
            }
        }
    }
    let charts = [
        chart("Expert", true),
        chart("Challenge", false),
        chart("Expert", false),
    ];
    assert_eq!(default_chart(&charts), Some(1));
    assert_eq!(default_chart(&[]), None);
    assert_eq!(
        default_chart(&[chart("Edit", true), chart("Hard", true)]),
        Some(1)
    );
}

#[test]
fn preview_selection_matches_baseline_across_generated_lists() {
    let difficulties = [
        "",
        "Hard",
        "Edit",
        "Challenge",
        "EXPERT",
        " expert",
        "Expert ",
        "Expert\0",
        "Éxpert",
        "ＣＨＡＬＬＥＮＧＥ",
        "猫",
        "challenge-extra",
    ];
    let mut seed = 0x714a_015b_u32;
    for length in 0..=24 {
        for _ in 0..160 {
            let charts: Vec<_> = (0..length)
                .map(|_| {
                    seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                    chart(
                        difficulties[(seed as usize >> 8) % difficulties.len()],
                        seed & 1 != 0,
                    )
                })
                .collect();
            assert_eq!(
                default_chart(&charts),
                pipelines_original::default_chart(&charts)
            );
        }
    }
}

#[test]
fn preview_selection_does_not_allocate_or_copy_difficulties() {
    for top in [None, Some(false), Some(true)] {
        let charts = fixture(64, top);
        let (old, before) = measure(|| pipelines_original::default_chart(&charts));
        let (new, after) = measure(|| default_chart(&charts));
        assert_eq!(old, new);
        assert!(before.allocs > 0);
        assert_eq!(after.allocs + after.reallocs + after.frees, 0);
    }
}

#[test]
#[ignore = "paired release benchmark; run with --ignored --nocapture --test-threads=1"]
fn benchmark_ui_pipelines_preview_selection() {
    for (count, top, label) in [
        (1, Some(false), "single"),
        (12, Some(false), "single"),
        (12, Some(true), "double"),
        (64, None, "fallback"),
        (128, Some(false), "single"),
    ] {
        let charts = fixture(count, top);
        compare(
            &format!("preview/{count}-{label}"),
            || {
                black_box(pipelines_original::default_chart(black_box(&charts)));
            },
            || {
                black_box(default_chart(black_box(&charts)));
            },
        );
    }
}
