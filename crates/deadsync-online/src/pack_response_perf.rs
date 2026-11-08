use super::response_original as original;
use super::*;
use crate::perf::{assert_no_churn, measure};
use std::hint::black_box;

#[path = "../../../tests/support/paired_bench.rs"]
mod paired_bench;

fn assert_same(left: &PackPage, right: &PackPage) {
    assert_eq!(left.songs, right.songs);
    assert_eq!(left.meter_labels, right.meter_labels);
    assert_eq!(left.meter_counts, right.meter_counts);
    assert_eq!(left.chart_count, right.chart_count);
    assert_eq!(left.authors, right.authors);
    assert_eq!(left.styles, right.styles);
    assert_eq!(left.banner_url, right.banner_url);
}

fn page(rows: usize, unicode: bool) -> String {
    let mut html = String::from(
        r#"<meta property="og:image" content="/media/images/packs/7.png"><script>labels: [1,4,9,30,5454], data: [2,7,3,1,99]</script><table>"#,
    );
    for id in 0..rows {
        let title = if unicode {
            "東京 &amp;lt;Ω&amp;gt;\u{2003}Song"
        } else {
            "Vertex &amp; Beta"
        };
        html.push_str(&format!(r#"<tr>
          <td><img src="/media/images/songs/{id}.png"></td>
          <td><a href="/song/{id}">{title} {id}</a><span class="text-gray-400">Artist {id}</span></td>
          <td>subtitle</td><td> 1:52 </td><td>150-190</td>
          <td>Alice<br>Bob,<br />Alice</td>
          <td data-sort="[&#x27;dance-double&#x27;, &#x27;dance-single&#x27;]"></td>
          <td>3, 6, 9, 12</td></tr>"#));
    }
    html.push_str("</table>");
    html
}

#[test]
fn numeric_filter_preserves_limits_leading_zeros_and_non_digit_rules() {
    for text in [
        "",
        ",,,",
        "nothing",
        "0,00,00000",
        "-12,+34,1x2y3,1.2.3",
        "4294967295,4294967296,42949672950,9,18446744073709551616,7",
        "[1, 2, 3]",
        "１２,١٢,1東京2,💃3e4",
        "1\u{2003}2,\t3\n4",
        "000000000000000000000000000000004294967295,0000000000000000000",
    ] {
        assert_eq!(numbers(text), original::numbers(text), "{text:?}");
    }
    assert_eq!(numbers("4294967295,4294967296,7"), [u32::MAX, 7]);
    assert_eq!(numbers("-12,1x2,１２,١٢"), [12, 12]);
    let zeros = "0".repeat(4096);
    assert_eq!(numbers(&format!("{zeros},1{zeros},7")), [0, 7]);
}

#[test]
fn numeric_filter_matches_original_for_exhaustive_short_inputs() {
    let alphabet = ["0", "1", "9", "x", "-", ",", " ", "東京"];
    for length in 0..=5 {
        for mut index in 0..alphabet.len().pow(length) {
            let mut input = String::new();
            for _ in 0..length {
                input.push_str(alphabet[index % alphabet.len()]);
                index /= alphabet.len();
            }
            assert_eq!(numbers(&input), original::numbers(&input), "{input:?}");
        }
    }
}

#[test]
fn histogram_indices_and_saturating_fallback_match_original() {
    for labels in ["0,1,31,30", "1,4294967296,2,3", "1,1,2,9", "-1,2x0,１２,3"] {
        for counts in ["", "1,2", "4294967295,1,2,3", "1,4294967296,2,3"] {
            let html = format!("<script>labels: [{labels}], data: [{counts}]</script>");
            assert_same(&parse(&html), &original::parse(&html));
        }
    }
    let html = "labels: [1,2], data: [4294967295,1]";
    assert_eq!(parse(html).chart_count.as_deref(), Some("4294967295"));
}

#[test]
fn borrowed_cells_preserve_malformed_html_entities_and_unicode() {
    for html in [
        "",
        "<td>x",
        "<td>1</td>junk<td attr=\"東京\">2</td>",
        "</td><td>first<td>nested</td><td>last</td>",
        "<td><a href=\"/song/1\">東京 &amp;lt;b&amp;gt;</a><span class=\"text-gray-400\">💃</span></td>",
        "<td><a href=\"/song/1\">unclosed<span text-gray-400>artist</span></td>",
    ] {
        let cells = table_cells(html);
        assert_eq!(cells, original::table_cells(html));
        for cell in cells {
            assert_eq!(title_of(cell), original::title_of(cell).as_deref());
            assert_eq!(artist_of(cell), original::artist_of(cell).as_deref());
            for part in [Some(cell), title_of(cell), artist_of(cell)]
                .into_iter()
                .flatten()
            {
                let start = part.as_ptr() as usize;
                assert!(start >= html.as_ptr() as usize);
                assert!(start + part.len() <= html.as_ptr() as usize + html.len());
            }
        }
    }
}

#[test]
fn complete_pages_and_truncated_rows_match_original() {
    for rows in [0, 1, 24, 200] {
        for unicode in [false, true] {
            let html = page(rows, unicode);
            assert_same(&parse(&html), &original::parse(&html));
        }
    }
    let html = page(2, true);
    for (boundary, _) in html.char_indices() {
        assert_same(
            &parse(&html[..boundary]),
            &original::parse(&html[..boundary]),
        );
    }
}

#[test]
fn borrowed_row_fields_remove_ten_allocations_per_complete_song() {
    let html = page(24, true);
    let (expected, old) = measure(|| original::parse_songs(&html));
    let (actual, new) = measure(|| parse_songs(&html));
    assert_eq!(actual, expected);
    assert_eq!(old.allocs - new.allocs, 10 * 24);
    assert!(new.allocated_bytes < old.allocated_bytes);
    let cells = table_cells(&html);
    assert_no_churn(|| {
        black_box(title_of(black_box(cells[1])));
        black_box(artist_of(black_box(cells[1])));
    });
    let list = (1..=60)
        .map(|n| n.to_string())
        .collect::<Vec<_>>()
        .join(",");
    let (expected, old) = measure(|| original::numbers(&list));
    let (actual, new) = measure(|| numbers(&list));
    assert_eq!(actual, expected);
    assert_eq!(old.allocs - new.allocs, 60);
}

#[test]
#[ignore = "paired release benchmark; run explicitly with --ignored --nocapture"]
fn benchmark_histogram_numbers() {
    let mut cases = vec![("empty".to_owned(), String::new())];
    for count in [24, 60, 200] {
        cases.push((
            count.to_string(),
            (1..=count)
                .map(|n| n.to_string())
                .collect::<Vec<_>>()
                .join(", "),
        ));
    }
    cases.push((
        "mixed".into(),
        "-12,4294967296,１２,1e3,,4294967295,7".repeat(20),
    ));
    cases.push((
        "zeros-overflow".into(),
        format!("{},1{},7", "0".repeat(4096), "0".repeat(4096)),
    ));
    for (label, text) in cases {
        let (expected, old) = measure(|| original::numbers(&text));
        let (actual, new) = measure(|| numbers(&text));
        assert_eq!(actual, expected);
        println!("numbers-{label} churn: original {old:?}, current {new:?}");
        paired_bench::compare(&format!("numbers-{label}"), 100, |current| {
            black_box(if current {
                numbers(black_box(&text))
            } else {
                original::numbers(black_box(&text))
            });
        });
    }
}

#[test]
#[ignore = "paired release benchmark; run explicitly with --ignored --nocapture"]
fn benchmark_borrowed_pack_rows() {
    for (count, unicode) in [
        (0, false),
        (1, false),
        (24, false),
        (200, false),
        (200, true),
    ] {
        let html = page(count, unicode);
        let label = format!("rows-{count}-{}", if unicode { "unicode" } else { "ascii" });
        let (expected, old) = measure(|| original::parse_songs(&html));
        let (actual, new) = measure(|| parse_songs(&html));
        assert_eq!(actual, expected);
        println!("{label} churn: original {old:?}, current {new:?}");
        paired_bench::compare(&label, 30, |current| {
            black_box(if current {
                parse_songs(black_box(&html))
            } else {
                original::parse_songs(black_box(&html))
            });
        });
    }
    let html = "<tr><td><a href=\"/song/1\">incomplete</a></td><td>東京</td></tr>".repeat(200);
    paired_bench::compare("rows-200-incomplete", 30, |current| {
        black_box(if current {
            parse_songs(black_box(&html))
        } else {
            original::parse_songs(black_box(&html))
        });
    });
    let html = page(200, true);
    let (expected, old) = measure(|| original::parse(&html));
    let (actual, new) = measure(|| parse(&html));
    assert_same(&actual, &expected);
    println!("page-200-combined churn: original {old:?}, current {new:?}");
    paired_bench::compare("page-200-combined", 30, |current| {
        black_box(if current {
            parse(black_box(&html))
        } else {
            original::parse(black_box(&html))
        });
    });
}
