// Reference method frozen from db7a654b3 / 0.5.1138.
use super::*;
use std::hint::black_box;

impl SongRankingIndex<'_> {
    fn legacy_rank_popular<H: AsRef<str>>(
        &self,
        chart_play_counts: impl IntoIterator<Item = (H, u32)>,
        limit: usize,
        include_zero_play_songs: bool,
        workspace: &mut SongRankingWorkspace,
        song_cmp: impl Fn(&SongData, &SongData) -> Ordering,
    ) {
        workspace.song_play_counts.resize(self.songs.len(), 0);
        workspace.song_play_counts.fill(0);
        for (chart_hash, chart_plays) in chart_play_counts {
            let Some(&song_ix) = self.chart_hash_to_song.get(chart_hash.as_ref()) else {
                continue;
            };
            workspace.song_play_counts[song_ix] =
                workspace.song_play_counts[song_ix].saturating_add(chart_plays);
        }

        workspace.popular.clear();
        workspace.popular.reserve(self.songs.len());
        workspace.popular.extend(
            self.songs
                .iter()
                .enumerate()
                .filter(|(song_ix, _)| {
                    include_zero_play_songs || workspace.song_play_counts[*song_ix] > 0
                })
                .map(|(song_ix, _)| (song_ix, workspace.song_play_counts[song_ix])),
        );
        workspace
            .popular
            .sort_unstable_by(|(left_ix, left_count), (right_ix, right_count)| {
                right_count.cmp(left_count).then_with(|| {
                    if self.song_order_rank.len() == self.songs.len() {
                        self.song_order_rank[*left_ix].cmp(&self.song_order_rank[*right_ix])
                    } else {
                        song_cmp(&self.songs[*left_ix], &self.songs[*right_ix])
                            .then_with(|| left_ix.cmp(right_ix))
                    }
                })
            });
        workspace
            .popular
            .truncate(limit.min(workspace.popular.len()));
    }
}

fn popular_fixture(count: usize, tied: bool) -> (Vec<Arc<SongData>>, Vec<(String, u32)>) {
    let songs = (0..count)
        .map(|i| {
            let mut song = song(vec![
                chart("Hard", &format!("hash-{i:05}")),
                chart("Edit", &format!("extra-{i:05}")),
            ]);
            song.title = format!("Title {:05}", (i * 7919) % 97);
            song.simfile_path = format!("Pack/{:05}/song.ssc", i / 3).into();
            // Shared chart hashes retain first-song ownership in the index.
            if i.is_multiple_of(17) {
                song.charts.push(chart("Easy", "shared"));
            }
            Arc::new(song)
        })
        .collect();
    let mut counts: Vec<_> = (0..count)
        .filter(|i| !i.is_multiple_of(7))
        .map(|i| {
            (
                format!("hash-{i:05}"),
                if tied {
                    5
                } else {
                    ((i * 7919 + 53) % count.max(1)) as u32
                },
            )
        })
        .collect();
    counts.extend([
        ("missing".into(), 100),
        ("shared".into(), u32::MAX),
        ("shared".into(), 17),
        ("hash-00001".into(), 2),
    ]);
    (songs, counts)
}

fn popular_song_cmp(left: &SongData, right: &SongData) -> Ordering {
    left.title
        .cmp(&right.title)
        .then_with(|| left.simfile_path.cmp(&right.simfile_path))
}

#[test]
fn partial_popularity_preserves_limits_ties_and_first_hash_ownership() {
    for count in [0, 1, 2, 3, 31, 64, 127, 513] {
        for tied in [false, true] {
            let (songs, counts) = popular_fixture(count, tied);
            for prepared in [false, true] {
                let mut index = SongRankingIndex::new(&songs);
                if prepared {
                    index.prepare_song_order(popular_song_cmp);
                }
                let mut actual = SongRankingWorkspace::default();
                let mut expected = SongRankingWorkspace::default();
                for zero in [false, true] {
                    for limit in [
                        0,
                        1,
                        2,
                        50,
                        count / 2,
                        count / 2 + 1,
                        count.saturating_sub(1),
                        count,
                        usize::MAX,
                    ] {
                        index.rank_popular(
                            counts.iter().map(|(hash, count)| (hash, *count)),
                            limit,
                            zero,
                            &mut actual,
                            popular_song_cmp,
                        );
                        index.legacy_rank_popular(
                            counts.iter().map(|(hash, count)| (hash, *count)),
                            limit,
                            zero,
                            &mut expected,
                            popular_song_cmp,
                        );
                        assert_eq!(actual.popular(), expected.popular());
                        assert_eq!(actual.song_play_counts, expected.song_play_counts);
                    }
                }
            }
        }
    }
}

#[test]
fn popularity_reuses_storage_across_limits_and_fewer_scores() {
    let (songs, counts) = popular_fixture(2048, false);
    let mut index = SongRankingIndex::new(&songs);
    index.prepare_song_order(popular_song_cmp);
    let mut workspace = SongRankingWorkspace::default();
    index.rank_popular(
        counts.iter().map(|(h, c)| (h, *c)),
        30,
        true,
        &mut workspace,
        popular_song_cmp,
    );
    for limit in [0, 1, 30, 1024, 2048] {
        crate::perf::assert_no_churn(|| {
            index.rank_popular(
                counts.iter().map(|(h, c)| (h, *c)),
                limit,
                true,
                &mut workspace,
                popular_song_cmp,
            );
        });
    }
    index.rank_popular(
        std::iter::empty::<(&str, u32)>(),
        30,
        false,
        &mut workspace,
        popular_song_cmp,
    );
    assert!(workspace.popular().is_empty());
}

#[test]
#[ignore = "manual old/new release benchmark"]
fn popularity_bench() {
    for (label, count, limit, prepared, tied) in [
        ("small", 32, 30, true, false),
        ("medium", 2048, 50, true, false),
        ("large", 8192, 50, true, false),
        ("uncached", 2048, 50, false, true),
        ("ties", 2048, 50, true, true),
        ("half", 2048, 1024, true, false),
        ("full", 2048, usize::MAX, true, false),
        ("zero", 2048, 0, true, false),
    ] {
        let (songs, counts) = popular_fixture(count, tied);
        let mut index = SongRankingIndex::new(&songs);
        if prepared {
            index.prepare_song_order(popular_song_cmp);
        }
        let reverse = std::env::var_os("DEADSYNC_PERF_REVERSE").is_some();
        for old in [!reverse, reverse] {
            let mut workspace = SongRankingWorkspace::default();
            let name = format!("popular_{label}_{}", if old { "old" } else { "new" });
            crate::perf::measure_sampled(
                &name,
                if count < 100 { 2000 } else { 128 },
                count,
                || {
                    let input = black_box(&counts)
                        .iter()
                        .map(|(hash, count)| (hash, *count));
                    if old {
                        index.legacy_rank_popular(
                            input,
                            limit,
                            true,
                            &mut workspace,
                            popular_song_cmp,
                        );
                    } else {
                        index.rank_popular(input, limit, true, &mut workspace, popular_song_cmp);
                    }
                    black_box(workspace.popular());
                },
            );
        }
    }
}
