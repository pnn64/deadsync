// Reference routines frozen from 5aab5d460 / 0.5.1137.
use super::*;
use std::hint::black_box;

fn legacy_bpm_grouped_songs(mut songs: Vec<Arc<SongData>>) -> Vec<GroupedSongs> {
    songs.sort_by(|left, right| {
        song_bpm_for_sort(left)
            .cmp(&song_bpm_for_sort(right))
            .then_with(|| song_title_cmp(left, right))
    });
    grouped_contiguous_songs(songs, |song| {
        let (lo, hi) = bpm_bucket_range(song_bpm_for_sort(song));
        SongSortGroup::Bpm { lo, hi }
    })
}

fn legacy_alpha_grouped_songs(
    songs: Vec<Arc<SongData>>,
    bucket_for: impl Fn(&SongData) -> u8,
    compare: impl Fn(&SongData, &SongData) -> Ordering,
    group_for: impl Fn(u8) -> SongSortGroup,
) -> Vec<GroupedSongs> {
    let mut buckets: [Vec<Arc<SongData>>; ALPHA_GROUP_COUNT] = std::array::from_fn(|_| Vec::new());
    for song in songs {
        buckets[usize::from(bucket_for(&song))].push(song);
    }
    let group_count = buckets.iter().filter(|songs| !songs.is_empty()).count();
    let mut groups = Vec::with_capacity(group_count);
    for (bucket, mut songs) in buckets.into_iter().enumerate() {
        if songs.is_empty() {
            continue;
        }
        songs.sort_by(|left, right| compare(left, right));
        groups.push(GroupedSongs {
            group: group_for(bucket as u8),
            songs,
        });
    }
    groups
}

fn legacy_title_grouped_songs(songs: Vec<Arc<SongData>>) -> Vec<GroupedSongs> {
    legacy_alpha_grouped_songs(
        songs,
        title_group_bucket,
        |left, right| {
            song_title_cmp(left, right)
                .then_with(|| left.title.cmp(&right.title))
                .then_with(|| left.subtitle.cmp(&right.subtitle))
        },
        SongSortGroup::Title,
    )
}

fn legacy_artist_grouped_songs(songs: Vec<Arc<SongData>>) -> Vec<GroupedSongs> {
    legacy_alpha_grouped_songs(
        songs,
        |song| alpha_group_bucket_from_text(&song.artist),
        |left, right| {
            cmp_ignore_ascii_case(&left.artist, &right.artist)
                .then_with(|| {
                    cmp_ignore_ascii_case(
                        left.simfile_path.to_string_lossy().as_ref(),
                        right.simfile_path.to_string_lossy().as_ref(),
                    )
                })
                .then_with(|| song_title_cmp(left, right))
        },
        SongSortGroup::Artist,
    )
}

fn library_fixture(count: usize, tagged: bool, skewed: bool) -> Vec<Arc<SongData>> {
    let prefixes = [
        "",
        "  ",
        "!",
        "7",
        "?",
        "\u{2003}A",
        "z",
        "A",
        "b",
        "C",
        "D",
        "E",
        "F",
        "G",
        "H",
        "I",
        "J",
        "K",
        "L",
        "M",
        "N",
        "O",
        "P",
        "Q",
        "R",
        "S",
        "T",
        "U",
        "V",
        "W",
        "X",
        "Y",
    ];
    let mut songs: Vec<_> = (0..count)
        .map(|i| {
            let mut song = test_song();
            let n = i / 3; // Equal keys in distinct Arcs exercise stable ordering.
            let prefix = if skewed {
                "A"
            } else {
                prefixes[n % prefixes.len()]
            };
            song.title = format!("{prefix}song {:05}", n % 211);
            song.artist = format!("{prefix}artist {:04}", n % 93);
            song.subtitle = if n % 7 == 0 { "Mix" } else { "mix" }.into();
            song.simfile_path = format!("Pack/{:05}/song.ssc", n % 357).into();
            if n % 13 == 0 {
                song.translit_title = "  Translated".into();
            }
            if tagged {
                song.display_bpm = match n % 19 {
                    0 => "*".into(),
                    1 => "".into(),
                    2 => "nonsense".into(),
                    3 => "  :  ".into(),
                    4 => "NaN".into(),
                    5 => "-25".into(),
                    _ => format!(" {} : {}.75 ", 60 + n % 90, 90 + n % 290),
                };
            }
            song.min_bpm = 60.0 + (n % 90) as f64;
            song.max_bpm = 90.0 + (n % 290) as f64;
            Arc::new(song)
        })
        .collect();
    let mut state = 0x9564_3321_u64;
    for i in (1..songs.len()).rev() {
        state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
        songs.swap(i, (state >> 32) as usize % (i + 1));
    }
    songs
}

fn assert_same_groups(actual: &[GroupedSongs], expected: &[GroupedSongs]) {
    assert_eq!(actual.len(), expected.len());
    for (actual, expected) in actual.iter().zip(expected) {
        assert_eq!(actual.group, expected.group);
        assert_eq!(actual.songs.len(), expected.songs.len());
        for (actual, expected) in actual.songs.iter().zip(&expected.songs) {
            assert!(
                Arc::ptr_eq(actual, expected),
                "song identity or stable order changed"
            );
        }
    }
}

#[test]
fn cached_bpm_preserves_groups_and_stable_identity() {
    for count in [0, 1, 2, 31, 63, 64, 65, 127, 256, 2048] {
        for tagged in [false, true] {
            let mut songs = library_fixture(count, tagged, false);
            for _ in 0..3 {
                assert_same_groups(
                    &bpm_grouped_songs(songs.clone()),
                    &legacy_bpm_grouped_songs(songs.clone()),
                );
                // Exercise the fallback used when source positions exceed u32.
                assert_same_groups(
                    &bpm_grouped_songs_uncached(songs.clone()),
                    &legacy_bpm_grouped_songs(songs.clone()),
                );
                songs.reverse();
                if count > 2 {
                    songs.rotate_left(count / 3);
                }
            }
        }
    }
}

#[test]
fn reserved_alpha_groups_preserve_unicode_ties_and_order() {
    for count in [0, 1, 2, 31, 64, 256, 2048] {
        for skewed in [false, true] {
            let songs = library_fixture(count, true, skewed);
            assert_same_groups(
                &title_grouped_songs(songs.clone()),
                &legacy_title_grouped_songs(songs.clone()),
            );
            assert_same_groups(
                &artist_grouped_songs(songs.clone()),
                &legacy_artist_grouped_songs(songs.clone()),
            );
            for groups in [
                title_grouped_songs(songs.clone()),
                artist_grouped_songs(songs.clone()),
            ] {
                for group in groups {
                    assert_eq!(group.songs.len(), group.songs.capacity());
                }
            }
        }
    }
}

#[test]
fn library_groups_do_not_reallocate() {
    let songs = library_fixture(2048, true, false);
    // Includes the consumed input copy, stable-sort scratch where needed, and outputs.
    crate::perf::assert_churn_budget(64, 100_000, || {
        black_box(bpm_grouped_songs(songs.clone()));
    });
    crate::perf::assert_churn_budget(64, 100_000, || {
        black_box(title_grouped_songs(songs.clone()));
    });
    crate::perf::assert_churn_budget(64, 100_000, || {
        black_box(artist_grouped_songs(songs.clone()));
    });
    crate::perf::assert_no_churn(|| {
        black_box(bpm_grouped_songs(Vec::new()));
        black_box(title_grouped_songs(Vec::new()));
        black_box(artist_grouped_songs(Vec::new()));
    });
}

#[test]
#[ignore = "manual old/new release benchmark"]
fn library_sort_bench() {
    for (label, count, tagged, skewed) in [
        ("small", 32, true, false),
        ("medium", 2048, true, false),
        ("large", 8192, true, false),
        ("untagged", 2048, false, false),
        ("sparse_tag", 2048, false, false),
        ("skewed", 2048, true, true),
    ] {
        let mut songs = library_fixture(count, tagged, skewed);
        if label == "sparse_tag" {
            Arc::make_mut(&mut songs[0]).display_bpm = "120:180".into();
        }
        for kind in ["bpm", "title", "artist"] {
            let reverse = std::env::var_os("DEADSYNC_PERF_REVERSE").is_some();
            for old in [!reverse, reverse] {
                let name = format!("library_{kind}_{label}_{}", if old { "old" } else { "new" });
                crate::perf::measure_sampled(
                    &name,
                    if count < 100 { 2000 } else { 64 },
                    count,
                    || {
                        let input = black_box(&songs).clone();
                        match (kind, old) {
                            ("bpm", true) => legacy_bpm_grouped_songs(input),
                            ("bpm", false) => bpm_grouped_songs(input),
                            ("title", true) => legacy_title_grouped_songs(input),
                            ("title", false) => title_grouped_songs(input),
                            ("artist", true) => legacy_artist_grouped_songs(input),
                            ("artist", false) => artist_grouped_songs(input),
                            _ => unreachable!(),
                        }
                    },
                );
            }
        }
    }
}
