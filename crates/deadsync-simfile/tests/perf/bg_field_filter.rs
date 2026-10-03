use super::*;
use crate::perf::{assert_no_churn, measure_sampled};
use std::hint::black_box;

mod baseline {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/perf/bg_field_filter/baseline.rs"
    ));
}

#[test]
fn background_field_filter_preserves_ascii_case_and_substring_rules() {
    let tokens = [
        "",
        ".",
        "i",
        "N",
        "x",
        "M",
        "l",
        ".ini",
        ".XML",
        "movie.mp4",
        "日本語",
        "é",
        "\n",
        "\\",
        "\0",
    ];
    for a in tokens {
        for b in tokens {
            for c in tokens {
                let field = format!("{a}{b}{c}");
                assert_eq!(
                    bgchange_field_rejects_non_media(&field),
                    baseline::bgchange_field_rejects_non_media(&field),
                    "{field:?}"
                );
            }
        }
    }
    for name in [
        ".ini",
        ".xml",
        ".iNi",
        ".XmL",
        ".initial",
        ".xmlsuffix",
        ".in",
        ".xm",
        "ini",
        "xml",
    ] {
        for width in [0, 1, 3, 15, 16, 31, 32, 63, 64, 127, 128, 1024] {
            for prefix in ["x", ".", "日本語/"] {
                let field = format!("{}{name}/tail", prefix.repeat(width));
                assert_eq!(
                    bgchange_field_rejects_non_media(&field),
                    baseline::bgchange_field_rejects_non_media(&field),
                    "{field:?}"
                );
            }
        }
    }
    for field in [
        ".ini",
        ".xml",
        "movie.mp4",
        "日本語/background.jpg",
        ".in",
        ".xm",
    ] {
        assert_no_churn(|| {
            black_box(bgchange_field_rejects_non_media(black_box(field)));
        });
    }
}

#[test]
#[ignore = "manual release CPU/allocation benchmark"]
fn bg_field_filter_benchmark() {
    let original = black_box(baseline::bgchange_field_rejects_non_media as fn(&str) -> bool);
    let current = black_box(bgchange_field_rejects_non_media as fn(&str) -> bool);
    for (name, field) in [
        ("empty", String::new()),
        ("short", "x".to_owned()),
        ("ini-early", ".ini".to_owned()),
        ("xml-early", ".XML".to_owned()),
        ("movie", "movie.mp4".to_owned()),
        ("image", "日本語/background.jpg".to_owned()),
        ("ini-path", "animations/settings.INI".to_owned()),
        ("xml-path", "animations/layers/default.XML".to_owned()),
        (
            "long-media",
            format!("{}movie.mp4", "日本語/directory/".repeat(24)),
        ),
        (
            "long-xml",
            format!("{}settings.xml", "日本語/directory/".repeat(24)),
        ),
        (
            "many-dots",
            format!("{}movie.mp4", "../directory/".repeat(24)),
        ),
    ] {
        assert_eq!(current(&field), original(&field));
        let run = |variant: &str, f: fn(&str) -> bool| {
            measure_sampled(
                &format!("bg-field-filter/{name}/{variant}"),
                65536,
                1,
                || f(black_box(&field)),
            );
        };
        if std::env::var_os("DEADSYNC_BENCH_NEW_FIRST").is_some() {
            run("current", current);
            run("original", original);
        } else {
            run("original", original);
            run("current", current);
        }
    }
}
