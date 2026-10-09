use super::metadata_original as original;
use super::*;
use crate::perf::measure;
use std::cell::{Cell, RefCell};
use std::hint::black_box;

#[path = "../../../tests/support/paired_bench.rs"]
mod paired_bench;

fn entry(name: impl Into<String>, index: usize) -> ZipEntry {
    ZipEntry {
        name: name.into(),
        compressed: 20,
        uncompressed: 40,
        local_header: index as u64 * 100,
        end: (index as u64 + 1) * 100,
    }
}

fn assert_folders(left: &[SongFolder], right: &[SongFolder]) {
    assert_eq!(left.len(), right.len());
    for (left, right) in left.iter().zip(right) {
        assert_eq!(left.name, right.name);
        assert_eq!(left.entries, right.entries);
        assert_eq!(left.simfile, right.simfile);
        assert_eq!(left.audio, right.audio);
        assert_eq!(left.entries.capacity(), right.entries.capacity());
        assert_eq!(left.audio.capacity(), right.audio.capacity());
    }
}

fn archive(songs: usize, files: usize, deep: bool) -> Vec<ZipEntry> {
    (0..songs)
        .flat_map(|song| {
            (0..files).map(move |file| {
                let suffix = match file {
                    0 => "first.sm".to_owned(),
                    1 => "main.SSC".to_owned(),
                    2 => "music.ogg".to_owned(),
                    _ if deep => format!("effects/layer-{}/images/image-{file}.png", file % 4),
                    _ => format!("image-{file}.png"),
                };
                entry(
                    format!("Example Pack/Song {song}/{suffix}"),
                    song * files + file,
                )
            })
        })
        .collect()
}

#[test]
fn one_pass_grouping_keeps_order_and_first_direct_ssc_priority() {
    let names = [
        "Pack/Song/first.sm",
        "Pack/Song/second.SM",
        "Pack/Song/nested/ignored.ssc",
        "Pack/Song/._ignored.ssc",
        "Pack/Song/first.SSC",
        "Pack/Song/second.ssc",
        "Pack/Song/music.OGG",
        "Pack/Song/effects/clip.opus",
        "Pack/Song/._music.ogg",
        "Pack/Song/",
        "Pack/Song/effects/",
        "__macosx/Song/main.ssc",
        "Pack/banner.png",
        "Pack/Other/main.sm",
        "pack/SONG/last.mp3",
        "Pack/Other/Sub/only.ssc",
    ];
    let entries: Vec<_> = names
        .into_iter()
        .enumerate()
        .map(|(i, n)| entry(n, i))
        .collect();
    let actual = group_folders(&entries);
    assert_folders(&actual, &original::group_folders(&entries));
    assert_eq!(actual.len(), 2);
    assert_eq!(actual[0].name, "Song");
    assert_eq!(actual[0].simfile, Some(4));
    assert_eq!(actual[0].audio, [6, 7, 14]);
    assert_eq!(actual[1].simfile, Some(13));
}

#[test]
fn grouping_matches_original_for_unicode_separators_and_irregular_paths() {
    let roots = ["Pack", "PACK", "ΟΣ", "ος", "__MACOSX", "__macosx", ""];
    let folders = ["Song", "song", "東京", "İ", "i\u{307}", "..", ""];
    let files = [
        "main.sm",
        "main.SSC",
        "other.ssc",
        "sound.FLAC",
        "._main.ssc",
        "sub/main.ssc",
        "a/b/c/music.oga",
        "",
        "main.ssc/",
        ".ssc",
    ];
    let separators = ["/", "\\", "//", "\\\\", "/\\"];
    let mut seed = 917_u64;
    for trial in 0..40 {
        let mut entries = Vec::new();
        for i in 0..(trial * 13) {
            let mut pick = |count: usize| {
                seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
                ((seed >> 32) as usize) % count
            };
            let root = roots[pick(roots.len())];
            let folder = folders[pick(folders.len())];
            let file = files[pick(files.len())];
            let sep = separators[pick(separators.len())];
            entries.push(entry(format!("{root}{sep}{folder}{sep}{file}"), i));
        }
        let actual = group_folders(&entries);
        let expected = original::group_folders(&entries);
        assert_folders(&actual, &expected);
        assert_eq!(actual.capacity(), expected.capacity());
    }
}

#[test]
fn grouping_preserves_allocation_churn_and_retained_capacities() {
    for (songs, files, deep) in [(0, 0, false), (24, 4, false), (200, 12, true)] {
        let entries = archive(songs, files, deep);
        let (old, before) = measure(|| original::group_folders(&entries));
        let (new, after) = measure(|| group_folders(&entries));
        assert_folders(&new, &old);
        assert_eq!(new.capacity(), old.capacity());
        assert_eq!(before, after);
    }
}

fn matching_fixture(count: usize) -> (PackIndex, HashMap<usize, SimfileTags>) {
    let mut tags = HashMap::new();
    let folders = (0..count)
        .map(|i| {
            tags.insert(
                i,
                SimfileTags {
                    title: format!("Catalog Title {i}"),
                    translit: format!("Roman Title {i}"),
                    artist: format!("Artist {}", i % 4),
                    music: format!("music-{i}.ogg"),
                    preview: format!("preview-{i}.ogg"),
                },
            );
            SongFolder {
                name: format!("Folder {i}"),
                entries: vec![i],
                simfile: Some(i),
                audio: Vec::new(),
            }
        })
        .collect();
    (
        PackIndex {
            pack_id: 1,
            total: 0,
            etag: None,
            entries: Vec::new(),
            folders,
            tail_start: 0,
            tail: Arc::from([]),
        },
        tags,
    )
}

fn same_match(index: &PackIndex, tags: &HashMap<usize, SimfileTags>, title: &str, artist: &str) {
    let before = RefCell::new(Vec::new());
    let after = RefCell::new(Vec::new());
    let expected = original::match_song(index, title, artist, |i| {
        before.borrow_mut().push(i);
        tags.get(&i).cloned()
    });
    let actual = match_song(index, title, artist, |i| {
        after.borrow_mut().push(i);
        tags.get(&i)
    });
    assert_eq!(actual, expected, "title {title:?}, artist {artist:?}");
    assert_eq!(
        after.into_inner(),
        before.into_inner(),
        "callback order/count must remain observable"
    );
}

#[test]
fn borrowed_metadata_preserves_all_matching_rules_and_callback_order() {
    let (mut index, mut tags) = matching_fixture(12);
    index.folders[0].name = "Song".into();
    index.folders[1].name = "S.O.N.G.".into();
    index.folders[2].name = "Song (Remix)".into();
    index.folders[3].name = "東京".into();
    index.folders[4].name = "!!!".into();
    tags.get_mut(&0).unwrap().title = "Song".into();
    tags.get_mut(&1).unwrap().title = "Song".into();
    tags.get_mut(&3).unwrap().translit = "Tokyo".into();
    tags.remove(&5);
    let before = tags.clone();
    for title in [
        "",
        "Song",
        "S.O.N.G.",
        "Song (Long Remix)",
        "Tokyo",
        "東京",
        "!!!",
        "No Match",
        "Catalog Title 9",
        "Roman Title 8",
        "Folder 11",
        "x",
    ] {
        for artist in ["", "Artist 0", "Artist 1", "Unknown", "!!!"] {
            same_match(&index, &tags, title, artist);
        }
    }
    assert_eq!(tags, before);
}

#[test]
fn borrowed_metadata_preserves_ambiguous_artist_callback_side_effects() {
    let (_, tags) = matching_fixture(6);
    for candidates in [&[][..], &[0][..], &[0, 1][..], &[0, 4, 5][..]] {
        for artist in ["", "artist0", "artist3", "missing"] {
            let old_calls = RefCell::new(Vec::new());
            let new_calls = RefCell::new(Vec::new());
            let old = original::settle(candidates, artist, &|i| {
                old_calls.borrow_mut().push(i);
                tags.get(&i).cloned()
            });
            let new = settle(candidates, artist, &|i| {
                new_calls.borrow_mut().push(i);
                tags.get(&i)
            });
            assert_eq!(new, old);
            assert_eq!(new_calls.into_inner(), old_calls.into_inner());
        }
    }
}

#[test]
fn matching_removes_five_string_copies_per_cached_metadata_lookup() {
    let (index, tags) = matching_fixture(200);
    let old_calls = Cell::new(0);
    let new_calls = Cell::new(0);
    let (old, before) = measure(|| {
        original::match_song(&index, "Missing Song", "Artist 0", |i| {
            old_calls.set(old_calls.get() + 1);
            tags.get(&i).cloned()
        })
    });
    let (new, after) = measure(|| {
        match_song(&index, "Missing Song", "Artist 0", |i| {
            new_calls.set(new_calls.get() + 1);
            tags.get(&i)
        })
    });
    assert_eq!(new, old);
    assert_eq!(old_calls.get(), new_calls.get());
    assert!(old_calls.get() >= 200);
    assert_eq!(before.allocs - after.allocs, old_calls.get() * 5);
    assert!(after.allocated_bytes < before.allocated_bytes);
}

#[test]
#[ignore = "paired release benchmark; run explicitly with --ignored --nocapture"]
fn benchmark_archive_grouping() {
    for (songs, files, deep) in [
        (0, 0, false),
        (1, 4, false),
        (24, 4, false),
        (200, 12, false),
        (1200, 8, false),
        (200, 32, true),
    ] {
        let entries = archive(songs, files, deep);
        let label = format!(
            "group-{songs}x{files}-{}",
            if deep { "deep" } else { "flat" }
        );
        let (old, before) = measure(|| original::group_folders(&entries));
        let (new, after) = measure(|| group_folders(&entries));
        assert_folders(&new, &old);
        assert_eq!(before, after);
        println!("{label} churn: original {before:?}, current {after:?}");
        paired_bench::compare(&label, 20, |current| {
            black_box(if current {
                group_folders(black_box(&entries))
            } else {
                original::group_folders(black_box(&entries))
            });
        });
    }
}

#[test]
#[ignore = "paired release benchmark; run explicitly with --ignored --nocapture"]
fn benchmark_borrowed_song_metadata() {
    for (count, title, artist) in [
        (0, "Song", ""),
        (24, "Folder 12", ""),
        (24, "Catalog Title 12", ""),
        (200, "Catalog Title 112", ""),
        (1200, "Catalog Title 112", ""),
        (200, "Roman Title 112", ""),
        (200, "Missing Song", ""),
        (200, "Catalog Title 112", "Artist 0"),
        (200, "Missing Song", "Artist 0"),
    ] {
        let (index, tags) = matching_fixture(count);
        let label = format!(
            "match-{count}-{}-{}",
            title.replace(' ', "-"),
            if artist.is_empty() {
                "no-artist"
            } else {
                "artist"
            }
        );
        same_match(&index, &tags, title, artist);
        let (old, before) =
            measure(|| original::match_song(&index, title, artist, |i| tags.get(&i).cloned()));
        let (new, after) = measure(|| match_song(&index, title, artist, |i| tags.get(&i)));
        assert_eq!(old, new);
        println!("{label} churn: original {before:?}, current {after:?}");
        paired_bench::compare(&label, 20, |current| {
            black_box(if current {
                match_song(
                    black_box(&index),
                    black_box(title),
                    black_box(artist),
                    |i| tags.get(&i),
                )
            } else {
                original::match_song(
                    black_box(&index),
                    black_box(title),
                    black_box(artist),
                    |i| tags.get(&i).cloned(),
                )
            });
        });
    }
}
