use super::*;
use deadsync_chart::course::CourseStagePlan;
use deadsync_chart::{SongPack, SyncPref};
use deadsync_simfile::cache::build_song_meta;
use deadsync_simfile::course::{CourseEntry, CourseFile, CourseSong, StepsSpec};
use deadsync_simfile::song::{ParseSongOptions, parse_song_bytes};
use deadsync_theme::views::CourseTypeView;

fn selected_state(repeat: bool) -> State {
    selected_state_with_songs(repeat, true)
}

fn selected_state_with_songs(repeat: bool, include_songs: bool) -> State {
    let song_path = Path::new("Songs/Fixture/Track/chart.sm");
    let source = b"#TITLE:Track;\n#BPMS:0=120;\n\
        #NOTES:dance-single::Medium:4:0,0,0,0,0:\n1000\n0000\n0100\n0000\n;\n\
        #NOTES:dance-single::Hard:9:0,0,0,0,0:\n1000\n0100\n0010\n0001\n;\n";
    let parsed = parse_song_bytes(
        source,
        "sm",
        song_path,
        &ParseSongOptions::new(Vec::new(), Vec::new(), Vec::new()),
        |_| 20.0,
    )
    .unwrap();
    let song = Arc::new(build_song_meta(parsed, 0.0));
    let path = PathBuf::from("Courses/Fixture/course.crs");
    let course = CourseFile {
        name: "Fixture Course".to_owned(),
        name_translit: String::new(),
        scripter: "Fixture Scripter".to_owned(),
        description: String::new(),
        banner: String::new(),
        background: String::new(),
        repeat,
        lives: 3,
        meters: [None, None, Some(5), Some(13), None, None],
        entries: (0..2)
            .map(|index| CourseEntry {
                song: CourseSong::Fixed {
                    group: Some("Fixture".to_owned()),
                    song: "Track".to_owned(),
                },
                steps: StepsSpec::Difficulty(Difficulty::Medium),
                modifiers: format!("{index}, Mirror, 1.5x"),
                secret: false,
                no_difficult: false,
                gain_seconds: 2.75 + index as f32,
                gain_lives: index - 1,
            })
            .collect(),
    };
    let mut state = init(SelectCourseInitView {
        song_packs: vec![SongPack {
            group_name: "Fixture".to_owned(),
            name: "Fixture".to_owned(),
            sort_title: "Fixture".to_owned(),
            translit_title: String::new(),
            series: String::new(),
            folder_series: String::new(),
            year: 0,
            sync_pref: SyncPref::Default,
            directory: "Songs/Fixture".into(),
            banner_path: None,
            songs: include_songs.then_some(song).into_iter().collect(),
        }],
        courses: vec![(path.clone(), course)],
        last_course_path: Some(path.clone()),
        last_course_difficulty: Some("Hard".to_owned()),
        ..Default::default()
    });
    assert!(restore_selection_for_course(
        &mut state,
        &path,
        Some("Hard")
    ));
    state
}

fn assert_stages(actual: &[CourseStagePlan], expected: &[CourseStagePlan]) {
    assert_eq!(actual.len(), expected.len());
    for (actual, expected) in actual.iter().zip(expected) {
        assert!(Arc::ptr_eq(&actual.song, &expected.song));
        assert_eq!(actual.chart_hash, expected.chart_hash);
        assert_eq!(actual.modifiers, expected.modifiers);
        assert_eq!(actual.gain_seconds, expected.gain_seconds);
        assert_eq!(actual.gain_lives, expected.gain_lives);
    }
}

#[test]
fn startup_and_legacy_views_preserve_summary_metadata_without_affecting_cycles() {
    let mut state = selected_state(true);
    let original_cycle = selected_course_cycle(&state).unwrap();
    let canonical = |state: &State| {
        course_catalog(state)
            .start(selected_course_choice(state).unwrap())
            .unwrap()
    };
    let original = canonical(&state);
    let meta = Arc::make_mut(state.course_selection.meta.as_mut().unwrap());
    meta.name = "Localized course title".to_owned();
    meta.banner_path = Some("theme/course-banner.png".into());
    meta.min_bpm = Some(70.0);
    meta.max_bpm = Some(190.0);
    meta.total_length_seconds = 111;
    let rating = meta.ratings[Difficulty::Hard as usize].as_mut().unwrap();
    rating.course_stepchart_label = "Custom presentation label".to_owned();
    rating.min_bpm = Some(130.0);
    rating.max_bpm = Some(150.0);
    rating.total_length_seconds = 222;

    let prepared = canonical(&state);
    assert_eq!(prepared.song.title, original.song.title);
    assert_eq!(prepared.song.banner_path, original.song.banner_path);
    assert_eq!(prepared.song.min_bpm, original.song.min_bpm);
    assert_eq!(
        prepared.song.total_length_seconds,
        original.song.total_length_seconds
    );
    assert_stages(&prepared.plan.stages, &original.plan.stages);
    let (plan, presentation) = selected_course_start(&state).unwrap();
    assert_eq!(plan.path, original_cycle.path);
    assert_eq!(plan.score_hash, course_score_hash(&plan.path));
    assert_eq!(plan.course_difficulty_name, "Hard");
    assert_eq!(plan.course_meter, Some(13));
    assert_eq!(plan.course_type, CourseTypeView::Endless);
    assert_eq!(plan.lives, 3);
    assert_stages(&plan.stages, &original_cycle.stages);
    assert_eq!(presentation.name, "Localized course title");
    assert_eq!(
        presentation.banner_path.as_deref(),
        Some(Path::new("theme/course-banner.png"))
    );
    assert_eq!(
        presentation.course_stepchart_label,
        "Custom presentation label"
    );
    assert_eq!(presentation.song_stub.simfile_path, plan.path);
    assert_eq!(presentation.song_stub.title, presentation.name);
    assert_eq!(presentation.song_stub.artist, "Fixture Scripter");
    assert_eq!(presentation.song_stub.banner_path, presentation.banner_path);
    assert_eq!(presentation.song_stub.min_bpm, 70.0);
    assert_eq!(presentation.song_stub.max_bpm, 190.0);
    assert_eq!(presentation.song_stub.music_length_seconds, 111.0);
    assert_eq!(presentation.song_stub.total_length_seconds, 111);
    assert_eq!(
        presentation.song_stub.precise_note_last_second_seconds,
        Some(111.0)
    );

    let legacy: SelectedCoursePlan = selected_course_plan(&state).unwrap();
    assert_eq!(legacy.path, plan.path);
    assert_eq!(legacy.score_hash, plan.score_hash);
    assert_eq!(legacy.course_difficulty_name, plan.course_difficulty_name);
    assert_eq!(legacy.course_meter, plan.course_meter);
    assert_eq!(legacy.course_type, plan.course_type);
    assert_eq!(legacy.lives, plan.lives);
    assert_eq!(legacy.name, presentation.name);
    assert_eq!(legacy.banner_path, presentation.banner_path);
    assert_eq!(
        legacy.course_stepchart_label,
        presentation.course_stepchart_label
    );
    assert_eq!(legacy.song_stub.artist, presentation.song_stub.artist);
    assert_eq!(legacy.song_stub.min_bpm, presentation.song_stub.min_bpm);
    assert_eq!(legacy.song_stub.max_bpm, presentation.song_stub.max_bpm);
    assert_eq!(
        legacy.song_stub.music_length_seconds,
        presentation.song_stub.music_length_seconds
    );
    assert_stages(&legacy.stages, &plan.stages);
    assert_stages(&selected_course_cycle(&state).unwrap().stages, &plan.stages);
    assert_stages(
        &reroll_selected_endless_cycle(&state).unwrap().stages,
        &plan.stages,
    );
    assert_stages(
        &reroll_selected_endless_plan(&state).unwrap().stages,
        &plan.stages,
    );
}

#[test]
fn compatibility_parts_move_stage_storage_strings_and_summary_arc() {
    let state = selected_state(true);
    let mut legacy = selected_course_plan(&state).unwrap();
    legacy.banner_path = Some("theme/retained-banner.png".into());
    legacy.stages.reserve(11);
    let path = legacy.path.as_os_str().as_encoded_bytes().as_ptr();
    let banner = legacy
        .banner_path
        .as_ref()
        .unwrap()
        .as_os_str()
        .as_encoded_bytes()
        .as_ptr();
    let name = legacy.name.as_ptr();
    let score_hash = legacy.score_hash.as_ptr();
    let difficulty = legacy.course_difficulty_name.as_ptr();
    let label = legacy.course_stepchart_label.as_ptr();
    let stages = legacy.stages.as_ptr();
    let capacity = legacy.stages.capacity();
    let modifiers = legacy.stages[0].modifiers.as_ptr();
    let song_stub = Arc::clone(&legacy.song_stub);
    let stub_references = Arc::strong_count(&song_stub);
    let stage_song = Arc::clone(&legacy.stages[0].song);
    let stage_references = Arc::strong_count(&stage_song);

    let (plan, presentation) = legacy.into_parts();
    assert_eq!(plan.path.as_os_str().as_encoded_bytes().as_ptr(), path);
    assert_eq!(plan.score_hash.as_ptr(), score_hash);
    assert_eq!(plan.course_difficulty_name.as_ptr(), difficulty);
    assert_eq!(plan.stages.as_ptr(), stages);
    assert_eq!(plan.stages.capacity(), capacity);
    assert_eq!(plan.stages[0].modifiers.as_ptr(), modifiers);
    assert_eq!(presentation.name.as_ptr(), name);
    assert_eq!(presentation.course_stepchart_label.as_ptr(), label);
    assert_eq!(
        presentation
            .banner_path
            .as_ref()
            .unwrap()
            .as_os_str()
            .as_encoded_bytes()
            .as_ptr(),
        banner
    );
    assert!(Arc::ptr_eq(&presentation.song_stub, &song_stub));
    assert_eq!(Arc::strong_count(&song_stub), stub_references);
    assert_eq!(Arc::strong_count(&stage_song), stage_references);

    let legacy = SelectedCoursePlan::from_parts(plan, presentation);
    assert_eq!(legacy.path.as_os_str().as_encoded_bytes().as_ptr(), path);
    assert_eq!(legacy.score_hash.as_ptr(), score_hash);
    assert_eq!(legacy.course_difficulty_name.as_ptr(), difficulty);
    assert_eq!(legacy.stages.as_ptr(), stages);
    assert_eq!(legacy.stages.capacity(), capacity);
    assert_eq!(legacy.stages[0].modifiers.as_ptr(), modifiers);
    assert_eq!(legacy.name.as_ptr(), name);
    assert_eq!(legacy.course_stepchart_label.as_ptr(), label);
    assert_eq!(
        legacy
            .banner_path
            .as_ref()
            .unwrap()
            .as_os_str()
            .as_encoded_bytes()
            .as_ptr(),
        banner
    );
    assert!(Arc::ptr_eq(&legacy.song_stub, &song_stub));
    assert_eq!(Arc::strong_count(&song_stub), stub_references);
    assert_eq!(Arc::strong_count(&stage_song), stage_references);
}

#[test]
fn readiness_and_cycle_helpers_preserve_empty_rating_and_reroll_fallbacks() {
    let missing = init(SelectCourseInitView::default());
    assert!(!has_selected_course(&missing));
    assert!(selected_course_start(&missing).is_none());
    assert!(selected_course_start_with_cycles(&missing).is_none());
    assert!(selected_course_cycle(&missing).is_none());
    assert!(selected_course_plan(&missing).is_none());
    assert!(reroll_selected_endless_cycle(&missing).is_none());
    assert!(reroll_selected_endless_plan(&missing).is_none());

    let mut state = selected_state(true);
    assert!(has_selected_course(&state));
    assert_eq!(selected_course_cycle(&state).unwrap().stages.len(), 2);
    state.course_selection.rating_index = usize::MAX;
    assert!(!has_selected_course(&state));
    assert!(selected_course_start(&state).is_none());
    assert!(selected_course_cycle(&state).is_none());
    assert!(selected_course_plan(&state).is_none());
    assert!(reroll_selected_endless_cycle(&state).is_none());
    assert!(reroll_selected_endless_plan(&state).is_none());

    let unresolved = selected_state_with_songs(true, false);
    assert!(!has_selected_course(&unresolved));
    assert!(selected_course_choice(&unresolved).is_some());
    assert!(selected_course_start_with_cycles(&unresolved).is_none());
    assert!(selected_course_cycle(&unresolved).is_none());
    assert!(reroll_selected_endless_cycle(&unresolved).is_none());

    let state = selected_state(false);
    assert!(has_selected_course(&state));
    assert!(selected_course_start(&state).is_some());
    let (_, _, source) = selected_course_start_with_cycles(&state).unwrap();
    assert!(source.is_none());
    assert!(selected_course_cycle(&state).is_some());
    assert!(reroll_selected_endless_cycle(&state).is_none());
    assert!(reroll_selected_endless_plan(&state).is_none());
}
