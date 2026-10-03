use super::*;
use crate::perf::{assert_no_churn, assert_reduced_churn, measure_sampled};
use std::hint::black_box;

mod baseline {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/perf/shop_effects/baseline.rs"
    ));
}

fn current_clean(text: &str) -> String {
    clean_cell(text).into_owned()
}
fn current_effect(text: &str) -> String {
    clean_cell(text).replace('|', "  •  ")
}
fn original_effect(text: &str) -> String {
    baseline::clean_cell(text).replace('|', "  •  ")
}

#[test]
fn borrowed_cell_cleanup_preserves_html_entities_whitespace_and_effects() {
    let tokens = [
        "",
        " ",
        "  ",
        "\t",
        "\n",
        "\r",
        "\u{a0}",
        "\u{85}",
        "\u{2003}",
        "A",
        "É日本",
        "|",
        "&amp;",
        "&amp;lt;",
        "&amp;gt;",
        "&lt;",
        "&gt;",
        "&apos;",
        "&quot;",
        "&unknown;",
        "<",
        ">",
        "<b>",
        "</b>",
    ];
    for a in tokens {
        for b in tokens {
            for c in tokens {
                let text = format!("{a}{b}{c}");
                let old = baseline::clean_cell(&text);
                let new = current_clean(&text);
                assert_eq!(old, new, "{text:?}");
                assert!(new.capacity() <= old.capacity(), "{text:?}");
                let old = original_effect(&text);
                let new = current_effect(&text);
                assert_eq!(old, new, "effect {text:?}");
                assert_eq!(old.capacity(), new.capacity(), "effect {text:?}");
            }
        }
    }
    for text in [
        "",
        "Already clean",
        "日本語 É",
        "Difficulty: 14|Speed Tier: 180 BPM",
    ] {
        assert!(matches!(clean_cell(text), Cow::Borrowed(_)));
        assert_no_churn(|| {
            black_box(clean_cell(black_box(text)));
        });
    }
}

fn catalog_body(count: usize, effect: &str, html: bool) -> String {
    let rows: Vec<Value> = (0..count)
        .map(|i| {
            serde_json::json!([
                i.to_string(),
                "chart.png",
                if html {
                    "<b>Song 日本語</b>"
                } else {
                    "Song 日本語"
                },
                "Purchase to unlock",
                effect,
                "2",
                "0",
                "1,234",
                i,
                "0",
                "0",
                "1",
                "14",
                "180",
                "0"
            ])
        })
        .collect();
    serde_json::json!({"data": rows}).to_string()
}

#[test]
fn borrowed_shop_cells_preserve_complete_catalog_rows_and_ownership() {
    for count in [0, 1, 16, 128] {
        for effect in [
            "",
            "Lv. 1 EP",
            "Difficulty: 14|Speed Tier: 180 BPM",
            "<b>A| B</b>&amp;",
            "  A\t|\u{a0}B  ",
        ] {
            for html in [false, true] {
                let text = catalog_body(count, effect, html);
                for shop in [0, 2] {
                    for balance in [0, 1234, u64::MAX] {
                        let old = baseline::parse_catalog(&text, shop, balance).unwrap();
                        let new = parse_catalog(&text, shop, balance).unwrap();
                        assert_eq!(old, new);
                        for (old, new) in old.iter().zip(&new) {
                            assert_eq!(old.name.capacity(), new.name.capacity());
                            assert_eq!(old.description.capacity(), new.description.capacity());
                            assert_eq!(old.effect.capacity(), new.effect.capacity());
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn borrowed_shop_cells_remove_effect_temporary_allocations() {
    for text in [
        "Lv. 1 EP",
        "Difficulty: 14|Speed Tier: 180 BPM",
        "日本語 É| Mix",
    ] {
        assert_reduced_churn(
            || drop(black_box(original_effect(black_box(text)))),
            || drop(black_box(current_effect(black_box(text)))),
        );
    }
    let text = catalog_body(128, "Difficulty: 14|Speed Tier: 180 BPM", false);
    assert_reduced_churn(
        || {
            drop(black_box(
                baseline::parse_catalog(black_box(&text), 0, 0).unwrap(),
            ))
        },
        || drop(black_box(parse_catalog(black_box(&text), 0, 0).unwrap())),
    );
}

#[test]
#[ignore = "manual release CPU/allocation benchmark"]
fn shop_effects_benchmark() {
    for (name, text) in [
        ("empty", String::new()),
        ("plain", "Already clean shop item".into()),
        ("pipe", "Difficulty: 14|Speed Tier: 180 BPM".into()),
        (
            "html",
            "<b>Difficulty: 14</b>|<span>Speed Tier: 180 BPM</span>".into(),
        ),
        (
            "entities",
            "&lt;b&gt;Song&amp;Name&lt;/b&gt; &quot;Mix&quot;".into(),
        ),
        (
            "whitespace",
            "  Difficulty:\t14\n|\u{a0} Speed Tier: 180 BPM  ".into(),
        ),
        ("unicode", "日本語 É Déjà Vu| Mix".into()),
        (
            "late-html",
            format!("{}<br>Mix", "Already clean item ".repeat(64)),
        ),
        (
            "late-space",
            format!("{}  Mix", "Already clean item ".repeat(64).trim_end()),
        ),
        (
            "long",
            format!("{}|Mix", "Already clean item ".repeat(128).trim_end()),
        ),
    ] {
        let run = |variant, clean: fn(&str) -> String, effect: fn(&str) -> String| {
            measure_sampled(
                &format!("shop-cells/clean-{name}/{variant}"),
                16384,
                1,
                || clean(black_box(&text)),
            );
            measure_sampled(
                &format!("shop-cells/effect-{name}/{variant}"),
                16384,
                1,
                || effect(black_box(&text)),
            );
        };
        let old = (
            black_box(baseline::clean_cell as fn(&str) -> String),
            black_box(original_effect as fn(&str) -> String),
        );
        let new = (
            black_box(current_clean as fn(&str) -> String),
            black_box(current_effect as fn(&str) -> String),
        );
        if std::env::var_os("DEADSYNC_BENCH_NEW_FIRST").is_some() {
            run("current", new.0, new.1);
            run("original", old.0, old.1);
        } else {
            run("original", old.0, old.1);
            run("current", new.0, new.1);
        }
    }
    for (name, text) in [
        (
            "catalog-plain",
            catalog_body(128, "Difficulty: 14|Speed Tier: 180 BPM", false),
        ),
        (
            "catalog-html",
            catalog_body(
                128,
                "<b>Difficulty: 14</b>|<span>Speed Tier: 180 BPM</span>",
                true,
            ),
        ),
        (
            "catalog-censored",
            catalog_body(128, "Difficulty: 14|Speed Tier: 180 BPM", false),
        ),
    ] {
        let shop = if name == "catalog-censored" { 2 } else { 0 };
        let run = |variant, f: fn(&str, u32, u64) -> Result<Vec<SrpgShopItem>, SrpgShopError>| {
            measure_sampled(&format!("shop-cells/{name}/{variant}"), 64, 128, || {
                f(black_box(&text), black_box(shop), 0).unwrap()
            });
        };
        let old = black_box(
            baseline::parse_catalog
                as fn(&str, u32, u64) -> Result<Vec<SrpgShopItem>, SrpgShopError>,
        );
        let new = black_box(
            parse_catalog as fn(&str, u32, u64) -> Result<Vec<SrpgShopItem>, SrpgShopError>,
        );
        if std::env::var_os("DEADSYNC_BENCH_NEW_FIRST").is_some() {
            run("current", new);
            run("original", old);
        } else {
            run("original", old);
            run("current", new);
        }
    }
}
