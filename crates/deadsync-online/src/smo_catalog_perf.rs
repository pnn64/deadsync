use super::*;
use crate::perf;
use std::hint::black_box;

#[path = "smo_catalog_original.rs"]
mod original;
#[path = "../../../tests/support/paired_bench.rs"]
mod paired;

fn catalog(count: usize) -> Vec<stepmaniaonline::PackInfo> {
    const NAMES: [&str; 8] = [
        "Pack Stars",
        "The Pack Stars",
        "Dance Singles",
        "Doubles Pack",
        "Tech Soup",
        "\u{39f}\u{3a3} Pack",
        "\u{130}stanbul pack",
        "Caf\u{e9} Selection",
    ];
    (0..count)
        .map(|id| {
            stepmaniaonline::PackInfo::new(
                id as u64,
                format!("{} {id}", NAMES[id % NAMES.len()]),
                12,
                1000,
                None,
                None,
                None,
                None,
            )
        })
        .collect()
}

#[test]
fn catalogue_name_pass_preserves_order_scores_and_unicode() {
    for count in [0, 1, 8, 200, 1200] {
        let catalog = catalog(count);
        for query in [
            "",
            "pa",
            "PACK",
            "soup",
            "stars",
            "unmatched",
            "\u{39f}\u{3a3}",
            "\u{3bf}\u{3c3}",
            "\u{130}",
            "i\u{307}",
            "caf\u{e9}",
        ] {
            let needle = query.to_lowercase();
            let old = original::from_catalog(&catalog, &needle);
            let new = Accumulator::from_catalog(&catalog, &needle);
            assert_eq!(new.hits, old.hits, "{count}: {query:?}");
            assert_eq!(new.capped, old.capped);
        }
    }
    let greek = catalog(8);
    let acc = Accumulator::from_catalog(&greek, "\u{3bf}\u{3c2}");
    assert_eq!(
        acc.hits[0].pack_id, 5,
        "keep context-sensitive final sigma lowercasing"
    );
}

#[test]
fn later_credit_and_title_hits_still_merge_and_nudge() {
    let catalog = catalog(500);
    let mut old = original::from_catalog(&catalog, "pack");
    let mut new = Accumulator::from_catalog(&catalog, "pack");
    for id in [0, 2, 8, 500, 0, 8, 2, 900] {
        for points in [score::TITLE, score::CREDIT, 160, score::NAME_PREFIX] {
            let why = format!("reason {id}: {points}");
            old.add(id, points, why.clone());
            new.add(id, points, why);
            assert_eq!(new.hits, old.hits);
            assert_eq!(new.capped, old.capped);
        }
    }
}

#[test]
fn parsed_catalogue_enforces_the_unique_id_invariant() {
    let header = "ID, Pack Name, Song Count, Size, Sync, PackType, Substyle, Min Version\n";
    let row = "1, \"Pack\", 1, 10, None, None, None, None\n";
    let one = stepmaniaonline::parse_catalog(&format!("{header}{row}")).unwrap();
    assert_eq!(
        Accumulator::from_catalog(&one, "pa").hits,
        original::from_catalog(&one, "pa").hits
    );
    let duplicate = stepmaniaonline::parse_catalog(&format!("{header}{row}{row}")).unwrap_err();
    assert!(duplicate.to_string().contains("duplicate pack ID"));
}

#[test]
#[ignore = "paired release benchmark"]
fn benchmark_catalogue_name_pass() {
    for count in [0, 200, 1200, 9000] {
        let catalog = catalog(count);
        for needle in ["unmatched", "soup", "pack"] {
            let label = format!("catalogue/{count}/{needle}");
            let (old, before) = perf::measure(|| original::from_catalog(&catalog, needle));
            let (new, after) = perf::measure(|| Accumulator::from_catalog(&catalog, needle));
            assert_eq!(old.hits, new.hits);
            assert_eq!(before, after, "same retained results, no additional index");
            println!("{label}: churn original {before:?}, current {after:?}");
            paired::compare(&label, 10, |current| {
                drop(black_box(if current {
                    Accumulator::from_catalog(black_box(&catalog), black_box(needle))
                } else {
                    original::from_catalog(black_box(&catalog), black_box(needle))
                }));
            });
        }
    }
}
