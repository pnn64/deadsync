use super::*;
use crate::{paired_bench, perf};
use std::hint::black_box;

mod original {
    use super::*;
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/support/perf_1870_subline.rs"
    ));
}

fn row(parts: [&str; 4]) -> SongRow {
    SongRow {
        artist: parts[0].into(),
        bpm: parts[1].into(),
        length: parts[2].into(),
        credit: parts[3].into(),
        ..SongRow::default()
    }
}

#[test]
fn subtitles_preserve_every_field_combination_and_exact_text() {
    for parts in [
        ["Artist", "150-190", "1:52", "Charter"],
        ["\u{97f3}\u{697d}", "  0 ", "\u{e9}\0", " A  -  B "],
        [" ", "\t", "\n", "  "],
    ] {
        for mask in 0..16 {
            let selected =
                std::array::from_fn(|i| if mask & (1 << i) != 0 { parts[i] } else { "" });
            let song = row(selected);
            let expected = selected
                .iter()
                .enumerate()
                .filter(|(_, text)| !text.is_empty())
                .map(|(i, text)| {
                    if i == 1 {
                        format!("{text} bpm")
                    } else {
                        (*text).into()
                    }
                })
                .collect::<Vec<_>>()
                .join("  -  ");
            let (actual, churn) = perf::measure(|| song.subline());
            assert_eq!(actual, expected, "mask={mask}");
            assert_eq!(actual, original::subline(&song));
            assert_eq!(churn.allocs, usize::from(mask != 0));
            assert_eq!(churn.reallocs, 0);
            assert_eq!(churn.allocated_bytes, actual.len());
        }
    }
    let long = "\u{97f3}\u{e9}".repeat(4096);
    let song = row([&long, &long, &long, &long]);
    assert_eq!(song.subline(), original::subline(&song));
}

#[test]
#[ignore = "paired release benchmark; run with --ignored --nocapture --test-threads=1"]
fn benchmark_pack_subtitles() {
    let long = "Long artist and charter ".repeat(24);
    for (name, song) in [
        ("empty", row(["", "", "", ""])),
        ("artist", row(["Artist", "", "", ""])),
        ("bpm", row(["", "150-190", "", ""])),
        ("full", row(["Artist", "150-190", "1:52", "Charter"])),
        (
            "unicode",
            row(["\u{97f3}\u{697d}", "180", "2:34", "\u{e9}lan"]),
        ),
        ("long", row([&long, "150-190", "12:34", &long])),
    ] {
        let label = format!("subtitle/{name}");
        let (_, before) = perf::measure(|| original::subline(black_box(&song)));
        let (_, after) = perf::measure(|| black_box(&song).subline());
        println!("{label} allocations: original {before:?}; current {after:?}");
        paired_bench::compare(&label, 20_000, |current| {
            drop(black_box(if current {
                black_box(&song).subline()
            } else {
                original::subline(black_box(&song))
            }));
        });
    }
}
