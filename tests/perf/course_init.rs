use super::*;

mod original {
    use super::*;
    include!("course_init_original.rs");
}

fn entry(song: CourseSong) -> course::CourseEntry {
    course::CourseEntry {
        song,
        steps: course::StepsSpec::Difficulty(Difficulty::Hard),
        modifiers: "1.5x, reverse".into(),
        secret: false,
        no_difficult: false,
        gain_seconds: 10.0,
        gain_lives: -1,
    }
}

fn init_fixture(stages: usize, song_choice: CourseSong) -> SelectCourseInitView {
    let mut source = song("Songs/Pack/Alpha/song.ssc", &["alpha"]);
    let song = Arc::get_mut(&mut source).unwrap();
    song.title = "Alpha".into();
    song.translit_title = "ALPHA".into();
    song.music_length_seconds = 123.75;
    song.total_length_seconds = 123;
    song.min_bpm = 120.0;
    song.max_bpm = 180.0;
    song.display_bpm = "120:180".into();
    song.charts[0].stats.total_steps = 123;
    SelectCourseInitView {
        song_packs: vec![pack("Pack", vec![source])],
        courses: vec![(
            PathBuf::from("C:/deadsync-perf-fixtures/course.crs"),
            CourseFile {
                name: "Fixture course".into(),
                name_translit: "Translated course".into(),
                scripter: "Author".into(),
                description: "Fixture description".into(),
                banner: String::new(),
                background: String::new(),
                repeat: false,
                lives: 0,
                meters: [None; 6],
                entries: vec![entry(song_choice); stages],
            },
        )],
        played_chart_counts: vec![("alpha".into(), 17)],
        chart_grades: vec![("alpha".into(), 3)],
        ..Default::default()
    }
}

fn fixed() -> CourseSong {
    CourseSong::Fixed {
        group: Some("Pack".into()),
        song: "Alpha".into(),
    }
}

fn assert_init_eq(before: InitData, after: InitData) {
    assert_eq!(
        before.course_meta_by_path.len(),
        after.course_meta_by_path.len()
    );
    for (path, expected) in &before.course_meta_by_path {
        let actual = &after.course_meta_by_path[path];
        assert_eq!(format!("{expected:?}"), format!("{actual:?}"));
        assert!(actual.ratings[actual.default_rating_index].is_some());
    }
    assert_eq!(
        format!("{:?}", before.all_entries),
        format!("{:?}", after.all_entries)
    );
    for (old, new) in before.all_entries.iter().zip(&after.all_entries) {
        if let (MusicWheelEntry::Song(old), MusicWheelEntry::Song(new)) = (old, new) {
            assert_eq!(
                before
                    .course_text_color_overrides
                    .get(&(Arc::as_ptr(old) as usize)),
                after
                    .course_text_color_overrides
                    .get(&(Arc::as_ptr(new) as usize))
            );
        }
    }
    assert_eq!(
        before.resolver.song_play_counts,
        after.resolver.song_play_counts
    );
    assert_eq!(
        before.resolver.song_grade_counts,
        after.resolver.song_grade_counts
    );
    assert_eq!(
        before.resolver.target_chart_type,
        after.resolver.target_chart_type
    );
    assert_eq!(
        before
            .resolver
            .by_song
            .keys()
            .collect::<std::collections::HashSet<_>>(),
        after.resolver.by_song.keys().collect()
    );
    assert_eq!(
        before.resolver.all_songs.len(),
        after.resolver.all_songs.len()
    );
}

#[test]
fn rating_metadata_replaces_the_discarded_preliminary_resolution() {
    for choice in [
        fixed(),
        CourseSong::RandomAny,
        CourseSong::RandomWithinGroup {
            group: "Pack".into(),
        },
        CourseSong::SortPick {
            sort: SongSort::MostPlays,
            index: 0,
        },
        CourseSong::Select(course::SongSelect {
            sort: Some(SongSort::MostPlays),
            ..Default::default()
        }),
        CourseSong::Unknown {
            raw: "invalid".into(),
        },
    ] {
        for stages in [0, 1, 4] {
            for meters in [
                [None; 6],
                [Some(-1); 6],
                [Some(0); 6],
                [None, None, None, Some(12), None, None],
                [None, None, None, None, None, Some(7)],
            ] {
                for available in [false, true] {
                    let mut view = init_fixture(stages, choice.clone());
                    view.courses[0].1.meters = meters;
                    view.courses[0].1.repeat = stages == 4;
                    view.translated_titles = true;
                    if !available {
                        view.song_packs.clear();
                    }
                    assert_init_eq(original::build_init_data(&view), build_init_data(&view));
                }
            }
        }
    }
}

#[test]
fn discarded_course_pass_no_longer_allocates_stage_work() {
    let view = init_fixture(8, fixed());
    // Initialize shared labels before scoped allocation counting.
    drop(original::build_init_data(&view));
    drop(build_init_data(&view));
    let (before, old) = crate::perf::measure(|| original::build_init_data(&view));
    let (after, new) = crate::perf::measure(|| build_init_data(&view));
    assert_init_eq(before, after);
    assert!(new.allocs < old.allocs);
    assert!(new.allocated_bytes < old.allocated_bytes);
}

#[test]
#[ignore = "paired release benchmark; run explicitly"]
fn benchmark_course_loading() {
    use crate::course_perf::compare;
    for stages in [0, 4, 32] {
        let view = init_fixture(stages, fixed());
        compare(
            &format!("init/fixed_{stages}"),
            || &view,
            original::build_init_data,
            build_init_data,
        );
    }
    let view = init_fixture(4, CourseSong::RandomAny);
    compare(
        "init/random_4",
        || &view,
        original::build_init_data,
        build_init_data,
    );
}
