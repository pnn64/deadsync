use deadsync_score::{
    GsCommentCounts, ghost::GhostData, parse_gs_comment_counts, parse_itl_points,
};
use std::hint::black_box;

#[path = "../../../tests/support/kernel_bench.rs"]
mod bench;
#[path = "support/score_kernels_original.rs"]
mod original;
#[allow(dead_code)]
#[path = "../../../tests/support/perf.rs"]
mod perf;

fn assert_ghost_bits(actual: &GhostData, expected: &GhostData) {
    for (a, b) in [&actual.itg, &actual.ex, &actual.hard_ex].into_iter().zip([
        &expected.itg,
        &expected.ex,
        &expected.hard_ex,
    ]) {
        assert_eq!(a.len(), b.len());
        for (a, b) in a.iter().zip(b) {
            assert_eq!(a.to_bits(), b.to_bits());
        }
    }
}

#[test]
fn ghost_updates_preserve_independent_bests_ties_and_nonfinite_points() {
    let histories = [
        vec![],
        vec![0.0],
        vec![-0.0],
        vec![1.0],
        vec![-2.0, 1.0],
        vec![1.0, 2.0],
        vec![f64::NAN, 3.0],
        vec![f64::INFINITY, 3.0],
        vec![f64::NEG_INFINITY, 3.0],
        vec![1.0, f64::NAN],
        vec![1.0, f64::INFINITY],
        vec![1.0, f64::NEG_INFINITY],
    ];
    for (a, old) in histories.iter().enumerate() {
        for (b, new) in histories.iter().enumerate() {
            let mut actual = GhostData {
                itg: old.clone(),
                ex: histories[(a + b) % histories.len()].clone(),
                hard_ex: histories[(a * 3 + b) % histories.len()].clone(),
            };
            let incoming = [
                new.clone(),
                histories[(b + 1) % histories.len()].clone(),
                histories[(a + b + 2) % histories.len()].clone(),
            ];
            let mut expected = actual.clone();
            assert_eq!(
                actual.update_best(&incoming),
                original::update_best(&mut expected, &incoming)
            );
            assert_ghost_bits(&actual, &expected);
        }
    }
}

#[test]
fn numeric_parsers_match_original_over_overflow_unicode_and_generated_inputs() {
    let pieces = [
        "",
        "0",
        "00000",
        "1",
        "4294967294",
        "4294967295",
        "4294967296",
        "9999999999999999999999999999999999999",
        "00000000000000000000000000000000001",
        "12345",
        "\u{660}\u{661}",
        "\u{ff11}",
        "\u{e9}",
        "\u{2003}",
        "\0",
        "-1",
        "+2",
    ];
    for first in pieces {
        for second in pieces {
            let text = format!("{first} (P) + {second} (S) / 7500 / 12000");
            assert_eq!(
                parse_itl_points(&text),
                original::parse_itl_points(&text),
                "{text:?}"
            );
            for suffix in [
                "w", "W", "e", "g", "d", "wo", "wO", "m", "miss", "", "\u{e9}",
            ] {
                let text = format!("\u{2003}{first}{suffix}, {second} {suffix} \u{a0}, 7WO");
                assert_eq!(
                    parse_gs_comment_counts(&text),
                    original::parse_gs_comment_counts(&text),
                    "{text:?}"
                );
            }
        }
    }
    let alphabet: Vec<_> =
        "0123456789wWeEgGdDmMoO,+- /().\t\r\n\0\u{2003}\u{a0}\u{e9}\u{660}\u{ff11}"
            .chars()
            .collect();
    let mut state = 0x9e37_79b9u32;
    for len in 0..128 {
        for _ in 0..128 {
            let text: String = (0..len)
                .map(|_| {
                    state ^= state << 13;
                    state ^= state >> 17;
                    state ^= state << 5;
                    alphabet[state as usize % alphabet.len()]
                })
                .collect();
            assert_eq!(
                parse_itl_points(&text),
                original::parse_itl_points(&text),
                "{text:?}"
            );
            assert_eq!(
                parse_gs_comment_counts(&text),
                original::parse_gs_comment_counts(&text),
                "{text:?}"
            );
        }
    }
}

fn measure(label: &str, iterations: usize, mut work: impl FnMut()) {
    let (_, counts) = perf::measure(&mut work);
    assert_eq!(counts, perf::Churn::default(), "{label}");
    println!("{label}: allocation {counts:?}");
    bench::measure(label, iterations, work);
}

#[test]
#[ignore = "release CPU and allocation benchmark"]
fn benchmark_score_kernels() {
    for len in [64, 4096] {
        for kind in ["tie", "worse", "better", "invalid"] {
            let points: Vec<_> = (0..len).map(|i| i as f64).collect();
            let mut data = GhostData {
                itg: points.clone(),
                ex: points.clone(),
                hard_ex: points.clone(),
            };
            let mut incoming = [points.clone(), points.clone(), points];
            if kind == "worse" {
                for history in &mut incoming {
                    *history.last_mut().unwrap() -= 1.0;
                }
            } else if kind == "invalid" {
                for history in &mut incoming {
                    history[len / 2] = f64::NAN;
                    *history.last_mut().unwrap() += 1.0;
                }
            }
            let update: fn(&mut GhostData, &[Vec<f64>; 3]) -> bool =
                black_box(GhostData::update_best);
            measure(&format!("ghost/{kind}/{len}"), 10000, || {
                if kind == "better" {
                    *data.itg.last_mut().unwrap() = -1.0;
                    *data.ex.last_mut().unwrap() = -1.0;
                    *data.hard_ex.last_mut().unwrap() = -1.0;
                }
                black_box(update(black_box(&mut data), black_box(&incoming)));
            });
        }
    }
    for kind in ["common", "overflow", "unicode", "missing"] {
        let names: Vec<_> = (0..128)
            .map(|i| match kind {
                "common" => format!("{} (P) + {} (S)", 1000 + i * 10, 10000 + i * 100),
                "overflow" => format!("4294967296 (P) + 999999999999999999999 / {i} / 4294967295"),
                "unicode" => format!("\u{6771}\u{4eac} {i} \u{2605} {} (S)", 7500 + i),
                _ => format!(
                    "No scoring points (Expert chart {})",
                    char::from(b'A' + (i % 26) as u8)
                ),
            })
            .collect();
        let parse: fn(&str) -> Option<(u32, u32)> = black_box(parse_itl_points);
        measure(&format!("itl/{kind}/128"), 500, || {
            for name in black_box(&names) {
                black_box(parse(black_box(name)));
            }
        });
    }
    for kind in ["common", "overflow", "unicode", "empty"] {
        let comments: Vec<_> = (0..128)
            .map(|i| match kind {
                "common" => format!(
                    "[DS], {}w, {}e, {}g, {}d, {}wo, {}m",
                    1000 + i,
                    i,
                    i / 2,
                    i / 3,
                    i / 4,
                    i / 5
                ),
                "overflow" => format!("999999999999999999999w, 4294967296e, {i}g, 00000000000001m"),
                "unicode" => format!(
                    "\u{2003}{} W\u{a0}, \u{660}E, {i} \u{e9}, 7 wO, 2 m",
                    i + 1000
                ),
                _ => ["", "[DS]", "No judgments", "0W, 0E, 0G"][i as usize % 4].to_owned(),
            })
            .collect();
        let parse: fn(&str) -> GsCommentCounts = black_box(parse_gs_comment_counts);
        measure(&format!("gs/{kind}/128"), 300, || {
            for comment in black_box(&comments) {
                black_box(parse(black_box(comment)));
            }
        });
    }
}
