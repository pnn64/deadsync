use deadsync_chart::SongData;
use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::sync::Arc;

const SORT_BPM_DIVISION: i32 = 10;
const SORT_LENGTH_DIVISION: i32 = 60;
const ALPHA_GROUP_COUNT: usize = 28;
const COMMON_METER_COUNT: usize = 128;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SongSortGroup {
    Title(u8),
    Artist(u8),
    Genre(Option<String>),
    Bpm { lo: i32, hi: i32 },
    Length { lo: i32, hi: i32 },
    Meter(Option<u32>),
}

#[derive(Clone, Debug)]
pub struct GroupedSongs {
    pub group: SongSortGroup,
    pub songs: Vec<Arc<SongData>>,
}

#[must_use]
pub fn song_title_sort_key(song: &SongData) -> (String, String, String) {
    let title = if song.translit_title.trim().is_empty() {
        song.title.as_str()
    } else {
        song.translit_title.as_str()
    };
    let subtitle = if song.translit_subtitle.trim().is_empty() {
        song.subtitle.as_str()
    } else {
        song.translit_subtitle.as_str()
    };
    (
        title.to_ascii_lowercase(),
        subtitle.to_ascii_lowercase(),
        song.simfile_path.to_string_lossy().to_ascii_lowercase(),
    )
}

#[must_use]
pub fn alpha_group_bucket_from_text(text: &str) -> u8 {
    let first = text.trim_start().chars().next();
    match first {
        Some(ch) if ch.is_ascii_digit() => 1,
        Some(ch) if ch.is_ascii_alphabetic() => {
            let c = ch.to_ascii_uppercase();
            (c as u8).saturating_sub(b'A').saturating_add(2)
        }
        _ => 0,
    }
}

#[must_use]
pub fn alpha_group_char(bucket: u8) -> Option<char> {
    (bucket >= 2).then(|| (b'A' + bucket.saturating_sub(2)) as char)
}

#[must_use]
pub fn title_group_bucket(song: &SongData) -> u8 {
    let title = if song.translit_title.trim().is_empty() {
        song.title.as_str()
    } else {
        song.translit_title.as_str()
    };
    alpha_group_bucket_from_text(title)
}

#[must_use]
pub fn song_artist_sort_key(song: &SongData) -> (String, String) {
    (
        song.artist.to_ascii_lowercase(),
        song.simfile_path.to_string_lossy().to_ascii_lowercase(),
    )
}

#[inline]
fn cmp_ignore_ascii_case(left: &str, right: &str) -> Ordering {
    left.bytes()
        .map(|byte| byte.to_ascii_lowercase())
        .cmp(right.bytes().map(|byte| byte.to_ascii_lowercase()))
}

#[inline]
#[must_use]
pub fn song_title_cmp(left: &SongData, right: &SongData) -> Ordering {
    cmp_ignore_ascii_case(left.display_title(true), right.display_title(true))
        .then_with(|| {
            cmp_ignore_ascii_case(left.display_subtitle(true), right.display_subtitle(true))
        })
        .then_with(|| {
            cmp_ignore_ascii_case(
                left.simfile_path.to_string_lossy().as_ref(),
                right.simfile_path.to_string_lossy().as_ref(),
            )
        })
}

#[must_use]
pub fn song_bpm_for_sort(song: &SongData) -> i32 {
    song.display_bpm_range()
        .map_or(0, |(_lo, hi)| hi.max(0.0) as i32)
}

#[must_use]
pub fn song_length_for_sort(song: &SongData) -> i32 {
    if song.music_length_seconds.is_finite() && song.music_length_seconds > 0.0 {
        song.music_length_seconds.max(0.0) as i32
    } else {
        song.total_length_seconds.max(0)
    }
}

/// Returns all unique meter values for a song, considering the given chart type.
/// Non-edit charts are preferred; edit charts are only included if the song has
/// no non-edit charts at all.
#[must_use]
pub fn song_meters_for_sort(song: &SongData, chart_type: &str) -> Vec<u32> {
    let mut meters = Vec::new();
    fill_song_meters_for_sort(song, chart_type, &mut meters);
    meters
}

fn fill_song_meters_for_sort(song: &SongData, chart_type: &str, meters: &mut Vec<u32>) {
    // For ordinary small chart lists, the existing short sort costs less
    // than bitmap setup. Larger lists deduplicate common meters before storing.
    if song.charts.len() <= 16 {
        fill_small_song_meters_for_sort(song, chart_type, meters);
        return;
    }
    meters.clear();
    let has_non_edit = song.charts.iter().any(|chart| {
        chart.has_note_data
            && chart.chart_type.eq_ignore_ascii_case(chart_type)
            && !chart.difficulty.eq_ignore_ascii_case("edit")
    });
    let mut common = 0u64;
    for chart in &song.charts {
        if !chart.has_note_data
            || !chart.chart_type.eq_ignore_ascii_case(chart_type)
            || has_non_edit == chart.difficulty.eq_ignore_ascii_case("edit")
        {
            continue;
        }
        if chart.meter < u64::BITS {
            common |= 1u64 << chart.meter;
        } else {
            if meters.is_empty() {
                meters.reserve(song.charts.len());
            }
            meters.push(chart.meter);
        }
    }
    meters.sort_unstable();
    meters.dedup();
    let overflow_len = meters.len();
    let common_len = common.count_ones() as usize;
    meters.resize(overflow_len + common_len, 0);
    meters.copy_within(..overflow_len, common_len);
    for meter in &mut meters[..common_len] {
        *meter = common.trailing_zeros();
        common &= common - 1;
    }
}

#[inline]
fn fill_small_song_meters_for_sort(song: &SongData, chart_type: &str, meters: &mut Vec<u32>) {
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

#[must_use]
pub fn title_grouped_songs(songs: Vec<Arc<SongData>>) -> Vec<GroupedSongs> {
    alpha_grouped_songs(
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

#[must_use]
pub fn artist_grouped_songs(songs: Vec<Arc<SongData>>) -> Vec<GroupedSongs> {
    alpha_grouped_songs(
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

#[must_use]
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

fn grouped_genre_songs(songs: Vec<Arc<SongData>>) -> Vec<GroupedSongs> {
    grouped_song_runs(
        songs,
        |left, right| {
            left.genre == right.genre
                || (left.genre.trim().is_empty() && right.genre.trim().is_empty())
        },
        |song| SongSortGroup::Genre((!song.genre.trim().is_empty()).then(|| song.genre.clone())),
    )
}

#[must_use]
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

fn bpm_grouped_songs_uncached(mut songs: Vec<Arc<SongData>>) -> Vec<GroupedSongs> {
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

#[must_use]
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

#[must_use]
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

#[must_use]
pub fn bpm_bucket_range(max_bpm: i32) -> (i32, i32) {
    bucket_range(max_bpm, SORT_BPM_DIVISION)
}

#[must_use]
pub fn length_bucket_range(length_seconds: i32) -> (i32, i32) {
    bucket_range(length_seconds, SORT_LENGTH_DIVISION)
}

fn bucket_range(value: i32, division: i32) -> (i32, i32) {
    let mut hi = value.max(0);
    let rem = hi.rem_euclid(division);
    hi += division - rem - 1;
    (hi - (division - 1), hi)
}

fn grouped_contiguous_songs(
    songs: Vec<Arc<SongData>>,
    group_for: impl Fn(&SongData) -> SongSortGroup,
) -> Vec<GroupedSongs> {
    grouped_song_runs(
        songs,
        |left, right| group_for(left) == group_for(right),
        &group_for,
    )
}

fn grouped_song_runs(
    mut songs: Vec<Arc<SongData>>,
    same_group: impl Fn(&SongData, &SongData) -> bool,
    group_for: impl Fn(&SongData) -> SongSortGroup,
) -> Vec<GroupedSongs> {
    let group_count = songs
        .chunk_by(|left, right| same_group(left, right))
        .count();
    if group_count == 1 {
        let group = group_for(&songs[0]);
        // The whole input is already the only output run. Keep its buffer;
        // trim spare capacity only if the caller supplied an oversized vector.
        songs.shrink_to_fit();
        return vec![GroupedSongs { group, songs }];
    }
    let mut groups = Vec::with_capacity(group_count);
    let mut songs = songs.into_iter();
    while let Some(first) = songs.next() {
        let group = group_for(&first);
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

fn alpha_grouped_songs(
    songs: Vec<Arc<SongData>>,
    bucket_for: impl Fn(&SongData) -> u8,
    compare: impl Fn(&SongData, &SongData) -> Ordering,
    group_for: impl Fn(u8) -> SongSortGroup,
) -> Vec<GroupedSongs> {
    let mut counts = [0usize; ALPHA_GROUP_COUNT];
    // Keep one byte per song so sizing the buckets does not require parsing
    // title/artist prefixes twice. The temporary replaces repeated Vec growth.
    let bucket_indices: Vec<_> = songs
        .iter()
        .map(|song| {
            let bucket = bucket_for(song);
            counts[usize::from(bucket)] += 1;
            bucket
        })
        .collect();
    let mut buckets: [Vec<Arc<SongData>>; ALPHA_GROUP_COUNT] =
        std::array::from_fn(|bucket| Vec::with_capacity(counts[bucket]));
    for (song, bucket) in songs.into_iter().zip(bucket_indices) {
        buckets[usize::from(bucket)].push(song);
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

#[cfg(test)]
mod tests {
    use super::*;
    use deadsync_chart::{ArrowStats, ChartData, StaminaCounts, TechCounts};
    use std::path::PathBuf;

    fn test_chart(difficulty: &str, meter: u32, has_note_data: bool) -> ChartData {
        ChartData {
            chart_type: "dance-single".to_string(),
            difficulty: difficulty.to_string(),
            description: String::new(),
            chart_name: String::new(),
            meter,
            step_artist: String::new(),
            music_path: None,
            short_hash: format!("{difficulty}-{meter}"),
            stats: ArrowStats::default(),
            tech_counts: TechCounts::default(),
            mines_nonfake: 0,
            stamina_counts: StaminaCounts::default(),
            total_streams: 0,
            matrix_rating: 0.0,
            matrix_profile: Box::default(),
            max_nps: 0.0,
            sn_detailed_breakdown: String::new(),
            sn_partial_breakdown: String::new(),
            sn_simple_breakdown: String::new(),
            detailed_breakdown: String::new(),
            partial_breakdown: String::new(),
            simple_breakdown: String::new(),
            total_measures: 0,
            measure_nps_vec: Vec::new(),
            measure_seconds_vec: Vec::new(),
            first_second: 0.0,
            has_note_data,
            has_chart_attacks: false,
            possible_grade_points: 0,
            holds_total: 0,
            rolls_total: 0,
            mines_total: 0,
            display_bpm: None,
            min_bpm: 120.0,
            max_bpm: 120.0,
        }
    }

    fn test_song() -> SongData {
        SongData {
            simfile_path: PathBuf::from("Pack/Song/song.ssc"),
            title: "Zulu".to_string(),
            subtitle: "Mix".to_string(),
            translit_title: String::new(),
            translit_subtitle: String::new(),
            artist: "Artist".to_string(),
            translit_artist: String::new(),
            genre: String::new(),
            banner_path: None,
            background_path: None,
            background_changes: Vec::new(),
            background_layer2_changes: Vec::new(),
            foreground_changes: Vec::new(),
            background_lua_changes: Vec::new(),
            foreground_lua_changes: Vec::new(),
            has_lua: false,
            cdtitle_path: None,
            music_path: None,
            display_bpm: String::new(),
            offset: 0.0,
            sample_start: None,
            sample_length: None,
            min_bpm: 120.0,
            max_bpm: 180.0,
            normalized_bpms: String::new(),
            music_length_seconds: 0.0,
            first_second: 0.0,
            total_length_seconds: 123,
            precise_last_second_seconds: 123.0,
            charts: Vec::new(),
        }
    }

    #[test]
    fn song_title_sort_key_prefers_translit_and_path_tiebreaker() {
        let mut song = test_song();
        song.translit_title = "Alpha".to_string();
        song.translit_subtitle = "Beta".to_string();

        assert_eq!(
            song_title_sort_key(&song),
            (
                "alpha".to_string(),
                "beta".to_string(),
                "pack/song/song.ssc".to_string()
            )
        );
    }

    #[test]
    fn alpha_group_bucket_classifies_digits_letters_and_other() {
        assert_eq!(alpha_group_bucket_from_text("  7th"), 1);
        assert_eq!(alpha_group_bucket_from_text(" alpha"), 2);
        assert_eq!(alpha_group_bucket_from_text("Zulu"), 27);
        assert_eq!(alpha_group_bucket_from_text("!bang"), 0);
        assert_eq!(alpha_group_char(27), Some('Z'));
        assert_eq!(alpha_group_char(1), None);
    }

    #[test]
    fn song_sort_values_follow_song_metadata() {
        let mut song = test_song();
        assert_eq!(song_bpm_for_sort(&song), 180);
        assert_eq!(song_length_for_sort(&song), 123);

        song.music_length_seconds = 65.9;
        assert_eq!(song_length_for_sort(&song), 65);
    }

    #[test]
    fn song_meters_for_sort_prefers_non_edit_meters() {
        let mut song = test_song();
        song.charts = vec![
            test_chart("Edit", 19, true),
            test_chart("Hard", 11, true),
            test_chart("Challenge", 13, true),
            test_chart("Challenge", 13, true),
            test_chart("Medium", 7, false),
        ];
        assert_eq!(song_meters_for_sort(&song, "dance-single"), vec![11, 13]);
        song.charts = vec![
            test_chart("Edit", 19, true),
            test_chart("Edit", 21, true),
            test_chart("Edit", 19, true),
        ];
        assert_eq!(song_meters_for_sort(&song, "dance-single"), vec![19, 21]);
    }

    #[test]
    fn grouped_songs_bucket_common_sort_modes() {
        let mut alpha = test_song();
        alpha.title = "Alpha".to_string();
        alpha.artist = "Zed".to_string();
        alpha.genre = "Pop".to_string();
        alpha.music_length_seconds = 65.0;
        alpha.charts = vec![
            test_chart("Hard", 11, true),
            test_chart("Challenge", 13, true),
        ];

        let mut numeric = test_song();
        numeric.title = "7th".to_string();
        numeric.artist = "123".to_string();
        numeric.genre = String::new();
        numeric.music_length_seconds = 5.0;
        numeric.min_bpm = 90.0;
        numeric.max_bpm = 99.0;
        numeric.charts = vec![test_chart("Hard", 7, true)];

        let songs = vec![Arc::new(alpha), Arc::new(numeric)];

        assert_eq!(
            title_grouped_songs(songs.clone())[0].group,
            SongSortGroup::Title(1)
        );
        assert_eq!(
            artist_grouped_songs(songs.clone())[0].group,
            SongSortGroup::Artist(1)
        );
        assert_eq!(
            genre_grouped_songs(songs.clone(), "Unknown Genre")[0].group,
            SongSortGroup::Genre(Some("Pop".to_string()))
        );
        assert_eq!(
            bpm_grouped_songs(songs.clone())[0].group,
            SongSortGroup::Bpm { lo: 90, hi: 99 }
        );
        assert_eq!(
            length_grouped_songs(songs.clone())[0].group,
            SongSortGroup::Length { lo: 0, hi: 59 }
        );

        let meter_groups = meter_grouped_songs(songs, "dance-single");
        assert_eq!(meter_groups[0].group, SongSortGroup::Meter(Some(7)));
        assert_eq!(meter_groups[1].group, SongSortGroup::Meter(Some(11)));
        assert_eq!(meter_groups[2].group, SongSortGroup::Meter(Some(13)));
    }

    #[test]
    fn title_sort_preserves_cached_key_order_for_case_and_translit_ties() {
        let mut songs = Vec::new();
        for (title, translit_title, subtitle, path) in [
            ("alpha", "", "Mix", "Pack/Z/song.ssc"),
            ("Alpha", "", "mix", "Pack/Y/song.ssc"),
            ("Zulu", "alpha", "Beta", "Pack/X/song.ssc"),
            ("ALPHA", "", "Mix", "Pack/W/song.ssc"),
        ] {
            let mut song = test_song();
            song.title = title.to_string();
            song.translit_title = translit_title.to_string();
            song.subtitle = subtitle.to_string();
            song.simfile_path = PathBuf::from(path);
            songs.push(Arc::new(song));
        }
        for left in &songs {
            for right in &songs {
                assert_eq!(
                    song_title_cmp(left, right),
                    song_title_sort_key(left).cmp(&song_title_sort_key(right))
                );
            }
        }
        let mut expected = songs.clone();
        expected.sort_by_cached_key(|song| {
            (
                title_group_bucket(song),
                song_title_sort_key(song),
                song.title.clone(),
                song.subtitle.clone(),
            )
        });

        let actual = title_grouped_songs(songs)
            .into_iter()
            .flat_map(|group| group.songs)
            .collect::<Vec<_>>();

        assert_eq!(
            actual
                .iter()
                .map(|song| song.simfile_path.as_path())
                .collect::<Vec<_>>(),
            expected
                .iter()
                .map(|song| song.simfile_path.as_path())
                .collect::<Vec<_>>()
        );
    }
    mod grouping_perf {
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/perf/song_grouping.rs"
        ));
    }
    mod library_sort_perf {
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/perf/library_sort.rs"
        ));
    }
}
