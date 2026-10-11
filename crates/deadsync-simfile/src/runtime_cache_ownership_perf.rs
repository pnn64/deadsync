use super::*;
use crate::metadata_perf::{compare, measure};
use std::hint::black_box;
mod original {
    use super::*;
    include!("runtime_cache_original.rs");
}

fn packs(count: usize, unicode: bool) -> Vec<SongPack> {
    (0..count)
        .map(|i| {
            let name = if i % 13 == 0 {
                "TARGET".to_owned()
            } else if unicode {
                format!("\u{00c9}t\u{00e9} Pack {i}")
            } else {
                format!("Ordinary Pack {i}")
            };
            pack(&name, SyncPref::Itg, Vec::new())
        })
        .collect()
}

#[test]
fn sync_updates_preserve_unicode_lowercase_and_all_matching_packs() {
    let names = [
        "Target",
        "TARGET",
        " target ",
        "K",
        "k",
        "\u{212a}",
        "\u{0130}",
        "i\u{0307}",
        "\u{039f}\u{03a3}",
        "\u{03bf}\u{03c2}",
        "\u{03bf}\u{03c3}",
        "\u{00c9}t\u{00e9}",
        "\u{00e9}t\u{00e9}",
        "",
    ];
    for wanted in names.iter().copied().chain(["missing", "TaRgEt"]) {
        let mut expected: Vec<_> = names
            .iter()
            .map(|name| pack(name, SyncPref::Itg, Vec::new()))
            .collect();
        let mut actual = expected.clone();
        for pref in [
            SyncPref::Null,
            SyncPref::Null,
            SyncPref::Default,
            SyncPref::Itg,
        ] {
            let old = original::set_sync_pref_in_packs(&mut expected, wanted, pref);
            let new = set_sync_pref_in_packs(&mut actual, wanted, pref);
            assert_eq!(old, new, "wanted: {wanted:?}");
            assert_eq!(format!("{actual:?}"), format!("{expected:?}"));
        }
    }
    let mut values = [
        pack("K", SyncPref::Itg, Vec::new()),
        pack("\u{03bf}\u{03c3}", SyncPref::Itg, Vec::new()),
    ];
    assert!(set_sync_pref_in_packs(
        &mut values,
        "\u{212a}",
        SyncPref::Null
    ));
    assert_eq!(values[0].sync_pref, SyncPref::Null);
    assert!(!set_sync_pref_in_packs(
        &mut values,
        "\u{039f}\u{03a3}",
        SyncPref::Null
    ));
    assert!(!set_sync_pref_in_packs(&mut [], "target", SyncPref::Null));
}

#[test]
fn ascii_sync_updates_allocate_only_the_search_name() {
    let mut old = packs(1024, false);
    let mut new = old.clone();
    let (_, a) = measure(|| original::set_sync_pref_in_packs(&mut old, "target", SyncPref::Null));
    let (_, b) = measure(|| set_sync_pref_in_packs(&mut new, "target", SyncPref::Null));
    assert_eq!(a.allocs, 1025);
    assert_eq!(b.allocs, 1);
    assert_eq!(b.allocated_bytes, "target".len());
}

#[test]
#[ignore = "paired release benchmark; run alone with --nocapture"]
fn benchmark_metadata_sync() {
    for count in [0, 1, 32, 1024, 8192] {
        for unicode in [false, true] {
            let mut old = packs(count, unicode);
            let mut new = old.clone();
            let label = format!("sync/{count}/{unicode}");
            let (_, a) =
                measure(|| original::set_sync_pref_in_packs(&mut old, "target", SyncPref::Null));
            let (_, b) = measure(|| set_sync_pref_in_packs(&mut new, "target", SyncPref::Null));
            println!("ALLOC {label}: original {a:?}, current {b:?}");
            compare(
                &label,
                16,
                || {
                    black_box(original::set_sync_pref_in_packs(
                        black_box(&mut old),
                        black_box("target"),
                        SyncPref::Null,
                    ));
                },
                || {
                    black_box(set_sync_pref_in_packs(
                        black_box(&mut new),
                        black_box("target"),
                        SyncPref::Null,
                    ));
                },
            );
        }
    }
}
