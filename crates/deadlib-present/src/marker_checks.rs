use super::*;
include!("marker_original.rs");

#[test]
fn marker_scanner_preserves_text_and_ownership() {
    let pieces = [
        "",
        "&",
        ";",
        "&&",
        "&START;",
        "&start;",
        "&missing;",
        "&#65;",
        "&x1F642;",
        "&#4294967296;",
        "&xD800;",
        "&x110000;",
        "& #65;",
        "日本語",
        "é",
        "\0",
        "\n",
    ];
    for a in pieces {
        for b in pieces {
            for c in pieces {
                let text = format!("{a}{b}{c}");
                let before = original_replace_markers(&text);
                let after = replace_markers(&text);
                assert_eq!(after, before, "{text:?}");
                assert_eq!(
                    matches!(after, Cow::Borrowed(_)),
                    matches!(before, Cow::Borrowed(_)),
                    "{text:?}"
                );
            }
        }
    }
}
