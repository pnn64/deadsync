// Frozen from 68ff515a3 (0.5.1649); comparator visibility widened for tests.
use super::*;

#[inline]
pub fn cmp_ignore_ascii_case(left: &str, right: &str) -> Ordering {
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
