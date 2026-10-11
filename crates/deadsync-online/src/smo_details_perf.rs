use super::*;
use crate::perf;
use std::hint::black_box;

#[path = "smo_details_original.rs"]
mod original;
#[path = "../../../tests/support/paired_bench.rs"]
mod paired;

fn page(count: usize, invalid: bool) -> String {
    let rows: Vec<_> = (0..count)
        .map(|id| {
            serde_json::json!([
                format!("<img data-src=\"/media/images/packs/{id}.jpg\">"),
                format!("<a href=\"/pack/{id}\">Pack {id}</a>"),
                "<span>1 MB</span>",
                "<span>12</span>",
                "<span data-sort=\"[&#x27;dance&#x27;]\">dance</span>",
                if invalid {
                    "<span>unknown release date</span>"
                } else {
                    "<span>2026-10-09</span>"
                },
                "<a>download</a>"
            ])
        })
        .collect();
    serde_json::json!({"data": rows}).to_string()
}

#[test]
fn attribute_and_date_edges_match_original() {
    let values = [
        "",
        "plain",
        "[&#x27;dance&#x27;]",
        "https://host/a.jpg",
        "/nobanner.png",
        "\u{e9}&amp;\"tail",
    ];
    let prefixes = ["", "<img ", "xdata-src=\"earlier\" ", "\u{2003}"];
    for name in ["data-src", "data-sort"] {
        let prefix = format!("{name}=\"");
        for leading in prefixes {
            for value in values {
                for ending in ["", "\"", "\">tail", "\" data-src=\"second\""] {
                    let cell = format!("{leading}{prefix}{value}{ending}");
                    assert_eq!(
                        attribute(&cell, &prefix),
                        original::attribute(&cell, name),
                        "{cell:?}"
                    );
                    assert_eq!(banner_url(&cell), original::banner_url(&cell));
                    assert_eq!(chart_types(&cell), original::chart_types(&cell));
                }
            }
        }
    }
    for text in [
        "",
        " ",
        "2026-10-09",
        "0000-99-99",
        "2026-1-09",
        "not a date",
        "\u{ff12}026-10-09",
        "2026-10-09<tail",
        "\u{2003}2026-10-09\u{a0}",
    ] {
        for (open, close) in [("", ""), ("<span>", ""), ("<span>", "</span>"), (">", "<")] {
            let cell = format!("{open}{text}{close}");
            assert_eq!(inner_text(&cell), original::inner_text(&cell).as_deref());
            let current = inner_text(&cell)
                .filter(|date| is_iso_date(date))
                .map(str::to_owned);
            let old = original::inner_text(&cell).filter(original::is_iso_date);
            assert_eq!(current, old, "{cell:?}");
        }
    }
}

#[test]
fn pages_and_malformed_rows_match_original() {
    for count in [0, 1, 200, 1200] {
        for invalid in [false, true] {
            let body = page(count, invalid);
            assert_eq!(parse_page(&body), original::parse_page(&body));
        }
    }
    for body in [
        "",
        "not json",
        "[]",
        "null",
        "{}",
        r#"{"data": null}"#,
        r#"{"data":[null,1,{},[],[""],[null,null,null,null,null,null,null]]}"#,
    ] {
        assert_eq!(parse_page(body), original::parse_page(body));
    }
    let mut value: serde_json::Value = serde_json::from_str(&page(1, false)).unwrap();
    for column in 0..7 {
        for replacement in [
            serde_json::Value::Null,
            serde_json::json!(42),
            serde_json::json!({}),
            serde_json::json!("malformed"),
        ] {
            let mut malformed = value.clone();
            malformed["data"][0][column] = replacement;
            let body = malformed.to_string();
            assert_eq!(parse_page(&body), original::parse_page(&body));
        }
    }
    value["data"][0][1] = serde_json::json!("/pack/18446744073709551616");
    assert_eq!(
        parse_page(&value.to_string()),
        original::parse_page(&value.to_string())
    );
}

#[test]
fn parsing_removes_only_temporary_allocations() {
    for invalid in [false, true] {
        let body = page(200, invalid);
        let (old, before) = perf::measure(|| original::parse_page(&body));
        let (new, after) = perf::measure(|| parse_page(&body));
        assert_eq!(old, new);
        assert_eq!(
            before.allocs - after.allocs,
            if invalid { 600 } else { 400 }
        );
        assert!(after.allocated_bytes < before.allocated_bytes);
    }
    perf::assert_no_churn(|| {
        black_box(attribute(
            black_box("<img data-src=\"a.png\">"),
            "data-src=\"",
        ));
        black_box(inner_text(black_box("<span>unknown date</span>")));
    });
}

#[test]
#[ignore = "paired release benchmark"]
fn benchmark_details_pages() {
    for count in [0, 1, 200, 1200] {
        for invalid in [false, true] {
            let body = page(count, invalid);
            let label = format!(
                "details/{count}/{}",
                if invalid { "invalid-dates" } else { "valid" }
            );
            let (old, before) = perf::measure(|| original::parse_page(&body));
            let (new, after) = perf::measure(|| parse_page(&body));
            assert_eq!(old, new);
            println!("{label}: churn original {before:?}, current {after:?}");
            paired::compare(&label, 10, |current| {
                drop(black_box(if current {
                    parse_page(black_box(&body))
                } else {
                    original::parse_page(black_box(&body))
                }));
            });
        }
    }
}
