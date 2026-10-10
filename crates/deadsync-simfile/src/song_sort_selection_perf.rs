mod selection_original {
    include!("song_sort_selection_original.rs");
}

fn selection_songs(count: usize, mixed: bool) -> Vec<Arc<SongData>> {
    (0..count)
        .map(|i| {
            let mut song = test_song();
            let initial = if mixed {
                char::from(b'A' + (i % 26) as u8)
            } else {
                'A'
            };
            song.title = format!("{initial} Song {:05}", count - i);
            song.artist = format!("{initial} Artist {:05}", count - i);
            song.simfile_path = PathBuf::from(format!("Pack/Song {i:05}/song.ssc"));
            Arc::new(song)
        })
        .collect()
}

fn assert_selection_groups_equal(old: &[GroupedSongs], new: &[GroupedSongs]) {
    assert_eq!(old.len(), new.len());
    for (old, new) in old.iter().zip(new) {
        assert_eq!(old.group, new.group);
        assert_eq!(old.songs.len(), new.songs.len());
        for (old, new) in old.songs.iter().zip(&new.songs) {
            assert!(Arc::ptr_eq(old, new));
        }
    }
}

#[test]
fn selection_alpha_grouping_preserves_order_identity_and_capacity() {
    for count in [0, 1, 2, 17, 1000] {
        for mixed in [false, true] {
            let songs = selection_songs(count, mixed);
            for spare in [0, 64] {
                for artist in [false, true] {
                    let old = if artist {
                        selection_original::artist_grouped_songs(songs.clone())
                    } else {
                        selection_original::title_grouped_songs(songs.clone())
                    };
                    let mut input = Vec::with_capacity(count + spare);
                    input.extend(songs.iter().cloned());
                    let new = if artist {
                        artist_grouped_songs(input)
                    } else {
                        title_grouped_songs(input)
                    };
                    assert_selection_groups_equal(&old, &new);
                    if new.len() == 1 {
                        assert_eq!(new[0].songs.capacity(), count);
                    }
                }
            }
        }
    }
    let mut songs = selection_songs(12, false);
    for (i, song) in songs.iter_mut().enumerate() {
        let song = Arc::make_mut(song);
        song.title = ["Same", "same", "\u{66f2}", "", "7th", "\u{2003}7th"][i % 6].into();
        song.artist = song.title.clone();
        song.translit_title = if i % 2 == 0 {
            "Alternate".into()
        } else {
            String::new()
        };
        song.simfile_path = PathBuf::from("same.ssc");
    }
    assert_selection_groups_equal(
        &selection_original::title_grouped_songs(songs.clone()),
        &title_grouped_songs(songs.clone()),
    );
    assert_selection_groups_equal(
        &selection_original::artist_grouped_songs(songs.clone()),
        &artist_grouped_songs(songs),
    );
}

#[test]
fn selection_single_alpha_group_keeps_input_buffer() {
    let songs = selection_songs(1024, false);
    let input = songs.clone();
    let pointer = input.as_ptr();
    let old_input = songs.clone();
    let (_, old) =
        crate::metadata_perf::measure(|| selection_original::title_grouped_songs(old_input));
    let (groups, new) = crate::metadata_perf::measure(|| title_grouped_songs(input));
    assert_eq!(groups[0].songs.as_ptr(), pointer);
    assert_eq!(old.allocs - new.allocs, 1);
    assert_eq!(
        old.allocated_bytes - new.allocated_bytes,
        1024 * size_of::<Arc<SongData>>()
    );
}

#[test]
#[ignore = "paired release benchmark; run with --ignored --nocapture --test-threads=1"]
fn benchmark_selection_alpha_grouping() {
    use crate::metadata_perf::{compare_owned, measure};
    use std::hint::black_box;
    for count in [0, 1, 16, 1024, 8192] {
        for mixed in [false, true] {
            if mixed && count <= 1 {
                continue;
            }
            for shuffled in [false, true] {
                if shuffled && count != 1024 {
                    continue;
                }
                let mut songs = selection_songs(count, mixed);
                if shuffled {
                    let original = songs.clone();
                    for (i, song) in songs.iter_mut().enumerate() {
                        *song = original[(i * 4051) % count].clone();
                    }
                }
                for artist in [false, true] {
                    let label = format!(
                        "alpha-{}-{}-{count}{}",
                        if artist { "artist" } else { "title" },
                        if mixed { "mixed" } else { "single" },
                        if shuffled { "-shuffled" } else { "" }
                    );
                    let original = |songs| {
                        if artist {
                            selection_original::artist_grouped_songs(songs)
                        } else {
                            selection_original::title_grouped_songs(songs)
                        }
                    };
                    let current = |songs| {
                        if artist {
                            artist_grouped_songs(songs)
                        } else {
                            title_grouped_songs(songs)
                        }
                    };
                    let old_input = songs.clone();
                    let new_input = songs.clone();
                    let (old, old_churn) = measure(|| original(old_input));
                    let (new, new_churn) = measure(|| current(new_input));
                    assert_selection_groups_equal(&old, &new);
                    println!("ALLOC {label}: original {old_churn:?}, current {new_churn:?}");
                    drop((old, new));
                    compare_owned(
                        &label,
                        match count {
                            0 => 65536,
                            1 => 16384,
                            16 => 1024,
                            1024 => 32,
                            _ => 12,
                        },
                        &songs,
                        |songs| {
                            black_box(original(black_box(songs)));
                        },
                        |songs| {
                            black_box(current(black_box(songs)));
                        },
                    );
                }
            }
        }
    }
}
