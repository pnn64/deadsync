use super::*;
use crate::{paired_bench, perf};
use std::hint::black_box;

mod original {
    use super::*;
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/support/perf_1870_popular.rs"
    ));
}

fn entry(mask: u8, empty: bool) -> Entry {
    Entry {
        name: if empty {
            String::new()
        } else {
            "Pack \u{97f3}\u{697d}".into()
        },
        popularity: 8011.25,
        simfile_count: 310,
        banner_url: (mask & 1 != 0).then(|| "https://a.test/full.png".into()),
        md_banner_url: (mask & 2 != 0).then(|| "https://a.test/md.png".into()),
        sm_banner_url: (mask & 4 != 0).then(|| {
            if empty {
                String::new()
            } else {
                "https://a.test/sm.png".into()
            }
        }),
    }
}

#[test]
fn owned_ranking_conversion_preserves_banner_precedence_and_string_storage() {
    for empty in [false, true] {
        for mask in 0..8 {
            let source = entry(mask, empty);
            let name_pointer = source.name.as_ptr();
            let banner_pointer = source
                .sm_banner_url
                .as_ref()
                .or(source.md_banner_url.as_ref())
                .or(source.banner_url.as_ref())
                .map(|value| value.as_ptr());
            let expected = original::into_pack(entry(mask, empty));
            let (actual, churn) = perf::measure(|| source.into_pack());
            assert_eq!(actual, expected, "mask={mask}, empty={empty}");
            assert_eq!(actual.name.as_ptr(), name_pointer);
            assert_eq!(
                actual.banner_url.as_ref().map(|value| value.as_ptr()),
                banner_pointer
            );
            assert_eq!(churn.allocs, 0);
            assert_eq!(churn.reallocs, 0);
        }
    }
}

#[test]
fn parsed_ranking_pages_keep_defaults_and_ignore_stale_banner_variants() {
    let json = r#"{"data":[
        {"name":"full","bannerUrl":"full","bannerVariants":{"sm":[{"url":"stale"}]}},
        {"name":"empty","smBannerUrl":"","mdBannerUrl":"medium","bannerUrl":"full"},
        {"name":"none"}, {"name":"medium","mdBannerUrl":"medium","bannerUrl":"full"},
        {"name":"small","smBannerUrl":"small","mdBannerUrl":"medium","bannerUrl":"full"}
    ]}"#;
    let current: Response = serde_json::from_str(json).unwrap();
    let original: Response = serde_json::from_str(json).unwrap();
    let actual: Vec<_> = current.data.into_iter().map(Entry::into_pack).collect();
    let expected: Vec<_> = original.data.into_iter().map(original::into_pack).collect();
    assert_eq!(actual, expected);
    assert_eq!(
        actual
            .iter()
            .map(|p| p.banner_url.as_deref())
            .collect::<Vec<_>>(),
        [Some("full"), Some(""), None, Some("medium"), Some("small")]
    );
    assert!(
        actual
            .iter()
            .all(|p| p.popularity == 0.0 && p.simfile_count == 0)
    );
    assert!(!current.meta.has_next_page);
}

#[test]
#[ignore = "paired release benchmark; run with --ignored --nocapture --test-threads=1"]
fn benchmark_owned_popularity_results() {
    for count in [0, 1, 100, 1200] {
        for mask in [0, 1, 7] {
            if count == 0 && mask != 0 {
                continue;
            }
            let prepare = || (0..count).map(|_| entry(mask, false)).collect::<Vec<_>>();
            let convert = |entries: Vec<Entry>, current: bool| {
                let mut packs = Vec::with_capacity(entries.len());
                for entry in entries {
                    packs.push(if current {
                        entry.into_pack()
                    } else {
                        original::into_pack(entry)
                    });
                }
                packs
            };
            let old_input = prepare();
            let new_input = prepare();
            let (_, before) = perf::measure(|| convert(old_input, false));
            let (_, after) = perf::measure(|| convert(new_input, true));
            let label = format!("popular/{count}/banners{mask}");
            println!("{label} allocations: original {before:?}; current {after:?}");
            paired_bench::compare_prepared(
                &label,
                if count < 100 { 20_000 } else { 64 },
                prepare,
                |input, current| {
                    drop(black_box(convert(black_box(input), current)));
                },
            );
        }
    }
}
