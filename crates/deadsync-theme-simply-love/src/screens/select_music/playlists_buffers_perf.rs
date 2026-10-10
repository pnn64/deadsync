use super::*;
use crate::buffers_support::compare;
use crate::perf::measure;
use std::hint::black_box;

fn fixture(count: usize, long: bool) -> Vec<SelectMusicPlaylistView> {
    (0..count)
        .map(|i| SelectMusicPlaylistView {
            id: i.to_string(),
            owner: (i % 3 != 0).then(|| format!("Player {}", (i * 41) % 17)),
            name: format!(
                "{} Mix {:04}",
                if long {
                    "Shared long prefix ".repeat(20)
                } else {
                    "Dance".into()
                },
                (i * 67) % 999
            ),
            text: String::new(),
        })
        .collect()
}

fn keys(entries: &[PlaylistCacheEntry]) -> Vec<&PlaylistMenuEntry> {
    entries.iter().map(|entry| &entry.menu_entry).collect()
}

#[test]
fn playlist_sort_preserves_ascii_unicode_and_stable_ties() {
    let labels = [
        "",
        "A",
        "a",
        "Z",
        " z",
        "a\0",
        "ß",
        "ẞ",
        "É",
        "é",
        "日本語",
        "🎵",
        "AB",
        "ab",
    ];
    let views: Vec<_> = (0..600)
        .map(|i| SelectMusicPlaylistView {
            id: i.to_string(),
            owner: (i % 4 != 0).then(|| labels[(i * 13) % labels.len()].into()),
            name: labels[(i * 7) % labels.len()].into(),
            text: String::new(),
        })
        .collect();
    let old = buffers_original::build_playlist_library(&[], &views, &[]);
    let new = build_playlist_library(&[], &views, &[]);
    assert_eq!(keys(&old), keys(&new));
    assert!(new.iter().all(|entry| entry.entries.is_empty()));
    for pair in new.windows(2) {
        let a = &pair[0].menu_entry;
        let b = &pair[1].menu_entry;
        if a.top_label.eq_ignore_ascii_case(&b.top_label)
            && a.bottom_label.eq_ignore_ascii_case(&b.bottom_label)
        {
            assert!(a.id.parse::<usize>().unwrap() < b.id.parse::<usize>().unwrap());
        }
    }
    crate::perf::assert_no_churn(|| {
        assert!(build_playlist_library(black_box(&[]), black_box(&[]), black_box(&[])).is_empty());
    });
}

#[test]
fn playlist_sort_avoids_per_entry_lowercase_key_allocations() {
    for count in [2, 16, 128, 1000] {
        let views = fixture(count, false);
        let (old, before) = measure(|| buffers_original::build_playlist_library(&[], &views, &[]));
        let (new, after) = measure(|| build_playlist_library(&[], &views, &[]));
        assert_eq!(keys(&old), keys(&new));
        assert!(before.allocs >= after.allocs + count * 2);
    }
}

#[test]
#[ignore = "paired release benchmark; run with --ignored --nocapture --test-threads=1"]
fn benchmark_view_buffers_playlists() {
    for (count, long) in [(16, false), (128, false), (1000, false), (128, true)] {
        let views = fixture(count, long);
        compare(
            &format!("playlists/{count}-{}", if long { "long" } else { "short" }),
            || {
                black_box(buffers_original::build_playlist_library(
                    black_box(&[]),
                    black_box(&views),
                    black_box(&[]),
                ));
            },
            || {
                black_box(build_playlist_library(
                    black_box(&[]),
                    black_box(&views),
                    black_box(&[]),
                ));
            },
        );
    }
}
