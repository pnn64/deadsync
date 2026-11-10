use deadlib_present::{
    actors::{Actor, TextContent},
    dsl::TextBuilder,
    font::replace_markers,
};
#[path = "../../../tests/support/allocations.rs"]
#[allow(dead_code)]
mod alloc;

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
