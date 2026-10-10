use super::match_keys_original as original;
use super::*;
use crate::perf::measure;
use std::cell::RefCell;
use std::hint::black_box;

#[path = "../../../tests/support/paired_bench.rs"]
mod paired_bench;

#[test]
fn ascii_match_keys_preserve_every_two_byte_input_and_capacity_bound() {
    for first in 0..128_u8 {
        for second in 0..128_u8 {
            let bytes = [first, second];
            let text = std::str::from_utf8(&bytes).unwrap();
            let old = original::match_key(text);
            let new = match_key(text);
            assert_eq!(new, old);
            assert!(new.capacity() <= old.capacity());
        }
    }
}

#[test]
fn match_keys_keep_unicode_filter_order_and_punctuation_fallbacks() {
    for text in [
        "",
        "   ",
        " !!! ",
        "V.L.S.I.",
        "ΟΣ",
        "İ",
        "İstanbul",
        "東京",
        "\u{307}",
        "💃",
        "A\u{307} B",
        "\0\t!",
    ] {
        assert_eq!(match_key(text), original::match_key(text), "{text:?}");
    }
    assert_eq!(match_key("İ"), "i", "filter after lowercase expansion");
    assert_eq!(match_key("ΟΣ"), "οσ", "per-character casing");
    assert_eq!(match_key(" !!! "), "!!!");
    for code in (0..=0x10ffff).step_by(97) {
        if let Some(ch) = char::from_u32(code) {
            let text = format!("Mix-{ch}-TITLE 42!");
            assert_eq!(match_key(&text), original::match_key(&text));
        }
    }
}

fn fixture(count: usize, unicode: bool) -> (PackIndex, Vec<SimfileTags>) {
    let tags = (0..count)
        .map(|i| SimfileTags {
            title: if unicode {
                format!("東京 Title {i}")
            } else {
                format!("Catalog Title {i}")
            },
            translit: format!("Roman Title {i}"),
            artist: format!("Artist {}", i % 4),
            music: format!("music-{i}.ogg"),
            preview: format!("preview-{i}.ogg"),
        })
        .collect();
    let index = PackIndex {
        pack_id: 1,
        total: 0,
        etag: None,
        entries: Vec::new(),
        tail_start: 0,
        tail: Arc::from([]),
        folders: (0..count)
            .map(|i| SongFolder {
                name: if unicode {
                    format!("曲 Folder {i}")
                } else {
                    format!("Folder {i}")
                },
                entries: vec![i],
                simfile: Some(i),
                audio: Vec::new(),
            })
            .collect(),
    };
    (index, tags)
}

fn check_match(index: &PackIndex, tags: &[SimfileTags], title: &str, artist: &str) {
    let old_calls = RefCell::new(Vec::new());
    let new_calls = RefCell::new(Vec::new());
    let old = original::match_song(index, title, artist, |i| {
        old_calls.borrow_mut().push(i);
        tags.get(i)
    });
    let new = match_song(index, title, artist, |i| {
        new_calls.borrow_mut().push(i);
        tags.get(i)
    });
    assert_eq!(new, old, "{title:?} / {artist:?}");
    assert_eq!(new_calls.into_inner(), old_calls.into_inner());
    assert_eq!(
        folders_needing_tags(index, title, artist),
        original::folders_needing_tags(index, title, artist)
    );
}

#[test]
fn normalization_preserves_matching_and_requested_metadata_order() {
    let (mut index, mut tags) = fixture(12, false);
    index.folders[0].name = "Song".into();
    index.folders[1].name = "S.O.N.G.".into();
    index.folders[2].name = "Song (Remix)".into();
    index.folders[3].name = "東京".into();
    index.folders[4].name = "!!!".into();
    tags[0].title = "Song".into();
    tags[1].title = "Song".into();
    tags[3].translit = "Tokyo".into();
    tags.pop(); // Missing metadata remains an observable callback result.
    for title in [
        "",
        "Song",
        "S.O.N.G.",
        "Song (Long Remix)",
        "Tokyo",
        "東京",
        "!!!",
        "Missing",
        "Catalog Title 9",
        "Roman Title 8",
        "Folder 11",
        "x",
    ] {
        for artist in ["", "Artist 0", "Artist 1", "Unknown", "!!!"] {
            check_match(&index, &tags, title, artist);
        }
    }
}

#[test]
fn ascii_match_keys_allocate_once_with_no_unused_capacity() {
    for text in [
        "x",
        "V.L.S.I.",
        "Catalog Title 112",
        "[1234] [14] Long Song Title (Expert)",
    ] {
        let (old, before) = measure(|| original::match_key(text));
        let (new, after) = measure(|| match_key(text));
        assert_eq!(new, old);
        assert_eq!(new.capacity(), new.len());
        assert!(new.capacity() <= old.capacity());
        assert_eq!(after.allocs, 1);
        assert_eq!(after.reallocs, 0);
        assert!(after.allocated_bytes <= before.allocated_bytes);
    }
}

#[test]
#[ignore = "paired release benchmark; run explicitly with --ignored --nocapture"]
fn benchmark_song_match_keys() {
    for (label, text) in [
        ("empty", String::new()),
        ("lower", "catalogtitle112".into()),
        ("ascii", "[1234] [14] Long Song Title (Expert)".into()),
        ("punctuation", "  ! ... / ? [] ( )  ".into()),
        ("greek", "ΟΣ ΣΟΣ - Ελληνικά".into()),
        ("japanese", "東京 音楽ゲーム １２３".into()),
        ("mixed", "StepMania - İstanbul & Été 2026".into()),
        ("late-unicode", format!("{}Ω", "Ascii Title ".repeat(400))),
    ] {
        let (old, before) = measure(|| original::match_key(&text));
        let (new, after) = measure(|| match_key(&text));
        assert_eq!(new, old);
        println!("key-{label} churn: original {before:?}, current {after:?}");
        println!(
            "key-{label} retained capacity: original {}, current {}",
            old.capacity(),
            new.capacity()
        );
        paired_bench::compare(&format!("key-{label}"), 100, |current| {
            black_box(if current {
                match_key(black_box(&text))
            } else {
                original::match_key(black_box(&text))
            });
        });
    }
    for (count, unicode, title, artist) in [
        (0, false, "Song", ""),
        (24, false, "Folder 12", ""),
        (200, false, "Catalog Title 112", ""),
        (1200, false, "Catalog Title 112", ""),
        (200, false, "Missing Song", "Artist 0"),
        (200, true, "東京 Title 112", ""),
    ] {
        let (index, tags) = fixture(count, unicode);
        let label = format!(
            "key-match-{count}-{}-{}",
            if unicode { "unicode" } else { "ascii" },
            title.replace(' ', "-")
        );
        check_match(&index, &tags, title, artist);
        let (old, before) =
            measure(|| original::match_song(&index, title, artist, |i| tags.get(i)));
        let (new, after) = measure(|| match_song(&index, title, artist, |i| tags.get(i)));
        assert_eq!(new, old);
        println!("{label} churn: original {before:?}, current {after:?}");
        paired_bench::compare(&label, 20, |current| {
            black_box(if current {
                match_song(
                    black_box(&index),
                    black_box(title),
                    black_box(artist),
                    |i| tags.get(i),
                )
            } else {
                original::match_song(
                    black_box(&index),
                    black_box(title),
                    black_box(artist),
                    |i| tags.get(i),
                )
            });
        });
    }
}
