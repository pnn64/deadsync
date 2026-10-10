use super::*;

mod original {
    use super::*;
    include!("course_resolve_originals.rs");
}

struct Fixture {
    songs: Vec<Arc<SongData>>,
    grouped: HashMap<(String, String), Arc<SongData>>,
    named: HashMap<String, Arc<SongData>>,
    groups: HashMap<String, Vec<Arc<SongData>>>,
    plays: HashMap<String, u32>,
    grades: HashMap<String, CourseGradeCounts>,
}

impl Fixture {
    fn new(charts: usize) -> Self {
        let songs: Vec<_> = ["Alpha", "Beta", "Gamma"]
            .map(|name| {
                song_with_charts(
                    &format!("Songs/Pack/{name}/song.ssc"),
                    (0..charts)
                        .map(|index| {
                            test_chart("Hard", 7 + index as u32, true, &format!("{name}-{index}"))
                        })
                        .collect(),
                )
            })
            .into();
        Self {
            grouped: ["alpha", "beta", "gamma"]
                .into_iter()
                .zip(&songs)
                .map(|(name, song)| (("pack".into(), name.into()), Arc::clone(song)))
                .collect(),
            named: ["alpha", "beta", "gamma"]
                .into_iter()
                .zip(&songs)
                .map(|(name, song)| (name.into(), Arc::clone(song)))
                .collect(),
            groups: HashMap::from([("Pack".into(), songs.clone())]),
            plays: songs
                .iter()
                .enumerate()
                .map(|(i, song)| (song_unique_key(song), i as u32 * 3))
                .collect(),
            grades: songs
                .iter()
                .enumerate()
                .map(|(i, song)| (song_unique_key(song), [i as u32; 19]))
                .collect(),
            songs,
        }
    }

    fn resolve(
        &self,
        before: bool,
        entry: &CourseEntry,
        seed: u64,
        chart_type: &str,
        diff: Difficulty,
        kind: CourseType,
        selected: &[String],
    ) -> Option<ResolvedCourseStage> {
        let resolve = if before {
            original::resolve_course_stage
        } else {
            resolve_course_stage
        };
        resolve(
            Path::new("Courses/Pack/course.crs"),
            7,
            seed,
            entry,
            &self.grouped,
            &self.named,
            &self.songs,
            &self.groups,
            &self.plays,
            &self.grades,
            kind,
            selected,
            chart_type,
            diff,
        )
    }
}

fn fixed_entry() -> CourseEntry {
    CourseEntry {
        song: CourseSong::Fixed {
            group: Some(" pAcK ".into()),
            song: " ALPHA ".into(),
        },
        steps: StepsSpec::Difficulty(Difficulty::Hard),
        modifiers: "1.5x, reverse, no mines".into(),
        secret: true,
        no_difficult: false,
        gain_seconds: -2.5,
        gain_lives: 3,
    }
}

fn assert_stage_eq(before: Option<ResolvedCourseStage>, after: Option<ResolvedCourseStage>) {
    assert_eq!(before.is_some(), after.is_some());
    if let (Some(a), Some(b)) = (before, after) {
        assert!(Arc::ptr_eq(&a.song, &b.song));
        assert_eq!(a.chart_index, b.chart_index);
        assert_eq!(a.modifiers, b.modifiers);
        assert_eq!(a.gain_seconds.to_bits(), b.gain_seconds.to_bits());
        assert_eq!(a.gain_lives, b.gain_lives);
    }
}

#[test]
fn fixed_resolution_preserves_filters_shifts_seeds_and_metadata() {
    for count in [0, 1, 8, 9, 64] {
        let fixture = Fixture::new(count);
        for seed in [0, 1, 42, u64::MAX] {
            for diff in COURSE_RATING_ORDER {
                for kind in [CourseType::Nonstop, CourseType::Endless] {
                    for chart_type in ["DANCE-SINGLE", "missing"] {
                        for steps in [
                            StepsSpec::Difficulty(Difficulty::Hard),
                            StepsSpec::Difficulty(Difficulty::Medium),
                            StepsSpec::MeterRange { low: 9, high: 15 },
                        ] {
                            let mut entry = fixed_entry();
                            entry.steps = steps;
                            entry.gain_seconds = f32::from_bits(0x7fc00042);
                            let selected = [song_unique_key(&fixture.songs[0])];
                            assert_stage_eq(
                                fixture
                                    .resolve(true, &entry, seed, chart_type, diff, kind, &selected),
                                fixture.resolve(
                                    false, &entry, seed, chart_type, diff, kind, &selected,
                                ),
                            );
                        }
                    }
                }
            }
        }
        for group in [None, Some("missing".into()), Some(" pAcK ".into())] {
            let mut entry = fixed_entry();
            entry.song = CourseSong::Fixed {
                group,
                song: " alpha ".into(),
            };
            entry.no_difficult = true;
            assert_stage_eq(
                fixture.resolve(
                    true,
                    &entry,
                    5,
                    "dance-single",
                    Difficulty::Challenge,
                    CourseType::Oni,
                    &[],
                ),
                fixture.resolve(
                    false,
                    &entry,
                    5,
                    "dance-single",
                    Difficulty::Challenge,
                    CourseType::Oni,
                    &[],
                ),
            );
        }
    }
}

#[test]
fn random_and_ranked_resolution_keep_seeded_choices_and_repeat_rules() {
    for count in [1, 4, 12] {
        let fixture = Fixture::new(count);
        let mut kinds = vec![
            CourseSong::RandomAny,
            CourseSong::RandomWithinGroup {
                group: " pAcK ".into(),
            },
            CourseSong::Unknown {
                raw: "unknown".into(),
            },
        ];
        for sort in [
            SongSort::MostPlays,
            SongSort::FewestPlays,
            SongSort::TopGrades,
            SongSort::LowestGrades,
        ] {
            for index in [-1, 0, 1, 2, 3] {
                kinds.push(CourseSong::SortPick { sort, index });
            }
        }
        kinds.push(CourseSong::Select(SongSelect {
            groups: vec!["Pack".into()],
            ..Default::default()
        }));
        for song in kinds {
            let entry = CourseEntry {
                song,
                ..fixed_entry()
            };
            for seed in 0..24 {
                for selected in [
                    vec![],
                    vec![song_unique_key(&fixture.songs[0])],
                    fixture
                        .songs
                        .iter()
                        .map(|song| song_unique_key(song))
                        .collect(),
                ] {
                    for kind in [CourseType::Nonstop, CourseType::Endless] {
                        assert_stage_eq(
                            fixture.resolve(
                                true,
                                &entry,
                                seed,
                                "dance-single",
                                Difficulty::Hard,
                                kind,
                                &selected,
                            ),
                            fixture.resolve(
                                false,
                                &entry,
                                seed,
                                "dance-single",
                                Difficulty::Hard,
                                kind,
                                &selected,
                            ),
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn fixed_resolution_preserves_mixed_chart_filters_and_difficulty_shifts() {
    let mut charts: Vec<_> = COURSE_RATING_ORDER
        .into_iter()
        .map(|diff| {
            test_chart(
                difficulty_label(diff),
                5 + diff as u32,
                true,
                difficulty_label(diff),
            )
        })
        .collect();
    let mut no_notes = test_chart("Hard", 9, false, "empty");
    no_notes.chart_type = "DANCE-SINGLE".into();
    charts.push(no_notes);
    let mut pump = test_chart("Hard", 12, true, "pump");
    pump.chart_type = "pump-single".into();
    charts.push(pump);
    let song = song_with_charts("Songs/Pack/Alpha/song.ssc", charts);
    let mut fixture = Fixture::new(0);
    fixture
        .grouped
        .insert(("pack".into(), "alpha".into()), Arc::clone(&song));
    fixture.named.insert("alpha".into(), song);
    for chart_type in ["dance-single", "DANCE-SINGLE", "pump-single"] {
        for diff in COURSE_RATING_ORDER {
            for no_difficult in [false, true] {
                let mut entry = fixed_entry();
                entry.no_difficult = no_difficult;
                for steps in [
                    StepsSpec::Difficulty(Difficulty::Hard),
                    StepsSpec::MeterRange { low: 0, high: 99 },
                    StepsSpec::Unknown {
                        raw: "unknown".into(),
                    },
                ] {
                    entry.steps = steps;
                    assert_stage_eq(
                        fixture.resolve(
                            true,
                            &entry,
                            42,
                            chart_type,
                            diff,
                            CourseType::Survival,
                            &[],
                        ),
                        fixture.resolve(
                            false,
                            &entry,
                            42,
                            chart_type,
                            diff,
                            CourseType::Survival,
                            &[],
                        ),
                    );
                }
            }
        }
    }
}

fn original_chart_pick(seed: u64, path: &Path, entry: usize, song: &SongData, len: usize) -> usize {
    original::random_pick_index(
        seed ^ song_key_hash(&song_unique_key(song)),
        path,
        entry ^ usize::MAX,
        len,
    )
}

#[test]
fn singleton_picks_skip_churn_and_keep_all_seeded_indices() {
    let fixture = Fixture::new(1);
    for path in [
        Path::new(""),
        Path::new("Courses/Pack/course.crs"),
        Path::new("曲/Été.crs"),
    ] {
        for seed in [0, 1, u64::MAX, 0x12345678] {
            for entry in [0, 7, usize::MAX] {
                for len in [0, 1, 2, 37, usize::MAX] {
                    assert_eq!(
                        original::random_pick_index(seed, path, entry, len),
                        random_pick_index(seed, path, entry, len)
                    );
                    assert_eq!(
                        original_chart_pick(seed, path, entry, &fixture.songs[0], len),
                        course_chart_pick_index(seed, path, entry, &fixture.songs[0], len)
                    );
                }
            }
        }
    }
    let (_, churn) = crate::perf::measure(|| {
        course_chart_pick_index(42, Path::new("course.crs"), 3, &fixture.songs[0], 1)
    });
    assert_eq!(churn.allocs, 0);
    assert_eq!(churn.reallocs, 0);
}

// Intermediate control retains the original candidate containers while using
// the same singleton shortcuts as production, isolating container removal.
fn buffered_fixed_stage(song: &Arc<SongData>, entry: &CourseEntry) -> Option<ResolvedCourseStage> {
    let mut candidates = course_candidates(std::slice::from_ref(song), entry, "dance-single");
    let candidate = candidates.items.pop()?;
    let pick = course_chart_pick_index(
        42,
        Path::new("course.crs"),
        7,
        candidate.song,
        candidate.chart_indices.len(),
    );
    let base_chart = *candidates
        .chart_indices
        .get(candidate.chart_indices)?
        .get(pick)?;
    Some(ResolvedCourseStage {
        song: Arc::clone(candidate.song),
        chart_index: shifted_chart_index(
            candidate.song,
            base_chart,
            entry,
            "dance-single",
            Difficulty::Medium,
        ),
        modifiers: entry.modifiers.clone(),
        gain_seconds: entry.gain_seconds,
        gain_lives: entry.gain_lives,
    })
}

fn direct_fixed_stage(song: &Arc<SongData>, entry: &CourseEntry) -> Option<ResolvedCourseStage> {
    fixed_course_stage(
        song,
        entry,
        "dance-single",
        Difficulty::Medium,
        42,
        Path::new("course.crs"),
        7,
    )
}

#[test]
fn small_fixed_candidates_remove_both_heap_buffers() {
    let fixture = Fixture::new(1);
    let mut entry = fixed_entry();
    entry.modifiers.clear();
    let (before, old) = crate::perf::measure(|| buffered_fixed_stage(&fixture.songs[0], &entry));
    let (after, new) = crate::perf::measure(|| direct_fixed_stage(&fixture.songs[0], &entry));
    assert_stage_eq(before, after);
    assert_eq!(old.allocs, 2);
    assert_eq!(new.allocs, 0);
    assert_eq!(new.reallocs, 0);
}

#[test]
#[ignore = "paired release benchmark; run explicitly"]
fn benchmark_course_loading() {
    use crate::course_perf::compare;
    let entry = fixed_entry();
    for count in [0, 1, 4, 64] {
        let fixture = Fixture::new(count);
        compare(
            &format!("resolve/fixed_{count}"),
            || &fixture,
            |f| {
                f.resolve(
                    true,
                    &entry,
                    42,
                    "dance-single",
                    Difficulty::Medium,
                    CourseType::Endless,
                    &[],
                )
            },
            |f| {
                f.resolve(
                    false,
                    &entry,
                    42,
                    "dance-single",
                    Difficulty::Medium,
                    CourseType::Endless,
                    &[],
                )
            },
        );
        assert_stage_eq(
            buffered_fixed_stage(&fixture.songs[0], &entry),
            direct_fixed_stage(&fixture.songs[0], &entry),
        );
        compare(
            &format!("fixed_buffers/{count}"),
            || &fixture.songs[0],
            |song| buffered_fixed_stage(song, &entry),
            |song| direct_fixed_stage(song, &entry),
        );
    }
    let fixture = Fixture::new(4);
    for len in [1, 4] {
        compare(
            &format!("chart_pick/{len}"),
            || {
                (
                    42,
                    Path::new("Courses/Pack/course.crs"),
                    7,
                    &*fixture.songs[0],
                    len,
                )
            },
            |(seed, path, entry, song, len)| original_chart_pick(seed, path, entry, song, len),
            |(seed, path, entry, song, len)| course_chart_pick_index(seed, path, entry, song, len),
        );
    }
    let entry = CourseEntry {
        song: CourseSong::RandomAny,
        ..entry
    };
    compare(
        "resolve/random_control",
        || &fixture,
        |f| {
            f.resolve(
                true,
                &entry,
                42,
                "dance-single",
                Difficulty::Medium,
                CourseType::Endless,
                &[],
            )
        },
        |f| {
            f.resolve(
                false,
                &entry,
                42,
                "dance-single",
                Difficulty::Medium,
                CourseType::Endless,
                &[],
            )
        },
    );
}
