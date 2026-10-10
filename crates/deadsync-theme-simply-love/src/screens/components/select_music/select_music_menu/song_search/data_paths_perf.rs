use super::*;
use crate::{data_paths_perf_support::compare, perf::assert_no_churn};
use std::hint::black_box;

include!("data_paths_original.rs");

fn open(query: &str) -> SongSearchOpen {
    let SongSearchState::Open(mut open) = begin_song_search() else {
        unreachable!()
    };
    open.query = query.to_owned();
    open.selected_index = 7;
    open.blink_t = 0.25;
    open
}

#[test]
fn data_paths_delete_word_preserves_unicode_and_whitespace() {
    let parts = [
        "",
        " ",
        "\t\r\n",
        "\u{85}",
        "\u{a0}",
        "\u{2003}",
        "\u{2028}",
        "\u{2029}",
        "\u{3000}",
        "\u{200b}",
        "\u{feff}",
        "alpha",
        "日本語",
        "déjà",
        "e\u{301}",
        "🎵",
        "a\0b",
    ];
    for a in parts {
        for b in parts {
            for c in parts {
                let query = format!("{a}{b}{c}");
                let mut old = open(&query);
                let mut new = open(&query);
                loop {
                    let before = original_song_search_delete_word(&mut old);
                    let after = song_search_delete_word(&mut new);
                    assert_eq!((before, &old.query), (after, &new.query), "{query:?}");
                    assert_eq!((new.selected_index, new.blink_t), (7, 0.25));
                    if !before {
                        break;
                    }
                }
            }
        }
    }
}

#[test]
fn data_paths_delete_word_retains_query_allocation() {
    let mut state = open("日本語 alpha   ");
    let pointer = state.query.as_ptr();
    let capacity = state.query.capacity();
    let mut changed = false;
    assert_no_churn(|| changed = song_search_delete_word(&mut state));
    assert!(changed);
    assert_eq!(state.query, "日本語 ");
    assert_eq!(state.query.as_ptr(), pointer);
    assert_eq!(state.query.capacity(), capacity);
}

#[test]
#[ignore = "paired release benchmark; run explicitly"]
fn benchmark_data_paths_delete_word() {
    for (label, query) in [
        ("empty", String::new()),
        ("one-word", "Epidermis".into()),
        ("ascii", "The Quick Brown Fox   ".into()),
        ("unicode", "日本語 déjà e\u{301} 🎵\u{3000}".into()),
        ("80-chars", "word ".repeat(16)),
        ("long-prefix", format!("{}tail", "前置 ".repeat(1024))),
    ] {
        let mut old = open(&query);
        let mut new = open(&query);
        // Restore input in the existing buffer. Repeated edits model typing /
        // deletion and include identical clear + append work in both variants.
        compare(
            &format!("delete/{label}"),
            || {
                old.query.clear();
                old.query.push_str(black_box(&query));
                black_box(original_song_search_delete_word(black_box(&mut old)));
                black_box(&old.query);
            },
            || {
                new.query.clear();
                new.query.push_str(black_box(&query));
                black_box(song_search_delete_word(black_box(&mut new)));
                black_box(&new.query);
            },
        );
    }
}
