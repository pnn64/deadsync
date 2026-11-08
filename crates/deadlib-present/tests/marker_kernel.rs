use deadlib_present::{
    actors::{Actor, TextContent},
    dsl::TextBuilder,
    font::replace_markers,
};
use std::hint::black_box;
#[path = "../../../tests/support/perf.rs"]
#[allow(dead_code)]
mod alloc;
#[path = "../../../tests/support/kernel_bench.rs"]
mod bench;

fn build(text: &'static str) -> Actor {
    let mut builder = TextBuilder::new();
    builder.settext(TextContent::Static(text));
    builder.build(0)
}

#[test]
fn building_text_preserves_marker_expansion() {
    for text in ["plain text", "Press &START;", "&missing;", "&&#65;\u{e9}"] {
        let expected = replace_markers(text);
        let Actor::Text { content, .. } = build(text) else {
            panic!("text builder returned another actor")
        };
        assert_eq!(content.as_str(), expected);
    }
}

#[test]
#[ignore = "release CPU benchmark"]
fn benchmark_marker_kernel() {
    black_box(replace_markers("&START;"));
    let fixtures = [
        ("plain-short", "Song selection".to_owned()),
        (
            "plain-long",
            "Chart title \u{65e5}\u{672c}\u{8a9e} ".repeat(32),
        ),
        ("alias", "Press &START;".to_owned()),
        ("numeric", "Score &#9654; combo &x2605;".to_owned()),
        ("sparse", format!("{} &START;", "a".repeat(2048))),
        ("unknown", format!("&{};", "a".repeat(2048))),
        ("unterminated", format!("&{}", "a".repeat(2048))),
        ("dense", "&START; &#9654; &missing; ".repeat(24)),
    ];
    for (label, text) in fixtures {
        let label = format!("{label}/bytes={}", text.len());
        let text: &'static str = Box::leak(text.into_boxed_str());
        let (_, counts) = alloc::measure(|| replace_markers(text));
        println!("markers/{label} allocations {counts:?}");
        bench::measure(&format!("markers/{label}"), 20_000, || {
            let scan = black_box(replace_markers as fn(&str) -> std::borrow::Cow<'_, str>);
            black_box(scan(black_box(text)));
        });
        let (_, counts) = alloc::measure(|| build(text));
        println!("text-build/{label} allocations {counts:?}");
        bench::measure(&format!("text-build/{label}"), 20_000, || {
            black_box(build(black_box(text)));
        });
    }
}
