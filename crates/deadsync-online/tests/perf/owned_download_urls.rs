use super::*;
use crate::perf::{assert_no_churn, assert_reduced_churn, measure_sampled};
use std::hint::black_box;

mod baseline {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/perf/owned_download_urls/baseline.rs"
    ));
}

fn body(count: usize, url: &str) -> String {
    serde_json::json!({"unlocks": (0..count).map(|id| serde_json::json!({
        "id": id.to_string(), "song": "Song 日本語", "data": "14 180", "url": url, "dled": id % 2
    })).collect::<Vec<_>>()})
    .to_string()
}

fn fields(row: &ParsedDownload) -> (&str, &str, &str, &str, bool) {
    (
        &row.item_id,
        &row.name,
        &row.details,
        &row.url,
        row.site_downloaded,
    )
}

#[test]
fn owned_urls_preserve_normalization_capacity_and_download_fields() {
    for url in [
        "",
        "song.zip",
        "/song.zip",
        "././song.zip",
        "https://example.test/song.zip",
        "http://example.test/song.zip",
        "HTTPS://example.test/song.zip",
        "//song.zip",
        r"https:\/\/example.test\/song.zip",
        r"a\\/b\/song.zip",
        r"\/\/song.zip",
        r"a\\\//b\/.zip",
        "日本語.zip?x=\0#frag",
        "song.ZIP",
        "song.mp3",
        ".../song.zip",
    ] {
        for capacity in [url.len(), 4096] {
            let old = baseline::absolutize_url(url);
            let mut input = String::with_capacity(capacity);
            input.push_str(url);
            let new = absolutize_url(input);
            assert_eq!(old, new, "{url:?}");
            assert!(new.capacity() <= old.capacity(), "{url:?}");
        }
        for count in [0, 1, 128] {
            let text = body(count, url);
            let old = baseline::parse_downloads(&text).unwrap();
            let new = parse_downloads(&text).unwrap();
            assert_eq!(
                old.iter().map(fields).collect::<Vec<_>>(),
                new.iter().map(fields).collect::<Vec<_>>()
            );
            assert!(new.capacity() <= old.capacity());
            for (old, new) in old.iter().zip(&new) {
                assert!(new.url.capacity() <= old.url.capacity());
            }
        }
    }
    // Exhaustive overlapping slash/backslash patterns exercise replacement order.
    for len in 0..=8 {
        for bits in 0..(1 << len) {
            let input: String = (0..len)
                .map(|n| if bits & (1 << n) == 0 { '/' } else { '\\' })
                .collect();
            assert_eq!(baseline::absolutize_url(&input), absolutize_url(input));
        }
    }
}

#[test]
fn absolute_owned_urls_reuse_storage_and_reduce_complete_parse_churn() {
    let url = "https://example.test/song.zip".to_owned();
    let pointer = url.as_ptr();
    let mut result = None;
    assert_no_churn(|| {
        result = Some(absolutize_url(black_box(url)));
    });
    let result = result.unwrap();
    assert_eq!(result.as_ptr(), pointer);
    for url in [
        "https://example.test/song.zip",
        "/song.zip",
        "./song.zip",
        r"https:\/\/example.test\/song.zip",
    ] {
        let text = body(128, url);
        assert_reduced_churn(
            || {
                drop(black_box(
                    baseline::parse_downloads(black_box(&text)).unwrap(),
                ))
            },
            || drop(black_box(parse_downloads(black_box(&text)).unwrap())),
        );
    }
}

#[test]
#[ignore = "manual release CPU/allocation benchmark"]
fn owned_download_urls_benchmark() {
    let old = black_box(
        baseline::parse_downloads as fn(&str) -> Result<Vec<ParsedDownload>, SrpgShopError>,
    );
    let new = black_box(parse_downloads as fn(&str) -> Result<Vec<ParsedDownload>, SrpgShopError>);
    for (name, count, url) in [
        ("empty", 0, "song.zip".to_owned()),
        ("rejected", 128, "song.ZIP".to_owned()),
        ("one-https", 1, "https://example.test/song.zip".to_owned()),
        ("https", 128, "https://example.test/song.zip".to_owned()),
        ("relative", 128, "song.zip".to_owned()),
        ("rooted", 128, "/song.zip".to_owned()),
        (
            "escaped-https",
            128,
            r"https:\/\/example.test\/song.zip".to_owned(),
        ),
        (
            "dense-escapes",
            128,
            format!("{}song.zip", r"a\/".repeat(128)),
        ),
        (
            "long-https",
            128,
            format!("https://example.test/{}.zip", "日本語".repeat(128)),
        ),
        (
            "unicode-escapes",
            128,
            format!(r"https:\/\/example.test\/{}.zip", r"日本語\/".repeat(128)),
        ),
    ] {
        let text = body(count, &url);
        let original = || {
            measure_sampled(
                &format!("owned-urls/{name}/original"),
                512,
                count.max(1),
                || old(black_box(&text)),
            )
        };
        let current = || {
            measure_sampled(
                &format!("owned-urls/{name}/current"),
                512,
                count.max(1),
                || new(black_box(&text)),
            )
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
