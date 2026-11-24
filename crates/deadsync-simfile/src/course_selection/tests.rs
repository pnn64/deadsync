use super::*;
use crate::course::{CourseEntry, SongSelect, StepsSpec};
use deadsync_chart::{ArrowStats, SyncPref};
use std::path::PathBuf;

fn chart(hash: &str) -> ChartData {
    ChartData {
        radar_numerators: [0.0; 5],
        chart_type: "dance-single".to_owned(),
        difficulty: "Hard".to_owned(),
        description: String::new(),
        chart_name: String::new(),
        meter: 10,
        step_artist: String::new(),
        music_file: String::new(),
        music_path: None,
        short_hash: hash.to_owned(),
        stats: ArrowStats::default(),
        tech_counts: deadsync_chart::TechCounts::default(),
        mines_nonfake: 0,
        stamina_counts: deadsync_chart::StaminaCounts::default(),
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
        has_note_data: true,
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

fn song(path: &str, chart_hashes: &[&str]) -> Arc<SongData> {
    Arc::new(SongData {
        simfile_path: PathBuf::from(path),
        title: String::new(),
        subtitle: String::new(),
        translit_title: String::new(),
        translit_subtitle: String::new(),
        artist: String::new(),
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
        min_bpm: 0.0,
        max_bpm: 0.0,
        normalized_bpms: String::new(),
        song_timing: None,
        music_length_seconds: 0.0,
        first_second: 0.0,
        total_length_seconds: 0,
        precise_last_second_seconds: 0.0,
        precise_note_first_second_seconds: None,
        precise_note_last_second_seconds: None,
        last_second_hint: 0.0,
        charts: chart_hashes.iter().map(|hash| chart(hash)).collect(),
    })
}

fn pack(name: &str, songs: Vec<Arc<SongData>>) -> SongPack {
    SongPack {
        group_name: name.to_owned(),
        name: name.to_owned(),
        sort_title: String::new(),
        translit_title: String::new(),
        series: String::new(),
        folder_series: String::new(),
        year: 0,
        sync_pref: SyncPref::Default,
        directory: PathBuf::from("Songs").join(name),
        banner_path: None,
        songs,
    }
}

fn entry(song: CourseSong) -> CourseEntry {
    CourseEntry {
        song,
        steps: StepsSpec::Difficulty(Difficulty::Hard),
        modifiers: String::new(),
        secret: false,
        no_difficult: false,
        gain_seconds: 0.0,
        gain_lives: -1,
    }
}

fn fixed(name: &str) -> CourseEntry {
    entry(CourseSong::Fixed {
        group: None,
        song: name.to_owned(),
    })
}

fn course(entries: Vec<CourseEntry>) -> CourseFile {
    CourseFile {
        name: "Course".to_owned(),
        name_translit: String::new(),
        scripter: String::new(),
        description: String::new(),
        banner: String::new(),
        background: String::new(),
        repeat: false,
        lives: -1,
        meters: [None; 6],
        entries,
    }
}

#[test]
fn song_lookup_handles_sparse_duplicate_play_counts() {
    let alpha = song("Songs/Pack A/Alpha/alpha.ssc", &["duplicate", "alpha"]);
    let beta = song("Songs/Pack A/Beta/beta.ssc", &["beta"]);
    let gamma = song("Songs/Pack B/Gamma/gamma.ssc", &["duplicate"]);
    let packs = [
        pack("Pack A", vec![alpha.clone(), beta.clone()]),
        pack("Pack B", vec![gamma.clone()]),
        pack("Empty Pack", Vec::new()),
    ];
    let played = vec![
        ("duplicate".to_owned(), 5),
        ("duplicate".to_owned(), 7),
        ("alpha".to_owned(), u32::MAX),
        ("beta".to_owned(), 3),
        ("missing".to_owned(), 99),
    ];

    let (_, _, groups, all, counts) = build_song_lookup(&packs, &played);
    assert_eq!(groups.len(), 2, "empty packs must not create lookup groups");
    assert!(groups.contains_key("Pack A"));
    assert!(!groups.contains_key("pack a"));
    assert_eq!(all.len(), 3);
    assert_eq!(counts.get(&song_unique_key(&alpha)), Some(&u32::MAX));
    assert_eq!(counts.get(&song_unique_key(&beta)), Some(&3));
    assert!(!counts.contains_key(&song_unique_key(&gamma)));
}

#[test]
fn catalog_preserves_grouped_and_bare_name_precedence_and_grade_filtering() {
    let first = song("Songs/First/Same/a.ssc", &["shared"]);
    let mut last = song("Songs/Last/Same/b.ssc", &["shared", "invalid", "pump"]);
    Arc::make_mut(&mut last).charts[0].chart_type = "DANCE-SINGLE".to_owned();
    Arc::make_mut(&mut last).charts[2].chart_type = "pump-single".to_owned();
    let packs = [
        pack(" Pack ", vec![first.clone()]),
        pack("pack", vec![last.clone()]),
    ];
    let grades = [
        ("shared".to_owned(), 3),
        ("shared".to_owned(), 1),
        ("invalid".to_owned(), 19),
        ("pump".to_owned(), 0),
    ];
    let resolver = CourseResolver::new(&packs, &[], &grades, "dance-single");
    assert!(Arc::ptr_eq(resolver.by_song.get("same").unwrap(), &first));
    assert!(Arc::ptr_eq(
        resolver
            .by_group_song
            .get(&("pack".to_owned(), "same".to_owned()))
            .unwrap(),
        &last,
    ));
    assert_eq!(resolver.songs_by_group.len(), 2);
    assert!(resolver.songs_by_group.contains_key("Pack"));
    assert!(resolver.songs_by_group.contains_key("pack"));
    assert!(Arc::ptr_eq(&resolver.all_songs[0], &first));
    assert!(Arc::ptr_eq(&resolver.all_songs[1], &last));
    for song in [&first, &last] {
        let counts = resolver
            .song_grade_counts
            .get(&song_unique_key(song))
            .unwrap();
        assert_eq!(counts[1], 1);
        assert_eq!(counts.iter().sum::<u32>(), 1);
    }
}

#[test]
fn difficulty_policy_preserves_random_flags_explicit_meters_and_default_ties() {
    let mut course = course(vec![fixed("missing")]);
    let preparation = CoursePreparation::new(&course);
    assert_eq!(
        preparation.difficulties().collect::<Vec<_>>(),
        [Difficulty::Medium]
    );
    assert!(!preparation.has_random_entries);
    assert!(!preparation.has_most_played_entries);

    course.meters[Difficulty::Hard as usize] = Some(0);
    course.meters[Difficulty::Easy as usize] = Some(-1);
    let preparation = CoursePreparation::new(&course);
    assert_eq!(
        preparation.difficulties().collect::<Vec<_>>(),
        [Difficulty::Hard]
    );
    let ratings = [None, Some(()), None, None, None, Some(())];
    assert_eq!(
        preparation.default_rating_index(&ratings),
        Difficulty::Easy as usize
    );

    course.entries[0].song = CourseSong::SortPick {
        sort: SongSort::MostPlays,
        index: 0,
    };
    let preparation = CoursePreparation::new(&course);
    assert!(!preparation.has_random_entries);
    assert!(preparation.has_most_played_entries);
    assert_eq!(
        preparation.difficulties().collect::<Vec<_>>(),
        [Difficulty::Hard]
    );

    course.entries[0].song = CourseSong::Select(SongSelect {
        sort: Some(SongSort::MostPlays),
        ..Default::default()
    });
    let preparation = CoursePreparation::new(&course);
    assert!(preparation.has_random_entries);
    assert!(preparation.has_most_played_entries);
    assert_eq!(
        preparation.difficulties().collect::<Vec<_>>(),
        COURSE_RATING_ORDER
    );

    course.meters[Difficulty::Challenge as usize] = Some(12);
    let preparation = CoursePreparation::new(&course);
    assert_eq!(
        preparation.difficulties().collect::<Vec<_>>(),
        [Difficulty::Hard, Difficulty::Challenge],
    );
    course.repeat = true;
    assert_eq!(
        CoursePreparation::new(&course).course_type,
        CourseType::Endless
    );
}

#[test]
fn prepare_rating_keeps_exact_charts_dense_callbacks_payloads_and_numeric_totals() {
    let mut alpha = song("Songs/Pack/Alpha/a.ssc", &["unselected", "alpha"]);
    let alpha_data = Arc::make_mut(&mut alpha);
    alpha_data.charts[0].chart_type = "pump-single".to_owned();
    alpha_data.charts[1].meter = 9;
    alpha_data.charts[1].stats.total_steps = u32::MAX;
    alpha_data.charts[1].stats.jumps = 2;
    alpha_data.music_length_seconds = 10.6;
    alpha_data.max_bpm = 150.0;
    let mut beta = song("Songs/Pack/Beta/b.ssc", &["beta"]);
    let beta_data = Arc::make_mut(&mut beta);
    beta_data.charts[0].meter = 12;
    beta_data.charts[0].stats.total_steps = 7;
    beta_data.charts[0].stats.jumps = 3;
    beta_data.charts[0].mines_nonfake = 5;
    beta_data.music_length_seconds = f32::NAN;
    beta_data.total_length_seconds = 20;
    beta_data.min_bpm = 160.0;
    let packs = [pack("Pack", vec![alpha.clone(), beta.clone()])];
    let resolver = CourseResolver::new(&packs, &[], &[], "dance-single");
    let mut course = course(vec![fixed("missing"), fixed("Alpha"), fixed("Beta")]);
    course.entries[1].modifiers = "1.5x,Mirror".to_owned();
    course.entries[1].gain_seconds = f32::from_bits(0x7fc0_1234);
    course.entries[1].gain_lives = 0;
    course.entries[2].gain_seconds = -4.5;
    course.entries[2].gain_lives = 42;
    let mut callbacks = Vec::new();
    let prepared = resolver
        .prepare_rating(
            Path::new("Courses/partial.crs"),
            &course,
            Difficulty::Medium,
            7,
            |song, chart, index| {
                let (expected, chart_index) = if index == 0 { (&alpha, 1) } else { (&beta, 0) };
                assert!(std::ptr::eq(song, expected.as_ref()));
                assert!(std::ptr::eq(chart, &expected.charts[chart_index]));
                callbacks.push((index, chart.short_hash.clone()));
            },
        )
        .expect("partial resolved course");
    assert_eq!(callbacks, [(0, "alpha".to_owned()), (1, "beta".to_owned())]);
    assert_eq!(prepared.stages.len(), 2);
    assert!(Arc::ptr_eq(&prepared.stages[0].song, &alpha));
    assert!(Arc::ptr_eq(&prepared.stages[1].song, &beta));
    assert_eq!(prepared.stages[0].chart_hash, "alpha");
    assert_eq!(prepared.stages[0].modifiers, "1.5x,Mirror");
    assert_eq!(prepared.stages[0].gain_seconds.to_bits(), 0x7fc0_1234);
    assert_eq!(prepared.stages[0].gain_lives, 0);
    assert_eq!(prepared.stages[1].gain_seconds, -4.5);
    assert_eq!(prepared.stages[1].gain_lives, 42);
    assert_eq!(prepared.rated_entry_count, 2);
    assert_eq!(prepared.course_meter, Some(11));
    assert_eq!(prepared.total_length_seconds, 31);
    assert_eq!(
        (prepared.min_bpm, prepared.max_bpm),
        (Some(150.0), Some(160.0))
    );
    assert_eq!(prepared.totals.steps, u32::MAX);
    assert_eq!(prepared.totals.jumps, 5);
    assert_eq!(prepared.totals.mines, 5);

    course.meters[Difficulty::Medium as usize] = Some(0);
    let explicit = resolver
        .prepare_rating(
            Path::new("Courses/partial.crs"),
            &course,
            Difficulty::Medium,
            7,
            |_, _, _| {},
        )
        .unwrap();
    assert_eq!(explicit.course_meter, Some(0));
    assert_eq!(explicit.stages.len(), 2);
}

#[test]
fn empty_ratings_keep_medium_and_nonnegative_explicit_meters() {
    let resolver = CourseResolver::new(&[], &[], &[], "dance-single");
    let mut course = course(vec![fixed("missing")]);
    let path = Path::new("Courses/empty.crs");
    let medium = resolver
        .prepare_rating(path, &course, Difficulty::Medium, 0, |_, _, _| {
            panic!("an unresolved entry must not produce presentation")
        })
        .expect("implicit Medium rating is retained");
    assert!(medium.stages.is_empty());
    assert_eq!(medium.rated_entry_count, 0);
    assert_eq!(medium.course_meter, None);
    assert_eq!(medium.total_length_seconds, 0);
    assert_eq!((medium.min_bpm, medium.max_bpm), (None, None));
    assert!(
        resolver
            .prepare_rating(path, &course, Difficulty::Hard, 0, |_, _, _| {})
            .is_none()
    );
    course.meters[Difficulty::Hard as usize] = Some(-1);
    assert!(
        resolver
            .prepare_rating(path, &course, Difficulty::Hard, 0, |_, _, _| {})
            .is_none()
    );
    course.meters[Difficulty::Hard as usize] = Some(0);
    let hard = resolver
        .prepare_rating(path, &course, Difficulty::Hard, 0, |_, _, _| {})
        .expect("explicit zero meter remains selectable");
    assert!(hard.stages.is_empty());
    assert_eq!(hard.course_meter, Some(0));
    course.repeat = true;
    assert!(
        resolver
            .reroll_endless(path, &course, Difficulty::Medium, 0)
            .is_none()
    );
}

#[test]
fn seeded_stage_resolution_keeps_authored_indices_after_skipped_entries() {
    let packs = [pack(
        "Pack",
        vec![
            song("Songs/Pack/Alpha/a.ssc", &["a"]),
            song("Songs/Pack/Beta/b.ssc", &["b"]),
            song("Songs/Pack/Gamma/c.ssc", &["c"]),
        ],
    )];
    let resolver = CourseResolver::new(&packs, &[], &[], "dance-single");
    let course = course(vec![
        fixed("missing"),
        entry(CourseSong::RandomAny),
        fixed("also missing"),
        entry(CourseSong::RandomAny),
    ]);
    let path = Path::new("Courses/indices.crs");
    for seed in 0..16 {
        let prepared = resolver
            .prepare_rating(path, &course, Difficulty::Medium, seed, |_, _, _| {})
            .expect("two resolved random entries");
        let mut history = Vec::new();
        for (position, authored_index) in [1, 3].into_iter().enumerate() {
            let expected = resolve_course_stage(
                path,
                authored_index,
                seed,
                &course.entries[authored_index],
                &resolver.by_group_song,
                &resolver.by_song,
                &resolver.all_songs,
                &resolver.songs_by_group,
                &resolver.song_play_counts,
                &resolver.song_grade_counts,
                CourseType::Nonstop,
                &history,
                &resolver.target_chart_type,
                Difficulty::Medium,
            )
            .unwrap();
            let actual = &prepared.stages[position];
            assert!(Arc::ptr_eq(&actual.song, &expected.song));
            assert_eq!(
                actual.chart_hash,
                expected.song.charts[expected.chart_index].short_hash
            );
            history.push(song_unique_key(&expected.song));
        }
        assert_eq!(prepared.stages.len(), 2);
        assert!(!Arc::ptr_eq(
            &prepared.stages[0].song,
            &prepared.stages[1].song
        ));
    }
}

#[test]
fn endless_rerolls_share_seeded_resolution_and_reset_history_each_time() {
    let packs = [pack(
        "Pack",
        vec![
            song("Songs/Pack/Alpha/a.ssc", &["a"]),
            song("Songs/Pack/Beta/b.ssc", &["b"]),
            song("Songs/Pack/Gamma/c.ssc", &["c"]),
        ],
    )];
    let resolver = CourseResolver::new(&packs, &[], &[], "dance-single");
    let mut course = course((0..3).map(|_| entry(CourseSong::RandomAny)).collect());
    let path = Path::new("Courses/endless.crs");
    assert!(
        resolver
            .reroll_endless(path, &course, Difficulty::Medium, 42)
            .is_none()
    );
    course.repeat = true;
    for difficulty in [Difficulty::Easy, Difficulty::Medium, Difficulty::Hard] {
        let expected = resolver
            .prepare_rating(path, &course, difficulty, 42, |_, _, _| {})
            .expect("rating");
        let first = resolver
            .reroll_endless(path, &course, difficulty, 42)
            .expect("first cycle");
        let _ = resolver
            .reroll_endless(path, &course, difficulty, 99)
            .expect("different seed");
        let repeated = resolver
            .reroll_endless(path, &course, difficulty, 42)
            .expect("same seed");
        assert_eq!(first.len(), 3);
        assert_eq!(expected.stages.len(), first.len());
        assert_eq!(repeated.len(), first.len());
        for ((expected, first), repeated) in expected.stages.iter().zip(&first).zip(&repeated) {
            assert!(Arc::ptr_eq(&expected.song, &first.song));
            assert!(Arc::ptr_eq(&first.song, &repeated.song));
            assert_eq!(expected.chart_hash, first.chart_hash);
            assert_eq!(first.chart_hash, repeated.chart_hash);
        }
        for a in 0..first.len() {
            for b in a + 1..first.len() {
                assert!(!Arc::ptr_eq(&first[a].song, &first[b].song));
            }
        }
    }
}

#[test]
fn catalog_start_retains_rating_and_falls_back_to_the_confirmed_cycle() {
    let packs = [pack("Pack", vec![song("Songs/Pack/Alpha/a.ssc", &["a"])])];
    let path = PathBuf::from("Courses/retained.crs");
    let mut course = course(vec![entry(CourseSong::RandomAny)]);
    course.repeat = true;
    course.name = "Original course title".into();
    let mut catalog = PreparedCourseCatalog::new(
        vec![(path.clone(), course)],
        &packs,
        &[],
        &[],
        "dance-single",
    );
    let choice = CourseChoice {
        path: &path,
        difficulty: Difficulty::Hard,
    };
    let expected = catalog
        .selected_cycle(choice)
        .expect("rating has a resolved stage");
    let (resolved_song, resolved_chart) =
        catalog.rating(choice).unwrap().1.charts().next().unwrap();
    assert!(std::ptr::eq(
        resolved_song,
        expected.stages[0].song.as_ref()
    ));
    assert_eq!(resolved_chart.short_hash, expected.stages[0].chart_hash);

    catalog.resolver = Arc::new(CourseResolver::new(&[], &[], &[], "dance-single"));
    assert!(catalog.reroll_cycle(choice).is_none());
    let started = catalog.start(choice).unwrap();
    assert_eq!(started.song.title, "Original course title");
    assert_eq!(started.plan.course_difficulty_name, "Hard");
    assert_eq!(
        started.plan.stages[0].chart_hash,
        expected.stages[0].chart_hash
    );
    let fallback = started.endless_source.unwrap().next_cycle();
    assert_eq!(fallback.path, expected.path);
    assert_eq!(fallback.stages[0].chart_hash, expected.stages[0].chart_hash);
    assert!(Arc::ptr_eq(
        &fallback.stages[0].song,
        &expected.stages[0].song
    ));
}
