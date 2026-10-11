use super::*;
use crate::metadata_perf::{compare, measure};
use std::hint::black_box;
mod original {
    use super::*;
    include!("tags_parse_original.rs");
}
const TAGS: [&[u8]; 3] = [b"#TITLE:", b"#ARTIST:", b"#SUBTITLE:"];

#[test]
fn decoded_tag_ownership_preserves_encodings_escapes_and_latest_values() {
    for input in [
        b"".as_slice(),
        b"#OTHER:value;",
        b"#TITLE:caf\xe9;#ARTIST:DJ;",
        b"#TITLE:caf\xe9\\;mix;#TITLE:last;",
        b"#TITLE:\xf0\x9f\x8e\xb5;#SUBTITLE:\xe9;",
        b"#TITLE:one\\\\;#ARTIST:two;",
        b"#TITLE:old;#title:new\n #ARTIST:DJ;",
        b"#TITLE:trailing\\;",
        b"#TITLE:\x00\x81\x8d\x90\x9d;",
        b"#TITLE:bad\xff\xc3\x28;#SUBTITLE:esc\\#hash;",
    ] {
        assert_eq!(
            latest_simfile_tag_values(input, TAGS),
            original::latest_simfile_tag_values(input, TAGS)
        );
    }
    assert_eq!(
        latest_simfile_tag_value(b"#TITLE:caf\xe9;", b"#TITLE:"),
        "caf\u{e9}"
    );
    assert_eq!(
        latest_simfile_tag_values(
            b"#TITLE:value;",
            [b"#TITLE:".as_slice(), b"#title:".as_slice()]
        ),
        ["value", ""]
    );
    assert_eq!(
        latest_simfile_tag_values(b"#TITLE:value;", []),
        [] as [String; 0]
    );
    let mut seed = 0xa963f81du32;
    for _ in 0..2048 {
        let mut input = b"#TITLE:".to_vec();
        for _ in 0..128 {
            seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
            input.push((seed >> 24) as u8);
        }
        input.extend_from_slice(b";#ARTIST:DJ;#SUBTITLE:last;");
        assert_eq!(
            latest_simfile_tag_values(&input, TAGS),
            original::latest_simfile_tag_values(&input, TAGS)
        );
    }
}

#[test]
fn decoded_legacy_tag_moves_its_buffer_without_a_second_allocation() {
    let input = b"#TITLE:caf\xe9;#ARTIST:cr\xe8me;#SUBTITLE:d\xe9j\xe0;";
    let (old, a) = measure(|| original::latest_simfile_tag_values(input, TAGS));
    let (new, b) = measure(|| latest_simfile_tag_values(input, TAGS));
    assert_eq!(old, new);
    assert_eq!(
        old.each_ref().map(String::capacity),
        new.each_ref().map(String::capacity)
    );
    assert_eq!(a.allocs, b.allocs + 3);
    assert_eq!(a.reallocs, b.reallocs);
    for input in [
        b"#TITLE:ASCII;".as_slice(),
        b"#TITLE:caf\xe9\\;mix;",
        b"#OTHER:empty;",
    ] {
        let (old, a) = measure(|| original::latest_simfile_tag_values(input, TAGS));
        let (new, b) = measure(|| latest_simfile_tag_values(input, TAGS));
        assert_eq!(old, new);
        assert_eq!(a.allocs, b.allocs);
        assert_eq!(a.reallocs, b.reallocs);
    }
}

#[test]
#[ignore = "paired release benchmark; run alone with --nocapture"]
fn benchmark_parse_tag_ownership() {
    for (label, value) in [
        ("absent", Vec::new()),
        ("ascii", b"Artwork/Background.png".to_vec()),
        ("utf8", "Artwork/caf\u{e9}.png".as_bytes().to_vec()),
        ("legacy-short", b"caf\xe9.png".to_vec()),
        ("legacy-128", b"caf\xe9.png".repeat(16)),
        ("legacy-4096", b"caf\xe9.png".repeat(512)),
        ("legacy-escaped", b"caf\xe9\\;mix.png".to_vec()),
        ("ascii-escaped", b"Artwork\\;Background.png".to_vec()),
    ] {
        let mut data = Vec::new();
        if !value.is_empty() {
            for tag in TAGS {
                data.extend_from_slice(tag);
                data.extend_from_slice(&value);
                data.push(b';');
            }
        }
        let (_, a) = measure(|| original::latest_simfile_tag_values(&data, TAGS));
        let (_, b) = measure(|| latest_simfile_tag_values(&data, TAGS));
        println!("ALLOC tags/{label}: original {a:?}, current {b:?}");
        compare(
            &format!("tags/{label}"),
            128,
            || {
                black_box(original::latest_simfile_tag_values(black_box(&data), TAGS));
            },
            || {
                black_box(latest_simfile_tag_values(black_box(&data), TAGS));
            },
        );
    }
}
