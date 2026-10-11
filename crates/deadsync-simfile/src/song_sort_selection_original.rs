// Frozen from main at 4d64b1a8b for behavioral and paired performance comparisons.
use super::*;

pub(super) fn alpha_grouped_songs(
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
    if bucket_indices.len() <= 1 {
        return grouped_contiguous_songs(songs, |_| group_for(bucket_indices[0]));
    }
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

pub(super) fn title_grouped_songs(songs: Vec<Arc<SongData>>) -> Vec<GroupedSongs> {
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

pub(super) fn artist_grouped_songs(songs: Vec<Arc<SongData>>) -> Vec<GroupedSongs> {
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
