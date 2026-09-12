use deadsync_chart::SongData;
use deadsync_simfile::playlist::{
    self as lookup, PlaylistEntry, PlaylistSongLookup, PlaylistSongSource,
};
use deadsync_theme_simply_love::screens::select_music::MusicWheelEntry;
use deadsync_theme_simply_love::views::SelectMusicPlaylistView;
use std::{hint::black_box, path::PathBuf, sync::Arc};

#[path = "playlist_preparation/baseline_library.rs"]
mod baseline_library;
#[path = "playlist_preparation/baseline_song.rs"]
mod baseline_song;
#[path = "playlist_preparation/fixtures.rs"]
mod fixtures;
#[path = "../../../tests/support/perf.rs"]
#[allow(dead_code)]
mod perf;
#[path = "../src/screens/select_music/playlists.rs"]
mod playlists;

fn sources(count: usize, groups: usize, aliases: bool, lobby: bool) -> Vec<PlaylistSongSource> {
    (0..count)
        .map(|index| {
            let pack = format!("Pack{}", index * groups / count);
            PlaylistSongSource {
                group_name: Some(if aliases {
                    format!("Display{}", index * groups / count)
                } else {
                    pack.clone()
                }),
                song: fixtures::song(&pack, &format!("Song{index}"), &format!("Title{index}")),
                lobby_path: lobby.then(|| format!("{pack}/Song{index}")),
            }
        })
        .collect()
}

fn wheel(sources: &[PlaylistSongSource]) -> Vec<MusicWheelEntry> {
    let mut entries = Vec::new();
    let mut last_group = None;
    for source in sources {
        if let Some(group) = &source.group_name
            && last_group != Some(group.as_str())
        {
            last_group = Some(group.as_str());
            entries.push(MusicWheelEntry::PackHeader {
                name: Arc::from(group.as_str()),
                original_index: entries.len(),
                banner_path: None,
                song_count: 1,
                pack_key: Some(Arc::from(group.as_str())),
                parent_series: None,
            });
        }
        entries.push(MusicWheelEntry::Song(source.song.clone()));
    }
    entries
}

fn assert_entries_equal(old: &[PlaylistEntry], new: &[PlaylistEntry]) {
    assert_eq!(old.len(), new.len());
    for (old, new) in old.iter().zip(new) {
        match (old, new) {
            (
                PlaylistEntry::Header {
                    name: a,
                    song_count: ac,
                },
                PlaylistEntry::Header {
                    name: b,
                    song_count: bc,
                },
            ) => assert_eq!((a, ac), (b, bc)),
            (PlaylistEntry::Song(a), PlaylistEntry::Song(b)) => assert!(Arc::ptr_eq(a, b)),
            _ => panic!("playlist entry kind differs"),
        }
    }
}

fn assert_wheel_equal(old: &[MusicWheelEntry], new: &[MusicWheelEntry]) {
    assert_eq!(old.len(), new.len());
    for (old, new) in old.iter().zip(new) {
        match (old, new) {
            (MusicWheelEntry::Song(a), MusicWheelEntry::Song(b)) => assert!(Arc::ptr_eq(a, b)),
            (
                MusicWheelEntry::PackHeader {
                    name: a,
                    original_index: ai,
                    song_count: ac,
                    banner_path: ab,
                    pack_key: ak,
                    parent_series: ap,
                },
                MusicWheelEntry::PackHeader {
                    name: b,
                    original_index: bi,
                    song_count: bc,
                    banner_path: bb,
                    pack_key: bk,
                    parent_series: bp,
                },
            ) => {
                assert_eq!((a, ai, ac, ab, ak, ap), (b, bi, bc, bb, bk, bp));
            }
            _ => panic!("wheel entry kind differs"),
        }
    }
}

fn unusual_sources() -> Vec<PlaylistSongSource> {
    let mut result = sources(24, 4, true, true);
    result.push(PlaylistSongSource {
        group_name: Some(" Pack0 ".into()),
        song: fixtures::song("Pack0", "Song0", "duplicate directory"),
        lobby_path: Some(" /PACK0\\Song0// ".into()),
    });
    result.push(PlaylistSongSource {
        group_name: Some("Pack0".into()),
        song: fixtures::song("Other", "Different", "duplicate path"),
        lobby_path: Some("Pack0/Song0".into()),
    });
    result.push(PlaylistSongSource {
        group_name: Some("  ".into()),
        song: fixtures::song("   ", "Song", "blank pack"),
        lobby_path: Some("////".into()),
    });
    result.push(PlaylistSongSource {
        group_name: None,
        song: fixtures::song("Fallback", "Alone", "no group"),
        lobby_path: None,
    });
    result.push(PlaylistSongSource {
        group_name: Some(" \u{2003} ".into()),
        song: fixtures::song("Fallback", "NoGroup", "whitespace group"),
        lobby_path: None,
    });
    result.push(PlaylistSongSource {
        group_name: Some("M\u{00dc}SIC".into()),
        song: fixtures::song("M\u{00dc}SIC", "\u{66f2}", "unicode"),
        lobby_path: Some("M\u{00dc}SIC/\u{66f2}".into()),
    });
    let mut invalid = (*fixtures::song("Unused", "Unused", "rootless")).clone();
    invalid.simfile_path = PathBuf::from("song.ssc");
    result.push(PlaylistSongSource {
        group_name: Some("Rootless".into()),
        song: Arc::new(invalid),
        lobby_path: None,
    });
    result
}

#[test]
fn lookup_preserves_aliases_duplicates_first_winner_and_wildcard_order() {
    for input in [
        Vec::new(),
        sources(1, 1, false, true),
        unusual_sources(),
        sources(257, 9, false, false),
    ] {
        let old = baseline_song::build_playlist_song_lookup(input.iter().cloned());
        let new = lookup::build_playlist_song_lookup(input.iter().cloned());
        let mut queries = vec![
            "Pack0/*".to_string(),
            "Display0/*".into(),
            "missing/*".into(),
            "Pack0/Song0".into(),
            "   /*".into(),
            "Rootless/*".into(),
        ];
        for source in &input {
            if let Some(path) = &source.lobby_path {
                queries.push(path.clone());
            }
            if let Some(group) = &source.group_name {
                queries.push(format!("{group}/*"));
            }
            if let Some((pack, song)) = lookup::song_pack_and_dir_name(&source.song) {
                queries.push(format!("{pack}/{song}"));
                queries.push(format!("{pack}/*"));
            }
        }
        for text in queries {
            for query in [
                &text,
                &text.to_ascii_lowercase(),
                &text.to_ascii_uppercase(),
                &text.replace('/', "\\\\"),
            ] {
                assert_entries_equal(
                    &baseline_song::playlist_entries_from_text(query, "Fallback", &old),
                    &lookup::playlist_entries_from_text(query, "Fallback", &new),
                );
            }
        }
        let old = baseline_song::playlist_entries_from_text("Pack0/Song0\n", "", &old);
        let new = lookup::playlist_entries_from_text("Pack0/Song0\n", "", &new);
        assert_entries_equal(&old, &new);
        if input.len() > 1 && input[0].lobby_path.is_some() {
            assert!(
                matches!(&new[1], PlaylistEntry::Song(song) if Arc::ptr_eq(song, &input[0].song))
            );
        }
    }
}

#[test]
fn streaming_sections_match_every_utf8_truncation_and_empty_header_run() {
    let input = unusual_sources();
    let old = baseline_song::build_playlist_song_lookup(input.iter().cloned());
    let new = lookup::build_playlist_song_lookup(input);
    let text = "\u{feff}---not a header\r\nPack0/Song0\n--- Empty\nmissing\n---\u{2003}\nDisplay0/*\n---\u{66f2}\nM\u{00dc}SIC/\u{66f2}\nPack0/*\n---\n---Unused\n\n---Again\nPack0/Song0\nPack0/Song0\n---Empty tail";
    for end in (0..=text.len()).filter(|end| text.is_char_boundary(*end)) {
        for fallback in ["Fallback", "  Fallback  ", ""] {
            assert_entries_equal(
                &baseline_song::playlist_entries_from_text(&text[..end], fallback, &old),
                &lookup::playlist_entries_from_text(&text[..end], fallback, &new),
            );
        }
    }
    for sections in [0, 1, 2, 17, 512] {
        let text = (0..sections)
            .map(|i| format!("--- Section {i}\nmissing\n---\nPack0/*\n"))
            .collect::<String>();
        assert_entries_equal(
            &baseline_song::playlist_entries_from_text(&text, "", &old),
            &lookup::playlist_entries_from_text(&text, "", &new),
        );
    }
}

#[test]
fn playlist_library_matches_menu_sorting_and_complete_wheel_entries() {
    let input = wheel(&unusual_sources());
    let roots = [PathBuf::from("/songs")];
    let views = [
        SelectMusicPlaylistView {
            id: "1".into(),
            owner: None,
            name: "z".into(),
            text: "Pack0/*\n---Final\nOther/Different\n".into(),
        },
        SelectMusicPlaylistView {
            id: "2".into(),
            owner: Some("Alice".into()),
            name: "\u{66f2}".into(),
            text: "Display0/*\n".into(),
        },
        SelectMusicPlaylistView {
            id: "3".into(),
            owner: Some("ALICE".into()),
            name: "\u{66f2}".into(),
            text: "---Empty\nmissing\n".into(),
        },
        SelectMusicPlaylistView {
            id: "1".into(),
            owner: None,
            name: "A".into(),
            text: "Pack0/Song0\n".into(),
        },
    ];
    for len in 0..=views.len() {
        let old = baseline_library::build_playlist_library(&input, &views[..len], &roots);
        let new = playlists::build_playlist_library(&input, &views[..len], &roots);
        assert_eq!(old.len(), new.len());
        for (old, new) in old.iter().zip(&new) {
            assert_eq!(
                (
                    &old.menu_entry.id,
                    &old.menu_entry.top_label,
                    &old.menu_entry.bottom_label
                ),
                (
                    &new.menu_entry.id,
                    &new.menu_entry.top_label,
                    &new.menu_entry.bottom_label
                )
            );
            assert_wheel_equal(&old.entries, &new.entries);
        }
    }
}

#[test]
fn empty_libraries_and_unused_headers_have_no_churn_and_sections_have_bounded_storage() {
    let input = sources(1024, 1, false, true);
    let entries = wheel(&input);
    perf::assert_no_churn(|| {
        assert!(playlists::build_playlist_library(&entries, &[], &[]).is_empty());
    });
    let lookup = lookup::build_playlist_song_lookup(input);
    let headers = "---Unused section\n".repeat(1024);
    perf::assert_no_churn(|| {
        assert!(lookup::playlist_entries_from_text(&headers, "fallback", &lookup).is_empty());
    });
    perf::assert_churn_budget(
        4,
        (1024 + 1) * std::mem::size_of::<PlaylistEntry>()
            + 1024 * std::mem::size_of::<Arc<SongData>>()
            + "Warmup".len()
            + 8, // String scratch starts with an eight-byte minimum capacity.
        || {
            let entries =
                lookup::playlist_entries_from_text("---Warmup\nPack0/*\n", "fallback", &lookup);
            assert_eq!(entries.len(), 1025);
        },
    );
}

#[cfg(windows)]
#[test]
fn non_utf8_song_directories_preserve_group_only_resolution() {
    use std::os::windows::ffi::OsStringExt;
    let mut song = (*fixtures::song("Pack", "Song", "invalid utf16")).clone();
    song.simfile_path = PathBuf::from(std::ffi::OsString::from_wide(&[
        80, 97, 99, 107, 47, 0xd800, 47, 115, 46, 115, 115, 99,
    ]));
    let source = PlaylistSongSource {
        group_name: Some("Group".into()),
        song: Arc::new(song),
        lobby_path: None,
    };
    let old = baseline_song::build_playlist_song_lookup([source.clone()]);
    let new = lookup::build_playlist_song_lookup([source]);
    for text in ["Group/*", "Pack/*", "Pack/Song", "Group/Song"] {
        assert_entries_equal(
            &baseline_song::playlist_entries_from_text(text, "", &old),
            &lookup::playlist_entries_from_text(text, "", &new),
        );
    }
}

#[test]
fn section_visitors_can_consume_prefixes_and_release_pending_handles() {
    let input = unusual_sources();
    let old = old_index(&input);
    let new = new_index(&input);
    let text = "---One\nPack0/*\n---Empty\n---Two\nDisplay0/*\nPack0/Song0\n";
    let expected = baseline_song::playlist_entries_from_text(text, "", &old);
    let owners: Vec<_> = input
        .iter()
        .map(|source| Arc::strong_count(&source.song))
        .collect();
    for count in [0, 1, 2, 128] {
        let mut actual = Vec::new();
        lookup::for_each_playlist_section(text, "", &new, |name, songs| {
            actual.push(PlaylistEntry::Header {
                name: name.to_owned(),
                song_count: songs.len(),
            });
            actual.extend(songs.take(count).map(PlaylistEntry::Song));
        });
        let mut kept = 0;
        let filtered: Vec<_> = expected
            .iter()
            .filter(|entry| match entry {
                PlaylistEntry::Header { .. } => {
                    kept = 0;
                    true
                }
                PlaylistEntry::Song(_) => {
                    kept += 1;
                    kept <= count
                }
            })
            .cloned()
            .collect();
        assert_entries_equal(&filtered, &actual);
        drop(filtered);
        drop(actual);
        for (source, owners) in input.iter().zip(&owners) {
            assert_eq!(Arc::strong_count(&source.song), *owners);
        }
    }
}

fn pair(name: &str, mut old: impl FnMut(&str), mut new: impl FnMut(&str)) {
    let a = format!("{name}_old");
    let b = format!("{name}_new");
    if std::env::var_os("DEADSYNC_PERF_REVERSE").is_some() {
        new(&b);
        old(&a);
    } else {
        old(&a);
        new(&b);
    }
}

fn old_index(input: &[PlaylistSongSource]) -> baseline_song::PlaylistSongLookup {
    baseline_song::build_playlist_song_lookup(input.iter().cloned())
}
fn new_index(input: &[PlaylistSongSource]) -> PlaylistSongLookup {
    lookup::build_playlist_song_lookup(input.iter().cloned())
}

// Adapted control removes just the empty-library guard, using the optimized
// index builder. Keep its work observable even though there are no views.
fn empty_library_without_guard(
    entries: &[MusicWheelEntry],
    views: &[SelectMusicPlaylistView],
    roots: &[PathBuf],
) -> Vec<playlists::PlaylistCacheEntry> {
    assert!(views.is_empty());
    black_box(playlists::build_playlist_song_lookup(entries, roots));
    Vec::new()
}

#[test]
#[ignore = "manual paired release benchmark; run alone with --nocapture --test-threads=1"]
fn playlist_preparation_bench() {
    for (name, count, groups, aliases, lobby) in [
        ("index_empty", 0, 1, false, false),
        ("index_single", 1, 1, false, true),
        ("index_grouped", 4096, 32, false, true),
        ("index_aliases", 4096, 32, true, true),
        ("index_no_lobby", 4096, 32, false, false),
        ("index_many_groups", 1024, 1024, false, true),
    ] {
        let input = sources(count, groups, aliases, lobby);
        let iterations = if count > 32 { 4 } else { 1024 };
        pair(
            name,
            |name| {
                perf::measure_sampled(name, iterations, count, || {
                    black_box(
                        old_index as fn(&[PlaylistSongSource]) -> baseline_song::PlaylistSongLookup,
                    )(black_box(&input))
                })
            },
            |name| {
                perf::measure_sampled(name, iterations, count, || {
                    black_box(new_index as fn(&[PlaylistSongSource]) -> PlaylistSongLookup)(
                        black_box(&input),
                    )
                })
            },
        );
    }
    let input = sources(4096, 32, false, true);
    let old = old_index(&input);
    let new = new_index(&input);
    let literal = input
        .iter()
        .map(|song| format!("{}\n", song.lobby_path.as_ref().unwrap()))
        .collect::<String>();
    let sectioned = input
        .iter()
        .enumerate()
        .map(|(i, song)| format!("--- Section {i}\n{}\n", song.lobby_path.as_ref().unwrap()))
        .collect::<String>();
    for (name, text) in [
        ("parse_empty", String::new()),
        ("parse_unused_headers", "--- Unused header\n".repeat(4096)),
        ("parse_literal", literal.clone()),
        ("parse_sections", sectioned),
        (
            "parse_wildcards",
            (0..32).map(|i| format!("Pack{i}/*\n")).collect(),
        ),
        ("parse_misses", "Missing/Unknown\n".repeat(4096)),
    ] {
        let units = text.lines().count();
        pair(
            name,
            |name| {
                perf::measure_sampled(name, 16, units, || {
                    black_box(
                        baseline_song::playlist_entries_from_text
                            as fn(
                                &str,
                                &str,
                                &baseline_song::PlaylistSongLookup,
                            ) -> Vec<PlaylistEntry>,
                    )(black_box(&text), "Fallback", black_box(&old))
                })
            },
            |name| {
                perf::measure_sampled(name, 16, units, || {
                    black_box(
                        lookup::playlist_entries_from_text
                            as fn(&str, &str, &PlaylistSongLookup) -> Vec<PlaylistEntry>,
                    )(black_box(&text), "Fallback", black_box(&new))
                })
            },
        );
    }
    for (name, text) in [
        ("wheel_literal", literal.clone()),
        (
            "wheel_sections",
            input
                .iter()
                .enumerate()
                .map(|(i, source)| {
                    format!("---Section {i}\n{}\n", source.lobby_path.as_ref().unwrap())
                })
                .collect(),
        ),
        (
            "wheel_wildcards",
            (0..32).map(|i| format!("Pack{i}/*\n")).collect(),
        ),
    ] {
        pair(
            name,
            |name| {
                perf::measure_sampled(name, 16, text.lines().count(), || {
                    black_box(
                        baseline_library::build_playlist_entries_from_text
                            as fn(
                                &str,
                                &str,
                                &baseline_song::PlaylistSongLookup,
                            ) -> Vec<MusicWheelEntry>,
                    )(black_box(&text), "Fallback", black_box(&old))
                });
            },
            |name| {
                perf::measure_sampled(name, 16, text.lines().count(), || {
                    black_box(
                        playlists::build_playlist_entries_from_text
                            as fn(&str, &str, &PlaylistSongLookup) -> Vec<MusicWheelEntry>,
                    )(black_box(&text), "Fallback", black_box(&new))
                });
            },
        );
    }
    let entries = wheel(&input);
    let roots = [PathBuf::from("/songs")];
    let views = [SelectMusicPlaylistView {
        id: "bench".into(),
        owner: None,
        name: "Bench".into(),
        text: literal,
    }];
    type Builder = fn(
        &[MusicWheelEntry],
        &[SelectMusicPlaylistView],
        &[PathBuf],
    ) -> Vec<playlists::PlaylistCacheEntry>;
    type OldBuilder = fn(
        &[MusicWheelEntry],
        &[SelectMusicPlaylistView],
        &[PathBuf],
    ) -> Vec<baseline_library::PlaylistCacheEntry>;
    for (name, views) in [
        ("library_none", &views[..0]),
        ("library_populated", &views[..]),
    ] {
        pair(
            name,
            |name| {
                perf::measure_sampled(name, 4, 1, || {
                    black_box(baseline_library::build_playlist_library as OldBuilder)(
                        black_box(&entries),
                        black_box(views),
                        black_box(&roots),
                    )
                })
            },
            |name| {
                perf::measure_sampled(name, if views.is_empty() { 4096 } else { 4 }, 1, || {
                    black_box(playlists::build_playlist_library as Builder)(
                        black_box(&entries),
                        black_box(views),
                        black_box(&roots),
                    )
                })
            },
        );
    }
    pair(
        "empty_guard_control",
        |name| {
            perf::measure_sampled(name, 4, 1, || {
                black_box(empty_library_without_guard as Builder)(
                    black_box(&entries),
                    black_box(&[]),
                    black_box(&roots),
                )
            })
        },
        |name| {
            perf::measure_sampled(name, 1024, 1, || {
                black_box(playlists::build_playlist_library as Builder)(
                    black_box(&entries),
                    black_box(&[]),
                    black_box(&roots),
                )
            })
        },
    );
}
