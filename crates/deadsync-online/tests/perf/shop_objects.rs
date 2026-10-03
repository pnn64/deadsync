use super::*;
use crate::perf::{assert_no_churn, assert_reduced_churn, measure_sampled};
use std::hint::black_box;

mod baseline {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/perf/shop_objects/baseline.rs"
    ));
}

fn fields(download: &ParsedDownload) -> (&str, &str, &str, &str, bool) {
    (
        &download.item_id,
        &download.name,
        &download.details,
        &download.url,
        download.site_downloaded,
    )
}

fn map(value: Value) -> Map<String, Value> {
    value.as_object().unwrap().clone()
}

#[test]
fn borrowed_object_text_preserves_alias_case_priority_and_scalar_conversion() {
    let keys = &["url", "href", "download_url"];
    let values = [
        Value::Null,
        Value::Bool(false),
        Value::Bool(true),
        serde_json::json!(0),
        serde_json::json!(-17),
        serde_json::json!(1.25),
        serde_json::json!(u64::MAX),
        serde_json::json!(""),
        serde_json::json!(" song.zip "),
        serde_json::json!("日本語.zip"),
        serde_json::json!([]),
        serde_json::json!({"nested":"song.zip"}),
    ];
    for first in &values {
        for second in &values {
            for key in ["url", "URL", "Url"] {
                let mut object = Map::new();
                object.insert(key.into(), first.clone());
                object.insert("Href".into(), second.clone());
                object.insert("download_url".into(), "fallback.zip".into());
                assert_eq!(
                    baseline::object_text(&object, keys).as_deref(),
                    object_text_value(&object, keys)
                        .map(|value| value_text_ref(Some(value)))
                        .as_deref()
                );
            }
        }
    }
    // Differently cased duplicate keys keep the map's first matching entry.
    for value in values {
        let object = map(serde_json::json!({"URL":value,"url":"later.zip","href":"fallback.zip"}));
        assert_eq!(
            baseline::object_text(&object, keys).as_deref(),
            object_text_value(&object, keys)
                .map(|value| value_text_ref(Some(value)))
                .as_deref()
        );
    }
    let object = map(serde_json::json!({"url":"song.zip"}));
    let borrowed = value_text_ref(object_text_value(&object, keys));
    assert!(matches!(borrowed, Cow::Borrowed(_)));
    assert!(std::ptr::eq(
        borrowed.as_ptr(),
        object["url"].as_str().unwrap().as_ptr()
    ));
    assert_no_churn(|| {
        drop(black_box(value_text_ref(object_text_value(
            black_box(&object),
            keys,
        ))))
    });
}

#[test]
fn borrowed_download_objects_preserve_fields_rejection_and_retained_capacity() {
    for url in [
        "",
        "song.mp3",
        "song.ZIP",
        "song.zip",
        "/downloads/song.zip",
        "././日本語.zip",
        "https://example.test/song.zip",
        "http://example.test/song.zip",
        r"https:\/\/example.test\/song.zip",
        r"a\\/b\/song.zip",
        "a.zip?x=\0#y",
    ] {
        for name in [
            "",
            "Song 日本語",
            "<b>Song</b>&amp;Mix",
            "  A\t B  ",
            "&amp;lt;b&amp;gt;Mix",
            "A|B",
        ] {
            for id in [
                Value::Null,
                serde_json::json!(""),
                serde_json::json!(42),
                serde_json::json!(false),
                serde_json::json!("001"),
            ] {
                for (url_key, name_key, id_key) in [
                    ("url", "song", "id"),
                    ("HREF", "TITLE", "CID"),
                    ("download_url", "name", "itemid"),
                ] {
                    let mut object = Map::new();
                    object.insert(url_key.into(), url.into());
                    object.insert(name_key.into(), name.into());
                    object.insert(id_key.into(), id.clone());
                    let old = baseline::download_from_object(&object);
                    let new = download_from_object(&object);
                    assert_eq!(old.as_ref().map(fields), new.as_ref().map(fields));
                    if let (Some(old), Some(new)) = (old, new) {
                        assert!(new.item_id.capacity() <= old.item_id.capacity());
                        assert!(new.name.capacity() <= old.name.capacity());
                        assert!(new.details.capacity() <= old.details.capacity());
                        assert!(new.url.capacity() <= old.url.capacity());
                    }
                }
            }
        }
    }
}

#[test]
fn borrowed_download_objects_remove_copies_before_cleanup_and_rejection() {
    for object in [
        map(serde_json::json!({"url":"song.zip","song":"Song","id":"1"})),
        map(
            serde_json::json!({"href":r"https:\/\/example.test\/song.zip","title":"<b>Song</b>","cid":17}),
        ),
        map(serde_json::json!({"url":"song.mp3"})),
    ] {
        assert_reduced_churn(
            || {
                drop(black_box(baseline::download_from_object(black_box(
                    &object,
                ))))
            },
            || drop(black_box(download_from_object(black_box(&object)))),
        );
    }
}

#[test]
#[ignore = "manual release CPU/allocation benchmark"]
fn shop_objects_benchmark() {
    let original = black_box(
        baseline::download_from_object as fn(&Map<String, Value>) -> Option<ParsedDownload>,
    );
    let current =
        black_box(download_from_object as fn(&Map<String, Value>) -> Option<ParsedDownload>);
    for (name, object) in [
        ("missing", map(serde_json::json!({}))),
        ("empty", map(serde_json::json!({"url":""}))),
        (
            "rejected",
            map(serde_json::json!({"url":"downloads/song.mp3"})),
        ),
        (
            "relative",
            map(serde_json::json!({"url":"downloads/song.zip","song":"Song 日本語","id":"001"})),
        ),
        (
            "rooted",
            map(serde_json::json!({"url":"/downloads/song.zip","song":"Song","id":"001"})),
        ),
        (
            "https",
            map(
                serde_json::json!({"url":"https://example.test/song.zip","song":"Song","id":"001"}),
            ),
        ),
        (
            "escaped",
            map(
                serde_json::json!({"url":r"https:\/\/example.test\/song.zip","song":"Song","id":"001"}),
            ),
        ),
        (
            "html",
            map(serde_json::json!({"url":"song.zip","song":"<b>Song 日本語</b>&amp;Mix","id":42})),
        ),
        (
            "fallback",
            map(
                serde_json::json!({"url":"","href":"","DOWNLOAD_URL":"song.zip","song":null,"Title":17,"id":false,"cid":"007"}),
            ),
        ),
        ("default-name", map(serde_json::json!({"href":"song.zip"}))),
        (
            "long",
            map(
                serde_json::json!({"url":format!("https://example.test/{}/song.zip","日本語".repeat(64)),"song":"Clean item ".repeat(128),"id":"001"}),
            ),
        ),
    ] {
        let run = |variant, f: fn(&Map<String, Value>) -> Option<ParsedDownload>| {
            measure_sampled(&format!("shop-objects/{name}/{variant}"), 16384, 1, || {
                f(black_box(&object))
            });
        };
        if std::env::var_os("DEADSYNC_BENCH_NEW_FIRST").is_some() {
            run("current", current);
            run("original", original);
        } else {
            run("original", original);
            run("current", current);
        }
    }
    for (name, object) in [
        ("lookup-string", map(serde_json::json!({"url":"song.zip"}))),
        ("lookup-number", map(serde_json::json!({"href":1234}))),
        ("lookup-missing", Map::new()),
    ] {
        let old =
            black_box(baseline::object_text as fn(&Map<String, Value>, &[&str]) -> Option<String>);
        let new = black_box(
            object_text_value as for<'a> fn(&'a Map<String, Value>, &[&str]) -> Option<&'a Value>,
        );
        let original = || {
            measure_sampled(&format!("shop-objects/{name}/original"), 16384, 1, || {
                old(
                    black_box(&object),
                    black_box(&["url", "href", "download_url"]),
                )
            })
        };
        let current = || {
            measure_sampled(&format!("shop-objects/{name}/current"), 16384, 1, || {
                new(
                    black_box(&object),
                    black_box(&["url", "href", "download_url"]),
                )
                .map(|value| value_text_ref(Some(value)))
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
