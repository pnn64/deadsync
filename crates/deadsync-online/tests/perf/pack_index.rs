use super::*;
use crate::perf::measure_sampled;
use std::hint::black_box;

mod baseline {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/perf/pack_index/baseline.rs"
    ));
}

#[test]
fn bulk_ascii_pack_index_preserves_unicode_expansions_boundaries_and_search_ranking() {
    for name in [
        "",
        "lowercase",
        "PACK Name",
        "ΟΣ ΣΣ",
        "İÉ日本語",
        "ẞİΣ",
        "Straße 🦀",
        "A\0B",
    ] {
        for metadata in [
            [None; 4],
            [Some("ITG"), Some("Stamina"), Some("Technical"), Some("5.1")],
            [Some("İΟΣ"), None, Some("日本語 MIX"), Some("")],
        ] {
            for id in [0, 1, u64::MAX] {
                let old = baseline::pack_search_index(id, name, metadata);
                let new = pack_search_index(id, name, metadata);
                assert_eq!(old, new);
                assert_eq!(old.1.capacity(), new.1.capacity());
                assert_eq!(
                    &new.1[..new.0],
                    name.chars()
                        .flat_map(char::to_lowercase)
                        .collect::<String>()
                );
            }
        }
    }
    // Cover every Unicode scalar individually, including case expansions and Greek sigma.
    let text: String = (0..=0x10ffff).filter_map(char::from_u32).collect();
    assert_eq!(
        baseline::pack_search_index(1, &text, [None; 4]),
        pack_search_index(1, &text, [None; 4])
    );
    let lines = [
        "1, \"Pack Name\", 10, 1024, ITG, Stamina, Technical, 5.1",
        "2, \"ΟΣ İ 日本語\", 1, 0, none, null, , ",
        "3, \"lowercase pack\", 1, 0, Sync, Type, Substyle, Version",
    ];
    let old: Vec<_> = lines
        .iter()
        .map(|line| baseline::parse_catalog_line(line, 2).unwrap())
        .collect();
    let new: Vec<_> = lines
        .iter()
        .map(|line| parse_catalog_line(line, 2).unwrap())
        .collect();
    assert_eq!(old, new);
    for query in [
        "",
        "PACK",
        "name",
        "stamina technical",
        "1",
        "İ",
        "ΟΣ",
        "日本語",
        "version",
        "missing",
    ] {
        assert_eq!(search_catalog(&old, query), search_catalog(&new, query));
    }
    for line in [
        "",
        "1, \"\", 1, 0, none, null, , ",
        "not-an-id, \"Pack\", 1, 0, none, null, , ",
    ] {
        assert_eq!(
            baseline::parse_catalog_line(line, 2)
                .unwrap_err()
                .to_string(),
            parse_catalog_line(line, 2).unwrap_err().to_string()
        );
    }
}

#[test]
#[ignore = "manual release CPU/allocation benchmark"]
fn pack_index_benchmark() {
    let old = black_box(
        baseline::pack_search_index as fn(u64, &str, [Option<&str>; 4]) -> (usize, String),
    );
    let new = black_box(pack_search_index as fn(u64, &str, [Option<&str>; 4]) -> (usize, String));
    for (name, text, metadata) in [
        ("empty", String::new(), [None; 4]),
        (
            "ascii-lower",
            "stamina rpg ten faction east".to_owned(),
            [None; 4],
        ),
        (
            "ascii-upper",
            "Stamina RPG Ten Faction East".to_owned(),
            [Some("ITG"), Some("Stamina"), Some("Technical"), Some("5.1")],
        ),
        (
            "long-ascii",
            "Pack Name ".repeat(128),
            [Some("ITG"), Some("Stamina"), Some("Technical"), Some("5.1")],
        ),
        (
            "unicode",
            "ΟΣ İ 日本語 Straße".to_owned(),
            [Some("İΣ"), Some("日本語"), None, None],
        ),
        (
            "long-unicode",
            "ΟΣ İ 日本語 Straße".repeat(128),
            [Some("İΣ"), Some("日本語"), None, None],
        ),
    ] {
        let original = || {
            measure_sampled(&format!("pack-index/{name}/original"), 8192, 1, || {
                old(black_box(1234), black_box(&text), black_box(metadata))
            })
        };
        let current = || {
            measure_sampled(&format!("pack-index/{name}/current"), 8192, 1, || {
                new(black_box(1234), black_box(&text), black_box(metadata))
            })
        };
        if std::env::var_os("DEADSYNC_BENCH_NEW_FIRST").is_some() {
            current();
            original();
        } else {
            original();
            current();
        }
    }
    let old = black_box(
        baseline::parse_catalog_line as fn(&str, usize) -> Result<PackInfo, StepManiaOnlineError>,
    );
    let new =
        black_box(parse_catalog_line as fn(&str, usize) -> Result<PackInfo, StepManiaOnlineError>);
    for (name, line) in [
        (
            "row-ascii",
            "1, \"Stamina RPG Ten Faction East\", 10, 1024, ITG, Stamina, Technical, 5.1",
        ),
        (
            "row-unicode",
            "1, \"ΟΣ İ 日本語 Straße\", 10, 1024, ITG, Stamina, Technical, 5.1",
        ),
    ] {
        let original = || {
            measure_sampled(&format!("pack-index/{name}/original"), 8192, 1, || {
                old(black_box(line), black_box(2))
            })
        };
        let current = || {
            measure_sampled(&format!("pack-index/{name}/current"), 8192, 1, || {
                new(black_box(line), black_box(2))
            })
        };
        if std::env::var_os("DEADSYNC_BENCH_NEW_FIRST").is_some() {
            current();
            original();
        } else {
            original();
            current();
        }
    }
}
