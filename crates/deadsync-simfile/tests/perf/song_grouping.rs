use super::*;
use crate::perf::{assert_churn_budget, assert_no_churn, measure_sampled};
use std::hint::black_box;

#[path = "song_grouping/baseline.rs"]
mod baseline;

type Group = fn(Vec<Arc<SongData>>) -> Vec<GroupedSongs>;
type NamedGroup = fn(Vec<Arc<SongData>>, &str) -> Vec<GroupedSongs>;
type Meters = fn(&SongData, &str) -> Vec<u32>;

fn assert_groups(old: &[GroupedSongs], new: &[GroupedSongs]) {
    assert_eq!(old.len(), new.len());
    for (old, new) in old.iter().zip(new) {
        assert_eq!(old.group, new.group);
        assert_eq!(old.songs.len(), new.songs.len());
        for (old, new) in old.songs.iter().zip(&new.songs) {
            assert!(
                Arc::ptr_eq(old, new),
                "song identity or stable order changed"
            );
        }
    }
}

fn fixture(count: usize, mode: &str) -> Vec<Arc<SongData>> {
    let mut songs: Vec<_> = (0..count)
        .map(|index| {
            let mut song = test_song();
            let key = index / 3;
            song.title = format!("Song{:05}", key % 151);
            song.simfile_path = format!("Pack/Song{}/chart.ssc", key % 113).into();
            song.translit_title = if key % 17 == 0 {
                "\u{2003}Translated"
            } else {
                ""
            }
            .into();
            song.subtitle = if key % 5 == 0 { "Mix" } else { "mix" }.into();
            song.genre = match mode {
                "one" => "Dance".into(),
                "many" => format!("Genre{index:05}"),
                "case" => [
                    "Rock", "rock", "ROCK", "", "  ", "\u{2003}", "Unknown", "\u{66f2}",
                ][index % 8]
                    .into(),
                _ => format!("Genre{}", key % 24),
            };
            song.music_length_seconds = match mode {
                "one" => 120.0 + (key % 59) as f32,
                "many" => (index * 60) as f32,
                _ => (key % 721) as f32 + 0.75,
            };
            song.min_bpm = 60.0;
            song.max_bpm = 60.0 + (key % 401) as f64;
            if mode == "tagged" {
                song.display_bpm = match key % 9 {
                    0 => "*".into(),
                    1 => "NaN".into(),
                    2 => "bad".into(),
                    _ => format!("90:{}.25", 90 + key % 330),
                };
            }
            song.charts = (0..5)
                .map(|chart| test_chart("Hard", (key as u32 + chart) % 20, true))
                .collect();
            Arc::new(song)
        })
        .collect();
    let mut state = 0x789a_abcd_u64;
    for index in (1..songs.len()).rev() {
        state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
        songs.swap(index, (state >> 32) as usize % (index + 1));
    }
    songs
}

#[test]
fn grouped_modes_preserve_metadata_boundaries_and_stable_identity() {
    for count in [0, 1, 2, 7, 31, 63, 64, 65, 128, 1025] {
        for mode in ["mixed", "one", "many", "case", "tagged"] {
            let mut songs = fixture(count, mode);
            if count > 7 {
                for (index, value) in [f32::NAN, f32::INFINITY, -10.0, -0.0, 0.0, 59.999, 60.0]
                    .into_iter()
                    .enumerate()
                {
                    let song = Arc::make_mut(&mut songs[index]);
                    song.music_length_seconds = value;
                    song.total_length_seconds = index as i32 - 3;
                }
            }
            for _ in 0..2 {
                assert_groups(
                    &baseline::bpm_grouped_songs(songs.clone()),
                    &bpm_grouped_songs(songs.clone()),
                );
                assert_groups(
                    &baseline::bpm_grouped_songs_uncached(songs.clone()),
                    &bpm_grouped_songs_uncached(songs.clone()),
                );
                assert_groups(
                    &baseline::length_grouped_songs(songs.clone()),
                    &length_grouped_songs(songs.clone()),
                );
                for unknown in ["Unknown", "", " rock ", "\u{2003}"] {
                    assert_groups(
                        &baseline::genre_grouped_songs(songs.clone(), unknown),
                        &genre_grouped_songs(songs.clone(), unknown),
                    );
                }
                assert_groups(
                    &baseline::meter_grouped_songs(songs.clone(), "DANCE-SINGLE"),
                    &meter_grouped_songs(songs.clone(), "DANCE-SINGLE"),
                );
                songs.reverse();
            }
        }
    }
}

#[test]
fn single_group_reuses_input_storage_and_trims_spare_capacity() {
    for count in [1, 16, 128, 4096] {
        let mut songs = fixture(count, "one");
        songs.shrink_to_fit();
        let original_pointer = songs.as_ptr();
        let expected = baseline::genre_grouped_songs(songs.clone(), "Unknown");
        let actual = genre_grouped_songs(songs, "Unknown");
        assert_groups(&expected, &actual);
        assert_eq!(actual[0].songs.as_ptr(), original_pointer);
        let mut oversized = fixture(count, "one");
        oversized.reserve(count + 17);
        let actual = genre_grouped_songs(oversized, "Unknown");
        assert_eq!(actual[0].songs.len(), count);
        assert_eq!(actual[0].songs.capacity(), count);
        assert_eq!(actual.capacity(), 1);
    }
}

#[test]
fn meters_preserve_edits_filtering_duplicates_and_full_u32_domain() {
    let mut song = test_song();
    for count in [0, 1, 5, 16, 17, 31, 127, 128, 129, 513] {
        for edits_only in [false, true] {
            song.charts = (0..count)
                .map(|index| {
                    let meter =
                        [0, 1, 2, 63, 64, 65, 126, 127, 128, 129, u32::MAX, 4096][index % 12];
                    let mut chart = test_chart(
                        if edits_only || index % 3 == 0 {
                            "eDiT"
                        } else {
                            "Hard"
                        },
                        meter,
                        index % 11 != 0,
                    );
                    chart.chart_type = if index % 7 == 0 {
                        "dance-double"
                    } else {
                        "dance-single"
                    }
                    .into();
                    chart
                })
                .collect();
            for chart_type in ["dance-single", "DANCE-DOUBLE", "missing", " dance-single "] {
                assert_eq!(
                    baseline::song_meters_for_sort(&song, chart_type),
                    song_meters_for_sort(&song, chart_type)
                );
                let mut second = song.clone();
                second.charts.reverse();
                let library = vec![
                    Arc::new(song.clone()),
                    Arc::new(second),
                    Arc::new(test_song()),
                ];
                assert_groups(
                    &baseline::meter_grouped_songs(library.clone(), chart_type),
                    &meter_grouped_songs(library, chart_type),
                );
                let mut reused = vec![99; 200];
                fill_song_meters_for_sort(&song, chart_type, &mut reused);
                assert_eq!(baseline::song_meters_for_sort(&song, chart_type), reused);
            }
        }
    }
}

#[test]
fn grouping_reserves_only_returned_entries_and_meter_scratch_reuses_capacity() {
    for mode in ["one", "many", "case", "mixed"] {
        let songs = fixture(1025, mode);
        for groups in [
            length_grouped_songs(songs.clone()),
            genre_grouped_songs(songs.clone(), "Unknown"),
        ] {
            assert_eq!(groups.capacity(), groups.len());
            for group in groups {
                assert_eq!(group.songs.capacity(), group.songs.len());
            }
        }
    }
    let mut song = test_song();
    song.charts = (0..512)
        .map(|index| test_chart("Hard", index % 16, true))
        .collect();
    assert_churn_budget(1, 16 * size_of::<u32>(), || {
        black_box(song_meters_for_sort(black_box(&song), "dance-single"));
    });
    let mut scratch = Vec::with_capacity(16);
    assert_no_churn(|| {
        for _ in 0..10 {
            fill_song_meters_for_sort(black_box(&song), "dance-single", &mut scratch);
        }
    });
    let empty = test_song();
    assert_no_churn(|| {
        black_box(song_meters_for_sort(&empty, "dance-single"));
    });
}

// Adapted control: optimized exact sizing, with single-group reuse disabled.
fn presized_genre_without_reuse(songs: Vec<Arc<SongData>>) -> Vec<GroupedSongs> {
    let same_group = |left: &SongData, right: &SongData| {
        left.genre == right.genre || (left.genre.trim().is_empty() && right.genre.trim().is_empty())
    };
    let group_count = songs.chunk_by(|a, b| same_group(a, b)).count();
    let mut groups = Vec::with_capacity(group_count);
    let mut songs = songs.into_iter();
    while let Some(first) = songs.next() {
        let group =
            SongSortGroup::Genre((!first.genre.trim().is_empty()).then(|| first.genre.clone()));
        let rest_count = songs
            .as_slice()
            .iter()
            .take_while(|song| same_group(&first, song))
            .count();
        let mut run = Vec::with_capacity(rest_count + 1);
        run.push(first);
        run.extend(songs.by_ref().take(rest_count));
        groups.push(GroupedSongs { group, songs: run });
    }
    groups
}

fn measure_group(name: &str, songs: &[Arc<SongData>], old: Group, new: Group) {
    let pairs = if std::env::var_os("DEADSYNC_PERF_REVERSE").is_some() {
        [("new", new), ("old", old)]
    } else {
        [("old", old), ("new", new)]
    };
    for (version, run) in pairs {
        let run = black_box(run);
        measure_sampled(
            &format!("{name}_{version}"),
            if songs.len() < 64 { 128 } else { 8 },
            songs.len(),
            || run(black_box(songs).to_vec()),
        );
    }
}
fn measure_named(name: &str, songs: &[Arc<SongData>], key: &str, old: NamedGroup, new: NamedGroup) {
    let pairs = if std::env::var_os("DEADSYNC_PERF_REVERSE").is_some() {
        [("new", new), ("old", old)]
    } else {
        [("old", old), ("new", new)]
    };
    for (version, run) in pairs {
        let run = black_box(run);
        measure_sampled(
            &format!("{name}_{version}"),
            if songs.len() < 64 { 128 } else { 8 },
            songs.len(),
            || run(black_box(songs).to_vec(), black_box(key)),
        );
    }
}

#[test]
#[ignore = "manual old/new CPU, throughput, and allocation comparison"]
fn song_grouping_bench() {
    for (name, count, mode) in [
        ("empty", 0, "mixed"),
        ("single", 1, "mixed"),
        ("small", 16, "mixed"),
        ("mixed", 4096, "mixed"),
        ("one", 4096, "one"),
        ("many", 4096, "many"),
        ("case", 4096, "case"),
    ] {
        let songs = fixture(count, mode);
        measure_group(
            &format!("length_{name}"),
            &songs,
            baseline::length_grouped_songs,
            length_grouped_songs,
        );
        measure_named(
            &format!("genre_{name}"),
            &songs,
            "Unknown",
            baseline::genre_grouped_songs,
            genre_grouped_songs,
        );
    }
    for mode in ["mixed", "one", "many", "case"] {
        let mut songs = fixture(4096, mode);
        songs.sort_by(|a, b| {
            cmp_ignore_ascii_case(&a.genre, &b.genre).then_with(|| song_title_cmp(a, b))
        });
        measure_group(
            &format!("genre_runs_{mode}"),
            &songs,
            baseline::grouped_genre_songs,
            grouped_genre_songs,
        );
    }
    let songs = fixture(4096, "one");
    measure_group(
        "single_group_reuse_control",
        &songs,
        presized_genre_without_reuse,
        grouped_genre_songs,
    );
    for (name, count, overflow) in [
        ("empty", 0, false),
        ("typical", 5, false),
        ("threshold", 16, false),
        ("medium", 32, false),
        ("duplicates", 512, false),
        ("overflow", 512, true),
        ("unique", 128, true),
    ] {
        let mut song = test_song();
        song.charts = (0..count)
            .map(|index| {
                test_chart(
                    "Hard",
                    if overflow { index % 257 } else { index % 16 },
                    true,
                )
            })
            .collect();
        let old: Meters = baseline::song_meters_for_sort;
        let new: Meters = song_meters_for_sort;
        let pairs = if std::env::var_os("DEADSYNC_PERF_REVERSE").is_some() {
            [("new", new), ("old", old)]
        } else {
            [("old", old), ("new", new)]
        };
        for (version, run) in pairs {
            let run = black_box(run);
            measure_sampled(
                &format!("meters_{name}_{version}"),
                512,
                count as usize,
                || run(black_box(&song), black_box("dance-single")),
            );
        }
    }
    let mut dense = fixture(128, "mixed");
    for song in &mut dense {
        Arc::make_mut(song).charts = (0..128)
            .map(|index| test_chart("Edit", index % 16, true))
            .collect();
    }
    measure_named(
        "meter_dense_library",
        &dense,
        "dance-single",
        baseline::meter_grouped_songs,
        meter_grouped_songs,
    );
    let songs = fixture(4096, "mixed");
    measure_named(
        "meter_library",
        &songs,
        "dance-single",
        baseline::meter_grouped_songs,
        meter_grouped_songs,
    );
}
