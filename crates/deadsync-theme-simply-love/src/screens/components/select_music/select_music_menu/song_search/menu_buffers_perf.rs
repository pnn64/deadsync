use super::*;


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

fn assert_truthful_ghost(state: &SongSearchOpen) {
    if let Some(completion) = song_search_completion(state) {
        assert_eq!(completion.typed, state.query);
        assert!(completion.display.starts_with(&completion.typed));
        assert!(completion.display.len() > completion.typed.len());
    }
}

#[test]
fn completion_preserves_unicode_ghost_and_acceptance() {
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
            assert_truthful_ghost(&open(query, label));
        }
        for length in 0..=label.chars().count() {
            let query: String = label.chars().take(length).collect();
            assert_truthful_ghost(&open(&query, label));
        }
    }
    let state = open("deja", "Déjà Vu");
    assert_eq!(
        values(song_search_completion(&state)),
        Some(("deja Vu".into(), "deja".into(), "Déjà Vu".into()))
    );
}

#[test]
fn completion_rejects_empty_queries_and_missing_selection() {
    for size in [0, 1, 79, 80, 81, 1024] {
        let label = "\u{1f3b5}".repeat(size);
        for count in [0, 1, 79, 80, 81] {
            let mut state = open(&"\u{1f3b5}".repeat(count), &label);
            for index in [0, 1, usize::MAX] {
                state.selected_index = index;
                assert_truthful_ghost(&state);
                if index != 0 || count == 0 {
                    assert!(song_search_completion(&state).is_none());
                }
            }
        }
    }
}
