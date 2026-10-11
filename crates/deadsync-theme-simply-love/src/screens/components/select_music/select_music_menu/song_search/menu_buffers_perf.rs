use super::*;
use crate::menu_buffers_perf_support::compare;
use std::hint::black_box;

include!("menu_buffers_original.rs");

fn open(query: &str, label: &str) -> SongSearchOpen {
    let SongSearchState::Open(mut open) = begin_song_search() else {
        unreachable!()
    };
    open.query = query.to_owned();
    open.scope = SongSearchScope::Pack;
    open.matches = vec![SongSearchMatch::Pack {
        name: Arc::from(label),
        song_count: 12,
        score: 7,
    }];
    open
}

fn values(value: Option<SongSearchCompletion>) -> Option<(String, String, String)> {
    value.map(|v| (v.display, v.typed, v.accepted))
}

#[test]
fn menu_buffers_completion_preserves_unicode_ghost_and_acceptance() {
    let labels = [
        "automate",
        "Déjà Vu",
        "e\u{301}cole",
        "日本語🎵",
        "[12] [mix] automate (Hard)",
        "  automate  ",
        "\u{301}abc",
        "",
    ];
    for label in labels {
        for query in [
            "",
            "a",
            "auto",
            "auto ",
            "auto [10]",
            "[10] auto",
            "deja",
            "Dé",
            "e",
            "e\u{301}",
            "日本",
            "\u{301}",
            "[10]",
            "no match",
        ] {
            let state = open(query, label);
            assert_eq!(
                values(song_search_completion(&state)),
                values(original_song_search_completion(&state)),
                "{query:?} / {label:?}"
            );
        }
        for length in 0..=label.chars().count() {
            let query: String = label.chars().take(length).collect();
            let state = open(&query, label);
            assert_eq!(
                values(song_search_completion(&state)),
                values(original_song_search_completion(&state))
            );
        }
    }
    let state = open("deja", "Déjà Vu");
    assert_eq!(
        values(song_search_completion(&state)),
        Some(("deja Vu".into(), "deja".into(), "Déjà Vu".into()))
    );
}

#[test]
fn menu_buffers_completion_preserves_limits_and_missing_selection() {
    for size in [0, 1, 79, 80, 81, 1024] {
        let label = "🎵".repeat(size);
        for count in [0, 1, 79, 80, 81] {
            let mut state = open(&"🎵".repeat(count), &label);
            for index in [0, 1, usize::MAX] {
                state.selected_index = index;
                assert_eq!(
                    values(song_search_completion(&state)),
                    values(original_song_search_completion(&state))
                );
            }
        }
    }
}

#[test]
#[ignore = "paired release benchmark; run explicitly"]
fn benchmark_menu_buffers_completion() {
    for (label, query, title) in [
        ("empty", "", "automate".to_owned()),
        (
            "ascii",
            "a",
            "A long song title with a lengthy ghost completion".into(),
        ),
        ("accented", "deja", "Déjà Vu".into()),
        ("combining", "e", "e\u{301}cole after hours".into()),
        ("unicode", "日本", "日本語🎵の歌".into()),
        ("filter", "[12] auto", "automate".into()),
        ("exact", "automate", "automate".into()),
        ("reordered", "auto [12]", "automate".into()),
        ("limit", "🎵", "🎵".repeat(100)),
    ] {
        let state = open(query, &title);
        compare(
            &format!("completion/{label}"),
            || {
                black_box(original_song_search_completion(black_box(&state)));
            },
            || {
                black_box(song_search_completion(black_box(&state)));
            },
        );
    }
}
