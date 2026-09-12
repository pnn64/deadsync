use super::*;
use std::hint::black_box;

#[path = "speed_parse_baseline.rs"]
mod baseline;

fn assert_same(text: &str) {
    let bits =
        |value: Option<(&'static str, f32)>| value.map(|(key, value)| (key, value.to_bits()));
    assert_eq!(
        bits(parse_player_speed_option(text)),
        bits(baseline::parse_player_speed_option(text)),
        "{text:?}"
    );
}

#[test]
fn lua_work_speed_parser_preserves_case_whitespace_precedence_and_float_bits() {
    for number in [
        "0",
        "-0",
        "+0",
        "1.5",
        ".5",
        "1.",
        "-3e-2",
        "+3E+2",
        "NaN",
        "-NAN",
        "+nAn",
        "inf",
        "Infinity",
        "-INFINITY",
        "1e999",
        "1e-999",
        "",
        "1_0",
        "1.2.3",
        "１２",
    ] {
        for marker in [
            "x", "X", "ca", "CA", "Ca", "cA", "c", "C", "m", "M", "a", "A",
        ] {
            for text in [
                format!("{number}{marker}"),
                format!("{marker}{number}"),
                format!("{marker}{number}{marker}"),
            ] {
                assert_same(&text);
                assert_same(&format!("\u{2003}\t{text}\r\n"));
                let spaced = text
                    .chars()
                    .map(|c| format!("{c}\u{2009}"))
                    .collect::<String>();
                assert_same(&spaced);
            }
        }
    }
    for text in [
        "reverse",
        "50% drunk",
        "C 2 5 0",
        "C\u{00a0}A\u{2028}400",
        "xca2",
        "c2x",
        "2xc",
        "\0",
        "a\0",
        "💃C400",
        "cあ",
        "あc",
        "Ca",
        "X",
        " ",
        "",
    ] {
        assert_same(text);
    }
    for byte in 0..=127u8 {
        assert_same(&format!("C{}400", char::from(byte)));
    }
    for len in [63, 64, 65, 127, 128, 129, 1024] {
        assert_same(&format!("C{}1", "0 ".repeat(len)));
        assert_same(&format!("{}X", "9".repeat(len)));
        assert_same(&format!("{} X", "あ".repeat(len)));
    }
}

#[test]
fn lua_work_speed_parser_matches_generated_inputs_without_normalizing_ownership() {
    let alphabet = [
        "a", "C", "x", "M", "+", "-", "0", "5", ".", "e", "I", "n", "F", " ", "\t", "\u{2003}",
        "é", "💃", "\0",
    ];
    let mut seed = 7u64;
    for _ in 0..20_000 {
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
        let mut text = String::new();
        for _ in 0..(seed as usize % 24) {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
            text.push_str(alphabet[(seed >> 32) as usize % alphabet.len()]);
        }
        assert_same(&text);
    }
}

#[test]
fn lua_work_speed_parser_has_no_churn_for_common_or_long_unspaced_tokens() {
    let long = format!("{}x", "9".repeat(1024));
    crate::perf::assert_no_churn(|| {
        for text in [
            "1.5x",
            "C400",
            "NaNX",
            "-INFINITYM",
            "reverse",
            "50% drunk",
            "C\u{2003}A 4 0 0",
            "",
            long.as_str(),
        ] {
            black_box(parse_player_speed_option(black_box(text)));
        }
    });
}

#[test]
#[ignore = "manual paired release benchmark; run serially with --nocapture"]
fn lua_work_bench_speed() {
    let long = format!("{}x", "9".repeat(256));
    let spaced = format!("C{}1", "0 ".repeat(80));
    let unicode = format!("C{}1", "0\u{2003}".repeat(80));
    for (name, inputs) in [
        (
            "speed_common",
            vec![
                "1.5x", "C400", "650m", "CA250", "a300", "-0X", "+3E2C", "infx",
            ],
        ),
        (
            "speed_mods",
            vec![
                "reverse",
                "50% drunk",
                "no mines",
                "MoveX1",
                "*2 50% tiny",
                "nomines",
                "dark",
                "bumpy",
            ],
        ),
        (
            "speed_spaced",
            vec![
                " C 4 0 0 ",
                "1 . 5 x",
                "\u{2003}C\u{00a0}A\t250",
                " 6 5 0 M ",
            ],
        ),
        ("speed_empty", vec!["", " \t\r\n"]),
        ("speed_long_plain", vec![long.as_str()]),
        ("speed_long_spaced", vec![spaced.as_str()]),
        ("speed_long_unicode", vec![unicode.as_str()]),
    ] {
        for text in &inputs {
            assert_same(text);
        }
        let run = |old| {
            for _ in 0..16 {
                for text in &inputs {
                    black_box(if old {
                        baseline::parse_player_speed_option(black_box(text))
                    } else {
                        parse_player_speed_option(black_box(text))
                    });
                }
            }
        };
        let order = if std::env::var_os("DEADSYNC_PERF_REVERSE").is_some() {
            [false, true]
        } else {
            [true, false]
        };
        for old in order {
            crate::perf::measure_sampled(
                &format!("{name}/{}", if old { "old" } else { "new" }),
                512,
                16 * inputs.len(),
                || run(old),
            );
        }
    }
}
