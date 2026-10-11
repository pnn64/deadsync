use super::names_original as original;
use super::*;
use crate::perf::measure;
use std::hint::black_box;

#[path = "../../../tests/support/paired_bench.rs"]
mod paired_bench;

#[test]
fn ascii_normalization_matches_every_two_byte_input() {
    for first in 0..128_u8 {
        for second in 0..128_u8 {
            let bytes = [first, second];
            let input = std::str::from_utf8(&bytes).unwrap();
            let old = original::normalize_name(input);
            let new = normalize_name(input);
            assert_eq!(new, old);
            assert_eq!(new.capacity(), old.capacity());
        }
    }
}

#[test]
fn unicode_normalization_keeps_per_character_casing_and_expansions() {
    for input in [
        "",
        "DEKW's double-deckered detritus",
        "ΟΣ ΣΟΣ",
        "İstanbul",
        "東京！１２",
        "Été Ω ꙮ",
        "\u{2003}A💃Z\u{2003}",
        "A\u{307}B",
        "\0\t?!",
    ] {
        assert_eq!(
            normalize_name(input),
            original::normalize_name(input),
            "{input:?}"
        );
    }
    assert_eq!(normalize_name("ΟΣ"), "οσ");
    assert_eq!(normalize_name("İ"), "i\u{307}");
    for code in (0..=0x10ffff).step_by(97) {
        if let Some(ch) = char::from_u32(code) {
            let input = format!("ASCII-{ch}-Mix42!");
            assert_eq!(normalize_name(&input), original::normalize_name(&input));
        }
    }
}

#[test]
fn dedicated_membership_and_allocation_churn_are_unchanged() {
    let names = [
        "DBK2 (Doubles Only)",
        "Double Raccoon",
        "東京",
        "ΟΣ",
        "İstanbul",
        "!!!",
    ];
    let snapshot = ItgdbSnapshot {
        dedicated: Arc::new(names.iter().map(|s| original::normalize_name(s)).collect()),
        ..ItgdbSnapshot::default()
    };
    for input in
        names
            .into_iter()
            .chain(["dbk2 doubles only", "DOUBLE RACCOON", "missing", "", "ος"])
    {
        assert_eq!(
            snapshot.is_dedicated(input),
            snapshot
                .dedicated
                .contains(&original::normalize_name(input))
        );
        let (old, before) = measure(|| original::normalize_name(input));
        let (new, after) = measure(|| normalize_name(input));
        assert_eq!(new, old);
        assert_eq!(new.capacity(), old.capacity());
        assert_eq!(before, after);
    }
}

#[test]
#[ignore = "paired release benchmark; run explicitly with --ignored --nocapture"]
fn benchmark_pack_name_normalization() {
    let mut cases = vec![
        ("empty".to_owned(), String::new()),
        ("lower".into(), "doublesraccoonvolume42".into()),
        (
            "ascii".into(),
            "DEKW's double-deckered detritus (Doubles Only)".into(),
        ),
        ("punctuation".into(), "--! ... / ? [] ( )".into()),
        ("greek".into(), "ΟΣ ΣΟΣ - Ελληνικά".into()),
        ("japanese".into(), "東京 音楽ゲーム １２３".into()),
        (
            "mixed".into(),
            "StepMania Pack - İstanbul & Été 2026".into(),
        ),
        (
            "late-unicode".into(),
            format!("{}Ω", "Ascii Pack ".repeat(400)),
        ),
    ];
    for (label, input) in cases.drain(..) {
        let (old, before) = measure(|| original::normalize_name(&input));
        let (new, after) = measure(|| normalize_name(&input));
        assert_eq!(new, old);
        assert_eq!(before, after);
        println!("normalize-{label} churn: original {before:?}, current {after:?}");
        paired_bench::compare(&format!("normalize-{label}"), 100, |current| {
            black_box(if current {
                normalize_name(black_box(&input))
            } else {
                original::normalize_name(black_box(&input))
            });
        });
    }
    for count in [1200, 9000] {
        let names: Vec<_> = (0..count)
            .map(|i| format!("Pack {i} - Dance (Doubles) Volume {}", i % 10))
            .collect();
        paired_bench::compare(&format!("normalize-catalog-{count}"), 10, |current| {
            for name in &names {
                black_box(if current {
                    normalize_name(black_box(name))
                } else {
                    original::normalize_name(black_box(name))
                });
            }
        });
    }
}
