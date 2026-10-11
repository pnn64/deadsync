use super::*;
use crate::{paired_bench, perf};
use std::hint::black_box;

#[path = "pack_meter_original.rs"]
mod original;

#[test]
fn meter_prefixes_match_the_original_without_allocation() {
    let prefixes = ["", "0", "1", "04", "4294967295", "4294967296", "+3", "-2"];
    let whitespace = ["", " \t\r\n", "\u{85}", "\u{a0}", "\u{2003}", "\u{3000}"];
    let suffixes = ["", ", 6, 9", "-12", "\u{e9}", "\u{ff11}", "\u{1f3b5}"];
    for prefix in prefixes {
        for space in whitespace {
            for suffix in suffixes {
                let meters = format!("{space}{prefix}{suffix}");
                let row = SongRow {
                    meters,
                    ..SongRow::default()
                };
                let expected = original::low_meter(&row);
                perf::assert_no_churn(|| assert_eq!(row.low_meter(), expected, "{}", row.meters));
            }
        }
    }
    for byte in 0..=127_u8 {
        for prefix in ["", "12"] {
            let meters = format!("{prefix}{}123", char::from(byte));
            let row = SongRow {
                meters,
                ..SongRow::default()
            };
            assert_eq!(row.low_meter(), original::low_meter(&row));
        }
    }
    for meters in ["9".repeat(4096), format!("{}4, 8", "0".repeat(4096))] {
        let row = SongRow {
            meters,
            ..SongRow::default()
        };
        assert_eq!(row.low_meter(), original::low_meter(&row));
    }
    for (meters, expected) in [
        ("", None),
        ("\u{2003}4-12", Some(4)),
        ("4294967295", Some(u32::MAX)),
        ("4294967296", None),
        ("+3", None),
    ] {
        assert_eq!(
            SongRow {
                meters: meters.to_owned(),
                ..SongRow::default()
            }
            .low_meter(),
            expected
        );
    }
}

#[test]
fn beginner_majorities_match_with_missing_and_invalid_meters() {
    let meters = ["", "1, 8", "4-12", "5", "0", "4294967296", "\u{a0}3", "-1"];
    for a in meters {
        for b in meters {
            for c in meters {
                let page = PackPage {
                    songs: [a, b, c]
                        .map(|meters| SongRow {
                            meters: meters.to_owned(),
                            ..SongRow::default()
                        })
                        .to_vec(),
                    ..PackPage::default()
                };
                let expected = original::is_beginner_friendly(&page);
                perf::assert_no_churn(|| assert_eq!(page.is_beginner_friendly(), expected));
            }
        }
    }
}

#[test]
#[ignore = "paired release benchmark"]
fn benchmark_meter_prefixes_and_beginner_pages() {
    for (label, meters) in [
        ("single", "4"),
        ("range", "3, 6, 9, 12"),
        ("unicode-space", "\u{2003}4-12"),
        ("overflow", "4294967296"),
        ("empty", ""),
        ("invalid", "unknown"),
    ] {
        let row = SongRow {
            meters: meters.to_owned(),
            ..SongRow::default()
        };
        let (_, before) = perf::measure(|| black_box(original::low_meter(&row)));
        let (_, after) = perf::measure(|| black_box(row.low_meter()));
        println!("meter/{label} churn: {before:?} -> {after:?}");
        paired_bench::compare(&format!("meter/{label}"), 100_000, |current| {
            black_box(if current {
                black_box(&row).low_meter()
            } else {
                original::low_meter(black_box(&row))
            });
        });
    }
    for count in [7, 200, 1000] {
        let page = PackPage {
            songs: (0..count)
                .map(|i| SongRow {
                    meters: [
                        "1, 4, 8",
                        "3-12",
                        "\u{a0}4, 9",
                        "9, 12",
                        "",
                        "4294967296",
                        "2",
                    ][i % 7]
                        .to_owned(),
                    ..SongRow::default()
                })
                .collect(),
            ..PackPage::default()
        };
        let (_, before) = perf::measure(|| black_box(original::is_beginner_friendly(&page)));
        let (_, after) = perf::measure(|| black_box(page.is_beginner_friendly()));
        println!("meter/page-{count} churn: {before:?} -> {after:?}");
        paired_bench::compare(&format!("meter/page-{count}"), 100_000 / count, |current| {
            black_box(if current {
                black_box(&page).is_beginner_friendly()
            } else {
                original::is_beginner_friendly(black_box(&page))
            });
        });
    }
}
