use deadsync_score::leaderboard::{
    format_scorebox_rank, format_scorebox_score_percent, format_scorebox_score_value,
    leaderboard_username_matches, scorebox_machine_tag,
};
use std::hint::black_box;

#[path = "../../../tests/support/kernel_bench.rs"]
mod bench;
#[path = "support/scorebox_text_original.rs"]
mod original;
#[allow(dead_code)]
#[path = "../../../tests/support/perf.rs"]
mod perf;

fn check_score(raw: f64) {
    assert_eq!(
        format_scorebox_score_value(raw),
        original::format_scorebox_score_value(raw),
        "value bits {:016x}",
        raw.to_bits()
    );
    assert_eq!(
        format_scorebox_score_percent(raw),
        original::format_scorebox_score_percent(raw),
        "percent bits {:016x}",
        raw.to_bits()
    );
}

#[test]
fn score_text_preserves_half_cent_neighbors_signed_zero_and_ieee_classes() {
    for raw in [
        0.0,
        -0.0,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::MAX,
        -f64::MAX,
        f64::MIN_POSITIVE,
        -f64::MIN_POSITIVE,
        f64::from_bits(1),
        -f64::from_bits(1),
        10000.0,
        10001.0,
        0.5,
        12.5,
        37.5,
        62.5,
        87.5,
    ] {
        check_score(raw);
    }
    for sign in [0, 1u64 << 63] {
        for exponent in 0..=2047 {
            for mantissa in [0, 1, (1 << 51) - 1, 1 << 51, (1 << 52) - 1] {
                check_score(f64::from_bits(sign | exponent << 52 | mantissa));
            }
        }
    }
    for units in 0..=10001 {
        for raw in [units as f64, units as f64 + 0.5] {
            let bits = raw.to_bits();
            for delta in -8i64..=8 {
                if let Some(bits) = bits.checked_add_signed(delta) {
                    check_score(f64::from_bits(bits));
                }
            }
        }
    }
    let mut state = 0x243f_6a88_85a3_08d3u64;
    for index in 0..1_000_000 {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        let raw = if index % 2 == 0 {
            f64::from_bits(state)
        } else {
            ((state >> 11) as f64 / (1u64 << 53) as f64) * 12000.0 - 1000.0
        };
        check_score(raw);
    }
}

#[test]
fn rank_text_preserves_decimal_boundaries_and_entire_small_domain() {
    let check = |rank| {
        assert_eq!(
            format_scorebox_rank(rank),
            original::format_scorebox_rank(rank),
            "{rank}"
        )
    };
    for rank in 0..1_000_000 {
        check(rank);
    }
    for power in [
        1u32, 10, 100, 1000, 10000, 100000, 1000000, 10000000, 100000000, 1000000000,
    ] {
        for delta in -3i32..=3 {
            if let Some(rank) = power.checked_add_signed(delta) {
                check(rank);
            }
        }
    }
    let mut state = 0x9e37_79b9u32;
    for _ in 0..100_000 {
        state ^= state << 13;
        state ^= state >> 17;
        state ^= state << 5;
        check(state);
    }
    for rank in (u32::MAX - 1024)..=u32::MAX {
        check(rank);
    }
}

fn names(kind: usize) -> Vec<(String, String)> {
    (0..512)
        .map(|i| match kind {
            0 => (format!("OtherUser{i:03}"), "PerfectTaste".to_owned()),
            1 => (
                ["PerfectTaste", "PERFECTTASTE", "perfecttaste"][i % 3].to_owned(),
                "PerfectTaste".to_owned(),
            ),
            2 => (
                "OtherUser000".to_owned(),
                ["", " ", "\u{2003}\u{a0}"][i % 3].to_owned(),
            ),
            _ => {
                let (a, b) = [
                    ("\u{e9}", "\u{e9}"),
                    ("\u{e9}", "\u{c9}"),
                    ("\u{6771}\u{4eac}", "\u{6771}\u{4eac}"),
                    ("\u{212a}", "K"),
                ][i % 4];
                (a.to_owned(), b.to_owned())
            }
        })
        .collect()
}

#[test]
fn row_text_and_name_search_preserve_complete_values_and_positions() {
    let values = [
        "",
        " \t",
        "\u{2003}\u{a0}",
        "Name",
        "NAME",
        " name ",
        "\u{e9}",
        "\u{c9}",
        "\u{212a}",
        "K",
        "\0",
    ];
    for name in values {
        for user in values {
            assert_eq!(
                leaderboard_username_matches(name, user),
                original::leaderboard_username_matches(name, user),
                "{name:?} / {user:?}"
            );
        }
    }
    for byte in 0..=127 {
        for index in 0..=12 {
            let base = "PerfectTaste";
            let name = format!("{}{}{}", &base[..index], char::from(byte), &base[index..]);
            assert_eq!(
                leaderboard_username_matches(&name, base),
                original::leaderboard_username_matches(&name, base)
            );
            assert_eq!(
                leaderboard_username_matches(base, &name),
                original::leaderboard_username_matches(base, &name)
            );
        }
    }
    let names = names(0);
    for user in [
        "",
        " ",
        "PerfectTaste",
        "OtherUser000",
        "otheruser511",
        "\u{2003}",
    ] {
        assert_eq!(
            names
                .iter()
                .position(|(name, _)| leaderboard_username_matches(name, user)),
            names
                .iter()
                .position(|(name, _)| original::leaderboard_username_matches(name, user))
        );
    }
    for i in 0..512 {
        let raw = (i as f64 - 32.0) * 25.5;
        let name = &names[i].0;
        assert_eq!(
            (
                format_scorebox_rank(i as u32),
                format_scorebox_score_value(raw),
                format_scorebox_score_percent(raw),
                leaderboard_username_matches(name, "OtherUser123")
            ),
            (
                original::format_scorebox_rank(i as u32),
                original::format_scorebox_score_value(raw),
                original::format_scorebox_score_percent(raw),
                original::leaderboard_username_matches(name, "OtherUser123")
            )
        );
    }
}

fn measure(label: &str, mut work: impl FnMut()) {
    let (_, counts) = perf::measure(&mut work);
    println!("{label}: allocation {counts:?}");
    bench::measure(label, 128, work);
}

#[test]
#[ignore = "release CPU and allocation benchmark"]
fn benchmark_scorebox_text() {
    let common: Vec<_> = (0..512).map(|i| 9876.0 + (i % 100) as f64 * 0.5).collect();
    let special: Vec<_> = (0..512)
        .map(|i| {
            [
                0.0,
                -0.0,
                f64::NAN,
                f64::INFINITY,
                f64::NEG_INFINITY,
                -1.0,
                10000.0,
                10001.0,
                f64::from_bits(1),
                -f64::from_bits(1),
            ][i % 10]
        })
        .collect();
    let boundary: Vec<_> = (0..512)
        .map(|i| {
            f64::from_bits(
                (i as f64 + 0.5)
                    .to_bits()
                    .wrapping_add((i % 3) as u64)
                    .wrapping_sub(1),
            )
        })
        .collect();
    for (label, values, percent) in [
        ("value/common", &common, false),
        ("percent/common", &common, true),
        ("value/special", &special, false),
        ("percent/special", &special, true),
        ("value/boundary", &boundary, false),
    ] {
        measure(label, || {
            for &raw in black_box(values) {
                black_box(if percent {
                    format_scorebox_score_percent(black_box(raw))
                } else {
                    format_scorebox_score_value(black_box(raw))
                });
            }
        });
    }
    let small: Vec<_> = (0..512).collect();
    let large: Vec<_> = (0..512).map(|i| u32::MAX - i).collect();
    let mixed: Vec<_> = (0..512)
        .map(|i| [0, 1, 9, 10, 99, 100, 1000, 1000000, u32::MAX][i % 9])
        .collect();
    for (label, values) in [
        ("rank/small", &small),
        ("rank/large", &large),
        ("rank/mixed", &mixed),
    ] {
        measure(label, || {
            for &rank in black_box(values) {
                black_box(format_scorebox_rank(black_box(rank)));
            }
        });
    }
    for (label, kind) in [
        ("name/miss", 0),
        ("name/match", 1),
        ("name/blank", 2),
        ("name/unicode", 3),
    ] {
        let values = names(kind);
        measure(label, || {
            for (name, user) in black_box(&values) {
                black_box(leaderboard_username_matches(
                    black_box(name),
                    black_box(user),
                ));
            }
        });
    }
    let mut candidates: Vec<_> = (0..128).map(|i| format!("OtherUser{i:03}")).collect();
    candidates[127] = "PerfectTaste".to_owned();
    for (label, user) in [("find/late", "perfecttaste"), ("find/miss", "MissingUser")] {
        measure(label, || {
            black_box(
                black_box(&candidates).iter().position(|name| {
                    leaderboard_username_matches(black_box(name), black_box(user))
                }),
            );
        });
    }
    let users = names(0);
    measure("compose/row", || {
        for i in 0..128 {
            let raw = black_box(common[i]);
            let score = if i % 2 == 0 {
                format_scorebox_score_percent(raw)
            } else {
                format_scorebox_score_value(raw)
            };
            black_box((
                format_scorebox_rank(black_box(i as u32 + 1)),
                score,
                leaderboard_username_matches(black_box(&users[i].0), black_box("OtherUser127")),
            ));
        }
    });
    measure("control/tag", || {
        for _ in 0..512 {
            black_box(scorebox_machine_tag(
                black_box(None),
                black_box("\u{e9}\u{96ea}\u{1f600}\u{df}tail"),
            ));
        }
    });
}
