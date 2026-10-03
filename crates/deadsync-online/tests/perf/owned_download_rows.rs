use super::*;
use crate::perf::{assert_reduced_churn, measure_sampled};
use std::hint::black_box;

mod baseline {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/perf/owned_download_rows/baseline.rs"
    ));
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

fn body(count: usize, id: &Value, song: &str, data: &str) -> String {
    serde_json::json!({"unlocks": (0..count).map(|i| serde_json::json!({
        "id":id,"song":song,"data":data,
        "url":if i % 3 == 0 { r"https:\/\/example.test\/song.zip" } else { "./downloads/song.zip" },
        "dled": i % 3
    })).collect::<Vec<_>>()})
    .to_string()
}

#[test]
fn owned_download_rows_preserve_fields_errors_filters_and_retained_capacity() {
    for count in [0, 1, 16, 128] {
        for id in [
            serde_json::json!("001"),
            serde_json::json!(42),
            serde_json::json!(1.25),
            Value::Bool(false),
            Value::Null,
            serde_json::json!({}),
        ] {
            for (song, data) in [
                ("Song 日本語", "14 180"),
                ("Song ", "14 180  "),
                ("<b>Song</b>&amp;Mix", " 14\t180 "),
                ("&amp;lt;b&amp;gt;Mix", ""),
                ("A|B", "\u{2003}14\n180"),
            ] {
                let text = body(count, &id, song, data);
                let old = baseline::parse_downloads(&text).unwrap();
                let new = parse_downloads(&text).unwrap();
                assert_eq!(
                    old.iter().map(fields).collect::<Vec<_>>(),
                    new.iter().map(fields).collect::<Vec<_>>()
                );
                assert!(new.capacity() <= old.capacity());
                for (old, new) in old.iter().zip(&new) {
                    for (a, b) in [
                        (&old.item_id, &new.item_id),
                        (&old.name, &new.name),
                        (&old.details, &new.details),
                        (&old.url, &new.url),
                    ] {
                        assert!(b.capacity() <= a.capacity());
                    }
                }
            }
        }
    }
    for text in [
        "",
        "{",
        "{}",
        r#"{"errors":["one","two"]}"#,
        r#"{"unlocks":[{"id":"1","song":"A","url":"A.ZIP"},{"id":2,"song":"B","url":"B.mp3"}]}"#,
        r#"{"unlocks":[{"id":3,"song":"C","url":"C.zip"}]}"#,
        r#"{"unlocks":[{"song":"C","url":"C.zip"}]}"#,
    ] {
        match (baseline::parse_downloads(text), parse_downloads(text)) {
            (Ok(old), Ok(new)) => assert_eq!(
                old.iter().map(fields).collect::<Vec<_>>(),
                new.iter().map(fields).collect::<Vec<_>>()
            ),
            (Err(old), Err(new)) => assert_eq!(format!("{old:?}"), format!("{new:?}")),
            _ => panic!("different result for {text:?}"),
        }
    }
}

#[test]
fn owned_cell_cleanup_reuses_clean_buffers_without_retaining_extra_storage() {
    for text in [
        "",
        "Clean text",
        "Clean text  ",
        "日本語 É",
        "A|B",
        "<b>A</b>&amp;",
        "  A\t B  ",
    ] {
        for capacity in [text.len(), 4096] {
            let mut input = String::with_capacity(capacity);
            input.push_str(text);
            let old = clean_cell(&input).into_owned();
            let new = clean_owned_cell(input);
            assert_eq!(old, new);
            assert!(new.capacity() <= old.capacity());
        }
    }
    for text in ["Clean text", "日本語 É", "14 180"] {
        let old = text.to_owned();
        let new = text.to_owned();
        let pointer = new.as_ptr();
        let result = clean_owned_cell(new);
        assert_eq!(result.as_ptr(), pointer);
        let new = text.to_owned();
        assert_reduced_churn(
            || {
                drop(black_box(clean_cell(black_box(&old)).into_owned()));
                drop(old);
            },
            || drop(black_box(clean_owned_cell(black_box(new)))),
        );
    }
    let text = body(128, &serde_json::json!("001"), "Song 日本語", "14 180");
    assert_reduced_churn(
        || {
            drop(black_box(
                baseline::parse_downloads(black_box(&text)).unwrap(),
            ))
        },
        || drop(black_box(parse_downloads(black_box(&text)).unwrap())),
    );
}

#[test]
#[ignore = "manual release CPU/allocation benchmark"]
fn owned_download_rows_benchmark() {
    let old = black_box(
        baseline::parse_downloads as fn(&str) -> Result<Vec<ParsedDownload>, SrpgShopError>,
    );
    let new = black_box(parse_downloads as fn(&str) -> Result<Vec<ParsedDownload>, SrpgShopError>);
    for (name, text, units) in [
        ("empty", body(0, &serde_json::json!("001"), "Song", ""), 1),
        (
            "one",
            body(1, &serde_json::json!("001"), "Song 日本語", "14 180"),
            1,
        ),
        (
            "sixteen",
            body(16, &serde_json::json!("001"), "Song 日本語", "14 180"),
            16,
        ),
        (
            "many",
            body(128, &serde_json::json!("001"), "Song 日本語", "14 180"),
            128,
        ),
        (
            "numeric",
            body(128, &serde_json::json!(42), "Song 日本語", "14 180"),
            128,
        ),
        (
            "html-numeric",
            body(
                128,
                &serde_json::json!(42),
                "<b>Song</b>&amp;Mix",
                "14\t180",
            ),
            128,
        ),
        (
            "trailing",
            body(128, &serde_json::json!("001"), "Song ", "14 180  "),
            128,
        ),
        (
            "long",
            body(
                128,
                &serde_json::json!("id".repeat(128)),
                "Clean song ".repeat(64).trim_end(),
                "14 180",
            ),
            128,
        ),
    ] {
        let run = |variant, f: fn(&str) -> Result<Vec<ParsedDownload>, SrpgShopError>| {
            measure_sampled(
                &format!("download-rows/{name}/{variant}"),
                256,
                units,
                || f(black_box(&text)).unwrap(),
            );
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
