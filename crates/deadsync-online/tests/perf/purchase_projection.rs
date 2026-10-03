use super::*;
use crate::perf::{assert_reduced_churn, measure_sampled};
use std::hint::black_box;

mod baseline {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/perf/purchase_projection/baseline.rs"
    ));
}

fn fields(result: &PurchaseResult) -> (&[String], Option<&str>) {
    (
        &result.errors,
        result
            .download
            .as_ref()
            .map(|download| download.name.as_str()),
    )
}

#[test]
fn purchase_projection_preserves_errors_name_url_eligibility_and_aliases() {
    let values = [
        Value::Null,
        Value::Bool(false),
        serde_json::json!(42),
        serde_json::json!(""),
        serde_json::json!("song.zip"),
        serde_json::json!("song.ZIP"),
        serde_json::json!(r"https:\/\/example.test\/song.zip"),
        serde_json::json!("./日本語.zip?x=1"),
        serde_json::json!({"url":"nested.zip"}),
    ];
    for first in &values {
        for second in &values {
            for name in [
                serde_json::json!("Song 日本語"),
                serde_json::json!("<b>A</b>&amp;"),
                serde_json::json!("  A\t B "),
                serde_json::json!(17),
                Value::Bool(false),
                Value::Null,
                serde_json::json!(""),
            ] {
                for errors in [
                    serde_json::json!([]),
                    serde_json::json!(["one", 17, "two", null]),
                    serde_json::json!("not-array"),
                ] {
                    let text = serde_json::json!({"errors":errors,"unlocks":{"URL":first,"url":"ignored.zip","Href":second,"download_url":"fallback.zip","song":name,"title":"Fallback","id":"unused","cid":17}}).to_string();
                    let old = baseline::parse_purchase(&text).unwrap();
                    let new = parse_purchase(&text).unwrap();
                    assert_eq!(fields(&old), fields(&new));
                    if let (Some(old), Some(new)) = (old.download, new.download) {
                        assert_eq!(old.name.capacity(), new.name.capacity());
                    }
                }
            }
        }
    }
    for text in [
        "",
        "{",
        "{}",
        "null",
        r#"{"unlocks":[]}"#,
        r#"{"errors":["failed"],"unlocks":{"url":"song.mp3"}}"#,
        r#"{"unlocks":{"href":"song.zip"}}"#,
    ] {
        match (baseline::parse_purchase(text), parse_purchase(text)) {
            (Ok(old), Ok(new)) => assert_eq!(fields(&old), fields(&new)),
            (Err(old), Err(new)) => assert_eq!(format!("{old:?}"), format!("{new:?}")),
            _ => panic!("different purchase result"),
        }
    }
}

fn body(url: &str, name: &str, id: Value) -> String {
    serde_json::json!({"unlocks":{"url":url,"song":name,"id":id}}).to_string()
}

#[test]
fn purchase_projection_does_not_build_discarded_ids_or_normalized_urls() {
    for text in [
        body("song.zip", "Song", serde_json::json!("001")),
        body(
            r"https:\/\/example.test\/song.zip",
            "<b>Song</b>",
            serde_json::json!(42),
        ),
        body("song.zip", "Song", serde_json::json!("id".repeat(2048))),
    ] {
        assert_reduced_churn(
            || {
                drop(black_box(
                    baseline::parse_purchase(black_box(&text)).unwrap(),
                ))
            },
            || drop(black_box(parse_purchase(black_box(&text)).unwrap())),
        );
    }
}

#[test]
#[ignore = "manual release CPU/allocation benchmark"]
fn purchase_projection_benchmark() {
    let old =
        black_box(baseline::parse_purchase as fn(&str) -> Result<PurchaseResult, SrpgShopError>);
    let new = black_box(parse_purchase as fn(&str) -> Result<PurchaseResult, SrpgShopError>);
    for (name, text) in [
        ("missing", "{}".into()),
        (
            "rejected",
            body("song.mp3", "Song", serde_json::json!("001")),
        ),
        (
            "relative",
            body(
                "downloads/song.zip",
                "Song 日本語",
                serde_json::json!("001"),
            ),
        ),
        (
            "https",
            body(
                "https://example.test/song.zip",
                "Song",
                serde_json::json!("001"),
            ),
        ),
        (
            "escaped",
            body(
                r"https:\/\/example.test\/song.zip",
                "Song",
                serde_json::json!("001"),
            ),
        ),
        (
            "html-numeric",
            body("song.zip", "<b>Song</b>&amp;Mix", serde_json::json!(42)),
        ),
        (
            "large-id",
            body("song.zip", "Song", serde_json::json!("id".repeat(2048))),
        ),
        (
            "long-url",
            body(
                &format!("https://example.test/{}/song.zip", "日本語".repeat(128)),
                "Song",
                serde_json::json!("001"),
            ),
        ),
    ] {
        let run = |variant, f: fn(&str) -> Result<PurchaseResult, SrpgShopError>| {
            measure_sampled(&format!("purchase/{name}/{variant}"), 4096, 1, || {
                f(black_box(&text)).unwrap()
            });
        };
        if std::env::var_os("DEADSYNC_BENCH_NEW_FIRST").is_some() {
            run("current", new);
            run("original", old);
        } else {
            run("original", old);
            run("current", new);
        }
    }
}
