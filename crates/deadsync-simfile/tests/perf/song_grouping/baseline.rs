// Frozen function bodies from b5093e110 / 0.5.1149.
// Shared types and unchanged comparators come from the parent module.
use super::*;

pub fn bpm_grouped_songs(mut songs: Vec<Arc<SongData>>) -> Vec<GroupedSongs> {
    // Compact indices keep the temporary to eight bytes per song. Preserve the
    // original path for inputs whose positions cannot be represented by u32.
    if songs.len() > u32::MAX as usize {
        return bpm_grouped_songs_uncached(songs);
    }
    let mut order: Vec<_> = songs
        .iter()
        .enumerate()
        .map(|(index, song)| (song_bpm_for_sort(song), index as u32))
        .collect();
    order.sort_unstable_by(|&(left_bpm, left), &(right_bpm, right)| {
        left_bpm
            .cmp(&right_bpm)
            .then_with(|| song_title_cmp(&songs[left as usize], &songs[right as usize]))
            .then_with(|| left.cmp(&right))
    });

    // Resolve the source position through earlier swaps. Keep the sorted BPM
    // keys intact so grouping can reuse them without parsing the tags again.
    for index in 0..order.len() {
        let mut source = order[index].1 as usize;
        while source < index {
            source = order[source].1 as usize;
        }
        order[index].1 = source as u32;
        songs.swap(index, source);
    }
    let runs =
        || order.chunk_by(|left, right| bpm_bucket_range(left.0) == bpm_bucket_range(right.0));
    let mut groups = Vec::with_capacity(runs().count());
    let mut songs = songs.into_iter();
    for run in runs() {
        let (lo, hi) = bpm_bucket_range(run[0].0);
        let grouped = songs.by_ref().take(run.len()).collect();
        groups.push(GroupedSongs {
            group: SongSortGroup::Bpm { lo, hi },
            songs: grouped,
        });
    }
    groups
}

pub(super) fn bpm_grouped_songs_uncached(mut songs: Vec<Arc<SongData>>) -> Vec<GroupedSongs> {
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

pub fn genre_grouped_songs(
    mut songs: Vec<Arc<SongData>>,
    unknown_genre_label: &str,
) -> Vec<GroupedSongs> {
    songs.sort_by(|left, right| {
        let left_genre = left.genre.trim();
        let left_name = if left_genre.is_empty() {
            unknown_genre_label
        } else {
            &left.genre
        };
        let right_genre = right.genre.trim();
        let right_name = if right_genre.is_empty() {
            unknown_genre_label
        } else {
            &right.genre
        };
        cmp_ignore_ascii_case(left_name, right_name).then_with(|| song_title_cmp(left, right))
    });
    grouped_genre_songs(songs)
}

pub(super) fn grouped_genre_songs(songs: Vec<Arc<SongData>>) -> Vec<GroupedSongs> {
    let mut groups = Vec::new();
    let mut current_group: Option<SongSortGroup> = None;
    let mut current_songs = Vec::new();

    for song in songs {
        let matches_current = match current_group.as_ref() {
            Some(SongSortGroup::Genre(Some(genre))) => {
                !song.genre.trim().is_empty() && genre == &song.genre
            }
            Some(SongSortGroup::Genre(None)) => song.genre.trim().is_empty(),
            _ => false,
        };
        if !matches_current {
            if let Some(group) = current_group.take() {
                groups.push(GroupedSongs {
                    group,
                    songs: std::mem::take(&mut current_songs),
                });
            }
            current_group = Some(SongSortGroup::Genre(
                (!song.genre.trim().is_empty()).then(|| song.genre.clone()),
            ));
        }
        current_songs.push(song);
    }

    if let Some(group) = current_group {
        groups.push(GroupedSongs {
            group,
            songs: current_songs,
        });
    }
    groups
}

pub fn length_grouped_songs(mut songs: Vec<Arc<SongData>>) -> Vec<GroupedSongs> {
    songs.sort_by(|left, right| {
        song_length_for_sort(left)
            .cmp(&song_length_for_sort(right))
            .then_with(|| song_title_cmp(left, right))
    });
    grouped_contiguous_songs(songs, |song| {
        let (lo, hi) = length_bucket_range(song_length_for_sort(song));
        SongSortGroup::Length { lo, hi }
    })
}

pub(super) fn grouped_contiguous_songs(
    songs: Vec<Arc<SongData>>,
    group_for: impl Fn(&SongData) -> SongSortGroup,
) -> Vec<GroupedSongs> {
    let mut groups = Vec::new();
    let mut current_group = None;
    let mut current_songs = Vec::new();

    for song in songs {
        let group = group_for(song.as_ref());
        if current_group
            .as_ref()
            .is_some_and(|current| current != &group)
        {
            groups.push(GroupedSongs {
                group: current_group.take().unwrap(),
                songs: std::mem::take(&mut current_songs),
            });
        }
        current_group = Some(group);
        current_songs.push(song);
    }

    if let Some(group) = current_group {
        groups.push(GroupedSongs {
            group,
            songs: current_songs,
        });
    }

    groups
}

pub fn song_meters_for_sort(song: &SongData, chart_type: &str) -> Vec<u32> {
    let mut meters = Vec::new();
    fill_song_meters_for_sort(song, chart_type, &mut meters);
    meters
}

pub(super) fn fill_song_meters_for_sort(song: &SongData, chart_type: &str, meters: &mut Vec<u32>) {
    meters.clear();
    let has_non_edit = song.charts.iter().any(|chart| {
        chart.has_note_data
            && chart.chart_type.eq_ignore_ascii_case(chart_type)
            && !chart.difficulty.eq_ignore_ascii_case("edit")
    });
    for chart in &song.charts {
        if !chart.has_note_data
            || !chart.chart_type.eq_ignore_ascii_case(chart_type)
            || has_non_edit == chart.difficulty.eq_ignore_ascii_case("edit")
        {
            continue;
        }
        if meters.is_empty() {
            meters.reserve(song.charts.len());
        }
        meters.push(chart.meter);
    }
    meters.sort_unstable();
    meters.dedup();
}

pub fn meter_grouped_songs(songs: Vec<Arc<SongData>>, chart_type: &str) -> Vec<GroupedSongs> {
    let mut common: [Vec<Arc<SongData>>; COMMON_METER_COUNT] = std::array::from_fn(|_| Vec::new());
    let mut overflow = BTreeMap::<u32, Vec<Arc<SongData>>>::new();
    let mut missing = Vec::new();
    let mut meters = Vec::new();
    for song in songs {
        fill_song_meters_for_sort(song.as_ref(), chart_type, &mut meters);
        let Some((&last_meter, preceding)) = meters.split_last() else {
            missing.push(song);
            continue;
        };
        for &meter in preceding {
            if let Some(bucket) = common.get_mut(meter as usize) {
                bucket.push(Arc::clone(&song));
            } else {
                overflow.entry(meter).or_default().push(Arc::clone(&song));
            }
        }
        if let Some(bucket) = common.get_mut(last_meter as usize) {
            bucket.push(song);
        } else {
            overflow.entry(last_meter).or_default().push(song);
        }
    }

    let group_count = common.iter().filter(|songs| !songs.is_empty()).count()
        + overflow.len()
        + usize::from(!missing.is_empty());
    let mut groups = Vec::with_capacity(group_count);
    for (meter, mut songs) in common.into_iter().enumerate() {
        if songs.is_empty() {
            continue;
        }
        songs.sort_by(|left, right| song_title_cmp(left, right));
        groups.push(GroupedSongs {
            group: SongSortGroup::Meter(Some(meter as u32)),
            songs,
        });
    }
    for (meter, mut songs) in overflow {
        songs.sort_by(|left, right| song_title_cmp(left, right));
        groups.push(GroupedSongs {
            group: SongSortGroup::Meter(Some(meter)),
            songs,
        });
    }
    if !missing.is_empty() {
        missing.sort_by(|left, right| song_title_cmp(left, right));
        groups.push(GroupedSongs {
            group: SongSortGroup::Meter(None),
            songs: missing,
        });
    }
    groups
}
