use super::*;
use deadsync_chart::{ArrowStats, ChartData, MatrixRatingInput};

mod original {
    use super::*;
    include!("course_originals.rs");
}

fn song(measures: usize) -> Arc<SongData> {
    let charts = ["Easy", "Hard", "Edit", "Edit", "Hard"]
        .into_iter()
        .enumerate()
        .map(|(i, difficulty)| ChartData {
            chart_type: if i == 4 {
                "pump-single"
            } else {
                "dance-single"
            }
            .into(),
            difficulty: difficulty.into(),
            description: format!("Fixture chart {i}"),
            chart_name: format!("Course chart {i}"),
            meter: 6 + i as u32,
            step_artist: "Course test author".into(),
            music_path: Some("test/music.ogg".into()),
            short_hash: format!("course-update-chart-{i}"),
            stats: ArrowStats {
                total_steps: 100 + i as u32,
                ..Default::default()
            },
            tech_counts: Default::default(),
            mines_nonfake: 3,
            stamina_counts: Default::default(),
            total_streams: 12,
            matrix_rating: 8.5,
            matrix_profile: vec![
                MatrixRatingInput {
                    effective_bpm: 150.0,
                    measures: 4
                };
                measures / 4
            ]
            .into_boxed_slice(),
            max_nps: 10.0,
            sn_detailed_breakdown: "4-4-8-4-16".into(),
            sn_partial_breakdown: "4-4-8-4-16".into(),
            sn_simple_breakdown: "36".into(),
            detailed_breakdown: "4-4-8-4-16".into(),
            partial_breakdown: "4-4-8-4-16".into(),
            simple_breakdown: "36".into(),
            total_measures: measures,
            measure_nps_vec: (0..measures).map(|n| (n % 12) as f64).collect(),
            measure_seconds_vec: (0..measures).map(|n| n as f32 * 1.6).collect(),
            first_second: 0.0,
            has_note_data: true,
            has_chart_attacks: false,
            possible_grade_points: 1000 + i as i32,
            holds_total: 6,
            rolls_total: 2,
            mines_total: 3,
            display_bpm: None,
            min_bpm: 150.0,
            max_bpm: 150.0,
        })
        .collect();
    Arc::new(SongData {
        simfile_path: "test/course-updates/song.ssc".into(),
        title: "Course updates".into(),
        subtitle: String::new(),
        translit_title: String::new(),
        translit_subtitle: String::new(),
        artist: "Fixture".into(),
        translit_artist: String::new(),
        genre: String::new(),
        banner_path: Some("test/banner.png".into()),
        background_path: None,
        background_changes: Vec::new(),
        background_layer2_changes: Vec::new(),
        foreground_changes: Vec::new(),
        background_lua_changes: Vec::new(),
        foreground_lua_changes: Vec::new(),
        has_lua: false,
        cdtitle_path: None,
        music_path: Some("test/music.ogg".into()),
        display_bpm: "150".into(),
        offset: 0.0,
        sample_start: Some(10.0),
        sample_length: Some(12.0),
        min_bpm: 150.0,
        max_bpm: 150.0,
        normalized_bpms: "0=150".into(),
        song_timing: None,
        music_length_seconds: 202.5,
        first_second: 0.0,
        total_length_seconds: 200,
        precise_last_second_seconds: 200.25,
        last_second_hint: 0.0,
        charts,
    })
}

fn selection(song: &Arc<SongData>, count: usize) -> SelectedCoursePlan {
    SelectedCoursePlan {
        path: "test/course-updates/course.crs".into(),
        name: "Fixture course".into(),
        banner_path: Some("test/course-banner.png".into()),
        score_hash: "course-hash".into(),
        song_stub: Arc::clone(song),
        course_difficulty_name: "Hard".into(),
        course_meter: Some(12),
        course_stepchart_label: "Hard 12".into(),
        course_type: CourseTypeView::Endless,
        lives: 4,
        stages: (0..count)
            .map(|i| CourseStagePlan {
                song: Arc::clone(song),
                chart_hash: song.charts[1].short_hash.clone(),
                modifiers: format!("1.5x, no mines, reverse, mini, stage-{i}"),
                gain_seconds: 10.0 + i as f32,
                gain_lives: (i % 4) as i32 - 1,
            })
            .collect(),
    }
}

fn assert_run_eq(a: &CourseRunState, b: &CourseRunState) {
    assert_eq!(a.path, b.path);
    assert_eq!(a.name, b.name);
    assert_eq!(a.banner_path, b.banner_path);
    assert_eq!(a.score_hash, b.score_hash);
    assert_eq!(a.course_difficulty_name, b.course_difficulty_name);
    assert_eq!(a.course_meter, b.course_meter);
    assert_eq!(a.course_stepchart_label, b.course_stepchart_label);
    assert_eq!(a.course_type, b.course_type);
    assert_eq!(a.lives, b.lives);
    assert!(Arc::ptr_eq(&a.song_stub, &b.song_stub));
    assert_eq!(a.next_stage_index, b.next_stage_index);
    assert_eq!(a.stage_summaries.len(), b.stage_summaries.len());
    assert_eq!(
        format!("{:?}", a.course_display_totals),
        format!("{:?}", b.course_display_totals)
    );
    assert_eq!(a.stages.len(), b.stages.len());
    for (a, b) in a.stages.iter().zip(&b.stages) {
        assert!(Arc::ptr_eq(&a.song, &b.song));
        assert_eq!(a.steps_index, b.steps_index);
        assert_eq!(a.preferred_difficulty_index, b.preferred_difficulty_index);
        assert_eq!(a.modifiers, b.modifiers);
        assert_eq!(a.gain_seconds.to_bits(), b.gain_seconds.to_bits());
        assert_eq!(a.gain_lives, b.gain_lives);
    }
}

#[test]
fn startup_preserves_selection_metadata_filtering_and_totals() {
    let mut song = song(12);
    let chart = &mut Arc::get_mut(&mut song).unwrap().charts[1];
    chart.possible_grade_points = i32::MAX;
    chart.stats.total_steps = u32::MAX;
    for count in [0, 1, 4, 24] {
        for chart_type in ["dance-single", "DANCE-SINGLE", "pump-single", "missing"] {
            let mut plan = selection(&song, count);
            for (i, stage) in plan.stages.iter_mut().enumerate() {
                stage.chart_hash = if i % 5 == 4 {
                    "missing".into()
                } else {
                    song.charts[i % 5].short_hash.clone()
                };
                if i % 3 == 0 {
                    stage.gain_seconds = f32::NAN;
                }
            }
            let before = original::build_course_run_from_selection(plan.clone(), chart_type);
            let after = build_course_run_from_selection(plan, chart_type);
            assert_eq!(before.is_some(), after.is_some());
            if let (Some(a), Some(b)) = (before, after) {
                assert_run_eq(&a, &b);
            }
        }
    }
}

#[test]
fn startup_moves_modifier_buffers_from_owned_stages() {
    let song = song(12);
    let plan = selection(&song, 4);
    let pointers: Vec<_> = plan.stages.iter().map(|s| s.modifiers.as_ptr()).collect();
    let (run, churn) =
        crate::perf::measure(|| build_course_run_from_selection(plan, "dance-single").unwrap());
    assert_eq!(churn.allocs, 1);
    assert_eq!(churn.reallocs, 0);
    assert_eq!(
        pointers,
        run.stages
            .iter()
            .map(|s| s.modifiers.as_ptr())
            .collect::<Vec<_>>()
    );
}

#[test]
fn endless_append_preserves_rejections_order_and_saturated_totals() {
    let song = song(12);
    for kind in [
        CourseTypeView::Endless,
        CourseTypeView::Nonstop,
        CourseTypeView::Oni,
        CourseTypeView::Survival,
    ] {
        for (count, invalid, wrong_path) in [
            (0, false, false),
            (8, false, false),
            (8, true, false),
            (8, false, true),
        ] {
            let mut a =
                original::build_course_run_from_selection(selection(&song, 4), "dance-single")
                    .unwrap();
            a.course_type = kind;
            a.next_stage_index = 3;
            a.course_display_totals[0] = CourseDisplayTotals {
                possible_grade_points: i32::MAX - 1,
                total_steps: u32::MAX - 1,
                holds_total: u32::MAX - 1,
                rolls_total: u32::MAX - 1,
                mines_total: u32::MAX - 1,
            };
            let mut b = a.clone();
            let mut plan = selection(&song, count);
            if wrong_path {
                plan.path = "other/course.crs".into();
            }
            for (i, stage) in plan.stages.iter_mut().enumerate() {
                if invalid || i % 3 == 0 {
                    stage.chart_hash = "missing".into();
                }
            }
            let old = original::append_endless_cycle(&mut a, plan.clone(), "dance-single");
            let new = append_endless_cycle(&mut b, plan, "dance-single");
            assert_eq!(old, new);
            assert_run_eq(&a, &b);
        }
    }
}

#[test]
fn reserved_endless_append_needs_no_transient_allocations() {
    let song = song(12);
    let mut run = build_course_run_from_selection(selection(&song, 4), "dance-single").unwrap();
    let plan = selection(&song, 8);
    run.stages.reserve(plan.stages.len());
    let pointers: Vec<_> = plan.stages.iter().map(|s| s.modifiers.as_ptr()).collect();
    let (appended, churn) =
        crate::perf::measure(|| append_endless_cycle(&mut run, plan, "dance-single"));
    assert!(appended);
    assert_eq!(churn.allocs, 0);
    assert_eq!(churn.reallocs, 0);
    assert_eq!(
        pointers,
        run.stages[4..]
            .iter()
            .map(|s| s.modifiers.as_ptr())
            .collect::<Vec<_>>()
    );
}

#[test]
fn graph_stages_preserve_both_players_with_missing_and_edit_charts() {
    let song = song(12);
    let mut run = build_course_run_from_selection(selection(&song, 8), "dance-single").unwrap();
    for (stage, indices) in run.stages.iter_mut().zip([
        [1, 1],
        [1, 3],
        [usize::MAX, 3],
        [3, usize::MAX],
        [usize::MAX; 2],
        [5, 5],
        [5, 6],
        [6, 5],
    ]) {
        stage.steps_index = indices;
    }
    for chart_type in ["dance-single", "DANCE-SINGLE", "pump-single", "missing"] {
        let expected = original::build_course_graph_stages(&run, chart_type);
        let actual = build_course_graph_stages(&run, chart_type);
        assert_eq!(format!("{expected:?}"), format!("{actual:?}"));
    }
    let mut graphs = build_course_graph_stages(&run, "dance-single");
    assert!(Arc::ptr_eq(&graphs[0][0].chart, &graphs[1][0].chart));
    assert!(!Arc::ptr_eq(&graphs[0][1].chart, &graphs[1][1].chart));
    Arc::make_mut(&mut graphs[0][0].chart).description = "modified only on P1".into();
    assert_ne!(
        graphs[0][0].chart.description,
        graphs[1][0].chart.description
    );
    assert_eq!(graphs[1][0].chart.description, song.charts[0].description);
    run.stages.clear();
    assert!(
        build_course_graph_stages(&run, "dance-single")
            .iter()
            .all(Vec::is_empty)
    );
}

// Intermediate reference isolates direct append from the separate owned-plan change.
fn buffered_owned_append(
    course: &mut CourseRunState,
    selection: SelectedCoursePlan,
    chart_type: &str,
) -> bool {
    if course.course_type != CourseTypeView::Endless || selection.path != course.path {
        return false;
    }
    let stages: Vec<_> = selection
        .stages
        .into_iter()
        .filter_map(|stage| course_stage_runtime_from_plan(stage, chart_type))
        .collect();
    if stages.is_empty() {
        return false;
    }
    for stage in &stages {
        add_course_stage_totals(&mut course.course_display_totals, stage, chart_type);
    }
    course.stages.extend(stages);
    true
}

#[test]
#[ignore = "paired release benchmark; run explicitly"]
fn benchmark_course_updates() {
    use crate::course_perf::compare;
    let song = song(256);
    for count in [4, 32] {
        let plan = selection(&song, count);
        compare(
            &format!("startup/{count}"),
            || plan.clone(),
            |plan| original::build_course_run_from_selection(plan, "dance-single"),
            |plan| build_course_run_from_selection(plan, "dance-single"),
        );
    }
    let mut invalid = selection(&song, 16);
    for stage in &mut invalid.stages {
        stage.chart_hash = "missing".into();
    }
    compare(
        "startup/all_invalid",
        || invalid.clone(),
        |plan| original::build_course_run_from_selection(plan, "dance-single"),
        |plan| build_course_run_from_selection(plan, "dance-single"),
    );
    for count in [4, 32] {
        let plan = selection(&song, count);
        let run = build_course_run_from_selection(plan.clone(), "dance-single").unwrap();
        compare(
            &format!("endless/{count}"),
            || (run.clone(), plan.clone()),
            |(mut run, plan)| {
                let ok = original::append_endless_cycle(&mut run, plan, "dance-single");
                (ok, run)
            },
            |(mut run, plan)| {
                let ok = append_endless_cycle(&mut run, plan, "dance-single");
                (ok, run)
            },
        );
        compare(
            &format!("endless/{count}_owned_control"),
            || (run.clone(), plan.clone()),
            |(mut run, plan)| {
                let ok = buffered_owned_append(&mut run, plan, "dance-single");
                (ok, run)
            },
            |(mut run, plan)| {
                let ok = append_endless_cycle(&mut run, plan, "dance-single");
                (ok, run)
            },
        );
    }
    for (label, count, mode) in [
        ("graphs/same_4", 4, 0),
        ("graphs/same_32", 32, 0),
        ("graphs/different_32", 32, 1),
        ("graphs/mixed_32", 32, 2),
        ("graphs/missing_32", 32, 3),
    ] {
        let mut run =
            build_course_run_from_selection(selection(&song, count), "dance-single").unwrap();
        for (i, stage) in run.stages.iter_mut().enumerate() {
            if mode == 1 || mode == 2 && i % 2 == 0 {
                stage.steps_index = [1, 3];
            }
            if mode == 3 {
                stage.steps_index = [usize::MAX; 2];
            }
        }
        assert_eq!(
            format!(
                "{:?}",
                original::build_course_graph_stages(&run, "dance-single")
            ),
            format!("{:?}", build_course_graph_stages(&run, "dance-single"))
        );
        compare(
            label,
            || &run,
            |run| original::build_course_graph_stages(run, "dance-single"),
            |run| build_course_graph_stages(run, "dance-single"),
        );
    }
}
