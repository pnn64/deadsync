use super::tests::{fixed_course, test_chart, test_song};
use super::*;
use crate::perf::{assert_churn_budget, assert_no_churn, measure_sampled};
use std::hint::black_box;

#[path = "course_selection/baseline.rs"]
mod baseline;

type Resolver = fn(
    &Path,
    usize,
    u64,
    &CourseEntry,
    &HashMap<(String, String), Arc<SongData>>,
    &HashMap<String, Arc<SongData>>,
    &[Arc<SongData>],
    &HashMap<String, Vec<Arc<SongData>>>,
    &HashMap<String, u32>,
    &HashMap<String, CourseGradeCounts>,
    CourseType,
    &[String],
    &str,
    Difficulty,
) -> Option<ResolvedCourseStage>;

struct Library {
    songs: Vec<Arc<SongData>>,
    groups: HashMap<String, Vec<Arc<SongData>>>,
    by_song: HashMap<String, Arc<SongData>>,
    by_group_song: HashMap<(String, String), Arc<SongData>>,
    plays: HashMap<String, u32>,
    grades: HashMap<String, CourseGradeCounts>,
}

impl Library {
    fn new(count: usize, charts: usize) -> Self {
        let mut library = Self {
            songs: Vec::with_capacity(count),
            groups: HashMap::new(),
            by_song: HashMap::new(),
            by_group_song: HashMap::new(),
            plays: HashMap::new(),
            grades: HashMap::new(),
        };
        for index in 0..count {
            let group = format!("Pack{}", index % 4);
            let mut song = test_song();
            song.simfile_path = PathBuf::from(format!("{group}/Song{index}/chart.ssc"));
            song.title = format!("Title{index}");
            song.translit_title = format!("Translit{index}");
            song.artist = format!("Artist{}", index % 32);
            song.translit_artist = format!("Translated{}", index % 8);
            song.genre = format!("Genre{}", index % 3);
            song.min_bpm = 100.0;
            song.max_bpm = 200.0;
            song.display_bpm = match index % 5 {
                0 => "*",
                1 => "120:180",
                2 => "150",
                _ => "",
            }
            .to_string();
            song.music_length_seconds = 60.0 + (index % 120) as f32;
            song.charts = (0..charts)
                .map(|chart| {
                    let mut value = test_chart(
                        difficulty_label(COURSE_RATING_ORDER[chart % 6]),
                        (chart % 20) as u32,
                        chart % 7 != 6,
                        "chart",
                    );
                    if chart % 11 == 10 {
                        value.chart_type = "dance-double".to_string();
                    }
                    value
                })
                .collect();
            let key = song_unique_key(&song);
            if index % 5 != 0 {
                library.plays.insert(key.clone(), (index * 37 % 13) as u32);
            }
            if index % 7 != 0 {
                let mut grades = [0; 19];
                grades[index % 19] = (index % 4) as u32;
                grades[18] = u32::MAX - index as u32;
                library.grades.insert(key, grades);
            }
            let song = Arc::new(song);
            library.by_song.insert(format!("song{index}"), song.clone());
            library.by_group_song.insert(
                (group.to_ascii_lowercase(), format!("song{index}")),
                song.clone(),
            );
            library.groups.entry(group).or_default().push(song.clone());
            library.songs.push(song);
        }
        library
    }

    fn resolve(
        &self,
        resolver: Resolver,
        entry: &CourseEntry,
        seed: u64,
        course_type: CourseType,
        selected: &[String],
        difficulty: Difficulty,
    ) -> Option<ResolvedCourseStage> {
        resolver(
            Path::new("Courses/\u{8ab2}\u{984c}/course.crs"),
            seed as usize % 11,
            seed,
            entry,
            &self.by_group_song,
            &self.by_song,
            &self.songs,
            &self.groups,
            &self.plays,
            &self.grades,
            course_type,
            selected,
            "DANCE-SINGLE",
            difficulty,
        )
    }
}

fn entry(song: CourseSong) -> CourseEntry {
    let mut entry = fixed_course(None, "Song").entries.remove(0);
    entry.song = song;
    entry.steps = StepsSpec::MeterRange { low: 0, high: 20 };
    entry.modifiers = "1.5x, Mirror".to_string();
    entry.gain_seconds = -0.0;
    entry.gain_lives = 3;
    entry
}

fn assert_stage_equal(old: Option<ResolvedCourseStage>, new: Option<ResolvedCourseStage>) {
    match (old, new) {
        (Some(old), Some(new)) => {
            assert!(Arc::ptr_eq(&old.song, &new.song));
            assert_eq!(old.chart_index, new.chart_index);
            assert_eq!(old.modifiers, new.modifiers);
            assert_eq!(old.gain_seconds.to_bits(), new.gain_seconds.to_bits());
            assert_eq!(old.gain_lives, new.gain_lives);
        }
        (None, None) => {}
        (old, new) => panic!("stage existence differs: old={old:?}, new={new:?}"),
    }
}

fn assert_candidates_equal(old: &baseline::CourseCandidates, new: &CourseCandidates<'_>) {
    assert_eq!(old.items.len(), new.items.len());
    for (old_item, new_item) in old.items.iter().zip(&new.items) {
        assert!(Arc::ptr_eq(&old_item.song, new_item.song));
        assert_eq!(old_item.source_index, new_item.source_index);
        assert_eq!(
            old.chart_indices[old_item.chart_indices.clone()],
            new.chart_indices[new_item.chart_indices.clone()]
        );
        assert!(new_item.song_key.get().is_none());
    }
}

#[test]
fn candidates_borrow_songs_and_preserve_matching_chart_order() {
    for count in [0, 1, 9, 64] {
        let library = Library::new(count, 24);
        let mut entry = entry(CourseSong::RandomAny);
        for steps in [
            StepsSpec::MeterRange { low: 4, high: 12 },
            StepsSpec::MeterRange { low: 12, high: 4 },
            StepsSpec::Difficulty(Difficulty::Hard),
            StepsSpec::Unknown { raw: "?".into() },
        ] {
            entry.steps = steps;
            let old = baseline::course_candidates(&library.songs, &entry, "dance-single");
            let owners: Vec<_> = library.songs.iter().map(Arc::strong_count).collect();
            let new = course_candidates(&library.songs, &entry, "dance-single");
            assert_candidates_equal(&old, &new);
            for (song, count) in library.songs.iter().zip(owners) {
                assert_eq!(Arc::strong_count(song), count);
            }
        }
    }
}

fn selectors() -> Vec<SongSelect> {
    vec![
        SongSelect::default(),
        SongSelect {
            groups: vec![
                "Pack2".into(),
                "missing".into(),
                "Pack0".into(),
                "Pack2".into(),
            ],
            ..SongSelect::default()
        },
        SongSelect {
            groups: vec!["pack0".into()],
            ..SongSelect::default()
        },
        SongSelect {
            titles: vec!["Song3".into(), "Title4".into(), "Translit5".into()],
            ..SongSelect::default()
        },
        SongSelect {
            titles: vec!["title4".into()],
            ..SongSelect::default()
        },
        SongSelect {
            artists: vec!["Artist0".into(), "Translated1".into()],
            ..SongSelect::default()
        },
        SongSelect {
            genres: vec!["Genre1".into()],
            difficulties: vec![Difficulty::Hard],
            meter_range: Some((3, 9)),
            ..SongSelect::default()
        },
        SongSelect {
            bpm_range: Some((120.0, 180.0)),
            duration_range: Some((62.0, 75.0)),
            ..SongSelect::default()
        },
        SongSelect {
            bpm_range: Some((f64::NAN, f64::NAN)),
            duration_range: Some((f32::NAN, f32::NAN)),
            ..SongSelect::default()
        },
        SongSelect {
            bpm_range: Some((180.0, 120.0)),
            ..SongSelect::default()
        },
        SongSelect {
            duration_range: Some((90.0, 30.0)),
            ..SongSelect::default()
        },
        SongSelect {
            artists: vec!["missing".into()],
            ..SongSelect::default()
        },
    ]
}

#[test]
fn select_filters_preserve_groups_duplicates_metadata_and_chart_indices() {
    let library = Library::new(64, 24);
    for select in selectors() {
        let entry = entry(CourseSong::Select(select.clone()));
        let pool = baseline::select_song_pool(&select, &library.songs, &library.groups);
        let mut old = baseline::course_candidates(&pool, &entry, "dance-single");
        old.items
            .retain(|candidate| song_select_matches(&candidate.song, &select));
        let new = select_course_candidates(
            &select,
            &library.songs,
            &library.groups,
            &entry,
            "dance-single",
        );
        assert_candidates_equal(&old, &new);
    }
}

#[test]
fn complete_stage_selection_matches_baseline_across_seeds_and_repeat_modes() {
    let library = Library::new(40, 24);
    let mut entries = vec![
        entry(CourseSong::RandomAny),
        entry(CourseSong::RandomWithinGroup {
            group: " PACK2 ".into(),
        }),
        entry(CourseSong::Fixed {
            group: Some(" PACK0 ".into()),
            song: " SONG4 ".into(),
        }),
        entry(CourseSong::Fixed {
            group: None,
            song: "Song7".into(),
        }),
        entry(CourseSong::Unknown { raw: "?".into() }),
    ];
    for select in selectors() {
        entries.push(entry(CourseSong::Select(select)));
    }
    for sort in [
        SongSort::MostPlays,
        SongSort::FewestPlays,
        SongSort::TopGrades,
        SongSort::LowestGrades,
    ] {
        for index in [-1, 0, 1, 17, 39, 40, i32::MAX] {
            entries.push(entry(CourseSong::SortPick { sort, index }));
            entries.push(entry(CourseSong::Select(SongSelect {
                groups: vec!["Pack2".into(), "Pack0".into(), "Pack2".into()],
                sort: Some(sort),
                index,
                ..SongSelect::default()
            })));
        }
    }
    let selected: Vec<_> = library
        .songs
        .iter()
        .map(|song| song_unique_key(song))
        .collect();
    for (index, mut entry) in entries.into_iter().enumerate() {
        entry.no_difficult = index % 3 == 0;
        for course_type in [
            CourseType::Nonstop,
            CourseType::Oni,
            CourseType::Endless,
            CourseType::Survival,
        ] {
            for history in [
                &selected[..0],
                &selected[..1],
                &selected[..8],
                &selected[..9],
                &selected[..39],
                &selected[..],
            ] {
                for seed in [0, 1, 17, u64::MAX] {
                    for difficulty in COURSE_RATING_ORDER {
                        assert_stage_equal(
                            library.resolve(
                                baseline::resolve_course_stage,
                                &entry,
                                seed,
                                course_type,
                                history,
                                difficulty,
                            ),
                            library.resolve(
                                resolve_course_stage,
                                &entry,
                                seed,
                                course_type,
                                history,
                                difficulty,
                            ),
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn ranked_picks_match_every_rank_with_ties_missing_scores_and_reordered_candidates() {
    for count in [0, 1, 2, 3, 9, 64, 257] {
        let mut library = Library::new(count, 3);
        let entry = entry(CourseSong::RandomAny);
        for tied in [false, true] {
            if tied {
                library.plays.clear();
                library.grades.clear();
            }
            let mut old = baseline::course_candidates(&library.songs, &entry, "dance-single");
            let mut new = course_candidates(&library.songs, &entry, "dance-single");
            old.items.reverse();
            new.items.reverse();
            if count > 3 {
                old.items.swap_remove(1);
                new.items.swap_remove(1);
            }
            for sort in [
                SongSort::MostPlays,
                SongSort::FewestPlays,
                SongSort::TopGrades,
                SongSort::LowestGrades,
            ] {
                for pick in 0..=new.items.len() {
                    let expected = baseline::selected_course_candidate_index(
                        &old.items,
                        sort,
                        pick,
                        &library.plays,
                        &library.grades,
                    );
                    let actual = selected_course_candidate_index(
                        &new.items,
                        sort,
                        pick,
                        &library.plays,
                        &library.grades,
                    );
                    assert_eq!(
                        expected, actual,
                        "count={count}, tied={tied}, sort={sort:?}, pick={pick}"
                    );
                    if tied && pick == 0 && !new.items.is_empty() {
                        assert_eq!(new.items[actual.unwrap()].source_index, 0);
                    }
                }
            }
        }
    }
}

#[test]
fn filtered_storage_and_extreme_rank_churn_stay_bounded() {
    let library = Library::new(64, 24);
    let select = SongSelect {
        artists: vec!["missing".into()],
        ..SongSelect::default()
    };
    let entry = entry(CourseSong::Select(select.clone()));
    assert_no_churn(|| {
        assert!(
            select_course_candidates(
                &select,
                &library.songs,
                &library.groups,
                &entry,
                "dance-single"
            )
            .items
            .is_empty()
        );
    });
    let select = SongSelect {
        titles: vec!["Title4".into()],
        ..SongSelect::default()
    };
    let entry = self::entry(CourseSong::Select(select.clone()));
    assert_churn_budget(
        2,
        library.songs.len() * std::mem::size_of::<CourseCandidate<'_>>()
            + 24 * std::mem::size_of::<usize>(),
        || {
            black_box(select_course_candidates(
                &select,
                &library.songs,
                &library.groups,
                &entry,
                "dance-single",
            ));
        },
    );
    let entry = self::entry(CourseSong::RandomAny);
    let new = course_candidates(&library.songs, &entry, "dance-single");
    for candidate in &new.items {
        black_box(candidate.song_key());
    }
    for sort in [
        SongSort::MostPlays,
        SongSort::FewestPlays,
        SongSort::TopGrades,
        SongSort::LowestGrades,
    ] {
        for pick in [0, new.items.len() - 1] {
            assert_no_churn(|| {
                black_box(selected_course_candidate_index(
                    &new.items,
                    sort,
                    pick,
                    &library.plays,
                    &library.grades,
                ));
            });
        }
    }
    let single = course_candidates(&library.songs[..1], &entry, "dance-single");
    assert_no_churn(|| {
        assert_eq!(
            selected_course_candidate_index(
                &single.items,
                SongSort::TopGrades,
                0,
                &library.plays,
                &library.grades
            ),
            Some(0)
        );
        assert_eq!(
            selected_course_candidate_index(
                &single.items,
                SongSort::MostPlays,
                1,
                &library.plays,
                &library.grades
            ),
            None
        );
    });
    assert!(single.items[0].song_key.get().is_none());
}

#[test]
fn stage_result_owns_selected_song_after_library_is_dropped() {
    let stage = {
        let library = Library::new(1, 12);
        library
            .resolve(
                resolve_course_stage,
                &entry(CourseSong::RandomAny),
                0,
                CourseType::Nonstop,
                &[],
                Difficulty::Medium,
            )
            .unwrap()
    };
    assert_eq!(Arc::strong_count(&stage.song), 1);
    assert!(stage.song.charts[stage.chart_index].has_note_data);
}

// Adapted control: keep borrowed candidates, but apply metadata after chart
// construction as in the baseline. This isolates predicate placement.
fn select_after_charts<'a>(
    songs: &'a [Arc<SongData>],
    select: &SongSelect,
    entry: &CourseEntry,
) -> CourseCandidates<'a> {
    let mut candidates = course_candidates(songs, entry, "dance-single");
    candidates
        .items
        .retain(|candidate| song_select_matches(candidate.song, select));
    candidates
}

fn select_before_charts<'a>(
    songs: &'a [Arc<SongData>],
    select: &SongSelect,
    entry: &CourseEntry,
) -> CourseCandidates<'a> {
    filtered_course_candidates(songs.iter().enumerate(), select, entry, "dance-single")
}

fn pair(name: &str, mut old: impl FnMut(&str), mut new: impl FnMut(&str)) {
    let old_name = format!("{name}_old");
    let new_name = format!("{name}_new");
    if std::env::var_os("DEADSYNC_PERF_REVERSE").is_some() {
        new(&new_name);
        old(&old_name);
    } else {
        old(&old_name);
        new(&new_name);
    }
}

#[test]
#[ignore = "manual release benchmark; run alone with --nocapture --test-threads=1"]
fn course_selection_bench() {
    type Builder = for<'a> fn(&'a [Arc<SongData>], &CourseEntry, &str) -> CourseCandidates<'a>;
    let library = Library::new(2048, 12);
    let random = entry(CourseSong::RandomAny);
    type Filter =
        for<'a> fn(&'a [Arc<SongData>], &SongSelect, &CourseEntry) -> CourseCandidates<'a>;
    for (name, select) in [
        (
            "filter_artist",
            SongSelect {
                artists: vec!["Artist0".into()],
                ..SongSelect::default()
            },
        ),
        (
            "filter_bpm",
            SongSelect {
                bpm_range: Some((100.0, 200.0)),
                ..SongSelect::default()
            },
        ),
    ] {
        let entry = entry(CourseSong::Select(select.clone()));
        pair(
            name,
            |name| {
                measure_sampled(name, 64, library.songs.len(), || {
                    black_box(select_after_charts as Filter)(
                        black_box(&library.songs),
                        black_box(&select),
                        black_box(&entry),
                    )
                });
            },
            |name| {
                measure_sampled(name, 64, library.songs.len(), || {
                    black_box(select_before_charts as Filter)(
                        black_box(&library.songs),
                        black_box(&select),
                        black_box(&entry),
                    )
                });
            },
        );
    }
    pair(
        "candidate_build",
        |name| {
            measure_sampled(name, 64, library.songs.len(), || {
                black_box(
                    baseline::course_candidates
                        as fn(&[Arc<SongData>], &CourseEntry, &str) -> baseline::CourseCandidates,
                )(
                    black_box(&library.songs),
                    black_box(&random),
                    "dance-single",
                )
            });
        },
        |name| {
            measure_sampled(name, 64, library.songs.len(), || {
                black_box(course_candidates as Builder)(
                    black_box(&library.songs),
                    black_box(&random),
                    "dance-single",
                )
            });
        },
    );
    for (name, course_song) in [
        ("resolve_random", CourseSong::RandomAny),
        (
            "resolve_select_bpm",
            CourseSong::Select(SongSelect {
                bpm_range: Some((100.0, 200.0)),
                ..SongSelect::default()
            }),
        ),
        (
            "resolve_select_all",
            CourseSong::Select(SongSelect::default()),
        ),
        (
            "resolve_select_artist",
            CourseSong::Select(SongSelect {
                artists: vec!["Artist0".into()],
                ..SongSelect::default()
            }),
        ),
        (
            "resolve_select_none",
            CourseSong::Select(SongSelect {
                artists: vec!["missing".into()],
                ..SongSelect::default()
            }),
        ),
        (
            "resolve_select_groups",
            CourseSong::Select(SongSelect {
                groups: vec!["Pack2".into(), "Pack0".into(), "Pack2".into()],
                ..SongSelect::default()
            }),
        ),
        (
            "resolve_most_plays",
            CourseSong::SortPick {
                sort: SongSort::MostPlays,
                index: 0,
            },
        ),
        (
            "resolve_top_grades",
            CourseSong::SortPick {
                sort: SongSort::TopGrades,
                index: 0,
            },
        ),
        (
            "resolve_middle",
            CourseSong::SortPick {
                sort: SongSort::FewestPlays,
                index: 1024,
            },
        ),
    ] {
        let entry = entry(course_song);
        pair(
            name,
            |name| {
                measure_sampled(name, 32, 1, || {
                    library.resolve(
                        black_box(baseline::resolve_course_stage as Resolver),
                        black_box(&entry),
                        black_box(17),
                        CourseType::Nonstop,
                        &[],
                        Difficulty::Medium,
                    )
                });
            },
            |name| {
                measure_sampled(name, 32, 1, || {
                    library.resolve(
                        black_box(resolve_course_stage as Resolver),
                        black_box(&entry),
                        black_box(17),
                        CourseType::Nonstop,
                        &[],
                        Difficulty::Medium,
                    )
                });
            },
        );
    }
    type Ranker = fn(
        &[CourseCandidate<'_>],
        SongSort,
        usize,
        &HashMap<String, u32>,
        &HashMap<String, CourseGradeCounts>,
    ) -> Option<usize>;
    let history: Vec<_> = library.songs[..512]
        .iter()
        .map(|song| song_unique_key(song))
        .collect();
    for (name, selected) in [
        ("resolve_endless_8", &history[..8]),
        ("resolve_endless_512", &history[..]),
    ] {
        pair(
            name,
            |name| {
                measure_sampled(name, 32, 1, || {
                    library.resolve(
                        black_box(baseline::resolve_course_stage as Resolver),
                        black_box(&random),
                        17,
                        CourseType::Endless,
                        black_box(selected),
                        Difficulty::Medium,
                    )
                });
            },
            |name| {
                measure_sampled(name, 32, 1, || {
                    library.resolve(
                        black_box(resolve_course_stage as Resolver),
                        black_box(&random),
                        17,
                        CourseType::Endless,
                        black_box(selected),
                        Difficulty::Medium,
                    )
                });
            },
        );
    }
    let old = baseline::course_candidates(&library.songs, &random, "dance-single");
    let new = course_candidates(&library.songs, &random, "dance-single");
    for candidate in &old.items {
        black_box(candidate.song_key());
    }
    for candidate in &new.items {
        black_box(candidate.song_key());
    }
    for (name, sort, pick) in [
        ("rank_most_first", SongSort::MostPlays, 0),
        ("rank_fewest_last", SongSort::FewestPlays, 2047),
        ("rank_top_first", SongSort::TopGrades, 0),
        ("rank_lowest_last", SongSort::LowestGrades, 2047),
        ("rank_middle", SongSort::MostPlays, 1024),
    ] {
        pair(
            name,
            |name| {
                measure_sampled(name, 128, library.songs.len(), || {
                    black_box(
                        baseline::selected_course_candidate_index
                            as fn(
                                &[baseline::CourseCandidate],
                                SongSort,
                                usize,
                                &HashMap<String, u32>,
                                &HashMap<String, CourseGradeCounts>,
                            ) -> Option<usize>,
                    )(
                        black_box(&old.items),
                        black_box(sort),
                        black_box(pick),
                        black_box(&library.plays),
                        black_box(&library.grades),
                    )
                });
            },
            |name| {
                measure_sampled(name, 128, library.songs.len(), || {
                    black_box(selected_course_candidate_index as Ranker)(
                        black_box(&new.items),
                        black_box(sort),
                        black_box(pick),
                        black_box(&library.plays),
                        black_box(&library.grades),
                    )
                });
            },
        );
    }
    for (name, count, charts, song) in [
        ("resolve_empty", 0, 12, CourseSong::RandomAny),
        (
            "resolve_single",
            1,
            12,
            CourseSong::SortPick {
                sort: SongSort::MostPlays,
                index: 0,
            },
        ),
        ("resolve_small", 32, 6, CourseSong::RandomAny),
        (
            "resolve_no_charts",
            2048,
            0,
            CourseSong::Select(SongSelect::default()),
        ),
    ] {
        let library = Library::new(count, charts);
        let entry = entry(song);
        pair(
            name,
            |name| {
                measure_sampled(name, 512, 1, || {
                    library.resolve(
                        black_box(baseline::resolve_course_stage as Resolver),
                        black_box(&entry),
                        17,
                        CourseType::Nonstop,
                        &[],
                        Difficulty::Medium,
                    )
                });
            },
            |name| {
                measure_sampled(name, 512, 1, || {
                    library.resolve(
                        black_box(resolve_course_stage as Resolver),
                        black_box(&entry),
                        17,
                        CourseType::Nonstop,
                        &[],
                        Difficulty::Medium,
                    )
                });
            },
        );
    }
}
