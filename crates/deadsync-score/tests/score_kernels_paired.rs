use deadsync_score::{
    GsCommentCounts, ghost::GhostData, parse_gs_comment_counts, parse_itl_points,
};
use std::hint::black_box;

#[path = "../../../tests/support/kernel_bench.rs"]
mod bench;
#[path = "support/score_kernels_original.rs"]
mod original;

// Compare within one executable as well as across archived production stages.
// Alternating variants on the same pinned thread reduces clock and load bias.
#[test]
#[ignore = "release paired CPU benchmark"]
fn benchmark_paired_score_kernels() {
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
            bench::compare(&format!("ghost/{kind}/{len}"), 10000, |current| {
                if kind == "better" {
                    *data.itg.last_mut().unwrap() = -1.0;
                    *data.ex.last_mut().unwrap() = -1.0;
                    *data.hard_ex.last_mut().unwrap() = -1.0;
                }
                let update: fn(&mut GhostData, &[Vec<f64>; 3]) -> bool = black_box(if current {
                    GhostData::update_best
                } else {
                    original::update_best
                });
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
        bench::compare(&format!("itl/{kind}/128"), 500, |current| {
            let parse: fn(&str) -> Option<(u32, u32)> = black_box(if current {
                parse_itl_points
            } else {
                original::parse_itl_points
            });
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
        bench::compare(&format!("gs/{kind}/128"), 300, |current| {
            let parse: fn(&str) -> GsCommentCounts = black_box(if current {
                parse_gs_comment_counts
            } else {
                original::parse_gs_comment_counts
            });
            for comment in black_box(&comments) {
                black_box(parse(black_box(comment)));
            }
        });
    }
}
